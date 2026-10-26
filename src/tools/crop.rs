//! Crop tool: a frame over the canvas, free or with a fixed aspect ratio, that becomes the
//! new canvas. Nothing is cut from the objects, they only move.

use eframe::egui::{self, Color32, CursorIcon, Pos2, Stroke, Ui};
use kurbo::{Point, Rect, Vec2};

use crate::app::{App, Gesture};
use crate::theme::pal;
use crate::tools::select::HANDLES;

/// How close the pointer must be to grab an edge or corner (screen points).
const GRAB: f32 = 10.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Aspect {
    Free,
    Ratio(u32, u32),
    /// Width and height in pixels.
    Exact,
}

impl Aspect {
    pub const PRESETS: [Aspect; 10] = [
        Aspect::Free,
        Aspect::Ratio(1, 1),
        Aspect::Ratio(4, 3),
        Aspect::Ratio(3, 2),
        Aspect::Ratio(16, 9),
        Aspect::Ratio(3, 4),
        Aspect::Ratio(2, 3),
        Aspect::Ratio(9, 16),
        Aspect::Ratio(4, 5),
        Aspect::Exact,
    ];

    pub fn label(self) -> String {
        let t = crate::i18n::t();
        match self {
            Aspect::Free => t.free.to_string(),
            Aspect::Ratio(w, h) => format!("{w}:{h}"),
            Aspect::Exact => t.exact_px.to_string(),
        }
    }
}

#[derive(Clone, Copy)]
enum CropDrag {
    Move,
    /// Index in [`HANDLES`].
    Handle(usize),
    /// A new frame from the press point.
    New,
}

pub struct CropState {
    pub rect: Rect,
    pub aspect: Aspect,
    /// Size for [`Aspect::Exact`].
    pub exact: (u32, u32),
    drag: Option<(CropDrag, Point, Rect)>,
}

impl Default for CropState {
    fn default() -> Self {
        CropState {
            rect: Rect::ZERO,
            aspect: Aspect::Free,
            exact: (1080, 1080),
            drag: None,
        }
    }
}

impl CropState {
    /// Width / height the frame must keep.
    pub fn ratio(&self) -> Option<f64> {
        match self.aspect {
            Aspect::Free => None,
            Aspect::Ratio(w, h) => Some(w as f64 / h as f64),
            Aspect::Exact => Some(self.exact.0.max(1) as f64 / self.exact.1.max(1) as f64),
        }
    }
}

/// Largest rectangle of `ratio` centered in `area`.
fn fit_ratio(area: Rect, ratio: f64) -> Rect {
    let (w, h) = (area.width(), area.height());
    let (nw, nh) = if w / h > ratio {
        (h * ratio, h)
    } else {
        (w, w / ratio)
    };
    Rect::from_center_size(area.center(), (nw, nh))
}

/// Frame from a fixed `anchor` to the pointer `p`, keeping `ratio`.
fn from_anchor(anchor: Point, p: Point, ratio: Option<f64>) -> Rect {
    let d = p - anchor;
    let (mut w, mut h) = (d.x.abs(), d.y.abs());
    if let Some(r) = ratio {
        if w / h.max(1e-9) > r {
            w = h * r;
        } else {
            h = w / r;
        }
    }
    Rect::from_points(anchor, anchor + Vec2::new(w.copysign(d.x), h.copysign(d.y)))
}

fn handle_pos(r: Rect, i: usize) -> Point {
    let (fx, fy) = HANDLES[i];
    let c = r.center();
    Point::new(c.x + fx * r.width() / 2.0, c.y + fy * r.height() / 2.0)
}

/// Frame after dragging handle `i` of `orig` to `p`.
fn resize(orig: Rect, i: usize, p: Point, ratio: Option<f64>) -> Rect {
    let (fx, fy) = HANDLES[i];
    if fx != 0.0 && fy != 0.0 {
        return from_anchor(handle_pos(orig, (i + 4) % 8), p, ratio);
    }
    let mut r = orig;
    if fx != 0.0 {
        if fx < 0.0 {
            r.x0 = p.x.min(r.x1 - 1.0);
        } else {
            r.x1 = p.x.max(r.x0 + 1.0);
        }
        if let Some(ratio) = ratio {
            let h = r.width() / ratio;
            r.y0 = orig.center().y - h / 2.0;
            r.y1 = orig.center().y + h / 2.0;
        }
    } else {
        if fy < 0.0 {
            r.y0 = p.y.min(r.y1 - 1.0);
        } else {
            r.y1 = p.y.max(r.y0 + 1.0);
        }
        if let Some(ratio) = ratio {
            let w = r.height() * ratio;
            r.x0 = orig.center().x - w / 2.0;
            r.x1 = orig.center().x + w / 2.0;
        }
    }
    r
}

impl App {
    fn canvas_rect(&self) -> Rect {
        Rect::new(0.0, 0.0, self.doc().width as f64, self.doc().height as f64)
    }

    /// Puts the frame back over the whole canvas (the biggest that keeps the aspect).
    pub fn reset_crop(&mut self) {
        let full = self.canvas_rect();
        self.crop.rect = match self.crop.aspect {
            Aspect::Exact => {
                let (w, h) = self.crop.exact;
                Rect::from_center_size(full.center(), (w as f64, h as f64))
            }
            _ => match self.crop.ratio() {
                Some(r) => fit_ratio(full, r),
                None => full,
            },
        };
        self.crop.drag = None;
    }

    pub fn apply_crop(&mut self) {
        let r = self.crop.rect;
        if r.width() < 1.0 || r.height() < 1.0 {
            return;
        }
        self.settle();
        self.session.edit(|d| d.crop(r));
        self.view.fit_pending = true;
        self.reset_crop();
    }

    fn crop_grab(&self, pos: Pos2) -> Option<CropDrag> {
        let r = self.crop.rect;
        let exact = self.crop.aspect == Aspect::Exact;
        if !exact
            && let Some(i) =
                (0..8).find(|&i| self.view.to_screen(handle_pos(r, i)).distance(pos) <= GRAB)
        {
            return Some(CropDrag::Handle(i));
        }
        self.view
            .rect_to_screen(r)
            .contains(pos)
            .then_some(CropDrag::Move)
    }

    pub(crate) fn crop_cursor(&self, pos: Pos2) -> CursorIcon {
        match self.crop_grab(pos) {
            Some(CropDrag::Handle(i)) => match i {
                0 | 4 => CursorIcon::ResizeNwSe,
                2 | 6 => CursorIcon::ResizeNeSw,
                1 | 5 => CursorIcon::ResizeVertical,
                _ => CursorIcon::ResizeHorizontal,
            },
            Some(_) => CursorIcon::Move,
            None => CursorIcon::Crosshair,
        }
    }

    pub(crate) fn crop_tool(&mut self, ui: &Ui, resp: &egui::Response) {
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let pos = ui
                .input(|i| i.pointer.press_origin())
                .or(resp.interact_pointer_pos())
                .unwrap_or_default();
            let kind = self
                .crop_grab(pos)
                .unwrap_or(if self.crop.aspect == Aspect::Exact {
                    CropDrag::Move
                } else {
                    CropDrag::New
                });
            self.crop.drag = Some((kind, self.view.to_doc(pos), self.crop.rect));
            self.gesture = Gesture::Crop;
        }
        if resp.dragged_by(egui::PointerButton::Primary)
            && let (Some(pos), Some((kind, start, orig))) =
                (resp.interact_pointer_pos(), self.crop.drag)
        {
            let p = self.view.to_doc(pos);
            let ratio = self.crop.ratio();
            self.crop.rect = match kind {
                CropDrag::Move => {
                    let d = p - start;
                    orig + Vec2::new(d.x.round(), d.y.round())
                }
                CropDrag::Handle(i) => resize(orig, i, p, ratio),
                CropDrag::New => from_anchor(start, p, ratio),
            };
        }
        if resp.drag_stopped() {
            let r = self.crop.rect;
            self.crop.rect = Rect::new(r.x0.round(), r.y0.round(), r.x1.round(), r.y1.round());
            self.crop.drag = None;
            self.gesture = Gesture::None;
        }
        if resp.double_clicked() {
            self.apply_crop();
        }
    }

    pub(crate) fn draw_crop(&self, painter: &egui::Painter) {
        let p = pal();
        let r = self.view.rect_to_screen(self.crop.rect);
        let clip = painter.clip_rect();
        let shade = Color32::from_black_alpha(110);
        // Darken around the frame.
        for part in [
            egui::Rect::from_min_max(clip.min, egui::pos2(clip.max.x, r.top())),
            egui::Rect::from_min_max(egui::pos2(clip.min.x, r.bottom()), clip.max),
            egui::Rect::from_min_max(
                egui::pos2(clip.min.x, r.top()),
                egui::pos2(r.left(), r.bottom()),
            ),
            egui::Rect::from_min_max(
                egui::pos2(r.right(), r.top()),
                egui::pos2(clip.max.x, r.bottom()),
            ),
        ] {
            if part.is_positive() {
                painter.rect_filled(part, 0.0, shade);
            }
        }
        // Rule of thirds.
        let thin = Stroke::new(1.0, Color32::from_white_alpha(90));
        for k in [1.0 / 3.0, 2.0 / 3.0] {
            let x = r.left() + r.width() * k;
            let y = r.top() + r.height() * k;
            painter.line_segment([egui::pos2(x, r.top()), egui::pos2(x, r.bottom())], thin);
            painter.line_segment([egui::pos2(r.left(), y), egui::pos2(r.right(), y)], thin);
        }
        painter.rect_stroke(
            r,
            0.0,
            Stroke::new(1.5, Color32::WHITE),
            egui::StrokeKind::Middle,
        );
        painter.rect_stroke(
            r.expand(1.5),
            0.0,
            Stroke::new(1.0, p.accent),
            egui::StrokeKind::Middle,
        );
        if self.crop.aspect != Aspect::Exact {
            for i in 0..8 {
                let c = self.view.to_screen(handle_pos(self.crop.rect, i));
                let h = egui::Rect::from_center_size(c, egui::Vec2::splat(9.0));
                painter.rect_filled(h, 1.5, Color32::WHITE);
                painter.rect_stroke(h, 1.5, Stroke::new(1.0, p.accent), egui::StrokeKind::Middle);
            }
        }
        let size = format!(
            "{} × {}",
            self.crop.rect.width().round(),
            self.crop.rect.height().round()
        );
        let galley = painter.layout_no_wrap(size, egui::FontId::proportional(12.0), Color32::WHITE);
        let at = egui::pos2(r.center().x - galley.size().x / 2.0, r.bottom() + 8.0);
        painter.rect_filled(
            egui::Rect::from_min_size(at, galley.size()).expand(4.0),
            4.0,
            Color32::from_black_alpha(160),
        );
        painter.galley(at, galley, Color32::WHITE);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ratio_is_kept() {
        let r = from_anchor(
            Point::new(0.0, 0.0),
            Point::new(300.0, 100.0),
            Some(16.0 / 9.0),
        );
        assert!((r.width() / r.height() - 16.0 / 9.0).abs() < 1e-9);
        let r = resize(
            Rect::new(0.0, 0.0, 160.0, 90.0),
            3,
            Point::new(320.0, 0.0),
            Some(16.0 / 9.0),
        );
        assert!((r.width() - 320.0).abs() < 1e-9 && (r.height() - 180.0).abs() < 1e-9);
        let f = fit_ratio(Rect::new(0.0, 0.0, 1280.0, 720.0), 1.0);
        assert_eq!((f.width(), f.height()), (720.0, 720.0));
    }
}

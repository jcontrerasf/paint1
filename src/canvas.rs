//! The canvas on screen: view (pan and zoom), the rendered document as a texture, overlays,
//! and handing pointer input to the active tool.

use eframe::egui::{
    self, Color32, CursorIcon, Pos2, Rect, Sense, Stroke, TextureOptions, Ui, Vec2,
};
use kurbo::Point;

use crate::app::{App, Gesture};
use crate::render::Job;
use crate::theme::pal;
use crate::tools::Tool;

const MIN_SCALE: f32 = 0.02;
const MAX_SCALE: f32 = 64.0;
/// Space left around the document when fitting it.
const FIT_MARGIN: f32 = 24.0;
/// Side of a checkerboard square, in screen points.
const CHECKER: f32 = 8.0;

pub struct View {
    /// Screen points per canvas unit.
    pub scale: f32,
    /// Offset of the document's center from the center of the widget.
    pub pan: Vec2,
    pub fit_pending: bool,
    /// Size of the widget when the view was fitted; while the user has not moved the view,
    /// the document is fitted again if the window changes size.
    fitted_for: Option<Vec2>,
    /// Widget rectangle (set every frame).
    pub rect: Rect,
    /// Screen position of the canvas origin (set every frame).
    pub origin: Pos2,
    doc_size: Vec2,
}

impl Default for View {
    fn default() -> Self {
        View {
            scale: 1.0,
            pan: Vec2::ZERO,
            fit_pending: true,
            fitted_for: None,
            rect: Rect::NOTHING,
            origin: Pos2::ZERO,
            doc_size: Vec2::ZERO,
        }
    }
}

impl View {
    pub fn to_screen(&self, p: Point) -> Pos2 {
        self.origin + Vec2::new(p.x as f32, p.y as f32) * self.scale
    }

    pub fn to_doc(&self, p: Pos2) -> Point {
        let v = (p - self.origin) / self.scale;
        Point::new(v.x as f64, v.y as f64)
    }

    pub fn rect_to_screen(&self, r: kurbo::Rect) -> Rect {
        Rect::from_two_pos(
            self.to_screen(Point::new(r.x0, r.y0)),
            self.to_screen(Point::new(r.x1, r.y1)),
        )
    }

    fn update_origin(&mut self) {
        self.origin = self.rect.center() + self.pan - self.doc_size * self.scale / 2.0;
    }

    /// Zooms keeping the canvas point under `anchor` (screen) in place.
    pub fn zoom_at(&mut self, factor: f32, anchor: Pos2) {
        self.fitted_for = None;
        let doc = self.to_doc(anchor);
        self.scale = (self.scale * factor).clamp(MIN_SCALE, MAX_SCALE);
        let d = Vec2::new(doc.x as f32, doc.y as f32);
        self.pan = anchor - self.rect.center() + self.doc_size * self.scale / 2.0 - d * self.scale;
        self.update_origin();
    }

    pub fn zoom_by(&mut self, factor: f32) {
        self.zoom_at(factor, self.rect.center());
    }

    fn fit(&mut self) {
        let avail = self.rect.size() - Vec2::splat(2.0 * FIT_MARGIN);
        let s = (avail.x / self.doc_size.x).min(avail.y / self.doc_size.y);
        self.scale = s.clamp(MIN_SCALE, 1.0);
        self.pan = Vec2::ZERO;
        self.fitted_for = Some(self.rect.size());
    }

    /// Canvas units per screen point (for tolerances).
    pub fn px(&self) -> f64 {
        1.0 / self.scale as f64
    }
}

/// The document rendered for the current view.
pub struct CanvasTexture {
    handle: egui::TextureHandle,
    /// Revision, region and scale it was rendered with.
    key: (u64, [i64; 4], u32),
    /// Part of the canvas it shows.
    region: kurbo::Rect,
}

impl App {
    pub(crate) fn canvas(&mut self, ui: &mut Ui) {
        let (resp, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
        let doc_size = Vec2::new(self.doc().width as f32, self.doc().height as f32);
        self.view.rect = resp.rect;
        self.view.doc_size = doc_size;
        if self.view.fitted_for.is_some_and(|s| s != resp.rect.size()) {
            self.view.fit_pending = true;
        }
        if self.view.fit_pending && resp.rect.width() > 2.0 * FIT_MARGIN {
            self.view.fit();
            self.view.fit_pending = false;
            // The status bar was already drawn with the old zoom.
            ui.ctx().request_repaint();
        }
        self.view.update_origin();
        self.pan_zoom(ui, &resp);

        let p = pal();
        let doc_rect = Rect::from_min_size(self.view.origin, doc_size * self.view.scale);
        painter.rect_filled(
            doc_rect.translate(Vec2::new(0.0, 3.0)).expand(1.0),
            2.0,
            Color32::from_black_alpha(40),
        );
        if self.doc().background.0[3] < 255 {
            self.draw_checker(ui, &painter, doc_rect.intersect(resp.rect));
        }
        self.draw_document(ui, &painter);
        painter.rect_stroke(
            doc_rect,
            0.0,
            Stroke::new(1.0, p.border),
            egui::StrokeKind::Outside,
        );

        self.pointer_doc = resp.hover_pos().map(|pos| self.view.to_doc(pos));
        if !matches!(self.gesture, Gesture::Pan) {
            match self.tool {
                Tool::Select => self.select_tool(ui, &resp),
                Tool::Crop => self.crop_tool(ui, &resp),
                Tool::Text => self.text_tool(ui, &resp),
                Tool::Polygon => self.polygon_tool(ui, &resp),
                Tool::Eyedropper => self.eyedropper_tool(ui, &resp),
                _ => self.draw_tool(ui, &resp),
            }
        }
        if resp.hovered() {
            let cursor = match self.tool {
                _ if matches!(self.gesture, Gesture::Pan) => CursorIcon::Grabbing,
                _ if ui.input(|i| i.key_down(egui::Key::Space)) && self.text_edit.is_none() => {
                    CursorIcon::Grab
                }
                Tool::Select | Tool::Crop => self.hover_cursor(&resp),
                Tool::Text => CursorIcon::Text,
                _ => CursorIcon::Crosshair,
            };
            ui.ctx().set_cursor_icon(cursor);
        }

        let painter = painter.with_clip_rect(resp.rect);
        self.draw_selection(&painter);
        self.draw_marquee(&painter);
        self.draw_polygon_hint(&painter);
        self.draw_text_caret(&painter);
        if self.tool == Tool::Crop {
            self.draw_crop(&painter);
        }
    }

    fn pan_zoom(&mut self, ui: &Ui, resp: &egui::Response) {
        let space = ui.input(|i| i.key_down(egui::Key::Space)) && self.text_edit.is_none();
        let pan_button = resp.dragged_by(egui::PointerButton::Middle)
            || resp.dragged_by(egui::PointerButton::Secondary)
            || (space && resp.dragged_by(egui::PointerButton::Primary));
        if pan_button {
            if !matches!(self.gesture, Gesture::Pan | Gesture::None) {
                // A tool gesture was in progress: drop it.
                self.session.cancel_group();
            }
            self.gesture = Gesture::Pan;
            self.view.pan += resp.drag_delta();
            self.view.fitted_for = None;
            self.view.update_origin();
        } else if matches!(self.gesture, Gesture::Pan) && !ui.input(|i| i.pointer.any_down()) {
            self.gesture = Gesture::None;
        }
        if let Some(pointer) = resp.hover_pos() {
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            let factor = zoom * (scroll / 400.0).exp();
            if (factor - 1.0).abs() > 1e-4 {
                self.view.zoom_at(factor, pointer);
            }
        }
    }

    fn draw_checker(&mut self, ui: &Ui, painter: &egui::Painter, rect: Rect) {
        if !rect.is_positive() {
            return;
        }
        let (a, b) = pal().checker;
        // Rebuilt when the theme changes the colors.
        if self.checker.as_ref().is_none_or(|(_, c)| *c != a) {
            let img = egui::ColorImage::new([2, 2], vec![a, b, b, a]);
            let options = TextureOptions {
                magnification: egui::TextureFilter::Nearest,
                minification: egui::TextureFilter::Nearest,
                wrap_mode: egui::TextureWrapMode::Repeat,
                mipmap_mode: None,
            };
            self.checker = Some((ui.ctx().load_texture("checker", img, options), a));
        }
        let Some((tex, _)) = &self.checker else {
            return;
        };
        let uv = Rect::from_min_max(
            Pos2::ZERO,
            Pos2::new(
                rect.width() / (2.0 * CHECKER),
                rect.height() / (2.0 * CHECKER),
            ),
        );
        painter.image(tex.id(), rect, uv, Color32::WHITE);
    }

    /// Renders the visible part of the document (re-rendering only when something changed)
    /// and draws it.
    fn draw_document(&mut self, ui: &Ui, painter: &egui::Painter) {
        let ppp = ui.ctx().pixels_per_point();
        let doc = self.session.doc();
        let full = kurbo::Rect::new(0.0, 0.0, doc.width as f64, doc.height as f64);
        let a = self.view.to_doc(self.view.rect.min);
        let b = self.view.to_doc(self.view.rect.max);
        let visible =
            kurbo::Rect::new(a.x.floor(), a.y.floor(), b.x.ceil(), b.y.ceil()).intersect(full);
        if visible.width() <= 0.0 || visible.height() <= 0.0 {
            return;
        }
        // Up to one texel per screen pixel; zoomed in, one per canvas pixel (shown sharp).
        let scale = ((self.view.scale * ppp) as f64).min(1.0);
        let key = (
            self.session.revision(),
            [
                visible.x0 as i64,
                visible.y0 as i64,
                visible.x1 as i64,
                visible.y1 as i64,
            ],
            (scale as f32).to_bits(),
        );
        if self.texture.as_ref().is_none_or(|t| t.key != key) {
            let job = Job {
                region: visible,
                scale,
                only: None,
                background: true,
            };
            if let Some(pm) = self.renderer.render(doc, &job) {
                let img = egui::ColorImage::from_rgba_premultiplied(
                    [pm.width() as usize, pm.height() as usize],
                    pm.data(),
                );
                let options = TextureOptions {
                    magnification: egui::TextureFilter::Nearest,
                    minification: egui::TextureFilter::Linear,
                    ..Default::default()
                };
                match &mut self.texture {
                    Some(t) => {
                        t.handle.set(img, options);
                        t.key = key;
                        t.region = visible;
                    }
                    None => {
                        self.texture = Some(CanvasTexture {
                            handle: ui.ctx().load_texture("document", img, options),
                            key,
                            region: visible,
                        });
                    }
                }
                self.renderer.prune(doc);
            }
        }
        if let Some(t) = &self.texture {
            let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
            painter.image(
                t.handle.id(),
                self.view.rect_to_screen(t.region),
                uv,
                Color32::WHITE,
            );
        }
    }

    /// Picks the color under the pointer.
    fn eyedropper_tool(&mut self, ui: &Ui, resp: &egui::Response) {
        if !resp.clicked() {
            return;
        }
        let Some(pos) = resp.interact_pointer_pos() else {
            return;
        };
        let p = self.view.to_doc(pos);
        if let Some(c) = self.renderer.sample(self.session.doc(), p) {
            if ui.input(|i| i.modifiers.shift) {
                self.style.fill = c;
                self.style.fill_on = true;
            } else {
                self.style.stroke = c;
            }
            let [r, g, b, _] = c.0;
            self.info(format!(
                "{} #{r:02x}{g:02x}{b:02x}",
                crate::i18n::t().sampled
            ));
        }
    }
}

//! Select tool: pick, area selection, move, scale, rotate and drag line ends.

use eframe::egui::{self, CursorIcon, Pos2, Stroke, Ui};
use kurbo::{Affine, Point, Rect, Vec2};

use crate::app::{App, Gesture, TransformKind};
use crate::document::Kind;
use crate::theme::pal;

/// Directions of the 8 scale handles: corners and edge middles, clockwise from top-left.
pub const HANDLES: [(f64, f64); 8] = [
    (-1.0, -1.0),
    (0.0, -1.0),
    (1.0, -1.0),
    (1.0, 0.0),
    (1.0, 1.0),
    (0.0, 1.0),
    (-1.0, 1.0),
    (-1.0, 0.0),
];
/// Half side of a handle, in screen points.
const HANDLE: f32 = 4.5;
/// How close the pointer must be to grab a handle.
const GRAB: f32 = 9.0;
/// Distance of the rotation handle above the frame.
const ROTATE_GAP: f32 = 26.0;
/// Pick tolerance in screen points.
pub const PICK: f64 = 4.0;
/// Snap of the rotation with Shift.
const ROTATE_SNAP: f64 = 15.0;

fn handle_pos(b: Rect, i: usize) -> Point {
    let (fx, fy) = HANDLES[i];
    let c = b.center();
    Point::new(c.x + fx * b.width() / 2.0, c.y + fy * b.height() / 2.0)
}

impl App {
    /// Bounds of the selection on the canvas.
    pub fn selection_bounds(&self) -> Option<Rect> {
        self.doc().bounds_of(&self.session.selection)
    }

    fn rotate_handle(&self, b: Rect) -> Pos2 {
        let top = self.view.to_screen(Point::new(b.center().x, b.y0));
        top - egui::vec2(0.0, ROTATE_GAP)
    }

    /// The single selected line or arrow, with its ends on screen.
    fn selected_ends(&self) -> Option<(u64, Pos2, Pos2)> {
        let [id] = self.session.selection.as_slice() else {
            return None;
        };
        let o = self.doc().get(*id)?;
        match o.kind {
            Kind::Line { a, b } | Kind::Arrow { a, b, .. } => Some((
                *id,
                self.view.to_screen(o.transform * a),
                self.view.to_screen(o.transform * b),
            )),
            _ => None,
        }
    }

    /// What is under the pointer: a handle of the selection?
    fn grab_at(&self, pos: Pos2) -> Option<TransformKind> {
        let b = self.selection_bounds()?;
        if self.selected_ends().is_some() {
            return None;
        }
        if self.rotate_handle(b).distance(pos) <= GRAB {
            return Some(TransformKind::Rotate);
        }
        (0..8)
            .find(|&i| self.view.to_screen(handle_pos(b, i)).distance(pos) <= GRAB)
            .map(TransformKind::Scale)
    }

    /// Is the screen point on a handle of the selection?
    pub(crate) fn grab_handle(&self, pos: Pos2) -> bool {
        self.grab_at(pos).is_some()
    }

    pub(crate) fn hover_cursor(&self, resp: &egui::Response) -> CursorIcon {
        let Some(pos) = resp.hover_pos() else {
            return CursorIcon::Default;
        };
        if self.tool == crate::tools::Tool::Crop {
            return self.crop_cursor(pos);
        }
        if let Some((_, a, b)) = self.selected_ends()
            && (a.distance(pos) <= GRAB || b.distance(pos) <= GRAB)
        {
            return CursorIcon::Crosshair;
        }
        match self.grab_at(pos) {
            Some(TransformKind::Rotate) => CursorIcon::Alias,
            Some(TransformKind::Scale(i)) => match i {
                0 | 4 => CursorIcon::ResizeNwSe,
                2 | 6 => CursorIcon::ResizeNeSw,
                1 | 5 => CursorIcon::ResizeVertical,
                _ => CursorIcon::ResizeHorizontal,
            },
            _ => {
                let p = self.view.to_doc(pos);
                match self.doc().pick(p, PICK * self.view.px()) {
                    Some(_) => CursorIcon::Move,
                    None => CursorIcon::Default,
                }
            }
        }
    }

    pub(crate) fn select_tool(&mut self, ui: &Ui, resp: &egui::Response) {
        let shift = ui.input(|i| i.modifiers.shift);
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let start_screen = ui
                .input(|i| i.pointer.press_origin())
                .or(resp.interact_pointer_pos())
                .unwrap_or_default();
            self.begin_select_drag(start_screen, shift);
        }
        if resp.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = resp.interact_pointer_pos()
        {
            self.update_select_drag(pos, shift);
        }
        if resp.drag_stopped() {
            self.end_select_drag();
        }
        if resp.clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let p = self.view.to_doc(pos);
            match self.doc().pick(p, PICK * self.view.px()) {
                Some(id) if shift => {
                    if self.session.is_selected(id) {
                        self.session.selection.retain(|s| *s != id);
                    } else {
                        self.session.selection.push(id);
                    }
                }
                Some(id) => self.session.select_only(id),
                None if !shift => self.session.selection.clear(),
                None => {}
            }
        }
        if resp.double_clicked()
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let p = self.view.to_doc(pos);
            if let Some(id) = self.doc().pick(p, PICK * self.view.px())
                && matches!(self.doc().get(id).map(|o| &o.kind), Some(Kind::Text { .. }))
            {
                self.tool = crate::tools::Tool::Text;
                self.start_text_edit(id, false);
            }
        }
    }

    pub(crate) fn begin_select_drag(&mut self, start_screen: Pos2, shift: bool) {
        let start = self.view.to_doc(start_screen);
        if let Some((id, a, b)) = self.selected_ends()
            && (a.distance(start_screen) <= GRAB || b.distance(start_screen) <= GRAB)
        {
            let is_b = b.distance(start_screen) < a.distance(start_screen);
            self.gesture = Gesture::Endpoint { id, b: is_b };
            return;
        }
        let kind = match self.grab_at(start_screen) {
            Some(k) => k,
            None => match self.doc().pick(start, PICK * self.view.px()) {
                Some(id) => {
                    if !self.session.is_selected(id) {
                        if shift {
                            self.session.selection.push(id);
                        } else {
                            self.session.select_only(id);
                        }
                    }
                    TransformKind::Move
                }
                None => {
                    self.gesture = Gesture::Marquee {
                        start,
                        end: start,
                        additive: shift,
                    };
                    return;
                }
            },
        };
        let Some(bounds) = self.selection_bounds() else {
            return;
        };
        let orig = self
            .doc()
            .objects
            .iter()
            .filter(|o| self.session.is_selected(o.id))
            .cloned()
            .collect();
        self.gesture = Gesture::Transform {
            kind,
            start,
            bounds,
            orig,
        };
    }

    pub(crate) fn update_select_drag(&mut self, pos: Pos2, shift: bool) {
        let p = self.view.to_doc(pos);
        match &self.gesture {
            Gesture::Transform {
                kind,
                start,
                bounds,
                orig,
            } => {
                let m = transform_for(*kind, *start, *bounds, p, shift);
                let orig = orig.clone();
                self.session.edit_grouped("transform", |d| {
                    for o in &orig {
                        if let Some(target) = d.get_mut(o.id) {
                            *target = o.clone();
                            target.apply(m);
                        }
                    }
                });
            }
            Gesture::Endpoint { id, b } => {
                let (id, is_b) = (*id, *b);
                self.session.edit_grouped("endpoint", |d| {
                    if let Some(o) = d.get_mut(id) {
                        let local = o.transform.inverse() * p;
                        if let Kind::Line { a, b } | Kind::Arrow { a, b, .. } = &mut o.kind {
                            let other = if is_b { *a } else { *b };
                            let local = if shift { snap_45(other, local) } else { local };
                            if is_b {
                                *b = local;
                            } else {
                                *a = local;
                            }
                        }
                    }
                });
            }
            Gesture::Marquee {
                start, additive, ..
            } => {
                self.gesture = Gesture::Marquee {
                    start: *start,
                    end: p,
                    additive: *additive,
                };
            }
            _ => {}
        }
    }

    pub(crate) fn end_select_drag(&mut self) {
        if let Gesture::Marquee {
            start,
            end,
            additive,
        } = self.gesture
        {
            let area = Rect::from_points(start, end);
            let hits: Vec<u64> = self
                .doc()
                .objects
                .iter()
                .filter(|o| o.visible && !o.locked && o.bounds().intersect(area).area() > 0.0)
                .map(|o| o.id)
                .collect();
            if !additive {
                self.session.selection.clear();
            }
            for id in hits {
                if !self.session.is_selected(id) {
                    self.session.selection.push(id);
                }
            }
        }
        self.gesture = Gesture::None;
        self.session.end_group();
    }

    pub(crate) fn draw_selection(&self, painter: &egui::Painter) {
        let p = pal();
        let stroke = Stroke::new(1.0, p.accent);
        // Each selected object gets a thin frame, the whole selection the handles.
        for o in self
            .doc()
            .objects
            .iter()
            .filter(|o| self.session.is_selected(o.id))
        {
            let r = self.view.rect_to_screen(o.bounds());
            painter.rect_stroke(
                r,
                0.0,
                Stroke::new(1.0, p.accent.gamma_multiply(0.5)),
                egui::StrokeKind::Middle,
            );
        }
        let Some(b) = self.selection_bounds() else {
            return;
        };
        if matches!(self.tool, crate::tools::Tool::Crop) {
            return;
        }
        let handle = |pos: Pos2| {
            let r = egui::Rect::from_center_size(pos, egui::Vec2::splat(2.0 * HANDLE));
            painter.rect_filled(r, 1.5, p.window);
            painter.rect_stroke(r, 1.5, stroke, egui::StrokeKind::Middle);
        };
        if let Some((_, a, bpos)) = self.selected_ends() {
            handle(a);
            handle(bpos);
            return;
        }
        let r = self.view.rect_to_screen(b);
        painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
        let rot = self.rotate_handle(b);
        painter.line_segment([egui::pos2(r.center().x, r.top()), rot], stroke);
        painter.circle(rot, HANDLE + 0.5, p.window, stroke);
        for i in 0..8 {
            handle(self.view.to_screen(handle_pos(b, i)));
        }
    }

    pub(crate) fn draw_marquee(&self, painter: &egui::Painter) {
        if let Gesture::Marquee { start, end, .. } = self.gesture {
            let p = pal();
            let r = self.view.rect_to_screen(Rect::from_points(start, end));
            painter.rect_filled(r, 0.0, p.accent.gamma_multiply(0.12));
            painter.rect_stroke(r, 0.0, Stroke::new(1.0, p.accent), egui::StrokeKind::Middle);
        }
    }
}

/// Canvas transform for a drag of the selection from `start` to `p`.
fn transform_for(kind: TransformKind, start: Point, b: Rect, p: Point, shift: bool) -> Affine {
    match kind {
        TransformKind::Move => {
            let mut d = p - start;
            if shift {
                if d.x.abs() > d.y.abs() {
                    d.y = 0.0;
                } else {
                    d.x = 0.0;
                }
            }
            Affine::translate(d)
        }
        TransformKind::Rotate => {
            let c = b.center();
            let mut angle = (p - c).atan2() - (start - c).atan2();
            if shift {
                let snap = ROTATE_SNAP.to_radians();
                angle = (angle / snap).round() * snap;
            }
            Affine::rotate_about(angle, c)
        }
        TransformKind::Scale(i) => {
            let (fx, fy) = HANDLES[i];
            let h = handle_pos(b, i);
            let anchor = handle_pos(b, (i + 4) % 8);
            let ratio = |cur: f64, h: f64, a: f64| {
                if (h - a).abs() < 1e-9 {
                    1.0
                } else {
                    (cur - a) / (h - a)
                }
            };
            let mut sx = if fx != 0.0 {
                ratio(p.x, h.x, anchor.x)
            } else {
                1.0
            };
            let mut sy = if fy != 0.0 {
                ratio(p.y, h.y, anchor.y)
            } else {
                1.0
            };
            if shift {
                let s = if fx == 0.0 {
                    sy
                } else if fy == 0.0 || sx.abs() > sy.abs() {
                    sx
                } else {
                    sy
                };
                sx = s;
                sy = s;
            }
            let clamp = |s: f64| {
                if s.abs() < 0.01 {
                    0.01_f64.copysign(s)
                } else {
                    s
                }
            };
            let (sx, sy) = (clamp(sx), clamp(sy));
            // The anchor of an edge handle is the middle of the opposite edge.
            let a = Vec2::new(anchor.x, anchor.y);
            Affine::translate(a) * Affine::scale_non_uniform(sx, sy) * Affine::translate(-a)
        }
    }
}

/// `p` moved so the segment from `from` is a multiple of 45°.
pub fn snap_45(from: Point, p: Point) -> Point {
    let d = p - from;
    let len = d.hypot();
    let step = std::f64::consts::FRAC_PI_4;
    let angle = (d.atan2() / step).round() * step;
    from + Vec2::from_angle(angle) * len
}

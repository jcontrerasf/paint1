//! Rectangle and free selection, as in Paint: they cut a piece out of an image that can then
//! be moved, scaled or rotated. A piece let go without changes goes back into the image.

use eframe::egui::{self, Color32, Pos2, Shape, Stroke, Ui};
use kurbo::{Point, Rect};

use crate::app::{App, Gesture};
use crate::document::{Kind, ObjectId};
use crate::i18n::t;
use crate::theme::pal;
use crate::tools::Tool;

impl App {
    /// Image the area tools work on: the selected one, else the topmost under `p`.
    fn area_target(&self, p: Point) -> Option<ObjectId> {
        let is_image = |id: &ObjectId| {
            self.doc()
                .get(*id)
                .is_some_and(|o| matches!(o.kind, Kind::Image { .. }))
        };
        if let [id] = self.session.selection.as_slice()
            && is_image(id)
            && self.floating.is_none_or(|(f, _)| f != *id)
        {
            return Some(*id);
        }
        let tol = self.view.px();
        self.doc()
            .objects
            .iter()
            .rev()
            .find(|o| o.visible && matches!(o.kind, Kind::Image { .. }) && o.hit(p, tol))
            .map(|o| o.id)
    }

    /// Is the screen point on the floating piece or one of its handles?
    fn on_floating(&self, pos: Pos2) -> bool {
        let Some((id, _)) = self.floating else {
            return false;
        };
        if self.session.selection.as_slice() != [id] {
            return false;
        }
        let inside = self
            .doc()
            .get(id)
            .is_some_and(|o| o.hit(self.view.to_doc(pos), self.view.px()));
        inside || self.grab_handle(pos)
    }

    pub(crate) fn area_tool(&mut self, ui: &Ui, resp: &egui::Response) {
        let shift = ui.input(|i| i.modifiers.shift);
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let start_screen = ui
                .input(|i| i.pointer.press_origin())
                .or(resp.interact_pointer_pos())
                .unwrap_or_default();
            if self.on_floating(start_screen) {
                // Move, scale or rotate the piece like with the select tool.
                self.begin_select_drag(start_screen, shift);
            } else {
                self.let_go_floating();
                let start = self.view.to_doc(start_screen);
                match self.area_target(start) {
                    Some(target) => {
                        self.session.select_only(target);
                        self.gesture = Gesture::Area {
                            target,
                            points: vec![start, start],
                            rect: self.tool == Tool::RectSelect,
                        };
                    }
                    None => self.info(t().select_an_image),
                }
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let p = self.view.to_doc(pos);
            let min_step = self.view.px();
            match &mut self.gesture {
                Gesture::Area { points, rect, .. } => {
                    if *rect {
                        points[1] = p;
                    } else if points.last().is_none_or(|l| (*l - p).hypot() >= min_step) {
                        points.push(p);
                    }
                }
                _ => self.update_select_drag(pos, shift),
            }
        }
        if resp.drag_stopped() {
            if let Gesture::Area {
                target,
                points,
                rect,
            } = std::mem::replace(&mut self.gesture, Gesture::None)
            {
                self.cut_area(target, &points, rect);
            } else {
                self.end_select_drag();
            }
        }
        if resp.clicked() {
            // A click outside the piece lets it go.
            self.let_go_floating();
            if let Some(pos) = resp.interact_pointer_pos()
                && let Some(target) = self.area_target(self.view.to_doc(pos))
            {
                self.session.select_only(target);
            } else {
                self.session.selection.clear();
            }
        }
    }

    /// Lets go of the floating piece now (back into its image if it was not changed).
    fn let_go_floating(&mut self) {
        if self.floating.is_some() {
            self.session.selection.clear();
            self.settle_floating();
        }
    }

    fn cut_area(&mut self, target: ObjectId, points: &[Point], rect: bool) {
        let area: Vec<Point> = if rect {
            let r = Rect::from_points(points[0], points[1]);
            vec![
                Point::new(r.x0, r.y0),
                Point::new(r.x1, r.y0),
                Point::new(r.x1, r.y1),
                Point::new(r.x0, r.y1),
            ]
        } else {
            points.to_vec()
        };
        self.session.end_group();
        let name = t().kind_selection;
        let piece = self
            .session
            .edit(|d| crate::raster::lift(d, target, &area, name));
        match piece {
            Some(id) => {
                self.session.select_only(id);
                self.floating = Some((id, self.session.revision()));
            }
            None => self.session.revert_last(),
        }
    }

    pub(crate) fn draw_area(&self, painter: &egui::Painter) {
        let Gesture::Area {
            target,
            points,
            rect,
        } = &self.gesture
        else {
            return;
        };
        let p = pal();
        if let Some(o) = self.doc().get(*target) {
            let r = self.view.rect_to_screen(o.shape_bounds());
            painter.rect_stroke(
                r,
                0.0,
                Stroke::new(1.0, p.accent.gamma_multiply(0.6)),
                egui::StrokeKind::Middle,
            );
        }
        let mut pts: Vec<Pos2> = if *rect {
            let r = self
                .view
                .rect_to_screen(Rect::from_points(points[0], points[1]));
            vec![
                r.left_top(),
                r.right_top(),
                r.right_bottom(),
                r.left_bottom(),
            ]
        } else {
            points.iter().map(|q| self.view.to_screen(*q)).collect()
        };
        if let Some(first) = pts.first().copied() {
            pts.push(first);
        }
        // Marching-ants look: white under dark dashes.
        painter.add(Shape::line(pts.clone(), Stroke::new(1.0, Color32::WHITE)));
        painter.extend(Shape::dashed_line(
            &pts,
            Stroke::new(1.0, Color32::from_gray(20)),
            4.0,
            4.0,
        ));
    }
}

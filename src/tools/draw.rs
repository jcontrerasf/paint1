//! Drawing tools: pencil, highlighter, eraser, rectangles, ellipse, line, arrow and polygon.
//! The new object goes into the document as soon as the drag starts and is updated while
//! dragging, all as one undo step.

use eframe::egui::{self, Stroke, Ui};
use kurbo::{Affine, Point, Rect, Size, Vec2};

use crate::app::{App, Gesture, add_object};
use crate::document::{Kind, StrokeMode};
use crate::theme::pal;
use crate::tools::Tool;
use crate::tools::select::snap_45;

/// Closing a polygon: how near the first point a click must be (screen points).
const CLOSE_PX: f32 = 8.0;
/// Shapes smaller than this (screen points) are dropped.
const MIN_SHAPE_PX: f64 = 2.0;

fn stroke_mode(tool: Tool) -> Option<StrokeMode> {
    match tool {
        Tool::Pencil => Some(StrokeMode::Pencil),
        Tool::Highlighter => Some(StrokeMode::Highlighter),
        Tool::Eraser => Some(StrokeMode::Eraser),
        _ => None,
    }
}

impl App {
    pub(crate) fn draw_tool(&mut self, ui: &Ui, resp: &egui::Response) {
        let shift = ui.input(|i| i.modifiers.shift);
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let start = self.view.to_doc(
                ui.input(|i| i.pointer.press_origin())
                    .or(resp.interact_pointer_pos())
                    .unwrap_or_default(),
            );
            self.begin_shape(start);
        }
        if resp.dragged_by(egui::PointerButton::Primary)
            && let Some(pos) = resp.interact_pointer_pos()
        {
            self.update_shape(self.view.to_doc(pos), shift);
        }
        if resp.drag_stopped() {
            self.end_shape();
        }
        // A click with a freehand tool leaves a dot.
        if resp.clicked()
            && let Some(mode) = stroke_mode(self.tool)
            && let Some(pos) = resp.interact_pointer_pos()
        {
            let p = self.view.to_doc(pos);
            let style = self.style.for_tool(self.tool);
            self.session.edit(|d| {
                add_object(
                    d,
                    Kind::Stroke {
                        points: vec![p],
                        mode,
                    },
                    style,
                    Affine::IDENTITY,
                )
            });
        }
    }

    fn begin_shape(&mut self, start: Point) {
        let style = self.style.for_tool(self.tool);
        let kind = match self.tool {
            Tool::Pencil | Tool::Highlighter | Tool::Eraser => Kind::Stroke {
                points: vec![start],
                mode: stroke_mode(self.tool).unwrap_or(StrokeMode::Pencil),
            },
            Tool::Rect => Kind::Rect {
                size: Size::ZERO,
                radius: 0.0,
            },
            Tool::RoundRect => Kind::Rect {
                size: Size::ZERO,
                radius: self.style.radius,
            },
            Tool::Ellipse => Kind::Ellipse { size: Size::ZERO },
            Tool::RegularPolygon => Kind::RegularPolygon {
                sides: self.style.sides,
                size: Size::ZERO,
            },
            Tool::Line => Kind::Line {
                a: Point::ZERO,
                b: Point::ZERO,
            },
            Tool::Arrow => Kind::Arrow {
                a: Point::ZERO,
                b: Point::ZERO,
                head: self.style.head,
            },
            _ => return,
        };
        let transform = match kind {
            Kind::Stroke { .. } => Affine::IDENTITY,
            _ => Affine::translate(start.to_vec2()),
        };
        self.session.end_group();
        let id = self
            .session
            .edit_grouped("draw", |d| add_object(d, kind, style, transform));
        self.gesture = Gesture::Draw { id, start };
    }

    fn update_shape(&mut self, p: Point, shift: bool) {
        let Gesture::Draw { id, start } = self.gesture else {
            return;
        };
        let min_step = self.view.px();
        self.session.edit_grouped("draw", |d| {
            let Some(o) = d.get_mut(id) else {
                return;
            };
            match &mut o.kind {
                Kind::Stroke { points, .. } => {
                    if points.last().is_none_or(|l| (*l - p).hypot() >= min_step) {
                        points.push(p);
                    }
                }
                Kind::Rect { size, .. }
                | Kind::Ellipse { size }
                | Kind::RegularPolygon { size, .. } => {
                    let mut d = p - start;
                    if shift {
                        let side = d.x.abs().max(d.y.abs());
                        d = Vec2::new(side.copysign(d.x), side.copysign(d.y));
                    }
                    let r = Rect::from_points(start, start + d);
                    *size = r.size();
                    o.transform = Affine::translate(r.origin().to_vec2());
                }
                Kind::Line { b, .. } | Kind::Arrow { b, .. } => {
                    let end = if shift { snap_45(start, p) } else { p };
                    *b = Point::ZERO + (end - start);
                }
                _ => {}
            }
        });
    }

    fn end_shape(&mut self) {
        let Gesture::Draw { id, .. } = self.gesture else {
            return;
        };
        self.gesture = Gesture::None;
        let tiny = self.doc().get(id).is_some_and(|o| {
            let r = o.local_rect();
            !matches!(o.kind, Kind::Stroke { .. })
                && r.width().max(r.height()) * (self.view.scale as f64) < MIN_SHAPE_PX
        });
        if tiny {
            self.session.cancel_group();
        } else {
            self.session.end_group();
        }
    }

    pub(crate) fn polygon_tool(&mut self, ui: &Ui, resp: &egui::Response) {
        // The last point follows the pointer.
        if let (Some(id), Some(pos)) = (self.polygon, resp.hover_pos()) {
            let p = self.view.to_doc(pos);
            let p = if ui.input(|i| i.modifiers.shift) {
                self.doc()
                    .get(id)
                    .and_then(|o| match &o.kind {
                        Kind::Polygon { points, .. } if points.len() >= 2 => {
                            Some(snap_45(points[points.len() - 2], p))
                        }
                        _ => None,
                    })
                    .unwrap_or(p)
            } else {
                p
            };
            let moved = self.doc().get(id).is_some_and(
                |o| matches!(&o.kind, Kind::Polygon { points, .. } if points.last() != Some(&p)),
            );
            if moved {
                self.session.edit_grouped("polygon", |d| {
                    if let Some(Kind::Polygon { points, .. }) = d.get_mut(id).map(|o| &mut o.kind)
                        && let Some(last) = points.last_mut()
                    {
                        *last = p;
                    }
                });
            }
        }
        if resp.double_clicked() {
            self.finish_polygon();
            return;
        }
        if !resp.clicked() {
            return;
        }
        let Some(pos) = resp.interact_pointer_pos() else {
            return;
        };
        let p = self.view.to_doc(pos);
        match self.polygon {
            None => {
                let style = self.style.for_tool(Tool::Polygon);
                self.session.end_group();
                let id = self.session.edit_grouped("polygon", |d| {
                    add_object(
                        d,
                        Kind::Polygon {
                            points: vec![p, p],
                            closed: false,
                        },
                        style,
                        Affine::IDENTITY,
                    )
                });
                self.polygon = Some(id);
            }
            Some(id) => {
                let first = self.doc().get(id).and_then(|o| match &o.kind {
                    Kind::Polygon { points, .. } if points.len() >= 4 => Some(points[0]),
                    _ => None,
                });
                if first.is_some_and(|f| self.view.to_screen(f).distance(pos) <= CLOSE_PX) {
                    self.finish_polygon();
                    return;
                }
                self.session.edit_grouped("polygon", |d| {
                    if let Some(Kind::Polygon { points, .. }) = d.get_mut(id).map(|o| &mut o.kind) {
                        let last = *points.last().unwrap_or(&p);
                        points.push(last);
                    }
                });
            }
        }
    }

    /// Ends the polygon in progress: drops the point that followed the pointer and repeated
    /// points; with 3 or more points it is closed.
    pub fn finish_polygon(&mut self) {
        let Some(id) = self.polygon.take() else {
            return;
        };
        let min = self.view.px();
        let keep = self.session.edit_grouped("polygon", |d| {
            let Some(Kind::Polygon { points, closed }) = d.get_mut(id).map(|o| &mut o.kind) else {
                return false;
            };
            points.pop();
            points.dedup_by(|a, b| (*a - *b).hypot() < min);
            if points.len() >= 3 && (points[0] - points[points.len() - 1]).hypot() < min * 8.0 {
                points.pop();
            }
            *closed = points.len() >= 3;
            points.len() >= 2
        });
        if keep {
            self.session.end_group();
        } else {
            self.session.cancel_group();
        }
    }

    /// Ring around the first point when a click there would close the polygon.
    pub(crate) fn draw_polygon_hint(&self, painter: &egui::Painter) {
        let Some(id) = self.polygon else {
            return;
        };
        if let Some(Kind::Polygon { points, .. }) = self.doc().get(id).map(|o| &o.kind)
            && points.len() >= 4
        {
            let first = self.view.to_screen(points[0]);
            painter.circle_stroke(first, CLOSE_PX, Stroke::new(1.5, pal().accent));
        }
    }
}

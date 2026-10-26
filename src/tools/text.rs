//! Text tool: click to place a text and type it right on the canvas.

use eframe::egui::{self, Key, Stroke, Ui};
use kurbo::{Affine, Vec2};

use crate::app::{App, add_object, default_text_kind};
use crate::document::{Kind, ObjectId};
use crate::text;
use crate::theme::pal;
use crate::tools::Tool;
use crate::tools::select::PICK;

/// The text being typed on the canvas.
pub struct TextEditing {
    pub id: ObjectId,
    /// Made by this edit (dropped if left empty).
    pub created: bool,
}

impl App {
    pub(crate) fn text_tool(&mut self, _ui: &Ui, resp: &egui::Response) {
        if !resp.clicked() {
            return;
        }
        let Some(pos) = resp.interact_pointer_pos() else {
            return;
        };
        let p = self.view.to_doc(pos);
        let tol = PICK * self.view.px();
        if let Some(editing) = &self.text_edit {
            let on_it = self.doc().get(editing.id).is_some_and(|o| o.hit(p, tol));
            if !on_it {
                self.finish_text();
            }
            return;
        }
        let picked = self.doc().pick(p, tol).filter(|id| {
            matches!(
                self.doc().get(*id).map(|o| &o.kind),
                Some(Kind::Text { .. })
            )
        });
        if let Some(id) = picked {
            self.start_text_edit(id, false);
            return;
        }
        let style = self.style.for_tool(Tool::Text);
        let kind = default_text_kind(&self.style);
        // The click marks the middle of the first line.
        let half_line = text::line_height(self.style.font_size, self.style.bold) / 2.0;
        let at = p - Vec2::new(0.0, half_line);
        self.session.end_group();
        let id = self.session.edit_grouped("text", |d| {
            add_object(d, kind, style, Affine::translate(at.to_vec2()))
        });
        self.start_text_edit(id, true);
    }

    pub fn start_text_edit(&mut self, id: ObjectId, created: bool) {
        if !created {
            self.session.end_group();
        }
        self.text_edit = Some(TextEditing { id, created });
        self.session.select_only(id);
    }

    /// Keys while typing on the canvas (instead of the shortcuts).
    pub(crate) fn text_keys(&mut self, ctx: &egui::Context) {
        if ctx.memory(|m| m.focused()).is_some() {
            return;
        }
        let Some(id) = self.text_edit.as_ref().map(|e| e.id) else {
            return;
        };
        let events = ctx.input(|i| i.events.clone());
        let mut finish = false;
        for e in events {
            match e {
                egui::Event::Text(s) | egui::Event::Paste(s) => {
                    self.type_text(id, |c| c.push_str(&s))
                }
                egui::Event::Key {
                    key: Key::Enter,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    if modifiers.command {
                        finish = true;
                    } else {
                        self.type_text(id, |c| c.push('\n'));
                    }
                }
                egui::Event::Key {
                    key: Key::Backspace,
                    pressed: true,
                    ..
                } => self.type_text(id, |c| {
                    c.pop();
                }),
                egui::Event::Key {
                    key: Key::Escape,
                    pressed: true,
                    ..
                } => finish = true,
                _ => {}
            }
        }
        // Keys used here must not reach other widgets.
        ctx.input_mut(|i| {
            i.events.retain(|e| {
                !matches!(
                    e,
                    egui::Event::Text(_) | egui::Event::Paste(_) | egui::Event::Key { .. }
                )
            })
        });
        if finish {
            self.finish_text();
        }
    }

    fn type_text(&mut self, id: ObjectId, f: impl FnOnce(&mut String)) {
        self.session.edit_grouped("text", |d| {
            if let Some(Kind::Text { content, .. }) = d.get_mut(id).map(|o| &mut o.kind) {
                f(content);
            }
        });
    }

    /// Stops typing; an empty text is removed.
    pub fn finish_text(&mut self) {
        let Some(editing) = self.text_edit.take() else {
            return;
        };
        let empty = matches!(
            self.doc().get(editing.id).map(|o| &o.kind),
            Some(Kind::Text { content, .. }) if content.trim().is_empty()
        );
        if empty {
            if editing.created {
                self.session.cancel_group();
            } else {
                self.session.end_group();
                self.session
                    .edit(|d| d.objects.retain(|o| o.id != editing.id));
            }
            self.session.selection.clear();
        } else {
            self.session.end_group();
        }
    }

    pub(crate) fn draw_text_caret(&self, painter: &egui::Painter) {
        let Some(editing) = &self.text_edit else {
            return;
        };
        let Some(o) = self.doc().get(editing.id) else {
            return;
        };
        if let Kind::Text {
            content,
            size,
            bold,
            align,
        } = &o.kind
        {
            let (top, h) = text::caret(content, *size, *bold, *align);
            let a = self.view.to_screen(o.transform * top);
            let b = self.view.to_screen(o.transform * (top + Vec2::new(0.0, h)));
            painter.line_segment([a, b], Stroke::new(2.0, pal().accent));
        }
    }
}

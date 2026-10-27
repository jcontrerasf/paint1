//! Text tool: click to place a text and type it right on the canvas.

use eframe::egui::{self, Key, Stroke, Ui};
use kurbo::{Affine, Point, Vec2};

use crate::app::{App, add_object};
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
    /// Byte index in the content where typing goes.
    pub cursor: usize,
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
        if let Some(id) = self.text_edit.as_ref().map(|e| e.id) {
            let on_it = self.doc().get(id).is_some_and(|o| o.hit(p, tol));
            if on_it {
                self.place_cursor(p);
            } else {
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
            self.place_cursor(p);
            return;
        }
        let style = self.style.for_tool(Tool::Text);
        let kind = self.style.text_kind("");
        // The click marks the middle of the first line.
        let half_line = text::line_height(&kind.text_look().unwrap()) / 2.0;
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
        let cursor = match self.doc().get(id).map(|o| &o.kind) {
            Some(Kind::Text { content, .. }) => content.len(),
            _ => 0,
        };
        self.text_edit = Some(TextEditing {
            id,
            created,
            cursor,
        });
        self.session.select_only(id);
    }

    /// Moves the cursor of the text being typed to the char closest to `p` (document).
    fn place_cursor(&mut self, p: Point) {
        let Some(editing) = &self.text_edit else {
            return;
        };
        let Some(o) = self.session.doc().get(editing.id) else {
            return;
        };
        if let (Kind::Text { content, .. }, Some(look)) = (&o.kind, o.kind.text_look()) {
            let local = o.transform.inverse() * p;
            let i = text::index_at(content, local, &look);
            if let Some(e) = &mut self.text_edit {
                e.cursor = i;
            }
        }
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
                    self.insert_text(id, &s.replace("\r\n", "\n"))
                }
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => match key {
                    Key::Enter if modifiers.command => finish = true,
                    Key::Enter => self.insert_text(id, "\n"),
                    Key::Escape => finish = true,
                    Key::Backspace => self.delete_text(id, |s, i| {
                        if modifiers.command {
                            text::prev_word(s, i)
                        } else {
                            text::prev_char(s, i)
                        }
                    }),
                    Key::Delete => self.delete_text(id, |s, i| {
                        if modifiers.command {
                            text::next_word(s, i)
                        } else {
                            text::next_char(s, i)
                        }
                    }),
                    Key::ArrowLeft if modifiers.command => self.move_cursor(id, text::prev_word),
                    Key::ArrowLeft => self.move_cursor(id, text::prev_char),
                    Key::ArrowRight if modifiers.command => self.move_cursor(id, text::next_word),
                    Key::ArrowRight => self.move_cursor(id, text::next_char),
                    Key::Home if modifiers.command => self.move_cursor(id, |_, _| 0),
                    Key::Home => self.move_cursor(id, text::line_start),
                    Key::End if modifiers.command => self.move_cursor(id, |s, _| s.len()),
                    Key::End => self.move_cursor(id, text::line_end),
                    Key::ArrowUp => self.move_line(id, -1.0),
                    Key::ArrowDown => self.move_line(id, 1.0),
                    _ => {}
                },
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

    /// Content of the text and the cursor kept inside it (the content may have changed from
    /// the properties panel or undo).
    fn content_and_cursor(&mut self, id: ObjectId) -> Option<(String, usize)> {
        let Some(Kind::Text { content, .. }) = self.session.doc().get(id).map(|o| &o.kind) else {
            return None;
        };
        let editing = self.text_edit.as_mut()?;
        let mut i = editing.cursor.min(content.len());
        while !content.is_char_boundary(i) {
            i -= 1;
        }
        editing.cursor = i;
        Some((content.clone(), i))
    }

    fn set_cursor(&mut self, i: usize) {
        if let Some(e) = &mut self.text_edit {
            e.cursor = i;
        }
    }

    fn type_text(&mut self, id: ObjectId, f: impl FnOnce(&mut String)) {
        self.session.edit_grouped("text", |d| {
            if let Some(Kind::Text { content, .. }) = d.get_mut(id).map(|o| &mut o.kind) {
                f(content);
            }
        });
    }

    fn insert_text(&mut self, id: ObjectId, s: &str) {
        let Some((_, i)) = self.content_and_cursor(id) else {
            return;
        };
        self.type_text(id, |c| c.insert_str(i, s));
        self.set_cursor(i + s.len());
    }

    /// Deletes between the cursor and where `to` goes from it (either side).
    fn delete_text(&mut self, id: ObjectId, to: impl Fn(&str, usize) -> usize) {
        let Some((content, i)) = self.content_and_cursor(id) else {
            return;
        };
        let j = to(&content, i);
        let (a, b) = (i.min(j), i.max(j));
        if a == b {
            return;
        }
        self.type_text(id, |c| {
            c.replace_range(a..b, "");
        });
        self.set_cursor(a);
    }

    fn move_cursor(&mut self, id: ObjectId, to: impl Fn(&str, usize) -> usize) {
        if let Some((content, i)) = self.content_and_cursor(id) {
            self.set_cursor(to(&content, i));
        }
    }

    /// Cursor to the line above (`-1`) or below (`1`), keeping its x.
    fn move_line(&mut self, id: ObjectId, dir: f64) {
        let Some((content, i)) = self.content_and_cursor(id) else {
            return;
        };
        let Some(look) = self.doc().get(id).and_then(|o| o.kind.text_look()) else {
            return;
        };
        let (top, h) = text::caret(&content, i, &look);
        let lines = content.split('\n').count() as f64;
        let y = top.y + h / 2.0 + dir * h;
        let to = if y < 0.0 {
            0
        } else if y >= lines * h {
            content.len()
        } else {
            text::index_at(&content, Point::new(top.x, y), &look)
        };
        self.set_cursor(to);
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
        if let (Kind::Text { content, .. }, Some(look)) = (&o.kind, o.kind.text_look()) {
            let cursor = editing.cursor.min(content.len());
            let cursor = (0..=cursor)
                .rev()
                .find(|i| content.is_char_boundary(*i))
                .unwrap_or(0);
            let (top, h) = text::caret(content, cursor, &look);
            let a = self.view.to_screen(o.transform * top);
            let b = self.view.to_screen(o.transform * (top + Vec2::new(0.0, h)));
            painter.line_segment([a, b], Stroke::new(2.0, pal().accent));
        }
    }
}

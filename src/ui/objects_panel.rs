//! Right panel: the list of everything drawn (front first) and, below, the properties of the
//! selection.

use eframe::egui::{self, Align, Layout, RichText, Ui};

use crate::app::App;
use crate::i18n::t;
use crate::icons;
use crate::theme::{self, pal};
use crate::tools::Tool;
use crate::ui::{display_name, drop_gap, dropped, kind_icon, row};

impl App {
    pub(crate) fn side_panel(&mut self, ui: &mut Ui) {
        egui::Panel::top("objects_list")
            .resizable(true)
            .default_size(300.0)
            .min_size(90.0)
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| self.objects_list(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 8)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("properties")
                    .auto_shrink([false, false])
                    .show(ui, |ui| self.properties(ui));
            });
    }

    fn objects_list(&mut self, ui: &mut Ui) {
        let t = t();
        ui.horizontal(|ui| {
            theme::section_title(ui, t.objects);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(self.doc().objects.len().to_string())
                        .small()
                        .color(pal().text_soft),
                );
            });
        });
        let n = self.doc().objects.len();
        if n == 0 {
            ui.label(RichText::new(t.nothing_drawn).color(pal().text_soft));
            return;
        }
        let mut gap = None;
        let mut clicked = None;
        let mut toggle_visible = None;
        let mut toggle_lock = None;
        let mut rename_done = None;
        let mut start_rename = None;
        egui::ScrollArea::vertical()
            .id_salt("objects")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                // Front first: display row `i` is object `n - 1 - i`.
                for i in 0..n {
                    let o = &self.session.doc().objects[n - 1 - i];
                    let (id, visible, locked) = (o.id, o.visible, o.locked);
                    let icon = kind_icon(o);
                    let name = display_name(o);
                    let selected = self.session.is_selected(id);
                    let renaming = self.rename.as_ref().is_some_and(|(r, _)| *r == id);
                    let soft = pal().text_soft;
                    let resp = row(
                        ui,
                        i,
                        selected,
                        |ui| {
                            ui.label(RichText::new(icon).size(16.0).color(if visible {
                                pal().text
                            } else {
                                soft
                            }));
                            if renaming {
                                if let Some((_, text)) = &mut self.rename {
                                    let r = ui
                                        .add(egui::TextEdit::singleline(text).desired_width(140.0));
                                    if !r.has_focus() && !r.lost_focus() {
                                        r.request_focus();
                                    }
                                    if r.lost_focus() {
                                        rename_done = Some((id, text.clone()));
                                    }
                                }
                            } else {
                                let text = RichText::new(name);
                                let text = if visible { text } else { text.color(soft) };
                                ui.add(egui::Label::new(text).truncate().selectable(false));
                            }
                        },
                        |ui| {
                            let (ico, active) = if locked {
                                (icons::LOCKED, true)
                            } else {
                                (icons::UNLOCKED, false)
                            };
                            if theme::mini(ui, active, ico, t.lock_unlock).clicked() {
                                toggle_lock = Some(id);
                            }
                            let ico = if visible {
                                icons::VISIBLE
                            } else {
                                icons::HIDDEN
                            };
                            if theme::mini(ui, visible, ico, t.show_hide).clicked() {
                                toggle_visible = Some(id);
                            }
                        },
                    );
                    if resp.double_clicked() {
                        start_rename = Some(id);
                    } else if resp.clicked() {
                        clicked = Some(id);
                    }
                    if let Some(g) = drop_gap(ui, &resp, i) {
                        gap = Some(g);
                    }
                }
            });
        if let Some((from, to)) = dropped(ui, gap) {
            self.reorder(n - 1 - from, n - 1 - to);
        }
        if let Some(id) = clicked {
            let multi = ui.input(|i| i.modifiers.command || i.modifiers.shift);
            if multi {
                if self.session.is_selected(id) {
                    self.session.selection.retain(|s| *s != id);
                } else {
                    self.session.selection.push(id);
                }
            } else {
                self.settle();
                self.session.select_only(id);
            }
            if !matches!(self.tool, Tool::Select) {
                self.set_tool(Tool::Select);
                self.session.select_only(id);
            }
        }
        if let Some(id) = start_rename {
            let name = self.doc().get(id).map(display_name).unwrap_or_default();
            self.rename = Some((id, name));
        }
        if let Some((id, name)) = rename_done {
            self.rename = None;
            let auto = self.doc().get(id).map(|o| {
                let mut o = o.clone();
                o.name.clear();
                display_name(&o)
            });
            let name = name.trim().to_string();
            // Leaving the automatic name as is keeps it automatic.
            let name = if auto.as_deref() == Some(name.as_str()) {
                String::new()
            } else {
                name
            };
            self.session.edit(|d| {
                if let Some(o) = d.get_mut(id) {
                    o.name = name;
                }
            });
        }
        if let Some(id) = toggle_visible {
            self.settle();
            self.session.edit(|d| {
                if let Some(o) = d.get_mut(id) {
                    o.visible = !o.visible;
                }
            });
        }
        if let Some(id) = toggle_lock {
            self.settle();
            self.session.edit(|d| {
                if let Some(o) = d.get_mut(id) {
                    o.locked = !o.locked;
                }
            });
            if self.doc().get(id).is_some_and(|o| o.locked) {
                self.session.selection.retain(|s| *s != id);
            }
        }
    }
}

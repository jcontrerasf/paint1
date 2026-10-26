//! Status bar: messages and hints on the left; zoom, canvas size and pointer on the right.

use eframe::egui::{self, Align, Layout, RichText, Ui};

use crate::app::App;
use crate::i18n::t;
use crate::icons;
use crate::theme::{self, pal};

impl App {
    pub(crate) fn status_bar(&mut self, ui: &mut Ui) {
        let p = pal();
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            // Fixed width: does not jump when the pointer moves or leaves the canvas.
            let pos = self.pointer_doc.map_or(String::new(), |p| {
                format!("{:5.0}, {:5.0} px", p.x.floor(), p.y.floor())
            });
            ui.add_sized(
                [110.0, 20.0],
                egui::Label::new(RichText::new(pos).monospace().color(p.text_soft)),
            );
            ui.separator();
            let d = self.doc();
            ui.label(RichText::new(format!("{} × {} px", d.width, d.height)).color(p.text_soft));
            ui.separator();
            ui.spacing_mut().item_spacing.x = 2.0;
            if theme::icon_button(ui, icons::ZOOM_IN, "Ctrl +").clicked() {
                self.view.zoom_by(1.25);
            }
            if ui
                .button(format!("{:.0} %", self.view.scale * 100.0))
                .on_hover_text(t().zoom_fit)
                .clicked()
            {
                self.view.fit_pending = true;
            }
            if theme::icon_button(ui, icons::ZOOM_OUT, "Ctrl −").clicked() {
                self.view.zoom_by(0.8);
            }
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.separator();
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.shrink_clip_rect(ui.max_rect());
                match &self.message {
                    Some((text, true)) => {
                        ui.add(egui::Label::new(RichText::new(text).color(p.error)).truncate());
                    }
                    Some((text, false)) => {
                        ui.add(egui::Label::new(text.as_str()).truncate());
                    }
                    None => {
                        ui.add(
                            egui::Label::new(RichText::new(self.tool.hint()).color(p.text_soft))
                                .truncate(),
                        );
                    }
                }
            });
        });
        // A message stays until the user does something on the canvas.
        if self.message.is_some()
            && self.export_job.is_none()
            && ui.input(|i| i.pointer.primary_pressed())
            && self
                .view
                .rect
                .contains(ui.input(|i| i.pointer.interact_pos()).unwrap_or_default())
        {
            self.message = None;
        }
    }
}

//! Column of tools on the left.

use eframe::egui::{self, Ui};

use crate::app::App;
use crate::i18n::t;
use crate::icons;
use crate::theme;
use crate::tools::Tool;

impl App {
    pub(crate) fn tools_bar(&mut self, ui: &mut Ui) {
        egui::ScrollArea::vertical()
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| self.tool_buttons(ui));
    }

    fn tool_buttons(&mut self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            for tool in Tool::BAR {
                match tool {
                    Some(tool) => {
                        if theme::toggle(ui, self.tool == tool, tool.icon(), tool.help()).clicked()
                        {
                            self.set_tool(tool);
                        }
                    }
                    None => {
                        ui.separator();
                    }
                }
            }
            ui.separator();
            if theme::icon_button(ui, icons::FIT, t().zoom_fit).clicked() {
                self.view.fit_pending = true;
            }
        });
    }
}

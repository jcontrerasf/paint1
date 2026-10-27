//! Bar under the top bar with the options of the active tool.

use eframe::egui::{self, RichText, Ui};

use crate::app::App;
use crate::document::{Kind, Rgba, TextAlign};
use crate::i18n::{fill, t};
use crate::icons;
use crate::theme::{self, pal};
use crate::tools::Tool;
use crate::tools::crop::Aspect;
use crate::ui::{color_button, font_picker, palette_row};

impl App {
    pub(crate) fn context_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.set_min_height(30.0);
            match self.tool {
                Tool::Select => self.ctx_select(ui),
                Tool::Crop => self.ctx_crop(ui),
                Tool::Eyedropper => self.ctx_eyedropper(ui),
                _ => self.ctx_draw(ui),
            }
        });
    }

    fn ctx_draw(&mut self, ui: &mut Ui) {
        let t = t();
        let tool = self.tool;
        let before = self.style.clone();
        if tool != Tool::Eraser {
            let label = if tool == Tool::Text {
                t.text_color
            } else {
                t.stroke
            };
            ui.label(label);
            color_button(ui, self.style.color_mut(tool), label);
        }
        if tool.uses_fill() {
            ui.separator();
            let label = if tool == Tool::Text {
                t.text_box
            } else {
                t.fill
            };
            ui.checkbox(&mut self.style.fill_on, label);
            if color_button(ui, &mut self.style.fill, label) {
                self.style.fill_on = true;
            }
        }
        ui.separator();
        if tool == Tool::Text {
            font_picker(ui, "ctx_font", &mut self.style.font);
            ui.label(t.font_size);
            ui.add(
                egui::DragValue::new(&mut self.style.font_size)
                    .range(4.0..=1000.0)
                    .speed(0.5)
                    .suffix(" px"),
            );
            if self.style.fill_on {
                ui.label(t.margin);
                let mut px = self.style.text_margin * self.style.font_size;
                if ui
                    .add(
                        egui::DragValue::new(&mut px)
                            .range(0.0..=1000.0)
                            .speed(0.5)
                            .suffix(" px")
                            .max_decimals(1),
                    )
                    .changed()
                {
                    self.style.text_margin = px / self.style.font_size;
                }
            }
            if theme::toggle(ui, self.style.bold, icons::BOLD, t.bold).clicked() {
                self.style.bold = !self.style.bold;
            }
            if theme::toggle(ui, self.style.italic, icons::ITALIC, t.italic).clicked() {
                self.style.italic = !self.style.italic;
            }
            if theme::toggle(ui, self.style.underline, icons::UNDERLINE, t.underline).clicked() {
                self.style.underline = !self.style.underline;
            }
            for (align, ico, help) in [
                (TextAlign::Left, icons::ALIGN_LEFT, t.align_left),
                (TextAlign::Center, icons::ALIGN_CENTER, t.align_center),
                (TextAlign::Right, icons::ALIGN_RIGHT, t.align_right),
            ] {
                if theme::toggle(ui, self.style.align == align, ico, help).clicked() {
                    self.style.align = align;
                }
            }
        } else {
            ui.label(t.width);
            ui.add(
                egui::DragValue::new(self.style.width_mut(tool))
                    .range(0.5..=500.0)
                    .speed(0.2)
                    .suffix(" px"),
            );
        }
        if tool == Tool::RoundRect {
            ui.label(t.radius);
            ui.add(
                egui::DragValue::new(&mut self.style.radius)
                    .range(0.0..=2000.0)
                    .speed(0.5)
                    .suffix(" px"),
            );
        }
        if tool == Tool::RegularPolygon {
            ui.label(t.sides);
            ui.add(egui::DragValue::new(&mut self.style.sides).range(crate::document::SIDES));
        }
        if tool == Tool::Arrow {
            ui.label(t.head);
            ui.add(
                egui::DragValue::new(&mut self.style.head)
                    .range(2.0..=500.0)
                    .speed(0.5)
                    .suffix(" px"),
            );
        }
        if tool != Tool::Eraser {
            ui.label(t.opacity);
            let mut pct = self.style.opacity * 100.0;
            if ui
                .add(
                    egui::DragValue::new(&mut pct)
                        .range(1.0..=100.0)
                        .speed(0.5)
                        .suffix(" %"),
                )
                .changed()
            {
                self.style.opacity = pct / 100.0;
            }
            ui.separator();
            if let Some((c, secondary)) = palette_row(ui) {
                if secondary && tool.uses_fill() {
                    self.style.fill = c;
                    self.style.fill_on = true;
                } else {
                    *self.style.color_mut(tool) = c;
                }
            }
        }
        // While typing a text, its look follows the bar.
        if self.style != before
            && let Some(id) = self.text_edit.as_ref().map(|e| e.id)
        {
            let style = self.style.for_tool(Tool::Text);
            let bar = self.style.clone();
            self.session.edit_grouped("text", |d| {
                if let Some(o) = d.get_mut(id)
                    && let Kind::Text { content, .. } = &mut o.kind
                {
                    let content = std::mem::take(content);
                    o.style = style;
                    o.kind = bar.text_kind(&content);
                }
            });
        }
    }

    fn ctx_select(&mut self, ui: &mut Ui) {
        let t = t();
        let n = self.session.selection.len();
        if theme::icon_button(ui, icons::SELECT_ALL, &format!("{} (Ctrl+A)", t.select_all))
            .clicked()
        {
            self.select_all();
        }
        if n == 0 {
            ui.label(RichText::new(t.hint_select).color(pal().text_soft));
            return;
        }
        ui.label(if n == 1 {
            self.doc()
                .get(self.session.selection[0])
                .map(crate::ui::display_name)
                .unwrap_or_default()
        } else {
            fill(t.n_selected, &[&n])
        });
        ui.separator();
        if theme::icon_button(ui, icons::DUPLICATE, &format!("{} (Ctrl+D)", t.duplicate)).clicked()
        {
            self.duplicate_selection();
        }
        if theme::icon_button(ui, icons::DELETE, &format!("{} (Supr)", t.delete)).clicked() {
            self.delete_selection();
        }
        ui.separator();
        if theme::icon_button(ui, icons::TO_FRONT, t.to_front).clicked() {
            self.restack(true, true);
        }
        if theme::icon_button(ui, icons::RAISE, t.raise).clicked() {
            self.restack(true, false);
        }
        if theme::icon_button(ui, icons::LOWER, t.lower).clicked() {
            self.restack(false, false);
        }
        if theme::icon_button(ui, icons::TO_BACK, t.to_back).clicked() {
            self.restack(false, true);
        }
        ui.separator();
        if theme::icon_button(
            ui,
            icons::FIT_CANVAS,
            &format!("{} · {}", t.fit_canvas, t.fit_canvas_help),
        )
        .clicked()
        {
            self.fit_canvas_to_selection();
        }
        if theme::icon_button(
            ui,
            icons::FLATTEN,
            &format!("{} · {}", t.flatten, t.flatten_help),
        )
        .clicked()
        {
            self.flatten_selection();
        }
        ui.separator();
        if let Some((c, secondary)) = palette_row(ui) {
            self.recolor_selection(c, secondary);
        }
    }

    /// Quick color on the selection: outline/text color, or fill (secondary click).
    fn recolor_selection(&mut self, c: Rgba, fill: bool) {
        let ids = self.session.selection.clone();
        self.settle();
        self.session.edit(|d| {
            for o in d.objects.iter_mut().filter(|o| ids.contains(&o.id)) {
                if fill {
                    if o.can_fill() {
                        o.style.fill = Some(c);
                    }
                } else if !matches!(
                    o.kind,
                    Kind::Image { .. }
                        | Kind::Stroke {
                            mode: crate::document::StrokeMode::Eraser,
                            ..
                        }
                ) {
                    o.style.stroke = c;
                }
            }
        });
    }

    fn ctx_crop(&mut self, ui: &mut Ui) {
        let t = t();
        ui.label(t.aspect);
        let before = (self.crop.aspect, self.crop.exact);
        egui::ComboBox::from_id_salt("aspect")
            .selected_text(self.crop.aspect.label())
            .show_ui(ui, |ui| {
                for a in Aspect::PRESETS {
                    ui.selectable_value(&mut self.crop.aspect, a, a.label());
                }
            });
        if self.crop.aspect == Aspect::Exact {
            ui.add(
                egui::DragValue::new(&mut self.crop.exact.0)
                    .range(1..=20000)
                    .suffix(" px"),
            );
            ui.label("×");
            ui.add(
                egui::DragValue::new(&mut self.crop.exact.1)
                    .range(1..=20000)
                    .suffix(" px"),
            );
        }
        if (self.crop.aspect, self.crop.exact) != before {
            self.reset_crop();
        }
        ui.separator();
        let r = self.crop.rect;
        ui.label(
            RichText::new(format!("{:.0} × {:.0} px", r.width(), r.height()))
                .color(pal().text_soft),
        );
        ui.separator();
        if theme::primary_button(ui, icons::OK, t.apply)
            .on_hover_text("Enter")
            .clicked()
        {
            self.apply_crop();
        }
        if ui
            .button(format!("{} {}", icons::CANCEL, t.cancel))
            .on_hover_text("Esc")
            .clicked()
        {
            self.set_tool(Tool::Select);
        }
    }

    fn ctx_eyedropper(&mut self, ui: &mut Ui) {
        let t = t();
        ui.label(t.stroke);
        color_button(ui, &mut self.style.stroke, t.stroke);
        ui.separator();
        ui.checkbox(&mut self.style.fill_on, t.fill);
        color_button(ui, &mut self.style.fill, t.fill);
        ui.separator();
        ui.label(RichText::new(t.hint_eyedropper).color(pal().text_soft));
    }
}

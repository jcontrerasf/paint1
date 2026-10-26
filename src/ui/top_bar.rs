//! Top bar: name of the document, file, edit and image actions; menu and export on the right.

use eframe::egui::{self, Align, Layout, Ui, Vec2};

use crate::app::{APP, App};
use crate::document::CanvasOp;
use crate::i18n::{self, Lang, t};
use crate::icons;
use crate::theme::{self, ThemeChoice};
use crate::ui::dialogs::Dialog;

impl App {
    pub(crate) fn top_bar(&mut self, ui: &mut Ui) {
        // Right to left first: the menu and the main action stay visible in a narrow window.
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.menu_button(theme::icon(icons::MENU, false), |ui| self.main_menu(ui));
            if theme::primary_button(ui, icons::EXPORT, &format!("{}…", t().export))
                .on_hover_text("Ctrl+E")
                .clicked()
            {
                self.dialog = Some(Dialog::export(self.prefs.export.clone()));
            }
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.shrink_clip_rect(ui.max_rect());
                self.top_bar_left(ui);
            });
        });
    }

    fn main_menu(&mut self, ui: &mut Ui) {
        let t = t();
        if ui
            .button(format!("{} {}…", icons::SAVE, t.save_as))
            .clicked()
        {
            self.save(true);
        }
        if ui
            .button(format!("{} {}…", icons::INSERT_IMAGE, t.insert_image))
            .clicked()
        {
            self.pick_insert_image();
        }
        ui.menu_button(format!("{} {}", icons::RECENT, t.recent), |ui| {
            if self.prefs.recent.is_empty() {
                ui.weak(t.nothing_yet);
            }
            let mut open = None;
            for path in &self.prefs.recent {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                if ui
                    .button(format!("{} {name}", icons::FILE))
                    .on_hover_text(path.display().to_string())
                    .clicked()
                {
                    open = Some(path.clone());
                }
            }
            if let Some(path) = open {
                self.request(crate::app::Pending::Open(path));
            }
        });
        ui.separator();
        ui.menu_button(format!("{} {}", icons::LANGUAGE, t.language), |ui| {
            for lang in [Lang::Es, Lang::En] {
                let name = match lang {
                    Lang::Es => i18n::ES.language_name,
                    Lang::En => i18n::EN.language_name,
                };
                if ui.radio(self.prefs.lang == lang, name).clicked() {
                    self.prefs.lang = lang;
                    i18n::set_lang(lang);
                }
            }
        });
        ui.menu_button(format!("{} {}", icons::THEME_SYSTEM, t.theme), |ui| {
            for (choice, ico, name) in [
                (ThemeChoice::System, icons::THEME_SYSTEM, t.theme_system),
                (ThemeChoice::Light, icons::THEME_LIGHT, t.theme_light),
                (ThemeChoice::Dark, icons::THEME_DARK, t.theme_dark),
            ] {
                if ui
                    .radio(self.prefs.theme == choice, format!("{ico} {name}"))
                    .clicked()
                {
                    self.prefs.theme = choice;
                    theme::set_choice(ui.ctx(), choice);
                }
            }
        });
        ui.horizontal(|ui| {
            let zoom = ui.ctx().zoom_factor();
            ui.label("UI");
            if theme::icon_button(ui, icons::ZOOM_OUT, "−").clicked() {
                ui.ctx().set_zoom_factor((zoom - 0.1).max(0.5));
            }
            if ui.button(format!("{:.0} %", zoom * 100.0)).clicked() {
                ui.ctx().set_zoom_factor(1.0);
            }
            if theme::icon_button(ui, icons::ZOOM_IN, "+").clicked() {
                ui.ctx().set_zoom_factor((zoom + 0.1).min(3.0));
            }
        });
        ui.separator();
        if ui.button(format!("{} {}", icons::INFO, t.about)).clicked() {
            self.dialog = Some(Dialog::About);
        }
    }

    fn top_bar_left(&mut self, ui: &mut Ui) {
        let t = t();
        ui.spacing_mut().item_spacing.x = 4.0;
        if let Some(logo) = &self.logo {
            ui.add(egui::Image::new(logo).fit_to_exact_size(Vec2::splat(24.0)))
                .on_hover_text(APP);
        }
        let dirty = if self.session.dirty { " •" } else { "" };
        ui.add(egui::Label::new(theme::strong(format!("{}{dirty}", self.doc_name()))).truncate());
        ui.separator();
        if theme::icon_button(ui, icons::NEW, &format!("{}… (Ctrl+N)", t.new_doc)).clicked() {
            self.dialog = Some(Dialog::new_image(self.doc()));
        }
        if theme::icon_button(ui, icons::OPEN, &format!("{}… (Ctrl+O)", t.open)).clicked() {
            self.pick_open();
        }
        if theme::icon_button(ui, icons::SAVE, &format!("{} (Ctrl+S)", t.save)).clicked() {
            self.save(false);
        }
        ui.separator();
        let (undo, redo) = (self.session.can_undo(), self.session.can_redo());
        if theme::icon_button_if(ui, undo, icons::UNDO, &format!("{} (Ctrl+Z)", t.undo)).clicked() {
            self.undo();
        }
        if theme::icon_button_if(ui, redo, icons::REDO, &format!("{} (Ctrl+Y)", t.redo)).clicked() {
            self.redo();
        }
        ui.separator();
        let any = !self.session.selection.is_empty();
        if theme::icon_button_if(ui, any, icons::CUT, &format!("{} (Ctrl+X)", t.cut)).clicked() {
            self.cut(ui.ctx());
        }
        if theme::icon_button_if(ui, any, icons::COPY, &format!("{} (Ctrl+C)", t.copy)).clicked() {
            self.copy(ui.ctx());
        }
        if theme::icon_button(ui, icons::PASTE, &format!("{} (Ctrl+V)", t.paste)).clicked() {
            // The integration answers with a Paste event, handled like Ctrl+V.
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        }
        ui.separator();
        let target = |name: &str| format!("{name}: {}", t.on_selection_or_image);
        for (op, ico, name) in [
            (CanvasOp::RotateLeft, icons::ROTATE_LEFT, t.rotate_left),
            (CanvasOp::RotateRight, icons::ROTATE_RIGHT, t.rotate_right),
            (CanvasOp::Rotate180, icons::ROTATE_180, t.rotate_180),
            (CanvasOp::FlipH, icons::FLIP_H, t.flip_h),
            (CanvasOp::FlipV, icons::FLIP_V, t.flip_v),
        ] {
            if theme::icon_button(ui, ico, &target(name)).clicked() {
                self.transform_op(op);
            }
        }
        if theme::icon_button(
            ui,
            icons::SCALE,
            &format!("{}…: {}", t.scale, t.on_selection_or_image),
        )
        .clicked()
        {
            self.dialog = Some(Dialog::scale(self));
        }
        if theme::icon_button(ui, icons::CANVAS_SIZE, &format!("{}…", t.canvas_size)).clicked() {
            self.dialog = Some(Dialog::canvas(self.doc()));
        }
        if theme::icon_button_if(
            ui,
            any,
            icons::FIT_CANVAS,
            &format!("{} · {}", t.fit_canvas, t.fit_canvas_help),
        )
        .clicked()
        {
            self.fit_canvas_to_selection();
        }
    }
}

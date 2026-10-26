//! Dialog windows: new image, scale, canvas size, export, about, and the confirmation for
//! unsaved changes.

use eframe::egui::{self, Align2, DragValue, Grid, RichText, Ui};
use kurbo::{Affine, Vec2};

use crate::app::{APP, App, Pending, human_size};
use crate::document::{Document, Rgba};
use crate::export::{self, Format, SizeUnit};
use crate::i18n::t;
use crate::icons;
use crate::theme::{self, pal};
use crate::ui::color_button;

pub enum Dialog {
    New {
        w: u32,
        h: u32,
        bg: Rgba,
    },
    Scale {
        /// Scale the selection (else the whole image).
        selection: bool,
        base: (f64, f64),
        w: f64,
        h: f64,
        keep: bool,
    },
    Canvas {
        w: u32,
        h: u32,
        center: bool,
    },
    Export(export::Settings),
    About,
}

impl Dialog {
    pub fn new_image(doc: &Document) -> Dialog {
        Dialog::New {
            w: doc.width,
            h: doc.height,
            bg: Rgba::WHITE,
        }
    }

    pub fn scale(app: &App) -> Dialog {
        let sel = app.selection_bounds();
        let base = match sel {
            Some(b) => (b.width().max(1.0), b.height().max(1.0)),
            None => (app.doc().width as f64, app.doc().height as f64),
        };
        Dialog::Scale {
            selection: sel.is_some(),
            base,
            w: base.0.round(),
            h: base.1.round(),
            keep: true,
        }
    }

    pub fn canvas(doc: &Document) -> Dialog {
        Dialog::Canvas {
            w: doc.width,
            h: doc.height,
            center: false,
        }
    }

    pub fn export(settings: export::Settings) -> Dialog {
        Dialog::Export(settings)
    }
}

/// What the buttons of a dialog asked for.
enum Outcome {
    Keep,
    Close,
}

fn buttons(ui: &mut Ui, ok_icon: &str, ok: &str) -> Option<bool> {
    let mut r = None;
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if theme::primary_button(ui, ok_icon, ok).clicked() {
            r = Some(true);
        }
        if ui.button(t().cancel).clicked() {
            r = Some(false);
        }
    });
    if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        r = Some(false);
    }
    r
}

impl App {
    pub(crate) fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(mut dialog) = self.dialog.take() else {
            return;
        };
        let t = t();
        let title = match &dialog {
            Dialog::New { .. } => t.new_title,
            Dialog::Scale {
                selection: true, ..
            } => t.scale_selection_title,
            Dialog::Scale { .. } => t.scale_title,
            Dialog::Canvas { .. } => t.canvas_title,
            Dialog::Export(_) => t.export_title,
            Dialog::About => t.about,
        };
        let mut open = true;
        let mut outcome = Outcome::Keep;
        egui::Window::new(title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                outcome = match &mut dialog {
                    Dialog::New { w, h, bg } => self.new_dialog(ui, w, h, bg),
                    Dialog::Scale {
                        selection,
                        base,
                        w,
                        h,
                        keep,
                    } => self.scale_dialog(ui, *selection, *base, w, h, keep),
                    Dialog::Canvas { w, h, center } => self.canvas_dialog(ui, w, h, center),
                    Dialog::Export(settings) => self.export_dialog(ui, settings),
                    Dialog::About => about(ui),
                };
            });
        if open && matches!(outcome, Outcome::Keep) {
            self.dialog = Some(dialog);
        }
    }

    fn new_dialog(&mut self, ui: &mut Ui, w: &mut u32, h: &mut u32, bg: &mut Rgba) -> Outcome {
        let t = t();
        Grid::new("new")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(t.width);
                ui.add(DragValue::new(w).range(1..=20000).suffix(" px"));
                ui.end_row();
                ui.label(t.height);
                ui.add(DragValue::new(h).range(1..=20000).suffix(" px"));
                ui.end_row();
                ui.label(t.background);
                ui.horizontal(|ui| {
                    color_button(ui, bg, t.background);
                    let mut transparent = bg.0[3] == 0;
                    if ui.checkbox(&mut transparent, t.transparent).changed() {
                        *bg = bg.with_alpha(if transparent { 0 } else { 255 });
                    }
                });
                ui.end_row();
            });
        ui.horizontal_wrapped(|ui| {
            for (pw, ph) in [
                (1280, 720),
                (1920, 1080),
                (1080, 1080),
                (1080, 1350),
                (1080, 1920),
                (800, 600),
            ] {
                if ui.small_button(format!("{pw}×{ph}")).clicked() {
                    (*w, *h) = (pw, ph);
                }
            }
        });
        match buttons(ui, icons::NEW, t.create) {
            Some(true) => {
                self.request(Pending::New(*w, *h, *bg));
                Outcome::Close
            }
            Some(false) => Outcome::Close,
            None => Outcome::Keep,
        }
    }

    fn scale_dialog(
        &mut self,
        ui: &mut Ui,
        selection: bool,
        base: (f64, f64),
        w: &mut f64,
        h: &mut f64,
        keep: &mut bool,
    ) -> Outcome {
        let t = t();
        Grid::new("scale")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(t.percent);
                let mut pct = *w / base.0 * 100.0;
                if ui
                    .add(
                        DragValue::new(&mut pct)
                            .range(1.0..=1000.0)
                            .speed(0.5)
                            .suffix(" %")
                            .max_decimals(1),
                    )
                    .changed()
                {
                    *w = base.0 * pct / 100.0;
                    if *keep {
                        *h = base.1 * pct / 100.0;
                    }
                }
                ui.end_row();
                ui.label(t.width);
                if ui
                    .add(
                        DragValue::new(w)
                            .range(1.0..=50000.0)
                            .suffix(" px")
                            .max_decimals(0),
                    )
                    .changed()
                    && *keep
                {
                    *h = *w * base.1 / base.0;
                }
                ui.end_row();
                ui.label(t.height);
                if ui
                    .add(
                        DragValue::new(h)
                            .range(1.0..=50000.0)
                            .suffix(" px")
                            .max_decimals(0),
                    )
                    .changed()
                    && *keep
                {
                    *w = *h * base.0 / base.1;
                }
                ui.end_row();
                ui.label("");
                ui.checkbox(keep, t.keep_aspect);
                ui.end_row();
            });
        match buttons(ui, icons::SCALE, t.apply) {
            Some(true) => {
                let (sx, sy) = (*w / base.0, *h / base.1);
                self.settle();
                if selection {
                    let ids = self.session.selection.clone();
                    if let Some(b) = self.selection_bounds() {
                        let o = b.origin().to_vec2();
                        let m = Affine::translate(o)
                            * Affine::scale_non_uniform(sx, sy)
                            * Affine::translate(-o);
                        self.session.edit(|d| d.transform_objects(&ids, m));
                    }
                } else {
                    self.session.edit(|d| d.scale(sx, sy));
                    self.view.fit_pending = true;
                }
                Outcome::Close
            }
            Some(false) => Outcome::Close,
            None => Outcome::Keep,
        }
    }

    fn canvas_dialog(
        &mut self,
        ui: &mut Ui,
        w: &mut u32,
        h: &mut u32,
        center: &mut bool,
    ) -> Outcome {
        let t = t();
        Grid::new("canvas")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(t.width);
                ui.add(DragValue::new(w).range(1..=20000).suffix(" px"));
                ui.end_row();
                ui.label(t.height);
                ui.add(DragValue::new(h).range(1..=20000).suffix(" px"));
                ui.end_row();
                ui.label("");
                ui.checkbox(center, t.align_center);
                ui.end_row();
            });
        match buttons(ui, icons::CANVAS_SIZE, t.apply) {
            Some(true) => {
                let (nw, nh, center) = (*w, *h, *center);
                self.settle();
                self.session.edit(|d| {
                    if center {
                        let off = Vec2::new(
                            ((nw as f64 - d.width as f64) / 2.0).round(),
                            ((nh as f64 - d.height as f64) / 2.0).round(),
                        );
                        for o in &mut d.objects {
                            o.apply(Affine::translate(off));
                        }
                    }
                    d.width = nw;
                    d.height = nh;
                });
                self.view.fit_pending = true;
                if self.tool == crate::tools::Tool::Crop {
                    self.reset_crop();
                }
                Outcome::Close
            }
            Some(false) => Outcome::Close,
            None => Outcome::Keep,
        }
    }

    fn export_dialog(&mut self, ui: &mut Ui, s: &mut export::Settings) -> Outcome {
        let t = t();
        let (dw, dh) = (self.doc().width as f64, self.doc().height as f64);
        Grid::new("export")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(t.format);
                egui::ComboBox::from_id_salt("format")
                    .selected_text(s.format.label())
                    .show_ui(ui, |ui| {
                        for f in Format::ALL {
                            ui.selectable_value(&mut s.format, f, f.label());
                        }
                    });
                ui.end_row();
                if s.format.lossy() {
                    ui.label(t.quality);
                    ui.add(egui::Slider::new(&mut s.quality, 1..=100));
                    ui.end_row();
                }
                ui.label(t.scale);
                ui.add(
                    DragValue::new(&mut s.scale)
                        .range(1.0..=1000.0)
                        .speed(0.5)
                        .suffix(" %")
                        .max_decimals(1),
                );
                ui.end_row();
                ui.label(t.output_size);
                let k = s.scale / 100.0;
                ui.label(
                    RichText::new(format!(
                        "{:.0} × {:.0} px",
                        (dw * k).round().max(1.0),
                        (dh * k).round().max(1.0)
                    ))
                    .color(pal().text_soft),
                );
                ui.end_row();
                ui.label(t.max_size);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut s.limit, "");
                    ui.add_enabled(
                        s.limit,
                        DragValue::new(&mut s.limit_value)
                            .range(1.0..=100000.0)
                            .speed(1.0)
                            .max_decimals(1),
                    );
                    ui.add_enabled_ui(s.limit, |ui| {
                        egui::ComboBox::from_id_salt("unit")
                            .width(60.0)
                            .selected_text(match s.limit_unit {
                                SizeUnit::Kb => "KB",
                                SizeUnit::Mb => "MB",
                            })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut s.limit_unit, SizeUnit::Kb, "KB");
                                ui.selectable_value(&mut s.limit_unit, SizeUnit::Mb, "MB");
                            });
                    });
                });
                ui.end_row();
            });
        if s.limit {
            let max = s.max_bytes().unwrap_or(0);
            ui.label(
                RichText::new(format!("≤ {} · {}", human_size(max), t.max_size_help))
                    .small()
                    .color(pal().text_soft),
            );
        }
        if !s.format.alpha() && self.doc().background.0[3] < 255 {
            ui.label(
                RichText::new(format!("{} → {}", t.transparent, t.background))
                    .small()
                    .color(pal().warning),
            );
        }
        match buttons(ui, icons::EXPORT, &format!("{}…", t.export)) {
            Some(true) => {
                self.start_export(s.clone());
                Outcome::Close
            }
            Some(false) => Outcome::Close,
            None => Outcome::Keep,
        }
    }

    pub(crate) fn confirm_dialog(&mut self, ctx: &egui::Context) {
        if self.pending.is_none() {
            return;
        }
        let t = t();
        let mut choice = None;
        egui::Window::new(t.unsaved_title)
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, [0.0, 0.0])
            .show(ctx, |ui| {
                ui.label(t.unsaved_body);
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if theme::primary_button(ui, icons::SAVE, t.save).clicked() {
                        choice = Some(0);
                    }
                    if ui.button(t.discard).clicked() {
                        choice = Some(1);
                    }
                    if ui.button(t.cancel).clicked() {
                        choice = Some(2);
                    }
                });
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    choice = Some(2);
                }
            });
        match choice {
            Some(0) => {
                if self.save(false)
                    && let Some(p) = self.pending.take()
                {
                    self.run_pending(p);
                }
            }
            Some(1) => {
                if let Some(p) = self.pending.take() {
                    self.run_pending(p);
                }
            }
            Some(_) => self.pending = None,
            None => {}
        }
        if choice.is_some() {
            ctx.request_repaint();
        }
    }
}

fn about(ui: &mut Ui) -> Outcome {
    ui.label(theme::strong(format!("{APP} {}", env!("CARGO_PKG_VERSION"))).size(18.0));
    ui.label(t().about_text);
    ui.add_space(4.0);
    ui.label(
        RichText::new("Inter (OFL) · Phosphor Icons (MIT) · egui · tiny-skia · libwebp")
            .small()
            .color(pal().text_soft),
    );
    ui.add_space(6.0);
    if ui.button(t().ok).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
        Outcome::Close
    } else {
        Outcome::Keep
    }
}

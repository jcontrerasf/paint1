//! Properties of the selection (or of the canvas when nothing is selected).

use eframe::egui::{self, DragValue, Grid, RichText, Ui};
use kurbo::{Affine, Vec2};

use crate::app::App;
use crate::document::{Document, Kind, Object, StrokeMode, TextAlign};
use crate::i18n::{fill, t};
use crate::icons;
use crate::theme::{self, pal};
use crate::ui::color_button;
use crate::ui::dialogs::Dialog;

impl App {
    pub(crate) fn properties(&mut self, ui: &mut Ui) {
        let t = t();
        theme::section_title(ui, t.properties);
        let ids = self.selection_in_order();
        match ids.as_slice() {
            [] => self.canvas_properties(ui),
            [id] => {
                if let Some(o) = self.doc().get(*id).cloned() {
                    self.object_properties(ui, &o);
                }
            }
            _ => {
                ui.label(fill(t.n_selected, &[&ids.len()]));
                if let Some(first) = self.doc().get(ids[0]).cloned() {
                    self.style_properties(ui, &first, &ids);
                }
            }
        }
        if !ids.is_empty() {
            ui.separator();
            if ui
                .button(format!("{} {}", icons::FLATTEN, t.flatten))
                .on_hover_text(t.flatten_help)
                .clicked()
            {
                self.flatten_selection();
            }
        }
    }

    fn canvas_properties(&mut self, ui: &mut Ui) {
        let t = t();
        ui.label(RichText::new(t.no_selection).color(pal().text_soft));
        ui.add_space(4.0);
        let d = self.doc();
        let (w, h, mut bg) = (d.width, d.height, d.background);
        Grid::new("canvas_props")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(t.canvas);
                ui.horizontal(|ui| {
                    ui.label(format!("{w} × {h} px"));
                    if theme::icon_button(ui, icons::CANVAS_SIZE, &format!("{}…", t.canvas_size))
                        .clicked()
                    {
                        self.dialog = Some(Dialog::canvas(self.doc()));
                    }
                });
                ui.end_row();
                ui.label(t.background);
                ui.horizontal(|ui| {
                    let mut transparent = bg.0[3] == 0;
                    let mut changed = color_button(ui, &mut bg, t.background);
                    if ui.checkbox(&mut transparent, t.transparent).changed() {
                        bg = if transparent {
                            bg.with_alpha(0)
                        } else {
                            bg.with_alpha(255)
                        };
                        changed = true;
                    }
                    if changed {
                        self.session
                            .edit_grouped("prop:background", |d| d.background = bg);
                    }
                });
                ui.end_row();
            });
    }

    fn object_properties(&mut self, ui: &mut Ui, o: &Object) {
        let t = t();
        let id = o.id;
        let b = o.shape_bounds();
        Grid::new("transform_props")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                ui.label(t.position);
                ui.horizontal(|ui| {
                    let (mut x, mut y) = (b.x0, b.y0);
                    let cx = ui
                        .add(
                            DragValue::new(&mut x)
                                .speed(1.0)
                                .prefix("x ")
                                .max_decimals(1),
                        )
                        .changed();
                    let cy = ui
                        .add(
                            DragValue::new(&mut y)
                                .speed(1.0)
                                .prefix("y ")
                                .max_decimals(1),
                        )
                        .changed();
                    if cx || cy {
                        let m = Affine::translate(Vec2::new(x - b.x0, y - b.y0));
                        self.session
                            .edit_grouped("prop:pos", |d| d.transform_objects(&[id], m));
                    }
                });
                ui.end_row();
                ui.label(t.size);
                ui.horizontal(|ui| {
                    let (mut w, mut h) = (b.width(), b.height());
                    let cw = ui
                        .add_enabled(
                            b.width() > 1e-6,
                            DragValue::new(&mut w)
                                .speed(1.0)
                                .range(1.0..=100000.0)
                                .prefix("w ")
                                .max_decimals(1),
                        )
                        .changed();
                    let ch = ui
                        .add_enabled(
                            b.height() > 1e-6,
                            DragValue::new(&mut h)
                                .speed(1.0)
                                .range(1.0..=100000.0)
                                .prefix("h ")
                                .max_decimals(1),
                        )
                        .changed();
                    if cw || ch {
                        let sx = if b.width() > 1e-6 { w / b.width() } else { 1.0 };
                        let sy = if b.height() > 1e-6 {
                            h / b.height()
                        } else {
                            1.0
                        };
                        let o0 = b.origin().to_vec2();
                        let m = Affine::translate(o0)
                            * Affine::scale_non_uniform(sx, sy)
                            * Affine::translate(-o0);
                        self.session
                            .edit_grouped("prop:size", |d| d.transform_objects(&[id], m));
                    }
                });
                ui.end_row();
                ui.label(t.rotation);
                let mut deg = o.rotation_deg();
                if ui
                    .add(
                        DragValue::new(&mut deg)
                            .speed(0.5)
                            .suffix("°")
                            .max_decimals(1),
                    )
                    .changed()
                {
                    let delta = (deg - o.rotation_deg()).to_radians();
                    let m = Affine::rotate_about(delta, b.center());
                    self.session
                        .edit_grouped("prop:rotation", |d| d.transform_objects(&[id], m));
                }
                ui.end_row();
            });
        ui.separator();
        self.style_properties(ui, o, &[id]);
        ui.separator();
        self.kind_properties(ui, o);
    }

    /// Color, width, fill and opacity, applied to all of `ids`.
    fn style_properties(&mut self, ui: &mut Ui, o: &Object, ids: &[u64]) {
        let t = t();
        let ids = ids.to_vec();
        let eraser = matches!(
            o.kind,
            Kind::Stroke {
                mode: StrokeMode::Eraser,
                ..
            }
        );
        let image = matches!(o.kind, Kind::Image { .. });
        let mut st = o.style.clone();
        let mut changed = None;
        Grid::new("style_props")
            .num_columns(2)
            .spacing([12.0, 8.0])
            .show(ui, |ui| {
                if !eraser && !image {
                    let label = if matches!(o.kind, Kind::Text { .. }) {
                        t.text_color
                    } else {
                        t.stroke
                    };
                    ui.label(label);
                    if color_button(ui, &mut st.stroke, label) {
                        changed = Some("stroke");
                    }
                    ui.end_row();
                }
                if o.has_outline() {
                    ui.label(t.width);
                    if ui
                        .add(
                            DragValue::new(&mut st.width)
                                .range(0.0..=500.0)
                                .speed(0.2)
                                .suffix(" px")
                                .max_decimals(1),
                        )
                        .changed()
                    {
                        changed = Some("width");
                    }
                    ui.end_row();
                }
                if o.can_fill() {
                    let label = if matches!(o.kind, Kind::Text { .. }) {
                        t.text_box
                    } else {
                        t.fill
                    };
                    ui.label(label);
                    ui.horizontal(|ui| {
                        let mut on = st.fill.is_some();
                        let mut color = st.fill.unwrap_or(self.style.fill);
                        if ui.checkbox(&mut on, "").changed() {
                            changed = Some("fill");
                        }
                        if color_button(ui, &mut color, label) {
                            on = true;
                            changed = Some("fill");
                        }
                        st.fill = on.then_some(color);
                    });
                    ui.end_row();
                }
                if !eraser {
                    ui.label(t.opacity);
                    let mut pct = st.opacity * 100.0;
                    if ui
                        .add(
                            DragValue::new(&mut pct)
                                .range(0.0..=100.0)
                                .speed(0.5)
                                .suffix(" %")
                                .max_decimals(0),
                        )
                        .changed()
                    {
                        st.opacity = pct / 100.0;
                        changed = Some("opacity");
                    }
                    ui.end_row();
                }
            });
        if let Some(field) = changed {
            self.session.edit_grouped(&format!("prop:{field}"), |d| {
                for x in d.objects.iter_mut().filter(|x| ids.contains(&x.id)) {
                    match field {
                        "stroke" => x.style.stroke = st.stroke,
                        "width" => x.style.width = st.width,
                        "fill" if x.can_fill() => x.style.fill = st.fill,
                        "opacity" => x.style.opacity = st.opacity,
                        _ => {}
                    }
                }
            });
        }
    }

    fn kind_properties(&mut self, ui: &mut Ui, o: &Object) {
        let t = t();
        let id = o.id;
        let edit = |app: &mut App, key: &str, f: &dyn Fn(&mut Kind)| {
            app.session.edit_grouped(key, |d: &mut Document| {
                if let Some(x) = d.get_mut(id) {
                    f(&mut x.kind);
                }
            });
        };
        match &o.kind {
            Kind::Text {
                content,
                size,
                bold,
                align,
            } => {
                ui.label(t.content);
                let mut text = content.clone();
                if ui
                    .add(
                        egui::TextEdit::multiline(&mut text)
                            .desired_rows(3)
                            .desired_width(f32::INFINITY),
                    )
                    .changed()
                {
                    edit(self, "prop:content", &|k| {
                        if let Kind::Text { content, .. } = k {
                            content.clone_from(&text);
                        }
                    });
                }
                ui.horizontal(|ui| {
                    ui.label(t.font_size);
                    let mut s = *size;
                    if ui
                        .add(
                            DragValue::new(&mut s)
                                .range(4.0..=1000.0)
                                .speed(0.5)
                                .suffix(" px")
                                .max_decimals(1),
                        )
                        .changed()
                    {
                        edit(self, "prop:font_size", &|k| {
                            if let Kind::Text { size, .. } = k {
                                *size = s;
                            }
                        });
                    }
                    if theme::toggle(ui, *bold, icons::BOLD, t.bold).clicked() {
                        let b = !*bold;
                        edit(self, "prop:bold", &|k| {
                            if let Kind::Text { bold, .. } = k {
                                *bold = b;
                            }
                        });
                    }
                    for (al, ico, help) in [
                        (TextAlign::Left, icons::ALIGN_LEFT, t.align_left),
                        (TextAlign::Center, icons::ALIGN_CENTER, t.align_center),
                        (TextAlign::Right, icons::ALIGN_RIGHT, t.align_right),
                    ] {
                        if theme::toggle(ui, *align == al, ico, help).clicked() {
                            edit(self, "prop:align", &|k| {
                                if let Kind::Text { align, .. } = k {
                                    *align = al;
                                }
                            });
                        }
                    }
                });
            }
            Kind::Rect { radius, .. } => {
                ui.horizontal(|ui| {
                    ui.label(t.radius);
                    let mut r = *radius;
                    if ui
                        .add(
                            DragValue::new(&mut r)
                                .range(0.0..=5000.0)
                                .speed(0.5)
                                .suffix(" px")
                                .max_decimals(1),
                        )
                        .changed()
                    {
                        edit(self, "prop:radius", &|k| {
                            if let Kind::Rect { radius, .. } = k {
                                *radius = r;
                            }
                        });
                    }
                });
            }
            Kind::Arrow { head, .. } => {
                ui.horizontal(|ui| {
                    ui.label(t.head);
                    let mut h = *head;
                    if ui
                        .add(
                            DragValue::new(&mut h)
                                .range(2.0..=1000.0)
                                .speed(0.5)
                                .suffix(" px")
                                .max_decimals(1),
                        )
                        .changed()
                    {
                        edit(self, "prop:head", &|k| {
                            if let Kind::Arrow { head, .. } = k {
                                *head = h;
                            }
                        });
                    }
                });
            }
            Kind::RegularPolygon { sides, .. } => {
                ui.horizontal(|ui| {
                    ui.label(t.sides);
                    let mut n = *sides;
                    if ui
                        .add(DragValue::new(&mut n).range(crate::document::SIDES))
                        .changed()
                    {
                        edit(self, "prop:sides", &|k| {
                            if let Kind::RegularPolygon { sides, .. } = k {
                                *sides = n;
                            }
                        });
                    }
                });
            }
            Kind::Polygon { closed, points } => {
                let mut c = *closed;
                if points.len() >= 3 && ui.checkbox(&mut c, t.closed).changed() {
                    edit(self, "prop:closed", &|k| {
                        if let Kind::Polygon { closed, .. } = k {
                            *closed = c;
                        }
                    });
                }
            }
            Kind::Image { asset, .. } => {
                let natural = self
                    .doc()
                    .assets
                    .get(asset)
                    .map(|a| (a.width(), a.height()));
                if let Some((w, h)) = natural {
                    ui.label(RichText::new(format!("{w} × {h} px")).color(pal().text_soft));
                    if ui.button(t.original_size).clicked() {
                        self.settle();
                        self.session.edit(|d| {
                            if let Some(x) = d.get_mut(id)
                                && let Kind::Image { size, .. } = &mut x.kind
                            {
                                *size = kurbo::Size::new(w as f64, h as f64);
                                // Keep only the position and the rotation.
                                let [a, b, ..] = x.transform.as_coeffs();
                                let angle = b.atan2(a);
                                x.transform = Affine::translate(x.transform.translation())
                                    * Affine::rotate(angle);
                            }
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

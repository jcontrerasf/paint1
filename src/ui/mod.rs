//! Panels and bars around the canvas, and the widgets they share.

pub mod context_bar;
pub mod dialogs;
pub mod objects_panel;
pub mod properties;
pub mod status_bar;
pub mod tools_bar;
pub mod top_bar;

use eframe::egui::{
    self, Align, Color32, Id, Layout, Popup, PopupCloseBehavior, Response, RichText, Sense, Stroke,
    Ui, UiBuilder, Vec2, color_picker, ecolor::Hsva,
};

use crate::document::{Kind, Object, Rgba, StrokeMode};
use crate::i18n::t;
use crate::icons;
use crate::theme::{self, pal};

/// Quick colors of the context bar.
pub const PALETTE: [Rgba; 16] = [
    Rgba([0x00, 0x00, 0x00, 255]),
    Rgba([0x5f, 0x63, 0x6b, 255]),
    Rgba([0xb4, 0xb8, 0xc0, 255]),
    Rgba([0xff, 0xff, 0xff, 255]),
    Rgba([0xe0, 0x2f, 0x2f, 255]),
    Rgba([0xf2, 0x7a, 0x1a, 255]),
    Rgba([0xff, 0xd0, 0x1f, 255]),
    Rgba([0x3c, 0xb0, 0x43, 255]),
    Rgba([0x10, 0x8a, 0x7a, 255]),
    Rgba([0x2f, 0xa8, 0xe8, 255]),
    Rgba([0x2f, 0x6b, 0xf0, 255]),
    Rgba([0x6b, 0x3f, 0xd8, 255]),
    Rgba([0xd8, 0x3f, 0xa8, 255]),
    Rgba([0x8a, 0x55, 0x2e, 255]),
    Rgba([0xff, 0xb3, 0xc1, 255]),
    Rgba([0xb8, 0xe9, 0x86, 255]),
];

pub fn color32(c: Rgba) -> Color32 {
    let [r, g, b, a] = c.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

/// Where the color button with an open picker announces itself to the canvas: its popup id
/// and the pass it was last shown in.
const PICK_TARGET: &str = "pick_target";
/// A color taken on the canvas for the button with that popup id.
const PICKED: &str = "picked_color";

/// Popup id of the color picker that is open now, if any: while it is, clicking the canvas
/// takes a color for it instead of using the tool.
pub fn pick_target(ctx: &egui::Context) -> Option<Id> {
    let (id, pass) = ctx.data(|d| d.get_temp::<(Id, u64)>(Id::unique(PICK_TARGET)))?;
    (pass + 1 >= ctx.cumulative_pass_nr() && Popup::is_id_open(ctx, id)).then_some(id)
}

/// Hands a color taken on the canvas to the picker `target`.
pub fn send_picked(ctx: &egui::Context, target: Id, color: Rgba) {
    ctx.data_mut(|d| d.insert_temp(Id::unique(PICKED), (target, color)));
    ctx.request_repaint();
}

/// Swatch that opens a color picker (with transparency). While it is open, clicking the
/// canvas takes the color under the pointer; Ok or Esc closes it. True if the color changed.
pub fn color_button(ui: &mut Ui, color: &mut Rgba, help: &str) -> bool {
    let popup_id = ui.auto_id_with("color_popup");
    let ctx = ui.ctx().clone();
    let open = Popup::is_id_open(&ctx, popup_id);
    let resp = color_swatch(ui, *color, open).on_hover_text(help);
    let mut new = *color;
    let picked = ctx.data_mut(|d| {
        let key = Id::unique(PICKED);
        match d.get_temp::<(Id, Rgba)>(key) {
            Some((id, c)) if id == popup_id => {
                d.remove::<(Id, Rgba)>(key);
                Some(c)
            }
            _ => None,
        }
    });
    if let Some(c) = picked {
        new = c;
    }
    let mut hsva = Hsva::from_srgba_unmultiplied(new.0);
    let mut confirmed = false;
    Popup::menu(&resp)
        .id(popup_id)
        .close_behavior(PopupCloseBehavior::IgnoreClicks)
        .show(|ui| {
            // Read the pass first: `ctx` must not be locked twice.
            let pass = ctx.cumulative_pass_nr();
            ctx.data_mut(|d| d.insert_temp(Id::unique(PICK_TARGET), (popup_id, pass)));
            ui.spacing_mut().slider_width = 275.0;
            if color_picker::color_picker_hsva_2d(ui, &mut hsva, color_picker::Alpha::OnlyBlend) {
                new = Rgba(hsva.to_srgba_unmultiplied());
            }
            ui.label(
                RichText::new(format!("{} {}", icons::EYEDROPPER, t().pick_hint))
                    .color(pal().text_soft),
            );
            ui.horizontal(|ui| confirmed = ui.button(t().ok).clicked());
        });
    if confirmed {
        Popup::close_id(&ctx, popup_id);
    }
    if new != *color {
        *color = new;
        true
    } else {
        false
    }
}

/// The button of `color_button`: the color, half of it opaque when it is translucent.
fn color_swatch(ui: &mut Ui, color: Rgba, open: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(ui.spacing().interact_size, Sense::click());
    if ui.is_rect_visible(rect) {
        let p = pal();
        let visuals = if open {
            &ui.visuals().widgets.open
        } else {
            ui.style().interact(&resp)
        };
        let painter = ui.painter();
        let inner = rect.shrink(1.0);
        let c = color32(color);
        if color.0[3] < 255 {
            painter.rect_filled(inner, 0.0, p.checker.0);
            let half = inner.height() / 2.0;
            let mut x = inner.left();
            let mut i = 0;
            while x < inner.right() {
                let top = if i % 2 == 0 {
                    inner.top()
                } else {
                    inner.top() + half
                };
                let cell = egui::Rect::from_min_size(egui::pos2(x, top), Vec2::splat(half))
                    .intersect(inner);
                painter.rect_filled(cell, 0.0, p.checker.1);
                x += half;
                i += 1;
            }
            let (left, right) = inner.split_left_right_at_fraction(0.5);
            painter.rect_filled(left, 0.0, c);
            painter.rect_filled(right, 0.0, c.to_opaque());
        } else {
            painter.rect_filled(inner, 0.0, c);
        }
        painter.rect_stroke(
            rect,
            visuals.corner_radius,
            Stroke::new(1.0, if open { p.text } else { p.border }),
            egui::StrokeKind::Inside,
        );
    }
    resp
}

/// Row of quick colors: what was clicked, and whether with the secondary button.
pub fn palette_row(ui: &mut Ui) -> Option<(Rgba, bool)> {
    let mut picked = None;
    ui.spacing_mut().item_spacing.x = 2.0;
    for c in PALETTE {
        let r = theme::swatch(ui, color32(c), 16.0).on_hover_text(t().palette_help);
        if r.clicked() {
            picked = Some((c, false));
        }
        if r.secondary_clicked() {
            picked = Some((c, true));
        }
    }
    ui.spacing_mut().item_spacing.x = 8.0;
    picked
}

pub fn kind_icon(o: &Object) -> &'static str {
    match &o.kind {
        Kind::Stroke { mode, .. } => match mode {
            StrokeMode::Pencil => icons::PENCIL,
            StrokeMode::Highlighter => icons::HIGHLIGHTER,
            StrokeMode::Eraser => icons::ERASER,
        },
        Kind::Line { .. } => icons::LINE,
        Kind::Arrow { .. } => icons::ARROW,
        Kind::Rect { radius, .. } if *radius > 0.0 => icons::ROUND_RECT,
        Kind::Rect { .. } => icons::RECT,
        Kind::Ellipse { .. } => icons::ELLIPSE,
        Kind::Polygon { .. } => icons::POLYGON,
        Kind::RegularPolygon { .. } => icons::REGULAR_POLYGON,
        Kind::Text { .. } => icons::TEXT,
        Kind::Image { .. } => icons::IMAGE,
    }
}

pub fn kind_name(o: &Object) -> &'static str {
    let t = t();
    match &o.kind {
        Kind::Stroke { mode, .. } => match mode {
            StrokeMode::Pencil => t.kind_stroke,
            StrokeMode::Highlighter => t.kind_highlight,
            StrokeMode::Eraser => t.kind_eraser,
        },
        Kind::Line { .. } => t.kind_line,
        Kind::Arrow { .. } => t.kind_arrow,
        Kind::Rect { .. } => t.kind_rect,
        Kind::Ellipse { .. } => t.kind_ellipse,
        Kind::Polygon { .. } => t.kind_polygon,
        Kind::RegularPolygon { .. } => t.kind_regular_polygon,
        Kind::Text { .. } => t.kind_text,
        Kind::Image { .. } => t.kind_image,
    }
}

/// Name shown in the list: the one given, the first line of a text, or the kind.
pub fn display_name(o: &Object) -> String {
    if !o.name.is_empty() {
        return o.name.clone();
    }
    if let Kind::Text { content, .. } = &o.kind
        && let Some(line) = content.lines().find(|l| !l.trim().is_empty())
    {
        let mut s: String = line.trim().chars().take(40).collect();
        if line.trim().chars().count() > 40 {
            s.push('…');
        }
        return s;
    }
    kind_name(o).to_string()
}

/// Height of a list row.
const ROW: f32 = 30.0;

/// A row that is picked with a click and dragged to reorder (`i` is the drag payload).
/// `left` draws the left part, `right` the right part right to left. Buttons inside take
/// priority over the click of the row.
pub fn row(
    ui: &mut Ui,
    i: usize,
    selected: bool,
    left: impl FnOnce(&mut Ui),
    right: impl FnOnce(&mut Ui),
) -> Response {
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), ROW),
        Sense::click_and_drag(),
    );
    let bg = if selected {
        Some(ui.visuals().selection.bg_fill)
    } else if resp.hovered() {
        Some(ui.visuals().widgets.hovered.weak_bg_fill)
    } else {
        None
    };
    if let Some(c) = bg {
        ui.painter().rect_filled(rect, 6.0, c);
    }
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect.shrink2(Vec2::new(6.0, 0.0)))
            .layout(Layout::left_to_right(Align::Center)),
    );
    child.spacing_mut().item_spacing.x = 6.0;
    left(&mut child);
    child.with_layout(Layout::right_to_left(Align::Center), right);
    if resp.drag_started() {
        resp.dnd_set_drag_payload(i);
    }
    if resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    resp
}

/// While a row is dragged over `resp` (row `i`): marks where it would land and returns the
/// gap (0..=n, before the row of the same number).
pub fn drop_gap(ui: &Ui, resp: &Response, i: usize) -> Option<usize> {
    resp.dnd_hover_payload::<usize>()?;
    let pos = ui.ctx().pointer_interact_pos()?;
    let r = resp.rect;
    let gap = if pos.y < r.center().y { i } else { i + 1 };
    let y = if gap == i { r.top() } else { r.bottom() };
    ui.painter()
        .hline(r.x_range(), y, Stroke::new(2.0, pal().accent));
    Some(gap)
}

/// If a row was released over gap `gap`: from where to where it moves.
pub fn dropped(ui: &Ui, gap: Option<usize>) -> Option<(usize, usize)> {
    let gap = gap?;
    if !ui.input(|i| i.pointer.any_released()) {
        return None;
    }
    let from = *egui::DragAndDrop::take_payload::<usize>(ui.ctx())?;
    let to = if gap > from { gap - 1 } else { gap };
    (to != from).then_some((from, to))
}

#[cfg(test)]
mod tests {
    #[test]
    fn drop_target_skips_the_dragged_row() {
        let target = |from: usize, gap: usize| if gap > from { gap - 1 } else { gap };
        assert_eq!(target(0, 1), 0);
        assert_eq!(target(0, 3), 2);
        assert_eq!(target(3, 0), 0);
    }
}

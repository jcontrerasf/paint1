//! Panels and bars around the canvas, and the widgets they share.

pub mod context_bar;
pub mod dialogs;
pub mod objects_panel;
pub mod properties;
pub mod status_bar;
pub mod tools_bar;
pub mod top_bar;

use eframe::egui::{
    self, Align, Color32, Layout, Response, Sense, Stroke, Ui, UiBuilder, Vec2, color_picker,
    ecolor::Hsva,
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

/// Swatch that opens a color picker (with transparency). True if the color changed.
pub fn color_button(ui: &mut Ui, color: &mut Rgba, help: &str) -> bool {
    let mut hsva = Hsva::from_srgba_unmultiplied(color.0);
    let resp = color_picker::color_edit_button_hsva(ui, &mut hsva, color_picker::Alpha::OnlyBlend)
        .on_hover_text(help);
    let new = Rgba(hsva.to_srgba_unmultiplied());
    if resp.changed() && new != *color {
        *color = new;
        true
    } else {
        false
    }
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

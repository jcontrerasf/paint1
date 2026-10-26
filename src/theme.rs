//! Look of the application: neutral light and dark palettes with one blue accent, Inter for
//! text and the Phosphor icon font.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Response,
    RichText, Shadow, Stroke, TextStyle, Theme, Ui, Vec2,
};
use serde::{Deserialize, Serialize};

/// Family with the filled icons (for what is active).
pub const FILL: &str = "fill";
/// Family with Inter SemiBold (titles).
pub const STRONG: &str = "strong";

/// Size of the icons in the toolbars.
const ICON_SIZE: f32 = 18.0;

/// Colors of one theme.
pub struct Palette {
    /// Background of the panels and bars.
    pub panel: Color32,
    /// Windows, text fields and "sunken" backgrounds.
    pub window: Color32,
    /// Area around the document.
    pub backdrop: Color32,
    pub button: Color32,
    pub button_hover: Color32,
    pub button_active: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_soft: Color32,
    /// Accent: active tool, selection, focus, main action.
    pub accent: Color32,
    pub accent_strong: Color32,
    pub accent_soft: Color32,
    pub warning: Color32,
    pub error: Color32,
    /// The two squares of the transparency checkerboard.
    pub checker: (Color32, Color32),
}

pub const LIGHT: Palette = Palette {
    panel: Color32::from_rgb(0xf4, 0xf4, 0xf6),
    window: Color32::from_rgb(0xff, 0xff, 0xff),
    backdrop: Color32::from_rgb(0xe3, 0xe4, 0xe8),
    button: Color32::from_rgb(0xe7, 0xe8, 0xec),
    button_hover: Color32::from_rgb(0xdc, 0xdd, 0xe3),
    button_active: Color32::from_rgb(0xcd, 0xcf, 0xd8),
    border: Color32::from_rgb(0xd3, 0xd5, 0xdc),
    text: Color32::from_rgb(0x1f, 0x21, 0x26),
    text_soft: Color32::from_rgb(0x6a, 0x6d, 0x78),
    accent: Color32::from_rgb(0x2f, 0x6b, 0xf0),
    accent_strong: Color32::from_rgb(0x1f, 0x52, 0xc4),
    accent_soft: Color32::from_rgb(0xd8, 0xe4, 0xff),
    warning: Color32::from_rgb(0xa8, 0x6b, 0x00),
    error: Color32::from_rgb(0xd0, 0x2e, 0x2e),
    checker: (
        Color32::from_rgb(0xff, 0xff, 0xff),
        Color32::from_rgb(0xdd, 0xdd, 0xdd),
    ),
};

pub const DARK: Palette = Palette {
    panel: Color32::from_rgb(0x1f, 0x20, 0x24),
    window: Color32::from_rgb(0x28, 0x29, 0x2e),
    backdrop: Color32::from_rgb(0x15, 0x16, 0x19),
    button: Color32::from_rgb(0x30, 0x31, 0x37),
    button_hover: Color32::from_rgb(0x3b, 0x3c, 0x44),
    button_active: Color32::from_rgb(0x4a, 0x4b, 0x55),
    border: Color32::from_rgb(0x3a, 0x3b, 0x43),
    text: Color32::from_rgb(0xe6, 0xe7, 0xeb),
    text_soft: Color32::from_rgb(0x9b, 0x9d, 0xa8),
    accent: Color32::from_rgb(0x4f, 0x86, 0xff),
    accent_strong: Color32::from_rgb(0x7c, 0xa5, 0xff),
    accent_soft: Color32::from_rgb(0x26, 0x36, 0x5e),
    warning: Color32::from_rgb(0xe0, 0xa4, 0x2c),
    error: Color32::from_rgb(0xff, 0x6b, 0x6b),
    checker: (
        Color32::from_rgb(0x4a, 0x4a, 0x4a),
        Color32::from_rgb(0x3a, 0x3a, 0x3a),
    ),
};

/// Which theme the user picked.
#[derive(Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

static IS_DARK: AtomicBool = AtomicBool::new(false);

/// Palette of the theme in use.
pub fn pal() -> &'static Palette {
    if IS_DARK.load(Ordering::Relaxed) {
        &DARK
    } else {
        &LIGHT
    }
}

/// Called every frame: follows the theme egui is showing (it can change with the system).
pub fn sync(ctx: &egui::Context) {
    IS_DARK.store(ctx.theme() == Theme::Dark, Ordering::Relaxed);
}

pub fn install(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_fonts(fonts());
    for (theme, palette) in [(Theme::Light, &LIGHT), (Theme::Dark, &DARK)] {
        ctx.set_visuals_of(theme, visuals(theme, palette));
        ctx.style_mut_of(theme, style);
    }
    set_choice(ctx, choice);
}

pub fn set_choice(ctx: &egui::Context, choice: ThemeChoice) {
    ctx.set_theme(match choice {
        ThemeChoice::System => egui::ThemePreference::System,
        ThemeChoice::Light => egui::ThemePreference::Light,
        ThemeChoice::Dark => egui::ThemePreference::Dark,
    });
    sync(ctx);
}

fn style(s: &mut egui::Style) {
    s.text_styles = [
        (TextStyle::Small, FontId::proportional(11.0)),
        (TextStyle::Body, FontId::proportional(13.5)),
        (TextStyle::Button, FontId::proportional(13.5)),
        (TextStyle::Monospace, FontId::monospace(12.5)),
        (
            TextStyle::Heading,
            FontId::new(17.0, FontFamily::Name(STRONG.into())),
        ),
    ]
    .into();
    let sp = &mut s.spacing;
    sp.item_spacing = Vec2::new(8.0, 6.0);
    sp.button_padding = Vec2::new(8.0, 4.0);
    sp.interact_size.y = 24.0;
    sp.window_margin = Margin::same(12);
    sp.menu_margin = Margin::same(6);
    sp.slider_width = 120.0;
    sp.indent = 16.0;
    s.interaction.tooltip_delay = 0.35;
}

fn fonts() -> FontDefinitions {
    let mut f = FontDefinitions::default();
    let data: [(&str, &'static [u8]); 4] = [
        ("inter", include_bytes!("../assets/fonts/Inter-Regular.ttf")),
        (
            "inter-semibold",
            include_bytes!("../assets/fonts/Inter-SemiBold.ttf"),
        ),
        ("phosphor", include_bytes!("../assets/fonts/Phosphor.ttf")),
        (
            "phosphor-fill",
            include_bytes!("../assets/fonts/Phosphor-Fill.ttf"),
        ),
    ];
    for (name, bytes) in data {
        f.font_data
            .insert(name.into(), FontData::from_static(bytes).into());
    }
    // egui's own fonts stay as fallback (symbols Inter does not have).
    let fallback = f.families[&FontFamily::Proportional].clone();
    let family = |text: &str, icons: &str| {
        let mut v = vec![text.to_string(), icons.to_string()];
        v.extend(fallback.iter().cloned());
        v
    };
    f.families
        .insert(FontFamily::Proportional, family("inter", "phosphor"));
    f.families.insert(
        FontFamily::Name(FILL.into()),
        family("inter", "phosphor-fill"),
    );
    f.families.insert(
        FontFamily::Name(STRONG.into()),
        family("inter-semibold", "phosphor"),
    );
    if let Some(mono) = f.families.get_mut(&FontFamily::Monospace) {
        mono.insert(1.min(mono.len()), "phosphor".into());
    }
    f
}

fn visuals(theme: Theme, p: &Palette) -> egui::Visuals {
    let mut v = match theme {
        Theme::Light => egui::Visuals::light(),
        Theme::Dark => egui::Visuals::dark(),
    };
    v.override_text_color = None;
    v.panel_fill = p.panel;
    v.window_fill = p.window;
    v.window_stroke = Stroke::new(1.0, p.border);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    let shadow_alpha = if theme == Theme::Dark { 90 } else { 30 };
    v.window_shadow = Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(shadow_alpha),
    };
    v.popup_shadow = Shadow {
        offset: [0, 4],
        blur: 12,
        spread: 0,
        color: Color32::from_black_alpha(shadow_alpha - 5),
    };
    v.faint_bg_color = p.button;
    v.extreme_bg_color = p.window;
    v.code_bg_color = p.button;
    v.hyperlink_color = p.accent_strong;
    v.warn_fg_color = p.warning;
    v.error_fg_color = p.error;
    v.selection.bg_fill = p.accent_soft;
    v.selection.stroke = Stroke::new(1.0, p.accent_strong);
    v.text_cursor.stroke = Stroke::new(2.0, p.accent);
    v.slider_trailing_fill = true;
    v.interact_cursor = Some(egui::CursorIcon::PointingHand);

    let w = &mut v.widgets;
    let radius = CornerRadius::same(6);
    w.noninteractive.bg_fill = p.panel;
    w.noninteractive.weak_bg_fill = p.panel;
    w.noninteractive.bg_stroke = Stroke::new(1.0, p.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    w.noninteractive.corner_radius = radius;
    for (state, fill) in [
        (&mut w.inactive, p.button),
        (&mut w.hovered, p.button_hover),
        (&mut w.active, p.button_active),
        (&mut w.open, p.button_hover),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.fg_stroke = Stroke::new(1.0, p.text);
        state.corner_radius = radius;
        state.expansion = 0.0;
    }
    w.inactive.bg_stroke = Stroke::NONE;
    w.hovered.bg_stroke = Stroke::new(1.0, p.border);
    w.active.bg_stroke = Stroke::new(1.0, p.text_soft);
    w.open.bg_stroke = Stroke::new(1.0, p.border);
    v
}

/// Text of an icon, filled if `active`.
pub fn icon(icon: &str, active: bool) -> RichText {
    let family = if active {
        FontFamily::Name(FILL.into())
    } else {
        FontFamily::Proportional
    };
    RichText::new(icon).family(family).size(ICON_SIZE)
}

/// Square button with only an icon; `help` goes in the tooltip (name and shortcut).
pub fn icon_button(ui: &mut Ui, ico: &str, help: &str) -> Response {
    icon_button_if(ui, true, ico, help)
}

/// Like [`icon_button`], disabled if `!enabled`.
pub fn icon_button_if(ui: &mut Ui, enabled: bool, ico: &str, help: &str) -> Response {
    let b = egui::Button::new(icon(ico, false))
        .frame_when_inactive(false)
        .min_size(Vec2::splat(30.0));
    ui.add_enabled(enabled, b).on_hover_text(help)
}

/// Tool button or switch: filled icon over the accent when active.
pub fn toggle(ui: &mut Ui, active: bool, ico: &str, help: &str) -> Response {
    let p = pal();
    let text = icon(ico, active).color(if active { Color32::WHITE } else { p.text });
    let mut b = egui::Button::new(text).min_size(Vec2::splat(30.0));
    b = if active {
        b.fill(p.accent)
    } else {
        b.frame_when_inactive(false)
    };
    ui.add(b).on_hover_text(help)
}

/// Button of the main action (accent, white text).
pub fn primary_button(ui: &mut Ui, ico: &str, text: &str) -> Response {
    let b = egui::Button::new(
        RichText::new(format!("{ico}  {text}"))
            .color(Color32::WHITE)
            .family(FontFamily::Name(STRONG.into())),
    )
    .fill(pal().accent)
    .min_size(Vec2::new(0.0, 30.0));
    ui.add(b)
}

/// Small icon button for list rows: strong if `active`, faded if not.
pub fn mini(ui: &mut Ui, active: bool, ico: &str, help: &str) -> Response {
    let p = pal();
    let color = if active {
        p.text
    } else {
        p.text_soft.gamma_multiply(0.7)
    };
    let text = RichText::new(ico).size(15.0).color(color);
    ui.add(egui::Button::new(text).frame_when_inactive(false).small())
        .on_hover_text(help)
}

/// Round color swatch that can be clicked (transparent colors show a checkerboard).
pub fn swatch(ui: &mut Ui, color: Color32, size: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), egui::Sense::click());
    let p = pal();
    let border = if resp.hovered() { p.text } else { p.border };
    let r = size / 2.0 - 1.0;
    if color.a() < 255 {
        let painter = ui.painter();
        painter.circle_filled(rect.center(), r, p.checker.0);
        let q = egui::Rect::from_center_size(rect.center(), Vec2::splat(r));
        painter.rect_filled(
            egui::Rect::from_min_max(q.min, q.center()),
            0.0,
            p.checker.1,
        );
        painter.rect_filled(
            egui::Rect::from_min_max(q.center(), q.max),
            0.0,
            p.checker.1,
        );
    }
    ui.painter()
        .circle(rect.center(), r, color, Stroke::new(1.5, border));
    resp
}

/// Section title of a panel: small, uppercase and soft.
pub fn section_title(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(text.to_uppercase())
            .size(11.0)
            .family(FontFamily::Name(STRONG.into()))
            .color(pal().text_soft),
    );
}

/// Bold text in the strong family.
pub fn strong(text: impl Into<String>) -> RichText {
    RichText::new(text.into()).family(FontFamily::Name(STRONG.into()))
}

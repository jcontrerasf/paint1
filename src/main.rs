//! paint1: a simple Paint-like image editor where everything drawn stays editable.
//!
//! `cargo run -- [drawing.paint1 | image.png]`

// No console window on Windows (kept in debug builds to see messages).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod canvas;
mod clipboard;
mod document;
mod export;
mod file_format;
mod history;
mod i18n;
mod icons;
mod prefs;
mod raster;
mod render;
mod text;
mod theme;
mod tools;
mod ui;

use std::path::PathBuf;

use eframe::egui;

fn main() -> eframe::Result {
    let initial = std::env::args_os().nth(1).map(PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(app::APP)
            .with_app_id(app::APP_ID)
            .with_icon(eframe::icon_data::from_png_bytes(app::ICON).expect("valid PNG icon"))
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([720.0, 480.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        app::APP,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, initial)))),
    )
}

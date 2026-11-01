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
        wgpu_options: wgpu_options(),
        ..Default::default()
    };
    eframe::run_native(
        app::APP,
        options,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, initial)))),
    )
}

/// On Windows wgpu tries Vulkan first, and some Vulkan drivers/layers crash with an access
/// violation while creating the window. DX12 (with GL as fallback) is always available on
/// Windows 10/11. `WGPU_BACKEND` still takes precedence.
fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    let mut opts = eframe::egui_wgpu::WgpuConfiguration::default();
    if cfg!(windows)
        && std::env::var_os("WGPU_BACKEND").is_none()
        && let eframe::egui_wgpu::WgpuSetup::CreateNew(setup) = &mut opts.wgpu_setup
    {
        setup.instance_descriptor.backends =
            eframe::wgpu::Backends::DX12 | eframe::wgpu::Backends::GL;
    }
    opts
}

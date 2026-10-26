//! What the program remembers between sessions (stored by eframe).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::export;
use crate::i18n::Lang;
use crate::theme::ThemeChoice;
use crate::tools::ToolStyle;

pub const KEY: &str = "prefs";
/// Recent files kept.
pub const MAX_RECENT: usize = 10;

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Prefs {
    pub lang: Lang,
    pub theme: ThemeChoice,
    pub recent: Vec<PathBuf>,
    pub export: export::Settings,
    pub style: ToolStyle,
    pub ui_zoom: f32,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            lang: system_lang(),
            theme: ThemeChoice::System,
            recent: Vec::new(),
            export: export::Settings::default(),
            style: ToolStyle::default(),
            ui_zoom: 1.0,
        }
    }
}

impl Prefs {
    pub fn add_recent(&mut self, path: PathBuf) {
        self.recent.retain(|p| p != &path);
        self.recent.insert(0, path);
        self.recent.truncate(MAX_RECENT);
    }
}

/// Spanish unless the system says English.
fn system_lang() -> Lang {
    let var = ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|v| std::env::var(v).ok().filter(|s| !s.is_empty()))
        .unwrap_or_default();
    if var.starts_with("en") {
        Lang::En
    } else {
        Lang::Es
    }
}

//! Phosphor icons (<https://phosphoricons.com>) used in the UI.
//!
//! The font is embedded (see `theme::install`) as a fallback of Inter, so an icon can be mixed
//! with text: `format!("{OPEN} Open…")`. Regular and Fill share codepoints; the filled one is
//! requested with the [`crate::theme::FILL`] family. Codes taken from `egui-phosphor` 0.14.

// File and edit.
pub const OPEN: &str = "\u{E256}"; // folder-open
pub const NEW: &str = "\u{E236}"; // file-plus
pub const SAVE: &str = "\u{E248}"; // floppy-disk
pub const EXPORT: &str = "\u{EAF0}"; // export
pub const UNDO: &str = "\u{E08A}"; // arrow-u-up-left
pub const REDO: &str = "\u{E08C}"; // arrow-u-up-right
pub const DELETE: &str = "\u{E4A6}"; // trash
pub const DUPLICATE: &str = "\u{E1CA}"; // copy
pub const COPY: &str = "\u{E1CA}"; // copy
pub const PASTE: &str = "\u{E196}"; // clipboard
pub const CUT: &str = "\u{EAE0}"; // scissors
pub const INFO: &str = "\u{E2CE}"; // info
pub const IMAGE: &str = "\u{E2CA}"; // image
pub const INSERT_IMAGE: &str = "\u{EA24}"; // file-image
pub const MENU: &str = "\u{E2F0}"; // list
pub const RECENT: &str = "\u{E1A0}"; // clock-counter-clockwise
pub const FILE: &str = "\u{E230}"; // file
pub const ZOOM_IN: &str = "\u{E310}"; // magnifying-glass-plus
pub const ZOOM_OUT: &str = "\u{E30E}"; // magnifying-glass-minus
pub const FIT: &str = "\u{E1D0}"; // corners-out
pub const LANGUAGE: &str = "\u{E4A2}"; // translate
pub const THEME_LIGHT: &str = "\u{E472}"; // sun
pub const THEME_DARK: &str = "\u{E330}"; // moon
pub const THEME_SYSTEM: &str = "\u{E18C}"; // circle-half
pub const OK: &str = "\u{E182}"; // check
pub const CANCEL: &str = "\u{E4F6}"; // x

// Object list.
pub const VISIBLE: &str = "\u{E220}"; // eye
pub const HIDDEN: &str = "\u{E224}"; // eye-slash
pub const LOCKED: &str = "\u{E2FA}"; // lock
pub const UNLOCKED: &str = "\u{E306}"; // lock-open
pub const RAISE: &str = "\u{E13C}"; // caret-up
pub const LOWER: &str = "\u{E136}"; // caret-down
pub const TO_FRONT: &str = "\u{E12C}"; // caret-double-up
pub const TO_BACK: &str = "\u{E126}"; // caret-double-down

// Tools.
pub const SELECT: &str = "\u{E1DC}"; // cursor
pub const PENCIL: &str = "\u{E3B4}"; // pencil-simple
pub const HIGHLIGHTER: &str = "\u{EC76}"; // highlighter
pub const ERASER: &str = "\u{E21E}"; // eraser
pub const RECT: &str = "\u{E3F0}"; // rectangle
pub const ROUND_RECT: &str = "\u{E45E}"; // square
pub const ELLIPSE: &str = "\u{E18A}"; // circle
pub const POLYGON: &str = "\u{E6D0}"; // polygon
pub const LINE: &str = "\u{E6D2}"; // line-segment
pub const ARROW: &str = "\u{E06A}"; // arrow-line-up-right
pub const TEXT: &str = "\u{E48A}"; // text-t
pub const CROP: &str = "\u{E1D4}"; // crop
pub const EYEDROPPER: &str = "\u{E568}"; // eyedropper

// Transform.
pub const ROTATE_LEFT: &str = "\u{E038}"; // arrow-counter-clockwise
pub const ROTATE_RIGHT: &str = "\u{E036}"; // arrow-clockwise
pub const ROTATE_180: &str = "\u{E094}"; // arrows-clockwise
pub const FLIP_H: &str = "\u{ED6A}"; // flip-horizontal
pub const FLIP_V: &str = "\u{ED6C}"; // flip-vertical
pub const SCALE: &str = "\u{ED6E}"; // resize
pub const FIT_CANVAS: &str = "\u{E626}"; // frame-corners
pub const CANVAS_SIZE: &str = "\u{E6CE}"; // bounding-box
pub const SELECT_ALL: &str = "\u{E746}"; // selection-all

// Text.
pub const BOLD: &str = "\u{E5BE}"; // text-b
pub const ALIGN_LEFT: &str = "\u{E484}"; // text-align-left
pub const ALIGN_CENTER: &str = "\u{E480}"; // text-align-center
pub const ALIGN_RIGHT: &str = "\u{E486}"; // text-align-right

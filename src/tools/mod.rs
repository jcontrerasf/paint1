//! Tools of the canvas: what a click or a drag does.

pub mod area;
pub mod crop;
pub mod draw;
pub mod select;
pub mod text;

use eframe::egui::Key;
use serde::{Deserialize, Serialize};

use crate::document::{Rgba, Style, TextAlign};
use crate::i18n::t;
use crate::icons;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tool {
    Select,
    RectSelect,
    FreeSelect,
    Pencil,
    Highlighter,
    Eraser,
    Rect,
    RoundRect,
    Ellipse,
    RegularPolygon,
    Line,
    Arrow,
    Polygon,
    Text,
    Crop,
    Eyedropper,
}

impl Tool {
    /// In the order of the toolbar; `None` is a separator.
    pub const BAR: [Option<Tool>; 19] = [
        Some(Tool::Select),
        Some(Tool::RectSelect),
        Some(Tool::FreeSelect),
        None,
        Some(Tool::Pencil),
        Some(Tool::Highlighter),
        Some(Tool::Eraser),
        None,
        Some(Tool::Rect),
        Some(Tool::RoundRect),
        Some(Tool::Ellipse),
        Some(Tool::RegularPolygon),
        Some(Tool::Polygon),
        Some(Tool::Line),
        Some(Tool::Arrow),
        Some(Tool::Text),
        None,
        Some(Tool::Crop),
        Some(Tool::Eyedropper),
    ];

    pub fn icon(self) -> &'static str {
        match self {
            Tool::Select => icons::SELECT,
            Tool::RectSelect => icons::RECT_SELECT,
            Tool::FreeSelect => icons::FREE_SELECT,
            Tool::RegularPolygon => icons::REGULAR_POLYGON,
            Tool::Pencil => icons::PENCIL,
            Tool::Highlighter => icons::HIGHLIGHTER,
            Tool::Eraser => icons::ERASER,
            Tool::Rect => icons::RECT,
            Tool::RoundRect => icons::ROUND_RECT,
            Tool::Ellipse => icons::ELLIPSE,
            Tool::Line => icons::LINE,
            Tool::Arrow => icons::ARROW,
            Tool::Polygon => icons::POLYGON,
            Tool::Text => icons::TEXT,
            Tool::Crop => icons::CROP,
            Tool::Eyedropper => icons::EYEDROPPER,
        }
    }

    pub fn help(self) -> &'static str {
        let t = t();
        match self {
            Tool::Select => t.tool_select,
            Tool::RectSelect => t.tool_rect_select,
            Tool::FreeSelect => t.tool_free_select,
            Tool::RegularPolygon => t.tool_regular_polygon,
            Tool::Pencil => t.tool_pencil,
            Tool::Highlighter => t.tool_highlighter,
            Tool::Eraser => t.tool_eraser,
            Tool::Rect => t.tool_rect,
            Tool::RoundRect => t.tool_round_rect,
            Tool::Ellipse => t.tool_ellipse,
            Tool::Line => t.tool_line,
            Tool::Arrow => t.tool_arrow,
            Tool::Polygon => t.tool_polygon,
            Tool::Text => t.tool_text,
            Tool::Crop => t.tool_crop,
            Tool::Eyedropper => t.tool_eyedropper,
        }
    }

    /// Hint shown in the status bar while the tool is active.
    pub fn hint(self) -> &'static str {
        let t = t();
        match self {
            Tool::Select => t.hint_select,
            Tool::RectSelect | Tool::FreeSelect => t.hint_area,
            Tool::Pencil | Tool::Highlighter | Tool::Eraser => t.hint_draw,
            Tool::Rect
            | Tool::RoundRect
            | Tool::Ellipse
            | Tool::RegularPolygon
            | Tool::Line
            | Tool::Arrow => t.hint_shape,
            Tool::Polygon => t.hint_polygon,
            Tool::Text => t.hint_text,
            Tool::Crop => t.hint_crop,
            Tool::Eyedropper => t.hint_eyedropper,
        }
    }

    pub fn shortcut(key: Key) -> Option<Tool> {
        Some(match key {
            Key::V => Tool::Select,
            Key::S => Tool::RectSelect,
            Key::F => Tool::FreeSelect,
            Key::N => Tool::RegularPolygon,
            Key::P => Tool::Pencil,
            Key::H => Tool::Highlighter,
            Key::E => Tool::Eraser,
            Key::R => Tool::Rect,
            Key::U => Tool::RoundRect,
            Key::O => Tool::Ellipse,
            Key::L => Tool::Line,
            Key::A => Tool::Arrow,
            Key::G => Tool::Polygon,
            Key::T => Tool::Text,
            Key::C => Tool::Crop,
            Key::I => Tool::Eyedropper,
            _ => return None,
        })
    }

    /// Tools that create shapes with a fill.
    pub fn uses_fill(self) -> bool {
        matches!(
            self,
            Tool::Rect
                | Tool::RoundRect
                | Tool::Ellipse
                | Tool::RegularPolygon
                | Tool::Polygon
                | Tool::Text
        )
    }
}

/// Style for the next objects drawn (edited in the context bar, remembered).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ToolStyle {
    pub stroke: Rgba,
    pub fill: Rgba,
    pub fill_on: bool,
    pub width: f64,
    pub opacity: f32,
    pub highlight: Rgba,
    pub highlight_width: f64,
    pub eraser_width: f64,
    pub radius: f64,
    pub head: f64,
    pub sides: u32,
    pub font_size: f64,
    pub bold: bool,
    pub align: TextAlign,
}

impl Default for ToolStyle {
    fn default() -> Self {
        ToolStyle {
            stroke: Rgba([0x20, 0x20, 0x20, 255]),
            fill: Rgba([0x4f, 0x86, 0xff, 255]),
            fill_on: false,
            width: 4.0,
            opacity: 1.0,
            highlight: Rgba([0xff, 0xe0, 0x2e, 255]),
            highlight_width: 8.0,
            eraser_width: 20.0,
            radius: 16.0,
            head: 18.0,
            sides: 6,
            font_size: 32.0,
            bold: false,
            align: TextAlign::Left,
        }
    }
}

impl ToolStyle {
    /// Style of a new object made with `tool`.
    pub fn for_tool(&self, tool: Tool) -> Style {
        let (stroke, width) = match tool {
            Tool::Highlighter => (self.highlight, self.highlight_width),
            Tool::Eraser => (Rgba::BLACK, self.eraser_width),
            _ => (self.stroke, self.width),
        };
        Style {
            stroke,
            width,
            fill: (self.fill_on && tool.uses_fill()).then_some(self.fill),
            opacity: if tool == Tool::Eraser {
                1.0
            } else {
                self.opacity
            },
        }
    }

    /// Width the context bar edits for `tool`.
    pub fn width_mut(&mut self, tool: Tool) -> &mut f64 {
        match tool {
            Tool::Highlighter => &mut self.highlight_width,
            Tool::Eraser => &mut self.eraser_width,
            _ => &mut self.width,
        }
    }

    pub fn color_mut(&mut self, tool: Tool) -> &mut Rgba {
        match tool {
            Tool::Highlighter => &mut self.highlight,
            _ => &mut self.stroke,
        }
    }
}

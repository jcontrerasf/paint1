//! Text objects: layout with the bundled Inter fonts and conversion of the glyphs to a path,
//! so text renders (and exports) exactly like any other shape.

use std::sync::LazyLock;

use ab_glyph::{Font, FontRef, OutlineCurve, ScaleFont};
use kurbo::{Point, Size};

use crate::document::TextAlign;

static REGULAR: LazyLock<FontRef<'static>> = LazyLock::new(|| {
    FontRef::try_from_slice(include_bytes!("../assets/fonts/Inter-Regular.ttf"))
        .expect("embedded font")
});
static BOLD: LazyLock<FontRef<'static>> = LazyLock::new(|| {
    FontRef::try_from_slice(include_bytes!("../assets/fonts/Inter-SemiBold.ttf"))
        .expect("embedded font")
});

fn font(bold: bool) -> &'static FontRef<'static> {
    if bold { &BOLD } else { &REGULAR }
}

/// Distance between baselines.
pub fn line_height(size: f64, bold: bool) -> f64 {
    let f = font(bold).as_scaled(size as f32);
    (f.height() + f.line_gap()) as f64
}

fn line_width(line: &str, size: f64, bold: bool) -> f64 {
    let f = font(bold).as_scaled(size as f32);
    let mut w = 0.0;
    let mut prev = None;
    for c in line.chars() {
        let id = f.glyph_id(c);
        if let Some(p) = prev {
            w += f.kern(p, id);
        }
        w += f.h_advance(id);
        prev = Some(id);
    }
    w as f64
}

/// Size of the laid out text. Never empty, so an empty text can still be clicked.
pub fn measure(content: &str, size: f64, bold: bool) -> Size {
    let lines: Vec<&str> = content.split('\n').collect();
    let w = lines
        .iter()
        .map(|l| line_width(l, size, bold))
        .fold(0.0, f64::max);
    Size::new(
        w.max(size * 0.3),
        lines.len() as f64 * line_height(size, bold),
    )
}

/// Where the caret goes after the last character (top of the caret, its height).
pub fn caret(content: &str, size: f64, bold: bool, align: TextAlign) -> (Point, f64) {
    let total = measure(content, size, bold).width;
    let lines: Vec<&str> = content.split('\n').collect();
    let last = lines.last().copied().unwrap_or("");
    let w = line_width(last, size, bold);
    let x = align_x(align, total, w) + w;
    let lh = line_height(size, bold);
    (Point::new(x, (lines.len() - 1) as f64 * lh), lh)
}

fn align_x(align: TextAlign, total: f64, width: f64) -> f64 {
    match align {
        TextAlign::Left => 0.0,
        TextAlign::Center => (total - width) / 2.0,
        TextAlign::Right => total - width,
    }
}

/// Outline of the text with its top-left at (0, 0).
pub fn path(content: &str, size: f64, bold: bool, align: TextAlign) -> Option<tiny_skia::Path> {
    let font = font(bold);
    let scaled = font.as_scaled(size as f32);
    let factor = scaled.scale_factor();
    let total = measure(content, size, bold).width;
    let lh = line_height(size, bold);
    let mut pb = tiny_skia::PathBuilder::new();
    for (i, line) in content.split('\n').enumerate() {
        let baseline = i as f32 * lh as f32 + scaled.ascent();
        let mut x = align_x(align, total, line_width(line, size, bold)) as f32;
        let mut prev = None;
        for c in line.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = prev {
                x += scaled.kern(p, id);
            }
            prev = Some(id);
            if let Some(outline) = font.outline(id) {
                let tr = |p: ab_glyph::Point| {
                    (
                        x + p.x * factor.horizontal,
                        baseline - p.y * factor.vertical,
                    )
                };
                let mut last: Option<(f32, f32)> = None;
                for curve in &outline.curves {
                    let start = match curve {
                        OutlineCurve::Line(a, _)
                        | OutlineCurve::Quad(a, _, _)
                        | OutlineCurve::Cubic(a, _, _, _) => tr(*a),
                    };
                    if last.is_none_or(|l| {
                        (l.0 - start.0).abs() > 1e-3 || (l.1 - start.1).abs() > 1e-3
                    }) {
                        if last.is_some() {
                            pb.close();
                        }
                        pb.move_to(start.0, start.1);
                    }
                    let end = match curve {
                        OutlineCurve::Line(_, b) => {
                            let b = tr(*b);
                            pb.line_to(b.0, b.1);
                            b
                        }
                        OutlineCurve::Quad(_, c, b) => {
                            let (c, b) = (tr(*c), tr(*b));
                            pb.quad_to(c.0, c.1, b.0, b.1);
                            b
                        }
                        OutlineCurve::Cubic(_, c1, c2, b) => {
                            let (c1, c2, b) = (tr(*c1), tr(*c2), tr(*b));
                            pb.cubic_to(c1.0, c1.1, c2.0, c2.1, b.0, b.1);
                            b
                        }
                    };
                    last = Some(end);
                }
                if last.is_some() {
                    pb.close();
                }
            }
            x += scaled.h_advance(id);
        }
    }
    pb.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measure_grows_with_lines_and_size() {
        let one = measure("Hola", 20.0, false);
        let two = measure("Hola\nmundo", 20.0, false);
        assert!(two.height > one.height * 1.9);
        assert!(measure("Hola", 40.0, false).width > one.width * 1.9);
        assert!(path("Hola", 20.0, true, TextAlign::Center).is_some());
        assert!(path("", 20.0, false, TextAlign::Left).is_none());
    }
}

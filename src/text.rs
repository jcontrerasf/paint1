//! Text objects: layout with the bundled Inter fonts or any installed font, and conversion of
//! the glyphs to a path, so text renders (and exports) exactly like any other shape.

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use ab_glyph::{Font, FontArc, FontRef, FontVec, OutlineCurve, PxScaleFont, ScaleFont};
use kurbo::{Point, Size};

use crate::document::TextAlign;

/// Horizontal slant of the synthetic italic (fonts without an italic face).
const SKEW: f32 = 0.2;

/// Everything that decides how a text is laid out and drawn.
#[derive(Clone, Copy, Debug)]
pub struct TextLook<'a> {
    /// Installed font family; empty = the bundled Inter.
    pub family: &'a str,
    pub size: f64,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub align: TextAlign,
}

/// A loaded font face with what layout needs to know about it.
struct Face {
    font: FontArc,
    /// The face itself is italic (otherwise italic is synthesized).
    italic: bool,
    /// Underline position (negative = below the baseline) and thickness, in font units.
    underline: (f32, f32),
}

impl Face {
    fn new(font: FontArc, data: &[u8], index: u32, italic: bool) -> Face {
        let upem = font.units_per_em().unwrap_or(1000.0);
        let underline = ttf_parser::Face::parse(data, index)
            .ok()
            .and_then(|f| f.underline_metrics())
            .filter(|m| m.thickness > 0)
            .map(|m| (m.position as f32, m.thickness as f32))
            .unwrap_or((-0.1 * upem, 0.06 * upem));
        Face {
            font,
            italic,
            underline,
        }
    }
}

const INTER_REGULAR: &[u8] = include_bytes!("../assets/fonts/Inter-Regular.ttf");
const INTER_BOLD: &[u8] = include_bytes!("../assets/fonts/Inter-SemiBold.ttf");

fn bundled(data: &'static [u8]) -> Arc<Face> {
    let font = FontRef::try_from_slice(data).expect("embedded font");
    Arc::new(Face::new(FontArc::new(font), data, 0, false))
}

static REGULAR: LazyLock<Arc<Face>> = LazyLock::new(|| bundled(INTER_REGULAR));
static BOLD: LazyLock<Arc<Face>> = LazyLock::new(|| bundled(INTER_BOLD));

/// Installed fonts (scanning takes a moment, see [`warm_up`]).
static SYSTEM: LazyLock<fontdb::Database> = LazyLock::new(|| {
    let mut db = fontdb::Database::new();
    db.load_system_fonts();
    db
});

static FAMILIES: LazyLock<Vec<String>> = LazyLock::new(|| {
    let mut names: Vec<String> = SYSTEM
        .faces()
        .filter_map(|f| f.families.first().map(|(name, _)| name.clone()))
        .collect();
    names.sort_by_key(|n| n.to_lowercase());
    names.dedup();
    names
});

type FaceKey = (String, bool, bool);
static LOADED: LazyLock<RwLock<HashMap<FaceKey, Arc<Face>>>> = LazyLock::new(Default::default);

/// Scans the installed fonts in the background, so the first text does not wait for it.
pub fn warm_up() {
    std::thread::spawn(|| LazyLock::force(&FAMILIES));
}

/// Families of the installed fonts, sorted.
pub fn families() -> &'static [String] {
    &FAMILIES
}

fn face(look: &TextLook) -> Arc<Face> {
    let fallback = || {
        if look.bold {
            BOLD.clone()
        } else {
            REGULAR.clone()
        }
    };
    if look.family.is_empty() {
        return fallback();
    }
    let key = (look.family.to_string(), look.bold, look.italic);
    if let Some(f) = LOADED.read().unwrap().get(&key) {
        return f.clone();
    }
    let face = load_system(&key).unwrap_or_else(fallback);
    LOADED.write().unwrap().insert(key, face.clone());
    face
}

fn load_system((family, bold, italic): &FaceKey) -> Option<Arc<Face>> {
    let query = fontdb::Query {
        families: &[fontdb::Family::Name(family)],
        weight: if *bold {
            fontdb::Weight::BOLD
        } else {
            fontdb::Weight::NORMAL
        },
        style: if *italic {
            fontdb::Style::Italic
        } else {
            fontdb::Style::Normal
        },
        ..Default::default()
    };
    let id = SYSTEM.query(&query)?;
    let is_italic = SYSTEM.face(id)?.style != fontdb::Style::Normal;
    SYSTEM
        .with_face_data(id, |data, index| {
            let font = FontVec::try_from_vec_and_index(data.to_vec(), index).ok()?;
            Some(Arc::new(Face::new(
                FontArc::new(font),
                data,
                index,
                is_italic,
            )))
        })
        .flatten()
}

fn scaled<'f>(face: &'f Face, look: &TextLook) -> PxScaleFont<&'f FontArc> {
    face.font.as_scaled(look.size as f32)
}

/// Distance between baselines.
pub fn line_height(look: &TextLook) -> f64 {
    let face = face(look);
    let f = scaled(&face, look);
    (f.height() + f.line_gap()) as f64
}

/// X of every char boundary of `line` (byte index in the line, x), starting with `(0, 0)`.
fn stops(f: &PxScaleFont<&FontArc>, line: &str) -> Vec<(usize, f64)> {
    let mut out = vec![(0, 0.0)];
    let mut x = 0.0;
    let mut prev = None;
    for (i, c) in line.char_indices() {
        let id = f.glyph_id(c);
        if let Some(p) = prev {
            x += f.kern(p, id);
        }
        x += f.h_advance(id);
        prev = Some(id);
        out.push((i + c.len_utf8(), x as f64));
    }
    out
}

fn line_width(f: &PxScaleFont<&FontArc>, line: &str) -> f64 {
    stops(f, line).last().map_or(0.0, |s| s.1)
}

fn text_width(f: &PxScaleFont<&FontArc>, content: &str, look: &TextLook) -> f64 {
    content
        .split('\n')
        .map(|l| line_width(f, l))
        .fold(0.0, f64::max)
        .max(look.size * 0.3)
}

/// Size of the laid out text. Never empty, so an empty text can still be clicked.
pub fn measure(content: &str, look: &TextLook) -> Size {
    let face = face(look);
    let f = scaled(&face, look);
    let lines = content.split('\n').count();
    Size::new(
        text_width(&f, content, look),
        lines as f64 * (f.height() + f.line_gap()) as f64,
    )
}

fn align_x(align: TextAlign, total: f64, width: f64) -> f64 {
    match align {
        TextAlign::Left => 0.0,
        TextAlign::Center => (total - width) / 2.0,
        TextAlign::Right => total - width,
    }
}

/// Lines with the byte index where each starts.
fn lines(content: &str) -> impl Iterator<Item = (usize, &str)> {
    content.split('\n').scan(0, |start, line| {
        let at = *start;
        *start += line.len() + 1;
        Some((at, line))
    })
}

/// Where the caret goes before byte `cursor` (top of the caret, its height).
pub fn caret(content: &str, cursor: usize, look: &TextLook) -> (Point, f64) {
    let face = face(look);
    let f = scaled(&face, look);
    let total = text_width(&f, content, look);
    let lh = (f.height() + f.line_gap()) as f64;
    let (row, (start, line)) = lines(content)
        .enumerate()
        .find(|(_, (start, line))| cursor <= start + line.len())
        .unwrap_or((0, (0, "")));
    let st = stops(&f, line);
    let x = st
        .iter()
        .find(|s| s.0 >= cursor - start)
        .map_or(0.0, |s| s.1);
    let w = st.last().map_or(0.0, |s| s.1);
    (
        Point::new(align_x(look.align, total, w) + x, row as f64 * lh),
        lh,
    )
}

/// Byte index of the char boundary closest to `p` (local coordinates).
pub fn index_at(content: &str, p: Point, look: &TextLook) -> usize {
    let face = face(look);
    let f = scaled(&face, look);
    let total = text_width(&f, content, look);
    let lh = (f.height() + f.line_gap()) as f64;
    let all: Vec<_> = lines(content).collect();
    let row = ((p.y / lh).floor().max(0.0) as usize).min(all.len() - 1);
    let (start, line) = all[row];
    let st = stops(&f, line);
    let w = st.last().map_or(0.0, |s| s.1);
    let x = p.x - align_x(look.align, total, w);
    let best = st
        .iter()
        .min_by(|a, b| (a.1 - x).abs().total_cmp(&(b.1 - x).abs()))
        .map_or(0, |s| s.0);
    start + best
}

/// Outline of the glyphs with the top-left of the text at (0, 0).
pub fn path(content: &str, look: &TextLook) -> Option<tiny_skia::Path> {
    let face = face(look);
    let scaled = scaled(&face, look);
    let factor = scaled.scale_factor();
    let skew = if look.italic && !face.italic {
        SKEW
    } else {
        0.0
    };
    let total = text_width(&scaled, content, look);
    let lh = scaled.height() + scaled.line_gap();
    let mut pb = tiny_skia::PathBuilder::new();
    for (i, (_, line)) in lines(content).enumerate() {
        let baseline = i as f32 * lh + scaled.ascent();
        let mut x = align_x(look.align, total, line_width(&scaled, line)) as f32;
        let mut prev = None;
        for c in line.chars() {
            let id = scaled.glyph_id(c);
            if let Some(p) = prev {
                x += scaled.kern(p, id);
            }
            prev = Some(id);
            if let Some(outline) = face.font.outline(id) {
                let tr = |p: ab_glyph::Point| {
                    let up = p.y * factor.vertical;
                    (x + p.x * factor.horizontal + skew * up, baseline - up)
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

/// Underline bars of every non-empty line (separate from [`path`] so where they cross the
/// glyphs the winding of the two does not cancel out).
pub fn underline(content: &str, look: &TextLook) -> Option<tiny_skia::Path> {
    if !look.underline {
        return None;
    }
    let face = face(look);
    let scaled = scaled(&face, look);
    let fv = scaled.scale_factor().vertical;
    let (pos, thick) = face.underline;
    let total = text_width(&scaled, content, look);
    let lh = scaled.height() + scaled.line_gap();
    let mut pb = tiny_skia::PathBuilder::new();
    for (i, (_, line)) in lines(content).enumerate() {
        let w = line_width(&scaled, line);
        let baseline = i as f32 * lh + scaled.ascent();
        let top = baseline - pos * fv - thick * fv / 2.0;
        let x = align_x(look.align, total, w) as f32;
        if let Some(r) = tiny_skia::Rect::from_xywh(x, top, w as f32, thick * fv) {
            pb.push_rect(r);
        }
    }
    pb.finish()
}

/// Char boundary before `i`.
pub fn prev_char(s: &str, i: usize) -> usize {
    s[..i].char_indices().next_back().map_or(0, |(j, _)| j)
}

/// Char boundary after `i`.
pub fn next_char(s: &str, i: usize) -> usize {
    s[i..].chars().next().map_or(i, |c| i + c.len_utf8())
}

/// Start of the word before `i` (skipping the spaces in between).
pub fn prev_word(s: &str, i: usize) -> usize {
    let mut j = i;
    let mut seen_word = false;
    for (k, c) in s[..i].char_indices().rev() {
        if c.is_alphanumeric() {
            seen_word = true;
        } else if seen_word || c == '\n' {
            break;
        }
        j = k;
    }
    j
}

/// End of the word after `i` (skipping the spaces before it).
pub fn next_word(s: &str, i: usize) -> usize {
    let mut seen_word = false;
    for (k, c) in s[i..].char_indices() {
        if c.is_alphanumeric() {
            seen_word = true;
        } else if seen_word || c == '\n' {
            return i + k;
        }
    }
    s.len()
}

/// Start of the line that holds `i`.
pub fn line_start(s: &str, i: usize) -> usize {
    s[..i].rfind('\n').map_or(0, |j| j + 1)
}

/// End of the line that holds `i`.
pub fn line_end(s: &str, i: usize) -> usize {
    s[i..].find('\n').map_or(s.len(), |j| i + j)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(size: f64) -> TextLook<'static> {
        TextLook {
            family: "",
            size,
            bold: false,
            italic: false,
            underline: false,
            align: TextAlign::Left,
        }
    }

    #[test]
    fn measure_grows_with_lines_and_size() {
        let l = look(20.0);
        let one = measure("Hola", &l);
        let two = measure("Hola\nmundo", &l);
        assert!(two.height > one.height * 1.9);
        assert!(measure("Hola", &look(40.0)).width > one.width * 1.9);
        let bold = TextLook {
            bold: true,
            align: TextAlign::Center,
            ..l
        };
        assert!(path("Hola", &bold).is_some());
        assert!(path("", &l).is_none());
    }

    #[test]
    fn caret_and_index_round_trip() {
        let l = TextLook {
            align: TextAlign::Center,
            ..look(20.0)
        };
        let s = "Hola\nmundo áé\n";
        for i in (0..=s.len()).filter(|i| s.is_char_boundary(*i)) {
            let (top, h) = caret(s, i, &l);
            assert_eq!(index_at(s, top + kurbo::Vec2::new(0.0, h / 2.0), &l), i);
        }
    }

    #[test]
    fn italic_and_underline_change_the_outline() {
        let l = look(20.0);
        let plain = path("Hola", &l).unwrap().bounds();
        let italic = path("Hola", &TextLook { italic: true, ..l })
            .unwrap()
            .bounds();
        assert!(italic.right() > plain.right());
        assert!(underline("Hola", &l).is_none());
        let bar = underline(
            "Hola",
            &TextLook {
                underline: true,
                ..l
            },
        )
        .unwrap();
        assert!(bar.bounds().bottom() > plain.bottom() - 1.0);
    }

    #[test]
    fn missing_family_falls_back_to_inter() {
        let l = look(20.0);
        let missing = TextLook {
            family: "No Such Font 1234",
            ..l
        };
        assert_eq!(measure("Hola", &missing), measure("Hola", &l));
    }

    #[test]
    fn cursor_moves() {
        let s = "uno dos\ntres";
        assert_eq!(prev_word(s, 7), 4);
        assert_eq!(prev_word(s, 4), 0);
        assert_eq!(next_word(s, 0), 3);
        assert_eq!(next_word(s, 3), 7);
        assert_eq!(line_start(s, 10), 8);
        assert_eq!(line_end(s, 2), 7);
        assert_eq!(prev_char("aé", 3), 1);
        assert_eq!(next_char("aé", 1), 3);
    }
}

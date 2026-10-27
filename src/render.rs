//! Rasterizes a document with tiny-skia. The canvas on screen and every export go through
//! [`Renderer::render`], so what you see is what you get.

use std::collections::HashMap;
use std::sync::Arc;

use image::RgbaImage;
use kurbo::{Affine, Point, Rect, Size};
use tiny_skia::{
    BlendMode, Color, FillRule, FilterQuality, IntSize, LineCap, LineJoin, Paint, PathBuilder,
    Pixmap, PixmapPaint, Stroke, Transform,
};

use crate::document::{
    Document, Kind, Object, ObjectId, Rgba, StrokeMode, arrow_head, stroke_width,
};
use crate::text;

/// Opacity of the highlighter.
const HIGHLIGHTER_ALPHA: f32 = 0.4;

/// Keeps the images of the document converted to premultiplied pixmaps between renders.
#[derive(Default)]
pub struct Renderer {
    /// By address of the shared image; the `Arc` is kept so the address is not reused.
    cache: HashMap<usize, (Arc<RgbaImage>, Pixmap)>,
}

/// What to render.
pub struct Job<'a> {
    /// Part of the canvas, in canvas units.
    pub region: Rect,
    /// Output pixels per canvas unit.
    pub scale: f64,
    /// Only these objects (for copying a selection as an image).
    pub only: Option<&'a [ObjectId]>,
    pub background: bool,
}

impl Renderer {
    pub fn render(&mut self, doc: &Document, job: &Job) -> Option<Pixmap> {
        let w = (job.region.width() * job.scale).round().max(1.0) as u32;
        let h = (job.region.height() * job.scale).round().max(1.0) as u32;
        let mut out = Pixmap::new(w, h)?;
        let base = Transform::from_translate(-job.region.x0 as f32, -job.region.y0 as f32)
            .post_scale(job.scale as f32, job.scale as f32);
        let objects: Vec<&Object> = doc
            .objects
            .iter()
            .filter(|o| o.visible && job.only.is_none_or(|ids| ids.contains(&o.id)))
            .collect();
        // The eraser must only erase objects, not the background: draw them on their own
        // layer then.
        let erases = objects.iter().any(|o| {
            matches!(
                o.kind,
                Kind::Stroke {
                    mode: StrokeMode::Eraser,
                    ..
                }
            )
        });
        let with_background = job.background && doc.background.0[3] > 0;
        let mut layer = if erases && with_background {
            Some(Pixmap::new(w, h)?)
        } else {
            None
        };
        if with_background {
            let canvas = tiny_skia::Rect::from_xywh(0.0, 0.0, doc.width as f32, doc.height as f32)?;
            out.fill_rect(canvas, &solid(doc.background, 1.0), base, None);
        }
        let target = layer.as_mut().unwrap_or(&mut out);
        for o in objects {
            self.draw(target, doc, o, base);
        }
        if let Some(layer) = layer {
            out.draw_pixmap(
                0,
                0,
                layer.as_ref(),
                &PixmapPaint::default(),
                Transform::identity(),
                None,
            );
        }
        Some(out)
    }

    /// Color of the canvas at a point (what the eyedropper sees).
    pub fn sample(&mut self, doc: &Document, p: Point) -> Option<Rgba> {
        let (x, y) = (p.x.floor(), p.y.floor());
        if x < 0.0 || y < 0.0 || x >= doc.width as f64 || y >= doc.height as f64 {
            return None;
        }
        let job = Job {
            region: Rect::new(x, y, x + 1.0, y + 1.0),
            scale: 1.0,
            only: None,
            background: true,
        };
        let px = self.render(doc, &job)?.pixel(0, 0)?.demultiply();
        Some(Rgba([px.red(), px.green(), px.blue(), px.alpha()]))
    }

    /// Forgets converted images the document no longer has.
    pub fn prune(&mut self, doc: &Document) {
        if self.cache.len() > doc.assets.len() {
            self.cache
                .retain(|_, (img, _)| doc.assets.values().any(|a| Arc::ptr_eq(a, img)));
        }
    }

    fn image(&mut self, img: &Arc<RgbaImage>) -> Option<&Pixmap> {
        let key = Arc::as_ptr(img) as usize;
        if let std::collections::hash_map::Entry::Vacant(e) = self.cache.entry(key) {
            e.insert((img.clone(), to_pixmap(img)?));
        }
        self.cache.get(&key).map(|(_, p)| p)
    }

    fn draw(&mut self, pm: &mut Pixmap, doc: &Document, o: &Object, base: Transform) {
        let ts = base.pre_concat(to_transform(o.transform));
        let st = &o.style;
        let opacity = st.opacity.clamp(0.0, 1.0);
        match &o.kind {
            Kind::Stroke { points, mode } => {
                let width = stroke_width(*mode, st.width) as f32;
                let (mut paint, cap) = match mode {
                    StrokeMode::Pencil => (solid(st.stroke, opacity), LineCap::Round),
                    StrokeMode::Highlighter => (
                        solid(st.stroke, opacity * HIGHLIGHTER_ALPHA),
                        LineCap::Square,
                    ),
                    StrokeMode::Eraser => {
                        let mut p = solid(Rgba::BLACK, opacity);
                        p.blend_mode = BlendMode::DestinationOut;
                        (p, LineCap::Round)
                    }
                };
                paint.anti_alias = true;
                if let [p] = points.as_slice() {
                    let mut pb = PathBuilder::new();
                    if cap == LineCap::Square {
                        let half = width / 2.0;
                        if let Some(r) = tiny_skia::Rect::from_xywh(
                            p.x as f32 - half,
                            p.y as f32 - half,
                            width,
                            width,
                        ) {
                            pb.push_rect(r);
                        }
                    } else {
                        pb.push_circle(p.x as f32, p.y as f32, width / 2.0);
                    }
                    if let Some(path) = pb.finish() {
                        pm.fill_path(&path, &paint, FillRule::Winding, ts, None);
                    }
                } else if let Some(path) = smooth_path(points) {
                    pm.stroke_path(
                        &path,
                        &paint,
                        &stroke(width, cap, LineJoin::Round),
                        ts,
                        None,
                    );
                }
            }
            Kind::Line { a, b } => {
                if let Some(path) = polyline(&[*a, *b], false) {
                    let s = stroke(st.width as f32, LineCap::Round, LineJoin::Round);
                    pm.stroke_path(&path, &solid(st.stroke, opacity), &s, ts, None);
                }
            }
            Kind::Arrow { a, b, head } => {
                let len = (*b - *a).hypot();
                if len < 1e-6 {
                    return;
                }
                let dir = (*b - *a) / len;
                let normal = kurbo::Vec2::new(-dir.y, dir.x);
                let head = arrow_head(*head, st.width).min(len);
                let back = *b - dir * head;
                let paint = solid(st.stroke, opacity);
                // The line ends inside the head so its round cap does not show at the tip.
                if let Some(path) = polyline(&[*a, *b - dir * head * 0.6], false) {
                    let s = stroke(st.width as f32, LineCap::Round, LineJoin::Round);
                    pm.stroke_path(&path, &paint, &s, ts, None);
                }
                let tri = [*b, back + normal * head * 0.5, back - normal * head * 0.5];
                if let Some(path) = polyline(&tri, true) {
                    pm.fill_path(&path, &paint, FillRule::Winding, ts, None);
                }
            }
            Kind::Rect { size, radius } => {
                if let Some(path) = rect_path(*size, *radius) {
                    let join = if *radius > 0.0 {
                        LineJoin::Round
                    } else {
                        LineJoin::Miter
                    };
                    fill_and_stroke(pm, &path, o, opacity, join, ts);
                }
            }
            Kind::Ellipse { size } => {
                let r = tiny_skia::Rect::from_xywh(0.0, 0.0, size.width as f32, size.height as f32);
                if let Some(path) = r.and_then(PathBuilder::from_oval) {
                    fill_and_stroke(pm, &path, o, opacity, LineJoin::Round, ts);
                }
            }
            Kind::RegularPolygon { sides, size } => {
                let points = crate::document::regular_points(*sides, *size);
                if let Some(path) = polyline(&points, true) {
                    fill_and_stroke(pm, &path, o, opacity, LineJoin::Miter, ts);
                }
            }
            Kind::Polygon { points, closed } => {
                if let Some(path) = polyline(points, *closed) {
                    if *closed && let Some(fill) = st.fill {
                        pm.fill_path(&path, &solid(fill, opacity), FillRule::EvenOdd, ts, None);
                    }
                    if st.width > 0.0 {
                        let s = stroke(st.width as f32, LineCap::Round, LineJoin::Round);
                        pm.stroke_path(&path, &solid(st.stroke, opacity), &s, ts, None);
                    }
                }
            }
            Kind::Text { content, .. } => {
                let look = o.kind.text_look().unwrap();
                if let Some(fill) = st.fill {
                    let b = o.text_box();
                    if let Some(path) = tiny_skia::Rect::from_ltrb(
                        b.x0 as f32,
                        b.y0 as f32,
                        b.x1 as f32,
                        b.y1 as f32,
                    )
                    .map(PathBuilder::from_rect)
                    {
                        pm.fill_path(&path, &solid(fill, opacity), FillRule::Winding, ts, None);
                    }
                }
                for path in [text::path(content, &look), text::underline(content, &look)]
                    .into_iter()
                    .flatten()
                {
                    pm.fill_path(
                        &path,
                        &solid(st.stroke, opacity),
                        FillRule::Winding,
                        ts,
                        None,
                    );
                }
            }
            Kind::Image { asset, size } => {
                let Some(img) = doc.assets.get(asset).cloned() else {
                    return;
                };
                let Some(src) = self.image(&img) else {
                    return;
                };
                let (iw, ih) = (src.width() as f64, src.height() as f64);
                let ts = ts.pre_scale((size.width / iw) as f32, (size.height / ih) as f32);
                // Shrinking a lot looks better with bicubic; enlarging shows the pixels.
                let shrink = ts.get_scale().0.min(ts.get_scale().1);
                let quality = if shrink > 1.5 {
                    FilterQuality::Nearest
                } else if shrink < 0.6 {
                    FilterQuality::Bicubic
                } else {
                    FilterQuality::Bilinear
                };
                let paint = PixmapPaint {
                    opacity,
                    blend_mode: BlendMode::SourceOver,
                    quality,
                };
                pm.draw_pixmap(0, 0, src.as_ref(), &paint, ts, None);
            }
        }
    }
}

fn fill_and_stroke(
    pm: &mut Pixmap,
    path: &tiny_skia::Path,
    o: &Object,
    opacity: f32,
    join: LineJoin,
    ts: Transform,
) {
    if let Some(fill) = o.style.fill {
        pm.fill_path(path, &solid(fill, opacity), FillRule::Winding, ts, None);
    }
    if o.style.width > 0.0 {
        let s = stroke(o.style.width as f32, LineCap::Butt, join);
        pm.stroke_path(path, &solid(o.style.stroke, opacity), &s, ts, None);
    }
}

pub fn to_transform(a: Affine) -> Transform {
    let [a, b, c, d, e, f] = a.as_coeffs().map(|v| v as f32);
    Transform::from_row(a, b, c, d, e, f)
}

fn solid(c: Rgba, opacity: f32) -> Paint<'static> {
    let [r, g, b, a] = c.0;
    let mut p = Paint::default();
    p.set_color(Color::from_rgba8(
        r,
        g,
        b,
        (a as f32 * opacity).round() as u8,
    ));
    p.anti_alias = true;
    p
}

fn stroke(width: f32, cap: LineCap, join: LineJoin) -> Stroke {
    Stroke {
        width,
        line_cap: cap,
        line_join: join,
        ..Stroke::default()
    }
}

fn polyline(points: &[Point], closed: bool) -> Option<tiny_skia::Path> {
    let (first, rest) = points.split_first()?;
    let mut pb = PathBuilder::new();
    pb.move_to(first.x as f32, first.y as f32);
    for p in rest {
        pb.line_to(p.x as f32, p.y as f32);
    }
    if closed {
        pb.close();
    }
    pb.finish()
}

/// Freehand stroke through the midpoints of its samples (smooth, no corners).
fn smooth_path(points: &[Point]) -> Option<tiny_skia::Path> {
    if points.len() < 3 {
        return polyline(points, false);
    }
    let mut pb = PathBuilder::new();
    pb.move_to(points[0].x as f32, points[0].y as f32);
    for w in points[1..].windows(2) {
        let mid = w[0].midpoint(w[1]);
        pb.quad_to(w[0].x as f32, w[0].y as f32, mid.x as f32, mid.y as f32);
    }
    let last = points[points.len() - 1];
    pb.line_to(last.x as f32, last.y as f32);
    pb.finish()
}

fn rect_path(size: Size, radius: f64) -> Option<tiny_skia::Path> {
    let (w, h) = (size.width as f32, size.height as f32);
    let r = (radius as f32)
        .min(w.abs() / 2.0)
        .min(h.abs() / 2.0)
        .max(0.0);
    if r <= 0.0 {
        return tiny_skia::Rect::from_xywh(0.0, 0.0, w, h).map(PathBuilder::from_rect);
    }
    // Cubic approximation of a quarter circle.
    let k = r * (1.0 - 0.552_284_8);
    let mut pb = PathBuilder::new();
    pb.move_to(r, 0.0);
    pb.line_to(w - r, 0.0);
    pb.cubic_to(w - k, 0.0, w, k, w, r);
    pb.line_to(w, h - r);
    pb.cubic_to(w, h - k, w - k, h, w - r, h);
    pb.line_to(r, h);
    pb.cubic_to(k, h, 0.0, h - k, 0.0, h - r);
    pb.line_to(0.0, r);
    pb.cubic_to(0.0, k, k, 0.0, r, 0.0);
    pb.close();
    pb.finish()
}

/// Premultiplies an image for tiny-skia.
pub fn to_pixmap(img: &RgbaImage) -> Option<Pixmap> {
    let mut data = img.as_raw().clone();
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3] as u16;
        if a < 255 {
            for c in &mut px[..3] {
                *c = ((*c as u16 * a + 127) / 255) as u8;
            }
        }
    }
    Pixmap::from_vec(data, IntSize::from_wh(img.width(), img.height())?)
}

/// Back to straight alpha.
pub fn to_image(pm: Pixmap) -> RgbaImage {
    let (w, h) = (pm.width(), pm.height());
    RgbaImage::from_raw(w, h, pm.take_demultiplied()).expect("pixmap size")
}

/// The whole canvas at its size (times `scale`), as an image.
pub fn render_document(doc: &Document, scale: f64) -> Option<RgbaImage> {
    let job = Job {
        region: Rect::new(0.0, 0.0, doc.width as f64, doc.height as f64),
        scale,
        only: None,
        background: true,
    };
    Renderer::default().render(doc, &job).map(to_image)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::Style;

    #[test]
    fn renders_background_and_eraser() {
        let mut doc = Document::new(40, 20, Rgba::WHITE);
        let id = doc.new_id();
        doc.objects.push(Object::new(
            id,
            Kind::Rect {
                size: Size::new(40.0, 20.0),
                radius: 0.0,
            },
            Style {
                fill: Some(Rgba([255, 0, 0, 255])),
                width: 0.0,
                ..Style::default()
            },
            Affine::IDENTITY,
        ));
        let id = doc.new_id();
        doc.objects.push(Object::new(
            id,
            Kind::Stroke {
                points: vec![Point::new(30.0, 0.0), Point::new(30.0, 20.0)],
                mode: StrokeMode::Eraser,
            },
            Style {
                width: 6.0,
                ..Style::default()
            },
            Affine::IDENTITY,
        ));
        let img = render_document(&doc, 1.0).unwrap();
        assert_eq!(img.get_pixel(5, 10).0, [255, 0, 0, 255]);
        // Erased down to the white background, not to transparent.
        assert_eq!(img.get_pixel(30, 10).0, [255, 255, 255, 255]);
    }
}

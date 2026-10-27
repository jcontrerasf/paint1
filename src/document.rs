//! The drawing: a canvas of a given size and background, and a list of editable objects
//! (back to front). Images are shared assets so undo snapshots stay cheap.

use std::collections::BTreeMap;
use std::sync::Arc;

use image::RgbaImage;
use kurbo::{Affine, Point, Rect, Size, Vec2};
use serde::{Deserialize, Serialize};

use crate::text;

pub type ObjectId = u64;
pub type AssetId = u64;

/// sRGB color with straight (not premultiplied) alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    pub const BLACK: Rgba = Rgba([0, 0, 0, 255]);
    pub const WHITE: Rgba = Rgba([255, 255, 255, 255]);
    pub const TRANSPARENT: Rgba = Rgba([0, 0, 0, 0]);

    pub fn with_alpha(self, a: u8) -> Rgba {
        let [r, g, b, _] = self.0;
        Rgba([r, g, b, a])
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    /// Outline color (and the color of text, pencil strokes, lines and arrows).
    pub stroke: Rgba,
    /// Outline width in local units.
    pub width: f64,
    /// Fill of closed shapes; the box behind a text.
    pub fill: Option<Rgba>,
    /// 0..=1, multiplies everything the object draws.
    pub opacity: f32,
}

impl Default for Style {
    fn default() -> Self {
        Style {
            stroke: Rgba([0x20, 0x20, 0x20, 255]),
            width: 4.0,
            fill: None,
            opacity: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StrokeMode {
    Pencil,
    /// Wide, translucent, flat ends.
    Highlighter,
    /// Erases what is under it (but stays an object that can be edited or deleted).
    Eraser,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// What an object is. Geometry is in local coordinates; [`Object::transform`] places it.
/// Box-like kinds (rect, ellipse, text, image) span `(0, 0)..size`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Kind {
    Stroke {
        points: Vec<Point>,
        mode: StrokeMode,
    },
    Line {
        a: Point,
        b: Point,
    },
    Arrow {
        a: Point,
        b: Point,
        /// Length of the head.
        head: f64,
    },
    Rect {
        size: Size,
        radius: f64,
    },
    Ellipse {
        size: Size,
    },
    Polygon {
        points: Vec<Point>,
        closed: bool,
    },
    /// Regular polygon stretched to fill `(0, 0)..size`.
    RegularPolygon {
        sides: u32,
        size: Size,
    },
    Text {
        content: String,
        size: f64,
        bold: bool,
        align: TextAlign,
        #[serde(default)]
        italic: bool,
        #[serde(default)]
        underline: bool,
        /// Installed font family; empty = the bundled Inter.
        #[serde(default)]
        font: String,
        /// Space around the text in its box, as a fraction of `size` (so it scales with it).
        #[serde(default = "default_text_margin")]
        margin: f64,
    },
    Image {
        asset: AssetId,
        size: Size,
    },
}

pub const DEFAULT_TEXT_MARGIN: f64 = 0.2;

fn default_text_margin() -> f64 {
    DEFAULT_TEXT_MARGIN
}

impl Kind {
    /// How a text is laid out (`None` for the other kinds).
    pub fn text_look(&self) -> Option<text::TextLook<'_>> {
        match self {
            Kind::Text {
                size,
                bold,
                align,
                italic,
                underline,
                font,
                ..
            } => Some(text::TextLook {
                family: font,
                size: *size,
                bold: *bold,
                italic: *italic,
                underline: *underline,
                align: *align,
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: ObjectId,
    /// Empty = automatic name (the kind, or the first line of a text).
    #[serde(default)]
    pub name: String,
    pub visible: bool,
    pub locked: bool,
    pub transform: Affine,
    pub style: Style,
    pub kind: Kind,
}

impl Object {
    pub fn new(id: ObjectId, kind: Kind, style: Style, transform: Affine) -> Object {
        Object {
            id,
            name: String::new(),
            visible: true,
            locked: false,
            transform,
            style,
            kind,
        }
    }

    /// Does the outline of this object get drawn? (Text and images have no outline.)
    pub fn has_outline(&self) -> bool {
        !matches!(self.kind, Kind::Text { .. } | Kind::Image { .. })
    }

    /// Can this object have a fill?
    pub fn can_fill(&self) -> bool {
        match &self.kind {
            Kind::Rect { .. }
            | Kind::Ellipse { .. }
            | Kind::RegularPolygon { .. }
            | Kind::Text { .. } => true,
            Kind::Polygon { closed, .. } => *closed,
            _ => false,
        }
    }

    /// Bounds of the geometry in local coordinates, without the outline width.
    pub fn local_rect(&self) -> Rect {
        match &self.kind {
            Kind::Stroke { points, .. } | Kind::Polygon { points, .. } => points_rect(points),
            Kind::Line { a, b } => Rect::from_points(*a, *b),
            Kind::Arrow { a, b, .. } => Rect::from_points(*a, *b),
            Kind::Rect { size, .. }
            | Kind::Ellipse { size }
            | Kind::RegularPolygon { size, .. }
            | Kind::Image { size, .. } => size.to_rect(),
            Kind::Text { content, .. } => {
                text::measure(content, &self.kind.text_look().unwrap()).to_rect()
            }
        }
    }

    /// How far the drawing goes outside [`Self::local_rect`] (outline, arrow head).
    fn local_pad(&self) -> f64 {
        let half = self.style.width / 2.0;
        match &self.kind {
            Kind::Arrow { head, .. } => arrow_head(*head, self.style.width).max(half),
            Kind::Stroke { mode, .. } => stroke_width(*mode, self.style.width) / 2.0,
            Kind::Text {
                size,
                italic,
                margin,
                ..
            } => {
                // Glyphs may reach a bit past their advances (more so slanted).
                let ink = size * if *italic { 0.25 } else { 0.05 };
                let fill = if self.style.fill.is_some() {
                    size * margin
                } else {
                    0.0
                };
                ink.max(fill)
            }
            Kind::Image { .. } => 0.0,
            // Sharp (miter) corners reach past half the width.
            Kind::Rect { radius, .. } if *radius <= 0.0 => half * std::f64::consts::SQRT_2,
            Kind::RegularPolygon { sides, .. } => {
                half / (std::f64::consts::PI / *sides.max(&3) as f64).cos()
            }
            _ => half,
        }
    }

    /// Box of a text in local coordinates: the text plus its margin when the box is filled.
    pub fn text_box(&self) -> Rect {
        let r = self.local_rect();
        match &self.kind {
            Kind::Text { size, margin, .. } if self.style.fill.is_some() => {
                r.inflate(size * margin, size * margin)
            }
            _ => r,
        }
    }

    /// Axis-aligned bounds of the object on the canvas, outline included.
    pub fn bounds(&self) -> Rect {
        let r = self
            .local_rect()
            .inflate(self.local_pad(), self.local_pad());
        self.transform.transform_rect_bbox(r)
    }

    /// Bounds on the canvas without the outline (what the selection frame hugs).
    pub fn shape_bounds(&self) -> Rect {
        self.transform.transform_rect_bbox(self.local_rect())
    }

    /// Applies a canvas-space transform (move, rotate, scale, flip) to the object.
    pub fn apply(&mut self, m: Affine) {
        self.transform = m * self.transform;
        self.normalize();
    }

    /// Moves the scale of the transform into the geometry when that is exact, so that
    /// scaling a shape changes its size, not its outline width.
    pub fn normalize(&mut self) {
        let [a, b, c, d, _, _] = self.transform.as_coeffs();
        let sx = a.hypot(b);
        let sy = c.hypot(d);
        let orthogonal = (a * c + b * d).abs() <= 1e-9 * sx.max(sy).max(1.0);
        if !orthogonal || sx < 1e-9 || sy < 1e-9 {
            return;
        }
        if (sx - 1.0).abs() < 1e-12 && (sy - 1.0).abs() < 1e-12 {
            return;
        }
        let baked = match &mut self.kind {
            Kind::Stroke { points, .. } | Kind::Polygon { points, .. } => {
                for p in points {
                    *p = Point::new(p.x * sx, p.y * sy);
                }
                true
            }
            Kind::Line { a, b } | Kind::Arrow { a, b, .. } => {
                *a = Point::new(a.x * sx, a.y * sy);
                *b = Point::new(b.x * sx, b.y * sy);
                true
            }
            Kind::Rect { size, .. }
            | Kind::Ellipse { size }
            | Kind::RegularPolygon { size, .. }
            | Kind::Image { size, .. } => {
                *size = Size::new(size.width * sx, size.height * sy);
                true
            }
            Kind::Text { size, .. } if ((sx - sy).abs() / sx) < 1e-6 => {
                *size *= sx;
                true
            }
            Kind::Text { .. } => false,
        };
        if baked {
            self.transform *= Affine::scale_non_uniform(1.0 / sx, 1.0 / sy);
        }
    }

    /// Rotation of the object in degrees (from the transform's x axis).
    pub fn rotation_deg(&self) -> f64 {
        let [a, b, ..] = self.transform.as_coeffs();
        b.atan2(a).to_degrees()
    }

    /// Is the canvas point `p` on the object? `tol` is in canvas units.
    pub fn hit(&self, p: Point, tol: f64) -> bool {
        if !self.bounds().inflate(tol, tol).contains(p) {
            return false;
        }
        let det = self.transform.determinant();
        if det.abs() < 1e-12 {
            return false;
        }
        let scale = det.abs().sqrt();
        let local = self.transform.inverse() * p;
        let tol = tol / scale;
        let half = self.style.width / 2.0 + tol;
        match &self.kind {
            Kind::Stroke { points, mode } => near_polyline(
                points,
                local,
                stroke_width(*mode, self.style.width) / 2.0 + tol,
            ),
            Kind::Line { a, b } => dist_segment(local, *a, *b) <= half,
            Kind::Arrow { a, b, head } => {
                dist_segment(local, *a, *b) <= half.max(arrow_head(*head, self.style.width) / 2.0)
            }
            Kind::Rect { size, .. } => {
                let r = size.to_rect();
                if self.style.fill.is_some() {
                    r.inflate(half, half).contains(local)
                } else {
                    r.inflate(half, half).contains(local)
                        && !r.inflate(-half, -half).contains(local)
                }
            }
            Kind::Ellipse { size } => {
                let c = size.to_rect().center();
                let (rx, ry) = (size.width / 2.0, size.height / 2.0);
                let norm = |extra: f64| {
                    let (ex, ey) = ((rx + extra).max(1e-6), (ry + extra).max(1e-6));
                    ((local.x - c.x) / ex).powi(2) + ((local.y - c.y) / ey).powi(2)
                };
                if self.style.fill.is_some() {
                    norm(half) <= 1.0
                } else {
                    norm(half) <= 1.0 && norm(-half) >= 1.0
                }
            }
            Kind::Polygon { points, closed } => {
                hit_polygon(points, *closed, self.style.fill.is_some(), local, half)
            }
            Kind::RegularPolygon { sides, size } => hit_polygon(
                &regular_points(*sides, *size),
                true,
                self.style.fill.is_some(),
                local,
                half,
            ),
            Kind::Text { .. } => self.text_box().inflate(tol, tol).contains(local),
            Kind::Image { .. } => self.local_rect().inflate(tol, tol).contains(local),
        }
    }
}

fn hit_polygon(points: &[Point], closed: bool, filled: bool, p: Point, half: f64) -> bool {
    let mut on_edge = near_polyline(points, p, half);
    if closed && points.len() > 2 {
        on_edge |= dist_segment(p, points[points.len() - 1], points[0]) <= half;
        if filled {
            on_edge |= point_in_polygon(points, p);
        }
    }
    on_edge
}

/// Allowed number of sides of a regular polygon.
pub const SIDES: std::ops::RangeInclusive<u32> = 3..=12;

/// Vertices of a regular polygon, the first at the top (even counts are turned half a step
/// so they sit on a flat side), stretched so their bounds are exactly `(0, 0)..size`.
pub fn regular_points(sides: u32, size: Size) -> Vec<Point> {
    let n = sides.clamp(*SIDES.start(), *SIDES.end());
    let step = std::f64::consts::TAU / n as f64;
    let start = -std::f64::consts::FRAC_PI_2 + if n.is_multiple_of(2) { step / 2.0 } else { 0.0 };
    let unit: Vec<Point> = (0..n)
        .map(|i| {
            let a = start + step * i as f64;
            Point::new(a.cos(), a.sin())
        })
        .collect();
    let b = points_rect(&unit);
    unit.iter()
        .map(|p| {
            Point::new(
                (p.x - b.x0) / b.width() * size.width,
                (p.y - b.y0) / b.height() * size.height,
            )
        })
        .collect()
}

/// Actual width of a freehand stroke (the highlighter is wider).
pub fn stroke_width(mode: StrokeMode, width: f64) -> f64 {
    match mode {
        StrokeMode::Highlighter => width * 3.0,
        _ => width,
    }
}

/// Length of an arrow head: at least a few times the line width.
pub fn arrow_head(head: f64, width: f64) -> f64 {
    head.max(width * 3.0)
}

fn points_rect(points: &[Point]) -> Rect {
    let mut it = points.iter();
    let Some(first) = it.next() else {
        return Rect::ZERO;
    };
    it.fold(Rect::from_points(*first, *first), |r, p| r.union_pt(*p))
}

fn dist_segment(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.hypot2();
    if len2 < 1e-12 {
        return (p - a).hypot();
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    (p - (a + ab * t)).hypot()
}

fn near_polyline(points: &[Point], p: Point, tol: f64) -> bool {
    match points {
        [] => false,
        [one] => (p - *one).hypot() <= tol,
        _ => points
            .windows(2)
            .any(|w| dist_segment(p, w[0], w[1]) <= tol),
    }
}

fn point_in_polygon(points: &[Point], p: Point) -> bool {
    let mut inside = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let (a, b) = (points[i], points[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Whole-canvas operations that also change its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanvasOp {
    RotateRight,
    RotateLeft,
    Rotate180,
    FlipH,
    FlipV,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Document {
    pub width: u32,
    pub height: u32,
    pub background: Rgba,
    /// Back to front.
    pub objects: Vec<Object>,
    pub assets: BTreeMap<AssetId, Arc<RgbaImage>>,
    pub next_id: u64,
}

impl Document {
    pub fn new(width: u32, height: u32, background: Rgba) -> Document {
        Document {
            width,
            height,
            background,
            objects: Vec::new(),
            assets: BTreeMap::new(),
            next_id: 1,
        }
    }

    pub fn new_id(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }

    pub fn index_of(&self, id: ObjectId) -> Option<usize> {
        self.objects.iter().position(|o| o.id == id)
    }

    pub fn get(&self, id: ObjectId) -> Option<&Object> {
        self.objects.iter().find(|o| o.id == id)
    }

    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut Object> {
        self.objects.iter_mut().find(|o| o.id == id)
    }

    pub fn add_asset(&mut self, image: Arc<RgbaImage>) -> AssetId {
        // The same image pasted twice is stored once.
        if let Some((&id, _)) = self.assets.iter().find(|(_, a)| Arc::ptr_eq(a, &image)) {
            return id;
        }
        let id = self.new_id();
        self.assets.insert(id, image);
        id
    }

    /// Adds an image as a new object of its natural size with its top-left at `at`.
    pub fn add_image(&mut self, image: Arc<RgbaImage>, at: Point) -> ObjectId {
        let size = Size::new(image.width() as f64, image.height() as f64);
        let asset = self.add_asset(image);
        let id = self.new_id();
        self.objects.push(Object::new(
            id,
            Kind::Image { asset, size },
            Style::default(),
            Affine::translate(at.to_vec2()),
        ));
        id
    }

    /// Drops assets no object uses.
    pub fn prune_assets(&mut self) {
        let used: Vec<AssetId> = self
            .objects
            .iter()
            .filter_map(|o| match o.kind {
                Kind::Image { asset, .. } => Some(asset),
                _ => None,
            })
            .collect();
        self.assets.retain(|id, _| used.contains(id));
    }

    /// Union of the bounds of these objects.
    pub fn bounds_of(&self, ids: &[ObjectId]) -> Option<Rect> {
        self.objects
            .iter()
            .filter(|o| ids.contains(&o.id))
            .map(Object::bounds)
            .reduce(|a, b| a.union(b))
    }

    /// Makes the canvas the (integer) rectangle `r`; objects keep their place on it.
    pub fn crop(&mut self, r: Rect) {
        let r = Rect::new(r.x0.floor(), r.y0.floor(), r.x1.ceil(), r.y1.ceil());
        let m = Affine::translate(Vec2::new(-r.x0, -r.y0));
        for o in &mut self.objects {
            o.apply(m);
        }
        self.width = (r.width() as u32).max(1);
        self.height = (r.height() as u32).max(1);
    }

    /// Scales the canvas and everything on it.
    pub fn scale(&mut self, sx: f64, sy: f64) {
        let m = Affine::scale_non_uniform(sx, sy);
        for o in &mut self.objects {
            o.apply(m);
        }
        self.width = ((self.width as f64 * sx).round() as u32).max(1);
        self.height = ((self.height as f64 * sy).round() as u32).max(1);
    }

    pub fn canvas_op(&mut self, op: CanvasOp) {
        let (w, h) = (self.width as f64, self.height as f64);
        // Canvas coordinates go right and down.
        let m = match op {
            CanvasOp::RotateRight => Affine::new([0.0, 1.0, -1.0, 0.0, h, 0.0]),
            CanvasOp::RotateLeft => Affine::new([0.0, -1.0, 1.0, 0.0, 0.0, w]),
            CanvasOp::Rotate180 => Affine::new([-1.0, 0.0, 0.0, -1.0, w, h]),
            CanvasOp::FlipH => Affine::new([-1.0, 0.0, 0.0, 1.0, w, 0.0]),
            CanvasOp::FlipV => Affine::new([1.0, 0.0, 0.0, -1.0, 0.0, h]),
        };
        for o in &mut self.objects {
            o.apply(m);
        }
        if matches!(op, CanvasOp::RotateRight | CanvasOp::RotateLeft) {
            std::mem::swap(&mut self.width, &mut self.height);
        }
    }

    /// Applies `op` to the given objects only, around the center of their bounds.
    pub fn objects_op(&mut self, ids: &[ObjectId], op: CanvasOp) {
        let Some(b) = self.bounds_of(ids) else {
            return;
        };
        let c = b.center().to_vec2();
        let local = match op {
            CanvasOp::RotateRight => Affine::rotate(std::f64::consts::FRAC_PI_2),
            CanvasOp::RotateLeft => Affine::rotate(-std::f64::consts::FRAC_PI_2),
            CanvasOp::Rotate180 => Affine::rotate(std::f64::consts::PI),
            CanvasOp::FlipH => Affine::FLIP_X,
            CanvasOp::FlipV => Affine::FLIP_Y,
        };
        self.transform_objects(ids, Affine::translate(c) * local * Affine::translate(-c));
    }

    pub fn transform_objects(&mut self, ids: &[ObjectId], m: Affine) {
        for o in self.objects.iter_mut().filter(|o| ids.contains(&o.id)) {
            o.apply(m);
        }
    }

    /// Topmost visible, unlocked object under `p`.
    pub fn pick(&self, p: Point, tol: f64) -> Option<ObjectId> {
        self.objects
            .iter()
            .rev()
            .filter(|o| o.visible && !o.locked)
            .find(|o| o.hit(p, tol))
            .map(|o| o.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Document {
        let mut d = Document::new(300, 200, Rgba::WHITE);
        let id = d.new_id();
        d.objects.push(Object::new(
            id,
            Kind::Rect {
                size: Size::new(50.0, 20.0),
                radius: 0.0,
            },
            Style::default(),
            Affine::translate((10.0, 30.0)),
        ));
        let id = d.new_id();
        d.objects.push(Object::new(
            id,
            Kind::Line {
                a: Point::new(0.0, 0.0),
                b: Point::new(100.0, 50.0),
            },
            Style::default(),
            Affine::IDENTITY,
        ));
        d
    }

    fn close(a: &Document, b: &Document) -> bool {
        a.width == b.width
            && a.height == b.height
            && a.objects.iter().zip(&b.objects).all(|(x, y)| {
                let (rx, ry) = (x.bounds(), y.bounds());
                (rx.x0 - ry.x0).abs() < 1e-6
                    && (rx.y0 - ry.y0).abs() < 1e-6
                    && (rx.x1 - ry.x1).abs() < 1e-6
                    && (rx.y1 - ry.y1).abs() < 1e-6
            })
    }

    #[test]
    fn four_right_rotations_are_identity() {
        let orig = sample();
        let mut d = orig.clone();
        for _ in 0..4 {
            d.canvas_op(CanvasOp::RotateRight);
        }
        assert!(close(&orig, &d));
        d.canvas_op(CanvasOp::RotateRight);
        assert_eq!((d.width, d.height), (200, 300));
        d.canvas_op(CanvasOp::RotateLeft);
        assert!(close(&orig, &d));
    }

    #[test]
    fn rotate_right_moves_top_left_to_top_right() {
        let mut d = sample();
        d.canvas_op(CanvasOp::RotateRight);
        let r = d.objects[0].shape_bounds();
        // Rect (10,30)-(60,50) in a 300x200 canvas -> x = 200 - y.
        assert!((r.x0 - 150.0).abs() < 1e-6 && (r.x1 - 170.0).abs() < 1e-6);
        assert!((r.y0 - 10.0).abs() < 1e-6 && (r.y1 - 60.0).abs() < 1e-6);
    }

    #[test]
    fn double_flip_is_identity() {
        let orig = sample();
        let mut d = orig.clone();
        d.canvas_op(CanvasOp::FlipH);
        d.canvas_op(CanvasOp::FlipH);
        d.canvas_op(CanvasOp::FlipV);
        d.canvas_op(CanvasOp::FlipV);
        assert!(close(&orig, &d));
    }

    #[test]
    fn scaling_bakes_into_geometry() {
        let mut d = sample();
        d.transform_objects(&[1], Affine::scale_non_uniform(2.0, 3.0));
        let o = &d.objects[0];
        assert_eq!(
            o.kind,
            Kind::Rect {
                size: Size::new(100.0, 60.0),
                radius: 0.0
            }
        );
        assert_eq!(o.transform, Affine::translate((20.0, 90.0)));
        assert_eq!(o.style.width, 4.0, "the outline keeps its width");
    }

    #[test]
    fn crop_translates_objects() {
        let mut d = sample();
        d.crop(Rect::new(5.0, 20.0, 105.0, 80.0));
        assert_eq!((d.width, d.height), (100, 60));
        let r = d.objects[0].shape_bounds();
        assert!((r.x0 - 5.0).abs() < 1e-9 && (r.y0 - 10.0).abs() < 1e-9);
    }

    #[test]
    fn fit_to_selection() {
        let mut d = sample();
        let b = d.bounds_of(&[1]).unwrap();
        d.crop(b);
        let r = d.objects[0].bounds();
        assert!(r.x0.abs() < 1.0 && r.y0.abs() < 1.0);
        // The miter corners reach half the width times √2 past the rect.
        assert_eq!((d.width, d.height), (56, 26));
    }

    #[test]
    fn regular_polygon_fills_its_box() {
        for n in SIDES {
            let pts = regular_points(n, Size::new(100.0, 50.0));
            assert_eq!(pts.len(), n as usize);
            let b = points_rect(&pts);
            assert!(b.x0.abs() < 1e-9 && b.y0.abs() < 1e-9, "{n}");
            assert!(
                (b.x1 - 100.0).abs() < 1e-9 && (b.y1 - 50.0).abs() < 1e-9,
                "{n}"
            );
        }
        // A square is axis-aligned: its corners are the corners of the box.
        let sq = regular_points(4, Size::new(10.0, 10.0));
        assert!(
            sq.iter()
                .all(|p| (p.x.abs() < 1e-9 || (p.x - 10.0).abs() < 1e-9)
                    && (p.y.abs() < 1e-9 || (p.y - 10.0).abs() < 1e-9))
        );
    }

    #[test]
    fn hit_testing() {
        let d = sample();
        // Outline of the unfilled rect, not its inside.
        assert_eq!(d.pick(Point::new(10.0, 40.0), 1.0), Some(1));
        assert_eq!(d.pick(Point::new(35.0, 40.0), 1.0), None);
        // On the line.
        assert_eq!(d.pick(Point::new(80.0, 40.0), 1.0), Some(2));
        assert_eq!(d.pick(Point::new(250.0, 150.0), 1.0), None);
    }
}

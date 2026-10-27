//! Pixel operations: flattening objects into an image, and cutting a piece out of an image
//! (rectangle and free selection).

use std::sync::Arc;

use image::RgbaImage;
use kurbo::{Affine, Point, Rect, Size};
use tiny_skia::{FillRule, Mask, PathBuilder, Transform};

use crate::document::{Document, Kind, Object, ObjectId, Style};
use crate::render::{Job, Renderer, to_image};

/// Replaces the visible objects among `ids` with one image of how they look together,
/// placed where the topmost of them was in the stacking order.
pub fn flatten(
    doc: &mut Document,
    renderer: &mut Renderer,
    ids: &[ObjectId],
    name: &str,
) -> Option<ObjectId> {
    let ids: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|o| o.visible && ids.contains(&o.id))
        .map(|o| o.id)
        .collect();
    let b = doc.bounds_of(&ids)?;
    // One extra pixel for anti-aliasing.
    let region = Rect::new(
        b.x0.floor() - 1.0,
        b.y0.floor() - 1.0,
        b.x1.ceil() + 1.0,
        b.y1.ceil() + 1.0,
    );
    let job = Job {
        region,
        scale: 1.0,
        only: Some(&ids),
        background: false,
    };
    let img = to_image(renderer.render(doc, &job)?);
    let top = doc.objects.iter().rposition(|o| ids.contains(&o.id))?;
    let asset = doc.add_asset(Arc::new(img));
    let id = doc.new_id();
    let mut o = Object::new(
        id,
        Kind::Image {
            asset,
            size: region.size(),
        },
        Style::default(),
        Affine::translate(region.origin().to_vec2()),
    );
    o.name = name.to_string();
    doc.objects.insert(top + 1, o);
    doc.objects.retain(|o| !ids.contains(&o.id));
    doc.prune_assets();
    Some(id)
}

/// Cuts the part of image `target` inside `area` (a polygon on the canvas) into a new image
/// object right above it; the hole left in `target` is transparent. Returns the new object.
pub fn lift(doc: &mut Document, target: ObjectId, area: &[Point], name: &str) -> Option<ObjectId> {
    if area.len() < 3 {
        return None;
    }
    let o = doc.get(target)?.clone();
    let Kind::Image { asset, size } = o.kind else {
        return None;
    };
    let img = doc.assets.get(&asset)?.clone();
    let (iw, ih) = img.dimensions();
    let (sx, sy) = (size.width / iw as f64, size.height / ih as f64);
    // Canvas -> pixels of the image.
    let to_px = (o.transform * Affine::scale_non_uniform(sx, sy)).inverse();
    let mut pb = PathBuilder::new();
    for (i, p) in area.iter().enumerate() {
        let q = to_px * *p;
        if i == 0 {
            pb.move_to(q.x as f32, q.y as f32);
        } else {
            pb.line_to(q.x as f32, q.y as f32);
        }
    }
    pb.close();
    let path = pb.finish()?;
    let mut mask = Mask::new(iw, ih)?;
    // Hard edges, as in Paint: the piece and the hole then add up exactly.
    mask.fill_path(&path, FillRule::Winding, false, Transform::identity());
    let m = mask.data();

    // Pixels the area touches.
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..ih {
        for x in 0..iw {
            if m[(y * iw + x) as usize] > 0 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x);
                y1 = y1.max(y);
            }
        }
    }
    if x0 > x1 || y0 > y1 {
        return None;
    }
    let (bw, bh) = (x1 - x0 + 1, y1 - y0 + 1);
    let scale = |a: u8, k: u8| ((a as u16 * k as u16 + 127) / 255) as u8;
    let piece = RgbaImage::from_fn(bw, bh, |x, y| {
        let (px, py) = (x0 + x, y0 + y);
        let mut c = *img.get_pixel(px, py);
        c.0[3] = scale(c.0[3], m[(py * iw + px) as usize]);
        c
    });
    let mut rest = (*img).clone();
    for (x, y, c) in rest.enumerate_pixels_mut() {
        let k = m[(y * iw + x) as usize];
        if k > 0 {
            c.0[3] = scale(c.0[3], 255 - k);
        }
    }

    let rest = doc.add_asset(Arc::new(rest));
    let piece = doc.add_asset(Arc::new(piece));
    let index = doc.index_of(target)?;
    if let Kind::Image { asset, .. } = &mut doc.objects[index].kind {
        *asset = rest;
    }
    let id = doc.new_id();
    let mut new = Object::new(
        id,
        Kind::Image {
            asset: piece,
            size: Size::new(bw as f64 * sx, bh as f64 * sy),
        },
        Style {
            opacity: o.style.opacity,
            ..Style::default()
        },
        o.transform * Affine::translate((x0 as f64 * sx, y0 as f64 * sy)),
    );
    new.name = name.to_string();
    doc.objects.insert(index + 1, new);
    doc.prune_assets();
    Some(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Rgba, StrokeMode};
    use crate::render::render_document;

    fn red_image_doc() -> (Document, ObjectId) {
        let mut d = Document::new(100, 80, Rgba::WHITE);
        let img = RgbaImage::from_pixel(40, 30, image::Rgba([255, 0, 0, 255]));
        let id = d.add_image(Arc::new(img), Point::new(10.0, 10.0));
        (d, id)
    }

    #[test]
    fn flatten_keeps_the_look() {
        let mut d = Document::new(120, 90, Rgba::WHITE);
        for (i, kind) in [
            Kind::Rect {
                size: Size::new(50.0, 40.0),
                radius: 6.0,
            },
            Kind::RegularPolygon {
                sides: 5,
                size: Size::new(60.0, 60.0),
            },
            Kind::Stroke {
                points: vec![Point::new(0.0, 0.0), Point::new(80.0, 70.0)],
                mode: StrokeMode::Eraser,
            },
        ]
        .into_iter()
        .enumerate()
        {
            let id = d.new_id();
            d.objects.push(Object::new(
                id,
                kind,
                Style {
                    fill: Some(Rgba([20, 120, 220, 255])),
                    width: 6.0,
                    ..Style::default()
                },
                Affine::translate((10.0 + i as f64 * 15.0, 12.0)),
            ));
        }
        let before = render_document(&d, 1.0).unwrap();
        let ids: Vec<ObjectId> = d.objects.iter().map(|o| o.id).collect();
        let id = flatten(&mut d, &mut Renderer::default(), &ids, "x").unwrap();
        assert_eq!(d.objects.len(), 1);
        assert!(!d.get(id).unwrap().locked);
        let after = render_document(&d, 1.0).unwrap();
        // Same up to rounding: half-transparent edge pixels pass through straight alpha.
        let off = before
            .pixels()
            .zip(after.pixels())
            .flat_map(|(a, b)| a.0.into_iter().zip(b.0).map(|(x, y)| x.abs_diff(y)))
            .max()
            .unwrap();
        assert!(off <= 16, "differs by {off}");
    }

    #[test]
    fn untouched_lift_looks_the_same() {
        let mut d = Document::new(100, 80, Rgba::WHITE);
        let img = RgbaImage::from_fn(40, 30, |x, y| {
            image::Rgba([x as u8 * 6, y as u8 * 8, 90, 255])
        });
        let img = d.add_image(Arc::new(img), Point::new(10.0, 10.0));
        let before = render_document(&d, 1.0).unwrap();
        let lasso = [
            Point::new(15.0, 12.0),
            Point::new(40.0, 20.0),
            Point::new(30.0, 35.0),
            Point::new(18.0, 30.0),
        ];
        lift(&mut d, img, &lasso, "sel").unwrap();
        assert_eq!(d.objects.len(), 2);
        assert_eq!(before, render_document(&d, 1.0).unwrap());
    }

    #[test]
    fn lift_moves_pixels_and_leaves_a_hole() {
        let (mut d, img) = red_image_doc();
        // Turn the image a bit: lifting must still follow its pixels.
        d.transform_objects(&[img], Affine::rotate_about(0.3, Point::new(30.0, 25.0)));
        let area = [
            Point::new(22.0, 18.0),
            Point::new(38.0, 18.0),
            Point::new(38.0, 30.0),
            Point::new(22.0, 30.0),
        ];
        let piece = lift(&mut d, img, &area, "sel").unwrap();
        // Moved away: the hole shows the white background, the piece is red elsewhere.
        d.transform_objects(&[piece], Affine::translate((50.0, 40.0)));
        let after = render_document(&d, 1.0).unwrap();
        assert_eq!(after.get_pixel(30, 24).0, [255, 255, 255, 255]);
        assert_eq!(after.get_pixel(80, 64).0, [255, 0, 0, 255]);
    }

    #[test]
    fn lift_outside_the_image_does_nothing() {
        let (mut d, img) = red_image_doc();
        let area = [
            Point::new(70.0, 60.0),
            Point::new(90.0, 60.0),
            Point::new(90.0, 75.0),
        ];
        assert!(lift(&mut d, img, &area, "sel").is_none());
        assert_eq!(d.objects.len(), 1);
    }
}

//! Copy and paste. Copied objects stay here (fully editable) and an image of them goes to
//! the system clipboard, so they can also be pasted in other programs. When that same image
//! comes back on paste, the objects are pasted instead of a flat image.

use std::collections::BTreeMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Arc;

use eframe::egui::ColorImage;
use image::RgbaImage;
use kurbo::{Affine, Vec2};

use crate::document::{AssetId, Document, Kind, Object, ObjectId};
use crate::render::{self, Job, Renderer};

/// How far each paste lands from the previous one.
const PASTE_OFFSET: f64 = 12.0;

#[derive(Default)]
pub struct Clipboard {
    objects: Vec<Object>,
    assets: BTreeMap<AssetId, Arc<RgbaImage>>,
    /// Fingerprint of the image put on the system clipboard.
    fingerprint: Option<u64>,
    pastes: u32,
}

/// Identifies an image well enough to recognize it after a trip through the system
/// clipboard (only opaque pixels: transparent ones may come back slightly different).
pub fn fingerprint(img: &ColorImage) -> u64 {
    let mut h = DefaultHasher::new();
    img.size.hash(&mut h);
    for px in &img.pixels {
        if px.a() == 255 {
            px.to_array().hash(&mut h);
        }
    }
    h.finish()
}

impl Clipboard {
    pub fn has_objects(&self) -> bool {
        !self.objects.is_empty()
    }

    /// Keeps the selected objects and returns their image for the system clipboard.
    pub fn copy(
        &mut self,
        doc: &Document,
        ids: &[ObjectId],
        renderer: &mut Renderer,
    ) -> Option<ColorImage> {
        self.objects = doc
            .objects
            .iter()
            .filter(|o| ids.contains(&o.id))
            .cloned()
            .collect();
        if self.objects.is_empty() {
            return None;
        }
        self.assets = self
            .objects
            .iter()
            .filter_map(|o| match o.kind {
                Kind::Image { asset, .. } => doc.assets.get(&asset).map(|a| (asset, a.clone())),
                _ => None,
            })
            .collect();
        self.pastes = 0;
        let b = doc.bounds_of(ids)?;
        let region = kurbo::Rect::new(b.x0.floor(), b.y0.floor(), b.x1.ceil(), b.y1.ceil());
        let job = Job {
            region,
            scale: 1.0,
            only: Some(ids),
            background: false,
        };
        let img = render::to_image(renderer.render(doc, &job)?);
        let color = ColorImage::from_rgba_unmultiplied(
            [img.width() as usize, img.height() as usize],
            img.as_raw(),
        );
        self.fingerprint = Some(fingerprint(&color));
        Some(color)
    }

    /// Is this the image we put on the system clipboard?
    pub fn is_ours(&self, img: &ColorImage) -> bool {
        self.has_objects() && self.fingerprint == Some(fingerprint(img))
    }

    /// Adds copies of the kept objects to `doc`, each paste a bit further. Returns their ids.
    pub fn paste(&mut self, doc: &mut Document) -> Vec<ObjectId> {
        self.pastes += 1;
        let offset = Vec2::splat(PASTE_OFFSET * self.pastes as f64);
        // Assets keep their ids unless the document uses that id for something else.
        let mut remap = BTreeMap::new();
        for (id, img) in &self.assets {
            let same = doc.assets.get(id).is_some_and(|a| Arc::ptr_eq(a, img));
            let new = if same {
                *id
            } else {
                doc.add_asset(img.clone())
            };
            remap.insert(*id, new);
        }
        let mut ids = Vec::new();
        for o in &self.objects {
            let mut o = o.clone();
            o.id = doc.new_id();
            o.locked = false;
            o.visible = true;
            if let Kind::Image { asset, .. } = &mut o.kind {
                *asset = remap.get(asset).copied().unwrap_or(*asset);
            }
            o.apply(Affine::translate(offset));
            ids.push(o.id);
            doc.objects.push(o);
        }
        ids
    }
}

/// An image from the system clipboard, as straight-alpha RGBA.
pub fn to_rgba(img: &ColorImage) -> Option<RgbaImage> {
    let [w, h] = img.size;
    let data: Vec<u8> = img
        .pixels
        .iter()
        .flat_map(|c| c.to_srgba_unmultiplied())
        .collect();
    RgbaImage::from_raw(w as u32, h as u32, data)
}

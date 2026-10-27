//! The editable `.paint1` format: a zip with `document.json` (canvas and objects) and the
//! images in `assets/<id>.png`.

use std::collections::BTreeMap;
use std::io::{Cursor, Read, Seek, Write};
use std::path::Path;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use zip::write::SimpleFileOptions;

use crate::document::{AssetId, Document, Object, Rgba};

pub const EXTENSION: &str = "paint1";
const FORMAT: &str = "paint1";
const VERSION: u32 = 1;
const DOCUMENT: &str = "document.json";

#[derive(Serialize, Deserialize)]
struct DocFile {
    format: String,
    version: u32,
    width: u32,
    height: u32,
    background: Rgba,
    next_id: u64,
    objects: Vec<Object>,
}

fn asset_name(id: AssetId) -> String {
    format!("assets/{id}.png")
}

pub fn write<W: Write + Seek>(doc: &Document, out: W) -> Result<(), String> {
    let mut doc = doc.clone();
    doc.prune_assets();
    let file = DocFile {
        format: FORMAT.into(),
        version: VERSION,
        width: doc.width,
        height: doc.height,
        background: doc.background,
        next_id: doc.next_id,
        objects: doc.objects.clone(),
    };
    let json = serde_json::to_vec_pretty(&file).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipWriter::new(out);
    let deflate = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file(DOCUMENT, deflate)
        .map_err(|e| e.to_string())?;
    zip.write_all(&json).map_err(|e| e.to_string())?;
    for (id, img) in &doc.assets {
        let mut png = Vec::new();
        img.write_to(&mut Cursor::new(&mut png), image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        zip.start_file(asset_name(*id), stored)
            .map_err(|e| e.to_string())?;
        zip.write_all(&png).map_err(|e| e.to_string())?;
    }
    zip.finish().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn read<R: Read + Seek>(input: R) -> Result<Document, String> {
    let mut zip = zip::ZipArchive::new(input).map_err(|e| e.to_string())?;
    let mut json = Vec::new();
    zip.by_name(DOCUMENT)
        .map_err(|e| e.to_string())?
        .read_to_end(&mut json)
        .map_err(|e| e.to_string())?;
    let file: DocFile = serde_json::from_slice(&json).map_err(|e| e.to_string())?;
    if file.format != FORMAT {
        return Err(format!("unknown format \"{}\"", file.format));
    }
    if file.version > VERSION {
        return Err(format!(
            "version {} is newer than this program",
            file.version
        ));
    }
    let mut assets = BTreeMap::new();
    for o in &file.objects {
        if let crate::document::Kind::Image { asset, .. } = o.kind
            && !assets.contains_key(&asset)
        {
            let mut png = Vec::new();
            zip.by_name(&asset_name(asset))
                .map_err(|e| format!("{}: {e}", asset_name(asset)))?
                .read_to_end(&mut png)
                .map_err(|e| e.to_string())?;
            let img = image::load_from_memory(&png).map_err(|e| e.to_string())?;
            assets.insert(asset, Arc::new(img.into_rgba8()));
        }
    }
    let max_id = file
        .objects
        .iter()
        .map(|o| o.id)
        .chain(assets.keys().copied())
        .max();
    Ok(Document {
        width: file.width.max(1),
        height: file.height.max(1),
        background: file.background,
        objects: file.objects,
        assets,
        next_id: file.next_id.max(max_id.unwrap_or(0) + 1),
    })
}

/// Saves through a temporary file and reads it back, so a failed save never leaves a
/// broken file in place of a good one.
pub fn save(doc: &Document, path: &Path) -> Result<(), String> {
    let tmp = path.with_extension(format!("{EXTENSION}.tmp"));
    let result = (|| {
        let f = std::fs::File::create(&tmp).map_err(|e| e.to_string())?;
        write(doc, std::io::BufWriter::new(f))?;
        let check = load(&tmp)?;
        if check.objects.len() != doc.objects.len() {
            return Err("verification failed".into());
        }
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

pub fn load(path: &Path) -> Result<Document, String> {
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    read(std::io::BufReader::new(f))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Kind, Style, TextAlign};
    use kurbo::{Affine, Point};

    #[test]
    fn round_trip() {
        let mut doc = Document::new(64, 48, Rgba::TRANSPARENT);
        let img = image::RgbaImage::from_fn(5, 4, |x, y| {
            image::Rgba([x as u8 * 40, y as u8 * 60, 7, 200])
        });
        doc.add_image(Arc::new(img), Point::new(3.0, 4.0));
        let id = doc.new_id();
        doc.objects.push(Object::new(
            id,
            Kind::Text {
                content: "Hola\nmundo".into(),
                size: 24.0,
                bold: true,
                align: TextAlign::Center,
                italic: true,
                underline: true,
                font: "DejaVu Serif".into(),
                margin: 0.5,
            },
            Style::default(),
            Affine::rotate(0.3).then_translate((10.0, 10.0).into()),
        ));
        let id = doc.new_id();
        let mut locked = Object::new(
            id,
            Kind::RegularPolygon {
                sides: 7,
                size: kurbo::Size::new(30.0, 20.0),
            },
            Style::default(),
            Affine::translate((-15.0, 5.0)),
        );
        locked.locked = true;
        doc.objects.push(locked);
        let mut buf = Cursor::new(Vec::new());
        write(&doc, &mut buf).unwrap();
        buf.set_position(0);
        let back = read(buf).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn old_texts_load_with_defaults() {
        let json = r#"{"Text":{"content":"Hola","size":20.0,"bold":false,"align":"Left"}}"#;
        let kind: Kind = serde_json::from_str(json).unwrap();
        let look = kind.text_look().unwrap();
        assert!(!look.italic && !look.underline && look.family.is_empty());
    }
}

//! Export to common image formats, optionally under a maximum file size: first lowering the
//! quality (lossy formats), then the resolution.

use std::io::Cursor;
use std::path::Path;

use image::codecs::gif::GifEncoder;
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
use image::imageops::FilterType;
use image::{ExtendedColorType, ImageEncoder, RgbaImage};
use serde::{Deserialize, Serialize};

use crate::document::Document;
use crate::render;

/// Lowest quality tried before reducing the resolution.
const MIN_QUALITY: u8 = 30;
/// Smallest side the resolution is reduced to.
const MIN_SIDE: u32 = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Format {
    #[default]
    Png,
    Jpeg,
    Webp,
    WebpLossless,
    Bmp,
    Gif,
    Tiff,
}

impl Format {
    pub const ALL: [Format; 7] = [
        Format::Png,
        Format::Jpeg,
        Format::Webp,
        Format::WebpLossless,
        Format::Bmp,
        Format::Gif,
        Format::Tiff,
    ];

    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Webp | Format::WebpLossless => "webp",
            Format::Bmp => "bmp",
            Format::Gif => "gif",
            Format::Tiff => "tiff",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Webp => "WebP",
            Format::WebpLossless => "WebP (lossless)",
            Format::Bmp => "BMP",
            Format::Gif => "GIF",
            Format::Tiff => "TIFF",
        }
    }

    /// Has a quality setting.
    pub fn lossy(self) -> bool {
        matches!(self, Format::Jpeg | Format::Webp)
    }

    /// Keeps transparency.
    pub fn alpha(self) -> bool {
        !matches!(self, Format::Jpeg | Format::Bmp)
    }

    pub fn from_extension(ext: &str) -> Option<Format> {
        Some(match ext.to_ascii_lowercase().as_str() {
            "png" => Format::Png,
            "jpg" | "jpeg" => Format::Jpeg,
            "webp" => Format::Webp,
            "bmp" => Format::Bmp,
            "gif" => Format::Gif,
            "tif" | "tiff" => Format::Tiff,
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SizeUnit {
    #[default]
    Kb,
    Mb,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub format: Format,
    /// 1..=100, for lossy formats.
    pub quality: u8,
    pub limit: bool,
    pub limit_value: f64,
    pub limit_unit: SizeUnit,
    /// Size of the output relative to the canvas, in percent.
    pub scale: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            format: Format::Png,
            quality: 90,
            limit: false,
            limit_value: 500.0,
            limit_unit: SizeUnit::Kb,
            scale: 100.0,
        }
    }
}

impl Settings {
    pub fn max_bytes(&self) -> Option<u64> {
        self.limit.then(|| {
            let unit = match self.limit_unit {
                SizeUnit::Kb => 1024.0,
                SizeUnit::Mb => 1024.0 * 1024.0,
            };
            (self.limit_value * unit).max(1.0) as u64
        })
    }
}

/// What got written.
#[derive(Debug, Clone)]
pub struct Encoded {
    pub bytes: Vec<u8>,
    /// Quality used, for lossy formats.
    pub quality: Option<u8>,
    pub width: u32,
    pub height: u32,
}

/// Composites over white (for formats without transparency).
fn flatten(img: &RgbaImage) -> image::RgbImage {
    image::RgbImage::from_fn(img.width(), img.height(), |x, y| {
        let [r, g, b, a] = img.get_pixel(x, y).0;
        let a = a as u16;
        let over = |c: u8| ((c as u16 * a + 255 * (255 - a) + 127) / 255) as u8;
        image::Rgb([over(r), over(g), over(b)])
    })
}

pub fn encode(img: &RgbaImage, format: Format, quality: u8) -> Result<Vec<u8>, String> {
    let (w, h) = img.dimensions();
    let mut out = Vec::new();
    let err = |e: image::ImageError| e.to_string();
    match format {
        Format::Png => {
            PngEncoder::new_with_quality(&mut out, CompressionType::Best, PngFilter::Adaptive)
                .write_image(img.as_raw(), w, h, ExtendedColorType::Rgba8)
                .map_err(err)?
        }
        Format::Jpeg => {
            let rgb = flatten(img);
            JpegEncoder::new_with_quality(&mut out, quality.clamp(1, 100))
                .write_image(rgb.as_raw(), w, h, ExtendedColorType::Rgb8)
                .map_err(err)?
        }
        Format::Webp => {
            let mem =
                webp::Encoder::from_rgba(img.as_raw(), w, h).encode(quality.clamp(1, 100) as f32);
            out.extend_from_slice(&mem);
        }
        Format::WebpLossless => {
            let mem = webp::Encoder::from_rgba(img.as_raw(), w, h).encode_lossless();
            out.extend_from_slice(&mem);
        }
        Format::Bmp => {
            let rgb = flatten(img);
            rgb.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Bmp)
                .map_err(err)?
        }
        Format::Gif => {
            let mut enc = GifEncoder::new_with_speed(&mut out, 10);
            enc.encode(img.as_raw(), w, h, ExtendedColorType::Rgba8)
                .map_err(err)?;
        }
        Format::Tiff => img
            .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Tiff)
            .map_err(err)?,
    }
    if out.is_empty() {
        return Err("encoder produced no data".into());
    }
    Ok(out)
}

/// Best quality (at most `max_q`) whose file fits in `limit`; `Err(smallest size)` if none.
fn fit_quality(
    img: &RgbaImage,
    format: Format,
    max_q: u8,
    limit: u64,
) -> Result<Result<(Vec<u8>, u8), u64>, String> {
    let first = encode(img, format, max_q)?;
    if first.len() as u64 <= limit {
        return Ok(Ok((first, max_q)));
    }
    let (mut lo, mut hi) = (MIN_QUALITY.min(max_q), max_q.saturating_sub(1));
    let mut best = None;
    let mut smallest = first.len() as u64;
    while lo <= hi {
        let mid = lo + (hi - lo) / 2;
        let b = encode(img, format, mid)?;
        smallest = smallest.min(b.len() as u64);
        if b.len() as u64 <= limit {
            best = Some((b, mid));
            lo = mid + 1;
        } else if mid == 0 {
            break;
        } else {
            hi = mid - 1;
        }
    }
    Ok(best.ok_or(smallest))
}

/// Encodes `img`; with a `limit`, lowers quality and then resolution until it fits.
pub fn encode_with_limit(
    img: &RgbaImage,
    format: Format,
    quality: u8,
    limit: Option<u64>,
) -> Result<Encoded, String> {
    let Some(limit) = limit else {
        return Ok(Encoded {
            bytes: encode(img, format, quality)?,
            quality: format.lossy().then_some(quality),
            width: img.width(),
            height: img.height(),
        });
    };
    let mut current = img.clone();
    loop {
        let size = if format.lossy() {
            match fit_quality(&current, format, quality, limit)? {
                Ok((bytes, q)) => {
                    return Ok(Encoded {
                        bytes,
                        quality: Some(q),
                        width: current.width(),
                        height: current.height(),
                    });
                }
                Err(smallest) => smallest,
            }
        } else {
            let bytes = encode(&current, format, quality)?;
            if bytes.len() as u64 <= limit {
                return Ok(Encoded {
                    bytes,
                    quality: None,
                    width: current.width(),
                    height: current.height(),
                });
            }
            bytes.len() as u64
        };
        let (w, h) = current.dimensions();
        if w <= MIN_SIDE || h <= MIN_SIDE {
            return Err(format!("cannot fit in {limit} bytes"));
        }
        // File size grows roughly with the area: aim a bit under the limit.
        let factor = ((limit as f64 / size as f64).sqrt() * 0.95).clamp(0.5, 0.92);
        let nw = ((w as f64 * factor).round() as u32).max(MIN_SIDE);
        let nh = ((h as f64 * factor).round() as u32).max(MIN_SIDE);
        // Always from the original, so quality does not degrade step by step.
        current = image::imageops::resize(img, nw, nh, FilterType::Triangle);
    }
}

/// Renders the document as `settings` say and writes it to `path`.
pub fn export(doc: &Document, settings: &Settings, path: &Path) -> Result<Encoded, String> {
    let scale = (settings.scale / 100.0).clamp(0.01, 10.0);
    let img = render::render_document(doc, scale).ok_or("image too large")?;
    let encoded = encode_with_limit(
        &img,
        settings.format,
        settings.quality,
        settings.max_bytes(),
    )?;
    std::fs::write(path, &encoded.bytes).map_err(|e| e.to_string())?;
    Ok(encoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Noise compresses badly: a hard case for the size limit.
    fn noisy(w: u32, h: u32) -> RgbaImage {
        let mut seed = 12345u32;
        RgbaImage::from_fn(w, h, |x, y| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            let n = (seed & 0x3f) as u8;
            image::Rgba([
                (x % 256) as u8 / 2 + n,
                (y % 256) as u8 / 2 + n,
                128 + n,
                255,
            ])
        })
    }

    #[test]
    fn limit_is_respected() {
        let img = noisy(2000, 1500);
        let limit = 100 * 1024;
        for format in [Format::Jpeg, Format::Webp, Format::Png] {
            let e = encode_with_limit(&img, format, 90, Some(limit)).unwrap();
            assert!(
                e.bytes.len() as u64 <= limit,
                "{format:?}: {} bytes",
                e.bytes.len()
            );
            let back = image::load_from_memory(&e.bytes).unwrap();
            assert_eq!((back.width(), back.height()), (e.width, e.height));
            if format == Format::Png {
                assert!(e.width < 2000, "PNG can only shrink");
            }
        }
    }

    #[test]
    fn every_format_encodes() {
        let img = noisy(40, 30);
        for format in Format::ALL {
            let bytes = encode(&img, format, 80).unwrap();
            let back = image::load_from_memory(&bytes).unwrap();
            assert_eq!(back.width(), 40, "{format:?}");
        }
    }
}

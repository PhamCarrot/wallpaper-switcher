//! Shared still selection. A poster describes only its stored still, never live frames.
use crate::scanner::{WallpaperItem, WallpaperSource};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Provenance {
    Image,
    ExistingPoster,
}
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
/// Cheap scheduling stamp; this is not a content hash.
pub struct Fingerprint {
    pub bytes: u64,
    pub modified: SystemTime,
}
impl Fingerprint {
    pub(crate) fn capture(path: &std::path::Path) -> Option<Self> {
        let meta = path.metadata().ok()?;
        if !meta.is_file() {
            return None;
        }
        Some(Self {
            bytes: meta.len(),
            modified: meta.modified().ok()?,
        })
    }

    pub(crate) fn matches(&self, path: &std::path::Path) -> bool {
        Self::capture(path).as_ref() == Some(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaStill {
    pub item_id: String,
    pub catalog_revision: u64,
    pub path: PathBuf,
    pub fingerprint: Fingerprint,
    pub provenance: Provenance,
}
pub(crate) fn is_gif(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gif"))
}

pub fn supported(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "bmp" | "webp" | "gif"
        )
    })
}
/// Still decodes share colour analysis's edge, pixel and allocation bounds, so
/// one oversized file cannot hold gigabytes on a poster or Apply worker.
pub(crate) const MAX_DECODE_EDGE: u32 = 16_384;
pub(crate) const MAX_DECODE_PIXELS: u64 = 32 * 1024 * 1024;
const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;

pub(crate) fn decode_bounded(path: &Path) -> image::ImageResult<image::DynamicImage> {
    use image::ImageDecoder;
    let mut decoder = image::ImageReader::open(path)?
        .with_guessed_format()?
        .into_decoder()?;
    let (width, height) = decoder.dimensions();
    if width == 0
        || height == 0
        || width > MAX_DECODE_EDGE
        || height > MAX_DECODE_EDGE
        || u64::from(width) * u64::from(height) > MAX_DECODE_PIXELS
    {
        return Err(image::ImageError::Limits(
            image::error::LimitError::from_kind(image::error::LimitErrorKind::DimensionError),
        ));
    }
    let mut limits = image::Limits::default();
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    decoder.set_limits(limits)?;
    image::DynamicImage::from_decoder(decoder)
}

impl MediaStill {
    pub fn is_current(&self) -> bool {
        self.fingerprint.matches(&self.path)
    }
    pub fn select(item: &WallpaperItem, catalog_revision: u64) -> Option<Self> {
        for (path, mut provenance) in Self::candidates(item) {
            let Some(fingerprint) = Fingerprint::capture(path) else {
                continue;
            };
            if path == &item.file_target {
                provenance = Provenance::Image;
            }
            return Some(Self {
                item_id: item.id.clone(),
                catalog_revision,
                path: path.clone(),
                fingerprint,
                provenance,
            });
        }
        None
    }

    pub(crate) fn candidates(item: &WallpaperItem) -> impl Iterator<Item = (&PathBuf, Provenance)> {
        item.preview_path
            .iter()
            .map(|p| (p, Provenance::ExistingPoster))
            .chain(
                (item.source == WallpaperSource::LocalImage
                    || item.wallpaper_type.eq_ignore_ascii_case("image"))
                .then_some((&item.file_target, Provenance::Image)),
            )
            .filter(|(path, _)| supported(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png_header(path: &Path, width: u32, height: u32) {
        fn crc32(bytes: &[u8]) -> u32 {
            let mut crc = !0u32;
            for byte in bytes {
                crc ^= u32::from(*byte);
                for _ in 0..8 {
                    crc = if crc & 1 != 0 {
                        (crc >> 1) ^ 0xedb8_8320
                    } else {
                        crc >> 1
                    };
                }
            }
            !crc
        }
        let mut chunk = b"IHDR".to_vec();
        chunk.extend(width.to_be_bytes());
        chunk.extend(height.to_be_bytes());
        chunk.extend([8, 2, 0, 0, 0]);
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend(13u32.to_be_bytes());
        png.extend(&chunk);
        png.extend(crc32(&chunk).to_be_bytes());
        // Header metadata ends at the first image-data chunk; no pixels follow.
        for empty in [b"IDAT", b"IEND"] {
            png.extend(0u32.to_be_bytes());
            png.extend(empty);
            png.extend(crc32(empty).to_be_bytes());
        }
        std::fs::write(path, png).unwrap();
    }

    #[test]
    fn bounded_still_decode_refuses_oversized_headers_before_allocating() {
        let root = std::env::temp_dir().join(format!("bounded-still-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let ordinary = root.join("ordinary.png");
        image::RgbaImage::from_pixel(5, 3, image::Rgba([1, 2, 3, 255]))
            .save(&ordinary)
            .unwrap();
        let decoded = decode_bounded(&ordinary).unwrap();
        assert_eq!((decoded.width(), decoded.height()), (5, 3));
        for (name, width, height) in [
            ("edge.png", MAX_DECODE_EDGE + 1, 4),
            ("pixels.png", 12_000, 12_000),
        ] {
            let path = root.join(name);
            png_header(&path, width, height);
            let result = decode_bounded(&path);
            assert!(
                matches!(result, Err(image::ImageError::Limits(_))),
                "{name}: {result:?}"
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn w4_still_provenance_fingerprint_missing_and_payload_refusal() {
        let root = std::env::temp_dir().join(format!("w4-still-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("owned.png");
        image::RgbaImage::new(2, 3).save(&path).unwrap();
        let mut item = crate::scanner::scan_local_images(&[root.clone()]).remove(0);
        let still = MediaStill::select(&item, 7).unwrap();
        assert_eq!(still.catalog_revision, 7);
        assert_eq!(still.item_id, item.id);
        assert_eq!(still.provenance, Provenance::Image);
        assert_eq!(still.fingerprint.bytes, path.metadata().unwrap().len());
        item.source = WallpaperSource::WallpaperEngine;
        item.file_target = root.join("scene.pkg");
        item.preview_path = Some(path.clone());
        assert_eq!(
            MediaStill::select(&item, 8).unwrap().provenance,
            Provenance::ExistingPoster
        );
        item.wallpaper_type = "image".into();
        item.file_target = path.clone();
        item.preview_path = None;
        assert_eq!(
            MediaStill::select(&item, 9).unwrap().provenance,
            Provenance::Image
        );
        item.wallpaper_type = "scene".into();
        item.file_target = root.join("scene.pkg");
        item.preview_path = None;
        std::fs::write(&item.file_target, b"not a still").unwrap();
        assert!(MediaStill::select(&item, 9).is_none());
        item.preview_path = Some(root.join("missing.png"));
        assert!(MediaStill::select(&item, 10).is_none());
    }
}

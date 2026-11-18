//! Shared still selection. A poster describes only its stored still, never live frames.
use crate::scanner::{WallpaperItem, WallpaperSource};
use std::path::PathBuf;
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
pub fn supported(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "jpg" | "jpeg" | "png" | "bmp" | "webp" | "gif"
        )
    })
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

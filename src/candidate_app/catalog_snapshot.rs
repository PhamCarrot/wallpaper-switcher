//! Disposable derived catalog data. Never part of the v4 profile or provider authority.
use crate::{file_publication, library::CatalogEntry, scanner::WallpaperItem};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

const VERSION: u32 = 1;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_ITEMS: usize = 50_000;

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Snapshot {
    version: u32,
    roots: Vec<PathBuf>,
    defaults: bool,
    working_directory: Option<PathBuf>,
    items: Vec<Item>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Item {
    wallpaper: WallpaperItem,
    width: u32,
    height: u32,
    modified_unix: u64,
    source_tags: BTreeSet<String>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

fn working_directory(roots: &[PathBuf]) -> io::Result<Option<PathBuf>> {
    roots
        .iter()
        .any(|root| !root.is_absolute())
        .then(std::env::current_dir)
        .transpose()
}

impl Snapshot {
    pub(super) fn capture(
        roots: &[PathBuf],
        defaults: bool,
        wallpapers: &[WallpaperItem],
        entries: &[CatalogEntry],
    ) -> io::Result<Self> {
        if wallpapers.len() != entries.len() || wallpapers.len() > MAX_ITEMS {
            return Err(invalid("catalog snapshot item bound or pairing failed"));
        }
        let snapshot = Self {
            version: VERSION,
            roots: roots.to_vec(),
            defaults,
            working_directory: working_directory(roots)?,
            items: wallpapers
                .iter()
                .zip(entries)
                .map(|(wallpaper, entry)| Item {
                    wallpaper: wallpaper.clone(),
                    width: entry.width,
                    height: entry.height,
                    modified_unix: entry.modified_unix,
                    source_tags: entry.source_tags.clone(),
                })
                .collect(),
        };
        // The DTO deliberately omits effective tags and every resource/provider owner.
        for (item, entry) in snapshot.items.iter().zip(entries) {
            if item.wallpaper.id != entry.id
                || item.wallpaper.title != entry.title
                || item.wallpaper.wallpaper_type != entry.kind
                || item.wallpaper.folder_path != entry.folder
            {
                return Err(invalid(
                    "catalog snapshot entry does not match its wallpaper",
                ));
            }
        }
        snapshot.validate(roots, defaults)?;
        Ok(snapshot)
    }

    fn validate(&self, roots: &[PathBuf], defaults: bool) -> io::Result<()> {
        if self.version != VERSION
            || self.roots != roots
            || self.defaults != defaults
            || self.working_directory != working_directory(roots)?
            || self.items.len() > MAX_ITEMS
        {
            return Err(invalid(
                "catalog snapshot version, configuration or item bound mismatch",
            ));
        }
        let mut ids = BTreeSet::new();
        for item in &self.items {
            let wallpaper = &item.wallpaper;
            if wallpaper.id.trim().is_empty()
                || wallpaper.id.contains('\0')
                || !ids.insert(&wallpaper.id)
                || [
                    &wallpaper.folder_path,
                    &wallpaper.project_json_path,
                    &wallpaper.file_target,
                ]
                .into_iter()
                .chain(wallpaper.preview_path.iter())
                .any(|path| !path.is_absolute() || path.to_string_lossy().contains('\0'))
                || wallpaper
                    .scan_metadata
                    .as_ref()
                    .is_some_and(|m| m.tags != item.source_tags)
            {
                return Err(invalid(
                    "catalog snapshot has inconsistent identities, paths or source tags",
                ));
            }
        }
        Ok(())
    }

    pub(super) fn into_catalog(self) -> (Vec<WallpaperItem>, Vec<CatalogEntry>) {
        self.items
            .into_iter()
            .map(|item| {
                let wallpaper = item.wallpaper;
                let entry = CatalogEntry {
                    id: wallpaper.id.clone(),
                    title: wallpaper.title.clone(),
                    kind: wallpaper.wallpaper_type.clone(),
                    folder: wallpaper.folder_path.clone(),
                    width: item.width,
                    height: item.height,
                    modified_unix: item.modified_unix,
                    tags: item.source_tags.clone(),
                    source_tags: item.source_tags,
                };
                (wallpaper, entry)
            })
            .unzip()
    }
}

pub(super) fn load(path: &Path, roots: &[PathBuf], defaults: bool) -> io::Result<Snapshot> {
    // No repair, mkdir, media stat, tag parse or write is permitted on this path.
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
        return Err(invalid("catalog snapshot exceeds byte bound"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(invalid("catalog snapshot grew beyond byte bound"));
    }
    let snapshot: Snapshot = serde_json::from_slice(&bytes)?;
    snapshot.validate(roots, defaults)?;
    Ok(snapshot)
}

struct BoundedBytes(Vec<u8>);
impl Write for BoundedBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_BYTES.saturating_sub(self.0.len()) {
            return Err(invalid("catalog snapshot exceeds byte bound"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(super) fn save(path: &Path, snapshot: &Snapshot) -> Result<(), String> {
    save_with(path, snapshot, &mut file_publication::replace_existing_file)
}

fn save_with(
    path: &Path,
    snapshot: &Snapshot,
    replace: &mut impl FnMut(&Path, &Path, &Path) -> io::Result<()>,
) -> Result<(), String> {
    let mut bytes = BoundedBytes(Vec::new());
    serde_json::to_writer(&mut bytes, snapshot).map_err(|e| e.to_string())?;
    let stage = file_publication::unique_recovery_path(path, "catalog-stage");
    let mut created = false;
    let mut prepare = || -> io::Result<()> {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&stage)?;
        created = true;
        file.write_all(&bytes.0)?;
        file.flush()?;
        file.sync_all()?;
        load(&stage, &snapshot.roots, snapshot.defaults)?;
        Ok(())
    };
    if let Err(error) = prepare() {
        let cleanup = created.then(|| fs::remove_file(&stage));
        return Err(format!(
            "Catalog snapshot preparation failed: {error}; stage={} cleanup={cleanup:?}",
            stage.display()
        ));
    }
    match file_publication::publish_staged_file_with(&stage, path, "catalog-recovery", replace) {
        Ok(()) => Ok(()),
        Err(error) => {
            let cleanup = (!error.replace_file_called && !error.publication_succeeded)
                .then(|| fs::remove_file(&stage));
            Err(format!(
                "Catalog snapshot publication failed: {}; publication_succeeded={} replace_called={} stage={} destination={} recovery={:?} cleanup={cleanup:?}",
                error.error, error.publication_succeeded, error.replace_file_called,
                error.replacement.display(), error.destination.display(), error.recovery,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scanner;
    fn fixture() -> (PathBuf, Vec<PathBuf>, Vec<WallpaperItem>, Vec<CatalogEntry>) {
        static ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "p45-snapshot-{}-{}",
            std::process::id(),
            ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        let media = root.join("media");
        fs::create_dir(&media).unwrap();
        image::RgbaImage::new(3, 2)
            .save(media.join("owned.png"))
            .unwrap();
        let roots = vec![media];
        let wallpapers = scanner::scan_local_images(&roots);
        let entries = wallpapers
            .iter()
            .map(CatalogEntry::from_wallpaper)
            .collect();
        (root.join("snapshot.json"), roots, wallpapers, entries)
    }

    #[test]
    fn p45_roundtrip_is_read_only_preserves_precise_metadata_and_omits_effective_tags() {
        let (path, roots, mut wallpapers, mut entries) = fixture();
        wallpapers[0]
            .scan_metadata
            .as_mut()
            .unwrap()
            .tags
            .insert("source".into());
        entries[0].source_tags.insert("source".into());
        entries[0].tags.insert("local-only-never-serialized".into());
        let snapshot = Snapshot::capture(&roots, false, &wallpapers, &entries).unwrap();
        assert_eq!(
            snapshot.working_directory, None,
            "absolute scan roots are independent of launch cwd"
        );
        save(&path, &snapshot).unwrap();
        let bytes = fs::read(&path).unwrap();
        let modified = path.metadata().unwrap().modified().unwrap();
        assert!(!String::from_utf8_lossy(&bytes).contains("local-only-never-serialized"));
        // Loading does not require any source file, and never repairs/re-publishes.
        fs::remove_file(&wallpapers[0].file_target).unwrap();
        let (loaded, catalog) = load(&path, &roots, false).unwrap().into_catalog();
        assert_eq!(loaded, wallpapers);
        assert_eq!((catalog[0].width, catalog[0].height), (3, 2));
        assert_eq!(catalog[0].tags, entries[0].source_tags);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(path.metadata().unwrap().modified().unwrap(), modified);
        wallpapers[0].scan_metadata = None;
        let synthetic = Snapshot::capture(&roots, false, &wallpapers, &entries).unwrap();
        save(&path, &synthetic).unwrap();
        assert!(load(&path, &roots, false).unwrap().into_catalog().0[0]
            .scan_metadata
            .is_none());
    }

    #[test]
    fn p45_rejects_corrupt_version_size_config_identity_paths_and_tags_without_writes() {
        let (path, roots, wallpapers, entries) = fixture();
        let snapshot = Snapshot::capture(&roots, false, &wallpapers, &entries).unwrap();
        let good = serde_json::to_value(snapshot).unwrap();
        for case in [
            "malformed",
            "version",
            "roots",
            "defaults",
            "cwd",
            "duplicate",
            "id",
            "path",
            "tags",
            "items",
        ] {
            let mut value = good.clone();
            match case {
                "version" => value["version"] = 999.into(),
                "roots" => value["roots"] = serde_json::json!([]),
                "defaults" => value["defaults"] = true.into(),
                "cwd" => value["working_directory"] = "elsewhere".into(),
                "duplicate" => {
                    let item = value["items"][0].clone();
                    value["items"].as_array_mut().unwrap().push(item);
                }
                "id" => value["items"][0]["wallpaper"]["id"] = "".into(),
                "path" => value["items"][0]["wallpaper"]["file_target"] = "relative.png".into(),
                "tags" => value["items"][0]["source_tags"] = serde_json::json!(["wrong"]),
                "items" => value["items"] = serde_json::json!({}),
                _ => {}
            }
            let bytes = if case == "malformed" {
                b"{broken".to_vec()
            } else {
                serde_json::to_vec(&value).unwrap()
            };
            fs::write(&path, &bytes).unwrap();
            let modified = path.metadata().unwrap().modified().unwrap();
            assert!(load(&path, &roots, false).is_err(), "{case}");
            assert_eq!(fs::read(&path).unwrap(), bytes, "{case}");
            assert_eq!(
                path.metadata().unwrap().modified().unwrap(),
                modified,
                "{case}"
            );
        }
        File::create(&path)
            .unwrap()
            .set_len(MAX_BYTES as u64 + 1)
            .unwrap();
        assert!(load(&path, &roots, false)
            .unwrap_err()
            .to_string()
            .contains("byte bound"));
        assert_eq!(path.metadata().unwrap().len(), MAX_BYTES as u64 + 1);
        let missing = path.with_file_name("missing-parent").join("snapshot.json");
        assert!(load(&missing, &roots, false).is_err());
        assert!(!missing.parent().unwrap().exists());
    }

    #[test]
    fn p45_snapshot_bounds_capture_pairing_and_output_before_publication() {
        let (path, roots, wallpapers, mut entries) = fixture();
        entries[0].id = "wrong".into();
        assert!(Snapshot::capture(&roots, false, &wallpapers, &entries).is_err());
        assert!(Snapshot::capture(&roots, false, &wallpapers, &[]).is_err());
        let many = vec![wallpapers[0].clone(); MAX_ITEMS + 1];
        let many_entries = vec![CatalogEntry::from_wallpaper(&wallpapers[0]); MAX_ITEMS + 1];
        assert!(Snapshot::capture(&roots, false, &many, &many_entries).is_err());
        let entries = wallpapers
            .iter()
            .map(CatalogEntry::from_wallpaper)
            .collect::<Vec<_>>();
        let mut snapshot = Snapshot::capture(&roots, false, &wallpapers, &entries).unwrap();
        let relative_roots = vec![PathBuf::from("relative-media")];
        let mut relative =
            Snapshot::capture(&relative_roots, false, &wallpapers, &entries).unwrap();
        assert_eq!(
            relative.working_directory.as_ref(),
            Some(&std::env::current_dir().unwrap())
        );
        relative.working_directory = Some(PathBuf::from("wrong-working-directory"));
        assert!(relative.validate(&relative_roots, false).is_err());
        snapshot.items[0].wallpaper.title = "x".repeat(MAX_BYTES);
        assert!(save(&path, &snapshot).is_err());
        assert!(!path.exists());
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
    }

    #[test]
    fn p45_publication_failure_preserves_previous_and_reports_partial_success() {
        let (path, roots, wallpapers, entries) = fixture();
        let mut snapshot = Snapshot::capture(&roots, false, &wallpapers, &entries).unwrap();
        save(&path, &snapshot).unwrap();
        let previous = fs::read(&path).unwrap();
        snapshot.items[0].wallpaper.title = "new".into();
        let before = save_with(&path, &snapshot, &mut |_, _, _| {
            Err(file_publication::before_replace_file_error(
                io::Error::other("injected before replace"),
            ))
        })
        .unwrap_err();
        assert!(before.contains("publication_succeeded=false replace_called=false"));
        assert_eq!(fs::read(&path).unwrap(), previous);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 2);
        let attempted = save_with(&path, &snapshot, &mut |_, _, _| {
            Err(io::Error::other("injected attempted replace"))
        })
        .unwrap_err();
        assert!(attempted.contains("publication_succeeded=false replace_called=true"));
        assert_eq!(fs::read(&path).unwrap(), previous);
        assert_eq!(
            fs::read_dir(path.parent().unwrap()).unwrap().count(),
            3,
            "uncertain attempted stage is retained"
        );
        let partial = save_with(&path, &snapshot, &mut |stage, destination, recovery| {
            file_publication::replace_existing_file(stage, destination, recovery)?;
            fs::remove_file(recovery)?;
            fs::create_dir(recovery)?;
            Ok(())
        })
        .unwrap_err();
        assert!(partial.contains("publication_succeeded=true replace_called=true"));
        assert_eq!(
            load(&path, &roots, false).unwrap().into_catalog().0[0].title,
            "new"
        );
    }
}

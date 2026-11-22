use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// I/O can overlap even on one core. An explicit 1 is useful for slow-storage comparisons.
pub(crate) fn catalog_scan_threads(logical: usize, configured: Option<u8>) -> usize {
    configured.map_or_else(
        || if logical <= 1 { 2 } else { logical.clamp(4, 8) },
        |threads| usize::from(threads).clamp(1, 8),
    )
}

pub(crate) fn default_catalog_scan_threads() -> usize {
    configured_catalog_scan_threads(None)
}

pub(crate) fn configured_catalog_scan_threads(configured: Option<u8>) -> usize {
    catalog_scan_threads(crate::poster_worker::process_parallelism().0, configured)
}

/// Non-overlapping scan phases share this ceiling, including the calling worker.
/// Claims are bounded; ordered gathering keeps discovery precedence independent of I/O timing.
pub(crate) fn scan_ordered<T: Sync, R: Send>(
    inputs: &[T],
    threads: usize,
    cancelled: &(dyn Fn() -> bool + Sync),
    operation: impl Fn(&T) -> R + Sync,
) -> Vec<R> {
    let traced = crate::apply_trace::enabled();
    let started = std::time::Instant::now();
    let next = AtomicUsize::new(0);
    let active = AtomicUsize::new(0);
    let peak = AtomicUsize::new(0);
    let work = || {
        let mut results = Vec::new();
        while !cancelled() {
            let index = next.fetch_add(1, Ordering::Relaxed);
            if index >= inputs.len() || cancelled() {
                break;
            }
            if traced {
                peak.fetch_max(
                    active.fetch_add(1, Ordering::Relaxed) + 1,
                    Ordering::Relaxed,
                );
            }
            let result = operation(&inputs[index]);
            if traced {
                active.fetch_sub(1, Ordering::Relaxed);
            }
            if cancelled() {
                break;
            }
            results.push((index, result));
        }
        results
    };
    let mut results = std::thread::scope(|scope| {
        let handles: Vec<_> = (1..threads.clamp(1, 8).min(inputs.len()))
            .filter_map(|_| {
                // On thread allocation failure the remaining lanes still drain the claims.
                std::thread::Builder::new()
                    .name("catalog-io".into())
                    .spawn_scoped(scope, &work)
                    .ok()
            })
            .collect();
        let mut results = work();
        for handle in handles {
            results.extend(handle.join().expect("catalog I/O worker panicked"));
        }
        results
    });
    let stopped = cancelled();
    crate::apply_trace::event_lazy("catalog-io-phase", || {
        format!("io_threads={} candidates={} completed={} peak_active={} cancelled={stopped} elapsed_us={}",
            threads.clamp(1, 8), inputs.len(), results.len(), peak.load(Ordering::Relaxed), started.elapsed().as_micros())
    });
    if stopped {
        return Vec::new();
    }
    results.sort_unstable_by_key(|(index, _)| *index);
    results.into_iter().map(|(_, result)| result).collect()
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WallpaperSource {
    LocalImage,
    LocalVideo,
    WallpaperEngine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WallpaperItem {
    pub id: String,
    pub title: String,
    pub folder_path: PathBuf,
    pub project_json_path: PathBuf,
    pub file_target: PathBuf,
    pub preview_path: Option<PathBuf>,
    pub wallpaper_type: String,
    pub source: WallpaperSource,
    /// Captured by the scanner. None is reserved for synthetic/legacy callers.
    pub scan_metadata: Option<ScanMetadata>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceStamp {
    pub bytes: u64,
    pub modified: Option<std::time::SystemTime>,
}
impl SourceStamp {
    fn capture(path: &Path) -> Option<Self> {
        let metadata = path.metadata().ok()?;
        metadata.is_file().then(|| Self {
            bytes: metadata.len(),
            modified: metadata.modified().ok(),
        })
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanMetadata {
    pub tags: std::collections::BTreeSet<String>,
    pub project: Option<SourceStamp>,
    pub target: Option<SourceStamp>,
    pub preview: Option<SourceStamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(crate) video: Option<VideoStamp>,
}

/// Scheduling identity from a single open handle, including NTFS changes that
/// preserve last-write time. A volume without this information cannot seed disk reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct VideoStamp {
    bytes: u64,
    modified: u64,
    file: Option<VideoFileIdentity>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct VideoFileIdentity {
    volume: u64,
    id: [u8; 16],
    changed: i64,
}

#[cfg(test)]
thread_local! {
    pub(crate) static VIDEO_STAMP_READS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl VideoStamp {
    pub(crate) fn capture(path: &Path) -> Option<Self> {
        #[cfg(test)]
        VIDEO_STAMP_READS.with(|reads| reads.set(reads.get() + 1));
        use std::os::windows::{
            fs::{MetadataExt, OpenOptionsExt},
            io::AsRawHandle,
        };
        use windows_sys::Win32::{Foundation::HANDLE, Storage::FileSystem::*};
        let file = std::fs::OpenOptions::new()
            .access_mode(FILE_READ_ATTRIBUTES)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .open(path)
            .ok()?;
        let handle = file.as_raw_handle() as HANDLE;
        let metadata = file.metadata().ok()?;
        if !metadata.is_file() {
            return None;
        }
        let mut basic = std::mem::MaybeUninit::<FILE_BASIC_INFO>::uninit();
        let mut id = std::mem::MaybeUninit::<FILE_ID_INFO>::uninit();
        let identity = unsafe {
            if GetFileInformationByHandleEx(
                handle,
                FileBasicInfo,
                basic.as_mut_ptr().cast(),
                std::mem::size_of::<FILE_BASIC_INFO>() as u32,
            ) != 0
                && GetFileInformationByHandleEx(
                    handle,
                    FileIdInfo,
                    id.as_mut_ptr().cast(),
                    std::mem::size_of::<FILE_ID_INFO>() as u32,
                ) != 0
            {
                let basic = basic.assume_init();
                let id = id.assume_init();
                (basic.ChangeTime > 0 && id.FileId.Identifier != [0; 16]).then_some(
                    VideoFileIdentity {
                        volume: id.VolumeSerialNumber,
                        id: id.FileId.Identifier,
                        changed: basic.ChangeTime,
                    },
                )
            } else {
                None
            }
        };
        Some(Self {
            bytes: metadata.len(),
            modified: metadata.last_write_time(),
            file: identity,
        })
    }

    pub(crate) fn matches(&self, path: &Path) -> bool {
        Self::capture(path) == Some(*self)
    }

    pub(crate) fn cacheable(&self) -> bool {
        self.file.is_some()
    }
}

impl WallpaperItem {
    fn capture_metadata(&mut self, tags: std::collections::BTreeSet<String>) {
        self.scan_metadata = Some(ScanMetadata {
            tags,
            project: SourceStamp::capture(&self.project_json_path),
            target: SourceStamp::capture(&self.file_target),
            preview: self.preview_path.as_deref().and_then(SourceStamp::capture),
            video: self
                .poster_video_path()
                .and_then(|path| VideoStamp::capture(path)),
        });
    }

    pub(crate) fn poster_video_path(&self) -> Option<&PathBuf> {
        if self.source == WallpaperSource::LocalVideo {
            return Some(&self.file_target);
        }
        if self.source == WallpaperSource::WallpaperEngine {
            return self.preview_path.as_ref().filter(|path| {
                is_supported_video(path)
                    && (*path != &self.file_target
                        || self.wallpaper_type.eq_ignore_ascii_case("video"))
            });
        }
        None
    }

    pub(crate) fn video_stamp(&self) -> Option<VideoStamp> {
        match &self.scan_metadata {
            Some(metadata) => metadata.video,
            // Synthetic callers have no scanner snapshot. Only resolve this on
            // submission, after the unchanged-demand return.
            None => self
                .poster_video_path()
                .and_then(|path| VideoStamp::capture(path)),
        }
    }
    pub fn engine_target(&self) -> &Path {
        if self.source == WallpaperSource::WallpaperEngine
            && self.wallpaper_type.eq_ignore_ascii_case("scene")
        {
            &self.project_json_path
        } else {
            &self.file_target
        }
    }
}

#[derive(Deserialize)]
struct ProjectJson {
    title: Option<String>,
    file: Option<String>,
    preview: Option<String>,
    #[serde(rename = "type")]
    wp_type: Option<String>,
    category: Option<String>,
    tags: Option<serde_json::Value>,
}

fn find_steam_registry_path() -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER, KEY_READ, REG_SZ,
    };

    let subkey = "Software\\Valve\\Steam\0";
    let subkey_w: Vec<u16> = subkey.encode_utf16().collect();
    let mut hkey: HKEY = std::ptr::null_mut();

    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey_w.as_ptr(), 0, KEY_READ, &mut hkey) != 0
            || hkey.is_null()
        {
            return None;
        }
    }

    let value_name = "SteamPath\0";
    let value_name_w: Vec<u16> = value_name.encode_utf16().collect();
    let mut type_out = 0u32;
    let mut buffer: [u16; 512] = [0; 512];
    let mut bytes_len = (buffer.len() * std::mem::size_of::<u16>()) as u32;

    let status = unsafe {
        RegQueryValueExW(
            hkey,
            value_name_w.as_ptr(),
            std::ptr::null_mut(),
            &mut type_out,
            buffer.as_mut_ptr() as *mut u8,
            &mut bytes_len,
        )
    };

    unsafe {
        RegCloseKey(hkey);
    }

    if status != 0 || type_out != REG_SZ {
        return None;
    }

    let used = (bytes_len as usize / std::mem::size_of::<u16>())
        .min(buffer.len())
        .max(1);
    let string_end = buffer[..used].iter().position(|&c| c == 0).unwrap_or(used);
    let steam_path = String::from_utf16_lossy(&buffer[..string_end])
        .trim()
        .to_string();
    if steam_path.is_empty() {
        return None;
    }

    let mut path = PathBuf::from(steam_path.replace('/', "\\"));
    path = path.canonicalize().unwrap_or(path);
    if path.exists() {
        Some(path)
    } else {
        Some(PathBuf::from(steam_path.replace('/', "\\")))
    }
}

fn parse_libraryfolders_vdf(vdf_path: &std::path::Path, libraries: &mut Vec<PathBuf>) {
    let Ok(content) = fs::read_to_string(vdf_path) else {
        return;
    };

    for line in content.lines() {
        let trimmed = line.trim();
        if !trimmed.contains("\"path\"") {
            continue;
        }

        let parts: Vec<&str> = trimmed.split('"').collect();
        if parts.len() < 4 {
            continue;
        }

        let path_str = parts[3].replace(r"\\", r"\");
        let path = PathBuf::from(path_str.replace('/', "\\"));
        if path.exists() && !libraries.iter().any(|lib| lib == &path) {
            libraries.push(path);
        }
    }
}

fn find_steam_libraries() -> &'static [PathBuf] {
    static LIBRARIES: std::sync::OnceLock<Vec<PathBuf>> = std::sync::OnceLock::new();
    LIBRARIES.get_or_init(discover_steam_libraries)
}

fn discover_steam_libraries() -> Vec<PathBuf> {
    let mut libraries = Vec::new();

    for root in [
        find_steam_registry_path(),
        Some(PathBuf::from(r"C:\Program Files (x86)\Steam")),
        Some(PathBuf::from(r"C:\Program Files\Steam")),
    ] {
        let Some(root) = root else {
            continue;
        };

        if root.exists() && !libraries.iter().any(|lib| lib == &root) {
            libraries.push(root.clone());
        }

        let vdf = root.join("steamapps").join("libraryfolders.vdf");
        if vdf.exists() {
            parse_libraryfolders_vdf(&vdf, &mut libraries);
        }
    }

    libraries.into_iter().filter(|p| p.exists()).collect()
}

pub fn wallpaper_engine_install_dir() -> Option<PathBuf> {
    find_steam_libraries().iter().find_map(|library| {
        let directory = library
            .join("steamapps")
            .join("common")
            .join("wallpaper_engine");
        directory.is_dir().then_some(directory)
    })
}

pub(crate) fn known_wallpaper_engine_executable(executable: &Path) -> bool {
    let Some(parent) = executable
        .parent()
        .and_then(|parent| parent.canonicalize().ok())
    else {
        return false;
    };
    find_steam_libraries().iter().any(|library| {
        library
            .join("steamapps")
            .join("common")
            .join("wallpaper_engine")
            .canonicalize()
            .is_ok_and(|directory| directory == parent)
    })
}

pub fn wallpaper_engine_executable() -> Option<PathBuf> {
    let directory = wallpaper_engine_install_dir()?;
    ["wallpaper64.exe", "wallpaper32.exe"]
        .into_iter()
        .map(|name| directory.join(name))
        .find(|path| path.is_file())
}

pub fn wallpaper_engine_config_path() -> Option<PathBuf> {
    let path = wallpaper_engine_install_dir()?.join("config.json");
    path.is_file().then_some(path)
}

pub fn scan_wallpapers(include_default_projects: bool) -> Vec<WallpaperItem> {
    scan_wallpapers_until(include_default_projects, &|| false)
}

fn scan_wallpapers_until(
    include_default_projects: bool,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Vec<WallpaperItem> {
    scan_wallpapers_from_libraries_until(
        find_steam_libraries(),
        include_default_projects,
        cancelled,
    )
}

pub(crate) fn scan_wallpapers_from_libraries_until(
    libraries: &[PathBuf],
    include_default_projects: bool,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Vec<WallpaperItem> {
    scan_project_roots(
        &engine_catalog_roots(libraries, include_default_projects),
        default_catalog_scan_threads(),
        cancelled,
        &read_project,
    )
}

/// Keep absent roots: the watcher must poll when a subscription root is not yet present.
pub(crate) fn engine_catalog_roots(libraries: &[PathBuf], defaults: bool) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for lib in libraries {
        roots.push(lib.join("steamapps/workshop/content/431960"));
        let projects = lib.join("steamapps/common/wallpaper_engine/projects");
        roots.push(projects.join("myprojects"));
        if defaults {
            roots.push(projects.join("defaultprojects"));
        }
    }
    roots
}

pub(crate) fn catalog_watch_roots(local: &[PathBuf], defaults: bool) -> Vec<PathBuf> {
    let mut roots = local.to_vec();
    roots.extend(engine_catalog_roots(find_steam_libraries(), defaults));
    roots.sort();
    roots.dedup();
    roots
}

struct Utf8Reader<R> {
    reader: R,
    tail: Vec<u8>,
}
impl<R: std::io::Read> std::io::Read for Utf8Reader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        if buffer.is_empty() {
            return Ok(0);
        }
        let count = self.reader.read(buffer)?;
        let chunk = if self.tail.is_empty() {
            &buffer[..count]
        } else {
            self.tail.extend_from_slice(&buffer[..count]);
            self.tail.as_slice()
        };
        let remaining = match std::str::from_utf8(chunk) {
            Ok(_) => 0,
            Err(error) if count > 0 && error.error_len().is_none() => {
                chunk.len() - error.valid_up_to()
            }
            Err(error) => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, error)),
        };
        if remaining == 0 {
            self.tail.clear();
        } else {
            self.tail = chunk[chunk.len() - remaining..].to_vec();
        }
        Ok(count)
    }
}

fn read_project(path: &Path) -> Option<ProjectJson> {
    // Avoid one whole-file UTF-8 buffer per lane. Owned JSON fields still scale with
    // metadata size; no new size limit rejects previously supported projects.
    // serde_json skips UTF-8 validation in ignored fields; retain read_to_string's rule.
    let reader = Utf8Reader {
        reader: fs::File::open(path).ok()?,
        tail: Vec::new(),
    };
    serde_json::from_reader(std::io::BufReader::new(reader)).ok()
}

fn scan_project_roots(
    search_dirs: &[PathBuf],
    threads: usize,
    cancelled: &(dyn Fn() -> bool + Sync),
    parse: &(dyn Fn(&Path) -> Option<ProjectJson> + Sync),
) -> Vec<WallpaperItem> {
    let mut candidates = Vec::new();

    for dir in search_dirs {
        if cancelled() {
            return Vec::new();
        }
        if !dir.exists() {
            continue;
        }
        let entries = match fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            if cancelled() {
                return Vec::new();
            }
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            candidates.push(path);
        }
    }
    let results = scan_ordered(&candidates, threads, cancelled, |path| {
        scan_project(path, parse)
    });
    let mut seen_ids = std::collections::HashSet::new();
    let mut items: Vec<_> = results
        .into_iter()
        .flatten()
        .filter(|item| seen_ids.insert(item.id.clone()))
        .collect();
    sort_items(&mut items);
    items
}

fn scan_project(
    path: &Path,
    parse: &(dyn Fn(&Path) -> Option<ProjectJson> + Sync),
) -> Option<WallpaperItem> {
    let id = path.file_name()?.to_string_lossy().to_string();
    let pj_path = path.join("project.json");
    let pj = parse(&pj_path)?;

    // Filter out non-wallpaper assets (editor effects, presets, shaders, templates)
    if let Some(ref cat) = pj.category {
        let cat_lower = cat.to_lowercase();
        if cat_lower == "asset" || cat_lower == "preset" || cat_lower == "template" {
            return None;
        }
    }

    // Skip items with missing or blank title (templates, internal assets)
    let title = match pj.title {
        Some(t) if !t.trim().is_empty() => t,
        _ => return None,
    };

    let file_rel = pj.file.unwrap_or_else(|| "scene.json".to_string());
    let file_lower = file_rel.to_lowercase();
    if file_lower.ends_with("effect.json") || file_lower.ends_with("shader.json") {
        return None;
    }
    let file_target = path.join(&file_rel);

    let preview_rel = pj.preview.unwrap_or_else(|| "preview.jpg".to_string());
    let mut preview_path = Some(path.join(preview_rel));

    if let Some(ref p) = preview_path {
        if !p.is_file() {
            preview_path = None;
            for cand in &[
                "preview.jpg",
                "preview.gif",
                "preview.png",
                "preview.jpeg",
                "preview.mp4",
            ] {
                let cp = path.join(cand);
                if cp.is_file() {
                    preview_path = Some(cp);
                    break;
                }
            }
        }
    }

    let tags = crate::library::normalized_project_tags(pj.tags.as_ref());
    let mut item = WallpaperItem {
        id,
        title,
        folder_path: path.to_path_buf(),
        project_json_path: pj_path.clone(),
        file_target: if file_target.exists() {
            file_target
        } else {
            pj_path
        },
        preview_path,
        wallpaper_type: pj.wp_type.unwrap_or_else(|| "scene".to_string()),
        source: WallpaperSource::WallpaperEngine,
        scan_metadata: None,
    };
    item.capture_metadata(tags);
    Some(item)
}

fn sort_items(items: &mut [WallpaperItem]) {
    items.sort_by(|a, b| {
        a.title
            .to_lowercase()
            .cmp(&b.title.to_lowercase())
            .then_with(|| a.id.cmp(&b.id))
    });
}

fn is_supported_local_image(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "bmp" | "webp"
            )
        })
        .unwrap_or(false)
}

pub(crate) fn is_supported_video(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "mp4" | "webm" | "mkv" | "avi" | "mov" | "wmv" | "m4v"
            )
        })
        .unwrap_or(false)
}

#[allow(dead_code)]
pub fn local_image_item(path: &Path) -> Result<WallpaperItem, String> {
    if !path.is_file() {
        return Err(format!(
            "Local wallpaper path does not name an existing file: {}",
            path.display()
        ));
    }
    if !is_supported_local_image(path) {
        return Err(format!(
            "Local wallpaper must be a JPG, JPEG, PNG, BMP, or WebP image: {}",
            path.display()
        ));
    }
    let canonical_file = path.canonicalize().map_err(|error| {
        format!(
            "Could not resolve local wallpaper path {}: {error}",
            path.display()
        )
    })?;
    let file_key = canonical_file.to_string_lossy().to_ascii_lowercase();
    let title = canonical_file
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("Untitled wallpaper")
        .to_string();
    let folder_path = canonical_file
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();

    let mut item = WallpaperItem {
        id: format!("local:{file_key}"),
        title,
        folder_path,
        project_json_path: canonical_file.clone(),
        file_target: canonical_file.clone(),
        preview_path: Some(canonical_file),
        wallpaper_type: "image".to_string(),
        source: WallpaperSource::LocalImage,
        scan_metadata: None,
    };
    item.capture_metadata(Default::default());
    Ok(item)
}

fn scan_local_media_impl(roots: &[PathBuf], include_videos: bool) -> Vec<WallpaperItem> {
    scan_local_media_until(
        roots,
        include_videos,
        default_catalog_scan_threads(),
        &|| false,
    )
}

fn scan_local_media_until(
    roots: &[PathBuf],
    include_videos: bool,
    threads: usize,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Vec<WallpaperItem> {
    let mut candidates = Vec::new();
    let mut visited_directories = std::collections::HashSet::new();
    let mut pending: Vec<PathBuf> = roots.to_vec();

    while let Some(directory) = pending.pop() {
        if cancelled() {
            return Vec::new();
        }
        let canonical_directory = match directory.canonicalize() {
            Ok(path) => path,
            Err(_) => continue,
        };
        let directory_key = canonical_directory.to_string_lossy().to_ascii_lowercase();
        if !visited_directories.insert(directory_key) {
            continue;
        }

        let entries = match fs::read_dir(&canonical_directory) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            if cancelled() {
                return Vec::new();
            }
            let path = entry.path();
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(_) => continue,
            };
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            if !metadata.is_file()
                || (!is_supported_local_image(&path)
                    && !(include_videos && is_supported_video(&path)))
            {
                continue;
            }

            candidates.push(path);
        }
    }
    let results = scan_ordered(&candidates, threads, cancelled, |path| {
        let canonical_file = path.canonicalize().unwrap_or_else(|_| path.clone());
        let file_key = canonical_file.to_string_lossy().to_ascii_lowercase();
        let title = canonical_file
            .file_stem()
            .and_then(|stem| stem.to_str())
            .filter(|stem| !stem.trim().is_empty())
            .unwrap_or("Untitled wallpaper")
            .to_string();
        let folder_path = canonical_file
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default();

        let source = if is_supported_video(&canonical_file) {
            WallpaperSource::LocalVideo
        } else {
            WallpaperSource::LocalImage
        };
        let mut item = WallpaperItem {
            id: format!("local:{file_key}"),
            title,
            folder_path,
            project_json_path: canonical_file.clone(),
            file_target: canonical_file.clone(),
            preview_path: (source == WallpaperSource::LocalImage).then_some(canonical_file.clone()),
            wallpaper_type: if source == WallpaperSource::LocalVideo {
                "video".to_string()
            } else {
                "image".to_string()
            },
            source,
            scan_metadata: None,
        };
        item.capture_metadata(Default::default());
        item
    });
    let mut seen_files = std::collections::HashSet::new();
    let mut items: Vec<_> = results
        .into_iter()
        .filter(|item| seen_files.insert(item.id.clone()))
        .collect();

    sort_items(&mut items);
    items
}

pub fn scan_local_images(roots: &[PathBuf]) -> Vec<WallpaperItem> {
    scan_local_media_impl(roots, false)
}

pub fn scan_local_media(roots: &[PathBuf]) -> Vec<WallpaperItem> {
    scan_local_media_impl(roots, true)
}

pub fn scan_catalog(local_roots: &[PathBuf], include_default_projects: bool) -> Vec<WallpaperItem> {
    scan_catalog_until(local_roots, include_default_projects, &|| false)
}

pub fn scan_catalog_until(
    local_roots: &[PathBuf],
    include_default_projects: bool,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Vec<WallpaperItem> {
    scan_catalog_with_threads_until(
        local_roots,
        include_default_projects,
        default_catalog_scan_threads(),
        cancelled,
    )
}

pub(crate) fn scan_catalog_with_threads_until(
    local_roots: &[PathBuf],
    include_default_projects: bool,
    threads: usize,
    cancelled: &(dyn Fn() -> bool + Sync),
) -> Vec<WallpaperItem> {
    let mut items = scan_local_media_until(local_roots, true, threads, cancelled);
    if !cancelled() {
        items.extend(scan_project_roots(
            &engine_catalog_roots(find_steam_libraries(), include_default_projects),
            threads,
            cancelled,
            &read_project,
        ));
    }
    if cancelled() {
        return Vec::new();
    }
    sort_items(&mut items);
    items
}

/// Resolve one supported installed Wallpaper Engine catalog identity or path.
///
/// Diagnostic callers use the same catalog scan and stable `WallpaperItem.id`
/// values as the application. A path may identify the project folder,
/// `project.json`, or the project's configured target file. Matching is
/// component/path exact and ambiguous matches are rejected.
pub fn resolve_installed_engine_item(identity: &str) -> Result<WallpaperItem, String> {
    let wallpapers = scan_wallpapers(true);
    let item = resolve_installed_engine_item_from_catalog(identity, &wallpapers)?;
    if !item.engine_target().is_file() {
        return Err(format!(
            "Installed Wallpaper Engine item {identity:?} has no supported target file: {}",
            item.engine_target().display()
        ));
    }
    Ok(item)
}

fn resolve_installed_engine_item_from_catalog(
    identity: &str,
    wallpapers: &[WallpaperItem],
) -> Result<WallpaperItem, String> {
    let identity = identity.trim();
    if identity.is_empty() {
        return Err("Wallpaper Engine catalog identity/path is empty".to_string());
    }

    let normalized_identity =
        crate::native_wallpaper::normalized_wallpaper_path(Path::new(identity));
    let matches: Vec<_> = wallpapers
        .iter()
        .filter(|item| item.source == WallpaperSource::WallpaperEngine)
        .filter(|item| {
            item.id.eq_ignore_ascii_case(identity)
                || [
                    item.folder_path.as_path(),
                    item.project_json_path.as_path(),
                    item.file_target.as_path(),
                ]
                .into_iter()
                .any(|candidate| {
                    crate::native_wallpaper::normalized_wallpaper_path(candidate)
                        == normalized_identity
                })
        })
        .collect();

    match matches.as_slice() {
        [] => Err(format!(
            "No supported installed Wallpaper Engine item matched catalog identity/path {identity:?}"
        )),
        [item] => Ok((*item).clone()),
        matches => Err(format!(
            "Wallpaper Engine catalog identity/path {identity:?} is ambiguous across {} items",
            matches.len()
        )),
    }
}

pub fn get_active_wallpaper_file(monitor_idx: usize) -> Option<String> {
    let mut config_paths = Vec::new();
    for lib in find_steam_libraries() {
        let cp = lib
            .join("steamapps")
            .join("common")
            .join("wallpaper_engine")
            .join("config.json");
        if cp.exists() {
            config_paths.push(cp);
        }
    }

    let mon_key = format!("Monitor{}", monitor_idx);

    for cp in config_paths {
        if let Ok(data) = fs::read_to_string(&cp) {
            if let Ok(val) = serde_json::from_str::<serde_json::Value>(&data) {
                if let Some(obj) = val.as_object() {
                    for (_user, user_val) in obj {
                        if let Some(file) = user_val
                            .get("general")
                            .and_then(|g| g.get("wallpaperconfig"))
                            .and_then(|wc| wc.get("selectedwallpapers"))
                            .and_then(|sw| sw.get(&mon_key))
                            .and_then(|m| m.get("file"))
                            .and_then(|f| f.as_str())
                        {
                            return Some(file.to_string());
                        }
                    }
                }
            }
        }
    }
    None
}

fn is_same_or_child_path(path: &Path, root: &Path) -> bool {
    let path = crate::native_wallpaper::normalized_wallpaper_path(path);
    let root = crate::native_wallpaper::normalized_wallpaper_path(root);
    path == root || path.starts_with(&format!("{root}/"))
}

/// Resolve a provider-reported path only when it identifies one catalog item
/// unambiguously. Local images use exact source paths or the deterministic
/// native-compatible path derived from their current metadata. Engine paths
/// may be inside a project folder, but only at a component boundary.
pub fn find_active_wallpaper_index(
    wallpapers: &[WallpaperItem],
    active_file: &str,
    native_cache_dir: &Path,
) -> Option<usize> {
    let active_path = Path::new(active_file);
    let active_normalized = crate::native_wallpaper::normalized_wallpaper_path(active_path);
    let mut matches = Vec::new();

    for (index, wallpaper) in wallpapers.iter().enumerate() {
        let matches_item = match wallpaper.source {
            WallpaperSource::LocalImage => {
                wallpaper.file_target.is_file()
                    && (crate::native_wallpaper::normalized_wallpaper_path(&wallpaper.file_target)
                        == active_normalized
                        || crate::native_wallpaper::compatible_image_path(
                            &wallpaper.file_target,
                            native_cache_dir,
                        )
                        .is_some_and(|compatible| {
                            crate::native_wallpaper::normalized_wallpaper_path(&compatible)
                                == active_normalized
                        }))
            }
            WallpaperSource::LocalVideo => {
                wallpaper.file_target.is_file()
                    && crate::native_wallpaper::normalized_wallpaper_path(&wallpaper.file_target)
                        == active_normalized
            }
            WallpaperSource::WallpaperEngine => {
                [
                    wallpaper.file_target.as_path(),
                    wallpaper.project_json_path.as_path(),
                ]
                .into_iter()
                .any(|candidate| {
                    crate::native_wallpaper::normalized_wallpaper_path(candidate)
                        == active_normalized
                }) || is_same_or_child_path(active_path, &wallpaper.folder_path)
            }
        };
        if matches_item {
            matches.push(index);
        }
    }

    if matches.len() == 1 {
        Some(matches[0])
    } else {
        None
    }
}

#[cfg(test)]
#[path = "scanner_concurrency_tests.rs"]
mod concurrency_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    pub(super) fn test_dir(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("wallpaper-switcher-{name}-{nonce}"))
    }

    #[test]
    fn p43_project_is_parsed_once_with_permissive_tags_and_precise_stamps() {
        let root = test_dir("p43");
        let project = root.join("431960/owned");
        fs::create_dir_all(&project).unwrap();
        let json = project.join("project.json");
        fs::write(&json, r#"{"title":"Owned","type":"image","file":"preview.png","tags":[" Sky ",42,null,"sky","夜空",""]}"#).unwrap();
        image::RgbaImage::new(2, 3)
            .save(project.join("preview.png"))
            .unwrap();
        let parses = AtomicUsize::new(0);
        let parse = |path: &Path| {
            parses.fetch_add(1, Ordering::Relaxed);
            read_project(path)
        };
        let items = scan_project_roots(&[root.join("431960")], 2, &|| false, &parse);
        assert_eq!(items.len(), 1);
        assert_eq!(parses.load(Ordering::Relaxed), 1);
        let first = &items[0];
        assert_eq!(first.wallpaper_type, "image");
        let metadata = first.scan_metadata.as_ref().unwrap();
        assert_eq!(metadata.tags, ["sky".into(), "夜空".into()].into());
        assert!(
            metadata.project.is_some() && metadata.target.is_some() && metadata.preview.is_some()
        );
        fs::remove_file(&json).unwrap();
        let entry = crate::library::CatalogEntry::from_wallpaper(first);
        assert_eq!(
            entry.source_tags, metadata.tags,
            "conversion must not reopen project JSON"
        );
        assert_eq!(parses.load(Ordering::Relaxed), 1);

        fs::write(
            &json,
            br#"{"title":"Owned","type":"image","file":"preview.png","tags":false}"#,
        )
        .unwrap();
        let file = fs::OpenOptions::new().write(true).open(&json).unwrap();
        let time = UNIX_EPOCH + std::time::Duration::new(1_800_000_000, 123_400_000);
        file.set_times(fs::FileTimes::new().set_modified(time))
            .unwrap();
        let empty = scan_project_roots(&[root.join("431960")], 2, &|| false, &parse).remove(0);
        assert!(empty.scan_metadata.as_ref().unwrap().tags.is_empty());
        assert_eq!(
            empty
                .scan_metadata
                .as_ref()
                .unwrap()
                .project
                .as_ref()
                .unwrap()
                .modified,
            Some(time)
        );
        let later = time + std::time::Duration::from_millis(100);
        file.set_times(fs::FileTimes::new().set_modified(later))
            .unwrap();
        let changed = scan_project_roots(&[root.join("431960")], 2, &|| false, &parse).remove(0);
        assert_ne!(
            empty.scan_metadata, changed.scan_metadata,
            "subsecond same-path changes must differ"
        );
        fs::write(&json, br#"{"tags":["must not reread"]}"#).unwrap();
        assert!(crate::library::CatalogEntry::from_wallpaper(&empty)
            .tags
            .is_empty());
        assert_eq!(parses.load(Ordering::Relaxed), 3);
    }

    #[test]
    fn p42_steam_roots_include_missing_workshop_myprojects_and_optional_defaults() {
        let library = test_dir("p42-libraries");
        let roots = engine_catalog_roots(&[library.clone()], false);
        assert_eq!(
            roots,
            vec![
                library.join("steamapps/workshop/content/431960"),
                library.join("steamapps/common/wallpaper_engine/projects/myprojects")
            ]
        );
        assert!(roots.iter().all(|r| !r.exists()));
        assert_eq!(engine_catalog_roots(&[library], true).len(), 3);
    }

    #[test]
    fn local_scan_is_recursive_filters_extensions_and_deduplicates_roots() {
        let root = test_dir("local-scan");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        fs::write(root.join("one.JPG"), b"not-decoded-during-scan").unwrap();
        fs::write(nested.join("two.webp"), b"not-decoded-during-scan").unwrap();
        fs::write(nested.join("ignored.gif"), b"not-supported").unwrap();

        let items = scan_local_images(&[root.clone(), root.clone()]);

        assert_eq!(items.len(), 2);
        assert!(items
            .iter()
            .all(|item| item.source == WallpaperSource::LocalImage));
        assert!(items.iter().any(|item| item.title == "one"));
        assert!(items.iter().any(|item| item.title == "two"));

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_scan_skips_missing_roots_without_failing() {
        let root = test_dir("missing-root");
        assert!(scan_local_images(&[root]).is_empty());
    }

    #[test]
    fn unified_local_scan_routes_video_files_to_wallpaper_engine() {
        let root = test_dir("local-media");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("loop.MP4"), b"scan-does-not-decode-video").unwrap();
        fs::write(root.join("still.png"), b"scan-does-not-decode-image").unwrap();

        let items = scan_local_media(&[root.clone()]);

        assert_eq!(items.len(), 2);
        let video = items.iter().find(|item| item.title == "loop").unwrap();
        assert_eq!(video.source, WallpaperSource::LocalVideo);
        assert_eq!(video.wallpaper_type, "video");
        assert_eq!(video.preview_path, None);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn exact_local_jpeg_and_bmp_paths_match_their_catalog_items() {
        let root = test_dir("active-exact-images");
        fs::create_dir_all(&root).unwrap();
        let jpeg = root.join("portrait.jpg");
        let bmp = root.join("texture.BMP");
        fs::write(&jpeg, b"jpeg-fixture").unwrap();
        fs::write(&bmp, b"bmp-fixture").unwrap();
        let cache_dir = root.join("native-images");
        let items = scan_local_images(std::slice::from_ref(&root));

        let jpeg_index = items
            .iter()
            .position(|item| item.file_target == jpeg.canonicalize().unwrap())
            .unwrap();
        let bmp_index = items
            .iter()
            .position(|item| item.file_target == bmp.canonicalize().unwrap())
            .unwrap();

        assert_eq!(
            find_active_wallpaper_index(
                &items,
                &jpeg.to_string_lossy().to_ascii_uppercase(),
                &cache_dir,
            ),
            Some(jpeg_index)
        );
        assert_eq!(
            find_active_wallpaper_index(
                &items,
                &bmp.to_string_lossy().replace('\\', "/"),
                &cache_dir,
            ),
            Some(bmp_index)
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn converted_png_and_webp_paths_resolve_without_creating_cache_files() {
        let root = test_dir("active-converted-images");
        fs::create_dir_all(&root).unwrap();
        let png = root.join("portrait.png");
        let webp = root.join("texture.webp");
        fs::write(&png, b"png-fixture").unwrap();
        fs::write(&webp, b"webp-fixture").unwrap();
        let cache_dir = root.join("native-images");
        let items = scan_local_images(std::slice::from_ref(&root));

        for source in [&png, &webp] {
            let item_index = items
                .iter()
                .position(|item| item.file_target == source.canonicalize().unwrap())
                .unwrap();
            let compatible = crate::native_wallpaper::compatible_image_path(
                &items[item_index].file_target,
                &cache_dir,
            )
            .unwrap();
            assert!(!compatible.exists());
            let provider_path = compatible.to_string_lossy().replace('\\', "/");

            assert_eq!(
                find_active_wallpaper_index(&items, &provider_path, &cache_dir),
                Some(item_index)
            );
        }

        assert!(!cache_dir.exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_matching_supports_case_separators_and_dot_segments() {
        let root = test_dir("active-path-normalization");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).unwrap();
        let source = nested.join("Case.PNG");
        fs::write(&source, b"png-fixture").unwrap();
        let items = scan_local_images(std::slice::from_ref(&root));
        let expected = items
            .iter()
            .position(|item| item.file_target == source.canonicalize().unwrap())
            .unwrap();
        let active = format!(
            "{}/nested/../nested/case.png",
            root.to_string_lossy().replace('\\', "/")
        );

        assert_eq!(
            find_active_wallpaper_index(&items, &active, &root.join("native-images")),
            Some(expected)
        );
        // Rust canonicalization may already return a verbatim path, including
        // when TEMP is extended. Derive a fixture once instead of double-prefixing.
        let extended = source
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        assert_eq!(
            find_active_wallpaper_index(&items, &extended, &root.join("native-images")),
            Some(expected)
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn local_same_directory_does_not_prove_identity() {
        let root = test_dir("active-sibling-collision");
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("first.png"), b"first").unwrap();
        fs::write(root.join("second.png"), b"second").unwrap();
        let items = scan_local_images(std::slice::from_ref(&root));
        let unrelated = root.join("third.png");

        assert_eq!(
            find_active_wallpaper_index(
                &items,
                &unrelated.to_string_lossy(),
                &root.join("native-images"),
            ),
            None
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn engine_project_matching_respects_component_boundaries() {
        let project_100 = PathBuf::from(r"C:\\projects\\100");
        let project_1000 = PathBuf::from(r"C:\\projects\\1000");
        let items = vec![
            WallpaperItem {
                scan_metadata: None,
                id: "100".into(),
                title: "100".into(),
                folder_path: project_100.clone(),
                project_json_path: project_100.join("project.json"),
                file_target: project_100.join("scene.json"),
                preview_path: None,
                wallpaper_type: "scene".into(),
                source: WallpaperSource::WallpaperEngine,
            },
            WallpaperItem {
                scan_metadata: None,
                id: "1000".into(),
                title: "1000".into(),
                folder_path: project_1000.clone(),
                project_json_path: project_1000.join("project.json"),
                file_target: project_1000.join("scene.json"),
                preview_path: None,
                wallpaper_type: "scene".into(),
                source: WallpaperSource::WallpaperEngine,
            },
        ];

        assert_eq!(
            find_active_wallpaper_index(
                &items,
                r"c:/projects/1000/assets/scene.json",
                Path::new("C:/unused/native-images"),
            ),
            Some(1)
        );
        assert_eq!(
            find_active_wallpaper_index(
                &items,
                r"C:\projects\1000-preview\scene.json",
                Path::new("C:/unused/native-images"),
            ),
            None
        );
    }

    #[test]
    fn id_substrings_and_unrelated_provider_paths_remain_unresolved() {
        let item = WallpaperItem {
            scan_metadata: None,
            id: "42".into(),
            title: "known".into(),
            folder_path: PathBuf::from(r"C:\projects\known"),
            project_json_path: PathBuf::from(r"C:\projects\known\project.json"),
            file_target: PathBuf::from(r"C:\projects\known\scene.json"),
            preview_path: None,
            wallpaper_type: "scene".into(),
            source: WallpaperSource::WallpaperEngine,
        };

        assert_eq!(
            find_active_wallpaper_index(
                &[item],
                r"C:\unrelated\wallpaper-42-copy\scene.json",
                Path::new("C:/unused/native-images"),
            ),
            None
        );
    }

    #[test]
    fn engine_matching_normalizes_extended_unc_paths() {
        let folder = PathBuf::from(r"\\server\share\projects\wall");
        let item = WallpaperItem {
            scan_metadata: None,
            id: "wall".into(),
            title: "wall".into(),
            folder_path: folder.clone(),
            project_json_path: folder.join("project.json"),
            file_target: folder.join("scene.json"),
            preview_path: None,
            wallpaper_type: "scene".into(),
            source: WallpaperSource::WallpaperEngine,
        };

        assert_eq!(
            find_active_wallpaper_index(
                &[item],
                r"\\?\UNC\server\share\projects\wall\assets\scene.json",
                Path::new("C:/unused/native-images"),
            ),
            Some(0)
        );
    }

    #[test]
    fn ambiguous_engine_containment_remains_unresolved() {
        let outer = PathBuf::from(r"C:\projects\outer");
        let inner = outer.join("inner");
        let item = |id: &str, folder: PathBuf| WallpaperItem {
            scan_metadata: None,
            id: id.into(),
            title: id.into(),
            project_json_path: folder.join("project.json"),
            file_target: folder.join("scene.json"),
            folder_path: folder,
            preview_path: None,
            wallpaper_type: "scene".into(),
            source: WallpaperSource::WallpaperEngine,
        };

        assert_eq!(
            find_active_wallpaper_index(
                &[item("outer", outer), item("inner", inner)],
                r"C:\projects\outer\inner\assets\scene.json",
                Path::new("C:/unused/native-images"),
            ),
            None
        );
    }

    #[test]
    fn stale_converted_path_does_not_match_changed_source_metadata() {
        let root = test_dir("active-stale-conversion");
        fs::create_dir_all(&root).unwrap();
        let source = root.join("mutable.webp");
        let cache_dir = root.join("native-images");
        fs::write(&source, b"before").unwrap();
        let before = scan_local_images(std::slice::from_ref(&root));
        let before_item = before
            .iter()
            .find(|item| item.file_target == source.canonicalize().unwrap())
            .unwrap();
        let old_cache =
            crate::native_wallpaper::compatible_image_path(&before_item.file_target, &cache_dir)
                .unwrap();

        fs::write(&source, b"after-with-different-size").unwrap();
        let after = scan_local_images(std::slice::from_ref(&root));

        assert_ne!(
            crate::native_wallpaper::compatible_image_path(&after[0].file_target, &cache_dir,)
                .unwrap(),
            old_cache
        );
        assert_eq!(
            find_active_wallpaper_index(&after, &old_cache.to_string_lossy(), &cache_dir),
            None
        );
        assert_eq!(
            find_active_wallpaper_index(
                &before,
                &PathBuf::from("missing.webp").to_string_lossy(),
                &cache_dir
            ),
            None
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn missing_converted_source_or_catalog_item_remains_unresolved() {
        let root = test_dir("active-missing-source");
        let source = root.join("gone.png");
        let cache_dir = root.join("native-images");
        let item = WallpaperItem {
            scan_metadata: None,
            id: "missing-source".into(),
            title: "missing-source".into(),
            folder_path: root.clone(),
            project_json_path: PathBuf::new(),
            file_target: source,
            preview_path: None,
            wallpaper_type: "image".into(),
            source: WallpaperSource::LocalImage,
        };
        let provider_path = cache_dir.join("native-deadbeef.bmp");

        assert_eq!(
            crate::native_wallpaper::compatible_image_path(&item.file_target, &cache_dir),
            None
        );
        assert_eq!(
            find_active_wallpaper_index(&[item], &provider_path.to_string_lossy(), &cache_dir),
            None
        );
        assert_eq!(
            find_active_wallpaper_index(&[], &provider_path.to_string_lossy(), &cache_dir),
            None
        );
    }

    #[test]
    fn installed_engine_identity_resolves_id_and_exact_project_paths() {
        let root = PathBuf::from(r"C:\projects\engine-scene");
        let item = WallpaperItem {
            scan_metadata: None,
            id: "scene-42".into(),
            title: "Scene 42".into(),
            folder_path: root.clone(),
            project_json_path: root.join("project.json"),
            file_target: root.join("scene.json"),
            preview_path: None,
            wallpaper_type: "scene".into(),
            source: WallpaperSource::WallpaperEngine,
        };

        assert_eq!(
            resolve_installed_engine_item_from_catalog("SCENE-42", std::slice::from_ref(&item))
                .unwrap()
                .id,
            "scene-42"
        );
        assert_eq!(
            resolve_installed_engine_item_from_catalog(
                r"c:/projects/engine-scene/project.json",
                std::slice::from_ref(&item),
            )
            .unwrap()
            .id,
            "scene-42"
        );
        assert_eq!(
            resolve_installed_engine_item_from_catalog(
                r"C:\projects\engine-scene\scene.json",
                std::slice::from_ref(&item),
            )
            .unwrap()
            .id,
            "scene-42"
        );
    }

    #[test]
    fn installed_engine_identity_rejects_missing_unsupported_and_ambiguous_items() {
        let outer = PathBuf::from(r"C:\projects\outer");
        let inner = outer.join("inner");
        let item = |id: &str, folder: PathBuf, source| WallpaperItem {
            scan_metadata: None,
            id: id.into(),
            title: id.into(),
            project_json_path: folder.join("project.json"),
            file_target: folder.join("scene.json"),
            folder_path: folder,
            preview_path: None,
            wallpaper_type: "scene".into(),
            source,
        };
        let items = vec![
            item("outer", outer.clone(), WallpaperSource::WallpaperEngine),
            item("inner", inner, WallpaperSource::WallpaperEngine),
            item(
                "outer-duplicate",
                outer.clone(),
                WallpaperSource::WallpaperEngine,
            ),
            item(
                "local",
                PathBuf::from(r"C:\local"),
                WallpaperSource::LocalImage,
            ),
        ];

        assert!(
            resolve_installed_engine_item_from_catalog("missing", &items)
                .unwrap_err()
                .contains("No supported installed")
        );
        assert!(
            resolve_installed_engine_item_from_catalog(r"C:\local", &items)
                .unwrap_err()
                .contains("No supported installed")
        );
        assert_eq!(
            resolve_installed_engine_item_from_catalog(
                r"C:\projects\outer\inner\scene.json",
                &items,
            )
            .unwrap()
            .id,
            "inner"
        );
        assert!(
            resolve_installed_engine_item_from_catalog(r"C:\projects\outer", &items)
                .unwrap_err()
                .contains("ambiguous")
        );
    }
}

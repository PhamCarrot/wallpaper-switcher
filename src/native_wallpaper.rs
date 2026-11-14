use std::collections::hash_map::DefaultHasher;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::hash::{Hash, Hasher};
use std::io::{self, BufReader, Write};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use image::{DynamicImage, ImageError, ImageFormat, ImageReader};

use windows::core::{HSTRING, PWSTR};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
    COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    DesktopWallpaper, IDesktopWallpaper, DESKTOP_WALLPAPER_POSITION, DWPOS_FILL,
};

use crate::file_publication;
use crate::monitor::DesktopRect;

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|error| format!("COM initialization failed: {error}"))?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[derive(Debug, Clone)]
pub struct NativeMonitor {
    pub device_id: String,
    pub rect: DesktopRect,
}

pub type WallpaperPosition = DESKTOP_WALLPAPER_POSITION;
pub const FILL_POSITION: WallpaperPosition = DWPOS_FILL;

pub fn needs_conversion(path: &Path) -> bool {
    !matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref(),
        Some("jpg" | "jpeg" | "bmp")
    )
}

fn compatible_cache_path(path: &Path, cache_dir: &Path, metadata: &std::fs::Metadata) -> PathBuf {
    let modified = metadata.modified().ok();
    let mut hasher = DefaultHasher::new();
    path.to_string_lossy()
        .to_ascii_lowercase()
        .hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    modified.hash(&mut hasher);
    cache_dir.join(format!("native-{:016x}.bmp", hasher.finish()))
}

/// Derive the native-compatible path without decoding, writing, or mutating
/// the source or cache. A missing source cannot provide a trustworthy
/// conversion fingerprint and therefore returns `None`.
pub fn compatible_image_path(path: &Path, cache_dir: &Path) -> Option<PathBuf> {
    if !path.is_file() {
        return None;
    }
    if !needs_conversion(path) {
        return Some(path.to_path_buf());
    }
    let metadata = path.metadata().ok()?;
    Some(compatible_cache_path(path, cache_dir, &metadata))
}

#[derive(Debug)]
enum CacheValidationError {
    Unreadable(String),
    Invalid(String),
}

impl fmt::Display for CacheValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(error) => write!(formatter, "unreadable: {error}"),
            Self::Invalid(error) => write!(formatter, "invalid: {error}"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum CacheEntry {
    Missing,
    Valid { width: u32, height: u32 },
}

/// A valid cache entry is a non-empty regular file whose bytes are detected as
/// BMP and fully decode to nonzero dimensions. This establishes structural
/// usability only; it does not prove that the pixels match the source image.
fn inspect_cached_bmp(path: &Path) -> Result<CacheEntry, CacheValidationError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(CacheEntry::Missing),
        Err(error) => {
            return Err(CacheValidationError::Unreadable(format!(
                "could not inspect {}: {error}",
                path.display()
            )))
        }
    };
    if !metadata.is_file() {
        return Err(CacheValidationError::Unreadable(format!(
            "{} is not a regular file",
            path.display()
        )));
    }
    if metadata.len() == 0 {
        return Err(CacheValidationError::Invalid(format!(
            "{} is empty",
            path.display()
        )));
    }

    let file = File::open(path).map_err(|error| {
        CacheValidationError::Unreadable(format!("could not open {}: {error}", path.display()))
    })?;
    let reader = ImageReader::new(BufReader::new(file))
        .with_guessed_format()
        .map_err(|error| {
            CacheValidationError::Unreadable(format!(
                "could not read the format of {}: {error}",
                path.display()
            ))
        })?;
    match reader.format() {
        Some(ImageFormat::Bmp) => {}
        Some(format) => {
            return Err(CacheValidationError::Invalid(format!(
                "{} contains {format:?}, not BMP",
                path.display()
            )))
        }
        None => {
            return Err(CacheValidationError::Invalid(format!(
                "{} has no recognized image format",
                path.display()
            )))
        }
    }

    let image = reader.decode().map_err(|error| match error {
        ImageError::IoError(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
            CacheValidationError::Invalid(format!("{} is truncated: {error}", path.display()))
        }
        ImageError::IoError(error) => CacheValidationError::Unreadable(format!(
            "could not read BMP contents from {}: {error}",
            path.display()
        )),
        error => CacheValidationError::Invalid(format!(
            "{} is not a usable BMP: {error}",
            path.display()
        )),
    })?;
    if image.width() == 0 || image.height() == 0 {
        return Err(CacheValidationError::Invalid(format!(
            "{} has unusable zero dimensions",
            path.display()
        )));
    }
    Ok(CacheEntry::Valid {
        width: image.width(),
        height: image.height(),
    })
}

struct TemporaryCacheFile {
    path: PathBuf,
    remove_on_drop: bool,
}

impl TemporaryCacheFile {
    fn mark_published(&mut self) {
        self.remove_on_drop = false;
    }

    fn retain_for_recovery(&mut self) {
        self.remove_on_drop = false;
    }
}

impl Drop for TemporaryCacheFile {
    fn drop(&mut self) {
        if self.remove_on_drop {
            let _ = fs::remove_file(&self.path);
        }
    }
}

fn create_temporary_cache_file(destination: &Path) -> io::Result<(PathBuf, File)> {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let filename = destination
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_else(|| "native-cache".to_string());
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());

    for attempt in 0..32u32 {
        let temporary = parent.join(format!(
            ".{filename}.native-cache-tmp-{}-{nonce}-{attempt}",
            std::process::id()
        ));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
        {
            Ok(file) => return Ok((temporary, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }

    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not reserve a unique native-cache temporary path",
    ))
}

fn stage_bmp<F>(
    image: &DynamicImage,
    destination: &Path,
    maybe_fail: &mut F,
) -> Result<TemporaryCacheFile, String>
where
    F: FnMut(&'static str) -> io::Result<()>,
{
    let (temporary_path, mut file) = create_temporary_cache_file(destination).map_err(|error| {
        format!(
            "Could not create a native-cache temporary file beside {}: {error}",
            destination.display()
        )
    })?;
    let temporary = TemporaryCacheFile {
        path: temporary_path,
        remove_on_drop: true,
    };

    maybe_fail("temporary-write").map_err(|error| {
        format!(
            "Could not write the native-cache temporary file {}: {error}",
            temporary.path.display()
        )
    })?;
    image
        .write_to(&mut file, ImageFormat::Bmp)
        .map_err(|error| {
            format!(
                "Could not write the native-cache temporary file {}: {error}",
                temporary.path.display()
            )
        })?;

    maybe_fail("temporary-flush").map_err(|error| {
        format!(
            "Could not flush the native-cache temporary file {}: {error}",
            temporary.path.display()
        )
    })?;
    file.flush()
        .and_then(|()| file.sync_all())
        .map_err(|error| {
            format!(
                "Could not flush the native-cache temporary file {}: {error}",
                temporary.path.display()
            )
        })?;
    drop(file);

    maybe_fail("temporary-validation").map_err(|error| {
        format!(
            "Could not validate the native-cache temporary file {}: {error}",
            temporary.path.display()
        )
    })?;
    match inspect_cached_bmp(&temporary.path) {
        Ok(CacheEntry::Valid { .. }) => Ok(temporary),
        Ok(CacheEntry::Missing) => Err(format!(
            "Native-cache temporary file disappeared before validation: {}",
            temporary.path.display()
        )),
        Err(error) => Err(format!(
            "Native-cache temporary file failed validation at {}: {error}",
            temporary.path.display()
        )),
    }
}

fn protected_cache_reference<'a>(
    cache_path: &Path,
    protected_paths: &'a [PathBuf],
) -> Option<&'a Path> {
    let cache_path = normalized_wallpaper_path(cache_path);
    protected_paths
        .iter()
        .find_map(|path| (normalized_wallpaper_path(path) == cache_path).then_some(path.as_path()))
}

enum PublicationOutcome {
    Published,
    ReusedExisting,
}

fn cache_entry_description(path: &Path) -> (String, bool) {
    match inspect_cached_bmp(path) {
        Ok(CacheEntry::Valid { .. }) => ("valid".to_string(), true),
        Ok(CacheEntry::Missing) => ("missing".to_string(), false),
        Err(CacheValidationError::Invalid(error)) => (format!("invalid ({error})"), false),
        Err(CacheValidationError::Unreadable(error)) => (format!("unreadable ({error})"), false),
    }
}

fn replace_failure_description(
    error: &io::Error,
    temporary: &mut TemporaryCacheFile,
    destination: &Path,
    backup: &Path,
) -> String {
    if !file_publication::replace_file_was_called(error) {
        return format!(
            "Native cache publication validation failed before ReplaceFileW for {}: {error}",
            destination.display()
        );
    }
    let (destination_state, _) = cache_entry_description(destination);
    let (staged_state, staged_valid) = cache_entry_description(&temporary.path);
    let (backup_state, backup_valid) = cache_entry_description(backup);
    if staged_valid {
        temporary.retain_for_recovery();
    }

    let mut recovery = Vec::new();
    if staged_valid {
        recovery.push(format!(
            "staged recovery remains at {}",
            temporary.path.display()
        ));
    }
    if backup_valid {
        recovery.push(format!(
            "tracked replacement backup remains at {}",
            backup.display()
        ));
    }
    let recovery = if recovery.is_empty() {
        "validated recovery could not be established".to_string()
    } else {
        recovery.join("; ")
    };

    format!(
        "ReplaceFileW returned {} ({}): destination after failure is {destination_state}; staged replacement is {staged_state}; tracked backup {} is {backup_state}; {recovery}",
        file_publication::replace_file_error_name(error),
        error,
        backup.display(),
    )
}

fn publish_staged_cache<F, R>(
    temporary: &mut TemporaryCacheFile,
    destination: &Path,
    protected_paths: &[PathBuf],
    maybe_fail: &mut F,
    replace_existing: &mut R,
) -> Result<PublicationOutcome, String>
where
    F: FnMut(&'static str) -> io::Result<()>,
    R: FnMut(&Path, &Path, &Path) -> io::Result<()>,
{
    maybe_fail("publication").map_err(|error| {
        format!(
            "Could not publish native cache {}: {error}",
            destination.display()
        )
    })?;

    match inspect_cached_bmp(destination) {
        Ok(CacheEntry::Valid { .. }) => return Ok(PublicationOutcome::ReusedExisting),
        Ok(CacheEntry::Missing) => {
            if let Err(error) = fs::rename(&temporary.path, destination) {
                let (staged_state, staged_valid) = cache_entry_description(&temporary.path);
                if staged_valid {
                    temporary.retain_for_recovery();
                }
                return Err(format!(
                    "Could not publish native cache {} by same-directory rename: {error}; destination after failure is {}; staged replacement is {}{}",
                    destination.display(),
                    cache_entry_description(destination).0,
                    staged_state,
                    if staged_valid {
                        format!("; staged recovery remains at {}", temporary.path.display())
                    } else {
                        String::new()
                    }
                ));
            }
            let (destination_state, destination_valid) = cache_entry_description(destination);
            if !destination_valid {
                return Err(format!(
                    "Same-directory rename reported success for native cache {} but its postcondition is not valid: destination is {destination_state}; validated recovery could not be established",
                    destination.display()
                ));
            }
        }
        Err(CacheValidationError::Invalid(_)) => {
            if let Some(protected) = protected_cache_reference(destination, protected_paths) {
                return Err(format!(
                    "Native cache {} is invalid but protected by current/rollback provider path {}; safe repair refused",
                    destination.display(),
                    protected.display()
                ));
            }
            let backup = file_publication::unique_recovery_path(
                destination,
                "native-cache-replace-recovery",
            );
            if let Err(error) = replace_existing(&temporary.path, destination, &backup) {
                return Err(replace_failure_description(
                    &error,
                    temporary,
                    destination,
                    &backup,
                ));
            }

            let (destination_state, destination_valid) = cache_entry_description(destination);
            if !destination_valid {
                let (staged_state, staged_valid) = cache_entry_description(&temporary.path);
                let (backup_state, backup_valid) = cache_entry_description(&backup);
                if staged_valid {
                    temporary.retain_for_recovery();
                }
                let mut recovery = Vec::new();
                if staged_valid {
                    recovery.push(format!("staged {}", temporary.path.display()));
                }
                if backup_valid {
                    recovery.push(format!("tracked backup {}", backup.display()));
                }
                let recovery = if recovery.is_empty() {
                    "validated recovery could not be established".to_string()
                } else {
                    format!("known recovery: {}", recovery.join("; "))
                };
                return Err(format!(
                    "ReplaceFileW reported success for native cache {} but its postcondition is not valid: destination is {destination_state}; staged replacement is {staged_state}; tracked backup {} is {backup_state}; {recovery}",
                    destination.display(),
                    backup.display(),
                ));
            }
            if let Err(error) = file_publication::remove_if_present(&backup) {
                return Err(format!(
                    "Native cache {} is valid after ReplaceFileW, but tracked recovery cleanup failed at {}: {error}; no ready path was returned",
                    destination.display(),
                    backup.display()
                ));
            }
        }
        Err(CacheValidationError::Unreadable(error)) => {
            return Err(format!(
                "Native cache {} became unreadable before publication: {error}",
                destination.display()
            ))
        }
    }
    temporary.mark_published();
    Ok(PublicationOutcome::Published)
}

fn prepare_compatible_image_inner<F>(
    path: &Path,
    cache_dir: &Path,
    protected_paths: &[PathBuf],
    maybe_fail: F,
) -> Result<PathBuf, String>
where
    F: FnMut(&'static str) -> io::Result<()>,
{
    let mut replace_existing = |replacement: &Path, destination: &Path, backup: &Path| {
        file_publication::replace_existing_file(replacement, destination, backup)
    };
    prepare_compatible_image_inner_with_replacer(
        path,
        cache_dir,
        protected_paths,
        maybe_fail,
        &mut replace_existing,
    )
}

fn prepare_compatible_image_inner_with_replacer<F, R>(
    path: &Path,
    cache_dir: &Path,
    protected_paths: &[PathBuf],
    mut maybe_fail: F,
    replace_existing: &mut R,
) -> Result<PathBuf, String>
where
    F: FnMut(&'static str) -> io::Result<()>,
    R: FnMut(&Path, &Path, &Path) -> io::Result<()>,
{
    crate::apply_trace::event(
        "native-conversion-start",
        format!(
            "source_path={path:?} bytes={} conversion_required={}",
            path.metadata().map_or(0, |metadata| metadata.len()),
            needs_conversion(path)
        ),
    );
    let mut cache_status = "not-required";
    let result = (|| {
        let metadata = match fs::metadata(path) {
            Ok(metadata) if metadata.is_file() => metadata,
            Ok(_) => {
                return Err(format!(
                    "Wallpaper image is not a regular file: {}",
                    path.display()
                ))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Err(format!(
                    "Wallpaper image does not exist: {}",
                    path.display()
                ))
            }
            Err(error) => {
                return Err(format!(
                    "Could not inspect wallpaper image {}: {error}",
                    path.display()
                ))
            }
        };
        if !needs_conversion(path) {
            return Ok(path.to_path_buf());
        }
        let cache_path = compatible_cache_path(path, cache_dir, &metadata);
        match inspect_cached_bmp(&cache_path) {
            Ok(CacheEntry::Valid { .. }) => {
                cache_status = "reused";
                return Ok(cache_path);
            }
            Ok(CacheEntry::Missing) => cache_status = "created",
            Err(CacheValidationError::Unreadable(error)) => {
                return Err(format!(
                    "Native cache entry is unreadable at {}: {error}",
                    cache_path.display()
                ))
            }
            Err(CacheValidationError::Invalid(error)) => {
                if let Some(protected) = protected_cache_reference(&cache_path, protected_paths) {
                    return Err(format!(
                        "Native cache {} is invalid but protected by current/rollback provider path {}; safe repair refused ({error})",
                        cache_path.display(),
                        protected.display()
                    ));
                }
                cache_status = "repaired";
            }
        }

        std::fs::create_dir_all(cache_dir)
            .map_err(|error| format!("Could not create wallpaper cache: {error}"))?;
        let image = image::open(path)
            .map_err(|error| format!("Could not decode {}: {error}", path.display()))?;
        let dimensions = (image.width(), image.height());
        let mut temporary = stage_bmp(&image, &cache_path, &mut maybe_fail)?;
        match publish_staged_cache(
            &mut temporary,
            &cache_path,
            protected_paths,
            &mut maybe_fail,
            replace_existing,
        )? {
            PublicationOutcome::Published => {}
            PublicationOutcome::ReusedExisting => cache_status = "reused",
        }
        crate::apply_trace::event(
            "native-conversion-complete",
            format!(
                "cache_path={cache_path:?} cache_status={cache_status} width={} height={}",
                dimensions.0, dimensions.1
            ),
        );
        Ok(cache_path)
    })();

    match &result {
        Ok(path) if !matches!(cache_status, "created" | "repaired") => crate::apply_trace::event(
            "native-conversion-complete",
            format!("cache_path={path:?} cache_status={cache_status}"),
        ),
        Ok(_) => {}
        Err(error) => {
            crate::apply_trace::event("native-conversion-failure", format!("error={error:?}"))
        }
    }
    result
}

pub fn prepare_compatible_image(path: &Path, cache_dir: &Path) -> Result<PathBuf, String> {
    prepare_compatible_image_inner(path, cache_dir, &[], |_| Ok(()))
}

pub(crate) fn prepare_compatible_image_with_protected_paths(
    path: &Path,
    cache_dir: &Path,
    protected_paths: &[PathBuf],
) -> Result<PathBuf, String> {
    prepare_compatible_image_inner(path, cache_dir, protected_paths, |_| Ok(()))
}

fn desktop_wallpaper() -> Result<(ComApartment, IDesktopWallpaper), String> {
    let apartment = ComApartment::initialize()?;
    let wallpaper = unsafe {
        CoCreateInstance(
            &DesktopWallpaper,
            None::<&windows::core::IUnknown>,
            CLSCTX_ALL,
        )
    }
    .map_err(|error| format!("IDesktopWallpaper is unavailable: {error}"))?;
    Ok((apartment, wallpaper))
}

fn owned_wide_string(value: PWSTR) -> Result<String, String> {
    let result = unsafe { value.to_string() }
        .map_err(|error| format!("Windows returned an invalid monitor identifier: {error}"));
    unsafe { CoTaskMemFree(Some(value.0.cast())) };
    result
}

pub fn monitors() -> Result<Vec<NativeMonitor>, String> {
    let (_apartment, wallpaper) = desktop_wallpaper()?;
    let count = unsafe { wallpaper.GetMonitorDevicePathCount() }
        .map_err(|error| format!("Could not enumerate desktop monitors: {error}"))?;
    let mut result = Vec::with_capacity(count as usize);

    for index in 0..count {
        let raw_id = unsafe { wallpaper.GetMonitorDevicePathAt(index) }
            .map_err(|error| format!("Could not read desktop monitor {index}: {error}"))?;
        let device_id = owned_wide_string(raw_id)?;
        let device_id_wide = HSTRING::from(&device_id);
        let rect = unsafe { wallpaper.GetMonitorRECT(&device_id_wide) }
            .map_err(|error| format!("Could not read desktop monitor rectangle: {error}"))?;
        result.push(NativeMonitor {
            device_id,
            rect: DesktopRect {
                left: rect.left,
                top: rect.top,
                right: rect.right,
                bottom: rect.bottom,
            },
        });
    }
    Ok(result)
}

pub fn set_wallpaper_for_device(path: &Path, device_id: &str) -> Result<PathBuf, String> {
    set_wallpaper_for_device_with_protected_paths(path, device_id, &[])
}

pub(crate) fn set_wallpaper_for_device_with_protected_paths(
    path: &Path,
    device_id: &str,
    protected_paths: &[PathBuf],
) -> Result<PathBuf, String> {
    let cache_dir = crate::config::Config::cache_dir().join("native-images");
    let compatible_path =
        prepare_compatible_image_with_protected_paths(path, &cache_dir, protected_paths)?;
    let (_apartment, wallpaper) = desktop_wallpaper()?;
    let wallpaper_path = HSTRING::from(compatible_path.to_string_lossy().as_ref());
    let device_id_string = device_id.to_string();
    let device_id = HSTRING::from(device_id);
    crate::apply_trace::event(
        "set-wallpaper-start",
        format!("target={device_id_string:?} compatible_path={compatible_path:?}"),
    );
    if let Err(error) = unsafe { wallpaper.SetWallpaper(&device_id, &wallpaper_path) } {
        crate::apply_trace::event(
            "set-wallpaper-failure",
            format!("target={device_id_string:?} error={error:?}"),
        );
        return Err(format!("Windows rejected the wallpaper change: {error}"));
    }
    crate::apply_trace::event(
        "set-wallpaper-complete",
        format!("target={device_id_string:?} expected_path={compatible_path:?}"),
    );
    unsafe { wallpaper.SetPosition(DWPOS_FILL) }
        .map_err(|error| format!("Wallpaper changed, but the Fill position failed: {error}"))?;
    Ok(compatible_path)
}

/// Restores a path that Windows previously reported for this monitor.
/// Unlike a new apply, this deliberately avoids conversion so rollback can
/// preserve the exact recorded per-monitor state.
pub fn restore_wallpaper_for_device(path: &Path, device_id: &str) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!(
            "Wallpaper image does not exist: {}",
            path.display()
        ));
    }
    let (_apartment, wallpaper) = desktop_wallpaper()?;
    let wallpaper_path = HSTRING::from(path.to_string_lossy().as_ref());
    let device_id = HSTRING::from(device_id);
    unsafe { wallpaper.SetWallpaper(&device_id, &wallpaper_path) }
        .map_err(|error| format!("Windows rejected the wallpaper restoration: {error}"))?;
    Ok(())
}

pub fn restore_wallpaper_state_for_device(
    path: &Path,
    device_id: &str,
    position: WallpaperPosition,
) -> Result<(), String> {
    if !path.is_file() {
        return Err(format!(
            "Wallpaper image does not exist: {}",
            path.display()
        ));
    }
    let (_apartment, wallpaper) = desktop_wallpaper()?;
    let wallpaper_path = HSTRING::from(path.to_string_lossy().as_ref());
    let device_id = HSTRING::from(device_id);
    unsafe { wallpaper.SetWallpaper(&device_id, &wallpaper_path) }
        .map_err(|error| format!("Windows rejected the wallpaper restoration: {error}"))?;
    unsafe { wallpaper.SetPosition(position) }
        .map_err(|error| format!("Windows rejected the wallpaper-position restoration: {error}"))?;
    Ok(())
}

pub fn get_position() -> Result<WallpaperPosition, String> {
    let (_apartment, wallpaper) = desktop_wallpaper()?;
    unsafe { wallpaper.GetPosition() }
        .map_err(|error| format!("Could not read the Windows wallpaper position: {error}"))
}

pub fn verify_wallpaper_state_for_device(
    expected_path: &Path,
    device_id: &str,
    expected_position: WallpaperPosition,
) -> Result<(), String> {
    let actual_path = get_wallpaper_for_device(device_id)?;
    if normalized_wallpaper_path(&actual_path) != normalized_wallpaper_path(expected_path) {
        return Err(format!(
            "Wallpaper restoration readback mismatch: Windows reports {}, expected {}",
            actual_path.display(),
            expected_path.display()
        ));
    }
    let actual_position = get_position()?;
    if actual_position != expected_position {
        return Err(format!(
            "Wallpaper-position readback mismatch: Windows reports {}, expected {}",
            actual_position.0, expected_position.0
        ));
    }
    Ok(())
}

pub fn normalized_wallpaper_path(path: &Path) -> String {
    let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let value = path.to_string_lossy().replace('\\', "/");
    let lower = value.to_ascii_lowercase();
    let value = if lower.starts_with("//?/unc/") {
        format!("//{}", &value[8..])
    } else if lower.starts_with("//?/") {
        value[4..].to_string()
    } else {
        value
    };
    let lexical_path = PathBuf::from(value);
    let mut normalized = PathBuf::new();
    for component in lexical_path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    let value = normalized.to_string_lossy().replace('\\', "/");
    let value = if normalized.has_root() && normalized.components().count() <= 2 {
        value
    } else {
        value.trim_end_matches('/').to_string()
    };
    value.to_ascii_lowercase()
}

pub fn get_wallpaper_for_device(device_id: &str) -> Result<PathBuf, String> {
    let (_apartment, wallpaper) = desktop_wallpaper()?;
    let device_id = HSTRING::from(device_id);
    let raw_path = unsafe { wallpaper.GetWallpaper(&device_id) }
        .map_err(|error| format!("Could not read the current Windows wallpaper: {error}"))?;
    let path = owned_wide_string(raw_path)?;
    if path.trim().is_empty() {
        Err("Windows returned an empty wallpaper path".to_string())
    } else {
        Ok(PathBuf::from(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn r3_long_native_cache_repair_reuses_valid_publication_without_provider_mutation() {
        use std::os::windows::ffi::OsStrExt;
        let fixture = TestDirectory::new("r3-long-cache");
        let source = source_png(&fixture.path, "source.png");
        let cache_dir = fixture
            .path
            .join("long Unicode 日本語 cache".repeat(4))
            .join("valid-cache-depth-".repeat(5));
        fs::create_dir_all(&cache_dir).unwrap();
        let cache_path = compatible_image_path(&source, &cache_dir).unwrap();
        assert!(cache_path.as_os_str().encode_wide().count() > 260);
        fs::write(&cache_path, b"malformed cache").unwrap();
        let ready = prepare_compatible_image(&source, &cache_dir).unwrap();
        assert_eq!(ready, cache_path);
        assert!(matches!(
            inspect_cached_bmp(&ready),
            Ok(CacheEntry::Valid { .. })
        ));
        let published = fs::read(&ready).unwrap();
        assert_eq!(
            prepare_compatible_image(&source, &cache_dir).unwrap(),
            ready
        );
        assert_eq!(fs::read(&ready).unwrap(), published);
        assert_no_owned_temporary_files(&cache_dir);
        eprintln!(
            "R3 native cache path={ready:?}, UTF16 units={}",
            ready.as_os_str().encode_wide().count()
        );
    }

    #[cfg(windows)]
    #[test]
    fn r3_native_before_call_rejection_cleans_stage_without_fabricating_recovery() {
        let fixture = TestDirectory::new("r3-before-call");
        let source = source_png(&fixture.path, "source.png");
        let cache_dir = fixture.path.join("native-images");
        fs::create_dir_all(&cache_dir).unwrap();
        let destination = compatible_image_path(&source, &cache_dir).unwrap();
        fs::write(&destination, b"original invalid cache").unwrap();
        let mut reject = |_: &Path, _: &Path, _: &Path| {
            Err(file_publication::before_replace_file_error(io::Error::new(
                io::ErrorKind::InvalidInput,
                "injected component validation rejection",
            )))
        };
        let error = prepare_compatible_image_inner_with_replacer(
            &source,
            &cache_dir,
            &[],
            |_| Ok(()),
            &mut reject,
        )
        .unwrap_err();
        assert!(error.contains("before ReplaceFileW"));
        assert!(!error.contains("ReplaceFileW returned"));
        assert_eq!(fs::read(&destination).unwrap(), b"original invalid cache");
        assert_no_owned_temporary_files(&cache_dir);
        assert_eq!(fs::read_dir(&cache_dir).unwrap().count(), 1);
    }
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "wallpaper-switcher-native-cache-{label}-{}-{nonce}",
                std::process::id()
            ));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum CacheFailurePoint {
        Write,
        Flush,
        Validation,
        Publication,
    }

    impl CacheFailurePoint {
        fn label(self) -> &'static str {
            match self {
                Self::Write => "temporary-write",
                Self::Flush => "temporary-flush",
                Self::Validation => "temporary-validation",
                Self::Publication => "publication",
            }
        }
    }

    fn prepare_compatible_image_with_fault(
        source: &Path,
        cache_dir: &Path,
        failure: CacheFailurePoint,
    ) -> Result<PathBuf, String> {
        prepare_compatible_image_inner(source, cache_dir, &[], |stage| {
            if stage == failure.label() {
                Err(io::Error::new(
                    io::ErrorKind::Other,
                    format!("injected native-cache failure at {stage}"),
                ))
            } else {
                Ok(())
            }
        })
    }

    #[derive(Debug, Clone, Copy)]
    enum PartialPublicationEffect {
        DestinationUnchanged,
        DestinationAbsent,
        OriginalMovedToKnownRecovery,
    }

    impl PartialPublicationEffect {
        fn error_code(self) -> i32 {
            match self {
                Self::DestinationUnchanged => 1175,
                Self::DestinationAbsent => 1176,
                Self::OriginalMovedToKnownRecovery => 1177,
            }
        }
    }

    fn prepare_compatible_image_with_partial_publication(
        source: &Path,
        cache_dir: &Path,
        effect: PartialPublicationEffect,
        recovery_path: &mut Option<PathBuf>,
    ) -> Result<PathBuf, String> {
        let mut maybe_fail = |_| Ok(());
        let mut replace = |replacement: &Path, destination: &Path, backup: &Path| {
            *recovery_path = Some(backup.to_path_buf());
            match effect {
                PartialPublicationEffect::DestinationUnchanged => {}
                PartialPublicationEffect::DestinationAbsent => {
                    fs::remove_file(destination).unwrap();
                }
                PartialPublicationEffect::OriginalMovedToKnownRecovery => {
                    fs::rename(destination, backup).unwrap();
                }
            }
            let _ = replacement;
            Err(io::Error::from_raw_os_error(effect.error_code()))
        };
        prepare_compatible_image_inner_with_replacer(
            source,
            cache_dir,
            &[],
            &mut maybe_fail,
            &mut replace,
        )
    }

    fn source_png(root: &Path, name: &str) -> PathBuf {
        let path = root.join(name);
        image::RgbaImage::from_pixel(4, 3, image::Rgba([88, 120, 164, 255]))
            .save_with_format(&path, image::ImageFormat::Png)
            .unwrap();
        path
    }

    fn valid_bmp_bytes(root: &Path) -> Vec<u8> {
        let path = root.join("valid.bmp");
        image::RgbaImage::from_pixel(4, 3, image::Rgba([164, 120, 88, 255]))
            .save_with_format(&path, image::ImageFormat::Bmp)
            .unwrap();
        fs::read(path).unwrap()
    }

    fn assert_no_owned_temporary_files(cache_dir: &Path) {
        if !cache_dir.exists() {
            return;
        }
        let leftovers: Vec<_> = fs::read_dir(cache_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".native-cache-tmp-"))
            .collect();
        assert!(
            leftovers.is_empty(),
            "owned native-cache temporary files remained: {leftovers:?}"
        );
    }

    #[test]
    fn converts_formats_that_windows_wallpaper_does_not_reliably_accept() {
        assert!(!needs_conversion(Path::new("wallpaper.jpg")));
        assert!(!needs_conversion(Path::new("wallpaper.JPEG")));
        assert!(!needs_conversion(Path::new("wallpaper.bmp")));
        assert!(needs_conversion(Path::new("wallpaper.png")));
        assert!(needs_conversion(Path::new("wallpaper.webp")));
    }

    #[test]
    fn valid_existing_bmp_is_reused_without_rewriting_or_decoding_source() {
        let fixture = TestDirectory::new("valid-reuse");
        let source = fixture.path.join("source.png");
        fs::write(&source, b"not a decodable source").unwrap();
        let cache_dir = fixture.path.join("native-images");
        fs::create_dir_all(&cache_dir).unwrap();
        let cache_path = compatible_image_path(&source, &cache_dir).unwrap();
        let bytes = valid_bmp_bytes(&fixture.path);
        fs::write(&cache_path, &bytes).unwrap();
        let modified = fs::metadata(&cache_path).unwrap().modified().unwrap();

        assert_eq!(
            prepare_compatible_image(&source, &cache_dir).unwrap(),
            cache_path
        );
        assert_eq!(fs::read(&cache_path).unwrap(), bytes);
        assert_eq!(
            fs::metadata(&cache_path).unwrap().modified().unwrap(),
            modified
        );
        assert_no_owned_temporary_files(&cache_dir);
    }

    #[test]
    fn empty_malformed_truncated_and_wrong_format_cache_entries_are_repaired() {
        let fixture = TestDirectory::new("invalid-repair");
        let source = source_png(&fixture.path, "source.png");
        let cache_dir = fixture.path.join("native-images");
        fs::create_dir_all(&cache_dir).unwrap();
        let cache_path = compatible_image_path(&source, &cache_dir).unwrap();
        let valid_bmp = valid_bmp_bytes(&fixture.path);
        let wrong_format = fs::read(&source).unwrap();
        let variants = [
            ("empty", Vec::new()),
            ("malformed", b"not a BMP".to_vec()),
            ("truncated", valid_bmp[..valid_bmp.len() / 2].to_vec()),
            ("wrong-format", wrong_format),
        ];

        for (label, bytes) in variants {
            fs::write(&cache_path, bytes).unwrap();
            let prepared = prepare_compatible_image(&source, &cache_dir)
                .unwrap_or_else(|error| panic!("{label} cache was not repaired: {error}"));
            assert_eq!(prepared, cache_path);
            assert!(matches!(
                inspect_cached_bmp(&cache_path),
                Ok(CacheEntry::Valid { .. })
            ));
            assert_no_owned_temporary_files(&cache_dir);
        }
    }

    #[test]
    fn unreadable_cache_entry_is_not_treated_as_corrupt_or_overwritten() {
        let fixture = TestDirectory::new("unreadable-cache");
        let source = source_png(&fixture.path, "source.png");
        let cache_dir = fixture.path.join("native-images");
        fs::create_dir_all(&cache_dir).unwrap();
        let cache_path = compatible_image_path(&source, &cache_dir).unwrap();
        fs::create_dir_all(&cache_path).unwrap();
        let marker = cache_path.join("must-remain");
        fs::write(&marker, b"unrelated directory content").unwrap();

        let error = prepare_compatible_image(&source, &cache_dir).unwrap_err();

        assert!(error.contains("unreadable"), "unexpected error: {error}");
        assert!(cache_path.is_dir());
        assert_eq!(fs::read(&marker).unwrap(), b"unrelated directory content");
        assert_no_owned_temporary_files(&cache_dir);
    }

    #[test]
    fn missing_entry_is_fully_written_validated_and_published() {
        let fixture = TestDirectory::new("missing-entry");
        let source = source_png(&fixture.path, "source.png");
        let cache_dir = fixture.path.join("native-images");
        let cache_path = compatible_image_path(&source, &cache_dir).unwrap();

        assert!(!cache_path.exists());
        assert_eq!(
            prepare_compatible_image(&source, &cache_dir).unwrap(),
            cache_path
        );
        assert!(matches!(
            inspect_cached_bmp(&cache_path),
            Ok(CacheEntry::Valid { .. })
        ));
        assert_no_owned_temporary_files(&cache_dir);
    }

    #[test]
    fn failed_source_decoding_leaves_existing_final_unchanged() {
        let fixture = TestDirectory::new("source-failure");
        let source = fixture.path.join("source.png");
        fs::write(&source, b"source decode failure").unwrap();
        let cache_dir = fixture.path.join("native-images");
        fs::create_dir_all(&cache_dir).unwrap();
        let cache_path = compatible_image_path(&source, &cache_dir).unwrap();
        let old_final = b"old invalid cache entry";
        fs::write(&cache_path, old_final).unwrap();

        let error = prepare_compatible_image(&source, &cache_dir).unwrap_err();

        assert!(
            error.contains("Could not decode"),
            "unexpected error: {error}"
        );
        assert_eq!(fs::read(&cache_path).unwrap(), old_final);
        assert_no_owned_temporary_files(&cache_dir);
    }

    #[test]
    fn staged_write_flush_validation_and_publication_failures_never_report_ready_path() {
        for failure in [
            CacheFailurePoint::Write,
            CacheFailurePoint::Flush,
            CacheFailurePoint::Validation,
            CacheFailurePoint::Publication,
        ] {
            let fixture = TestDirectory::new(failure.label());
            let source = source_png(&fixture.path, "source.png");
            let cache_dir = fixture.path.join("native-images");
            fs::create_dir_all(&cache_dir).unwrap();
            let cache_path = compatible_image_path(&source, &cache_dir).unwrap();
            let old_final = format!("old final for {}", failure.label());
            let unrelated = cache_dir.join("unrelated-cache-file.txt");
            fs::write(&cache_path, old_final.as_bytes()).unwrap();
            fs::write(&unrelated, b"do not delete").unwrap();

            let result = prepare_compatible_image_with_fault(&source, &cache_dir, failure);

            assert!(
                result.is_err(),
                "{} unexpectedly succeeded",
                failure.label()
            );
            assert_eq!(fs::read(&cache_path).unwrap(), old_final.as_bytes());
            assert_eq!(fs::read(&unrelated).unwrap(), b"do not delete");
            assert_no_owned_temporary_files(&cache_dir);
        }
    }

    #[test]
    fn documented_partial_replace_failures_retain_only_known_valid_recovery_copies() {
        for effect in [
            PartialPublicationEffect::DestinationUnchanged,
            PartialPublicationEffect::DestinationAbsent,
            PartialPublicationEffect::OriginalMovedToKnownRecovery,
        ] {
            let fixture = TestDirectory::new("partial-publication");
            let source = source_png(&fixture.path, "source.png");
            let cache_dir = fixture.path.join("native-images");
            fs::create_dir_all(&cache_dir).unwrap();
            let cache_path = compatible_image_path(&source, &cache_dir).unwrap();
            fs::write(&cache_path, b"old invalid cache").unwrap();
            let mut recovery_path = None;

            let error = prepare_compatible_image_with_partial_publication(
                &source,
                &cache_dir,
                effect,
                &mut recovery_path,
            )
            .unwrap_err();

            assert!(error.contains("ReplaceFileW"), "unexpected error: {error}");
            assert!(
                error.contains("staged recovery"),
                "unexpected error: {error}"
            );
            let staged = fs::read_dir(&cache_dir)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .find(|path| {
                    path.file_name()
                        .is_some_and(|name| name.to_string_lossy().contains(".native-cache-tmp-"))
                })
                .expect("validated staged cache must be retained");
            assert!(matches!(
                inspect_cached_bmp(&staged),
                Ok(CacheEntry::Valid { .. })
            ));

            match effect {
                PartialPublicationEffect::DestinationUnchanged => {
                    assert_eq!(fs::read(&cache_path).unwrap(), b"old invalid cache");
                }
                PartialPublicationEffect::DestinationAbsent => {
                    assert!(!cache_path.exists());
                }
                PartialPublicationEffect::OriginalMovedToKnownRecovery => {
                    let recovery_path = recovery_path.expect("ReplaceFileW backup path");
                    assert_eq!(fs::read(recovery_path).unwrap(), b"old invalid cache");
                    assert!(error.contains("ERROR_UNABLE_TO_MOVE_REPLACEMENT_2"));
                }
            }
        }
    }

    #[test]
    fn invalid_replacement_is_refused_when_it_matches_a_protected_provider_path() {
        let fixture = TestDirectory::new("protected-replacement");
        let source = source_png(&fixture.path, "source.png");
        let cache_dir = fixture.path.join("native-images");
        fs::create_dir_all(&cache_dir).unwrap();
        let cache_path = compatible_image_path(&source, &cache_dir).unwrap();
        let old_final = b"invalid but protected";
        fs::write(&cache_path, old_final).unwrap();

        let error = prepare_compatible_image_with_protected_paths(
            &source,
            &cache_dir,
            std::slice::from_ref(&cache_path),
        )
        .unwrap_err();

        assert!(error.contains("protected"), "unexpected error: {error}");
        assert_eq!(fs::read(&cache_path).unwrap(), old_final);
        assert_no_owned_temporary_files(&cache_dir);
    }

    #[test]
    fn compatible_lookup_preserves_key_and_remains_read_only() {
        let fixture = TestDirectory::new("read-only-lookup");
        let source = source_png(&fixture.path, "source.png");
        let cache_dir = fixture.path.join("native-images");
        fs::create_dir_all(&cache_dir).unwrap();
        let compatible = compatible_image_path(&source, &cache_dir).unwrap();
        assert_eq!(
            compatible,
            compatible_image_path(&source, &cache_dir).unwrap()
        );
        assert_eq!(
            compatible.extension().and_then(|value| value.to_str()),
            Some("bmp")
        );
        assert!(compatible
            .file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.starts_with("native-") && value.ends_with(".bmp")));

        let old_final = b"lookup must not rewrite this";
        fs::write(&compatible, old_final).unwrap();
        assert_eq!(
            compatible_image_path(&source, &cache_dir).unwrap(),
            compatible
        );
        assert_eq!(fs::read(&compatible).unwrap(), old_final);
    }

    #[test]
    #[ignore = "restores the path supplied through WSL_RESTORE_WALLPAPER"]
    fn restore_wallpaper_path_from_environment() {
        let path = std::env::var_os("WSL_RESTORE_WALLPAPER")
            .map(PathBuf::from)
            .expect("WSL_RESTORE_WALLPAPER");
        let monitor = monitors().unwrap().into_iter().next().unwrap();
        restore_wallpaper_for_device(&path, &monitor.device_id).unwrap();
        let current = get_wallpaper_for_device(&monitor.device_id).unwrap();
        assert_eq!(
            std::fs::canonicalize(current).unwrap(),
            std::fs::canonicalize(path).unwrap()
        );
    }
}

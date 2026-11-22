use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn unique_recovery_path(destination: &Path, suffix: &str) -> PathBuf {
    let parent = destination.parent().unwrap_or_else(|| Path::new("."));
    let filename = destination
        .file_name()
        .unwrap_or_else(|| std::ffi::OsStr::new("published-file"));
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());

    for attempt in 0..32u32 {
        let mut name = std::ffi::OsString::from(".");
        name.push(filename);
        name.push(format!(
            ".{suffix}-{}-{nonce}-{attempt}",
            std::process::id()
        ));
        let candidate = parent.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }

    let mut name = std::ffi::OsString::from(".");
    name.push(filename);
    name.push(format!(
        ".{suffix}-{}-{nonce}-exhausted",
        std::process::id()
    ));
    parent.join(name)
}

pub(crate) fn remove_if_present(path: &Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        result => result,
    }
}

#[derive(Debug)]
pub(crate) struct StagedPublicationError {
    pub(crate) error: io::Error,
    pub(crate) replacement: PathBuf,
    pub(crate) destination: PathBuf,
    pub(crate) recovery: Option<PathBuf>,
    pub(crate) replace_file_called: bool,
    pub(crate) publication_succeeded: bool,
}

pub(crate) fn publish_staged_file_with<F>(
    replacement: &Path,
    destination: &Path,
    recovery_label: &str,
    replace_existing: &mut F,
) -> Result<(), StagedPublicationError>
where
    F: FnMut(&Path, &Path, &Path) -> io::Result<()>,
{
    if destination.exists() {
        let recovery = unique_recovery_path(destination, recovery_label);
        match replace_existing(replacement, destination, &recovery) {
            Ok(()) => {
                if let Err(error) = remove_if_present(&recovery) {
                    return Err(StagedPublicationError {
                        error,
                        replacement: replacement.to_path_buf(),
                        destination: destination.to_path_buf(),
                        recovery: Some(recovery),
                        replace_file_called: true,
                        publication_succeeded: true,
                    });
                }
                Ok(())
            }
            Err(error) => Err(StagedPublicationError {
                replace_file_called: replace_file_was_called(&error),
                error,
                replacement: replacement.to_path_buf(),
                destination: destination.to_path_buf(),
                recovery: Some(recovery),
                publication_succeeded: false,
            }),
        }
    } else {
        fs::rename(replacement, destination).map_err(|error| StagedPublicationError {
            error,
            replacement: replacement.to_path_buf(),
            destination: destination.to_path_buf(),
            recovery: None,
            replace_file_called: false,
            publication_succeeded: false,
        })
    }
}

/// Disposable owners can publish a flushed stage without ever replacing a destination.
/// The create-only rename preserves a concurrent destination without a two-link window.
pub(crate) fn publish_staged_file_create_only(
    replacement: &Path,
    destination: &Path,
) -> io::Result<()> {
    rename_file_create_only(replacement, destination)
}

#[cfg(windows)]
pub(crate) fn rename_file_create_only(source: &Path, destination: &Path) -> io::Result<()> {
    let base = std::env::current_dir()?;
    let source = publication_windows_path(source, &base)?;
    let destination = publication_windows_path(destination, &base)?;
    // No replace or cross-volume copy flags. Both paths are in the same directory.
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            0,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(crate) fn replace_existing_file(
    replacement: &Path,
    destination: &Path,
    backup: &Path,
) -> io::Result<()> {
    #[cfg(windows)]
    {
        // Resolve every relative argument against one captured cwd, then validate
        // all three paths before any publication. No file needs to exist here.
        let (replacement, destination, backup) = (|| {
            let base = std::env::current_dir()?;
            let replacement = publication_windows_path(replacement, &base)?;
            let destination = publication_windows_path(destination, &base)?;
            let backup = publication_windows_path(backup, &base)?;
            for path in [&replacement, &destination, &backup] {
                validate_volume_components(path)?;
            }
            Ok::<_, io::Error>((replacement, destination, backup))
        })()
        .map_err(before_replace_file_error)?;

        // REPLACEFILE_WRITE_THROUGH is unsupported. The replacement is
        // already flushed and synchronized before this same-directory swap.
        let result = unsafe {
            ReplaceFileW(
                destination.as_ptr(),
                replacement.as_ptr(),
                backup.as_ptr(),
                0,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        if result == 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = backup;
        fs::rename(replacement, destination)
    }
}

pub(crate) fn replace_file_error_name(error: &io::Error) -> &'static str {
    if !replace_file_was_called(error) {
        return "publication validation failed before ReplaceFileW";
    }
    match error.raw_os_error() {
        Some(1175) => "ERROR_UNABLE_TO_REMOVE_REPLACED",
        Some(1176) => "ERROR_UNABLE_TO_MOVE_REPLACEMENT",
        Some(1177) => "ERROR_UNABLE_TO_MOVE_REPLACEMENT_2",
        _ => "unclassified ReplaceFileW failure",
    }
}

#[derive(Debug)]
struct BeforeReplaceFile(io::Error);

impl std::fmt::Display for BeforeReplaceFile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "publication validation before ReplaceFileW: {}",
            self.0
        )
    }
}

impl std::error::Error for BeforeReplaceFile {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

pub(crate) fn before_replace_file_error(error: io::Error) -> io::Error {
    io::Error::new(error.kind(), BeforeReplaceFile(error))
}

pub(crate) fn replace_file_was_called(error: &io::Error) -> bool {
    // Unmarked errors are conservatively treated as API attempts, including
    // injected partial-effect errors. ErrorKind alone cannot establish phase.
    !failed_before_replace_file(error)
}

pub(crate) fn failed_before_replace_file(error: &io::Error) -> bool {
    error
        .get_ref()
        .is_some_and(|inner| inner.is::<BeforeReplaceFile>())
}

#[cfg(windows)]
fn invalid_publication_path(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(windows)]
fn publication_windows_path(path: &Path, base: &Path) -> io::Result<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;
    use std::path::{Component, Prefix};
    let raw: Vec<u16> = path.as_os_str().encode_wide().collect();
    if raw.is_empty() || raw.contains(&0) {
        return Err(invalid_publication_path(
            "publication path is empty or contains an embedded NUL",
        ));
    }
    if raw.len() >= 32_767 {
        return Err(invalid_publication_path(
            "publication whole path exceeds the extended Windows path bound",
        ));
    }
    let prefix = match path.components().next() {
        Some(Component::Prefix(prefix)) => Some(prefix.kind()),
        _ => None,
    };
    let extended = match prefix {
        Some(Prefix::VerbatimDisk(_)) | Some(Prefix::VerbatimUNC(_, _)) if path.is_absolute() => {
            true
        }
        Some(Prefix::Disk(_)) | Some(Prefix::UNC(_, _)) if path.is_absolute() => false,
        None if !path.has_root() => false,
        _ => {
            return Err(invalid_publication_path(
                "unsupported publication path namespace or drive/root-relative form",
            ))
        }
    };
    if extended {
        if raw.contains(&(b'/' as u16))
            || path
                .components()
                .any(|component| matches!(component, Component::CurDir | Component::ParentDir))
        {
            return Err(invalid_publication_path(
                "extended publication paths must use backslashes and no dot navigation",
            ));
        }
    } else {
        // Avoid silently changing a filename when Win32 normalizes an ordinary path.
        for component in path.components() {
            if let Component::Normal(name) = component {
                let units: Vec<_> = name.encode_wide().collect();
                if matches!(units.last(), Some(value) if *value == b'.' as u16 || *value == b' ' as u16)
                {
                    return Err(invalid_publication_path(
                        "ordinary publication filename has ambiguous trailing dot or space",
                    ));
                }
            }
        }
    }
    let mut result = if extended {
        raw
    } else {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            // Cwd can itself be verbatim. Resolve ordinary relative navigation
            // lexically before joining, rather than asking verbatim Win32 paths
            // to normalize dot components that their namespace does not allow.
            let mut resolved = base.to_path_buf();
            for component in path.components() {
                match component {
                    Component::CurDir => {}
                    Component::ParentDir => {
                        resolved.pop();
                    }
                    Component::Normal(name) => resolved.push(name),
                    _ => {
                        return Err(invalid_publication_path(
                            "unsupported relative publication path",
                        ))
                    }
                }
            }
            resolved
        };
        if !absolute.is_absolute() {
            return Err(invalid_publication_path(
                "publication base must be an absolute drive or UNC path",
            ));
        }
        let input: Vec<u16> = absolute
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        if input[..input.len() - 1].contains(&0) {
            return Err(invalid_publication_path(
                "publication base contains an embedded NUL",
            ));
        }
        let mut output = vec![0u16; 32_768];
        let length = unsafe {
            GetFullPathNameW(
                input.as_ptr(),
                output.len() as u32,
                output.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        if length == 0 {
            return Err(io::Error::last_os_error());
        }
        if length as usize >= output.len() {
            return Err(invalid_publication_path(
                "resolved publication whole path exceeds the extended Windows path bound",
            ));
        }
        output.truncate(length as usize);
        if output.starts_with(&[92, 92, 63, 92]) {
            output
        } else if output.starts_with(&[92, 92]) {
            let mut prefixed: Vec<u16> = "\\\\?\\UNC\\".encode_utf16().collect();
            prefixed.extend_from_slice(&output[2..]);
            prefixed
        } else {
            let mut prefixed: Vec<u16> = "\\\\?\\".encode_utf16().collect();
            prefixed.extend(output);
            prefixed
        }
    };
    // Validate even pre-prefixed inputs; never double-prefix a device namespace.
    validate_component_lengths(&result, u32::MAX)?;
    if result.len() >= 32_767 {
        return Err(invalid_publication_path(
            "extended publication whole path exceeds the Windows bound",
        ));
    }
    result.push(0);
    Ok(result)
}

#[cfg(windows)]
fn publication_volume_root(path: &[u16]) -> io::Result<Vec<u16>> {
    let path = path.strip_suffix(&[0]).unwrap_or(path);
    let unc: Vec<u16> = "\\\\?\\UNC\\".encode_utf16().collect();
    let drive: Vec<u16> = "\\\\?\\".encode_utf16().collect();
    let end = if path.starts_with(&unc) {
        let rest = &path[unc.len()..];
        let server = rest
            .iter()
            .position(|unit| *unit == 92)
            .filter(|index| *index > 0)
            .ok_or_else(|| invalid_publication_path("publication UNC server/share is missing"))?;
        let share_start = server + 1;
        let share = rest[share_start..]
            .iter()
            .position(|unit| *unit == 92)
            .filter(|index| *index > 0)
            .ok_or_else(|| {
                invalid_publication_path("publication UNC share or file path is missing")
            })?;
        unc.len() + share_start + share + 1
    } else if path.starts_with(&drive)
        && path.len() > 7
        && ((65..=90).contains(&path[4]) || (97..=122).contains(&path[4]))
        && path[5] == 58
        && path[6] == 92
    {
        7
    } else {
        return Err(invalid_publication_path(
            "publication supports only absolute extended drive and UNC file paths",
        ));
    };
    if end >= path.len() {
        return Err(invalid_publication_path("publication path has no filename"));
    }
    let mut root = path[..end].to_vec();
    root.push(0);
    Ok(root)
}

#[cfg(windows)]
fn validate_component_lengths(path: &[u16], maximum: u32) -> io::Result<()> {
    let root = publication_volume_root(path)?;
    let body = path.strip_suffix(&[0]).unwrap_or(path);
    for component in body[root.len() - 1..].split(|unit| *unit == 92) {
        if component.is_empty()
            || component == [46]
            || component == [46, 46]
            || component
                .iter()
                .any(|unit| *unit < 32 || [34, 42, 58, 60, 62, 63, 124].contains(unit))
        {
            return Err(invalid_publication_path("publication path contains an empty, navigational, or unsupported filename component"));
        }
        if component.len() > maximum as usize {
            return Err(invalid_publication_path(
                "publication component exceeds the volume's filename-length limit",
            ));
        }
    }
    Ok(())
}

#[cfg(windows)]
fn validate_volume_components(path: &[u16]) -> io::Result<()> {
    let root = publication_volume_root(path)?;
    let mut maximum = 0;
    let result = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            &mut maximum,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            0,
        )
    };
    if result == 0 {
        return Err(io::Error::last_os_error());
    }
    if maximum == 0 {
        return Err(invalid_publication_path(
            "publication volume component limit is unavailable",
        ));
    }
    validate_component_lengths(path, maximum)
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetFullPathNameW(
        name: *const u16,
        length: u32,
        buffer: *mut u16,
        part: *mut *mut u16,
    ) -> u32;
    fn GetVolumeInformationW(
        root: *const u16,
        name: *mut u16,
        name_length: u32,
        serial: *mut u32,
        maximum_component: *mut u32,
        flags: *mut u32,
        filesystem: *mut u16,
        filesystem_length: u32,
    ) -> i32;
    fn ReplaceFileW(
        replaced_file_name: *const u16,
        replacement_file_name: *const u16,
        backup_file_name: *const u16,
        replace_flags: u32,
        exclude: *mut std::ffi::c_void,
        reserved: *mut std::ffi::c_void,
    ) -> i32;
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::os::windows::ffi::OsStrExt;

    #[test]
    fn p37_create_only_publication_is_atomic_when_stage_cannot_be_renamed() {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::{FILE_SHARE_READ, FILE_SHARE_WRITE};
        let (_, root) = crate::poster_worker::cache::tests::fixture();
        let stage = root.join("publication.tmp");
        let destination = root.join("published.bin");
        fs::write(&stage, b"complete stage").unwrap();
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .open(&stage)
            .unwrap();
        assert!(publish_staged_file_create_only(&stage, &destination).is_err());
        assert!(
            !destination.exists(),
            "a failed rename must not publish a second link"
        );
        assert_eq!(fs::read(&stage).unwrap(), b"complete stage");
        drop(held);
        publish_staged_file_create_only(&stage, &destination).unwrap();
        assert!(!stage.exists());
        assert_eq!(fs::read(&destination).unwrap(), b"complete stage");
        fs::write(&stage, b"competing stage").unwrap();
        assert!(publish_staged_file_create_only(&stage, &destination).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"complete stage");
        assert_eq!(fs::read(&stage).unwrap(), b"competing stage");
    }

    fn converted(path: &str, base: &str) -> String {
        let units = publication_windows_path(Path::new(path), Path::new(base)).unwrap();
        String::from_utf16(&units[..units.len() - 1]).unwrap()
    }

    #[test]
    fn r3_drive_unc_relative_and_extended_conversion_is_idempotent() {
        for (input, expected) in [
            (
                r"C:\folder\child\..\日本語 space.txt",
                r"\\?\C:\folder\日本語 space.txt",
            ),
            (
                r"\\server\share\folder\new backup.txt",
                r"\\?\UNC\server\share\folder\new backup.txt",
            ),
            (
                r"\\?\C:\folder\existing extended.txt",
                r"\\?\C:\folder\existing extended.txt",
            ),
            (
                r"\\?\UNC\server\share\future.txt",
                r"\\?\UNC\server\share\future.txt",
            ),
        ] {
            let result = converted(input, r"C:\base");
            assert_eq!(result, expected);
            assert_eq!(converted(&result, r"D:\unrelated"), result);
        }
        assert_eq!(
            converted("nested/../future backup.txt", r"C:\base"),
            r"\\?\C:\base\future backup.txt"
        );
        assert_eq!(
            converted("nested/../future backup.txt", r"\\?\C:\base"),
            r"\\?\C:\base\future backup.txt"
        );
        assert_eq!(
            converted("future.txt", r"\\?\UNC\server\share\base"),
            r"\\?\UNC\server\share\base\future.txt"
        );
    }

    #[test]
    fn r3_rejects_unsupported_paths_nul_and_distinguishes_component_limits() {
        for path in [
            "",
            "C:relative.txt",
            r"\root-relative.txt",
            r"\\.\pipe\device",
            r"\\?\GLOBALROOT\Device\file",
            r"\\?\C:\nested\..\file",
            r"\\?\C:/mixed/file",
            r"\\?\UNC\server",
            r"\\?\UNC\server\share",
            "C:\\folder\\embedded\0nul.txt",
            r"C:\folder\file:stream",
            r"C:\folder\wild*.txt",
        ] {
            assert!(
                publication_windows_path(Path::new(path), Path::new(r"C:\base")).is_err(),
                "{path:?}"
            );
        }
        let component = format!(r"C:\folder\{}", "x".repeat(256));
        let wide = publication_windows_path(Path::new(&component), Path::new(r"C:\base")).unwrap();
        let error = validate_component_lengths(&wide, 255).unwrap_err();
        assert!(error.to_string().contains("component"));
        let whole = format!(r"C:\{}", "valid-component\\".repeat(3000));
        let error = publication_windows_path(Path::new(&whole), Path::new(r"C:\base")).unwrap_err();
        assert!(error.to_string().contains("whole path"));
    }

    #[test]
    fn r3_utf16_and_generated_recovery_names_are_lossless() {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;
        let mut wide: Vec<u16> = r"\\?\C:\folder\unpaired-".encode_utf16().collect();
        wide.push(0xd800);
        wide.extend(".txt".encode_utf16());
        let path = PathBuf::from(OsString::from_wide(&wide));
        let result = publication_windows_path(&path, Path::new(r"C:\base")).unwrap();
        assert_eq!(&result[..result.len() - 1], &wide);
        let backup = unique_recovery_path(&path, "recovery");
        let backup_wide: Vec<_> = backup.as_os_str().encode_wide().collect();
        assert!(backup_wide.contains(&0xd800));
        assert!(!backup_wide.contains(&0xfffd));
        assert!(!backup.exists());
    }

    #[test]
    fn r3_invalid_backup_fails_before_replacement_and_locked_destination_stays_truthful() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = std::env::temp_dir().join(format!("r3-failure-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let destination = root.join("destination.txt");
        let replacement = root.join("replacement.txt");
        let backup = root.join("backup.txt");
        fs::write(&destination, b"old").unwrap();
        fs::write(&replacement, b"new").unwrap();
        let invalid_backup = root.join("nul\0backup.txt");
        let invalid =
            replace_existing_file(&replacement, &destination, &invalid_backup).unwrap_err();
        assert_eq!(invalid.kind(), io::ErrorKind::InvalidInput);
        assert!(!replace_file_was_called(&invalid));
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert_eq!(fs::read(&replacement).unwrap(), b"new");
        let locked = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&destination)
            .unwrap();
        assert!(replace_existing_file(&replacement, &destination, &backup).is_err());
        assert_eq!(fs::read(&destination).unwrap(), b"old");
        assert_eq!(fs::read(&replacement).unwrap(), b"new");
        assert!(!backup.exists());
        drop(locked);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn r3_pre_call_provenance_does_not_depend_on_error_kind() {
        let validation = before_replace_file_error(io::Error::from_raw_os_error(87));
        let api = io::Error::from_raw_os_error(87);
        assert_eq!(validation.kind(), api.kind());
        assert!(!replace_file_was_called(&validation));
        assert!(replace_file_was_called(&api));
        for code in [1175, 1176, 1177] {
            assert!(replace_file_was_called(&io::Error::from_raw_os_error(code)));
        }
    }

    #[test]
    fn r3_real_drive_relative_and_extended_publication_keeps_exact_backups() {
        use std::os::windows::ffi::OsStringExt;
        let base = std::env::current_dir().unwrap();
        let relative_root = PathBuf::from("target").join("r12").join(format!(
            "r3-relative-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
        ));
        let owned_root = base.join(&relative_root);
        fs::create_dir_all(&owned_root).unwrap();
        let destination = relative_root.join("original 日本語.txt");
        let replacement = relative_root.join("stage space.txt");
        let backup = relative_root.join("not-yet-existing.bak");
        fs::write(&destination, b"one").unwrap();
        fs::write(&replacement, b"two").unwrap();
        replace_existing_file(&replacement, &destination, &backup).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"two");
        assert_eq!(fs::read(&backup).unwrap(), b"one");
        fs::write(&replacement, b"three").unwrap();
        let extended_destination = PathBuf::from(std::ffi::OsString::from_wide(
            &publication_windows_path(&destination, &base)
                .unwrap()
                .into_iter()
                .take_while(|unit| *unit != 0)
                .collect::<Vec<_>>(),
        ));
        let extended_replacement = replacement.canonicalize().unwrap();
        let next_backup = base.join(&relative_root).join("second future.bak");
        replace_existing_file(&extended_replacement, &extended_destination, &next_backup).unwrap();
        assert_eq!(fs::read(&destination).unwrap(), b"three");
        assert_eq!(fs::read(&next_backup).unwrap(), b"two");
        assert!(owned_root.starts_with(base.join("target").join("r12")));
        fs::remove_dir_all(owned_root).unwrap();
    }

    #[test]
    fn r3_replacement_publishes_long_paths_and_preserves_backup() {
        let root = std::env::temp_dir().join(format!("r3-publication-{}", std::process::id()));
        let directory = root
            .join("publication Unicode 日本語 spaces".repeat(4))
            .join("valid-directory-component-".repeat(5));
        fs::create_dir_all(&directory).unwrap();
        let destination = directory.join("original.txt");
        let replacement = directory.join("staged.txt");
        let backup = unique_recovery_path(&destination, "long-generated-recovery");
        for path in [&destination, &replacement, &backup] {
            assert!(path.as_os_str().encode_wide().count() > 260, "{path:?}");
        }
        fs::write(&destination, b"original exact bytes").unwrap();
        fs::write(&replacement, b"new exact bytes").unwrap();
        assert!(!backup.exists());
        let result = replace_existing_file(&replacement, &destination, &backup);
        eprintln!("R3 destination={destination:?}, replacement={replacement:?}, backup={backup:?}, result={result:?}");
        assert!(
            result.is_ok(),
            "valid long publication after staging failed: {result:?}"
        );
        assert_eq!(fs::read(&destination).unwrap(), b"new exact bytes");
        assert_eq!(fs::read(&backup).unwrap(), b"original exact bytes");
        assert!(!replacement.exists());
        fs::remove_dir_all(root).unwrap();
    }
}

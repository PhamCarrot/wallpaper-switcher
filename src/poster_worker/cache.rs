//! The only disposable files owned here are the current writer's fixed-size RGBA entries.
use std::{
    fs::{File, OpenOptions},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::{Path, PathBuf},
};
use windows_sys::Win32::{Foundation::*, Storage::FileSystem::*};

#[derive(Debug, Clone, Default)]
pub struct Report {
    pub epoch: u64,
    pub deleted: usize,
    pub retained: usize,
    pub failed: usize,
    pub error: Option<String>,
}
impl Report {
    pub fn summary(&self) -> String {
        format!(
            "Cache operation acknowledged: {} deleted, {} retained, {} failed{}",
            self.deleted,
            self.retained,
            self.failed,
            self.error
                .as_ref()
                .map(|e| format!(" · {e}"))
                .unwrap_or_default()
        )
    }
}
pub struct Request {
    pub profile: PathBuf,
    pub root: PathBuf,
    pub names: Option<Vec<String>>,
    pub epoch: u64,
    pub colour: bool,
    pub acknowledge: Box<dyn FnOnce(Report) + Send>,
}
fn pin_directory(path: &Path) -> Result<File, String> {
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .map_err(|e| e.to_string())?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    if !meta.is_dir() || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err("Reparse or non-directory cache ancestor refused".into());
    }
    Ok(file)
}
fn owned_name(name: &str) -> bool {
    name.strip_prefix("poster-")
        .and_then(|n| n.strip_suffix(".rgba"))
        .is_some_and(|n| {
            n.len() == 16
                && n.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
}
pub struct RootGuard {
    _pins: Vec<File>,
    _profile: crate::candidate_instance::ProfileIdentity,
    _root: crate::candidate_instance::ProfileIdentity,
}
impl RootGuard {
    pub fn open(profile: &Path, root: &Path, create: bool) -> Result<Option<Self>, String> {
        Self::open_owner(profile, root, create, "posters")
    }
    pub fn open_colour(profile: &Path, root: &Path, create: bool) -> Result<Option<Self>, String> {
        Self::open_owner(profile, root, create, "colour-v1")
    }
    fn open_owner(
        profile: &Path,
        root: &Path,
        create: bool,
        owner: &str,
    ) -> Result<Option<Self>, String> {
        if !profile.is_absolute() || root != profile.join("cache").join(owner) {
            return Err("Cache is not the exact validated profile cache owner".into());
        }
        let mut chain: Vec<_> = profile
            .ancestors()
            .filter(|p| p.parent().is_some())
            .collect();
        chain.reverse();
        let mut pins = Vec::new();
        for path in chain {
            pins.push(pin_directory(path)?);
        }
        let profile_id = crate::candidate_instance::ProfileIdentity::open(profile)?;
        for path in [profile.join("cache"), root.to_path_buf()] {
            match std::fs::symlink_metadata(&path) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound && create => {
                    std::fs::create_dir(&path).map_err(|e| e.to_string())?;
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(e) => return Err(e.to_string()),
            }
            pins.push(pin_directory(&path)?);
        }
        let root_id = crate::candidate_instance::ProfileIdentity::open(root)?;
        Ok(Some(Self {
            _pins: pins,
            _profile: profile_id,
            _root: root_id,
        }))
    }
}
pub fn read_poster(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    let root = path.parent()?;
    let profile = root.parent()?.parent()?;
    let _guard = RootGuard::open(profile, root, false).ok()??;
    if !owned_name(path.file_name()?.to_str()?) {
        return None;
    }
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    let expected =
        (crate::gpu_gallery::POSTER_WIDTH * crate::gpu_gallery::POSTER_HEIGHT * 4) as usize;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if !meta.is_file()
        || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || meta.len() != expected as u64
        || unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut info) } == 0
        || info.nNumberOfLinks != 1
    {
        return None;
    }
    let mut bytes = Vec::with_capacity(expected);
    file.take(expected as u64 + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() == expected).then_some(bytes)
}

pub fn read_colour_file(path: &Path) -> Option<Vec<u8>> {
    use std::io::Read;
    if !super::colour::owned_name(path.file_name()?.to_str()?) {
        return None;
    }
    let file = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if !meta.is_file()
        || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
        || meta.len() > super::colour::CACHE_LIMIT
        || unsafe { GetFileInformationByHandle(file.as_raw_handle() as HANDLE, &mut info) } == 0
        || info.nNumberOfLinks != 1
    {
        return None;
    }
    let mut bytes = Vec::new();
    file.take(super::colour::CACHE_LIMIT + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    (bytes.len() as u64 <= super::colour::CACHE_LIMIT).then_some(bytes)
}
pub fn clear_colour(profile: &Path, epoch: u64) -> Report {
    clear_owner(profile, &profile.join("cache/colour-v1"), None, epoch, true)
}
pub fn clear(profile: &Path, root: &Path, names: Option<&[String]>, epoch: u64) -> Report {
    clear_owner(profile, root, names, epoch, false)
}
fn clear_owner(
    profile: &Path,
    root: &Path,
    names: Option<&[String]>,
    epoch: u64,
    colour: bool,
) -> Report {
    let mut report = Report {
        epoch,
        ..Report::default()
    };
    let run = || -> Result<(usize, usize, usize), String> {
        let Some(_guard) = (if colour {
            RootGuard::open_colour(profile, root, false)
        } else {
            RootGuard::open(profile, root, false)
        })?
        else {
            return Ok((0, 0, 0));
        };
        let mut deleted = 0;
        let mut retained = 0;
        let mut failed = 0;
        for entry in std::fs::read_dir(root).map_err(|e| e.to_string())? {
            let Ok(entry) = entry else {
                failed += 1;
                continue;
            };
            let name = entry.file_name().to_string_lossy().into_owned();
            if !(if colour {
                super::colour::owned_name(&name)
            } else {
                owned_name(&name)
            }) || names.is_some_and(|names| !names.contains(&name))
            {
                retained += 1;
                continue;
            }
            match std::fs::symlink_metadata(entry.path()) {
                Ok(meta)
                    if !meta.is_file()
                        || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 =>
                {
                    retained += 1;
                    continue;
                }
                Err(_) => {
                    failed += 1;
                    continue;
                }
                _ => {}
            }
            // OPEN_REPARSE_POINT plus handle-based disposition closes the lstat/delete race.
            let file = OpenOptions::new()
                .access_mode(
                    FILE_READ_ATTRIBUTES | 0x0001_0000 | if colour { FILE_READ_DATA } else { 0 },
                )
                .share_mode(FILE_SHARE_READ)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(entry.path());
            let Ok(file) = file else {
                failed += 1;
                continue;
            };
            let Ok(meta) = file.metadata() else {
                failed += 1;
                continue;
            };
            let expected = (crate::gpu_gallery::POSTER_WIDTH as u64)
                * (crate::gpu_gallery::POSTER_HEIGHT as u64)
                * 4;
            let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
            let handle = file.as_raw_handle() as HANDLE;
            let identified = unsafe { GetFileInformationByHandle(handle, &mut info) } != 0;
            if !identified {
                failed += 1;
                continue;
            }
            if !meta.is_file()
                || meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                || (!colour && meta.len() != expected)
                || (colour && meta.len() > super::colour::CACHE_LIMIT)
                || info.nNumberOfLinks != 1
            {
                retained += 1;
                continue;
            }
            if colour {
                use std::io::Read;
                let mut bytes = Vec::new();
                if (&file)
                    .take(super::colour::CACHE_LIMIT + 1)
                    .read_to_end(&mut bytes)
                    .is_err()
                {
                    failed += 1;
                    continue;
                }
                if super::colour::valid_bytes(&name, &bytes).is_none() {
                    retained += 1;
                    continue;
                }
            }
            let disposition = FILE_DISPOSITION_INFO { DeleteFile: 1 };
            if unsafe {
                SetFileInformationByHandle(
                    handle,
                    FileDispositionInfo,
                    (&disposition as *const FILE_DISPOSITION_INFO).cast(),
                    std::mem::size_of::<FILE_DISPOSITION_INFO>() as u32,
                )
            } == 0
            {
                failed += 1;
            } else {
                deleted += 1;
            }
        }
        Ok((deleted, retained, failed))
    };
    match run() {
        Ok((d, r, f)) => {
            report.deleted = d;
            report.retained = r;
            report.failed = f;
        }
        Err(e) => report.error = Some(e),
    }
    report
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    pub fn fixture() -> (PathBuf, PathBuf) {
        static ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let profile = std::env::temp_dir().join(format!(
            "w4-cache-{}-{}",
            std::process::id(),
            ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let root = profile.join("cache").join("posters");
        std::fs::create_dir_all(&root).unwrap();
        (profile, root)
    }
    pub fn poster(root: &Path, name: &str) -> PathBuf {
        let path = root.join(name);
        std::fs::write(
            &path,
            vec![
                91;
                (crate::gpu_gallery::POSTER_WIDTH * crate::gpu_gallery::POSTER_HEIGHT * 4) as usize
            ],
        )
        .unwrap();
        path
    }
    #[test]
    fn w4_precise_cache_counts_partial_failure_and_source_recovery_export_survival() {
        let (profile, root) = fixture();
        let owned = poster(&root, "poster-0123456789abcdef.rgba");
        let held = poster(&root, "poster-1111111111111111.rgba");
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&held)
            .unwrap();
        for name in [
            "poster-old.rgba",
            "poster-2222222222222222.rgba.bak",
            "poster-3333333333333333.rgba.1.tmp",
            "native-image.png",
            "recovery.sentinel",
            "export.png",
            "export.receipt.json",
        ] {
            std::fs::write(root.join(name), b"must survive").unwrap();
        }
        std::fs::write(profile.join("source.png"), b"original").unwrap();
        std::fs::write(profile.join("candidate-state.toml.bak"), b"recovery").unwrap();
        let unknown = root.join("poster-4444444444444444.rgba");
        std::fs::write(&unknown, b"unknown legacy format").unwrap();
        let link = poster(&root, "poster-5555555555555555.rgba");
        std::fs::hard_link(&link, profile.join("source-hardlink.rgba")).unwrap();
        std::fs::create_dir(root.join("poster-6666666666666666.rgba")).unwrap();
        let report = clear(&profile, &root, None, 9);
        eprintln!("W4 acknowledged cache receipt: {report:?}");
        assert_eq!(report.deleted, 1);
        assert_eq!(report.failed, 1);
        assert_eq!(report.retained, 10);
        assert!(report.error.is_none());
        assert!(!owned.exists());
        assert!(held.exists());
        drop(file);
        assert_eq!(
            std::fs::read(profile.join("source.png")).unwrap(),
            b"original"
        );
        assert_eq!(
            std::fs::read(profile.join("candidate-state.toml.bak")).unwrap(),
            b"recovery"
        );
        assert_eq!(clear(&profile, &root, None, 10).deleted, 1);
        assert!(unknown.exists());
        assert!(link.exists());
    }
    #[test]
    fn w4_cache_refuses_wrong_root_and_junction_escape() {
        let (profile, root) = fixture();
        let target = profile.join("outside");
        std::fs::create_dir(&target).unwrap();
        let sentinel = poster(&target, "poster-0123456789abcdef.rgba");
        assert!(clear(&profile, &target, None, 1).error.is_some());
        assert!(sentinel.exists());
        // A distinct owned profile with a junction in place of its posters directory.
        let other = profile.join("junction-profile");
        std::fs::create_dir_all(other.join("cache")).unwrap();
        let junction = other.join("cache").join("posters");
        let output = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(&target)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "owned junction fixture creation failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(clear(&other, &junction, None, 2).error.is_some());
        assert!(sentinel.exists());
        assert!(root.exists());
    }
}

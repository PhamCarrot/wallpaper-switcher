use super::{delete_file, owned_key, owned_name, poster_entry_size, touch, PosterCache, RootGuard};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::os::windows::{ffi::OsStrExt, fs::OpenOptionsExt, io::AsRawHandle};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use windows_sys::Win32::{Foundation::*, Storage::FileSystem::*};

const MIB: u64 = 1024 * 1024;
const LOW_DISK_BYTES: u64 = 10_000_000_000;
const TEMP_MAX_AGE: Duration = Duration::from_secs(60 * 60);

pub(in crate::poster_worker) struct Maintenance {
    profile: PathBuf,
    override_mib: Option<u32>,
    preview: bool,
}

impl Maintenance {
    pub(in crate::poster_worker) fn new(
        profile: PathBuf,
        override_mib: Option<u32>,
        preview: bool,
    ) -> Self {
        Self {
            profile,
            override_mib,
            preview,
        }
    }

    pub(in crate::poster_worker) fn budget(&self) -> u64 {
        let free = free_bytes(&self.profile);
        let budget = budget_bytes(free, self.override_mib);
        crate::config::log_debug(&format!(
            "Disk cache cap: {} MiB (override={:?}, available_bytes={free:?})",
            budget / MIB,
            self.override_mib
        ));
        budget
    }

    fn scan(
        &self,
        retain_entries: bool,
        idle: &impl Fn() -> bool,
    ) -> Option<(Pass, Vec<RootGuard>)> {
        let mut pass = Pass::default();
        let mut guards = Vec::new();
        for owner in [Owner::Posters, Owner::Native] {
            if !idle() {
                return None;
            }
            let root = self.profile.join("cache").join(owner.dir());
            let guard = match owner {
                Owner::Posters => RootGuard::open(&self.profile, &root, false),
                Owner::Native => RootGuard::open_native(&self.profile, &root),
            };
            match guard {
                Ok(Some(guard)) => {
                    guards.push(guard);
                    if !pass.scan(owner, &root, retain_entries, idle) {
                        return None;
                    }
                }
                Ok(None) => (),
                Err(error) => {
                    pass.skipped += 1;
                    crate::apply_trace::event_lazy("disk-cache-refused", || {
                        format!("owner={}; error={error}", owner.dir())
                    });
                }
            }
        }
        Some((pass, guards))
    }

    /// Optional background writes only use existing headroom.
    pub(in crate::poster_worker) fn headroom(&self, budget: u64, idle: &impl Fn() -> bool) -> u64 {
        let Some((pass, _guards)) = self.scan(false, idle) else {
            return 0;
        };
        if pass.skipped > 0 || !idle() {
            return 0;
        }
        budget
            .saturating_sub(pass.bytes)
            .min(free_bytes(&self.profile).unwrap_or(0))
    }

    pub(in crate::poster_worker) fn open_posters(&self) -> Option<PosterCache> {
        PosterCache::open(&self.profile, &self.profile.join("cache/posters"), true)
    }

    // Run entirely on the poster worker. Keep both root guards until the last
    // disposition and yield to pending foreground work between filesystem calls.
    pub(in crate::poster_worker) fn run(&self, budget: u64, idle: impl Fn() -> bool) -> bool {
        let Some((mut pass, _guards)) = self.scan(true, &idle) else {
            return false;
        };
        pass.entries.sort_unstable_by(|a, b| {
            (a.stamp.last_used(), &a.path).cmp(&(b.stamp.last_used(), &b.path))
        });
        let before = pass.bytes;
        let needs_native = pass.temps.iter().any(Entry::is_native)
            || (before > budget && pass.entries.iter().any(Entry::is_native));
        let native = if self.preview {
            pass.remove(budget, Some(&HashSet::new()), &idle);
            "preview"
        } else if needs_native && idle() {
            match crate::coordinator::with_native_cache_eviction(|protected| {
                pass.remove(budget, Some(protected), &idle);
            }) {
                Ok(()) => "checked",
                Err(error) => {
                    crate::apply_trace::event("disk-cache-native-deferred", error);
                    pass.remove(budget, None, &idle);
                    "deferred"
                }
            }
        } else {
            pass.remove(budget, None, &idle);
            "unneeded"
        };
        let finished = idle();
        crate::apply_trace::event_lazy("disk-cache-maintenance", || {
            format!(
                "cap_bytes={budget}; known_before_bytes={before}; known_remaining_bytes={}; posters_evicted={}; native_evicted={}; temps_swept={}; protected={}; skipped={}; native={native}; idle={finished}",
                pass.bytes, pass.posters, pass.native, pass.swept, pass.protected,
                pass.skipped,
            )
        });
        finished
    }
}

fn free_bytes(profile: &Path) -> Option<u64> {
    let path: Vec<u16> = profile.as_os_str().encode_wide().chain([0]).collect();
    let mut available = 0;
    (unsafe {
        GetDiskFreeSpaceExW(
            path.as_ptr(),
            &mut available,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    } != 0)
        .then_some(available)
}

fn budget_bytes(free: Option<u64>, override_mib: Option<u32>) -> u64 {
    let cap_mib = override_mib
        .filter(|&mib| mib > 0)
        .map(u64::from)
        .unwrap_or_else(|| {
            if free.is_some_and(|bytes| bytes >= LOW_DISK_BYTES) {
                512
            } else {
                128
            }
        });
    cap_mib * MIB
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Owner {
    Posters,
    Native,
}

impl Owner {
    fn dir(self) -> &'static str {
        match self {
            Self::Posters => "posters",
            Self::Native => "native-images",
        }
    }

    fn entry(self, name: &str) -> bool {
        match self {
            Self::Posters => owned_name(name),
            Self::Native => owned_key(name, "native-", ".bmp"),
        }
    }

    fn temp(self, name: &str) -> bool {
        let Some(stem) = name.strip_suffix(".tmp") else {
            return false;
        };
        match self {
            Self::Posters => stem.rsplit_once('.').is_some_and(|(entry, nonce)| {
                self.entry(entry) && decimal(nonce) && nonce.parse::<u64>().is_ok()
            }),
            Self::Native => stem
                .strip_prefix('.')
                .and_then(|stage| stage.split_once(".native-cache-tmp-"))
                .is_some_and(|(entry, suffix)| {
                    let mut parts = suffix.split('-');
                    self.entry(entry)
                        && parts
                            .next()
                            .is_some_and(|p| decimal(p) && p.parse::<u32>().is_ok())
                        && parts
                            .next()
                            .is_some_and(|n| decimal(n) && n.parse::<u128>().is_ok())
                        && parts
                            .next()
                            .is_some_and(|a| decimal(a) && a.parse::<u32>().is_ok_and(|a| a < 32))
                        && parts.next().is_none()
                }),
        }
    }
}

fn decimal(value: &str) -> bool {
    !value.is_empty()
        && (value == "0" || !value.starts_with('0'))
        && value.bytes().all(|c| c.is_ascii_digit())
}

#[derive(PartialEq, Eq)]
struct Stamp {
    volume: u64,
    id: [u8; 16],
    bytes: u64,
    accessed: u64,
    written: u64,
}

impl Stamp {
    fn last_used(&self) -> u64 {
        self.accessed.max(self.written)
    }
}

fn join_words(high: u32, low: u32) -> u64 {
    (u64::from(high) << 32) | u64::from(low)
}

fn checked_file(path: &Path, access: u32, share: u32) -> Option<(File, Stamp)> {
    let file = OpenOptions::new()
        .access_mode(access | FILE_READ_ATTRIBUTES)
        .share_mode(share)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
        .ok()?;
    let handle = file.as_raw_handle() as HANDLE;
    let mut info: BY_HANDLE_FILE_INFORMATION = unsafe { std::mem::zeroed() };
    if unsafe { GetFileInformationByHandle(handle, &mut info) } == 0
        || info.dwFileAttributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT) != 0
        || info.nNumberOfLinks != 1
    {
        return None;
    }
    let mut id: FILE_ID_INFO = unsafe { std::mem::zeroed() };
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileIdInfo,
            (&mut id as *mut FILE_ID_INFO).cast(),
            std::mem::size_of::<FILE_ID_INFO>() as u32,
        )
    } == 0
        || id.FileId.Identifier == [0; 16]
    {
        return None;
    }
    let stamp = Stamp {
        volume: id.VolumeSerialNumber,
        id: id.FileId.Identifier,
        bytes: join_words(info.nFileSizeHigh, info.nFileSizeLow),
        accessed: join_words(
            info.ftLastAccessTime.dwHighDateTime,
            info.ftLastAccessTime.dwLowDateTime,
        ),
        written: join_words(
            info.ftLastWriteTime.dwHighDateTime,
            info.ftLastWriteTime.dwLowDateTime,
        ),
    };
    Some((file, stamp))
}

pub(crate) fn touch_native(path: &Path) {
    let Some(root) = path.parent() else { return };
    let Some(profile) = root.parent().and_then(Path::parent) else {
        return;
    };
    let Ok(Some(_guard)) = RootGuard::open_native(profile, root) else {
        return;
    };
    if let Some((file, _stamp)) = checked_file(
        path,
        FILE_WRITE_ATTRIBUTES,
        FILE_SHARE_READ | FILE_SHARE_WRITE,
    ) {
        touch(&file);
    }
}

struct Entry {
    path: PathBuf,
    stamp: Stamp,
    owner: Owner,
}

impl Entry {
    fn is_native(&self) -> bool {
        self.owner == Owner::Native
    }

    fn remove(&self) -> bool {
        // Reopen by handle, excluding readers, writers, and replacement during
        // disposition. A replacement, cache hit, or link added since the scan wins.
        checked_file(&self.path, DELETE, 0).is_some_and(|(file, stamp)| {
            stamp == self.stamp && delete_file(file.as_raw_handle() as HANDLE)
        })
    }
}

#[derive(Default)]
struct Pass {
    entries: Vec<Entry>,
    temps: Vec<Entry>,
    bytes: u64,
    posters: usize,
    native: usize,
    swept: usize,
    protected: usize,
    skipped: usize,
}

impl Pass {
    fn scan(
        &mut self,
        owner: Owner,
        root: &Path,
        retain_entries: bool,
        idle: &impl Fn() -> bool,
    ) -> bool {
        let Ok(entries) = fs::read_dir(root) else {
            self.skipped += 1;
            return true;
        };
        let now = SystemTime::now();
        for entry in entries {
            if !idle() {
                return false;
            }
            let Ok(entry) = entry else {
                self.skipped += 1;
                continue;
            };
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let temporary = owner.temp(name);
            if temporary && !retain_entries {
                continue;
            }
            if !temporary && !owner.entry(name) {
                continue;
            }
            let path = entry.path();
            let Some((file, stamp)) = checked_file(
                &path,
                0,
                FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ) else {
                self.skipped += 1;
                continue;
            };
            if temporary {
                let old = file
                    .metadata()
                    .and_then(|m| m.modified())
                    .ok()
                    .and_then(|written| now.duration_since(written).ok())
                    .is_some_and(|age| age >= TEMP_MAX_AGE);
                if old {
                    self.temps.push(Entry { path, stamp, owner });
                }
            } else if !retain_entries
                || owner == Owner::Native
                || Some(stamp.bytes) == poster_entry_size(name)
            {
                self.bytes = self.bytes.saturating_add(stamp.bytes);
                if retain_entries {
                    self.entries.push(Entry { path, stamp, owner });
                }
            }
        }
        true
    }

    fn remove(
        &mut self,
        budget: u64,
        protected: Option<&HashSet<String>>,
        idle: &impl Fn() -> bool,
    ) {
        for entry in &self.temps {
            if !idle() {
                return;
            }
            if entry.is_native() && protected.is_none() {
                continue;
            }
            if entry.remove() {
                self.swept += 1;
            } else {
                self.skipped += 1;
            }
        }
        for entry in &self.entries {
            if self.bytes <= budget || !idle() {
                return;
            }
            if entry.is_native()
                && protected.is_none_or(|paths| {
                    paths.contains(&crate::native_wallpaper::normalized_wallpaper_path(
                        &entry.path,
                    ))
                })
            {
                self.protected += 1;
                continue;
            }
            if entry.remove() {
                self.bytes -= entry.stamp.bytes;
                if entry.is_native() {
                    self.native += 1;
                } else {
                    self.posters += 1;
                }
            } else {
                self.skipped += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poster_worker::cache::{tests as fixtures, PosterCache};

    fn age(path: &Path, hours: u64) -> SystemTime {
        let time = SystemTime::now() - Duration::from_secs(hours * 3600);
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(fs::FileTimes::new().set_accessed(time).set_modified(time))
            .unwrap();
        fs::metadata(path).unwrap().modified().unwrap()
    }

    #[test]
    fn budget_uses_section_zero_threshold_and_positive_config_override() {
        assert_eq!(budget_bytes(Some(LOW_DISK_BYTES), None), 512 * MIB);
        assert_eq!(budget_bytes(Some(LOW_DISK_BYTES - 1), None), 128 * MIB);
        assert_eq!(budget_bytes(None, None), 128 * MIB);
        assert_eq!(budget_bytes(None, Some(0)), 128 * MIB);
        assert_eq!(budget_bytes(Some(0), Some(3)), 3 * MIB);
    }

    #[test]
    fn idle_cap_is_shared_and_only_disposable_temps_are_swept() {
        let (profile, posters) = fixtures::fixture();
        let native = profile.join("cache/native-images");
        fs::create_dir(&native).unwrap();
        let kept = fixtures::poster(&posters, "poster-1111111111111111.rgba");
        let older = fixtures::poster(&posters, "poster-2222222222222222.rgba");
        let old_native = native.join("native-aaaaaaaaaaaaaaaa.bmp");
        fs::write(&old_native, b"native derivative").unwrap();
        let original_modified = age(&kept, 5);
        age(&old_native, 4);
        age(&older, 3);
        let cache = PosterCache::open(&profile, &posters, false).unwrap();
        let mut pixels = vec![0; MIB as usize];
        cache.read_into(&kept, &mut pixels).unwrap();
        let after_hit = fs::metadata(&kept).unwrap();
        assert_eq!(after_hit.modified().unwrap(), original_modified);
        assert!(after_hit.accessed().unwrap() > original_modified);
        assert!(pixels.iter().all(|&byte| byte == 91));
        drop(cache);

        let stale_poster = posters.join("poster-1111111111111111.rgba.1.tmp");
        let stale_native =
            native.join(".native-aaaaaaaaaaaaaaaa.bmp.native-cache-tmp-123-1234-0.tmp");
        let recent = posters.join("poster-1111111111111111.rgba.2.tmp");
        let foreign = posters.join("foreign.tmp");
        let lookalike = posters.join("poster-1111111111111111.rgba.not-owned.tmp");
        let recovery = native.join(".native-aaaaaaaaaaaaaaaa.bmp.native-cache-tmp-123-1234-0");
        let backup =
            native.join(".native-aaaaaaaaaaaaaaaa.bmp.native-cache-replace-recovery-123-1234-0");
        let malformed = posters.join("poster-3333333333333333.rgba");
        for path in [
            &stale_poster,
            &stale_native,
            &foreign,
            &lookalike,
            &recovery,
            &backup,
            &malformed,
        ] {
            fs::write(path, b"fixture").unwrap();
            age(path, 2);
        }
        fs::write(&recent, b"recent").unwrap();
        let hardlink = native.join("native-eeeeeeeeeeeeeeee.bmp");
        fs::hard_link(&foreign, &hardlink).unwrap();

        assert!(Maintenance::new(profile, Some(1), true).run(MIB, || true));
        for evicted in [older, old_native, stale_poster, stale_native] {
            assert!(!evicted.exists(), "retained {}", evicted.display());
        }
        for retained in [
            kept, foreign, lookalike, recent, recovery, backup, malformed, hardlink,
        ] {
            assert!(retained.is_file(), "lost {}", retained.display());
        }
    }

    #[test]
    fn eviction_leaves_active_protected_and_replaced_entries_intact() {
        let (profile, posters) = fixtures::fixture();
        let native = profile.join("cache/native-images");
        fs::create_dir(&native).unwrap();
        let protected = native.join("native-aaaaaaaaaaaaaaaa.bmp");
        fs::write(&protected, b"active native path").unwrap();
        let busy = fixtures::poster(&posters, "poster-1111111111111111.rgba");
        let replaced = fixtures::poster(&posters, "poster-2222222222222222.rgba");
        let removable = fixtures::poster(&posters, "poster-3333333333333333.rgba");
        let busy_read = File::options()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(&busy)
            .unwrap();
        let mut pass = Pass::default();
        assert!(pass.scan(Owner::Posters, &posters, true, &|| true));
        assert!(pass.scan(Owner::Native, &native, true, &|| true));
        // Keep the old file alive so the replacement cannot reuse its ID.
        fs::rename(&replaced, posters.join("old-file")).unwrap();
        fs::write(&replaced, vec![42; MIB as usize]).unwrap();
        let paths = HashSet::from([crate::native_wallpaper::normalized_wallpaper_path(
            &protected,
        )]);
        pass.remove(0, Some(&paths), &|| false);
        assert!(removable.exists());
        pass.remove(0, Some(&paths), &|| true);
        assert_eq!(
            (pass.posters, pass.native, pass.skipped, pass.protected),
            (1, 0, 2, 1)
        );
        assert!(busy.exists() && protected.exists() && !removable.exists());
        assert_eq!(fs::read(&replaced).unwrap(), vec![42; MIB as usize]);
        drop(busy_read);
    }
}

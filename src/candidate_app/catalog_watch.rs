//! One lifetime owner, one scan and one dirty/request bit. No visibility dependency.
use super::Wake;
use crate::{candidate_state::preferences::LibraryPreferences, library, scanner};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex,
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[derive(Clone, PartialEq)]
struct Configuration {
    roots: Vec<PathBuf>,
    defaults: bool,
    prefs: LibraryPreferences,
}
struct State {
    config: Configuration,
    generation: u64,
    requested: bool,
    publication: bool,
    dirty_wake: bool,
    stop: bool,
    status: String,
    fallback: bool,
}
impl State {
    fn observe_notification(&mut self, result: std::io::Result<bool>) -> bool {
        match result {
            Ok(changed) => changed,
            Err(_) => {
                self.fallback = true;
                self.status = "Polling fallback active: notification error or overflow".into();
                true
            }
        }
    }
}
struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}
pub(super) struct CatalogWatch {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}
impl std::fmt::Debug for CatalogWatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CatalogWatch").finish_non_exhaustive()
    }
}
impl CatalogWatch {
    pub(super) fn spawn(
        roots: Vec<PathBuf>,
        defaults: bool,
        prefs: LibraryPreferences,
        clock: Arc<AtomicU64>,
        publish: impl Fn(Wake) -> bool + Send + 'static,
    ) -> std::io::Result<Self> {
        Self::spawn_scanning(
            roots,
            defaults,
            prefs,
            clock,
            publish,
            scanner::scan_catalog_until,
        )
    }
    fn spawn_scanning(
        roots: Vec<PathBuf>,
        defaults: bool,
        prefs: LibraryPreferences,
        clock: Arc<AtomicU64>,
        publish: impl Fn(Wake) -> bool + Send + 'static,
        scan: impl Fn(&[PathBuf], bool, &dyn Fn() -> bool) -> Vec<scanner::WallpaperItem>
            + Send
            + 'static,
    ) -> std::io::Result<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                config: Configuration {
                    roots,
                    defaults,
                    prefs,
                },
                generation: clock.load(Ordering::Acquire),
                requested: true,
                publication: false,
                dirty_wake: false,
                stop: false,
                status: "Starting library watch…".into(),
                fallback: false,
            }),
            wake: Condvar::new(),
        });
        let worker = shared.clone();
        let thread = std::thread::Builder::new()
            .name("catalog-watch".into())
            .spawn(move || run(worker, clock, publish, scan))?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }
    pub(super) fn request(
        &self,
        roots: Vec<PathBuf>,
        defaults: bool,
        prefs: LibraryPreferences,
        generation: u64,
    ) {
        let mut s = self.shared.state.lock().unwrap();
        s.config = Configuration {
            roots,
            defaults,
            prefs,
        };
        s.generation = generation;
        s.requested = true;
        s.dirty_wake = false;
        self.shared.wake.notify_one();
    }
    pub(super) fn acknowledge(&self) {
        self.shared.state.lock().unwrap().publication = false;
        self.shared.wake.notify_one();
    }
    pub(super) fn status(&self) -> (String, bool) {
        let s = self.shared.state.lock().unwrap();
        (s.status.clone(), s.fallback)
    }
}
impl Drop for CatalogWatch {
    fn drop(&mut self) {
        self.shared.state.lock().unwrap().stop = true;
        self.shared.wake.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn run(
    shared: Arc<Shared>,
    clock: Arc<AtomicU64>,
    publish: impl Fn(Wake) -> bool,
    scan: impl Fn(&[PathBuf], bool, &dyn Fn() -> bool) -> Vec<scanner::WallpaperItem>,
) {
    let mut installed = None;
    let mut watches = Vec::new();
    let mut dirty_at = None;
    let mut last_poll = Instant::now();
    loop {
        let mut s = shared.state.lock().unwrap();
        if s.stop {
            return;
        }
        if installed.as_ref() != Some(&s.config) {
            watches.clear(); // Drop cancels and settles every overlapped request.
            s.fallback = false;
            if s.config.prefs.watching() {
                for root in &s.config.roots.clone() {
                    match Notification::open(root) {
                        Ok(w) => watches.push(w),
                        Err(_) => s.fallback = true,
                    }
                }
            }
            s.status = if !s.config.prefs.watching() {
                "Automatic watching off".into()
            } else if s.config.roots.is_empty() {
                "No configured local roots".into()
            } else if s.fallback {
                "Polling fallback active: a root cannot provide notifications".into()
            } else {
                format!(
                    "Watching {} local root(s), including while hidden",
                    watches.len()
                )
            };
            installed = Some(s.config.clone());
            dirty_at = None;
            last_poll = Instant::now();
        }
        if s.config.prefs.watching() {
            for watch in &mut watches {
                if s.observe_notification(watch.changed()) && dirty_at.is_none() {
                    dirty_at = Some(Instant::now());
                }
            }

            let poll = s.fallback
                && last_poll.elapsed() >= Duration::from_secs(s.config.prefs.interval() as u64);
            if poll {
                last_poll = Instant::now();
                dirty_at = Some(Instant::now() - Duration::from_secs(1));
            }
            if dirty_at.is_some_and(|t| t.elapsed() >= Duration::from_millis(180)) && !s.dirty_wake
            {
                s.dirty_wake = true;
                dirty_at = None;
                let generation = s.generation;
                drop(s);
                if !publish(Wake::CatalogDirty { generation }) {
                    return;
                }
                s = shared.state.lock().unwrap();
            }
        }
        if s.requested && !s.publication {
            s.requested = false;
            let config = s.config.clone();
            let generation = s.generation;
            drop(s);
            let cancelled =
                || shared.state.lock().unwrap().stop || generation != clock.load(Ordering::Acquire);
            let wallpapers = scan(&config.roots, config.defaults, &cancelled);
            let entries = wallpapers
                .iter()
                .take_while(|_| !cancelled())
                .map(library::CatalogEntry::from_wallpaper)
                .collect();
            s = shared.state.lock().unwrap();
            if s.stop {
                return;
            }
            if generation == s.generation && generation == clock.load(Ordering::Acquire) {
                s.publication = true;
                drop(s);
                if !publish(Wake::CatalogLoaded {
                    generation,
                    wallpapers,
                    entries,
                }) {
                    return;
                }
                continue;
            }
        }
        let _ = shared
            .wake
            .wait_timeout(s, Duration::from_millis(50))
            .unwrap();
    }
}

// Boxes keep the kernel's buffer and OVERLAPPED addresses stable until settlement.
#[repr(C)]
struct Overlapped {
    internal: usize,
    high: usize,
    offset: u64,
    event: windows_sys::Win32::Foundation::HANDLE,
}
struct Notification {
    file: std::fs::File,
    operation: Box<Overlapped>,
    buffer: Box<[u32; 4096]>,
    pending: bool,
    failed: bool,
}
#[link(name = "kernel32")]
extern "system" {
    fn CreateEventW(
        a: *const std::ffi::c_void,
        manual: i32,
        initial: i32,
        name: *const u16,
    ) -> windows_sys::Win32::Foundation::HANDLE;
    fn ResetEvent(event: windows_sys::Win32::Foundation::HANDLE) -> i32;
    fn ReadDirectoryChangesW(
        file: windows_sys::Win32::Foundation::HANDLE,
        buffer: *mut std::ffi::c_void,
        size: u32,
        subtree: i32,
        filter: u32,
        bytes: *mut u32,
        op: *mut Overlapped,
        completion: *const std::ffi::c_void,
    ) -> i32;
    fn GetOverlappedResult(
        file: windows_sys::Win32::Foundation::HANDLE,
        op: *mut Overlapped,
        bytes: *mut u32,
        wait: i32,
    ) -> i32;
    fn CancelIoEx(file: windows_sys::Win32::Foundation::HANDLE, op: *const Overlapped) -> i32;
}
impl Notification {
    fn open(path: &std::path::Path) -> std::io::Result<Self> {
        use std::os::windows::fs::OpenOptionsExt;
        use windows_sys::Win32::Storage::FileSystem::*;
        let file = std::fs::OpenOptions::new()
            .access_mode(FILE_LIST_DIRECTORY)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OVERLAPPED)
            .open(path)?;
        let event = unsafe { CreateEventW(std::ptr::null(), 1, 0, std::ptr::null()) };
        if event.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        let mut n = Self {
            file,
            operation: Box::new(Overlapped {
                internal: 0,
                high: 0,
                offset: 0,
                event,
            }),
            buffer: Box::new([0; 4096]),
            pending: false,
            failed: false,
        };
        n.arm()?;
        Ok(n)
    }
    fn handle(&self) -> windows_sys::Win32::Foundation::HANDLE {
        use std::os::windows::io::AsRawHandle;
        self.file.as_raw_handle() as _
    }
    fn arm(&mut self) -> std::io::Result<()> {
        use windows_sys::Win32::Storage::FileSystem::*;
        // SAFETY: owned handles, aligned boxed buffer and operation outlive all pending I/O.
        unsafe {
            ResetEvent(self.operation.event);
            if ReadDirectoryChangesW(
                self.handle(),
                self.buffer.as_mut_ptr().cast(),
                16384,
                1,
                FILE_NOTIFY_CHANGE_FILE_NAME
                    | FILE_NOTIFY_CHANGE_DIR_NAME
                    | FILE_NOTIFY_CHANGE_SIZE
                    | FILE_NOTIFY_CHANGE_LAST_WRITE,
                std::ptr::null_mut(),
                &mut *self.operation,
                std::ptr::null(),
            ) == 0
            {
                self.failed = true;
                return Err(std::io::Error::last_os_error());
            }
        }
        self.pending = true;
        Ok(())
    }
    fn changed(&mut self) -> std::io::Result<bool> {
        if self.failed {
            return Ok(false);
        }
        let mut bytes = 0;
        let ok = unsafe { GetOverlappedResult(self.handle(), &mut *self.operation, &mut bytes, 0) };
        if ok == 0 {
            let e = std::io::Error::last_os_error();
            if e.raw_os_error() == Some(996) {
                return Ok(false);
            } // ERROR_IO_INCOMPLETE
            self.pending = false;
            self.failed = true;
            return Err(e);
        }
        self.pending = false;
        self.arm()?;
        if bytes == 0 {
            return Err(std::io::Error::other("notification overflow"));
        }
        Ok(true)
    }
}
impl Drop for Notification {
    fn drop(&mut self) {
        unsafe {
            if self.pending {
                CancelIoEx(self.handle(), &*self.operation);
                let mut bytes = 0;
                GetOverlappedResult(self.handle(), &mut *self.operation, &mut bytes, 1);
            }
            windows_sys::Win32::Foundation::CloseHandle(self.operation.event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::{self, Receiver};
    fn root(label: &str) -> PathBuf {
        static ID: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "w4-watch-{label}-{}-{}",
            std::process::id(),
            ID.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }
    fn next(
        owner: &CatalogWatch,
        clock: &AtomicU64,
        rx: &Receiver<Wake>,
        roots: &[PathBuf],
        prefs: &LibraryPreferences,
    ) -> Vec<scanner::WallpaperItem> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            assert!(Instant::now() < deadline, "catalog did not converge");
            match rx.recv_timeout(Duration::from_secs(20)).unwrap() {
                Wake::CatalogDirty { generation }
                    if generation == clock.load(Ordering::Acquire) =>
                {
                    let generation = clock.fetch_add(1, Ordering::AcqRel) + 1;
                    owner.request(roots.to_vec(), false, prefs.clone(), generation);
                }
                Wake::CatalogLoaded {
                    generation,
                    wallpapers,
                    ..
                } => {
                    owner.acknowledge();
                    if generation == clock.load(Ordering::Acquire) {
                        return wallpapers;
                    }
                }
                _ => {}
            }
        }
    }
    fn has(items: &[scanner::WallpaperItem], path: &std::path::Path) -> bool {
        items
            .iter()
            .any(|i| i.file_target == path.canonicalize().unwrap_or_else(|_| path.into()))
    }
    #[test]
    fn w4_real_notifications_add_rename_remove_coalesce_and_release_on_drop() {
        let root = root("notifications");
        let roots = vec![root.clone()];
        let clock = Arc::new(AtomicU64::new(1));
        let prefs = LibraryPreferences::default();
        let (tx, rx) = mpsc::channel();
        let owner = CatalogWatch::spawn(
            roots.clone(),
            false,
            prefs.clone(),
            clock.clone(),
            move |w| tx.send(w).is_ok(),
        )
        .unwrap();
        next(&owner, &clock, &rx, &roots, &prefs);
        assert!(
            !owner.status().1,
            "local NTFS notifications should be active"
        );
        let a = root.join("owned-added.png");
        let b = root.join("owned-renamed.png");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([19, 88, 201, 255]))
            .save(&a)
            .unwrap();
        assert!(has(&next(&owner, &clock, &rx, &roots, &prefs), &a));
        std::fs::rename(&a, &b).unwrap();
        let items = next(&owner, &clock, &rx, &roots, &prefs);
        assert!(has(&items, &b));
        assert!(!has(&items, &a));
        std::fs::remove_file(&b).unwrap();
        assert!(!has(&next(&owner, &clock, &rx, &roots, &prefs), &b));
        // Hold publication: repeated requests occupy exactly one dirty slot and cannot publish a backlog.
        owner.shared.state.lock().unwrap().publication = true;
        for _ in 0..50 {
            let g = clock.fetch_add(1, Ordering::AcqRel) + 1;
            owner.request(roots.clone(), false, prefs.clone(), g);
        }
        assert!(owner.shared.state.lock().unwrap().requested);
        assert!(rx.recv_timeout(Duration::from_millis(120)).is_err());
        owner.acknowledge();
        next(&owner, &clock, &rx, &roots, &prefs);
        let start = Instant::now();
        drop(owner);
        assert!(start.elapsed() < Duration::from_secs(3));
        // A fresh notification handle can open the same root after cancellation/join.
        drop(Notification::open(&root).unwrap());
    }
    #[test]
    fn w4_unavailable_notifications_mandatory_real_poll_convergence_and_stale_roots() {
        let base = root("poll");
        let missing = base.join("initially-unavailable");
        let roots = vec![missing.clone()];
        let prefs = LibraryPreferences {
            polling_seconds: Some(1),
            ..Default::default()
        };
        let clock = Arc::new(AtomicU64::new(1));
        let (tx, rx) = mpsc::channel();
        let owner = CatalogWatch::spawn(
            roots.clone(),
            false,
            prefs.clone(),
            clock.clone(),
            move |w| tx.send(w).is_ok(),
        )
        .unwrap();
        next(&owner, &clock, &rx, &roots, &prefs);
        assert!(owner.status().1);
        std::fs::create_dir(&missing).unwrap();
        let image = missing.join("owned-poll.png");
        image::RgbaImage::new(2, 2).save(&image).unwrap();
        assert!(has(&next(&owner, &clock, &rx, &roots, &prefs), &image));
        let current = root("current");
        let current_roots = vec![current.clone()];
        let g = clock.fetch_add(1, Ordering::AcqRel) + 1;
        owner.request(current_roots.clone(), false, prefs.clone(), g);
        let items = next(&owner, &clock, &rx, &current_roots, &prefs);
        assert!(!has(&items, &image));
        drop(owner);
    }
    #[test]
    fn w4_overflow_error_force_fallback_and_held_scan_is_latest_only_and_cancellable() {
        let old = root("held-old");
        let current = root("held-current");
        let image = current.join("owned-current.png");
        image::RgbaImage::new(2, 2).save(&image).unwrap();
        let clock = Arc::new(AtomicU64::new(1));
        let (tx, rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let release = Arc::new(std::sync::Barrier::new(2));
        let held = release.clone();
        let scans = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counted = scans.clone();
        let owner = CatalogWatch::spawn_scanning(
            vec![old],
            false,
            Default::default(),
            clock.clone(),
            move |wake| tx.send(wake).is_ok(),
            move |roots, defaults, cancelled| {
                if counted.fetch_add(1, Ordering::AcqRel) == 0 {
                    started_tx.send(()).unwrap();
                    held.wait();
                    assert!(cancelled(), "old-root scan must be cancelled");
                }
                scanner::scan_catalog_until(roots, defaults, cancelled)
            },
        )
        .unwrap();
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        for _ in 0..20 {
            let generation = clock.fetch_add(1, Ordering::AcqRel) + 1;
            owner.request(vec![current.clone()], false, Default::default(), generation);
        }
        assert_eq!(scans.load(Ordering::Acquire), 1);
        assert!(owner.shared.state.lock().unwrap().requested);
        assert!(rx.try_recv().is_err());
        release.wait();
        let items = next(&owner, &clock, &rx, &[current.clone()], &Default::default());
        assert!(has(&items, &image));
        assert_eq!(scans.load(Ordering::Acquire), 2);
        {
            let mut s = owner.shared.state.lock().unwrap();
            assert!(s.observe_notification(Err(std::io::Error::from_raw_os_error(1022))));
            assert!(s.fallback);
            assert!(s.status.contains("overflow"));
            s.fallback = false;
            assert!(s.observe_notification(Err(std::io::Error::from_raw_os_error(5))));
            assert!(s.fallback);
        }
        drop(owner);
        // Exit interrupts an active cooperative scan and joins instead of publishing an obsolete result.
        let (tx, rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let stopped = CatalogWatch::spawn_scanning(
            Vec::new(),
            false,
            Default::default(),
            clock,
            move |wake| tx.send(wake).is_ok(),
            move |_, _, cancelled| {
                started_tx.send(()).unwrap();
                while !cancelled() {
                    std::thread::yield_now();
                }
                Vec::new()
            },
        )
        .unwrap();
        started_rx.recv_timeout(Duration::from_secs(3)).unwrap();
        let start = Instant::now();
        drop(stopped);
        assert!(start.elapsed() < Duration::from_secs(2));
        assert!(rx.try_recv().is_err());
    }
}

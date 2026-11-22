//! One lifetime owner, one scan and one dirty/request bit. No visibility dependency.
use super::{catalog_snapshot, Wake};
use crate::{candidate_state::preferences::LibraryPreferences, library, scanner};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
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
    status: String,
    fallback: bool,
    snapshot_path: Option<PathBuf>,
    offered_snapshot: Option<(u64, Result<catalog_snapshot::Snapshot, String>)>,
    accepted_snapshot: Option<Result<catalog_snapshot::Snapshot, String>>,
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
    stop: AtomicBool,
}
const STOP_GRACE: Duration = Duration::from_millis(100);
const AUTOMATIC_QUIET: Duration = Duration::from_secs(2);
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
        threads: usize,
        roots: Vec<PathBuf>,
        defaults: bool,
        prefs: LibraryPreferences,
        clock: Arc<AtomicU64>,
        publish: impl Fn(Wake) -> bool + Send + 'static,
    ) -> std::io::Result<Self> {
        Self::spawn_scanning_with_installer(
            threads,
            roots,
            defaults,
            prefs,
            clock,
            publish,
            move |roots, defaults, cancelled| {
                scanner::scan_catalog_with_threads_until(roots, defaults, threads, cancelled)
            },
            install_notifications,
        )
    }
    #[cfg(test)]
    pub(super) fn spawn_scanning(
        roots: Vec<PathBuf>,
        defaults: bool,
        prefs: LibraryPreferences,
        clock: Arc<AtomicU64>,
        publish: impl Fn(Wake) -> bool + Send + 'static,
        scan: impl Fn(&[PathBuf], bool, &(dyn Fn() -> bool + Sync)) -> Vec<scanner::WallpaperItem>
            + Send
            + 'static,
    ) -> std::io::Result<Self> {
        Self::spawn_scanning_with_installer(
            scanner::default_catalog_scan_threads(),
            roots,
            defaults,
            prefs,
            clock,
            publish,
            scan,
            |config, watches| install_roots(config, watches, &config.roots),
        )
    }
    fn spawn_scanning_with_installer(
        threads: usize,
        roots: Vec<PathBuf>,
        defaults: bool,
        prefs: LibraryPreferences,
        clock: Arc<AtomicU64>,
        publish: impl Fn(Wake) -> bool + Send + 'static,
        scan: impl Fn(&[PathBuf], bool, &(dyn Fn() -> bool + Sync)) -> Vec<scanner::WallpaperItem>
            + Send
            + 'static,
        install: impl FnMut(&Configuration, &mut Vec<Notification>) -> bool + Send + 'static,
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
                status: "Starting library watch…".into(),
                fallback: false,
                snapshot_path: None,
                offered_snapshot: None,
                accepted_snapshot: None,
            }),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
        });
        let worker = shared.clone();
        let thread = std::thread::Builder::new()
            .name("catalog-watch".into())
            .spawn(move || run(threads, worker, clock, publish, scan, install))?;
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
        let mut s = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
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
        let mut s = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        s.publication = false;
        s.offered_snapshot = None;
        self.shared.wake.notify_one();
    }
    pub(super) fn enable_snapshots(&self, path: PathBuf) {
        self.shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .snapshot_path = Some(path);
    }
    /// Delivery acknowledgement alone is never permission to persist a scan.
    pub(super) fn accept_snapshot(&self, generation: u64) {
        let mut s = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some((offered_generation, snapshot)) = s.offered_snapshot.take() {
            if generation == offered_generation {
                s.accepted_snapshot = Some(snapshot);
            }
        }
        s.publication = false;
        self.shared.wake.notify_one();
    }
    pub(super) fn status(&self) -> (String, bool) {
        let s = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        (s.status.clone(), s.fallback)
    }
}
impl Drop for CatalogWatch {
    fn drop(&mut self) {
        // Do not acquire state or free worker-owned native buffers during exit.
        self.shared.stop.store(true, Ordering::Release);
        self.shared.wake.notify_one();
        if let Some(thread) = self.thread.take() {
            let deadline = Instant::now() + STOP_GRACE;
            while !thread.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(1));
            }
            if thread.is_finished() {
                let _ = thread.join();
            }
            // Otherwise detach: its Arc and notifications remain owned until
            // scan/I/O teardown actually settles, including OVERLAPPED storage.
        }
    }
}

fn install_notifications(config: &Configuration, watches: &mut Vec<Notification>) -> bool {
    install_roots(
        config,
        watches,
        &scanner::catalog_watch_roots(&config.roots, config.defaults),
    )
}

fn install_roots(
    config: &Configuration,
    watches: &mut Vec<Notification>,
    roots: &[PathBuf],
) -> bool {
    watches.clear();
    let mut fallback = false;
    if config.prefs.watching() {
        for root in roots {
            match Notification::open(root) {
                Ok(watch) => watches.push(watch),
                Err(_) => fallback = true,
            }
        }
    }
    fallback
}

fn persist_accepted_snapshot(shared: &Shared, publish: &impl Fn(Wake) -> bool) -> bool {
    let accepted = {
        let mut s = shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        s.accepted_snapshot.take().zip(s.snapshot_path.clone())
    };
    if let Some((snapshot, path)) = accepted {
        let result = snapshot.and_then(|snapshot| catalog_snapshot::save(&path, &snapshot));
        publish(Wake::CatalogSnapshotSaved(result))
    } else {
        true
    }
}

fn run(
    threads: usize,
    shared: Arc<Shared>,
    clock: Arc<AtomicU64>,
    publish: impl Fn(Wake) -> bool,
    scan: impl Fn(&[PathBuf], bool, &(dyn Fn() -> bool + Sync)) -> Vec<scanner::WallpaperItem>,
    mut install: impl FnMut(&Configuration, &mut Vec<Notification>) -> bool,
) {
    let mut installed = None;
    let mut watches = Vec::new();
    let mut dirty_at = None;
    let mut last_poll = Instant::now();
    loop {
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        if !persist_accepted_snapshot(&shared, &publish) {
            break;
        }
        let (config, generation) = {
            let s = shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (s.config.clone(), s.generation)
        };
        if installed.as_ref() != Some(&config) {
            installed = None;
            // All native open/cancellation/settlement happens without state held.
            let fallback = install(&config, &mut watches);
            let mut s = shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if shared.stop.load(Ordering::Acquire) {
                break;
            }
            if s.config != config || s.generation != generation {
                continue;
            }
            s.fallback = fallback;
            s.status = if !s.config.prefs.watching() {
                "Automatic watching off".into()
            } else if s.fallback {
                "Polling fallback active: a root cannot provide notifications".into()
            } else {
                format!(
                    "Watching {} catalog root(s), including while hidden",
                    watches.len()
                )
            };
            installed = Some(config.clone());
            dirty_at = None;
            last_poll = Instant::now();
        }
        // changed() rearms native I/O too; do not call it under state either.
        let changes: Vec<_> = if config.prefs.watching() {
            watches.iter_mut().map(Notification::changed).collect()
        } else {
            Vec::new()
        };
        let mut s = shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if shared.stop.load(Ordering::Acquire) {
            break;
        }
        if s.config != config || s.generation != generation {
            // A discarded error can leave a notification permanently failed.
            // Rebuild outside the lock instead of losing its fallback signal.
            installed = None;
            continue;
        }
        if s.config.prefs.watching() {
            for changed in changes {
                if s.observe_notification(changed) {
                    dirty_at = Some(Instant::now());
                }
            }

            let poll = s.fallback
                && last_poll.elapsed() >= Duration::from_secs(s.config.prefs.interval() as u64);
            if poll {
                last_poll = Instant::now();
                // A short polling interval must not postpone convergence forever.
                if dirty_at.is_none() {
                    dirty_at = Some(Instant::now() - AUTOMATIC_QUIET);
                }
            }
            if dirty_at.is_some_and(|t| t.elapsed() >= AUTOMATIC_QUIET) && !s.dirty_wake {
                s.dirty_wake = true;
                dirty_at = None;
                let generation = s.generation;
                drop(s);
                if !publish(Wake::CatalogDirty { generation }) {
                    break;
                }
                s = shared
                    .state
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                if s.config != config || s.generation != generation {
                    installed = None;
                    continue;
                }
            }
        }
        if s.requested && !s.publication {
            s.requested = false;
            let config = s.config.clone();
            let generation = s.generation;
            drop(s);
            let cancelled = || {
                shared.stop.load(Ordering::Acquire) || generation != clock.load(Ordering::Acquire)
            };
            let wallpapers = scan(&config.roots, config.defaults, &cancelled);
            // Item and header phases never overlap, so the same cap covers all scan I/O.
            let entries = scanner::scan_ordered(
                &wallpapers,
                threads,
                &cancelled,
                library::CatalogEntry::from_wallpaper,
            );
            if cancelled() {
                continue;
            }
            let snapshot = catalog_snapshot::Snapshot::capture(
                &config.roots,
                config.defaults,
                &wallpapers,
                &entries,
            )
            .map_err(|e| e.to_string());
            s = shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if shared.stop.load(Ordering::Acquire) {
                break;
            }
            if config == s.config
                && generation == s.generation
                && generation == clock.load(Ordering::Acquire)
            {
                s.publication = true;
                s.offered_snapshot = Some((generation, snapshot));
                drop(s);
                if !publish(Wake::CatalogLoaded {
                    generation,
                    wallpapers,
                    entries,
                }) {
                    break;
                }
                continue;
            }
        }
        let _ = shared
            .wake
            .wait_timeout(s, Duration::from_millis(50))
            .unwrap_or_else(std::sync::PoisonError::into_inner);
    }
    // An already accepted snapshot survives owner shutdown, including a cancelled scan.
    // Drop remains bounded: a slow flush retains the worker Arc until it completes.
    let _ = persist_accepted_snapshot(&shared, &publish);
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
    fn wait_for_release(weak: std::sync::Weak<Shared>) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while weak.upgrade().is_some() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(weak.upgrade().is_none(), "detached worker did not settle");
    }

    #[test]
    fn p54_latest_only_and_drop_retain_scoped_io_until_it_settles() {
        for stop in [false, true] {
            let current = root("p54-scoped-current");
            let image = current.join("owned.png");
            image::RgbaImage::new(2, 3).save(&image).unwrap();
            let clock = Arc::new(AtomicU64::new(1));
            let (tx, rx) = mpsc::channel();
            let (entered, starts) = mpsc::channel();
            let gate = Arc::new((Mutex::new(false), Condvar::new()));
            let held = gate.clone();
            let active = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let count = active.clone();
            let scans = std::sync::atomic::AtomicUsize::new(0);
            let owner = CatalogWatch::spawn_scanning_with_installer(
                2,
                vec![current.clone()],
                false,
                Default::default(),
                clock.clone(),
                move |wake| tx.send(wake).is_ok(),
                move |roots, defaults, cancelled| {
                    if scans.fetch_add(1, Ordering::SeqCst) == 0 {
                        let result = scanner::scan_ordered(&[0, 1, 2, 3], 2, cancelled, |_| {
                            count.fetch_add(1, Ordering::SeqCst);
                            entered.send(()).unwrap();
                            let (open, _) = held
                                .1
                                .wait_timeout_while(
                                    held.0.lock().unwrap(),
                                    Duration::from_secs(10),
                                    |open| !*open,
                                )
                                .unwrap();
                            assert!(*open);
                            count.fetch_sub(1, Ordering::SeqCst);
                        });
                        assert!(cancelled());
                        assert!(result.is_empty());
                    }
                    scanner::scan_catalog_with_threads_until(roots, defaults, 2, cancelled)
                },
                |config, watches| install_roots(config, watches, &config.roots),
            )
            .unwrap();
            let entered: Vec<_> = (0..2)
                .map(|_| starts.recv_timeout(Duration::from_secs(10)))
                .collect();
            let weak = Arc::downgrade(&owner.shared);
            if stop {
                let start = Instant::now();
                drop(owner);
                let elapsed = start.elapsed();
                let retained = weak.upgrade().is_some();
                let held_count = active.load(Ordering::SeqCst);
                *gate.0.lock().unwrap() = true;
                gate.1.notify_all();
                wait_for_release(weak);
                assert!(entered.iter().all(Result::is_ok));
                assert!(elapsed < Duration::from_millis(500));
                assert!(retained, "detached owner must retain both scoped lanes");
                assert_eq!(held_count, 2);
                assert!(rx.try_recv().is_err(), "stopped work cannot publish");
            } else {
                clock.store(2, Ordering::Release);
                owner.request(vec![current.clone()], false, Default::default(), 2);
                let before_release = rx.try_recv();
                *gate.0.lock().unwrap() = true;
                gate.1.notify_all();
                let wake = rx.recv_timeout(Duration::from_secs(20)).unwrap();
                assert!(entered.iter().all(Result::is_ok));
                assert!(before_release.is_err());
                match wake {
                    Wake::CatalogLoaded {
                        generation,
                        wallpapers,
                        entries,
                    } => {
                        assert_eq!(generation, 2, "held generation must never publish");
                        assert!(has(&wallpapers, &image));
                        assert_eq!(wallpapers.len(), entries.len());
                        for (item, entry) in wallpapers.iter().zip(&entries) {
                            assert_eq!(item.id, entry.id);
                        }
                    }
                    wake => panic!("expected current catalog, got {wake:?}"),
                }
                drop(owner);
                wait_for_release(weak);
            }
            assert_eq!(active.load(Ordering::SeqCst), 0);
            std::fs::remove_dir_all(current).unwrap();
        }
    }

    #[test]
    fn p45_stopped_worker_flushes_accepted_snapshot_after_held_scan_releases() {
        let root = root("p45-shutdown");
        image::RgbaImage::new(3, 2)
            .save(root.join("owned.png"))
            .unwrap();
        let roots = vec![root.clone()];
        let wallpapers = scanner::scan_local_images(&roots);
        let entries = wallpapers
            .iter()
            .map(library::CatalogEntry::from_wallpaper)
            .collect::<Vec<_>>();
        let snapshot =
            catalog_snapshot::Snapshot::capture(&roots, false, &wallpapers, &entries).unwrap();
        let path = root.join("snapshot.json");
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (tx, rx) = mpsc::channel();
        let owner = CatalogWatch::spawn_scanning(
            roots.clone(),
            false,
            Default::default(),
            Arc::new(AtomicU64::new(1)),
            move |w| tx.send(w).is_ok(),
            move |_, _, _| {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                Vec::new()
            },
        )
        .unwrap();
        entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        // Force already-accepted disk work to remain pending while scan I/O is held.
        {
            let mut s = owner.shared.state.lock().unwrap();
            s.snapshot_path = Some(path.clone());
            s.accepted_snapshot = Some(Ok(snapshot));
        }
        let weak = Arc::downgrade(&owner.shared);
        let started = Instant::now();
        drop(owner);
        assert!(
            started.elapsed() < Duration::from_millis(500),
            "bounded owner shutdown"
        );
        assert!(
            !path.exists(),
            "accepted save is still pending behind held I/O"
        );
        assert!(
            weak.upgrade().is_some(),
            "detached worker retains its lifetime owner"
        );
        release_tx.send(()).unwrap();
        let result = rx.recv_timeout(Duration::from_secs(10)).unwrap();
        assert!(
            matches!(result, Wake::CatalogSnapshotSaved(Ok(()))),
            "{result:?}"
        );
        wait_for_release(weak);
        assert_eq!(
            catalog_snapshot::load(&path, &roots, false)
                .unwrap()
                .into_catalog()
                .0,
            wallpapers
        );
    }

    #[test]
    fn p42_synthetic_steam_subscriptions_and_myprojects_converge_without_manual_refresh() {
        for initially_missing in [false, true] {
            let library = root(if initially_missing {
                "steam-missing"
            } else {
                "steam-watch"
            });
            let libraries = vec![library];
            let watch_roots = scanner::engine_catalog_roots(&libraries, false);
            if !initially_missing {
                for path in &watch_roots {
                    std::fs::create_dir_all(path).unwrap();
                }
            }
            let clock = Arc::new(AtomicU64::new(1));
            let prefs = LibraryPreferences {
                polling_seconds: Some(1),
                ..Default::default()
            };
            let (tx, rx) = mpsc::channel();
            let scan_libraries = libraries.clone();
            let installed = watch_roots.clone();
            let owner = CatalogWatch::spawn_scanning_with_installer(
                scanner::default_catalog_scan_threads(),
                Vec::new(),
                false,
                prefs.clone(),
                clock.clone(),
                move |w| tx.send(w).is_ok(),
                move |_, defaults, cancelled| {
                    scanner::scan_wallpapers_from_libraries_until(
                        &scan_libraries,
                        defaults,
                        cancelled,
                    )
                },
                move |config, watches| install_roots(config, watches, &installed),
            )
            .unwrap();
            assert!(next(&owner, &clock, &rx, &[], &prefs).is_empty());
            assert_eq!(owner.status().1, initially_missing);
            for (i, path) in watch_roots.iter().enumerate() {
                let item = path.join(format!("owned-{i}"));
                std::fs::create_dir_all(&item).unwrap();
                std::fs::write(
                    item.join("project.json"),
                    format!(r#"{{"title":"Owned {i}","type":"scene"}}"#),
                )
                .unwrap();
            }
            let deadline = Instant::now() + Duration::from_secs(20);
            let items = loop {
                let items = next(&owner, &clock, &rx, &[], &prefs);
                if items.len() == 2 {
                    break items;
                }
                assert!(
                    Instant::now() < deadline,
                    "synthetic Steam catalog did not converge"
                );
            };
            assert_eq!(items.len(), 2);
            assert!(items
                .iter()
                .all(|i| i.source == scanner::WallpaperSource::WallpaperEngine));
            assert!(items.iter().any(|i| i.id == "owned-0"));
            assert!(items.iter().any(|i| i.id == "owned-1"));
            drop(owner);
        }
    }

    #[test]
    fn p44_automatic_burst_is_trailing_but_initial_and_manual_scans_are_prompt() {
        let root = root("p44-storm");
        let roots = vec![root.clone()];
        let clock = Arc::new(AtomicU64::new(1));
        let prefs = LibraryPreferences::default();
        assert_eq!(prefs.interval(), 60);
        let (tx, rx) = mpsc::channel();
        let start = Instant::now();
        let owner = CatalogWatch::spawn_scanning(
            roots.clone(),
            false,
            prefs.clone(),
            clock.clone(),
            move |w| tx.send(w).is_ok(),
            |roots, _, _| scanner::scan_local_media(roots),
        )
        .unwrap();
        next(&owner, &clock, &rx, &roots, &prefs);
        assert!(start.elapsed() < Duration::from_millis(1500));
        for i in 0..5 {
            std::fs::write(root.join("owned-storm.txt"), format!("{i}")).unwrap();
            assert!(
                rx.recv_timeout(Duration::from_millis(350)).is_err(),
                "leading-edge scan during event storm"
            );
        }
        let start = Instant::now();
        let generation = clock.fetch_add(1, Ordering::AcqRel) + 1;
        owner.request(roots.clone(), false, prefs.clone(), generation);
        next(&owner, &clock, &rx, &roots, &prefs);
        assert!(
            start.elapsed() < Duration::from_millis(1500),
            "manual refresh waited for quiet window"
        );
        for i in 5..9 {
            std::fs::write(root.join("owned-storm.txt"), format!("{i}")).unwrap();
            assert!(rx.recv_timeout(Duration::from_millis(250)).is_err());
        }
        let final_event = Instant::now();
        std::fs::write(root.join("owned-storm.txt"), "final").unwrap();
        assert!(
            rx.recv_timeout(Duration::from_millis(1750)).is_err(),
            "automatic scan preceded trailing quiet window"
        );
        assert!(matches!(
            rx.recv_timeout(Duration::from_millis(1500)).unwrap(),
            Wake::CatalogDirty { .. }
        ));
        assert!(final_event.elapsed() >= Duration::from_secs(2));
        assert!(
            rx.recv_timeout(Duration::from_millis(150)).is_err(),
            "duplicate dirty publication"
        );
        drop(owner);
    }

    #[test]
    fn p39_held_installation_keeps_state_responsive_and_discards_stale_config() {
        let old = root("install-old");
        let current = root("install-current");
        let image = current.join("current.png");
        image::RgbaImage::new(2, 2).save(&image).unwrap();
        let clock = Arc::new(AtomicU64::new(1));
        let (tx, rx) = mpsc::channel();
        let (entered, started) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let (installed, installations) = mpsc::channel();
        let mut first = true;
        let owner = Arc::new(
            CatalogWatch::spawn_scanning_with_installer(
                scanner::default_catalog_scan_threads(),
                vec![old.clone()],
                false,
                Default::default(),
                clock.clone(),
                move |wake| tx.send(wake).is_ok(),
                scanner::scan_catalog_until,
                move |config, watches| {
                    if first {
                        first = false;
                        entered.send(()).unwrap();
                        resume.recv_timeout(Duration::from_secs(5)).unwrap();
                    }
                    let fallback = install_notifications(config, watches);
                    installed.send(config.roots.clone()).unwrap();
                    fallback
                },
            )
            .unwrap(),
        );
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let controlled = Arc::clone(&owner);
        let roots = vec![current.clone()];
        let requested = roots.clone();
        let generation = clock.fetch_add(1, Ordering::AcqRel) + 1;
        let (done, finished) = mpsc::channel();
        let control = std::thread::spawn(move || {
            controlled.status();
            controlled.acknowledge();
            controlled.request(requested, false, Default::default(), generation);
            done.send(()).unwrap();
        });
        let responsive = finished.recv_timeout(Duration::from_secs(1));
        let _ = release.send(());
        control.join().unwrap();
        assert!(responsive.is_ok(), "state API blocked behind installation");
        let items = next(&owner, &clock, &rx, &roots, &Default::default());
        assert!(has(&items, &image));
        assert_eq!(
            installations.recv_timeout(Duration::from_secs(5)).unwrap(),
            vec![old]
        );
        assert_eq!(
            installations.recv_timeout(Duration::from_secs(5)).unwrap(),
            roots
        );
        drop(owner);
    }

    #[test]
    fn p39_held_teardown_keeps_state_responsive_and_drop_retains_native_owner() {
        let old = root("teardown-old");
        let current = root("teardown-current");
        let clock = Arc::new(AtomicU64::new(1));
        let (tx, rx) = mpsc::channel();
        let (entered, started) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let owner = Arc::new(
            CatalogWatch::spawn_scanning_with_installer(
                scanner::default_catalog_scan_threads(),
                vec![old.clone()],
                false,
                Default::default(),
                clock.clone(),
                move |wake| tx.send(wake).is_ok(),
                scanner::scan_catalog_until,
                move |config, watches| {
                    if !watches.is_empty() {
                        entered.send(()).unwrap();
                        resume.recv_timeout(Duration::from_secs(5)).unwrap();
                    }
                    install_notifications(config, watches)
                },
            )
            .unwrap(),
        );
        next(&owner, &clock, &rx, &[old], &Default::default());
        let generation = clock.fetch_add(1, Ordering::AcqRel) + 1;
        owner.request(vec![current], false, Default::default(), generation);
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let weak = Arc::downgrade(&owner.shared);
        let controlled = Arc::clone(&owner);
        let (done, finished) = mpsc::channel();
        let control = std::thread::spawn(move || {
            controlled.status();
            controlled.acknowledge();
            done.send(()).unwrap();
        });
        let responsive = finished.recv_timeout(Duration::from_secs(1));
        // Always release the native gate even when a regression blocks state.
        if responsive.is_err() {
            let _ = release.send(());
            control.join().unwrap();
            panic!("state API blocked behind teardown");
        }
        control.join().unwrap();
        let (dropped, returned) = mpsc::channel();
        let stop = std::thread::spawn(move || {
            let start = Instant::now();
            drop(owner);
            dropped.send(start.elapsed()).unwrap();
        });
        let result = returned.recv_timeout(Duration::from_secs(1));
        let retained = weak.upgrade().is_some();
        let _ = release.send(());
        stop.join().unwrap();
        assert!(result.is_ok(), "owner Drop waited for native teardown");
        assert!(
            retained,
            "pending native resources must remain worker-owned"
        );
        wait_for_release(weak);
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn p39_drop_is_bounded_with_held_state_and_noncooperative_scan() {
        for hold_state in [false, true] {
            let (tx, rx) = mpsc::channel();
            let (entered, started) = mpsc::channel();
            let (release, resume) = mpsc::channel();
            let owner = CatalogWatch::spawn_scanning(
                Vec::new(),
                false,
                Default::default(),
                Arc::new(AtomicU64::new(1)),
                move |wake| tx.send(wake).is_ok(),
                move |_, _, _| {
                    entered.send(()).unwrap();
                    resume.recv_timeout(Duration::from_secs(5)).unwrap();
                    Vec::new()
                },
            )
            .unwrap();
            started.recv_timeout(Duration::from_secs(5)).unwrap();
            let shared = Arc::clone(&owner.shared);
            let weak = Arc::downgrade(&shared);
            let held = hold_state.then(|| shared.state.lock().unwrap());
            let (dropped, returned) = mpsc::channel();
            let stop = std::thread::spawn(move || {
                drop(owner);
                dropped.send(()).unwrap();
            });
            let result = returned.recv_timeout(Duration::from_secs(1));
            assert!(shared.stop.load(Ordering::Acquire));
            drop(held);
            let _ = release.send(());
            stop.join().unwrap();
            assert!(result.is_ok(), "Drop waited for held state or scan");
            drop(shared);
            wait_for_release(weak);
            assert!(rx.try_recv().is_err());
        }
    }

    #[test]
    fn p39_poisoned_state_accessors_and_current_publication_recover() {
        let current = root("poison");
        let clock = Arc::new(AtomicU64::new(1));
        let (tx, rx) = mpsc::channel();
        let (entered, started) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let mut first = true;
        let owner = CatalogWatch::spawn_scanning_with_installer(
            scanner::default_catalog_scan_threads(),
            vec![current.clone()],
            false,
            Default::default(),
            clock.clone(),
            move |wake| tx.send(wake).is_ok(),
            scanner::scan_catalog_until,
            move |config, watches| {
                if first {
                    first = false;
                    entered.send(()).unwrap();
                    resume.recv_timeout(Duration::from_secs(5)).unwrap();
                }
                install_notifications(config, watches)
            },
        )
        .unwrap();
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let poisoned = Arc::clone(&owner.shared);
        assert!(std::thread::spawn(move || {
            let _held = poisoned.state.lock().unwrap();
            panic!("controlled catalog-state poison");
        })
        .join()
        .is_err());
        owner.status();
        owner.acknowledge();
        let generation = clock.fetch_add(1, Ordering::AcqRel) + 1;
        owner.request(vec![current.clone()], false, Default::default(), generation);
        release.send(()).unwrap();
        next(&owner, &clock, &rx, &[current], &Default::default());
        assert!(owner.shared.state.is_poisoned());
        drop(owner);
    }

    #[test]
    fn w4_real_notifications_add_rename_remove_coalesce_and_release_on_drop() {
        let root = root("notifications");
        let roots = vec![root.clone()];
        let clock = Arc::new(AtomicU64::new(1));
        let prefs = LibraryPreferences::default();
        let (tx, rx) = mpsc::channel();
        let owner = CatalogWatch::spawn_scanning(
            roots.clone(),
            false,
            prefs.clone(),
            clock.clone(),
            move |w| tx.send(w).is_ok(),
            scanner::scan_catalog_until,
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
            scanner::default_catalog_scan_threads(),
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

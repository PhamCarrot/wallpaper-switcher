use std::{
    ffi::c_void,
    fmt,
    fs::{File, OpenOptions},
    os::windows::{ffi::OsStringExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::{Path, PathBuf},
    ptr::null,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread::{self, JoinHandle},
};

use windows_sys::Win32::{
    Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
    Storage::FileSystem::{
        FileIdInfo, GetFileInformationByHandleEx, GetFinalPathNameByHandleW,
        FILE_FLAG_BACKUP_SEMANTICS, FILE_ID_INFO, FILE_SHARE_READ, FILE_SHARE_WRITE,
    },
    UI::WindowsAndMessaging::{MessageBoxW, MB_ICONINFORMATION, MB_OK, MB_SETFOREGROUND},
};

const CANDIDATE_MUTEX_PREFIX: &str = r"Local\WallpaperSwitcher.GpuCandidate.SingleInstance";
const REOPEN_EVENT_SUFFIX: &str = ".Reopen.v1";
const WAIT_OBJECT_0: u32 = 0;
const WAIT_TIMEOUT: u32 = 258;
const REOPEN_POLL_MS: u32 = 100;

/// One resolved directory, pinned against rename/deletion until the runtime exits.
#[derive(Debug)]
pub struct ProfileIdentity {
    volume_serial: u64,
    file_id: [u8; 16],
    canonical_path: PathBuf,
    _directory_handle: File,
}

impl ProfileIdentity {
    pub fn open(profile: &Path) -> Result<Self, String> {
        let fail = |error| {
            format!(
                "Could not resolve physical profile {}: {error}",
                profile.display()
            )
        };
        // Follow aliases; omit FILE_SHARE_DELETE so the resolved directory stays put.
        let directory = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
            .open(profile)
            .map_err(fail)?;
        if !directory.metadata().map_err(fail)?.is_dir() {
            return Err(format!("Profile is not a directory: {}", profile.display()));
        }
        let handle = directory.as_raw_handle() as HANDLE;
        let mut info: FILE_ID_INFO = unsafe { std::mem::zeroed() };
        // SAFETY: the owned directory handle and correctly sized output are valid.
        if unsafe {
            GetFileInformationByHandleEx(
                handle,
                FileIdInfo,
                &mut info as *mut _ as *mut c_void,
                std::mem::size_of::<FILE_ID_INFO>() as u32,
            )
        } == 0
        {
            return Err(fail(std::io::Error::last_os_error()));
        }
        // Refuse unsupported/ambiguous identity rather than falling back to spelling.
        if info.FileId.Identifier == [0; 16] {
            return Err("Filesystem returned an unusable physical profile ID.".into());
        }
        let mut path = vec![0u16; 32768];
        // SAFETY: handle is live and path is a writable UTF-16 buffer. Flags 0 request
        // the normalized final DOS path, including resolution of junctions/symlinks.
        let length =
            unsafe { GetFinalPathNameByHandleW(handle, path.as_mut_ptr(), path.len() as u32, 0) };
        if length == 0 {
            return Err(fail(std::io::Error::last_os_error()));
        }
        if length as usize >= path.len() {
            return Err("Physical profile final path is ambiguous or too long.".into());
        }
        let canonical_path = PathBuf::from(std::ffi::OsString::from_wide(&path[..length as usize]));
        if !canonical_path.is_absolute() {
            return Err("Physical profile final path is not absolute.".into());
        }
        Ok(Self {
            volume_serial: info.VolumeSerialNumber,
            file_id: info.FileId.Identifier,
            canonical_path,
            _directory_handle: directory,
        })
    }

    pub fn canonical_path(&self) -> &Path {
        &self.canonical_path
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(
        mutex_attributes: *const c_void,
        initial_owner: i32,
        name: *const u16,
    ) -> HANDLE;
    fn CreateEventW(
        event_attributes: *const c_void,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> HANDLE;
    fn SetEvent(event: HANDLE) -> i32;
    fn WaitForSingleObject(handle: HANDLE, milliseconds: u32) -> u32;
}

#[derive(Debug, Eq, PartialEq)]
pub enum CandidateInstanceError {
    AlreadyRunning,
    System(u32),
}

impl fmt::Display for CandidateInstanceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRunning => formatter.write_str(
                "Wallpaper Switcher GPU candidate is already running for this profile.",
            ),
            Self::System(code) => write!(
                formatter,
                "Wallpaper Switcher GPU candidate could not establish its single-instance guard (Windows error {code})."
            ),
        }
    }
}

pub struct CandidateInstanceGuard {
    handle: HANDLE,
    reopen_event: HANDLE,
    listener: Option<CandidateReopenListener>,
    // Dropped after the listener and kernel handles, including early runtime returns.
    profile: Option<ProfileIdentity>,
}

struct CandidateReopenListener {
    stop: Arc<AtomicBool>,
    event_handle: usize,
    thread: Option<JoinHandle<()>>,
}

impl CandidateInstanceGuard {
    pub fn acquire(
        profile: ProfileIdentity,
        request_reopen: bool,
    ) -> Result<Self, CandidateInstanceError> {
        let mut guard =
            Self::acquire_named_with_policy(&profile_instance_name(&profile), request_reopen)?;
        guard.profile = Some(profile);
        Ok(guard)
    }

    pub fn profile(&self) -> &ProfileIdentity {
        self.profile.as_ref().expect("physical profile guard")
    }

    #[cfg(test)]
    fn acquire_named(name: &str) -> Result<Self, CandidateInstanceError> {
        Self::acquire_named_with_policy(name, true)
    }

    fn acquire_named_with_policy(
        name: &str,
        request_reopen: bool,
    ) -> Result<Self, CandidateInstanceError> {
        let event_name = format!("{name}{REOPEN_EVENT_SUFFIX}");
        let event_name = event_name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();
        // SAFETY: `event_name` is a valid, null-terminated UTF-16 buffer alive
        // through the call. The auto-reset event is shared by candidate instances.
        let reopen_event = unsafe { CreateEventW(null(), 0, 0, event_name.as_ptr()) };
        if reopen_event.is_null() {
            return Err(CandidateInstanceError::System(unsafe { GetLastError() }));
        }

        let name = name
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<_>>();

        // SAFETY: `name` is a valid, null-terminated UTF-16 buffer that remains alive
        // for the duration of the call. Default security attributes are requested.
        let handle = unsafe { CreateMutexW(null(), 0, name.as_ptr()) };
        if handle.is_null() {
            let error = unsafe { GetLastError() };
            unsafe { CloseHandle(reopen_event) };
            // SAFETY: `GetLastError` has no preconditions and is called immediately
            // after the failed Windows API call.
            return Err(CandidateInstanceError::System(error));
        }

        // This must be read before making another Windows API call.
        // SAFETY: `GetLastError` has no preconditions.
        let last_error = unsafe { GetLastError() };
        if last_error == ERROR_ALREADY_EXISTS {
            if request_reopen {
                // Signal the resident process before releasing this duplicate's handles.
                // SAFETY: both handles are valid results returned by Windows.
                if unsafe { SetEvent(reopen_event) } == 0 {
                    let error = unsafe { GetLastError() };
                    unsafe {
                        CloseHandle(reopen_event);
                        CloseHandle(handle);
                    }
                    return Err(CandidateInstanceError::System(error));
                }
            }
            unsafe { CloseHandle(handle) };
            unsafe { CloseHandle(reopen_event) };
            return Err(CandidateInstanceError::AlreadyRunning);
        }

        Ok(Self {
            handle,
            reopen_event,
            listener: None,
            profile: None,
        })
    }

    pub fn start_reopen_listener<F>(&mut self, on_reopen: F) -> Result<(), CandidateInstanceError>
    where
        F: Fn() + Send + 'static,
    {
        if self.listener.is_some() {
            return Err(CandidateInstanceError::System(ERROR_ALREADY_EXISTS));
        }
        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let event_handle = self.reopen_event as usize;
        let worker = thread::Builder::new()
            .name("gpu-candidate-resident-reopen".into())
            .spawn(move || {
                let event = event_handle as HANDLE;
                while !thread_stop.load(Ordering::Acquire) {
                    // SAFETY: the owner guard remains alive until this listener is
                    // dropped; the wait is bounded so shutdown is responsive.
                    match unsafe { WaitForSingleObject(event, REOPEN_POLL_MS) } {
                        WAIT_OBJECT_0 => {
                            if !thread_stop.load(Ordering::Acquire) {
                                on_reopen();
                            }
                        }
                        WAIT_TIMEOUT => {}
                        _ => break,
                    }
                }
            })
            .map_err(|error| {
                CandidateInstanceError::System(error.raw_os_error().unwrap_or(0) as u32)
            })?;

        self.listener = Some(CandidateReopenListener {
            stop,
            event_handle,
            thread: Some(worker),
        });
        Ok(())
    }

    #[cfg(test)]
    fn try_take_reopen_signal(&self) -> bool {
        // SAFETY: this guard owns a valid auto-reset event handle.
        unsafe { WaitForSingleObject(self.reopen_event, 0) == WAIT_OBJECT_0 }
    }
}

fn profile_instance_name(profile: &ProfileIdentity) -> String {
    let file_id = u128::from_be_bytes(profile.file_id);
    format!(
        "{CANDIDATE_MUTEX_PREFIX}.{:016X}.{file_id:032X}.physical.v1",
        profile.volume_serial
    )
}

impl Drop for CandidateReopenListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        // Wake the bounded waiter; it checks `stop` before posting a reopen.
        unsafe {
            SetEvent(self.event_handle as HANDLE);
        }
        if let Some(worker) = self.thread.take() {
            let _ = worker.join();
        }
    }
}

impl Drop for CandidateInstanceGuard {
    fn drop(&mut self) {
        // Join before either event or mutex can disappear.
        drop(self.listener.take());
        // SAFETY: this guard uniquely owns the valid handle returned by
        // `CreateMutexW`, and `drop` runs at most once.
        unsafe { CloseHandle(self.reopen_event) };
        unsafe { CloseHandle(self.handle) };
    }
}

pub fn show_rejection_diagnostic(error: &CandidateInstanceError) {
    let caption = "Wallpaper Switcher GPU candidate\0"
        .encode_utf16()
        .collect::<Vec<_>>();
    let message = format!("{error}\0").encode_utf16().collect::<Vec<_>>();

    // SAFETY: the caption and message are valid, null-terminated UTF-16 buffers
    // and remain alive for the duration of the modal message box call.
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            message.as_ptr(),
            caption.as_ptr(),
            MB_OK | MB_ICONINFORMATION | MB_SETFOREGROUND,
        );
    }
}

#[cfg(test)]
mod tests {
    use std::{
        env,
        path::Path,
        process,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    };

    use super::{
        profile_instance_name, CandidateInstanceError, CandidateInstanceGuard, ProfileIdentity,
    };

    static NEXT_TEST_MUTEX: AtomicU64 = AtomicU64::new(0);
    const TEST_MUTEX_ENV: &str = "WSL_CANDIDATE_MUTEX_TEST_NAME";
    const TEST_EXPECTATION_ENV: &str = "WSL_CANDIDATE_MUTEX_TEST_EXPECTATION";

    fn unique_mutex_name() -> String {
        format!(
            r"Local\WallpaperSwitcher.GpuCandidate.Test.{}.{}",
            process::id(),
            NEXT_TEST_MUTEX.fetch_add(1, Ordering::Relaxed)
        )
    }

    #[test]
    fn named_guard_rejects_a_duplicate_and_releases_on_drop() {
        let name = unique_mutex_name();
        let first = CandidateInstanceGuard::acquire_named(&name).expect("first guard");

        assert!(matches!(
            CandidateInstanceGuard::acquire_named(&name),
            Err(CandidateInstanceError::AlreadyRunning)
        ));
        assert!(first.try_take_reopen_signal());

        drop(first);
        CandidateInstanceGuard::acquire_named(&name).expect("guard after release");
    }

    fn owned_profile(label: &str) -> std::path::PathBuf {
        let path = env::temp_dir().join(format!(
            "wsl-s2-{}-{}-{label}",
            process::id(),
            NEXT_TEST_MUTEX.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn instance_names_are_case_insensitive_and_scoped_to_the_profile() {
        let path = owned_profile("case");
        let upper = std::path::PathBuf::from(path.to_string_lossy().to_uppercase());
        let first = ProfileIdentity::open(&path).unwrap();
        let second = ProfileIdentity::open(&upper).unwrap();
        assert_eq!(
            profile_instance_name(&first),
            profile_instance_name(&second)
        );
        assert_ne!(
            profile_instance_name(&first),
            profile_instance_name(&ProfileIdentity::open(&owned_profile("other")).unwrap())
        );
    }

    #[test]
    fn candidate_instances_are_isolated_by_profile_directory() {
        let root = env::temp_dir().join(format!(
            "wsl-instance-profile-{}-{}",
            process::id(),
            NEXT_TEST_MUTEX.fetch_add(1, Ordering::Relaxed)
        ));
        let diagnostic_profile = root.join("diagnostic");
        let normal_profile = root.join("normal");

        std::fs::create_dir_all(&diagnostic_profile).unwrap();
        std::fs::create_dir_all(&normal_profile).unwrap();
        let diagnostic = CandidateInstanceGuard::acquire(
            ProfileIdentity::open(&diagnostic_profile).unwrap(),
            true,
        )
        .expect("diagnostic profile guard");
        let normal =
            CandidateInstanceGuard::acquire(ProfileIdentity::open(&normal_profile).unwrap(), true)
                .expect("independent normal profile guard");
        assert!(matches!(
            CandidateInstanceGuard::acquire(
                ProfileIdentity::open(&diagnostic_profile).unwrap(),
                true
            ),
            Err(CandidateInstanceError::AlreadyRunning)
        ));

        drop(normal);
        drop(diagnostic);
    }

    #[test]
    fn non_ui_duplicate_does_not_post_a_reopen_signal() {
        let profile = env::temp_dir().join(format!(
            "wsl-instance-no-reopen-{}-{}",
            process::id(),
            NEXT_TEST_MUTEX.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&profile).unwrap();
        let resident =
            CandidateInstanceGuard::acquire(ProfileIdentity::open(&profile).unwrap(), true)
                .expect("resident guard");
        assert!(matches!(
            CandidateInstanceGuard::acquire(ProfileIdentity::open(&profile).unwrap(), false),
            Err(CandidateInstanceError::AlreadyRunning)
        ));
        assert!(!resident.try_take_reopen_signal());
    }

    #[test]
    fn resident_reopen_listener_dispatches_a_duplicate_launch_request() {
        let name = unique_mutex_name();
        let mut resident = CandidateInstanceGuard::acquire_named(&name).expect("resident guard");
        let (sender, receiver) = std::sync::mpsc::channel();
        resident
            .start_reopen_listener(move || sender.send(()).unwrap())
            .expect("listener");

        assert!(matches!(
            CandidateInstanceGuard::acquire_named(&name),
            Err(CandidateInstanceError::AlreadyRunning)
        ));
        receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("reopen callback");
        drop(resident);
    }

    #[test]
    fn named_guard_rejects_another_process_and_releases_for_it() {
        let name = unique_mutex_name();
        let first = CandidateInstanceGuard::acquire_named(&name).expect("first guard");

        run_child_probe(&name, "blocked");
        drop(first);
        run_child_probe(&name, "available");
    }

    #[test]
    fn physical_alias_children_exclude_reopen_and_release_with_listener() {
        let root = owned_profile("physical-children");
        let profile = root.join("profile");
        let other = root.join("distinct");
        std::fs::create_dir(&profile).unwrap();
        std::fs::create_dir(&other).unwrap();
        let alias = root.join("junction");
        let status = Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&alias)
            .arg(&profile)
            .status()
            .unwrap();
        assert!(status.success(), "real owned directory junction required");
        let mut resident =
            CandidateInstanceGuard::acquire(ProfileIdentity::open(&profile).unwrap(), true)
                .unwrap();
        eprintln!(
            "S2 fixture: {root:?}; physical owner: {:?}",
            resident.profile()
        );
        let upper = std::path::PathBuf::from(profile.to_string_lossy().to_uppercase());
        let verbatim = resident.profile().canonical_path().to_path_buf();
        for path in [&profile, &upper, &verbatim, &alias] {
            run_child_probe(path.to_str().unwrap(), "profile-blocked-quiet");
            assert!(
                !resident.try_take_reopen_signal(),
                "non-UI duplicate signalled"
            );
        }
        run_child_probe(other.to_str().unwrap(), "profile-available");
        let (tx, rx) = std::sync::mpsc::channel();
        resident
            .start_reopen_listener(move || {
                tx.send(()).unwrap();
            })
            .unwrap();
        run_child_probe(alias.to_str().unwrap(), "profile-blocked-quiet");
        assert!(rx.recv_timeout(Duration::from_millis(150)).is_err());
        run_child_probe(alias.to_str().unwrap(), "profile-blocked-reopen");
        rx.recv_timeout(Duration::from_secs(2))
            .expect("normal reopen via junction");
        assert!(
            std::fs::rename(&profile, root.join("moved")).is_err(),
            "retained directory must stay pinned"
        );
        drop(resident); // owns listener: joins it before releasing handles/identity
        assert!(
            rx.recv_timeout(Duration::from_millis(150)).is_err(),
            "shutdown must not post a reopen"
        );
        run_child_probe(alias.to_str().unwrap(), "profile-available");
        std::fs::rename(&profile, root.join("moved"))
            .expect("directory released after guard/listener drop");
    }

    #[test]
    fn physical_identity_resolution_errors_never_create_or_fall_back() {
        use std::os::windows::fs::OpenOptionsExt;
        let root = owned_profile("resolution-errors");
        let missing = root.join("missing");
        assert!(ProfileIdentity::open(&missing).is_err());
        assert!(!missing.exists());
        let file = root.join("file");
        std::fs::write(&file, b"preserved").unwrap();
        assert!(ProfileIdentity::open(&file).is_err());
        let denied = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .custom_flags(windows_sys::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS)
            .open(&root)
            .unwrap();
        let error = ProfileIdentity::open(&root).unwrap_err();
        eprintln!("S2 deterministic access/sharing refusal: {error}");
        assert!(error.contains("resolve physical profile"));
        drop(denied);
        ProfileIdentity::open(&root).expect("access restored");
        assert_eq!(std::fs::read(&file).unwrap(), b"preserved");
    }

    #[test]
    fn simultaneous_physical_alias_children_have_one_owner() {
        let root = owned_profile("simultaneous");
        let profile = root.join("profile");
        std::fs::create_dir(&profile).unwrap();
        let alias = root.join("junction");
        assert!(Command::new("cmd.exe")
            .args(["/d", "/c", "mklink", "/J"])
            .arg(&alias)
            .arg(&profile)
            .status()
            .unwrap()
            .success());
        let mut children = Vec::new();
        for (index, path) in [&profile, &alias].iter().enumerate() {
            children.push(
                child_probe_command(path.to_str().unwrap(), "profile-race")
                    .env("WSL_S2_RACE_ROOT", &root)
                    .env("WSL_S2_RACE_INDEX", index.to_string())
                    .spawn()
                    .unwrap(),
            );
        }
        let wait_for = |path: &Path| {
            let until = std::time::Instant::now() + Duration::from_secs(10);
            while !path.exists() {
                assert!(
                    std::time::Instant::now() < until,
                    "child handshake timed out: {path:?}"
                );
                std::thread::sleep(Duration::from_millis(10));
            }
        };
        wait_for(&root.join("ready0"));
        wait_for(&root.join("ready1"));
        std::fs::write(root.join("go"), b"go").unwrap();
        wait_for(&root.join("result0"));
        wait_for(&root.join("result1"));
        let results = [
            std::fs::read_to_string(root.join("result0")).unwrap(),
            std::fs::read_to_string(root.join("result1")).unwrap(),
        ];
        assert_eq!(
            results.iter().filter(|s| s.as_str() == "owner").count(),
            1,
            "{results:?}"
        );
        assert_eq!(
            results.iter().filter(|s| s.as_str() == "blocked").count(),
            1,
            "{results:?}"
        );
        std::fs::write(root.join("release"), b"release").unwrap();
        for child in &mut children {
            assert!(child.wait().unwrap().success());
        }
        run_child_probe(alias.to_str().unwrap(), "profile-available");
        eprintln!("S2 simultaneous fixture retained: {root:?}; outcomes: {results:?}");
    }

    #[test]
    fn cross_process_probe() {
        let Ok(name) = env::var(TEST_MUTEX_ENV) else {
            return;
        };

        let expectation = env::var(TEST_EXPECTATION_ENV).unwrap();
        if expectation.starts_with("profile-") {
            if expectation == "profile-race" {
                let root = std::path::PathBuf::from(env::var_os("WSL_S2_RACE_ROOT").unwrap());
                let index = env::var("WSL_S2_RACE_INDEX").unwrap();
                std::fs::write(root.join(format!("ready{index}")), b"ready").unwrap();
                let wait_for = |path: &Path| {
                    let until = std::time::Instant::now() + Duration::from_secs(10);
                    while !path.exists() {
                        assert!(std::time::Instant::now() < until, "race timed out");
                        std::thread::sleep(Duration::from_millis(10));
                    }
                };
                wait_for(&root.join("go"));
                let result = CandidateInstanceGuard::acquire(
                    ProfileIdentity::open(Path::new(&name)).unwrap(),
                    false,
                );
                let outcome = match &result {
                    Ok(_) => "owner",
                    Err(CandidateInstanceError::AlreadyRunning) => "blocked",
                    Err(e) => panic!("{e}"),
                };
                std::fs::write(root.join(format!("result{index}")), outcome).unwrap();
                wait_for(&root.join("release"));
                drop(result);
                return;
            }
            let identity = ProfileIdentity::open(Path::new(&name)).unwrap();
            eprintln!("S2 child {} {expectation}: {identity:?}", process::id());
            let result =
                CandidateInstanceGuard::acquire(identity, expectation == "profile-blocked-reopen");
            if expectation == "profile-available" {
                assert!(result.is_ok());
            } else {
                assert!(matches!(
                    result,
                    Err(CandidateInstanceError::AlreadyRunning)
                ));
            }
            return;
        }

        match env::var(TEST_EXPECTATION_ENV).as_deref() {
            Ok("blocked") => assert!(matches!(
                CandidateInstanceGuard::acquire_named(&name),
                Err(CandidateInstanceError::AlreadyRunning)
            )),
            Ok("available") => {
                CandidateInstanceGuard::acquire_named(&name).expect("released cross-process guard");
            }
            expectation => panic!("unexpected child probe expectation: {expectation:?}"),
        }
    }

    fn run_child_probe(name: &str, expectation: &str) {
        let status = child_probe_command(name, expectation)
            .status()
            .expect("start cross-process guard probe");
        assert!(status.success(), "cross-process guard probe failed");
    }

    fn child_probe_command(name: &str, expectation: &str) -> Command {
        let mut command = Command::new(env::current_exe().expect("current test executable"));
        command
            .args([
                "--exact",
                "candidate_instance::tests::cross_process_probe",
                "--nocapture",
            ])
            .env(TEST_MUTEX_ENV, name)
            .env(TEST_EXPECTATION_ENV, expectation);
        command
    }
}

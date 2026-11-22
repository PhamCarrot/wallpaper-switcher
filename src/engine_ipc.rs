use std::collections::{HashMap, HashSet};
use std::ffi::OsString;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{mpsc, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    CloseHandle, GetLastError, ERROR_NO_MORE_FILES, HANDLE, HWND, INVALID_HANDLE_VALUE,
};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, GetWindowThreadProcessId};

#[path = "engine_ipc/query_console.rs"]
mod query_console;
pub(crate) use query_console::dispatch_readback_helper;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EngineAvailability {
    ControlAvailable,
    ProcessOnly,
    Unavailable,
    Unreadable,
}

pub fn is_running() -> bool {
    !control_window().is_null()
}

fn control_window() -> HWND {
    unsafe { FindWindowW(windows_sys::w!("WPEEventWindow"), std::ptr::null()) }
}

pub(crate) fn control_window_identity() -> Result<(usize, u32), String> {
    let window = control_window();
    let mut process = 0;
    if window.is_null()
        || unsafe { GetWindowThreadProcessId(window, &mut process) } == 0
        || process == 0
    {
        return Err("Wallpaper Engine control window has no readable owner".into());
    }
    Ok((window as usize, process))
}

#[derive(Debug, PartialEq, Eq)]
struct ControlEndpoint {
    identity: (usize, u32),
    executable: PathBuf,
}

impl ControlEndpoint {
    fn config_path(&self) -> PathBuf {
        self.executable.with_file_name("config.json")
    }

    fn command(&self, arguments: &[OsString]) -> Command {
        let mut command = Command::new(&self.executable);
        command.arg("-control").args(arguments);
        command
    }
}

fn trace_endpoint(endpoint: &ControlEndpoint) -> String {
    format!(
        "hwnd=0x{:x};owner_pid={};executable={}",
        endpoint.identity.0,
        endpoint.identity.1,
        endpoint.executable.display()
    )
}

fn trace_arguments(arguments: &[OsString]) -> String {
    let arguments = arguments
        .iter()
        .map(|argument| argument.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    format!("{arguments:?}")
}

fn trace_target(target: &EngineTarget) -> String {
    format!(
        "location={};device={}",
        target.location,
        target.device_id.as_deref().unwrap_or("<unbound>")
    )
}

fn trace_observation(observation: &EngineObservation) -> String {
    format!(
        "target={};source={:?};state={:?};diagnostic={}",
        trace_target(&observation.target),
        observation.source,
        observation.state,
        observation.diagnostic.as_deref().unwrap_or("<none>")
    )
}

fn resolve_control_endpoint(
    mut identity: impl FnMut() -> Result<(usize, u32), String>,
    image: impl FnOnce(u32) -> Result<Option<String>, String>,
    known_install: impl FnOnce(&Path) -> bool,
) -> Result<ControlEndpoint, String> {
    let owner = identity()?;
    let executable = image(owner.1)?
        .map(PathBuf::from)
        .ok_or("Wallpaper Engine main process disappeared")?;
    let main_process = executable
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.eq_ignore_ascii_case("wallpaper32.exe")
                || name.eq_ignore_ascii_case("wallpaper64.exe")
        });
    if !main_process || !executable.is_absolute() || !known_install(&executable) {
        return Err(
            "Wallpaper Engine control window owner is not a recognized installed main executable"
                .into(),
        );
    }
    if identity()? != owner {
        return Err("Wallpaper Engine control window owner changed during discovery".into());
    }
    Ok(ControlEndpoint {
        identity: owner,
        executable,
    })
}

fn active_control_endpoint() -> Result<ControlEndpoint, String> {
    let result = resolve_control_endpoint(
        control_window_identity,
        process_image_path,
        crate::scanner::known_wallpaper_engine_executable,
    );
    crate::apply_trace::event_lazy("provider-endpoint", || match &result {
        Ok(endpoint) => format!("status=resolved {}", trace_endpoint(endpoint)),
        Err(error) => format!("status=unavailable error={error}"),
    });
    result
}

fn process_image_path(process_id: u32) -> Result<Option<String>, String> {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return Err(format!(
            "could not inspect Wallpaper Engine candidate process {process_id}"
        ));
    }
    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    let success = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &mut length,
        )
    } != 0;
    unsafe {
        CloseHandle(process);
    }
    if success {
        Ok(Some(String::from_utf16_lossy(&buffer[..length as usize])))
    } else {
        Err(format!(
            "could not read Wallpaper Engine candidate process {process_id} path"
        ))
    }
}

fn engine_process_is_running() -> Result<bool, String> {
    // Snapshot executable names without opening unrelated protected processes.
    // A failure to inspect an identified Engine candidate still blocks mutation.
    engine_processes_from_entries(process_entries()?, process_image_path)
}

fn process_entries() -> Result<Vec<(u32, String)>, String> {
    struct ProcessSnapshot(HANDLE);
    impl Drop for ProcessSnapshot {
        fn drop(&mut self) {
            unsafe { CloseHandle(self.0) };
        }
    }

    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(format!(
            "Wallpaper Engine process snapshot failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let snapshot = ProcessSnapshot(snapshot);
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    if unsafe { Process32FirstW(snapshot.0, &mut entry) } == 0 {
        return Err(format!(
            "Wallpaper Engine process snapshot has no readable first entry: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut entries = Vec::new();
    loop {
        let length = entry
            .szExeFile
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(entry.szExeFile.len());
        entries.push((
            entry.th32ProcessID,
            String::from_utf16_lossy(&entry.szExeFile[..length]),
        ));
        if unsafe { Process32NextW(snapshot.0, &mut entry) } == 0 {
            let error = unsafe { GetLastError() };
            if error == ERROR_NO_MORE_FILES {
                break;
            }
            return Err(format!(
                "Wallpaper Engine process snapshot iteration failed: {}",
                std::io::Error::from_raw_os_error(error as i32)
            ));
        }
    }
    Ok(entries)
}

fn is_engine_process_name(name: &str) -> bool {
    [
        "wallpaper64.exe",
        "wallpaper32.exe",
        "webwallpaper64.exe",
        "webwallpaper32.exe",
    ]
    .iter()
    .any(|candidate| name.eq_ignore_ascii_case(candidate))
}

fn engine_processes_from_entries<F>(
    entries: impl IntoIterator<Item = (u32, String)>,
    mut image_path: F,
) -> Result<bool, String>
where
    F: FnMut(u32) -> Result<Option<String>, String>,
{
    for (process_id, snapshot_name) in entries {
        if !is_engine_process_name(&snapshot_name) {
            continue;
        }
        let path = image_path(process_id)?.ok_or_else(|| {
            format!("Wallpaper Engine candidate process {process_id} disappeared during inspection")
        })?;
        let name = std::path::Path::new(&path)
            .file_name()
            .and_then(|name| name.to_str())
            .map(str::to_ascii_lowercase)
            .ok_or_else(|| "Wallpaper Engine process path was not valid UTF-16 text".to_string())?;
        if name.eq_ignore_ascii_case(&snapshot_name) && is_engine_process_name(&name) {
            return Ok(true);
        }
        return Err(format!(
            "Wallpaper Engine candidate process {process_id} changed executable identity during inspection"
        ));
    }
    Ok(false)
}

fn availability_from_observation(
    control_window: bool,
    process: Result<bool, String>,
) -> EngineAvailability {
    if control_window {
        EngineAvailability::ControlAvailable
    } else {
        match process {
            Ok(true) => EngineAvailability::ProcessOnly,
            Ok(false) => EngineAvailability::Unavailable,
            Err(_) => EngineAvailability::Unreadable,
        }
    }
}

pub(crate) fn availability() -> EngineAvailability {
    availability_from_observation(is_running(), engine_process_is_running())
}

#[derive(Debug, Clone, Default)]
pub struct EngineMonitorMap {
    locations: HashMap<String, String>,
}

impl EngineMonitorMap {
    pub fn location_for_device(&self, device_id: &str) -> Option<String> {
        self.locations.get(&normalize_device_id(device_id)).cloned()
    }

    pub fn monitor_index_for_device(&self, device_id: &str) -> Option<usize> {
        self.location_for_device(device_id).and_then(|location| {
            location
                .strip_prefix("Monitor")
                .and_then(|index| index.parse().ok())
        })
    }

    pub fn locations(&self) -> Vec<String> {
        let mut seen = HashSet::new();
        let mut locations = Vec::new();
        for location in self.locations.values() {
            if seen.insert(location.clone()) {
                locations.push(location.clone());
            }
        }
        locations.sort();
        locations
    }
}

fn normalize_device_id(device_id: &str) -> String {
    device_id.replace('\\', "/").to_ascii_lowercase()
}

fn collect_monitor_maps(
    value: &serde_json::Value,
    output: &mut EngineMonitorMap,
) -> Result<(), String> {
    let general = active_general(value)?;
    let monitors = general
        .get("user")
        .and_then(|user| user.get("monitormap"))
        .and_then(serde_json::Value::as_object)
        .ok_or("Missing or malformed active general.user.monitormap")?;
    let mut seen_locations = HashSet::new();
    for (device_id, details) in monitors {
        if device_id.trim().is_empty() {
            return Err("Empty Engine display identity".into());
        }
        let index = details
            .get("location")
            .and_then(serde_json::Value::as_u64)
            .ok_or("Unsupported Engine monitor location; expected numeric index")?;
        let location = format!("Monitor{index}");
        if !seen_locations.insert(location.clone())
            || output
                .locations
                .insert(normalize_device_id(device_id), location)
                .is_some()
        {
            return Err("Ambiguous Engine display/location mapping".into());
        }
    }
    Ok(())
}

pub fn parse_monitor_map(json: &str) -> Result<EngineMonitorMap, String> {
    let value: UniqueJson = serde_json::from_str(json)
        .map_err(|error| format!("Wallpaper Engine config is invalid JSON: {error}"))?;
    let mut result = EngineMonitorMap::default();
    collect_monitor_maps(&value.0, &mut result)?;
    Ok(result)
}

pub fn load_monitor_map() -> Result<EngineMonitorMap, String> {
    let config = if is_running() {
        active_control_endpoint()?.config_path()
    } else {
        crate::scanner::wallpaper_engine_config_path()
            .ok_or_else(|| "Wallpaper Engine config.json was not found".to_string())?
    };
    load_monitor_map_at(&config)
}

fn load_monitor_map_at(config: &Path) -> Result<EngineMonitorMap, String> {
    let json = std::fs::read_to_string(config)
        .map_err(|error| format!("Could not read {}: {error}", config.display()))?;
    parse_monitor_map(&json)
}

const CONTROL_OUTPUT_LIMIT: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct CapturedStream {
    bytes: Vec<u8>,
    discarded_bytes: usize,
}

#[derive(Debug)]
struct ChildRunOutput {
    process_id: u32,
    status: ExitStatus,
    stdout: CapturedStream,
    stderr: CapturedStream,
}

#[derive(Debug)]
struct ChildRunError {
    message: String,
    process_id: Option<u32>,
    child_stopped: Option<bool>,
    unsettled_child: Option<Child>,
    stdout: Option<CapturedStream>,
    stderr: Option<CapturedStream>,
}

impl ChildRunError {
    fn before_start(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            process_id: None,
            child_stopped: Some(true),
            unsettled_child: None,
            stdout: None,
            stderr: None,
        }
    }

    fn after_start(
        message: impl Into<String>,
        process_id: u32,
        child_stopped: Option<bool>,
    ) -> Self {
        Self {
            message: message.into(),
            process_id: Some(process_id),
            child_stopped,
            unsettled_child: None,
            stdout: None,
            stderr: None,
        }
    }

    fn after_start_with_child(
        message: impl Into<String>,
        process_id: u32,
        child_stopped: Option<bool>,
        child: Option<Child>,
    ) -> Self {
        Self {
            message: message.into(),
            process_id: Some(process_id),
            child_stopped,
            unsettled_child: child,
            stdout: None,
            stderr: None,
        }
    }
}

enum PendingProcess {
    Child(Child),
    ReadbackJob(query_console::QueryJob),
}

struct UnsettledChild {
    process_id: u32,
    child: PendingProcess,
}

static UNSETTLED_CHILDREN: OnceLock<Mutex<Vec<UnsettledChild>>> = OnceLock::new();

fn unsettled_children() -> &'static Mutex<Vec<UnsettledChild>> {
    UNSETTLED_CHILDREN.get_or_init(|| Mutex::new(Vec::new()))
}

fn register_unsettled_child(child: UnsettledChild) {
    if let Ok(mut children) = unsettled_children().lock() {
        children.push(child);
    }
}

fn observe_unsettled_child(entry: &mut UnsettledChild) -> Result<bool, String> {
    let child = match &mut entry.child {
        PendingProcess::Child(child) => child,
        PendingProcess::ReadbackJob(job) => {
            return job
                .settle(Instant::now())
                .map_err(|error| error.to_string())
        }
    };
    match child.try_wait() {
        Ok(Some(_)) => Ok(true),
        Ok(None) => Ok(false),
        Err(error) => Err(error.to_string()),
    }
}

/// Mutating Engine commands are serialized behind any child whose exit could
/// not be observed. This keeps a later command from racing an active or
/// unknown provider operation while retaining ownership of the exact child.
fn ensure_no_unsettled_children() -> Result<(), String> {
    let mut children = unsettled_children()
        .lock()
        .map_err(|_| "provider-state-unknown: Engine child gate was poisoned".to_string())?;
    let mut unresolved = Vec::new();
    let mut errors = Vec::new();
    for mut entry in children.drain(..) {
        match observe_unsettled_child(&mut entry) {
            Ok(true) => {}
            Ok(false) => {
                let process_id = entry.process_id;
                errors.push(format!(
                    "provider-state-unknown: Wallpaper Engine child {process_id} is still unsettled"
                ));
                unresolved.push(entry);
            }
            Err(error) => {
                let process_id = entry.process_id;
                errors.push(format!(
                    "provider-state-unknown: could not observe Wallpaper Engine child {process_id}: {error}"
                ));
                unresolved.push(entry);
            }
        }
    }
    *children = unresolved;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

#[cfg(test)]
fn settle_unsettled_child_for_test(process_id: u32) -> Result<bool, String> {
    let mut children = unsettled_children()
        .lock()
        .map_err(|_| "provider-state-unknown: Engine child gate was poisoned".to_string())?;
    let Some(index) = children
        .iter()
        .position(|entry| entry.process_id == process_id)
    else {
        return Ok(true);
    };

    match observe_unsettled_child(&mut children[index])? {
        true => {
            children.swap_remove(index);
            Ok(true)
        }
        false => Ok(false),
    }
}

#[cfg(test)]
fn stop_unsettled_child_for_test(process_id: u32, deadline: Instant) -> Result<bool, String> {
    let mut children = unsettled_children()
        .lock()
        .map_err(|_| "provider-state-unknown: Engine child gate was poisoned".to_string())?;
    let Some(index) = children
        .iter()
        .position(|entry| entry.process_id == process_id)
    else {
        return Ok(true);
    };

    let stopped = match &mut children[index].child {
        PendingProcess::Child(child) => stop_owned_child(child, deadline) == Some(true),
        PendingProcess::ReadbackJob(job) => {
            job.settle(deadline).map_err(|error| error.to_string())?
        }
    };
    if stopped {
        children.swap_remove(index);
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Prevents any new coordinator mutation while a previously retained Engine
/// child is still active or cannot be observed to have settled. Provider
/// operation recovery is owned by the surrounding WallpaperCoordinator.
pub(crate) fn ensure_mutation_allowed() -> Result<(), String> {
    ensure_no_unsettled_children()
}

fn drain_stream<R: Read>(mut stream: R) -> Result<CapturedStream, String> {
    let mut bytes = Vec::with_capacity(CONTROL_OUTPUT_LIMIT.min(64 * 1024));
    let mut discarded_bytes = 0usize;
    let mut buffer = [0u8; 16 * 1024];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => {
                let remaining = CONTROL_OUTPUT_LIMIT.saturating_sub(bytes.len());
                let retained = remaining.min(read);
                bytes.extend_from_slice(&buffer[..retained]);
                discarded_bytes = discarded_bytes.saturating_add(read - retained);
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("could not drain child output: {error}")),
        }
    }
    Ok(CapturedStream {
        bytes,
        discarded_bytes,
    })
}

fn spawn_stream_reader<R: Read + Send + 'static>(
    stream: R,
    sender: mpsc::Sender<Result<CapturedStream, String>>,
    name: &'static str,
) -> Result<(), String> {
    thread::Builder::new()
        .name(name.to_string())
        .spawn(move || {
            let _ = sender.send(drain_stream(stream));
        })
        .map(|_| ())
        .map_err(|error| format!("could not start child output reader: {error}"))
}

fn stop_owned_child(child: &mut Child, deadline: Instant) -> Option<bool> {
    let _ = child.kill();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Some(true),
            Ok(None) if Instant::now() < deadline => {
                thread::sleep(
                    Duration::from_millis(10)
                        .min(deadline.saturating_duration_since(Instant::now())),
                );
            }
            Ok(None) => return Some(false),
            Err(_) => return None,
        }
    }
}

fn error_after_stop(
    message: impl Into<String>,
    process_id: u32,
    child: Child,
    stopped: Option<bool>,
) -> ChildRunError {
    error_after_stop_with_output(message, process_id, child, stopped, None, None)
}

fn error_after_stop_with_output(
    message: impl Into<String>,
    process_id: u32,
    child: Child,
    stopped: Option<bool>,
    stdout: Option<CapturedStream>,
    stderr: Option<CapturedStream>,
) -> ChildRunError {
    let mut error = ChildRunError::after_start_with_child(
        message,
        process_id,
        stopped,
        (stopped != Some(true)).then_some(child),
    );
    error.stdout = stdout;
    error.stderr = stderr;
    error
}

fn capture_stream_after_stop(
    receiver: &mpsc::Receiver<Result<CapturedStream, String>>,
    deadline: Instant,
    stream_name: &str,
) -> Result<CapturedStream, String> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    match receiver.recv_timeout(remaining) {
        Ok(Ok(stream)) => Ok(stream),
        Ok(Err(error)) => Err(format!("{stream_name} output reader failed: {error}")),
        Err(mpsc::RecvTimeoutError::Timeout) => Err(format!(
            "{stream_name} output did not close before the cleanup deadline"
        )),
        Err(mpsc::RecvTimeoutError::Disconnected) => Err(format!(
            "{stream_name} output reader ended without a result"
        )),
    }
}

fn capture_stopped_output(
    stdout_receiver: &mpsc::Receiver<Result<CapturedStream, String>>,
    stderr_receiver: &mpsc::Receiver<Result<CapturedStream, String>>,
    deadline: Instant,
) -> (
    Option<CapturedStream>,
    Option<CapturedStream>,
    Option<String>,
) {
    let stdout = capture_stream_after_stop(stdout_receiver, deadline, "stdout");
    let stderr = capture_stream_after_stop(stderr_receiver, deadline, "stderr");
    let mut errors = Vec::new();
    let stdout = match stdout {
        Ok(stream) => Some(stream),
        Err(error) => {
            errors.push(error);
            None
        }
    };
    let stderr = match stderr {
        Ok(stream) => Some(stream),
        Err(error) => {
            errors.push(error);
            None
        }
    };
    let error = (!errors.is_empty()).then(|| errors.join("; "));
    (stdout, stderr, error)
}

fn run_command_bounded(
    command: &mut Command,
    timeout: Duration,
    description: &str,
) -> Result<ChildRunOutput, ChildRunError> {
    run_command_bounded_with_dispatch(command, timeout, description, || {})
}

fn run_command_bounded_with_dispatch(
    command: &mut Command,
    timeout: Duration,
    description: &str,
    on_dispatch: impl FnOnce(),
) -> Result<ChildRunOutput, ChildRunError> {
    run_command_after_spawn(command, timeout, description, |_| {
        on_dispatch();
        Ok(())
    })
}

fn run_command_after_spawn(
    command: &mut Command,
    timeout: Duration,
    description: &str,
    on_spawn: impl FnOnce(&mut Child) -> Result<(), String>,
) -> Result<ChildRunOutput, ChildRunError> {
    let started = Instant::now();
    let deadline = started + timeout;
    let cleanup_reserve = Duration::from_millis(750).min(timeout / 4);
    let run_deadline = deadline.checked_sub(cleanup_reserve).unwrap_or(started);

    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| {
            ChildRunError::before_start(format!("could not start {description}: {error}"))
        })?;
    let process_id = child.id();
    if let Err(message) = on_spawn(&mut child) {
        let stopped = stop_owned_child(&mut child, deadline);
        return Err(error_after_stop(message, process_id, child, stopped));
    }

    let Some(stdout) = child.stdout.take() else {
        let stopped = stop_owned_child(&mut child, deadline);
        return Err(error_after_stop(
            format!("{description} stdout pipe was unavailable"),
            process_id,
            child,
            stopped,
        ));
    };
    let Some(stderr) = child.stderr.take() else {
        let stopped = stop_owned_child(&mut child, deadline);
        return Err(error_after_stop(
            format!("{description} stderr pipe was unavailable"),
            process_id,
            child,
            stopped,
        ));
    };

    let (stdout_sender, stdout_receiver) = mpsc::channel();
    if let Err(error) = spawn_stream_reader(stdout, stdout_sender, "engine-control-stdout") {
        let stopped = stop_owned_child(&mut child, deadline);
        return Err(error_after_stop(
            format!("{description}: {error}"),
            process_id,
            child,
            stopped,
        ));
    }
    let (stderr_sender, stderr_receiver) = mpsc::channel();
    if let Err(error) = spawn_stream_reader(stderr, stderr_sender, "engine-control-stderr") {
        let stopped = stop_owned_child(&mut child, deadline);
        return Err(error_after_stop(
            format!("{description}: {error}"),
            process_id,
            child,
            stopped,
        ));
    }

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if Instant::now() < run_deadline => {
                thread::sleep(
                    Duration::from_millis(10)
                        .min(run_deadline.saturating_duration_since(Instant::now())),
                );
            }
            Ok(None) => {
                break Err(format!(
                    "{description} timed out after {} ms",
                    timeout.as_millis()
                ))
            }
            Err(error) => break Err(format!("could not observe {description} exit: {error}")),
        }
    };

    let status = match status {
        Ok(status) => status,
        Err(message) => {
            let stopped = stop_owned_child(&mut child, deadline);
            let (stdout, stderr, capture_error) = if stopped == Some(true) {
                capture_stopped_output(&stdout_receiver, &stderr_receiver, deadline)
            } else {
                (None, None, None)
            };
            let message = match capture_error {
                Some(capture_error) => format!("{message}; {capture_error}"),
                None => message,
            };
            return Err(error_after_stop_with_output(
                message, process_id, child, stopped, stdout, stderr,
            ));
        }
    };

    let mut captured = Vec::with_capacity(2);
    for receiver in [stdout_receiver, stderr_receiver] {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining) {
            Ok(Ok(stream)) => captured.push(stream),
            Ok(Err(error)) => {
                return Err(ChildRunError::after_start(
                    format!("{description} completed but {error}"),
                    process_id,
                    Some(true),
                ));
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                return Err(ChildRunError::after_start(
                    format!("{description} output pipes did not close before the deadline"),
                    process_id,
                    Some(true),
                ));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return Err(ChildRunError::after_start(
                    format!("{description} output reader ended without a result"),
                    process_id,
                    Some(true),
                ));
            }
        }
    }

    Ok(ChildRunOutput {
        process_id,
        status,
        stdout: captured.remove(0),
        stderr: captured.remove(0),
    })
}

fn output_text(stream: &CapturedStream) -> String {
    let mut text = String::from_utf8_lossy(&stream.bytes).trim().to_string();
    if stream.discarded_bytes > 0 {
        text.push_str(&format!(" [truncated {} bytes]", stream.discarded_bytes));
    }
    text
}

fn child_run_error_text(error: ChildRunError) -> String {
    let mut text = format!(
        "{} (child_pid={:?}, child_stopped={:?})",
        error.message, error.process_id, error.child_stopped
    );
    if error.stdout.is_some() || error.stderr.is_some() {
        let stdout = error
            .stdout
            .as_ref()
            .map(output_text)
            .unwrap_or_else(|| "<unavailable>".to_string());
        let stderr = error
            .stderr
            .as_ref()
            .map(output_text)
            .unwrap_or_else(|| "<unavailable>".to_string());
        text.push_str(&format!("; stdout: {stdout}; stderr: {stderr}"));
    }
    text
}

fn run_control_result(
    endpoint: &ControlEndpoint,
    arguments: &[OsString],
    timeout: Duration,
    on_dispatch: impl FnOnce(),
) -> Result<String, ChildRunError> {
    run_control_output_with(
        endpoint,
        arguments,
        timeout,
        on_dispatch,
        active_control_endpoint,
        || endpoint.command(arguments),
    )
    .map(|result| output_text(&result.stdout))
}

// Inject only endpoint discovery and command construction. The integration
// harness still exercises the production bounded runner and dispatch boundary.
fn run_control_output_with(
    endpoint: &ControlEndpoint,
    arguments: &[OsString],
    timeout: Duration,
    on_dispatch: impl FnOnce(),
    mut current_endpoint: impl FnMut() -> Result<ControlEndpoint, String>,
    command: impl FnOnce() -> Command,
) -> Result<ChildRunOutput, ChildRunError> {
    let query = trace_arguments(arguments);
    let started = Instant::now();
    crate::apply_trace::event_lazy("provider-command-start", || {
        format!(
            "{} timeout_ms={} args={query}",
            trace_endpoint(endpoint),
            timeout.as_millis()
        )
    });
    if current_endpoint().as_ref() != Ok(endpoint) {
        let error = ChildRunError::before_start(
            "Wallpaper Engine control endpoint changed before dispatch",
        );
        crate::apply_trace::event_lazy("provider-command-failure", || {
            format!(
                "phase=before-dispatch elapsed_ms={} args={query} error={}",
                started.elapsed().as_millis(),
                error.message
            )
        });
        return Err(error);
    }
    let mut command = command();
    let output = match run_command_bounded_with_dispatch(
        &mut command,
        timeout,
        "Wallpaper Engine control command",
        || {
            crate::apply_trace::event_lazy("provider-command-dispatched", || {
                format!("parent_pid={} args={query}", std::process::id())
            });
            on_dispatch();
        },
    ) {
        Ok(output) => output,
        Err(error) => {
            crate::apply_trace::event_lazy("provider-command-failure", || {
                format!(
                    "phase=execution elapsed_ms={} args={query} child_pid={:?} child_stopped={:?} error={}",
                    started.elapsed().as_millis(),
                    error.process_id,
                    error.child_stopped,
                    error.message
                )
            });
            return Err(error);
        }
    };
    if !output.status.success() {
        let stderr = output_text(&output.stderr);
        let stdout = output_text(&output.stdout);
        let error = ChildRunError::after_start(
            format!(
                "Wallpaper Engine control command exited with {}; stdout: {}; stderr: {}",
                output.status,
                if stdout.is_empty() {
                    "<empty>"
                } else {
                    &stdout
                },
                if stderr.is_empty() {
                    "<empty>"
                } else {
                    &stderr
                },
            ),
            output.process_id,
            Some(true),
        );
        crate::apply_trace::event_lazy("provider-command-failure", || {
            format!(
                "phase=exit elapsed_ms={} args={query} child_pid={} status={} stdout_bytes={} stderr_bytes={}",
                started.elapsed().as_millis(),
                output.process_id,
                output.status,
                output.stdout.bytes.len(),
                output.stderr.bytes.len()
            )
        });
        return Err(error);
    }
    if current_endpoint().as_ref() != Ok(endpoint) {
        let error = ChildRunError::after_start(
            "Wallpaper Engine control endpoint changed during the command",
            output.process_id,
            Some(true),
        );
        crate::apply_trace::event_lazy("provider-command-failure", || {
            format!(
                "phase=endpoint-drift elapsed_ms={} args={query} child_pid={} error={}",
                started.elapsed().as_millis(),
                output.process_id,
                error.message
            )
        });
        return Err(error);
    }
    crate::apply_trace::event_lazy("provider-command-complete", || {
        let stderr_prefix: String = String::from_utf8_lossy(&output.stderr.bytes)
            .chars()
            .take(1024)
            .collect();
        format!(
            "elapsed_ms={} args={query} child_pid={} status={} stdout_bytes={} stdout_empty={} stderr_bytes={} stderr_discarded_bytes={} stderr_prefix={stderr_prefix:?}",
            started.elapsed().as_millis(),
            output.process_id,
            output.status,
            output.stdout.bytes.len(),
            output.stdout.bytes.is_empty(),
            output.stderr.bytes.len(),
            output.stderr.discarded_bytes
        )
    });
    Ok(output)
}

fn run_control(
    endpoint: &ControlEndpoint,
    arguments: &[OsString],
    timeout: Duration,
) -> Result<query_console::QueryPath, String> {
    ensure_no_unsettled_children()?;
    query_console::readback(endpoint, arguments, timeout).map_err(|mut error| {
        let unsettled = if let Some(child) = error.unsettled_child.take() {
            register_unsettled_child(UnsettledChild {
                process_id: error.process_id.unwrap_or_default(),
                child: PendingProcess::Child(child),
            });
            true
        } else {
            false
        };
        if unsettled {
            format!("provider-state-unknown: {}", child_run_error_text(error))
        } else {
            child_run_error_text(error)
        }
    })
}

fn run_control_mutation_typed(
    arguments: &[OsString],
    timeout: Duration,
) -> Result<String, EngineControlFailure> {
    ensure_no_unsettled_children().map_err(EngineControlFailure::DispatchedOrUnknown)?;
    let endpoint =
        active_control_endpoint().map_err(EngineControlFailure::PreflightObservationFailed)?;
    run_control_result(&endpoint, arguments, timeout, || {}).map_err(control_failure_from_run_error)
}

fn control_failure_from_run_error(mut error: ChildRunError) -> EngineControlFailure {
    let no_dispatch = error.process_id.is_none()
        && error.child_stopped == Some(true)
        && error.unsettled_child.is_none();
    if let Some(child) = error.unsettled_child.take() {
        register_unsettled_child(UnsettledChild {
            process_id: error.process_id.unwrap_or_default(),
            child: PendingProcess::Child(child),
        });
    }
    let message = child_run_error_text(error);
    if no_dispatch {
        EngineControlFailure::BeforeDispatch(message)
    } else {
        EngineControlFailure::DispatchedOrUnknown(format!("provider-state-unknown: {message}"))
    }
}

fn run_control_mutation_for_project(
    endpoint: &ControlEndpoint,
    arguments: &[OsString],
    timeout: Duration,
    on_dispatch: impl FnOnce(),
) -> Result<String, EngineControlFailure> {
    ensure_no_unsettled_children().map_err(EngineControlFailure::DispatchedOrUnknown)?;
    run_control_result(endpoint, arguments, timeout, on_dispatch).map_err(|mut error| {
        if let Some(child) = error.unsettled_child.take() {
            register_unsettled_child(UnsettledChild {
                process_id: error.process_id.unwrap_or_default(),
                child: PendingProcess::Child(child),
            });
        }
        control_failure_from_run_error(error)
    })
}

fn require_running() -> Result<(), String> {
    match availability() {
        EngineAvailability::ControlAvailable => Ok(()),
        EngineAvailability::ProcessOnly => Err(
            "provider-state-unknown: Wallpaper Engine process is running but its control window is unavailable"
                .to_string(),
        ),
        EngineAvailability::Unavailable => Err("Wallpaper Engine is not running".to_string()),
        EngineAvailability::Unreadable => Err(
            "provider-state-unknown: Wallpaper Engine process availability is unreadable"
                .to_string(),
        ),
    }
}

#[derive(Debug)]
pub enum EngineControlFailure {
    PreflightObservationFailed(String),
    BeforeDispatch(String),
    DispatchedOrUnknown(String),
}

impl EngineControlFailure {
    pub(crate) fn into_message(self) -> String {
        match self {
            Self::PreflightObservationFailed(message)
            | Self::BeforeDispatch(message)
            | Self::DispatchedOrUnknown(message) => message,
        }
    }
}

pub fn set_paused(paused: bool) -> Result<(), EngineControlFailure> {
    require_running().map_err(EngineControlFailure::PreflightObservationFailed)?;
    run_control_mutation_typed(
        &[OsString::from(if paused { "pause" } else { "play" })],
        Duration::from_secs(5),
    )
    .map(|_| ())
}

pub fn set_muted(muted: bool) -> Result<(), EngineControlFailure> {
    require_running().map_err(EngineControlFailure::PreflightObservationFailed)?;
    run_control_mutation_typed(
        &[OsString::from(if muted { "mute" } else { "unmute" })],
        Duration::from_secs(5),
    )
    .map(|_| ())
}

pub fn apply_properties(
    properties: &serde_json::Map<String, serde_json::Value>,
    location: &str,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    require_running()?;
    if properties.is_empty() {
        return Err("No Wallpaper Engine properties were supplied".to_string());
    }
    let endpoint =
        active_control_endpoint().map_err(|error| format!("provider-state-unknown: {error}"))?;
    let json = serde_json::to_string(properties)
        .map_err(|error| format!("Could not encode Wallpaper Engine properties: {error}"))?;
    let payload = encode_property_payload(&json)?;
    let mut command = endpoint.command(&control_args("applyProperties", Some(location)));
    command.raw_arg(format!("-properties {payload}"));
    ensure_no_unsettled_children()?;
    if active_control_endpoint().as_ref() != Ok(&endpoint) {
        return Err("provider-state-unknown: Wallpaper Engine control endpoint changed before property dispatch".into());
    }
    let output = match run_command_bounded(
        &mut command,
        Duration::from_secs(5),
        "Wallpaper Engine property command",
    ) {
        Ok(output) => output,
        Err(mut error) => {
            if let Some(child) = error.unsettled_child.take() {
                register_unsettled_child(UnsettledChild {
                    process_id: error.process_id.unwrap_or_default(),
                    child: PendingProcess::Child(child),
                });
            }
            return Err(match error.process_id {
                Some(_) => format!("provider-state-unknown: {}", child_run_error_text(error)),
                None => child_run_error_text(error),
            });
        }
    };
    if active_control_endpoint().as_ref() != Ok(&endpoint) {
        return Err("provider-state-unknown: Wallpaper Engine control endpoint changed during the property command".into());
    }
    if output.status.success() {
        return Ok(());
    }
    Err(format!(
        "provider-state-unknown: Wallpaper Engine property command exited with {}; stdout: {}; stderr: {} (child_pid={}, child_stopped=true)",
        output.status,
        output_text(&output.stdout),
        output_text(&output.stderr),
        output.process_id
    ))
}

fn encode_property_payload(json: &str) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| format!("Wallpaper Engine properties are invalid JSON: {error}"))?;
    if !value.is_object() {
        return Err("Wallpaper Engine properties must be a JSON object".to_string());
    }
    Ok(format!("RAW~({})~END", json.replace(')', "\\u0029")))
}

pub fn set_volume(volume: u8) -> Result<(), EngineControlFailure> {
    require_running().map_err(EngineControlFailure::PreflightObservationFailed)?;
    ensure_no_unsettled_children().map_err(EngineControlFailure::DispatchedOrUnknown)?;
    crate::engine_volume::set_wallpaper_engine_volume(volume.min(100)).map_err(|error| {
        EngineControlFailure::DispatchedOrUnknown(format!(
            "provider-state-unknown: Wallpaper Engine volume update did not settle: {error}"
        ))
    })
}

fn ensure_running() -> Result<(), String> {
    match availability() {
        EngineAvailability::ControlAvailable => return Ok(()),
        EngineAvailability::Unreadable => {
            return Err(
                "provider-state-unknown: Wallpaper Engine process availability is unreadable"
                    .to_string(),
            );
        }
        EngineAvailability::ProcessOnly => {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                match availability() {
                    EngineAvailability::ControlAvailable => return Ok(()),
                    EngineAvailability::ProcessOnly if Instant::now() < deadline => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    EngineAvailability::ProcessOnly => {
                        return Err(
                            "provider-state-unknown: Wallpaper Engine is running without a usable control window"
                                .to_string(),
                        );
                    }
                    EngineAvailability::Unreadable => {
                        return Err(
                            "provider-state-unknown: Wallpaper Engine process availability became unreadable"
                                .to_string(),
                        );
                    }
                    EngineAvailability::Unavailable => break,
                }
            }
        }
        EngineAvailability::Unavailable => {}
    }
    let executable = crate::scanner::wallpaper_engine_executable().ok_or_else(|| {
        "Wallpaper Engine executable was not found in a Steam library".to_string()
    })?;
    let mut child = Command::new(&executable)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Could not start {}: {error}", executable.display()))?;
    let process_id = child.id();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if matches!(availability(), EngineAvailability::ControlAvailable) {
            return Ok(());
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return Err(engine_startup_failure(
                    process_id,
                    EngineStartupFailure::Exited(status.to_string()),
                ));
            }
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Ok(None) => {
                register_unsettled_child(UnsettledChild {
                    process_id,
                    child: PendingProcess::Child(child),
                });
                return Err(engine_startup_failure(
                    process_id,
                    EngineStartupFailure::StillRunningAtDeadline,
                ));
            }
            Err(error) => {
                register_unsettled_child(UnsettledChild {
                    process_id,
                    child: PendingProcess::Child(child),
                });
                return Err(engine_startup_failure(
                    process_id,
                    EngineStartupFailure::WaitFailed(error.to_string()),
                ));
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EngineStartupFailure {
    Exited(String),
    StillRunningAtDeadline,
    WaitFailed(String),
}

fn engine_startup_failure(process_id: u32, failure: EngineStartupFailure) -> String {
    match failure {
        EngineStartupFailure::Exited(status) => format!(
            "Wallpaper Engine exited before its control window was ready ({status}; child_pid={process_id}, child_stopped=true)"
        ),
        EngineStartupFailure::StillRunningAtDeadline => format!(
            "provider-state-unknown: Wallpaper Engine startup did not become ready within 10 seconds; the launched process may still be starting and was not terminated (child_pid={process_id}, child_stopped=false)"
        ),
        EngineStartupFailure::WaitFailed(error) => format!(
            "provider-state-unknown: could not observe Wallpaper Engine startup: {error}; the launched process was not terminated (child_pid={process_id}, child_stopped=unknown)"
        ),
    }
}

fn control_args(command: &str, location: Option<&str>) -> Vec<OsString> {
    let mut args = vec![OsString::from(command)];
    if let Some(location) = location {
        args.push(OsString::from("-location"));
        args.push(OsString::from(location));
    }
    args
}

fn control_args_for_monitor(command: &str, monitor: usize) -> Vec<OsString> {
    vec![
        OsString::from(command),
        OsString::from("-monitor"),
        OsString::from(monitor.to_string()),
    ]
}

fn readback_with_monitor_fallback(
    target: &EngineTarget,
    monitor_index: Option<usize>,
    mut query: impl FnMut(&[OsString]) -> Result<query_console::QueryPath, String>,
) -> Result<String, String> {
    use query_console::QueryPath;
    let location_args = control_args("getWallpaper", Some(&target.location));
    crate::apply_trace::event_lazy("provider-readback", || {
        format!(
            "phase=location-query target={} monitor_index={:?} args={}",
            trace_target(target),
            monitor_index,
            trace_arguments(&location_args)
        )
    });
    let rejected_location = match query(&location_args) {
        Ok(QueryPath::Path(output)) => {
            crate::apply_trace::event_lazy("provider-readback", || {
                format!(
                    "phase=location-result target={} empty={} output_bytes={}",
                    trace_target(target),
                    output.is_empty(),
                    output.len()
                )
            });
            if !output.is_empty() {
                return Ok(output);
            }
            None
        }
        Ok(QueryPath::Rejected(reason)) => {
            crate::apply_trace::event_lazy("provider-readback", || {
                format!(
                    "phase=location-rejected target={} reason={reason}",
                    trace_target(target)
                )
            });
            Some(reason)
        }
        Err(error) => {
            crate::apply_trace::event_lazy("provider-readback", || {
                format!(
                    "phase=location-query-failure target={} error={error}",
                    trace_target(target)
                )
            });
            return Err(error);
        }
    };
    let Some(monitor) = monitor_index else {
        return Err(rejected_location
            .unwrap_or_else(|| "Wallpaper Engine numeric monitor identity is unavailable".into()));
    };
    let monitor_args = control_args_for_monitor("getWallpaper", monitor);
    crate::apply_trace::event_lazy("provider-readback", || {
        format!(
            "phase=numeric-fallback target={} monitor={} args={}",
            trace_target(target),
            monitor,
            trace_arguments(&monitor_args)
        )
    });
    let output = match query(&monitor_args) {
        Ok(QueryPath::Path(path)) if path.is_empty() => rejected_location.map_or(Ok(path), Err),
        Ok(QueryPath::Path(path)) => Ok(path),
        Ok(QueryPath::Rejected(reason)) | Err(reason) => Err(reason),
    };
    crate::apply_trace::event_lazy("provider-readback", || {
        format!(
            "phase=numeric-result target={} monitor={} result={} output_bytes={}",
            trace_target(target),
            monitor,
            if output.is_ok() { "ok" } else { "error" },
            output.as_ref().map_or(0, String::len)
        )
    });
    output
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum AbsenceConfirmationCapability {
    #[default]
    Unsupported,
    // Demonstrates the contract only; no production observation supports absence.
    #[cfg(test)]
    SyntheticSupported,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct EngineCapabilities {
    pub absence_confirmation: AbsenceConfirmationCapability,
}

impl EngineCapabilities {
    pub(crate) fn require_absence_confirmation(self) -> Result<(), String> {
        match self.absence_confirmation {
            AbsenceConfirmationCapability::Unsupported => Err(
                "unsupported Engine absence confirmation: no version-correlated absence observation is established; choose an operation that does not require Engine close or native restoration after Engine Apply".into(),
            ),
            #[cfg(test)]
            AbsenceConfirmationCapability::SyntheticSupported => Ok(()),
        }
    }
}

pub(crate) fn capabilities() -> EngineCapabilities {
    EngineCapabilities::default()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineCloseFailure {
    UnsupportedBeforeDispatch(String),
    BeforeDispatch(String),
    DispatchedOrUnknown(String),
}

impl EngineCloseFailure {
    pub(crate) fn description(&self) -> String {
        match self {
            Self::UnsupportedBeforeDispatch(message)
            | Self::BeforeDispatch(message)
            | Self::DispatchedOrUnknown(message) => message.clone(),
        }
    }
}

pub fn close_wallpaper(location: &str) -> Result<(), EngineCloseFailure> {
    close_wallpaper_with(
        capabilities(),
        &EngineTarget::location(location),
        || require_running(),
        || {
            run_control_mutation_typed(
                &control_args("closeWallpaper", Some(location)),
                Duration::from_secs(5),
            )
            .map(|_| ())
        },
        || ensure_no_unsettled_children().is_ok(),
        observe_wallpaper,
        Duration::from_secs(3),
    )
}

pub(crate) fn close_wallpaper_with(
    capabilities: EngineCapabilities,
    target: &EngineTarget,
    prepare: impl FnOnce() -> Result<(), String>,
    dispatch: impl FnOnce() -> Result<(), EngineControlFailure>,
    children_settled: impl Fn() -> bool,
    mut observe: impl FnMut(&EngineTarget) -> EngineObservation,
    timeout: Duration,
) -> Result<(), EngineCloseFailure> {
    capabilities
        .require_absence_confirmation()
        .map_err(EngineCloseFailure::UnsupportedBeforeDispatch)?;
    prepare().map_err(EngineCloseFailure::BeforeDispatch)?;
    if !children_settled() {
        return Err(EngineCloseFailure::BeforeDispatch(
            "An owned child remains unsettled before close".into(),
        ));
    }
    dispatch().map_err(|error| match error {
        EngineControlFailure::PreflightObservationFailed(message)
        | EngineControlFailure::BeforeDispatch(message) => {
            EngineCloseFailure::BeforeDispatch(message)
        }
        EngineControlFailure::DispatchedOrUnknown(message) => {
            EngineCloseFailure::DispatchedOrUnknown(message)
        }
    })?;
    let deadline = Instant::now() + timeout;
    loop {
        let observation = observe(target);
        if children_settled()
            && observation.adequate_for(target).is_ok()
            && observation.state == EngineObservationState::ConfirmedAbsent
        {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(EngineCloseFailure::DispatchedOrUnknown(format!(
                "provider-state-unknown: Engine close was not independently confirmed after child settlement: {}",
                observation.description(),
            )));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EngineTarget {
    pub device_id: Option<String>,
    pub location: String,
}

impl EngineTarget {
    pub(crate) fn bound(device_id: &str, location: &str) -> Self {
        Self {
            device_id: Some(device_id.into()),
            location: location.into(),
        }
    }

    fn location(location: &str) -> Self {
        Self {
            device_id: None,
            location: location.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EngineObservationSource {
    Availability,
    ControlQuery,
    PersistedConfiguration {
        path: PathBuf,
    },
    #[cfg(test)]
    SyntheticProvider,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EngineObservationState {
    Present(PathBuf),
    // No production absence schema is established by saved provider evidence.
    ConfirmedAbsent,
    Unreadable,
    Unsupported,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EngineObservation {
    pub target: EngineTarget,
    pub source: EngineObservationSource,
    pub state: EngineObservationState,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservationInadequacy(&'static str);

impl std::fmt::Display for ObservationInadequacy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

impl EngineObservation {
    // The caller supplies the current captured binding, never the observed target as authority.
    pub(crate) fn adequate_for(&self, target: &EngineTarget) -> Result<(), ObservationInadequacy> {
        let supported_source = match self.source {
            EngineObservationSource::ControlQuery => true,
            #[cfg(test)]
            EngineObservationSource::SyntheticProvider => true,
            _ => false,
        };
        if !supported_source {
            return Err(ObservationInadequacy("Not a current control query"));
        }
        if self.diagnostic.is_some() {
            return Err(ObservationInadequacy(
                "Observation carries a failure diagnostic",
            ));
        }
        if target.location.is_empty()
            || self.target.location != target.location
            || !target.device_id.as_deref().is_some_and(|expected| {
                !expected.is_empty()
                    && self
                        .target
                        .device_id
                        .as_deref()
                        .is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
            })
        {
            return Err(ObservationInadequacy(
                "Observation does not match the captured device/location",
            ));
        }
        match &self.state {
            EngineObservationState::Present(path)
                if valid_observed_path(&path.to_string_lossy()) =>
            {
                Ok(())
            }
            #[cfg(test)]
            EngineObservationState::ConfirmedAbsent
                if self.source == EngineObservationSource::SyntheticProvider =>
            {
                Ok(())
            }
            _ => Err(ObservationInadequacy(
                "No supported present-path observation",
            )),
        }
    }

    fn failed(
        target: &EngineTarget,
        source: EngineObservationSource,
        state: EngineObservationState,
        diagnostic: impl Into<String>,
    ) -> Self {
        Self {
            target: target.clone(),
            source,
            state,
            diagnostic: Some(diagnostic.into()),
        }
    }

    pub(crate) fn description(&self) -> String {
        format!(
            "{:?} for {:?} from {:?}: {}",
            self.state,
            self.target,
            self.source,
            self.diagnostic
                .as_deref()
                .unwrap_or("supported observation")
        )
    }
}

fn valid_observed_path(path: &str) -> bool {
    !path.trim().is_empty()
        && path == path.trim()
        && !path.contains(['\n', '\r', '\0'])
        && !path.strip_prefix(r"\\?\").unwrap_or(path).contains('?')
        && Path::new(path).is_absolute()
}

// Evidence: saved 2.8.0.42-era provider config, one top-level user.general.
// Deliberately no recursive search through history/screensaver/arbitrary objects.
fn active_general(value: &serde_json::Value) -> Result<&serde_json::Value, String> {
    let root = value
        .as_object()
        .ok_or("Engine config root is not an object")?;
    let mut candidates = root.values().filter_map(|user| user.get("general"));
    let general = candidates
        .next()
        .ok_or("Unsupported Engine config: no user.general")?;
    if candidates.next().is_some() {
        return Err("Ambiguous Engine config: multiple user.general contexts".into());
    }
    if !general.is_object() || root.contains_key("wallpaperconfig") {
        return Err("Unsupported or conflicting active Engine structure".into());
    }
    Ok(general)
}

// serde_json::Value normally overwrites duplicate keys. Provider decisions must
// reject conflicting fields instead. This only decodes JSON; schema selection
// below is still restricted to the evidenced, nonrecursive active structure.
struct UniqueJson(serde_json::Value);

impl<'de> serde::Deserialize<'de> for UniqueJson {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = UniqueJson;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("JSON with unique object keys")
            }
            fn visit_bool<E: serde::de::Error>(self, value: bool) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_i64<E: serde::de::Error>(self, value: i64) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_u64<E: serde::de::Error>(self, value: u64) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, value: f64) -> Result<Self::Value, E> {
                serde_json::Number::from_f64(value)
                    .map(|value| UniqueJson(value.into()))
                    .ok_or_else(|| E::custom("Nonfinite JSON number"))
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(UniqueJson(value.into()))
            }
            fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
                Ok(UniqueJson(serde_json::Value::Null))
            }
            fn visit_seq<A: serde::de::SeqAccess<'de>>(
                self,
                mut sequence: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(value) = sequence.next_element::<UniqueJson>()? {
                    values.push(value.0);
                }
                Ok(UniqueJson(values.into()))
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut object: A,
            ) -> Result<Self::Value, A::Error> {
                let mut values = serde_json::Map::new();
                while let Some((key, value)) = object.next_entry::<String, UniqueJson>()? {
                    if values.insert(key, value.0).is_some() {
                        return Err(serde::de::Error::custom("Duplicate Engine config field"));
                    }
                }
                Ok(UniqueJson(values.into()))
            }
        }
        deserializer.deserialize_any(Visitor)
    }
}

pub(crate) fn selected_wallpaper_from_json(
    json: &str,
    target: &EngineTarget,
    config_path: &Path,
) -> EngineObservation {
    let source = EngineObservationSource::PersistedConfiguration {
        path: config_path.into(),
    };
    let fail = |state, diagnostic: String| {
        EngineObservation::failed(target, source.clone(), state, diagnostic)
    };
    let value: serde_json::Value = match serde_json::from_str::<UniqueJson>(json) {
        Ok(value) => value.0,
        Err(error) => {
            return fail(
                EngineObservationState::Unreadable,
                format!("Invalid Engine config JSON: {error}"),
            )
        }
    };
    let general = match active_general(&value) {
        Ok(general) => general,
        Err(error) => return fail(EngineObservationState::Unsupported, error),
    };
    if target.device_id.is_some() {
        let mut mapping = EngineMonitorMap::default();
        if let Err(error) = collect_monitor_maps(&value, &mut mapping)
            .and_then(|()| validate_target_binding(target, &mapping))
        {
            return fail(EngineObservationState::Unreadable, error);
        }
    }
    let Some(config) = general
        .get("wallpaperconfig")
        .and_then(serde_json::Value::as_object)
    else {
        return fail(
            EngineObservationState::Unreadable,
            "Missing or malformed active wallpaperconfig".into(),
        );
    };
    if config.get("layout").and_then(serde_json::Value::as_u64) != Some(0)
        || config.get("profile") != Some(&serde_json::Value::Null)
    {
        return fail(
            EngineObservationState::Unsupported,
            "Unsupported Engine layout/profile selection semantics".into(),
        );
    }
    let Some(selected) = config
        .get("selectedwallpapers")
        .and_then(serde_json::Value::as_object)
    else {
        return fail(
            EngineObservationState::Unreadable,
            "Missing or malformed active selection table".into(),
        );
    };
    let Some(selection) = selected.get(&target.location) else {
        return fail(
            EngineObservationState::Unsupported,
            "Target missing from active selection; absence encoding is not established".into(),
        );
    };
    let Some(file) = selection.get("file").and_then(serde_json::Value::as_str) else {
        return fail(
            EngineObservationState::Unreadable,
            "Active target file is missing or malformed".into(),
        );
    };
    if !valid_observed_path(file) {
        return fail(
            EngineObservationState::Unreadable,
            "Active target file is not a nonempty absolute path".into(),
        );
    }
    EngineObservation {
        target: target.clone(),
        source,
        state: EngineObservationState::Present(file.into()),
        diagnostic: Some(
            "Persisted selection only; currently rendered scene is not observed".into(),
        ),
    }
}

pub(crate) fn control_query_observation(target: &EngineTarget, output: &str) -> EngineObservation {
    if !valid_observed_path(output) {
        return EngineObservation::failed(
            target,
            EngineObservationSource::ControlQuery,
            EngineObservationState::Unreadable,
            "Control output is not one absolute wallpaper path",
        );
    }
    EngineObservation {
        target: target.clone(),
        source: EngineObservationSource::ControlQuery,
        state: EngineObservationState::Present(output.into()),
        diagnostic: None,
    }
}

pub(crate) fn observe_wallpaper(target: &EngineTarget) -> EngineObservation {
    observe_wallpaper_with_identity(target).0
}

pub(crate) fn observe_wallpaper_with_identity(
    target: &EngineTarget,
) -> (EngineObservation, Option<(usize, u32)>) {
    crate::apply_trace::event_lazy("provider-observation", || {
        format!("phase=start target={}", trace_target(target))
    });
    if availability() != EngineAvailability::ControlAvailable {
        let observation = EngineObservation::failed(
            target,
            EngineObservationSource::Availability,
            EngineObservationState::Unreadable,
            "Engine target readback requires available control",
        );
        crate::apply_trace::event_lazy("provider-observation", || {
            format!("phase=unavailable {}", trace_observation(&observation))
        });
        return (observation, None);
    }
    match active_control_endpoint() {
        Ok(endpoint) => {
            let observation = observe_wallpaper_at(target, &endpoint);
            crate::apply_trace::event_lazy("provider-observation", || {
                format!("phase=complete {}", trace_observation(&observation))
            });
            (observation, Some(endpoint.identity))
        }
        Err(error) => {
            let observation = EngineObservation::failed(
                target,
                EngineObservationSource::Availability,
                EngineObservationState::Unreadable,
                error,
            );
            crate::apply_trace::event_lazy("provider-observation", || {
                format!("phase=endpoint-failure {}", trace_observation(&observation))
            });
            (observation, None)
        }
    }
}

fn observe_wallpaper_with(
    target: &EngineTarget,
    mut mapping: impl FnMut() -> Result<EngineMonitorMap, String>,
    readback: impl FnOnce(Option<&EngineMonitorMap>) -> Result<String, String>,
    persisted: impl FnOnce() -> EngineObservation,
) -> EngineObservation {
    crate::apply_trace::event_lazy("provider-observation", || {
        format!("phase=readback-start target={}", trace_target(target))
    });
    let failed = |error| {
        let observation = EngineObservation::failed(
            target,
            EngineObservationSource::ControlQuery,
            EngineObservationState::Unreadable,
            error,
        );
        crate::apply_trace::event_lazy("provider-observation", || {
            format!("phase=readback-failure {}", trace_observation(&observation))
        });
        observation
    };
    let before = if target.device_id.is_some() {
        crate::apply_trace::event_lazy("provider-observation", || {
            format!("phase=mapping-before target={}", trace_target(target))
        });
        match mapping() {
            Ok(map) => {
                if let Err(error) = validate_target_binding(target, &map) {
                    return failed(error);
                }
                crate::apply_trace::event_lazy("provider-observation", || {
                    format!(
                        "phase=mapping-before-ok target={} locations={:?}",
                        trace_target(target),
                        map.locations()
                    )
                });
                Some(map)
            }
            Err(error) => return failed(error),
        }
    } else {
        None
    };
    let output = match readback(before.as_ref()) {
        Ok(output) => output,
        Err(error) => return failed(error),
    };
    crate::apply_trace::event_lazy("provider-observation", || {
        format!(
            "phase=readback-output target={} empty={} output_bytes={}",
            trace_target(target),
            output.is_empty(),
            output.len()
        )
    });
    if let Some(before) = before.as_ref() {
        crate::apply_trace::event_lazy("provider-observation", || {
            format!("phase=mapping-after target={}", trace_target(target))
        });
        let after = match mapping() {
            Ok(map) => map,
            Err(error) => return failed(error),
        };
        if let Err(error) = validate_target_binding(target, &after) {
            return failed(error);
        }
        if target
            .device_id
            .as_deref()
            .and_then(|device| before.monitor_index_for_device(device))
            != target
                .device_id
                .as_deref()
                .and_then(|device| after.monitor_index_for_device(device))
        {
            return failed("Wallpaper Engine monitor mapping changed during readback".into());
        }
        crate::apply_trace::event_lazy("provider-observation", || {
            format!(
                "phase=mapping-after-ok target={} locations={:?}",
                trace_target(target),
                after.locations()
            )
        });
    }
    let observation = if output.is_empty() {
        // Neither bound selector provided a live path. Saved configuration
        // contributes only a diagnostic.
        persisted()
    } else {
        control_query_observation(target, &output)
    };
    crate::apply_trace::event_lazy("provider-observation", || {
        format!(
            "phase=readback-complete {}",
            trace_observation(&observation)
        )
    });
    observation
}

fn observe_wallpaper_at(target: &EngineTarget, endpoint: &ControlEndpoint) -> EngineObservation {
    let config = endpoint.config_path();
    crate::apply_trace::event_lazy("provider-observation", || {
        format!(
            "phase=endpoint-readback target={} {} config={}",
            trace_target(target),
            trace_endpoint(endpoint),
            config.display()
        )
    });
    observe_wallpaper_with(
        target,
        || load_monitor_map_at(&config),
        |mapping| {
            readback_with_monitor_fallback(
                target,
                target.device_id.as_deref().and_then(|device| {
                    mapping.and_then(|map| map.monitor_index_for_device(device))
                }),
                |arguments| run_control(endpoint, arguments, Duration::from_secs(5)),
            )
        },
        || match std::fs::read_to_string(&config) {
            Ok(json) => selected_wallpaper_from_json(&json, target, &config),
            Err(error) => EngineObservation::failed(
                target,
                EngineObservationSource::PersistedConfiguration {
                    path: config.clone(),
                },
                EngineObservationState::Unreadable,
                format!("Could not read {}: {error}", config.display()),
            ),
        },
    )
}

fn validate_target_binding(
    target: &EngineTarget,
    mapping: &EngineMonitorMap,
) -> Result<(), String> {
    if let Some(device_id) = &target.device_id {
        match mapping.location_for_device(device_id) {
            Some(location) if location == target.location => Ok(()),
            _ => Err(format!(
                "Stable display {device_id} is no longer uniquely mapped to {}",
                target.location
            )),
        }
    } else {
        Ok(())
    }
}

pub(crate) fn current_wallpaper(target: &EngineTarget) -> Result<PathBuf, String> {
    let observation = observe_wallpaper(target);
    observation
        .adequate_for(target)
        .map_err(|error| format!("{error}: {}", observation.description()))?;
    match observation.state {
        EngineObservationState::Present(ref path) => Ok(path.clone()),
        _ => Err(observation.description()),
    }
}

pub(crate) fn same_engine_project(expected: &Path, actual: &Path) -> bool {
    let normalized = |path: &Path| {
        path.to_string_lossy()
            .replace('\\', "/")
            .trim_end_matches('/')
            .to_ascii_lowercase()
    };
    let expected_normalized = normalized(expected);
    let actual_normalized = normalized(actual);
    if expected_normalized == actual_normalized {
        return true;
    }
    expected
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.eq_ignore_ascii_case("project.json"))
        .unwrap_or(false)
        && expected
            .parent()
            .map(|parent| actual_normalized.starts_with(&format!("{}/", normalized(parent))))
            .unwrap_or(false)
}

pub(crate) fn validate_project_file(file_path: &Path) -> Result<(), String> {
    if file_path.is_file() {
        Ok(())
    } else {
        Err(format!(
            "Wallpaper Engine target file is unavailable: {}",
            file_path.display()
        ))
    }
}

// Engine can lose Unicode characters when forwarding a raw payload filename.
fn engine_open_argument(payload: &Path, project_file: Option<&Path>) -> PathBuf {
    let direct = || payload.to_path_buf();
    let Some(manifest) = project_file else {
        return direct();
    };
    if payload.as_os_str().as_encoded_bytes().is_ascii()
        || !manifest.as_os_str().as_encoded_bytes().is_ascii()
        || manifest.parent() != payload.parent()
        || manifest.file_name() != Some(std::ffi::OsStr::new("project.json"))
    {
        return direct();
    }
    let Ok(file) = std::fs::File::open(manifest) else {
        return direct();
    };
    #[derive(serde::Deserialize)]
    struct Project {
        file: String,
        #[serde(rename = "type")]
        kind: String,
    }
    let Ok(project) = serde_json::from_reader::<_, Project>(std::io::BufReader::new(file)) else {
        return direct();
    };
    if project.kind.eq_ignore_ascii_case("video")
        && Some(std::ffi::OsStr::new(&project.file)) == payload.file_name()
    {
        manifest.to_path_buf()
    } else {
        direct()
    }
}

pub(crate) fn open_wallpaper(
    file_path: &Path,
    project_file: Option<&Path>,
    target: &EngineTarget,
    on_dispatch: impl FnOnce(),
) -> Result<(EngineObservation, (usize, u32)), EngineControlFailure> {
    let location = &target.location;
    validate_project_file(file_path).map_err(EngineControlFailure::BeforeDispatch)?;
    ensure_running().map_err(EngineControlFailure::DispatchedOrUnknown)?;
    let endpoint =
        active_control_endpoint().map_err(EngineControlFailure::PreflightObservationFailed)?;
    if target.device_id.is_some() {
        load_monitor_map_at(&endpoint.config_path())
            .and_then(|map| validate_target_binding(target, &map))
            .map_err(EngineControlFailure::PreflightObservationFailed)?;
    }
    let arguments = vec![
        OsString::from("openWallpaper"),
        OsString::from("-file"),
        engine_open_argument(file_path, project_file).into_os_string(),
        OsString::from("-location"),
        OsString::from(location),
    ];
    run_control_mutation_for_project(&endpoint, &arguments, Duration::from_secs(5), on_dispatch)?;
    crate::apply_trace::event_lazy("transition-provider-command-accepted", || {
        format!(
            "target={} {}",
            trace_target(target),
            trace_endpoint(&endpoint)
        )
    });

    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let observation = observe_wallpaper_at(target, &endpoint);
        let last_observation = match &observation.state {
            EngineObservationState::Present(active)
                if observation.adequate_for(target).is_ok()
                    && ensure_no_unsettled_children().is_ok()
                    && same_engine_project(file_path, active) =>
            {
                crate::apply_trace::event_lazy("transition-provider-confirmed", || {
                    trace_observation(&observation)
                });
                return Ok((observation, endpoint.identity));
            }
            _ => observation.description(),
        };
        if Instant::now() >= deadline {
            let error = format!(
                "provider-state-unknown: Wallpaper Engine command did not reach independently confirmed target {location}; expected {}, last readback: {last_observation}",
                file_path.display()
            );
            return Err(EngineControlFailure::DispatchedOrUnknown(error));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

#[cfg(test)]
mod tests {
    use super::query_console::QueryPath;
    use super::*;
    use std::io::Write;

    const CHILD_RUN_FIXTURE_ENV: &str = "WSL_ENGINE_CHILD_RUN_FIXTURE";
    const CHILD_RUN_OUTPUT_LIMIT: usize = 1024 * 1024;

    #[test]
    fn engine_inventory_does_not_open_unrelated_protected_processes() {
        let entries = vec![(4, "System".into()), (100, "csrss.exe".into())];
        let mut queries = Vec::new();
        let observed = engine_processes_from_entries(entries, |pid| {
            queries.push(pid);
            Err("access denied to unrelated protected process".into())
        });
        assert_eq!(observed, Ok(false));
        assert!(queries.is_empty());
    }

    #[test]
    fn engine_inventory_queries_only_identified_engine_candidates() {
        let entries = vec![(4, "System".into()), (123, "WALLPAPER64.EXE".into())];
        let mut queries = Vec::new();
        let observed = engine_processes_from_entries(entries, |pid| {
            queries.push(pid);
            Ok(Some(
                r"F:\SteamLibrary\wallpaper_engine\wallpaper64.exe".into(),
            ))
        });
        assert_eq!(observed, Ok(true));
        assert_eq!(queries, vec![123]);
    }

    #[test]
    fn engine_inventory_unreadable_or_reused_candidate_remains_unknown() {
        let entries = || vec![(123, "wallpaper64.exe".into())];
        assert!(engine_processes_from_entries(entries(), |_| Err("access denied".into())).is_err());
        assert!(engine_processes_from_entries(entries(), |_| Ok(None)).is_err());
        assert!(
            engine_processes_from_entries(entries(), |_| Ok(Some(r"C:\other.exe".into()))).is_err()
        );
        assert!(engine_processes_from_entries(entries(), |_| Ok(Some(
            r"C:\webwallpaper64.exe".into()
        )))
        .is_err());
    }

    #[test]
    fn provider_endpoint_harness_accepts_only_installed_main_engine_images() {
        for executable in [
            r"C:\SteamLibrary\steamapps\common\wallpaper_engine\wallpaper32.exe",
            r"C:\SteamLibrary\steamapps\common\wallpaper_engine\wallpaper64.exe",
        ] {
            let endpoint = resolve_control_endpoint(
                || Ok((0x1234, 77)),
                |_| Ok(Some(executable.into())),
                |path| path.to_string_lossy().contains("wallpaper_engine"),
            )
            .expect("installed 32/64 main images should bind");
            assert_eq!(endpoint.identity, (0x1234, 77));
            assert_eq!(endpoint.executable, Path::new(executable));
        }
        for executable in [
            r"C:\SteamLibrary\steamapps\common\wallpaper_engine\webwallpaper64.exe",
            r"C:\other\wallpaper64.exe",
        ] {
            assert!(resolve_control_endpoint(
                || Ok((0x1234, 77)),
                |_| Ok(Some(executable.into())),
                |path| path.to_string_lossy().contains("wallpaper_engine"),
            )
            .is_err());
        }
    }

    #[test]
    fn provider_endpoint_harness_rejects_owner_identity_drift() {
        let calls = std::cell::Cell::new(0);
        let result = resolve_control_endpoint(
            || {
                calls.set(calls.get() + 1);
                Ok(if calls.get() == 1 {
                    (0x1234, 77)
                } else {
                    (0x5678, 88)
                })
            },
            |_| {
                Ok(Some(
                    r"C:\SteamLibrary\steamapps\common\wallpaper_engine\wallpaper64.exe".into(),
                ))
            },
            |_| true,
        );
        assert!(result.is_err());
        assert!(result
            .err()
            .is_some_and(|error| error.contains("owner changed")));
    }

    const FIXTURE_SETTLEMENT_TIMEOUT: Duration = Duration::from_secs(2);
    const FIXTURE_CLEANUP_TIMEOUT: Duration = Duration::from_secs(2);

    #[test]
    fn provider_command_harness_refuses_endpoint_drift_before_dispatch() {
        let endpoint = ControlEndpoint {
            identity: (0x1234, 77),
            executable: std::env::current_exe().unwrap(),
        };
        let lookups = std::cell::Cell::new(0);
        let error = run_control_output_with(
            &endpoint,
            &["-control".into(), "getWallpaper".into()],
            Duration::from_secs(5),
            || panic!("changed endpoint must not dispatch"),
            || {
                lookups.set(lookups.get() + 1);
                Ok(ControlEndpoint {
                    identity: (0x5678, 88),
                    executable: endpoint.executable.clone(),
                })
            },
            || panic!("changed endpoint must not construct a command"),
        )
        .expect_err("stale endpoint must be refused");
        assert_eq!(lookups.get(), 1);
        assert!(error.message.contains("before dispatch"));
        assert_eq!(error.process_id, None);
        assert_eq!(error.child_stopped, Some(true));
        assert!(error.unsettled_child.is_none());
    }

    #[test]
    fn provider_command_harness_revalidates_owned_child_result() {
        for drift in [false, true] {
            let endpoint = ControlEndpoint {
                identity: (0x1234, 77),
                executable: std::env::current_exe().unwrap(),
            };
            let lookups = std::cell::Cell::new(0);
            let dispatches = std::cell::Cell::new(0);
            let result = run_control_output_with(
                &endpoint,
                &["-control".into(), "getWallpaper".into()],
                Duration::from_secs(5),
                || dispatches.set(dispatches.get() + 1),
                || {
                    lookups.set(lookups.get() + 1);
                    Ok(ControlEndpoint {
                        identity: if drift && lookups.get() == 2 {
                            (0x5678, 88)
                        } else {
                            endpoint.identity
                        },
                        executable: endpoint.executable.clone(),
                    })
                },
                || fixture_command("small-output"),
            );
            assert_eq!(lookups.get(), 2);
            assert_eq!(dispatches.get(), 1);
            if drift {
                let error = result.expect_err("changed endpoint cannot confirm output");
                assert!(error.message.contains("during the command"));
                assert!(error.process_id.is_some());
                assert_eq!(error.child_stopped, Some(true));
                assert!(error.unsettled_child.is_none());
            } else {
                assert!(output_text(&result.unwrap().stdout).contains("fixture-stdout"));
            }
        }
    }

    #[test]
    fn provider_command_harness_preserves_timeout_provenance() {
        let endpoint = ControlEndpoint {
            identity: (0x1234, 77),
            executable: std::env::current_exe().unwrap(),
        };
        let lookups = std::cell::Cell::new(0);
        let dispatches = std::cell::Cell::new(0);
        let error = run_control_output_with(
            &endpoint,
            &["-control".into(), "getWallpaper".into()],
            Duration::from_secs(2),
            || dispatches.set(dispatches.get() + 1),
            || {
                lookups.set(lookups.get() + 1);
                Ok(ControlEndpoint {
                    identity: endpoint.identity,
                    executable: endpoint.executable.clone(),
                })
            },
            || fixture_command("timeout-output"),
        )
        .expect_err("owned sleeping query must time out");
        assert_eq!(lookups.get(), 1);
        assert_eq!(dispatches.get(), 1);
        assert!(error.message.contains("timed out"));
        assert!(error.process_id.is_some());
        assert_eq!(error.child_stopped, Some(true));
        assert!(error.unsettled_child.is_none());
        let message = child_run_error_text(error);
        assert!(message.contains("timeout-stdout"));
        assert!(message.contains("timeout-stderr"));
    }

    struct UnsettledChildFixture {
        process_id: u32,
    }

    impl UnsettledChildFixture {
        fn spawn(mode: &str) -> Self {
            let child = fixture_command(mode)
                .spawn()
                .expect("start owned child fixture");
            let process_id = child.id();
            register_unsettled_child(UnsettledChild {
                process_id,
                child: PendingProcess::Child(child),
            });
            Self { process_id }
        }

        fn process_id(&self) -> u32 {
            self.process_id
        }

        fn wait_until_settled(&self, timeout: Duration) -> Result<(), String> {
            let deadline = Instant::now() + timeout;
            loop {
                if settle_unsettled_child_for_test(self.process_id)? {
                    return Ok(());
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(format!(
                        "fixture child {} did not settle before the {} ms deadline",
                        self.process_id,
                        timeout.as_millis()
                    ));
                }
                thread::sleep(Duration::from_millis(10).min(remaining));
            }
        }
    }

    impl Drop for UnsettledChildFixture {
        fn drop(&mut self) {
            let deadline = Instant::now() + FIXTURE_CLEANUP_TIMEOUT;
            match stop_unsettled_child_for_test(self.process_id, deadline) {
                Ok(true) => {}
                Ok(false) => eprintln!(
                    "fixture child {} remained registered after bounded exact cleanup",
                    self.process_id
                ),
                Err(error) => eprintln!(
                    "fixture child {} cleanup could not inspect its exact registry entry: {}",
                    self.process_id, error
                ),
            }
        }
    }

    fn fixture_command(mode: &str) -> Command {
        let executable = std::env::current_exe().expect("current test executable");
        let mut command = Command::new(executable);
        command
            .args([
                "--exact",
                "engine_ipc::tests::bounded_child_runner_fixture",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD_RUN_FIXTURE_ENV, mode);
        command
    }

    #[test]
    fn bounded_child_runner_fixture() {
        let Ok(mode) = std::env::var(CHILD_RUN_FIXTURE_ENV) else {
            return;
        };
        match mode.as_str() {
            "small-output" => {
                std::io::stdout().write_all(b"fixture-stdout\n").unwrap();
                std::io::stderr().write_all(b"fixture-stderr\n").unwrap();
            }
            "nonzero-output" => {
                let mut stdout = std::io::stdout().lock();
                stdout.write_all(b"nonzero-stdout\n").unwrap();
                stdout.flush().unwrap();
                std::io::stderr().write_all(b"nonzero-stderr\n").unwrap();
                std::process::exit(7);
            }
            "large-output" => {
                let bytes = vec![b'x'; CHILD_RUN_OUTPUT_LIMIT * 2];
                std::io::stdout().write_all(&bytes).unwrap();
                std::io::stderr().write_all(&bytes).unwrap();
            }
            "timeout-output" => {
                let mut stdout = std::io::stdout().lock();
                stdout.write_all(b"timeout-stdout\n").unwrap();
                stdout.flush().unwrap();
                let mut stderr = std::io::stderr().lock();
                stderr.write_all(b"timeout-stderr\n").unwrap();
                stderr.flush().unwrap();
                std::thread::sleep(Duration::from_secs(30));
            }
            "nonzero" => std::process::exit(7),
            "short-sleep" => std::thread::sleep(Duration::from_millis(250)),
            "sleep" => std::thread::sleep(Duration::from_secs(30)),
            unexpected => panic!("unexpected child fixture mode: {unexpected}"),
        }
    }

    #[test]
    fn bounded_child_runner_drains_both_streams_and_preserves_exit_status() {
        let output = run_command_bounded(
            &mut fixture_command("small-output"),
            Duration::from_secs(5),
            "fixture success",
        )
        .expect("fixture child should settle");

        assert_eq!(output.status.code(), Some(0));
        assert!(String::from_utf8_lossy(&output.stdout.bytes).contains("fixture-stdout"));
        assert!(String::from_utf8_lossy(&output.stderr.bytes).contains("fixture-stderr"));
        assert_eq!(output.stdout.discarded_bytes, 0);
        assert_eq!(output.stderr.discarded_bytes, 0);
    }

    #[test]
    fn bounded_child_runner_keeps_draining_after_capture_limit() {
        let output = run_command_bounded(
            &mut fixture_command("large-output"),
            Duration::from_secs(10),
            "fixture large output",
        )
        .expect("large-output child should not deadlock on full pipes");

        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout.bytes.len(), CHILD_RUN_OUTPUT_LIMIT);
        assert_eq!(output.stderr.bytes.len(), CHILD_RUN_OUTPUT_LIMIT);
        assert!(output.stdout.discarded_bytes > 0);
        assert!(output.stderr.discarded_bytes > 0);
    }

    #[test]
    fn bounded_child_runner_reports_nonzero_exit_without_losing_output() {
        let output = run_command_bounded(
            &mut fixture_command("nonzero-output"),
            Duration::from_secs(5),
            "fixture nonzero exit",
        )
        .expect("nonzero exit is a completed child outcome");

        assert_eq!(output.status.code(), Some(7));
        assert!(String::from_utf8_lossy(&output.stdout.bytes).contains("nonzero-stdout"));
        assert!(String::from_utf8_lossy(&output.stderr.bytes).contains("nonzero-stderr"));
    }

    #[test]
    fn bounded_child_runner_reaps_its_exact_timeout_child_within_deadline() {
        let started = Instant::now();
        let error = run_command_bounded(
            &mut fixture_command("sleep"),
            Duration::from_secs(2),
            "fixture timeout",
        )
        .expect_err("sleeping fixture should time out");

        assert!(error.message.contains("timed out"));
        assert_eq!(error.child_stopped, Some(true));
        assert!(error.process_id.is_some());
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn bounded_child_runner_preserves_output_after_exact_timeout_child_is_stopped() {
        let error = run_command_bounded(
            &mut fixture_command("timeout-output"),
            Duration::from_secs(2),
            "fixture timeout output",
        )
        .expect_err("timeout-output child should time out");

        assert_eq!(error.child_stopped, Some(true));
        let message = child_run_error_text(error);
        assert!(message.contains("timeout-stdout"));
        assert!(message.contains("timeout-stderr"));
    }

    #[test]
    fn unsettled_engine_child_blocks_mutation_until_the_exact_child_exits() {
        let fixture = UnsettledChildFixture::spawn("sleep");
        let process_id = fixture.process_id();

        let blocked = ensure_mutation_allowed().expect_err("active child must block");
        assert!(blocked.contains(&process_id.to_string()));
    }

    #[test]
    fn exited_command_child_releases_only_the_child_gate() {
        let settled = UnsettledChildFixture::spawn("short-sleep");
        let settled_id = settled.process_id();
        let still_running = UnsettledChildFixture::spawn("sleep");
        let still_running_id = still_running.process_id();

        settled
            .wait_until_settled(FIXTURE_SETTLEMENT_TIMEOUT)
            .expect("the exact command child should settle before its deadline");

        let blocked = ensure_mutation_allowed()
            .expect_err("an unrelated active child must keep the production gate blocked");
        assert!(blocked.contains(&still_running_id.to_string()));
        assert!(!blocked.contains(&settled_id.to_string()));
    }

    #[test]
    fn unsettled_child_fixture_cleanup_is_exact_after_unwind() {
        let mut process_id = None;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let fixture = UnsettledChildFixture::spawn("sleep");
            process_id = Some(fixture.process_id());
            panic!("intentional fixture unwind");
        }));

        assert!(result.is_err());
        let process_id = process_id.expect("fixture must record its exact child");
        let children = unsettled_children()
            .lock()
            .expect("the child gate should remain usable after fixture unwind");
        assert!(!children.iter().any(|entry| entry.process_id == process_id));
    }

    #[test]
    fn engine_startup_timeout_is_unknown_but_observed_exit_is_settled() {
        let timed_out = engine_startup_failure(4242, EngineStartupFailure::StillRunningAtDeadline);
        assert!(timed_out.starts_with("provider-state-unknown:"));
        assert!(timed_out.contains("child_pid=4242"));
        assert!(timed_out.contains("child_stopped=false"));

        let wait_failed = engine_startup_failure(
            4243,
            EngineStartupFailure::WaitFailed("fixture wait error".into()),
        );
        assert!(wait_failed.starts_with("provider-state-unknown:"));
        assert!(wait_failed.contains("child_stopped=unknown"));

        let exited =
            engine_startup_failure(4244, EngineStartupFailure::Exited("exit status: 1".into()));
        assert!(!exited.starts_with("provider-state-unknown:"));
        assert!(exited.contains("child_pid=4244"));
        assert!(exited.contains("child_stopped=true"));
    }

    #[test]
    fn parses_wallpaper_engine_device_paths_into_locations() {
        let json = r#"{
            "user": {
                "general": {
                    "user": { "monitormap": {
                        "//?/DISPLAY#ABC#1#{GUID}": { "location": 2 }
                    }}
                }
            }
        }"#;

        let map = parse_monitor_map(json).unwrap();
        assert_eq!(
            map.location_for_device(r"\\?\DISPLAY#ABC#1#{GUID}"),
            Some("Monitor2".to_string())
        );
    }

    #[test]
    fn engine_availability_separates_control_process_and_unreadable_state() {
        assert_eq!(
            availability_from_observation(true, Ok(false)),
            EngineAvailability::ControlAvailable
        );
        assert_eq!(
            availability_from_observation(false, Ok(true)),
            EngineAvailability::ProcessOnly
        );
        assert_eq!(
            availability_from_observation(false, Ok(false)),
            EngineAvailability::Unavailable
        );
        assert_eq!(
            availability_from_observation(false, Err("enumeration failed".into())),
            EngineAvailability::Unreadable
        );
    }

    #[test]
    fn property_payload_uses_the_documented_raw_delimiter_safely() {
        assert_eq!(
            encode_property_payload(r#"{"rate":10,"label":"a)b"}"#).unwrap(),
            r#"RAW~({"rate":10,"label":"a\u0029b"})~END"#
        );
        assert!(encode_property_payload("[]").is_err());
    }

    #[test]
    fn unicode_payload_uses_only_its_declaring_manifest() {
        let root = std::env::temp_dir().join(format!("engine-manifest-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let manifest = root.join("project.json");
        let video = root.join("中文.mp4");
        std::fs::write(&manifest, r#"{"type":"video","file":"中文.mp4"}"#).unwrap();
        assert_eq!(engine_open_argument(&video, Some(&manifest)), manifest);
        let other = root.join("其他.mp4");
        assert_eq!(engine_open_argument(&other, Some(&manifest)), other);
        let ascii = root.join("video.mp4");
        assert_eq!(engine_open_argument(&ascii, Some(&manifest)), ascii);
        assert!(!valid_observed_path(r"C:\workshop\??.mp4"));
        assert!(valid_observed_path(r"\\?\C:\workshop\中文.mp4"));
        assert_eq!(engine_open_argument(&video, None), video);
        std::fs::write(&manifest, r#"{"type":"web","file":"中文.mp4"}"#).unwrap();
        assert_eq!(engine_open_argument(&video, Some(&manifest)), video);
        std::fs::remove_file(manifest).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn project_json_matches_the_running_project_payload() {
        assert!(same_engine_project(
            Path::new(r"C:\workshop\123\project.json"),
            Path::new(r"C:\workshop\123\scene.pkg")
        ));
        assert!(!same_engine_project(
            Path::new(r"C:\workshop\123\project.json"),
            Path::new(r"C:\workshop\456\scene.pkg")
        ));
    }

    #[test]
    fn reads_only_the_active_selected_wallpaper() {
        // Reduced from the saved provider-maintained config described in SCHEMA-PROVENANCE.md.
        let json = r#"{"account":{"general":{
            "user":{"monitormap":{"display-a":{"location":0},"display-b":{"location":1}}},
            "wallpaperconfig":{"layout":0,"profile":null,"selectedwallpapers":{"Monitor0":{"file":"C:/workshop/123/scene.pkg"}}},
            "wallpaperconfigrecent":[{"config":{"selectedwallpapers":{"Monitor0":{"file":"C:/old/scene.pkg"}}}}]
        }}}"#;
        let target = EngineTarget::bound("display-a", "Monitor0");
        let observed = selected_wallpaper_from_json(json, &target, Path::new("saved-config.json"));
        assert_eq!(observed.target, target);
        assert_eq!(
            observed.state,
            EngineObservationState::Present("C:/workshop/123/scene.pkg".into())
        );
        assert!(matches!(
            observed.source,
            EngineObservationSource::PersistedConfiguration { .. }
        ));
        let missing = selected_wallpaper_from_json(
            json,
            &EngineTarget::bound("display-b", "Monitor1"),
            Path::new("saved-config.json"),
        );
        assert_eq!(missing.state, EngineObservationState::Unsupported);
    }

    #[test]
    fn empty_location_readback_uses_the_bound_numeric_monitor() {
        let target = EngineTarget::bound("display-a", "Monitor2");
        let map = parse_monitor_map(
            r#"{"account":{"general":{"user":{"monitormap":{"display-a":{"location":2}}}}}}"#,
        )
        .unwrap();
        let mut calls = Vec::new();
        let output = readback_with_monitor_fallback(
            &target,
            map.monitor_index_for_device("display-a"),
            |args| {
                calls.push(args.to_vec());
                if calls.len() == 1 {
                    assert_eq!(args, control_args("getWallpaper", Some("Monitor2")));
                    Ok(QueryPath::Path(String::new()))
                } else {
                    assert_eq!(args, control_args_for_monitor("getWallpaper", 2));
                    Ok(QueryPath::Path("C:/fixture/active.pkg".into()))
                }
            },
        )
        .unwrap();
        assert_eq!(output, "C:/fixture/active.pkg");
        assert_eq!(calls.len(), 2);
    }

    #[test]
    fn rejected_location_requires_a_fresh_bound_path() {
        let rejected = "Engine wallpaper path contains literal question marks";
        let target = EngineTarget::bound("display-a", "Monitor2");
        for fallback in [
            QueryPath::Path(String::new()),
            QueryPath::Rejected(rejected.into()),
        ] {
            let mut results = [QueryPath::Rejected(rejected.into()), fallback].into_iter();
            let error =
                readback_with_monitor_fallback(&target, Some(2), |_| Ok(results.next().unwrap()))
                    .unwrap_err();
            assert_eq!(error, rejected);
            assert_eq!(results.len(), 0);
        }
        let unbound = EngineTarget::location("Monitor2");
        let error = readback_with_monitor_fallback(&unbound, None, |args| {
            assert_eq!(args, control_args("getWallpaper", Some("Monitor2")));
            Ok(QueryPath::Rejected(rejected.into()))
        })
        .unwrap_err();
        assert_eq!(error, rejected);
    }

    #[test]
    fn location_transport_failure_does_not_retry() {
        let target = EngineTarget::bound("display-a", "Monitor2");
        for failure in [
            "Engine endpoint changed during readback",
            "Readback job did not settle",
            "Conflicting Engine query outputs",
        ] {
            let error = readback_with_monitor_fallback(&target, Some(2), |args| {
                assert_eq!(args, control_args("getWallpaper", Some("Monitor2")));
                Err(failure.into())
            })
            .unwrap_err();
            assert_eq!(error, failure);
        }
    }

    #[test]
    fn provider_observation_harness_revalidates_mapping_and_accepts_fresh_query() {
        let target = EngineTarget::bound("display-a", "Monitor2");
        let map = parse_monitor_map(
            r#"{"account":{"general":{"user":{"monitormap":{"display-a":{"location":2}}}}}}"#,
        )
        .unwrap();
        let mapping_calls = std::cell::Cell::new(0);
        let query_calls = std::cell::Cell::new(0);
        let observed = observe_wallpaper_with(
            &target,
            || {
                mapping_calls.set(mapping_calls.get() + 1);
                Ok(map.clone())
            },
            |mapping| {
                assert_eq!(
                    mapping.and_then(|map| map.monitor_index_for_device("display-a")),
                    Some(2)
                );
                readback_with_monitor_fallback(&target, Some(2), |arguments| {
                    query_calls.set(query_calls.get() + 1);
                    if query_calls.get() == 1 {
                        assert_eq!(arguments, control_args("getWallpaper", Some("Monitor2")));
                        Ok(QueryPath::Rejected(
                            "Engine wallpaper path contains literal question marks".into(),
                        ))
                    } else {
                        assert_eq!(arguments, control_args_for_monitor("getWallpaper", 2));
                        Ok(QueryPath::Path(r"C:\fixture\active.pkg".into()))
                    }
                })
            },
            || panic!("fresh control readback must not fall back to persisted config"),
        );

        assert_eq!(mapping_calls.get(), 2);
        assert_eq!(query_calls.get(), 2);
        assert!(matches!(
            observed.source,
            EngineObservationSource::ControlQuery
        ));
        assert_eq!(
            observed.state,
            EngineObservationState::Present(r"C:\fixture\active.pkg".into())
        );
        assert!(observed.adequate_for(&target).is_ok());
    }

    #[test]
    fn provider_observation_harness_keeps_two_empty_queries_unconfirmed() {
        let device = "display-a";
        let target = EngineTarget::bound(device, "Monitor2");
        let saved_path = Path::new("saved-config.json");
        let json = serde_json::json!({"account":{"general":{
            "user":{"monitormap":{"display-a":{"location":2}}},
            "wallpaperconfig":{"layout":0,"profile":null,
                "selectedwallpapers":{"Monitor2":{"file":"C:/saved/scene.pkg"}}}
        }}})
        .to_string();
        let mut mapping_reads = 0;
        let mut commands = Vec::new();
        let observed = observe_wallpaper_with(
            &target,
            || {
                mapping_reads += 1;
                parse_monitor_map(&json)
            },
            |map| {
                let monitor = map.and_then(|map| map.monitor_index_for_device(device));
                readback_with_monitor_fallback(&target, monitor, |args| {
                    commands.push(args.to_vec());
                    Ok(QueryPath::Path(String::new()))
                })
            },
            || selected_wallpaper_from_json(&json, &target, saved_path),
        );

        assert_eq!(mapping_reads, 2);
        assert_eq!(
            commands,
            [
                control_args("getWallpaper", Some("Monitor2")),
                control_args_for_monitor("getWallpaper", 2),
            ]
        );
        assert_eq!(observed.target, target);
        assert_eq!(
            observed.source,
            EngineObservationSource::PersistedConfiguration {
                path: saved_path.into()
            }
        );
        assert_eq!(
            observed.state,
            EngineObservationState::Present("C:/saved/scene.pkg".into())
        );
        assert!(observed.adequate_for(&target).is_err());
    }

    #[test]
    #[ignore = "requires an already-running Engine, explicit owner PID and display ID"]
    fn live_bound_get_wallpaper() -> Result<(), String> {
        let device = std::env::var("WALLPAPER_SWITCHER_READBACK_DEVICE").map_err(|_| {
            "Set WALLPAPER_SWITCHER_READBACK_DEVICE to the affected Windows device ID"
        })?;
        let pid: u32 = std::env::var("WALLPAPER_SWITCHER_READBACK_OWNER_PID")
            .map_err(|_| {
                "Set WALLPAPER_SWITCHER_READBACK_OWNER_PID to the observed Engine owner PID"
            })?
            .parse()
            .map_err(|_| "Invalid WALLPAPER_SWITCHER_READBACK_OWNER_PID")?;
        let endpoint = active_control_endpoint()
            .map_err(|error| format!("Live endpoint unavailable before dispatch: {error}"))?;
        if pid != endpoint.identity.1 {
            return Err(format!(
                "Unexpected endpoint before dispatch: {}",
                trace_endpoint(&endpoint)
            ));
        }
        let monitors = crate::native_wallpaper::monitors()?;
        let mut matches = monitors
            .iter()
            .filter(|m| m.device_id.eq_ignore_ascii_case(&device));
        let monitor = matches
            .next()
            .ok_or("Requested Windows display is absent")?;
        if matches.next().is_some() {
            return Err("Requested Windows display is ambiguous".into());
        }
        let map = load_monitor_map_at(&endpoint.config_path())?;
        let location = map
            .location_for_device(&monitor.device_id)
            .ok_or("No Engine location for requested Windows display")?;
        let target = EngineTarget::bound(&monitor.device_id, &location);
        let tracing = crate::apply_trace::enabled();
        crate::apply_trace::set_enabled(true);
        let observed = observe_wallpaper_at(&target, &endpoint);
        let after = active_control_endpoint();
        crate::apply_trace::set_enabled(tracing);

        eprintln!("{}", observed.description());
        if after? != endpoint {
            return Err("Engine endpoint changed during the probe".into());
        }
        observed.adequate_for(&target).map_err(|e| e.to_string())
    }

    #[test]
    fn provider_observation_harness_rejects_mapping_drift_after_query() {
        let target = EngineTarget::bound("display-a", "Monitor2");
        let before = parse_monitor_map(
            r#"{"account":{"general":{"user":{"monitormap":{"display-a":{"location":2}}}}}}"#,
        )
        .unwrap();
        let after = parse_monitor_map(
            r#"{"account":{"general":{"user":{"monitormap":{"display-a":{"location":3}}}}}}"#,
        )
        .unwrap();
        let mapping_calls = std::cell::Cell::new(0);
        let observed = observe_wallpaper_with(
            &target,
            || {
                mapping_calls.set(mapping_calls.get() + 1);
                Ok(if mapping_calls.get() == 1 {
                    before.clone()
                } else {
                    after.clone()
                })
            },
            |map| {
                let monitor = map.and_then(|map| map.monitor_index_for_device("display-a"));
                readback_with_monitor_fallback(&target, monitor, |args| {
                    if args == control_args("getWallpaper", Some("Monitor2")) {
                        Ok(QueryPath::Rejected(
                            "Engine query did not return one absolute path".into(),
                        ))
                    } else {
                        assert_eq!(args, control_args_for_monitor("getWallpaper", 2));
                        Ok(QueryPath::Path(r"C:\fixture\active.pkg".into()))
                    }
                })
            },
            || panic!("mapping drift must fail before persisted fallback"),
        );

        assert_eq!(mapping_calls.get(), 2);
        assert_eq!(observed.state, EngineObservationState::Unreadable);
        assert!(observed
            .diagnostic
            .as_deref()
            .is_some_and(|error| error.contains("Stable display")));
        assert!(observed.adequate_for(&target).is_err());
    }

    #[test]
    fn empty_location_without_a_bound_monitor_does_not_use_persisted_selection() {
        let target = EngineTarget::location("Monitor0");
        let error =
            readback_with_monitor_fallback(&target, None, |_| Ok(QueryPath::Path(String::new())))
                .unwrap_err();
        assert_eq!(
            error,
            "Wallpaper Engine numeric monitor identity is unavailable"
        );
    }

    #[test]
    fn r12_unknown_selection_data_never_proves_absence() {
        let target = EngineTarget::bound("display-a", "Monitor0");
        let selections = [
            "null",
            "[]",
            "{}",
            r#"{"Monitor1":{"file":"C:/other.pkg"}}"#,
            r#"{"Monitor0":{"file":42}}"#,
            r#"{"Monitor0":{"file":null}}"#,
            r#"{"Monitor0":{"file":""}}"#,
            r#"{"Monitor0":{"file":"relative.pkg"}}"#,
            r#"{"Monitor0":{"file":"  "}}"#,
        ];
        for selected in selections {
            let selected: serde_json::Value = serde_json::from_str(selected).unwrap();
            let json = serde_json::json!({"account":{"general":{"user":{"monitormap":{"display-a":{"location":0},"display-b":{"location":1}}},"wallpaperconfig":{"layout":0,"profile":null,"selectedwallpapers":selected}}}}).to_string();
            let observed =
                selected_wallpaper_from_json(&json, &target, Path::new("saved-config.json"));
            assert!(
                matches!(
                    observed.state,
                    EngineObservationState::Unreadable | EngineObservationState::Unsupported
                ),
                "{json}: {observed:?}"
            );
        }
        for json in [
            "",
            "{",
            "{}",
            r#"{"wallpaperconfig":{"selectedwallpapers":[]}}"#,
            r#"{"account":{"general":{"wallpaperconfigrecent":[{"config":{"selectedwallpapers":{"Monitor0":{"file":"C:/old.pkg"}}}}]}}}"#,
            r#"{"a":{"general":{}},"b":{"general":{}}}"#,
            r#"{"nested":{"arbitrary":{"wallpaperconfig":{"selectedwallpapers":{"Monitor0":{"file":"C:/fake.pkg"}}}}}}"#,
        ] {
            let observed =
                selected_wallpaper_from_json(json, &target, Path::new("saved-config.json"));
            assert!(
                matches!(
                    observed.state,
                    EngineObservationState::Unreadable | EngineObservationState::Unsupported
                ),
                "{json}: {observed:?}"
            );
        }
    }

    #[test]
    fn r12_duplicate_selections_and_ambiguous_monitor_maps_are_rejected() {
        let duplicate = r#"{"account":{"general":{"wallpaperconfig":{"layout":0,"profile":null,"selectedwallpapers":{"Monitor0":{"file":"C:/first.pkg","file":"C:/second.pkg"}}}}}}"#;
        assert_eq!(
            selected_wallpaper_from_json(
                duplicate,
                &EngineTarget::bound("display-a", "Monitor0"),
                Path::new("config.json")
            )
            .state,
            EngineObservationState::Unreadable
        );
        for json in [
            r#"{"a":{"general":{}},"b":{"general":{}}}"#,
            r#"{"a":{"general":{"user":{"monitormap":{"a":{"location":0},"b":{"location":0}}}}}}"#,
            r#"{"a":{"general":{"user":{"monitormap":{"a":{"location":0},"A":{"location":1}}}}}}"#,
            r#"{"a":{"general":{"user":{"monitormap":{"a":{"location":"garbage"}}}}}}"#,
            r#"{"history":{"monitormap":{"a":{"location":0}}}}"#,
        ] {
            assert!(parse_monitor_map(json).is_err(), "{json}");
        }
    }

    #[test]
    fn r12_selection_validates_bound_device_in_the_same_config_snapshot() {
        let target = EngineTarget::bound("display-a", "Monitor0");
        let snapshot = |mapping| {
            serde_json::json!({"account":{"general":{
            "user":{"monitormap":mapping},
            "wallpaperconfig":{"layout":0,"profile":null,"selectedwallpapers":{"Monitor0":{"file":"C:/other-or-original.pkg"}}}
        }}}).to_string()
        };
        let original =
            snapshot(serde_json::json!({"display-a":{"location":0},"display-b":{"location":1}}));
        assert!(matches!(
            selected_wallpaper_from_json(&original, &target, Path::new("config.json")).state,
            EngineObservationState::Present(_)
        ));
        for mapping in [
            serde_json::json!({"display-a":{"location":1},"display-b":{"location":0}}),
            serde_json::json!({"display-b":{"location":0}}),
            serde_json::json!({"display-a":{"location":0},"display-b":{"location":0}}),
        ] {
            let changed = snapshot(mapping);
            assert_eq!(
                selected_wallpaper_from_json(&changed, &target, Path::new("config.json")).state,
                EngineObservationState::Unreadable
            );
            let map = parse_monitor_map(&changed);
            assert!(map
                .and_then(|map| validate_target_binding(&target, &map))
                .is_err());
        }
        let no_mapping = r#"{"account":{"general":{"wallpaperconfig":{"layout":0,"profile":null,"selectedwallpapers":{"Monitor0":{"file":"C:/other.pkg"}}}}}}"#;
        assert_eq!(
            selected_wallpaper_from_json(no_mapping, &target, Path::new("config.json")).state,
            EngineObservationState::Unreadable
        );
    }

    #[test]
    fn r12_control_errors_preserve_child_dispatch_provenance() {
        let before = control_failure_from_run_error(ChildRunError::before_start(
            "Executable missing or spawn failed",
        ));
        assert!(matches!(before, EngineControlFailure::BeforeDispatch(_)));
        for settled in [Some(true), Some(false), None] {
            let after = control_failure_from_run_error(ChildRunError::after_start(
                "Control result uncertain",
                123,
                settled,
            ));
            assert!(matches!(
                after,
                EngineControlFailure::DispatchedOrUnknown(_)
            ));
        }
        let unknown = control_failure_from_run_error(ChildRunError {
            message: "Unknown child provenance".into(),
            process_id: None,
            child_stopped: None,
            unsettled_child: None,
            stdout: None,
            stderr: None,
        });
        assert!(matches!(
            unknown,
            EngineControlFailure::DispatchedOrUnknown(_)
        ));
    }

    #[test]
    fn absence_guard_defensive_close_dispatches_no_command_when_unsupported() {
        let target = EngineTarget::bound("display-a", "Monitor0");
        let result = close_wallpaper_with(
            capabilities(),
            &target,
            || panic!("Unsupported close must stop before provider availability query"),
            || panic!("Unsupported close must issue zero commands"),
            || panic!("Unsupported capability needs no child observation"),
            |_| panic!("Unsupported close must not query or invent absence"),
            Duration::ZERO,
        );
        assert!(matches!(
            result,
            Err(EngineCloseFailure::UnsupportedBeforeDispatch(_))
        ));
        assert_eq!(
            capabilities().absence_confirmation,
            AbsenceConfirmationCapability::Unsupported
        );
    }

    #[test]
    fn absence_guard_supported_close_requires_exact_target_and_settled_child() {
        let supported = EngineCapabilities {
            absence_confirmation: AbsenceConfirmationCapability::SyntheticSupported,
        };
        let target = EngineTarget::bound("display-a", "Monitor0");
        for (exact_target, settled, state) in [
            (true, true, EngineObservationState::ConfirmedAbsent),
            (false, true, EngineObservationState::ConfirmedAbsent),
            (true, false, EngineObservationState::ConfirmedAbsent),
            (true, true, EngineObservationState::Unsupported),
            (
                true,
                true,
                EngineObservationState::Present("C:/still-present.pkg".into()),
            ),
        ] {
            let dispatched = std::cell::Cell::new(false);
            let calls = std::cell::Cell::new(0);
            let result = close_wallpaper_with(
                supported,
                &target,
                || Ok(()),
                || {
                    dispatched.set(true);
                    calls.set(calls.get() + 1);
                    Ok(())
                },
                || !dispatched.get() || settled,
                |target| EngineObservation {
                    target: if exact_target {
                        target.clone()
                    } else {
                        EngineTarget::bound("display-b", &target.location)
                    },
                    source: EngineObservationSource::SyntheticProvider,
                    state: state.clone(),
                    diagnostic: None,
                },
                Duration::ZERO,
            );
            assert_eq!(calls.get(), 1);
            if exact_target && settled && state == EngineObservationState::ConfirmedAbsent {
                assert_eq!(result, Ok(()));
            } else {
                assert!(matches!(
                    result,
                    Err(EngineCloseFailure::DispatchedOrUnknown(_))
                ));
            }
        }
        let failed = close_wallpaper_with(
            supported,
            &target,
            || Ok(()),
            || {
                Err(EngineControlFailure::BeforeDispatch(
                    "Known spawn failure".into(),
                ))
            },
            || true,
            |_| panic!("No readback after zero-dispatch failure"),
            Duration::ZERO,
        );
        assert!(matches!(failed, Err(EngineCloseFailure::BeforeDispatch(_))));
        let unknown = close_wallpaper_with(
            supported,
            &target,
            || Ok(()),
            || {
                Err(EngineControlFailure::DispatchedOrUnknown(
                    "Actual child uncertainty".into(),
                ))
            },
            || true,
            |_| panic!("Dispatch uncertainty is not confirmation"),
            Duration::ZERO,
        );
        assert!(matches!(
            unknown,
            Err(EngineCloseFailure::DispatchedOrUnknown(_))
        ));
    }
}

//! Private, one-shot transport for Engine's console-only getWallpaper output.
use super::*;
use serde::{Deserialize, Serialize};
use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::ffi::OsStringExt;
use std::os::windows::io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle};
use std::os::windows::process::CommandExt;
use windows_sys::Win32::Foundation::{SetHandleInformation, HANDLE_FLAG_INHERIT};
use windows_sys::Win32::System::Console::*;
use windows_sys::Win32::System::JobObjects::*;
use windows_sys::Win32::System::Pipes::PeekNamedPipe;
use windows_sys::Win32::System::SystemInformation::{GetSystemDirectoryW, GetTickCount64};
use windows_sys::Win32::System::Threading::{CREATE_NO_WINDOW, DETACHED_PROCESS};
use windows_sys::Win32::UI::WindowsAndMessaging::IsWindowVisible;

const FLAG: &str = "--internal-engine-readback";
const SETUP_LIMIT: Duration = Duration::from_millis(700);
// The Windows console rejects 32767 as a buffer dimension.
const ROW_WIDTH: i16 = i16::MAX - 1;

#[derive(Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum QueryPath {
    Path(String),
    Rejected(String),
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    child_pid: u32,
    path: QueryPath,
    console_units: usize,
    stdout_bytes: usize,
    stderr_bytes: usize,
    stderr_discarded: usize,
    stderr_prefix: String,
}

pub(super) fn readback(
    endpoint: &ControlEndpoint,
    arguments: &[OsString],
    timeout: Duration,
) -> Result<QueryPath, ChildRunError> {
    if active_control_endpoint().as_ref() != Ok(endpoint) {
        return Err(ChildRunError::before_start(
            "Engine endpoint changed before readback",
        ));
    }
    let [verb, selector, value] = arguments else {
        return Err(ChildRunError::before_start("Invalid readback arguments"));
    };
    if verb != "getWallpaper" || !valid_selector(selector, value) {
        return Err(ChildRunError::before_start("Invalid readback selector"));
    }
    let exe = std::env::current_exe().map_err(|e| ChildRunError::before_start(e.to_string()))?;
    // The ignored live integration test runs the same shipped helper entry point.
    #[cfg(test)]
    let exe = exe
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| ChildRunError::before_start("Missing sibling GPU executable"))?
        .join("wallpaper-switcher-gpu-candidate.exe");
    let started = Instant::now();
    let expires = unsafe { GetTickCount64() }.saturating_add(
        timeout
            .saturating_sub(Duration::from_millis(900))
            .as_millis() as u64,
    );
    let mut command = Command::new(exe);
    command
        .creation_flags(DETACHED_PROCESS)
        .arg(FLAG)
        .arg(endpoint.identity.0.to_string())
        .arg(endpoint.identity.1.to_string())
        .arg(&endpoint.executable)
        .arg(selector)
        .arg(value)
        .arg(expires.to_string());
    crate::apply_trace::event_lazy("provider-query-helper", || {
        format!(
            "phase=start {} args={}",
            trace_endpoint(endpoint),
            trace_arguments(arguments)
        )
    });
    let output = run_in_query_job(&mut command, timeout)?;
    let failed = |message| ChildRunError::after_start(message, output.process_id, Some(true));
    if !output.status.success() || output.stdout.discarded_bytes != 0 {
        return Err(failed(format!(
            "Engine readback helper exited {}: {}",
            output.status,
            output_text(&output.stderr)
        )));
    }
    let reply: Reply = serde_json::from_slice(&output.stdout.bytes)
        .map_err(|e| failed(format!("Invalid Engine readback reply: {e}")))?;
    if active_control_endpoint().as_ref() != Ok(endpoint) {
        return Err(failed("Engine endpoint changed during readback".into()));
    }
    if let QueryPath::Path(path) = &reply.path {
        if let Some(reason) = path_rejection(path) {
            return Err(failed(reason.into()));
        }
    }
    crate::apply_trace::event_lazy("provider-command-complete", || {
        format!(
        "transport=private-console elapsed_ms={} args={} helper_pid={} child_pid={} status=success path_rejected={} console_utf16={} stdout_bytes={} stderr_bytes={} stderr_discarded_bytes={} stderr_prefix={:?}",
        started.elapsed().as_millis(), trace_arguments(arguments), output.process_id, reply.child_pid, matches!(&reply.path, QueryPath::Rejected(_)), reply.console_units, reply.stdout_bytes, reply.stderr_bytes, reply.stderr_discarded, reply.stderr_prefix)
    });
    Ok(reply.path)
}

fn valid_selector(selector: &OsStr, value: &OsStr) -> bool {
    let Some(value) = value.to_str() else {
        return false;
    };
    match selector.to_str() {
        Some("-location") => {
            !value.is_empty() && !value.starts_with('-') && !value.chars().any(char::is_control)
        }
        Some("-monitor") => {
            !value.is_empty()
                && value.bytes().all(|c| c.is_ascii_digit())
                && value.parse::<usize>().is_ok()
        }
        _ => false,
    }
}

pub(crate) fn dispatch_readback_helper() {
    let mut args = std::env::args_os().skip(1);
    if args.next().as_deref() != Some(OsStr::new(FLAG)) {
        return;
    }
    let args: Vec<_> = args.collect();
    let [hwnd, pid, executable, selector, value, expires] = args.as_slice() else {
        std::process::exit(2)
    };
    let Some((hwnd, pid, expires)) = (|| {
        Some((
            hwnd.to_str()?.parse().ok()?,
            pid.to_str()?.parse().ok()?,
            expires.to_str()?.parse::<u64>().ok()?,
        ))
    })() else {
        std::process::exit(2)
    };
    let left = expires.saturating_sub(unsafe { GetTickCount64() });
    if left == 0 || left > 5000 || !valid_selector(selector, value) {
        std::process::exit(2);
    }
    let deadline = Instant::now() + Duration::from_millis(left);
    let endpoint = ControlEndpoint {
        identity: (hwnd, pid),
        executable: executable.into(),
    };
    let arguments = [
        OsString::from("getWallpaper"),
        selector.clone(),
        value.clone(),
    ];
    // Keep private IPC pipes independent from the standard handles established by AttachConsole.
    let Ok(mut response) = io::stdout()
        .as_handle()
        .try_clone_to_owned()
        .map(File::from)
    else {
        std::process::exit(2)
    };
    let Ok(mut errors) = io::stderr()
        .as_handle()
        .try_clone_to_owned()
        .map(File::from)
    else {
        std::process::exit(2)
    };
    let result = wait_for_release(deadline)
        .map_err(|e| e.to_string())
        .and_then(|_| {
            let screen =
                private_screen(deadline).map_err(|e| format!("Private readback console: {e}"))?;
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err("Readback deadline expired before dispatch".into());
            }
            let output = run_control_output_with(
                &endpoint,
                &arguments,
                remaining,
                || {},
                active_control_endpoint,
                || endpoint.command(&arguments),
            )
            .map_err(child_run_error_text)?;
            let console = screen_text(&screen).map_err(|e| format!("Console readback: {e}"))?;
            let reply = combine_output(&output, console)?;
            serde_json::to_writer(&mut response, &reply).map_err(|e| e.to_string())?;
            response.flush().map_err(|e| e.to_string())
        });
    let code = match result {
        Ok(()) => 0,
        Err(e) => {
            let _ = writeln!(errors, "{e}");
            1
        }
    };
    std::process::exit(code)
}

fn path_rejection(path: &str) -> Option<&'static str> {
    if path.strip_prefix(r"\\?\").unwrap_or(path).contains('?') {
        Some("Engine wallpaper path contains literal question marks")
    } else if path.chars().any(char::is_control) {
        Some("Engine wallpaper path contains a control character")
    } else if !path.is_empty() && !valid_observed_path(path) {
        Some("Engine query did not return one absolute path")
    } else {
        None
    }
}

fn combine_output(output: &ChildRunOutput, console: String) -> Result<Reply, String> {
    if output.stdout.discarded_bytes != 0 {
        return Err("Truncated Engine stdout".into());
    }
    let pipe = std::str::from_utf8(&output.stdout.bytes)
        .map_err(|_| "Invalid UTF-8 on Engine stdout")?
        .trim_end_matches(['\r', '\n']);
    if !console.is_empty() && !pipe.is_empty() && console != pipe {
        return Err("Conflicting Engine query outputs".into());
    }
    // A legacy screen-buffer read substitutes this character for glyphs it
    // cannot round-trip. Never publish the substituted spelling as evidence.
    if console.contains('\u{fffd}') {
        return Err("Console could not preserve the wallpaper path".into());
    }
    let path = if console.is_empty() { pipe } else { &console };
    // Send the rejection reason without the unusable path. The caller may try
    // another bound selector once the job and endpoint have been validated.
    Ok(Reply {
        path: match path_rejection(path) {
            Some(reason) => QueryPath::Rejected(reason.into()),
            None => QueryPath::Path(path.into()),
        },
        child_pid: output.process_id,
        console_units: console.encode_utf16().count(),
        stdout_bytes: output.stdout.bytes.len(),
        stderr_bytes: output.stderr.bytes.len(),
        stderr_discarded: output.stderr.discarded_bytes,
        stderr_prefix: String::from_utf8_lossy(&output.stderr.bytes)
            .chars()
            .take(1024)
            .collect(),
    })
}

// The parent owns the only job handle. The helper waits for a byte on its
// private stdin before it may start the console bootstrap or the actual query.
// A parent crash closes both the pipe and the kill-on-close job.
pub(super) struct QueryJob(OwnedHandle);

impl QueryJob {
    fn new() -> io::Result<Self> {
        unsafe {
            let raw = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if raw.is_null() {
                return Err(io::Error::last_os_error());
            }
            let job = Self(OwnedHandle::from_raw_handle(raw));
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            if SetInformationJobObject(
                raw,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            ) == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(job)
        }
    }

    fn release_helper(&self, child: &mut Child) -> io::Result<()> {
        if unsafe { AssignProcessToJobObject(self.0.as_raw_handle(), child.as_raw_handle()) } == 0 {
            return Err(io::Error::last_os_error());
        }
        child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("No readback start pipe"))?
            .write_all(b"Q")
    }

    pub(super) fn is_empty(&self) -> io::Result<bool> {
        let mut info: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION = unsafe { std::mem::zeroed() };
        if unsafe {
            QueryInformationJobObject(
                self.0.as_raw_handle(),
                JobObjectBasicAccountingInformation,
                std::ptr::from_mut(&mut info).cast(),
                std::mem::size_of_val(&info) as u32,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(info.ActiveProcesses == 0)
    }

    pub(super) fn settle(&self, deadline: Instant) -> io::Result<bool> {
        if matches!(self.is_empty(), Ok(true)) {
            return Ok(true);
        }
        if unsafe { TerminateJobObject(self.0.as_raw_handle(), 1) } == 0 {
            let failure = io::Error::last_os_error();
            return if self.is_empty()? {
                Ok(true)
            } else {
                Err(failure)
            };
        }
        while Instant::now() < deadline {
            if self.is_empty()? {
                return Ok(true);
            }
            thread::sleep(Duration::from_millis(4));
        }
        self.is_empty()
    }
}

fn wait_for_release(deadline: Instant) -> io::Result<()> {
    // Prevent descendants from prolonging either end of the helper protocol.
    for handle in [
        io::stdin().as_raw_handle(),
        io::stdout().as_raw_handle(),
        io::stderr().as_raw_handle(),
    ] {
        if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    loop {
        if Instant::now() >= deadline {
            return Err(io::Error::other("Readback expired before release"));
        }
        let mut available = 0;
        if unsafe {
            PeekNamedPipe(
                io::stdin().as_raw_handle(),
                std::ptr::null_mut(),
                0,
                std::ptr::null_mut(),
                &mut available,
                std::ptr::null_mut(),
            )
        } == 0
        {
            return Err(io::Error::last_os_error());
        }
        if available != 0 {
            break;
        }
        thread::sleep(Duration::from_millis(4));
    }
    let mut token = [0];
    io::stdin().read_exact(&mut token)?;
    if token != *b"Q" {
        return Err(io::Error::other("Invalid readback start byte"));
    }
    Ok(())
}

fn run_in_query_job(
    command: &mut Command,
    timeout: Duration,
) -> Result<ChildRunOutput, ChildRunError> {
    let deadline = Instant::now() + timeout;
    let job = QueryJob::new()
        .map_err(|e| ChildRunError::before_start(format!("Could not create readback job: {e}")))?;
    let mut helper_pid = 0;
    let result = run_command_after_spawn(
        command.stdin(Stdio::piped()),
        timeout.saturating_sub(Duration::from_millis(300)),
        "Engine readback helper",
        |child| {
            helper_pid = child.id();
            job.release_helper(child)
                .map_err(|e| format!("Could not release contained readback helper: {e}"))
        },
    );
    if job.settle(deadline).unwrap_or(false) {
        return result;
    }
    register_unsettled_child(UnsettledChild {
        process_id: helper_pid,
        child: PendingProcess::ReadbackJob(job),
    });
    let mut error = result.err().unwrap_or_else(|| {
        ChildRunError::after_start("Readback job did not settle", helper_pid, None)
    });
    error.message = format!(
        "provider-state-unknown: readback job for helper {helper_pid} remains unsettled: {}",
        error.message
    );
    error.child_stopped = None;
    Err(error)
}

fn private_screen(deadline: Instant) -> io::Result<File> {
    // On Windows 10 and 11 CREATE_NO_WINDOW gives a console-subsystem process
    // a private console. The system sort utility waits for EOF on our empty
    // pipe and exits before Engine starts. The GUI process never attaches.
    let mut dir = [0u16; 32768];
    let len = unsafe { GetSystemDirectoryW(dir.as_mut_ptr(), dir.len() as u32) } as usize;
    if len == 0 || len >= dir.len() {
        return Err(io::Error::other("System directory unavailable"));
    }
    let exe = PathBuf::from(OsString::from_wide(&dir[..len])).join("sort.exe");
    let mut anchor = Command::new(exe)
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let until = deadline.min(Instant::now() + SETUP_LIMIT);
    loop {
        if unsafe { AttachConsole(anchor.id()) } != 0 {
            break;
        }
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(6)
            || Instant::now() >= until
            || anchor.try_wait()?.is_some()
        {
            return Err(error);
        }
        thread::sleep(Duration::from_millis(4));
    }
    let screen = OpenOptions::new().read(true).write(true).open("CONOUT$")?;
    drop(anchor.stdin.take());
    while anchor.try_wait()?.is_none() {
        if Instant::now() >= until {
            return Err(io::Error::other("Console bootstrap failed to exit"));
        }
        thread::sleep(Duration::from_millis(4));
    }
    let mut owners = [0; 2];
    let info = screen_info(&screen)?;
    if unsafe { GetConsoleProcessList(owners.as_mut_ptr(), 2) } != 1
        || owners[0] != std::process::id()
        || unsafe { IsWindowVisible(GetConsoleWindow()) } != 0
        || info.dwCursorPosition.X != 0
        || info.dwCursorPosition.Y != 0
    {
        return Err(io::Error::other(
            "Readback console is not private, hidden and empty",
        ));
    }
    // Keep the single-line response unambiguous. A wrap onto another row
    // cannot be distinguished from a second response.
    let height = info.srWindow.Bottom.saturating_add(1).max(2);
    if height > 128
        || unsafe {
            SetConsoleScreenBufferSize(
                screen.as_raw_handle(),
                COORD {
                    X: ROW_WIDTH,
                    Y: height,
                },
            )
        } == 0
    {
        return Err(io::Error::other("Could not bound the readback console"));
    }
    Ok(screen)
}

fn screen_info(screen: &File) -> io::Result<CONSOLE_SCREEN_BUFFER_INFO> {
    let mut info = unsafe { std::mem::zeroed() };
    if unsafe { GetConsoleScreenBufferInfo(screen.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(info)
}

fn screen_text(screen: &File) -> io::Result<String> {
    let info = screen_info(screen)?;
    let cursor = info.dwCursorPosition;
    if cursor.Y != 0 || cursor.X < 0 || info.dwSize.X != ROW_WIDTH {
        return Err(io::Error::other("Multiline or overflowing readback"));
    }
    if cursor.X == 0 {
        return Ok(String::new());
    }
    let mut text = vec![0u16; cursor.X as usize];
    let mut read = 0;
    if unsafe {
        ReadConsoleOutputCharacterW(
            screen.as_raw_handle(),
            text.as_mut_ptr(),
            text.len() as u32,
            COORD { X: 0, Y: 0 },
            &mut read,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    text.truncate(read as usize);
    String::from_utf16(&text).map_err(|_| io::Error::other("Invalid UTF-16 in console readback"))
}

#[cfg(test)]
#[path = "query_console_tests.rs"]
mod tests;

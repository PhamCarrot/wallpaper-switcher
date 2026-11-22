use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use windows::Graphics::Capture::GraphicsCaptureItem;
use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
use windows::Win32::Graphics::Direct3D11::{ID3D11Device, ID3D11DeviceContext};
use windows::Win32::Graphics::Dxgi::IDXGIDevice;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows_sys::Win32::UI::HiDpi::{
    SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetAncestor, GetClassNameW, GetDesktopWindow, GetShellWindow, GetWindowRect,
    GetWindowTextW, GetWindowThreadProcessId, IsChild, IsWindowVisible, GA_PARENT, GA_ROOT,
};

use crate::desktop_transition::desktop_icon_view;
use crate::monitor::DesktopRect;
use crate::transition::FrozenFrame;

#[derive(Debug, Clone)]
struct EngineWindow {
    pub hwnd: HWND,
    pub class_name: String,
    pub title: String,
    pub rect: DesktopRect,
    pub visible: bool,
}

impl EngineWindow {
    fn overlap_area(&self, target: DesktopRect) -> i64 {
        let width = (self.rect.right.min(target.right) - self.rect.left.max(target.left)).max(0);
        let height = (self.rect.bottom.min(target.bottom) - self.rect.top.max(target.top)).max(0);
        width as i64 * height as i64
    }
}

struct Search {
    process_id: u32,
    windows: Vec<EngineWindow>,
}

fn windows_for_process(process_id: u32) -> Vec<EngineWindow> {
    let mut search = Search {
        process_id,
        windows: Vec::new(),
    };
    unsafe {
        EnumChildWindows(
            GetDesktopWindow(),
            Some(collect_engine_window),
            &mut search as *mut Search as LPARAM,
        );
    }
    search.windows
}

fn presenter_overlap(window: &EngineWindow, target: DesktopRect) -> Result<i64, &'static str> {
    if !window.visible {
        return Err("hidden");
    }
    if window.class_name == "WPEEventWindow"
        || (window.class_name != "WPEVideoWallpaper"
            && !window.class_name.starts_with("WPEDesktop")
            && window.title != "WPELiveWallpaper")
    {
        return Err("not-a-presenter");
    }
    let overlap = window.overlap_area(target);
    if overlap == 0 || overlap != i64::from(target.width()) * i64::from(target.height()) {
        return Err("wrong-display");
    }
    Ok(overlap)
}

#[derive(Debug, Clone)]
struct ApplyCapture {
    operation: u64,
    display: String,
    phase: &'static str,
    endpoint: (usize, u32),
    overlay_windows: Vec<usize>,
    progress: Arc<Mutex<String>>,
    // Incoming captures must differ from the frozen outgoing frame.
    outgoing: Option<FrameFingerprint>,
}

// Mean 8×4 luma difference (0–255) below which an incoming frame is treated
// as the outgoing wallpaper still on screen.
const UNCHANGED_FRAME_DIFFERENCE: f32 = 1.0;

fn incoming_frame_is_stale(frame: &FrozenFrame, outgoing: &FrameFingerprint) -> Option<String> {
    if !has_visible_detail(&frame.rgba) {
        return Some("incoming frame is uniform and may predate the new wallpaper".into());
    }
    let difference = FrameFingerprint::of(frame).mean_difference(outgoing);
    (difference < UNCHANGED_FRAME_DIFFERENCE).then(|| {
        format!("incoming frame still matches the outgoing wallpaper (difference {difference:.2})")
    })
}

pub(crate) fn capture_for_apply(
    target: DesktopRect,
    operation: u64,
    display: &str,
    phase: &'static str,
    endpoint: (usize, u32),
    overlay_windows: &[usize],
    outgoing: Option<FrameFingerprint>,
) -> Result<FrozenFrame, String> {
    let details = ApplyCapture {
        operation,
        display: display.into(),
        phase,
        endpoint,
        overlay_windows: overlay_windows.to_vec(),
        progress: Arc::default(),
        outgoing,
    };
    crate::apply_trace::event_for_lazy(operation, "transition-engine-capture-start", || {
        format!("display={display:?} phase={phase} endpoint={endpoint:?} target={target:?}")
    });
    let result = capture_frame_with_worker(
        target,
        &CAPTURE_BUSY,
        &CAPTURE_WORKER,
        capture_deadline(phase),
        Some(details),
        |receiver| {
            std::thread::Builder::new()
                .name("wallpaper-engine-capture".to_string())
                .spawn(move || capture_worker(receiver))
                .map(|_| ())
                .map_err(|error| format!("Could not start the capture worker: {error}"))
        },
    );
    crate::apply_trace::event_for_lazy(operation, "transition-engine-capture-result", || {
        let outcome = match &result {
            Ok(frame) => format!("frame={}x{}", frame.width, frame.height),
            Err(error) => format!("error={error:?}"),
        };
        format!("display={display:?} phase={phase} {outcome}")
    });
    result
}

type CaptureResponse = Result<FrozenFrame, String>;
type CaptureRequest = (
    DesktopRect,
    mpsc::SyncSender<CaptureResponse>,
    CaptureBusyGuard,
    Instant,
    Option<ApplyCapture>,
);

static CAPTURE_WORKER: Mutex<Option<mpsc::SyncSender<CaptureRequest>>> = Mutex::new(None);
static CAPTURE_BUSY: AtomicBool = AtomicBool::new(false);
const CAPTURE_DEADLINE: Duration = Duration::from_secs(5);
// Apply captures hold either a pending switch or a frozen cover over the live
// wallpaper. Successful readback took 59–275 ms in the 22 November trace.
const APPLY_CAPTURE_DEADLINE: Duration = Duration::from_millis(1_500);
// A structural refusal that repeats unchanged is not waiting on the provider.
const PERMANENT_REFUSAL_ATTEMPTS: u32 = 3;
const STRUCTURAL_REFUSAL: &str = "WorkerW unavailable: ";

fn capture_deadline(phase: &str) -> Duration {
    if matches!(phase, "outgoing" | "incoming") {
        APPLY_CAPTURE_DEADLINE
    } else {
        CAPTURE_DEADLINE
    }
}

fn is_permanent_refusal(reason: &str) -> bool {
    reason.contains(STRUCTURAL_REFUSAL) && !reason.contains("no longer contains")
}

/// Coarse 8×4 luma grid. Compares outgoing and incoming frames in the trace
/// without retaining pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FrameFingerprint([u8; 32]);

impl FrameFingerprint {
    pub(crate) fn of(frame: &FrozenFrame) -> Self {
        let (width, height) = (frame.width.max(1) as usize, frame.height.max(1) as usize);
        let mut sums = [0u64; 32];
        let mut counts = [0u64; 32];
        for y in (0..height).step_by(4) {
            let row = y * 4 / height;
            for x in (0..width).step_by(4) {
                let Some(pixel) = frame.rgba.get((y * width + x) * 4..(y * width + x) * 4 + 3)
                else {
                    continue;
                };
                let cell = row * 8 + x * 8 / width;
                sums[cell] += (u64::from(pixel[0]) * 54
                    + u64::from(pixel[1]) * 183
                    + u64::from(pixel[2]) * 19)
                    >> 8;
                counts[cell] += 1;
            }
        }
        Self(std::array::from_fn(|cell| {
            (sums[cell] / counts[cell].max(1)) as u8
        }))
    }

    pub(crate) fn mean_difference(&self, other: &Self) -> f32 {
        self.0
            .iter()
            .zip(other.0)
            .map(|(a, b)| f32::from(a.abs_diff(b)))
            .sum::<f32>()
            / 32.0
    }
}

impl std::fmt::Display for FrameFingerprint {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0
            .iter()
            .try_for_each(|value| write!(formatter, "{value:02x}"))
    }
}

/// Visible windows under the presenter's parent, for render-tree changes.
fn subtree_summary(presenter: HWND) -> String {
    struct Found(Vec<String>);
    unsafe extern "system" fn collect(hwnd: HWND, param: LPARAM) -> BOOL {
        if IsWindowVisible(hwnd) != 0 {
            let mut process = 0;
            GetWindowThreadProcessId(hwnd, &mut process);
            (*(param as *mut Found))
                .0
                .push(format!("{}@{process}", window_string(hwnd, GetClassNameW)));
        }
        1
    }
    let parent = unsafe { GetAncestor(presenter, GA_PARENT) };
    let mut found = Found(Vec::new());
    unsafe { EnumChildWindows(parent, Some(collect), (&mut found as *mut Found) as LPARAM) };
    found.0.join(",")
}

struct CaptureBusyGuard(&'static AtomicBool);

impl CaptureBusyGuard {
    fn acquire(busy: &'static AtomicBool) -> Result<Self, String> {
        busy.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| {
                "The Wallpaper Engine capture worker is still completing an earlier capture"
                    .to_string()
            })?;
        Ok(Self(busy))
    }
}

impl Drop for CaptureBusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureSubmissionError {
    Deadline,
    Disconnected,
}

fn submit_capture_request(
    sender: &mpsc::SyncSender<CaptureRequest>,
    mut request: CaptureRequest,
    deadline: Instant,
) -> Result<(), CaptureSubmissionError> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(CaptureSubmissionError::Deadline);
        }
        match sender.try_send(request) {
            Ok(()) => return Ok(()),
            Err(mpsc::TrySendError::Full(returned_request)) => {
                request = returned_request;
                std::thread::sleep(remaining.min(Duration::from_millis(1)));
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                return Err(CaptureSubmissionError::Disconnected);
            }
        }
    }
}

fn capture_frame_with_worker(
    target: DesktopRect,
    busy: &'static AtomicBool,
    worker: &Mutex<Option<mpsc::SyncSender<CaptureRequest>>>,
    timeout: Duration,
    capture: Option<ApplyCapture>,
    start_worker: impl FnOnce(mpsc::Receiver<CaptureRequest>) -> Result<(), String>,
) -> CaptureResponse {
    let deadline = Instant::now() + timeout;
    let busy = CaptureBusyGuard::acquire(busy)?;
    let progress = capture
        .as_ref()
        .map(|details| Arc::clone(&details.progress));
    // Keep disconnection cleanup bound to this sender, even if the worker
    // releases busy before this caller receives its response.
    let mut worker = worker.lock().unwrap_or_else(|error| error.into_inner());
    let sender = match &*worker {
        Some(sender) => sender,
        None => {
            let (sender, receiver) = mpsc::sync_channel::<CaptureRequest>(0);
            start_worker(receiver)?;
            worker.insert(sender)
        }
    };
    let (response_sender, response_receiver) = mpsc::sync_channel(1);
    if let Err(error) = submit_capture_request(
        sender,
        (target, response_sender, busy, deadline, capture),
        deadline,
    ) {
        if error == CaptureSubmissionError::Disconnected {
            *worker = None;
        }
        return Err(match error {
            CaptureSubmissionError::Deadline => format!(
                "Wallpaper Engine capture worker was not ready before its {:.1}-second deadline",
                timeout.as_secs_f32()
            ),
            CaptureSubmissionError::Disconnected => {
                "The Wallpaper Engine capture worker stopped".to_string()
            }
        });
    }
    // The submitted request owns busy now. A caller timeout must not release
    // another capture while the worker is still using its graphics resources.
    match response_receiver.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
        Ok(result) => result,
        Err(error) => Err(match error {
            mpsc::RecvTimeoutError::Timeout => match progress {
                Some(progress) => {
                    let stage = progress.lock().unwrap_or_else(|error| error.into_inner());
                    if stage.is_empty() {
                        "Wallpaper Engine timed out before the capture worker began processing"
                            .into()
                    } else {
                        format!("Wallpaper Engine capture deadline exceeded: {stage}")
                    }
                }
                None => format!(
                    "Wallpaper Engine frame capture exceeded its {:.1}-second deadline",
                    timeout.as_secs_f32()
                ),
            },
            mpsc::RecvTimeoutError::Disconnected => {
                *worker = None;
                "The Wallpaper Engine capture worker did not return a frame".to_string()
            }
        }),
    }
}

#[cfg(test)]
mod capture_submission_tests {
    use super::*;

    fn target() -> DesktopRect {
        DesktopRect {
            left: 0,
            top: 0,
            right: 1,
            bottom: 1,
        }
    }

    fn frame() -> FrozenFrame {
        FrozenFrame {
            width: 1,
            height: 1,
            rgba: vec![1, 2, 3, 255],
        }
    }

    #[test]
    fn busy_guard_rejects_a_second_owner_and_releases_on_unwind() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        let result = std::panic::catch_unwind(|| {
            let _busy = CaptureBusyGuard::acquire(&BUSY).unwrap();
            assert!(CaptureBusyGuard::acquire(&BUSY).is_err());
            assert!(BUSY.load(Ordering::Acquire));
            panic!("controlled capture owner unwind");
        });
        assert!(result.is_err());
        assert!(!BUSY.load(Ordering::Acquire));
        drop(CaptureBusyGuard::acquire(&BUSY).unwrap());
        assert!(!BUSY.load(Ordering::Acquire));
    }

    #[test]
    fn submission_deadline_releases_the_unaccepted_request() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        let worker = Mutex::new(None);
        let mut held_receiver = None;
        let result =
            capture_frame_with_worker(target(), &BUSY, &worker, Duration::ZERO, None, |rx| {
                held_receiver = Some(rx);
                Ok(())
            });
        assert!(result.unwrap_err().contains("not ready"));
        assert!(!BUSY.load(Ordering::Acquire));
        assert!(worker.lock().unwrap().is_some());
        drop(held_receiver);
    }

    #[test]
    fn startup_failure_releases_busy_and_allows_retry() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        let worker = Mutex::new(None);
        for _ in 0..2 {
            let result = capture_frame_with_worker(
                target(),
                &BUSY,
                &worker,
                Duration::from_secs(1),
                None,
                |_| Err("controlled startup failure".into()),
            );
            assert_eq!(result.unwrap_err(), "controlled startup failure");
            assert!(!BUSY.load(Ordering::Acquire));
            assert!(worker.lock().unwrap().is_none());
        }
    }

    #[test]
    fn capture_error_and_success_release_busy_and_reuse_the_worker() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        let worker = Mutex::new(None);
        let mut thread = None;
        let result = capture_frame_with_worker(
            target(),
            &BUSY,
            &worker,
            Duration::from_secs(1),
            None,
            |rx| {
                thread = Some(std::thread::spawn(move || {
                    let request = rx.recv().unwrap();
                    respond_to_capture(request, |_, _, _| Err("controlled capture failure".into()));
                    let request = rx.recv().unwrap();
                    respond_to_capture(request, |_, _, _| Ok(frame()));
                }));
                Ok(())
            },
        );
        assert_eq!(result.unwrap_err(), "controlled capture failure");
        assert!(!BUSY.load(Ordering::Acquire));
        let result = capture_frame_with_worker(
            target(),
            &BUSY,
            &worker,
            Duration::from_secs(1),
            None,
            |_| panic!("a capture error must retain the worker"),
        );
        assert_eq!(result.unwrap(), frame());
        thread.unwrap().join().unwrap();
        assert!(!BUSY.load(Ordering::Acquire));
    }

    fn assert_replacement_succeeds(
        busy: &'static AtomicBool,
        worker: &Mutex<Option<mpsc::SyncSender<CaptureRequest>>>,
    ) {
        assert!(!busy.load(Ordering::Acquire));
        assert!(worker.lock().unwrap().is_none());
        let mut thread = None;
        let result =
            capture_frame_with_worker(target(), busy, worker, Duration::from_secs(1), None, |rx| {
                thread = Some(std::thread::spawn(move || {
                    respond_to_capture(rx.recv().unwrap(), |_, _, _| Ok(frame()));
                }));
                Ok(())
            });
        assert_eq!(result.unwrap(), frame());
        thread.unwrap().join().unwrap();
        assert!(!busy.load(Ordering::Acquire));
    }

    #[test]
    fn disconnected_submission_clears_the_slot_and_allows_replacement() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        let (sender, receiver) = mpsc::sync_channel(0);
        drop(receiver);
        let worker = Mutex::new(Some(sender));
        let result = capture_frame_with_worker(
            target(),
            &BUSY,
            &worker,
            Duration::from_secs(1),
            None,
            |_| panic!("the disconnected sender is still installed until submission"),
        );
        assert!(result.unwrap_err().contains("worker stopped"));
        assert_replacement_succeeds(&BUSY, &worker);
    }

    #[test]
    fn worker_panic_releases_busy_and_allows_replacement() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        let worker = Mutex::new(None);
        let mut thread = None;
        let result = capture_frame_with_worker(
            target(),
            &BUSY,
            &worker,
            Duration::from_secs(1),
            None,
            |rx| {
                thread = Some(std::thread::spawn(move || {
                    respond_to_capture(rx.recv().unwrap(), |_, _, _| {
                        panic!("controlled capture panic")
                    });
                }));
                Ok(())
            },
        );
        assert!(result.unwrap_err().contains("did not return a frame"));
        assert!(thread.unwrap().join().is_err());
        assert_replacement_succeeds(&BUSY, &worker);
    }

    #[test]
    fn caller_timeout_keeps_busy_until_the_worker_settles_then_reuses_it() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        let worker = Mutex::new(None);
        let (release_tx, release_rx) = mpsc::channel();
        let (settled_tx, settled_rx) = mpsc::channel();
        let mut thread = None;
        let result = capture_frame_with_worker(
            target(),
            &BUSY,
            &worker,
            Duration::from_millis(100),
            Some(ApplyCapture {
                operation: 0,
                display: "test-display".into(),
                phase: "incoming",
                endpoint: (0, 0),
                overlay_windows: Vec::new(),
                progress: Arc::default(),
                outgoing: None,
            }),
            |rx| {
                thread = Some(std::thread::spawn(move || {
                    respond_to_capture(rx.recv().unwrap(), |_, _, details| {
                        record_stage(&details.unwrap().progress, "waiting for an incoming frame");
                        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                        Ok(frame())
                    });
                    settled_tx.send(()).unwrap();
                    respond_to_capture(rx.recv().unwrap(), |_, _, _| Ok(frame()));
                }));
                Ok(())
            },
        );
        let error = result.unwrap_err();
        assert!(error.contains("deadline exceeded"));
        assert!(error.contains("waiting for an incoming frame"));
        assert!(BUSY.load(Ordering::Acquire));
        let refused = capture_frame_with_worker(
            target(),
            &BUSY,
            &worker,
            Duration::from_secs(1),
            None,
            |_| panic!("a timed-out active capture must not be replaced"),
        );
        assert!(refused.unwrap_err().contains("earlier capture"));
        assert!(BUSY.load(Ordering::Acquire));
        release_tx.send(()).unwrap();
        settled_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(!BUSY.load(Ordering::Acquire));
        let recovered = capture_frame_with_worker(
            target(),
            &BUSY,
            &worker,
            Duration::from_secs(1),
            None,
            |_| panic!("the settled worker must be reused"),
        );
        assert_eq!(recovered.unwrap(), frame());
        thread.unwrap().join().unwrap();
        assert!(!BUSY.load(Ordering::Acquire));
    }

    #[test]
    fn submission_retries_until_the_rendezvous_worker_is_receiving() {
        static BUSY: AtomicBool = AtomicBool::new(false);
        let (sender, receiver) = mpsc::sync_channel(0);
        let target = DesktopRect {
            left: 10,
            top: 20,
            right: 30,
            bottom: 40,
        };
        let (response_sender, _response_receiver) = mpsc::sync_channel(1);
        let request = (
            target,
            response_sender,
            CaptureBusyGuard::acquire(&BUSY).unwrap(),
            Instant::now() + Duration::from_secs(1),
            None,
        );
        let request = match sender.try_send(request) {
            Err(mpsc::TrySendError::Full(request)) => request,
            _ => panic!("an unserviced rendezvous channel must reject"),
        };

        let (start_tx, start_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            start_rx
                .recv_timeout(Duration::from_secs(1))
                .expect("test worker start signal");
            receiver
                .recv_timeout(Duration::from_secs(1))
                .expect("capture request should arrive after the worker starts receiving")
                .0
        });
        let release_worker = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(20));
            start_tx.send(()).expect("test worker is waiting");
        });

        submit_capture_request(&sender, request, Instant::now() + Duration::from_secs(1))
            .expect("submission should wait for the worker instead of dropping the first request");

        release_worker.join().expect("release thread should finish");
        let received_target = worker.join().expect("worker should receive one request");
        assert_eq!(received_target.left, target.left);
        assert_eq!(received_target.top, target.top);
        assert_eq!(received_target.right, target.right);
        assert_eq!(received_target.bottom, target.bottom);
    }
}

fn capture_worker(receiver: mpsc::Receiver<CaptureRequest>) {
    use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};

    struct WinRtApartment;
    impl Drop for WinRtApartment {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }

    let initialized = unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
        .map(|_| WinRtApartment)
        .map_err(|error| format!("WinRT initialization failed: {error}"));
    for request in receiver {
        respond_to_capture(request, |target, deadline, details| {
            match (&initialized, details) {
                (Ok(_), Some(details)) => {
                    crate::apply_trace::with_operation(details.operation, || {
                        capture_frame_on_worker(target, deadline, &details)
                    })
                }
                (Err(error), _) => Err(error.clone()),
                (Ok(_), None) => {
                    Err("The capture request has no verified provider identity".into())
                }
            }
        });
    }
}

fn respond_to_capture(
    (target, response, busy, deadline, details): CaptureRequest,
    capture: impl FnOnce(DesktopRect, Instant, Option<ApplyCapture>) -> CaptureResponse,
) {
    let result = capture(target, deadline, details);
    drop(busy);
    let _ = response.send(result);
}

#[derive(Debug, PartialEq, Eq)]
struct CaptureWindow {
    presenter: HWND,
    presenter_rect: DesktopRect,
    root: HWND,
    source_rect: DesktopRect,
}

struct PhysicalWindowCoordinates(DPI_AWARENESS_CONTEXT);

impl PhysicalWindowCoordinates {
    fn enter() -> Result<Self, String> {
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous.is_null() {
            return Err(last_error(
                "Could not use physical window coordinates for capture",
            ));
        }
        Ok(Self(previous))
    }
}

impl Drop for PhysicalWindowCoordinates {
    fn drop(&mut self) {
        unsafe { SetThreadDpiAwarenessContext(self.0) };
    }
}

impl CaptureWindow {
    fn from_presenter(window: &EngineWindow) -> Option<Self> {
        let root = unsafe { GetAncestor(window.hwnd, GA_ROOT) };
        if root.is_null() {
            return None;
        }
        Some(Self {
            presenter: window.hwnd,
            presenter_rect: window.rect,
            root,
            source_rect: capture_root_rect(root)?,
        })
    }

    fn current(&self, target: DesktopRect, endpoint: (usize, u32)) -> bool {
        crate::engine_ipc::control_window_identity().as_ref() == Ok(&endpoint)
            && inspect_engine_window(self.presenter, endpoint.1).is_some_and(|window| {
                presenter_overlap(&window, target).is_ok()
                    && Self::from_presenter(&window).as_ref() == Some(self)
            })
    }
}

enum PresenterAttempt<T> {
    Ready(T),
    Retry(String),
}

fn wait_for_presenter<T>(
    deadline: Instant,
    mut attempt: impl FnMut() -> Result<PresenterAttempt<T>, String>,
) -> Result<T, String> {
    let mut last_rejection = None;
    loop {
        if Instant::now() >= deadline {
            return Err(last_rejection.unwrap_or_else(|| {
                "The presenter did not become ready before the capture deadline".into()
            }));
        }
        match attempt()? {
            PresenterAttempt::Ready(ready) => return Ok(ready),
            PresenterAttempt::Retry(reason) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(reason);
                }
                last_rejection = Some(reason);
                std::thread::sleep(remaining.min(Duration::from_millis(30)));
            }
        }
    }
}

fn current_capture_window(
    target: DesktopRect,
    details: &ApplyCapture,
    log_candidates: bool,
) -> Result<PresenterAttempt<CaptureWindow>, String> {
    if crate::engine_ipc::control_window_identity().as_ref() != Ok(&details.endpoint) {
        return Err(
            "Wallpaper Engine's verified control owner changed before frame capture".into(),
        );
    }
    let mut eligible = Vec::new();
    for window in windows_for_process(details.endpoint.1) {
        let candidate = (|| {
            presenter_overlap(&window, target)?;
            let candidate = CaptureWindow::from_presenter(&window).ok_or("root-unavailable")?;
            if !covers(candidate.source_rect, target) {
                return Err("root-does-not-cover-display");
            }
            Ok(candidate)
        })();
        if log_candidates {
            crate::apply_trace::event_lazy("transition-engine-presenter-candidate", || {
                format!(
                    "display={:?} phase={} hwnd={:?} class={:?} title={:?} rect={:?} selection={candidate:?}",
                    details.display, details.phase, window.hwnd, window.class_name, window.title, window.rect,
                )
            });
        }
        if let Ok(candidate) = candidate {
            eligible.push(candidate);
        }
    }
    Ok(choose_presenter(eligible, target))
}

fn covers(outer: DesktopRect, inner: DesktopRect) -> bool {
    outer.left <= inner.left
        && outer.top <= inner.top
        && outer.right >= inner.right
        && outer.bottom >= inner.bottom
}

fn choose_presenter(
    mut eligible: Vec<CaptureWindow>,
    target: DesktopRect,
) -> PresenterAttempt<CaptureWindow> {
    if eligible
        .iter()
        .any(|window| window.presenter_rect == target)
    {
        eligible.retain(|window| window.presenter_rect == target);
    }
    match eligible.len() {
        0 => PresenterAttempt::Retry(
            "No rendered Wallpaper Engine window covers the target monitor".into(),
        ),
        1 => PresenterAttempt::Ready(eligible.pop().unwrap()),
        _ => PresenterAttempt::Retry(
            "Multiple Wallpaper Engine presenters cover the target monitor".into(),
        ),
    }
}

fn shares_capture_root(root: HWND, target: DesktopRect, overlay_windows: &[usize]) -> bool {
    overlay_windows.iter().copied().any(|hwnd| {
        let hwnd = hwnd as HWND;
        let visible = unsafe { IsWindowVisible(hwnd) != 0 && GetAncestor(hwnd, GA_ROOT) == root };
        visible
            && window_rect(hwnd).is_some_and(|overlay| {
                overlay.left < target.right
                    && overlay.right > target.left
                    && overlay.top < target.bottom
                    && overlay.bottom > target.top
            })
    })
}

fn contains_overlay(presenter: HWND, overlays: &[usize]) -> bool {
    overlays
        .iter()
        .any(|hwnd| unsafe { *hwnd as HWND == presenter || IsChild(presenter, *hwnd as HWND) != 0 })
}

#[derive(Debug, PartialEq, Eq)]
struct WallpaperWorker {
    hwnd: HWND,
    rect: DesktopRect,
    thread: u32,
    process: u32,
}

fn wallpaper_worker(
    window: &CaptureWindow,
    target: DesktopRect,
    overlays: &[usize],
    shell: HWND,
) -> Result<WallpaperWorker, &'static str> {
    if shell.is_null() || window.root != shell || desktop_icon_view(shell).is_null() {
        return Err("presenter is not under the shell desktop with a sibling icon view");
    }
    let hwnd = unsafe { GetAncestor(window.presenter, GA_PARENT) };
    let mut shell_process = 0;
    let mut process = 0;
    let thread = unsafe {
        GetWindowThreadProcessId(shell, &mut shell_process);
        GetWindowThreadProcessId(hwnd, &mut process)
    };
    if thread == 0
        || process == 0
        || process != shell_process
        || unsafe { GetAncestor(hwnd, GA_PARENT) } != shell
        || unsafe { window_string(hwnd, GetClassNameW) } != "WorkerW"
        || unsafe { IsWindowVisible(hwnd) } == 0
    {
        return Err("presenter has no visible intermediate WorkerW owned by the shell");
    }
    let rect = window_rect(hwnd)
        .filter(|rect| covers(*rect, target))
        .ok_or("WorkerW does not cover the physical target display")?;
    if contains_overlay(hwnd, overlays) {
        return Err("WorkerW contains the outgoing transition cover");
    }

    struct Children {
        presenter: HWND,
        found: bool,
        rejection: Option<&'static str>,
    }
    unsafe extern "system" fn inspect(hwnd: HWND, param: LPARAM) -> BOOL {
        let children = &mut *(param as *mut Children);
        let class = window_string(hwnd, GetClassNameW);
        children.rejection = match class.as_str() {
            "SHELLDLL_DefView" | "SysListView32" => Some("WorkerW contains desktop icons"),
            crate::desktop_transition::CLASS_NAME => Some("WorkerW contains a transition cover"),
            _ if hwnd == children.presenter => {
                children.found = true;
                None
            }
            // Web wallpapers render through visible browser children of the
            // presenter; they are part of the verified wallpaper content.
            _ if IsChild(children.presenter, hwnd) != 0 => None,
            _ if IsWindowVisible(hwnd) != 0 => Some("WorkerW contains another visible window"),
            _ => None,
        };
        i32::from(children.rejection.is_none())
    }
    let mut children = Children {
        presenter: window.presenter,
        found: false,
        rejection: None,
    };
    unsafe {
        EnumChildWindows(
            hwnd,
            Some(inspect),
            (&mut children as *mut Children) as LPARAM,
        )
    };
    if let Some(reason) = children.rejection {
        return Err(reason);
    }
    if !children.found {
        return Err("WorkerW no longer contains the selected presenter");
    }
    Ok(WallpaperWorker {
        hwnd,
        rect,
        thread,
        process,
    })
}

const UNWRITTEN_PIXEL: u8 = 91;

/// Copy the window into an owned top-down bitmap, retaining the fourth byte
/// until the target crop has been checked for unwritten pixels.
fn copy_window_pixels(hwnd: HWND, rect: DesktopRect) -> Result<FrozenFrame, String> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GdiFlush, SelectObject,
        BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HDC, HGDIOBJ,
    };
    use windows_sys::Win32::Storage::Xps::PrintWindow;
    use windows_sys::Win32::UI::WindowsAndMessaging::{IsHungAppWindow, PW_RENDERFULLCONTENT};

    struct WindowBuffer {
        dc: HDC,
        bitmap: HBITMAP,
        previous: HGDIOBJ,
    }
    impl Drop for WindowBuffer {
        fn drop(&mut self) {
            unsafe {
                if !self.previous.is_null() {
                    SelectObject(self.dc, self.previous);
                }
                DeleteDC(self.dc);
                if !self.bitmap.is_null() {
                    DeleteObject(self.bitmap);
                }
            }
        }
    }

    let width = rect.width();
    let height = rect.height();
    let len = (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .filter(|bytes| *bytes > 0 && *bytes <= isize::MAX as usize)
        .ok_or("invalid bitmap dimensions")?;
    if unsafe { IsHungAppWindow(hwnd) } != 0 {
        return Err("the window is unresponsive".into());
    }
    let dc = unsafe { CreateCompatibleDC(null_mut()) };
    if dc.is_null() {
        return Err(last_error(
            "Could not create the presenter capture device context",
        ));
    }
    let mut buffer = WindowBuffer {
        dc,
        bitmap: null_mut(),
        previous: null_mut(),
    };
    let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
    info.bmiHeader = BITMAPINFOHEADER {
        biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: width as i32,
        biHeight: -(height as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB,
        ..unsafe { std::mem::zeroed() }
    };
    let mut bits = null_mut();
    buffer.bitmap =
        unsafe { CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, null_mut(), 0) };
    if buffer.bitmap.is_null() || bits.is_null() {
        return Err(last_error(
            "Could not allocate the presenter capture bitmap",
        ));
    }
    buffer.previous = unsafe { SelectObject(dc, buffer.bitmap) };
    if buffer.previous.is_null() {
        return Err(last_error("Could not select the presenter capture bitmap"));
    }
    unsafe {
        std::ptr::write_bytes(bits.cast::<u8>(), UNWRITTEN_PIXEL, len);
        // The request's busy guard stays with this worker if the synchronous
        // window copy outlives the caller's deadline.
        if PrintWindow(hwnd, dc, PW_RENDERFULLCONTENT) == 0 {
            return Err("Windows could not copy pixels from the window".into());
        }
        if GdiFlush() == 0 {
            return Err("Windows could not finish copying the window".into());
        }
        let bgra = std::slice::from_raw_parts(bits.cast::<u8>(), len);
        let mut rgba = Vec::with_capacity(len);
        for pixel in bgra.chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
        Ok(FrozenFrame {
            width,
            height,
            rgba,
        })
    }
}

fn validate_capture_target(target: DesktopRect, details: &ApplyCapture) -> Result<(), String> {
    let current = crate::native_wallpaper::monitor_rect_in_current_apartment(&details.display)?;
    if current != target {
        return Err("The target display geometry changed during Wallpaper Engine capture".into());
    }
    Ok(())
}

fn record_stage(progress: &Mutex<String>, stage: impl Into<String>) {
    *progress.lock().unwrap_or_else(|error| error.into_inner()) = stage.into();
}

struct CaptureGraphics {
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    winrt: IDirect3DDevice,
    interop: IGraphicsCaptureItemInterop,
}

impl CaptureGraphics {
    fn new() -> Result<Self, String> {
        use windows::core::{factory, Interface};
        use windows::Win32::Foundation::HMODULE;
        use windows::Win32::Graphics::Direct3D::{
            D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP,
        };
        use windows::Win32::Graphics::Direct3D11::{
            D3D11CreateDevice, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_SDK_VERSION,
        };
        use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
        fn create_device(
            driver: D3D_DRIVER_TYPE,
        ) -> windows::core::Result<(ID3D11Device, ID3D11DeviceContext)> {
            let mut device = None;
            let mut context = None;
            unsafe {
                D3D11CreateDevice(
                    None,
                    driver,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_BGRA_SUPPORT,
                    None,
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                )?;
            }
            Ok((device.unwrap(), context.unwrap()))
        }

        let (device, context) = create_device(D3D_DRIVER_TYPE_HARDWARE)
            .or_else(|_| create_device(D3D_DRIVER_TYPE_WARP))
            .map_err(|error| format!("Could not create a capture graphics device: {error}"))?;
        let dxgi: IDXGIDevice = device
            .cast()
            .map_err(|error| format!("Could not access the DXGI capture device: {error}"))?;
        let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }
            .map_err(|error| format!("Could not bridge the capture device to WinRT: {error}"))?;
        let winrt = inspectable
            .cast()
            .map_err(|error| format!("Could not access the WinRT capture device: {error}"))?;
        let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
            .map_err(|error| format!("Windows Graphics Capture is unavailable: {error}"))?;
        Ok(Self {
            device,
            context,
            winrt,
            interop,
        })
    }

    fn read_window(
        &self,
        hwnd: HWND,
        deadline: Instant,
        current: &impl Fn() -> bool,
        progress: &Mutex<String>,
    ) -> Result<PresenterAttempt<FrozenFrame>, String> {
        if Instant::now() >= deadline {
            return Ok(PresenterAttempt::Retry(
                "The Wallpaper Engine capture surface was not ready within the frame budget".into(),
            ));
        }
        if !current() {
            return Ok(PresenterAttempt::Retry(
                "The Wallpaper Engine capture surface moved or was replaced".into(),
            ));
        }
        record_stage(progress, "creating the Windows Graphics Capture item");
        let item: GraphicsCaptureItem = unsafe {
            self.interop
                .CreateForWindow(windows::Win32::Foundation::HWND(hwnd))
        }
        .map_err(|error| {
            format!("Could not target the Wallpaper Engine capture surface: {error}")
        })?;
        read_capture_item(
            &item,
            &self.winrt,
            &self.device,
            &self.context,
            deadline,
            current,
            progress,
        )
    }
}

fn read_presenter(
    graphics: &mut Option<CaptureGraphics>,
    window: &CaptureWindow,
    target: DesktopRect,
    overlays: &[usize],
    deadline: Instant,
    presenter_current: impl Fn() -> bool,
    progress: &Mutex<String>,
    shell_root: impl Fn() -> HWND,
) -> Result<PresenterAttempt<FrozenFrame>, String> {
    let needs_isolation = || {
        shares_capture_root(window.root, target, overlays)
            || (window.presenter != window.root && !desktop_icon_view(window.root).is_null())
    };
    let isolate = needs_isolation();
    let current = || {
        presenter_current()
            && if isolate {
                !contains_overlay(window.presenter, overlays)
            } else {
                !needs_isolation()
            }
    };
    if !current() {
        return Ok(PresenterAttempt::Retry(
            "The Wallpaper Engine capture surface moved or contains the outgoing overlay".into(),
        ));
    }
    if isolate {
        if window.presenter == window.root {
            return Ok(PresenterAttempt::Retry(
                "The shared wallpaper host has no isolated presenter child".into(),
            ));
        }
        return read_isolated_presenter(
            window,
            target,
            overlays,
            deadline,
            &current,
            progress,
            shell_root,
            copy_window_pixels,
        );
    }

    let hwnd = window.root;
    let rect = window.source_rect;
    let surface = format!("WGC independent root hwnd={hwnd:?} rect={rect:?}");
    crate::apply_trace::event_lazy("transition-engine-capture-surface", || surface.clone());
    let graphics = match graphics {
        Some(graphics) => graphics,
        None => {
            record_stage(
                progress,
                format!("{surface}: initializing the capture graphics device"),
            );
            graphics.insert(CaptureGraphics::new().map_err(|error| format!("{surface}: {error}"))?)
        }
    };
    let deadline = deadline.min(Instant::now() + Duration::from_secs(2));
    let frame = match graphics.read_window(hwnd, deadline, &current, progress) {
        Ok(PresenterAttempt::Ready(frame)) => frame,
        Ok(PresenterAttempt::Retry(reason)) => {
            return Ok(PresenterAttempt::Retry(format!("{surface}: {reason}")))
        }
        Err(error) if !current() => {
            return Ok(PresenterAttempt::Retry(format!("{surface}: {error}")))
        }
        Err(error) => return Err(format!("{surface}: {error}")),
    };
    if !current() {
        return Ok(PresenterAttempt::Retry(format!(
            "{surface}: surface changed during readback"
        )));
    }
    if Instant::now() >= deadline {
        return Ok(PresenterAttempt::Retry(format!(
            "{surface}: frame exceeded the readback budget"
        )));
    }
    let frame = match crop_to_monitor(&frame, rect, target) {
        Ok(frame) => frame,
        Err(reason) => return Ok(PresenterAttempt::Retry(format!("{surface}: {reason}"))),
    };
    Ok(if has_visible_detail(&frame.rgba) {
        PresenterAttempt::Ready(frame)
    } else {
        PresenterAttempt::Retry(format!(
            "{surface}: could not distinguish an empty capture from dark or uniform artwork"
        ))
    })
}

fn read_isolated_presenter(
    window: &CaptureWindow,
    target: DesktopRect,
    overlays: &[usize],
    deadline: Instant,
    current: &dyn Fn() -> bool,
    progress: &Mutex<String>,
    shell_root: impl Fn() -> HWND,
    copy: impl Fn(HWND, DesktopRect) -> Result<FrozenFrame, String>,
) -> Result<PresenterAttempt<FrozenFrame>, String> {
    let read = |hwnd, rect, name, current: &dyn Fn() -> bool| {
        let surface = format!("PrintWindow {name} hwnd={hwnd:?} rect={rect:?}");
        crate::apply_trace::event_lazy("transition-engine-capture-surface", || surface.clone());
        record_stage(progress, surface.clone());
        let frame: Result<FrozenFrame, String> = (|| {
            if Instant::now() >= deadline || !current() {
                return Err("surface changed or the capture deadline elapsed".into());
            }
            let copied = copy(hwnd, rect)?;
            if Instant::now() >= deadline || !current() {
                return Err(
                    "surface changed or the capture deadline elapsed during readback".into(),
                );
            }
            let mut frame = crop_to_monitor(&copied, rect, target)?;
            finish_window_copy(&mut frame.rgba)?;
            Ok(frame)
        })();
        match frame {
            Ok(frame) => Ok(frame),
            Err(reason) => Err(format!("{surface}: {reason}")),
        }
    };
    let child_error = match read(
        window.presenter,
        window.presenter_rect,
        "presenter child",
        current,
    ) {
        Ok(frame) => return Ok(PresenterAttempt::Ready(frame)),
        Err(reason) => reason,
    };
    crate::apply_trace::event_lazy("transition-engine-capture-surface-rejected", || {
        child_error.clone()
    });
    if Instant::now() >= deadline || !current() {
        return Ok(PresenterAttempt::Retry(child_error));
    }
    let worker = || wallpaper_worker(window, target, overlays, shell_root());
    let selected = match worker() {
        Ok(selected) => selected,
        Err(reason) => {
            return Ok(PresenterAttempt::Retry(format!(
                "{child_error}; WorkerW unavailable: {reason}"
            )))
        }
    };
    return Ok(
        match read(selected.hwnd, selected.rect, "wallpaper WorkerW", &|| {
            current() && worker().as_ref() == Ok(&selected)
        }) {
            Ok(frame) => PresenterAttempt::Ready(frame),
            Err(reason) => PresenterAttempt::Retry(format!("{child_error}; {reason}")),
        },
    );
}

fn finish_window_copy(rgba: &mut [u8]) -> Result<(), &'static str> {
    let mut has_content = false;
    for pixel in rgba.chunks_exact_mut(4) {
        if pixel == [UNWRITTEN_PIXEL; 4] {
            return Err("destination contains unwritten pixels");
        }
        has_content |= pixel[..3] != [0; 3];
        pixel[3] = 255;
    }
    if !has_content {
        return Err("all-black pixels cannot distinguish empty capture from black artwork");
    }
    Ok(())
}

fn capture_frame_on_worker(
    target: DesktopRect,
    capture_deadline: Instant,
    details: &ApplyCapture,
) -> Result<FrozenFrame, String> {
    let _coordinates = PhysicalWindowCoordinates::enter()?;
    let progress = &details.progress;
    let mut graphics = None;

    let started = Instant::now();
    let (frame, presenter, attempts) = retry_capture(
        capture_deadline,
        details,
        started,
        |attempt| {
            record_stage(
                progress,
                "checking the verified Engine presenter and display",
            );
            let log_candidates = attempt.is_multiple_of(8);
            let window = match current_capture_window(target, details, log_candidates)? {
                PresenterAttempt::Ready(window) => window,
                PresenterAttempt::Retry(reason) => return Ok(PresenterAttempt::Retry(reason)),
            };
            validate_capture_target(target, details)?;
            crate::apply_trace::event_lazy("transition-engine-presenter-selected", || {
                format!(
                    "display={:?} phase={} candidate={window:?}",
                    details.display, details.phase
                )
            });
            let frame = read_presenter(
                &mut graphics,
                &window,
                target,
                &details.overlay_windows,
                capture_deadline,
                || window.current(target, details.endpoint),
                progress,
                || unsafe { GetShellWindow() },
            )?;
            if matches!(frame, PresenterAttempt::Ready(_)) {
                validate_capture_target(target, details)?;
                if !matches!(current_capture_window(target, details, false)?, PresenterAttempt::Ready(current) if current == window)
                {
                    return Ok(PresenterAttempt::Retry(
                        "The verified presenter changed or became ambiguous after frame readback"
                            .into(),
                    ));
                }
                if Instant::now() >= capture_deadline {
                    return Ok(PresenterAttempt::Retry(
                        "Wallpaper Engine's frame exceeded the capture deadline".into(),
                    ));
                }
            }
            Ok(match frame {
                PresenterAttempt::Ready(frame) => {
                    PresenterAttempt::Ready((frame, window.presenter))
                }
                PresenterAttempt::Retry(reason) => PresenterAttempt::Retry(reason),
            })
        },
    )?;
    crate::apply_trace::event_lazy("transition-engine-capture-frame", || {
        format!(
            "display={:?} phase={} attempts={attempts} elapsed_ms={} fingerprint={} visible_detail={} subtree={:?}",
            details.display,
            details.phase,
            started.elapsed().as_millis(),
            FrameFingerprint::of(&frame),
            has_visible_detail(&frame.rgba),
            subtree_summary(presenter),
        )
    });
    Ok(frame)
}

/// Retry policy around one capture's native readback attempts. An incoming
/// frame that still looks like the outgoing wallpaper retries until the
/// deadline; a structural refusal that repeats unchanged ends the capture.
/// Returns the frame, the attempt's companion value and the attempt count.
fn retry_capture<T>(
    deadline: Instant,
    details: &ApplyCapture,
    started: Instant,
    mut attempt: impl FnMut(usize) -> Result<PresenterAttempt<(FrozenFrame, T)>, String>,
) -> Result<(FrozenFrame, T, usize), String> {
    let mut attempts = 0usize;
    let mut repeated = (String::new(), 0u32);
    wait_for_presenter(deadline, || {
        let mut result = attempt(attempts);
        attempts += 1;
        if let Ok(PresenterAttempt::Ready((frame, _))) = &result {
            // Provider confirmation precedes the new presenter's first
            // frame; keep waiting within the budget for fresh pixels.
            if let Some(reason) = details
                .outgoing
                .as_ref()
                .and_then(|outgoing| incoming_frame_is_stale(frame, outgoing))
            {
                result = Ok(PresenterAttempt::Retry(reason));
            }
        }
        if let Ok(PresenterAttempt::Retry(reason)) = &result {
            record_stage(&details.progress, reason.clone());
            if *reason == repeated.0 {
                repeated.1 += 1;
            } else {
                repeated = (reason.clone(), 1);
                crate::apply_trace::event_lazy("transition-engine-capture-retry", || {
                    format!(
                        "display={:?} phase={} attempt={attempts} elapsed_ms={} reason={reason:?}",
                        details.display,
                        details.phase,
                        started.elapsed().as_millis(),
                    )
                });
            }
            if is_permanent_refusal(reason) && repeated.1 >= PERMANENT_REFUSAL_ATTEMPTS {
                return Err(format!(
                    "{reason} (unchanged after {} attempts)",
                    repeated.1
                ));
            }
        }
        result.map(|ready| match ready {
            PresenterAttempt::Ready((frame, companion)) => {
                PresenterAttempt::Ready((frame, companion, attempts))
            }
            PresenterAttempt::Retry(reason) => PresenterAttempt::Retry(reason),
        })
    })
}

fn read_capture_item(
    item: &windows::Graphics::Capture::GraphicsCaptureItem,
    winrt_device: &windows::Graphics::DirectX::Direct3D11::IDirect3DDevice,
    device: &windows::Win32::Graphics::Direct3D11::ID3D11Device,
    context: &windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
    deadline: Instant,
    current: impl Fn() -> bool,
    progress: &Mutex<String>,
) -> Result<PresenterAttempt<FrozenFrame>, String> {
    use windows::Graphics::Capture::Direct3D11CaptureFramePool;
    use windows::Graphics::DirectX::DirectXPixelFormat;

    let size = item
        .Size()
        .map_err(|error| format!("Could not read the capture size: {error}"))?;
    if size.Width <= 0 || size.Height <= 0 {
        return Ok(PresenterAttempt::Retry(
            "The Wallpaper Engine presenter has no frame size yet".into(),
        ));
    }
    record_stage(progress, "starting the Windows Graphics Capture session");
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        winrt_device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        2,
        size,
    )
    .map_err(|error| format!("Could not create the capture frame pool: {error}"))?;
    let session = match pool.CreateCaptureSession(item) {
        Ok(session) => session,
        Err(error) => {
            let _ = pool.Close();
            return Err(format!("Could not create the capture session: {error}"));
        }
    };
    let _ = session.SetIsCursorCaptureEnabled(false);
    let _ = session.SetIsBorderRequired(false);
    let result = (|| {
        session
            .StartCapture()
            .map_err(|error| format!("Could not start Wallpaper Engine capture: {error}"))?;
        record_stage(progress, "waiting for a frame from the capture surface");
        loop {
            if !current() {
                return Ok(PresenterAttempt::Retry(
                    "The Wallpaper Engine presenter changed during capture".into(),
                ));
            }
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(PresenterAttempt::Retry(
                    "No Wallpaper Engine frame arrived within the capture time budget".into(),
                ));
            }
            let frame = match pool.TryGetNextFrame() {
                Ok(frame) => frame,
                // Projected COM replaces a successful null frame with an empty error.
                Err(error) if error.code().0 == 0 => {
                    std::thread::sleep(remaining.min(Duration::from_millis(10)));
                    continue;
                }
                Err(error) => {
                    return Err(format!(
                        "Could not retrieve a Wallpaper Engine frame: {error}"
                    ))
                }
            };
            record_stage(progress, "reading back the captured texture");
            let result = frame
                .ContentSize()
                .map_err(|error| format!("Could not read the captured content size: {error}"))
                .and_then(|content| {
                    if content != size {
                        return Ok(PresenterAttempt::Retry(
                            "The Wallpaper Engine frame changed size during capture".into(),
                        ));
                    }
                    read_capture_texture(&frame, device, context, deadline)
                        .map(PresenterAttempt::Ready)
                });
            // Release the frame before closing the pool, including on error.
            let _ = frame.Close();
            drop(frame);
            return result;
        }
    })();
    let _ = session.Close();
    let _ = pool.Close();
    result
}

fn read_capture_texture(
    frame: &windows::Graphics::Capture::Direct3D11CaptureFrame,
    device: &windows::Win32::Graphics::Direct3D11::ID3D11Device,
    context: &windows::Win32::Graphics::Direct3D11::ID3D11DeviceContext,
    deadline: Instant,
) -> Result<FrozenFrame, String> {
    use windows::core::Interface;
    use windows::Win32::Graphics::Direct3D11::{
        ID3D11Texture2D, D3D11_CPU_ACCESS_READ, D3D11_MAPPED_SUBRESOURCE,
        D3D11_MAP_FLAG_DO_NOT_WAIT, D3D11_MAP_READ, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    };
    use windows::Win32::Graphics::Dxgi::DXGI_ERROR_WAS_STILL_DRAWING;
    use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;

    let surface = frame
        .Surface()
        .map_err(|error| format!("Could not access the captured surface: {error}"))?;
    let access: IDirect3DDxgiInterfaceAccess = surface
        .cast()
        .map_err(|error| format!("Could not access the captured DXGI surface: {error}"))?;
    let texture: ID3D11Texture2D = unsafe { access.GetInterface() }
        .map_err(|error| format!("Could not access the captured texture: {error}"))?;
    let mut description = D3D11_TEXTURE2D_DESC::default();
    unsafe { texture.GetDesc(&mut description) };
    description.Usage = D3D11_USAGE_STAGING;
    description.BindFlags = 0;
    description.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
    description.MiscFlags = 0;
    let mut staging = None;
    unsafe { device.CreateTexture2D(&description, None, Some(&mut staging)) }
        .map_err(|error| format!("Could not allocate a readable capture texture: {error}"))?;
    let staging = staging.unwrap();
    unsafe {
        context.CopyResource(&staging, &texture);
        // Submit the copy before polling a nonblocking map of the staging texture.
        context.Flush();
    }
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(
                "Wallpaper Engine's captured texture exceeded the readback deadline".into(),
            );
        }
        match unsafe {
            context.Map(
                &staging,
                0,
                D3D11_MAP_READ,
                D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32,
                Some(&mut mapped),
            )
        } {
            Ok(()) => break,
            Err(error) if error.code() == DXGI_ERROR_WAS_STILL_DRAWING => {
                std::thread::sleep(remaining.min(Duration::from_millis(2)));
            }
            Err(error) => return Err(format!("Could not read the captured texture: {error}")),
        }
    }
    let row_bytes = description.Width as usize * 4;
    let mut rgba = Vec::with_capacity(row_bytes * description.Height as usize);
    for y in 0..description.Height as usize {
        let row = unsafe {
            std::slice::from_raw_parts(
                (mapped.pData as *const u8).add(y * mapped.RowPitch as usize),
                row_bytes,
            )
        };
        for pixel in row.chunks_exact(4) {
            rgba.extend_from_slice(&[pixel[2], pixel[1], pixel[0], 255]);
        }
    }
    unsafe { context.Unmap(&staging, 0) };

    Ok(FrozenFrame {
        width: description.Width,
        height: description.Height,
        rgba,
    })
}

fn crop_to_monitor(
    source: &FrozenFrame,
    source_rect: DesktopRect,
    target: DesktopRect,
) -> Result<FrozenFrame, String> {
    if source.width != source_rect.width() || source.height != source_rect.height() {
        return Err(format!("Wallpaper Engine's {}x{} capture does not match its physical root bounds {source_rect:?}", source.width, source.height));
    }
    if !covers(source_rect, target) {
        return Err("Wallpaper Engine's render frame does not cover the target monitor".into());
    }
    let left = (target.left - source_rect.left) as u32;
    let top = (target.top - source_rect.top) as u32;
    let mut rgba = Vec::with_capacity((target.width() * target.height() * 4) as usize);
    for y in 0..target.height() {
        let start = (((top + y) * source.width + left) * 4) as usize;
        let end = start + (target.width() * 4) as usize;
        rgba.extend_from_slice(&source.rgba[start..end]);
    }
    Ok(FrozenFrame {
        width: target.width(),
        height: target.height(),
        rgba,
    })
}

fn has_visible_detail(rgba: &[u8]) -> bool {
    let mut minimum = 255u8;
    let mut maximum = 0u8;
    for pixel in rgba.chunks_exact(4).step_by(257) {
        let luminance =
            ((pixel[0] as u16 * 54 + pixel[1] as u16 * 183 + pixel[2] as u16 * 19) >> 8) as u8;
        minimum = minimum.min(luminance);
        maximum = maximum.max(luminance);
    }
    maximum.saturating_sub(minimum) >= 4
}

fn last_error(context: &str) -> String {
    format!("{context}: {}", std::io::Error::last_os_error())
}

fn capture_root_rect(hwnd: HWND) -> Option<DesktopRect> {
    // Match the capture surface to physical screen bounds without invisible
    // resize borders or GetWindowRect's DPI virtualization.
    let mut rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let result = unsafe {
        DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS as u32,
            (&mut rect as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        )
    };
    (result >= 0 && rect.right > rect.left && rect.bottom > rect.top).then_some(DesktopRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}

fn window_rect(hwnd: HWND) -> Option<DesktopRect> {
    let mut rect: RECT = unsafe { std::mem::zeroed() };
    (unsafe { GetWindowRect(hwnd, &mut rect) } != 0).then_some(DesktopRect {
        left: rect.left,
        top: rect.top,
        right: rect.right,
        bottom: rect.bottom,
    })
}

fn inspect_engine_window(hwnd: HWND, process_id: u32) -> Option<EngineWindow> {
    let mut owner = 0;
    if unsafe { GetWindowThreadProcessId(hwnd, &mut owner) } == 0 || owner != process_id {
        return None;
    }
    Some(EngineWindow {
        hwnd,
        class_name: unsafe { window_string(hwnd, GetClassNameW) },
        title: unsafe { window_string(hwnd, GetWindowTextW) },
        rect: window_rect(hwnd)?,
        visible: unsafe { IsWindowVisible(hwnd) } != 0,
    })
}

unsafe extern "system" fn collect_engine_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = &mut *(lparam as *mut Search);
    if let Some(window) = inspect_engine_window(hwnd, search.process_id) {
        search.windows.push(window);
    }
    1
}

unsafe fn window_string(
    hwnd: HWND,
    reader: unsafe extern "system" fn(HWND, *mut u16, i32) -> i32,
) -> String {
    let mut buffer = [0u16; 512];
    let length = reader(hwnd, buffer.as_mut_ptr(), buffer.len() as i32).max(0) as usize;
    String::from_utf16_lossy(&buffer[..length])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a running Wallpaper Engine wallpaper"]
    fn report_render_windows_for_running_engine() {
        let _coordinates = PhysicalWindowCoordinates::enter().unwrap();
        let endpoint = crate::engine_ipc::control_window_identity().unwrap();
        let windows = windows_for_process(endpoint.1);
        for window in &windows {
            eprintln!("{window:?}");
        }
        let monitor = crate::native_wallpaper::monitors()
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let details = ApplyCapture {
            operation: 0,
            display: monitor.device_id,
            phase: "diagnostic",
            endpoint,
            overlay_windows: Vec::new(),
            progress: Arc::default(),
            outgoing: None,
        };
        match current_capture_window(monitor.rect, &details, true).unwrap() {
            PresenterAttempt::Ready(selected) => eprintln!("selected={selected:?}"),
            PresenterAttempt::Retry(reason) => panic!("{reason}"),
        }
    }

    #[test]
    #[ignore = "requires a running Wallpaper Engine wallpaper"]
    fn captures_the_actual_engine_render_window() {
        let monitor = crate::native_wallpaper::monitors()
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let endpoint = crate::engine_ipc::control_window_identity().unwrap();
        let frame = capture_for_apply(
            monitor.rect,
            0,
            &monitor.device_id,
            "diagnostic",
            endpoint,
            &[],
            None,
        )
        .unwrap();
        assert_eq!(frame.width, monitor.rect.width());
        assert_eq!(frame.height, monitor.rect.height());
        let checksum = frame
            .rgba
            .iter()
            .fold(0u64, |sum, byte| sum.wrapping_mul(16777619) ^ *byte as u64);
        eprintln!(
            "captured {}x{} actual Engine frame checksum={checksum:016x}",
            frame.width, frame.height
        );
    }
}

#[cfg(test)]
mod presenter_tests {
    use super::*;

    #[test]
    fn apply_phases_use_the_short_deadline_and_only_unchanged_structure_is_permanent() {
        assert_eq!(capture_deadline("outgoing"), APPLY_CAPTURE_DEADLINE);
        assert_eq!(capture_deadline("incoming"), APPLY_CAPTURE_DEADLINE);
        assert_eq!(capture_deadline("diagnostic"), CAPTURE_DEADLINE);
        assert!(is_permanent_refusal(
            "PrintWindow presenter child hwnd=0x1: destination contains unwritten pixels; WorkerW unavailable: WorkerW contains another visible window"
        ));
        assert!(!is_permanent_refusal(
            "child; WorkerW unavailable: WorkerW no longer contains the selected presenter"
        ));
        assert!(!is_permanent_refusal(
            "No rendered Wallpaper Engine window covers the target monitor"
        ));
    }

    #[test]
    fn retry_loop_ends_unchanged_structure_and_waits_for_a_fresh_incoming_frame() {
        let gradient = |mirrored: bool| {
            let (width, height) = (64u32, 36u32);
            let mut rgba = Vec::new();
            for _ in 0..height {
                for x in 0..width {
                    let value = (x * 4) as u8;
                    let value = if mirrored { 255 - value } else { value };
                    rgba.extend([value, value, value, 255]);
                }
            }
            FrozenFrame {
                width,
                height,
                rgba,
            }
        };
        let outgoing = gradient(false);
        let details = ApplyCapture {
            operation: 0,
            display: "test-display".into(),
            phase: "incoming",
            endpoint: (0, 0),
            overlay_windows: Vec::new(),
            progress: Arc::default(),
            outgoing: Some(FrameFingerprint::of(&outgoing)),
        };
        let structural = |detail: &str| {
            format!("PrintWindow presenter child hwnd=0x1: destination contains unwritten pixels; WorkerW unavailable: {detail}")
        };
        // Runs the policy with scripted attempts; returns the result, the
        // number of attempts and the elapsed time.
        let run =
            |budget: Duration, script: &dyn Fn(usize) -> PresenterAttempt<(FrozenFrame, ())>| {
                let started = Instant::now();
                let mut calls = 0;
                let result = retry_capture(started + budget, &details, started, |attempt| {
                    calls += 1;
                    Ok(script(attempt))
                });
                (result, calls, started.elapsed())
            };

        let visible = structural("WorkerW contains another visible window");
        let (result, calls, elapsed) = run(Duration::from_secs(5), &|_| {
            PresenterAttempt::Retry(visible.clone())
        });
        let error = result.err().unwrap();
        assert!(error.ends_with("(unchanged after 3 attempts)"), "{error}");
        assert_eq!(calls, PERMANENT_REFUSAL_ATTEMPTS as usize);
        assert!(elapsed < Duration::from_secs(1), "{elapsed:?}");

        let budget = Duration::from_millis(300);
        let icons = structural("WorkerW contains icons");
        let (result, calls, elapsed) = run(budget, &|attempt| {
            PresenterAttempt::Retry(if attempt % 2 == 0 {
                visible.clone()
            } else {
                icons.clone()
            })
        });
        let error = result.err().unwrap();
        assert!(!error.contains("unchanged after"), "{error}");
        assert!(calls > 3 && elapsed >= budget, "{calls} {elapsed:?}");

        let replaced = structural("WorkerW no longer contains the selected presenter");
        let (result, calls, elapsed) = run(budget, &|_| PresenterAttempt::Retry(replaced.clone()));
        let error = result.err().unwrap();
        assert!(!error.contains("unchanged after"), "{error}");
        assert!(calls > 3 && elapsed >= budget, "{calls} {elapsed:?}");

        let (result, calls, elapsed) =
            run(budget, &|_| PresenterAttempt::Ready((outgoing.clone(), ())));
        let error = result.err().unwrap();
        assert!(
            error.contains("still matches the outgoing wallpaper"),
            "{error}"
        );
        assert!(calls > 3 && elapsed >= budget, "{calls} {elapsed:?}");

        let (result, calls, _) = run(Duration::from_secs(5), &|attempt| {
            PresenterAttempt::Ready((gradient(attempt >= 4), ()))
        });
        let (frame, (), attempts) = result.unwrap();
        assert_eq!(frame.rgba, gradient(true).rgba);
        assert_eq!((calls, attempts), (5, 5));
    }

    #[test]
    fn frame_fingerprint_separates_distinct_frames_and_matches_identical_ones() {
        let frame = |shade: fn(u32, u32) -> u8| {
            let (width, height) = (64u32, 36u32);
            let mut rgba = Vec::new();
            for y in 0..height {
                for x in 0..width {
                    let value = shade(x, y);
                    rgba.extend([value, value, value, 255]);
                }
            }
            FrozenFrame {
                width,
                height,
                rgba,
            }
        };
        let gradient = FrameFingerprint::of(&frame(|x, _| (x * 4) as u8));
        let same = FrameFingerprint::of(&frame(|x, _| (x * 4) as u8));
        let mirrored = FrameFingerprint::of(&frame(|x, _| 255 - (x * 4) as u8));
        assert_eq!(gradient, same);
        assert_eq!(gradient.mean_difference(&same), 0.0);
        assert!(gradient.mean_difference(&mirrored) > 64.0);
        assert_eq!(gradient.to_string().len(), 64);

        let outgoing = frame(|x, _| (x * 4) as u8);
        let outgoing_print = FrameFingerprint::of(&outgoing);
        assert!(incoming_frame_is_stale(&outgoing, &outgoing_print)
            .unwrap()
            .contains("still matches"));
        assert!(incoming_frame_is_stale(&frame(|_, _| 64), &outgoing_print)
            .unwrap()
            .contains("uniform"));
        assert_eq!(
            incoming_frame_is_stale(&frame(|x, _| 255 - (x * 4) as u8), &outgoing_print),
            None
        );
    }

    #[test]
    fn copied_dark_artwork_is_distinct_from_an_unwritten_or_zeroed_buffer() {
        assert_eq!(
            finish_window_copy(&mut [UNWRITTEN_PIXEL; 8]),
            Err("destination contains unwritten pixels")
        );
        assert!(finish_window_copy(&mut [0; 8])
            .unwrap_err()
            .contains("all-black pixels"));
        let mut dark = [1, 2, 3, 0, 1, 2, 3, 0];
        finish_window_copy(&mut dark).unwrap();
        assert_eq!(dark, [1, 2, 3, 255, 1, 2, 3, 255]);
        assert!(finish_window_copy(&mut [0, 0, 0, 255]).is_err());
    }

    #[test]
    fn capture_crop_requires_the_full_physical_extent_before_using_its_origin() {
        let root = DesktopRect {
            left: -3,
            top: -2,
            right: 1,
            bottom: 1,
        };
        let target = DesktopRect {
            left: -2,
            top: -1,
            right: 0,
            bottom: 1,
        };
        let frame = |width| FrozenFrame {
            width,
            height: 3,
            rgba: (0..width * 3)
                .flat_map(|index| [index as u8, 0, 0, 255])
                .collect(),
        };
        let cropped = crop_to_monitor(&frame(4), root, target).unwrap();
        assert_eq!((cropped.width, cropped.height), (2, 2));
        assert_eq!(
            cropped.rgba.into_iter().step_by(4).collect::<Vec<_>>(),
            [5, 6, 9, 10]
        );

        let displaced = frame(5);
        assert!(crop_to_monitor(&displaced, root, target)
            .unwrap_err()
            .contains("physical root bounds"));
    }

    #[test]
    fn delayed_or_replaced_presenter_is_retried_only_for_its_display() {
        let target = DesktopRect {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        };
        let old = EngineWindow {
            hwnd: 1usize as HWND,
            class_name: "WPEDesktopDX11Window".into(),
            title: "WPELiveWallpaper".into(),
            rect: DesktopRect {
                left: 100,
                right: 200,
                ..target
            },
            visible: true,
        };
        assert_eq!(presenter_overlap(&old, target), Err("wrong-display"));
        let replacement = EngineWindow {
            hwnd: 2usize as HWND,
            rect: target,
            ..old.clone()
        };
        assert_eq!(presenter_overlap(&replacement, target), Ok(10_000));
        let hidden = EngineWindow {
            visible: false,
            ..replacement.clone()
        };
        assert_eq!(presenter_overlap(&hidden, target), Err("hidden"));

        let mut attempts = 0;
        let actual = wait_for_presenter(Instant::now() + Duration::from_secs(1), || {
            attempts += 1;
            let current = if attempts == 1 { &old } else { &replacement };
            Ok(match presenter_overlap(current, target) {
                Ok(_) => PresenterAttempt::Ready(current.hwnd),
                Err(reason) => PresenterAttempt::Retry(reason.into()),
            })
        })
        .unwrap();
        assert_eq!(attempts, 2);
        assert_eq!(actual, replacement.hwnd);
    }

    #[test]
    fn persistent_unavailability_settles_and_a_restarted_endpoint_is_terminal() {
        let mut attempts = 0;
        let deadline = Instant::now() + Duration::from_millis(50);
        let unavailable = wait_for_presenter::<()>(deadline, || {
            attempts += 1;
            Ok(PresenterAttempt::Retry(
                "still no friendly presenter".into(),
            ))
        });
        assert_eq!(unavailable, Err("still no friendly presenter".into()));
        assert!(attempts >= 1);
        let mut changed = 0;
        let result = wait_for_presenter::<()>(Instant::now() + Duration::from_secs(1), || {
            changed += 1;
            Err("verified endpoint changed".into())
        });
        assert_eq!(result.unwrap_err(), "verified endpoint changed");
        assert_eq!(changed, 1);
    }

    #[test]
    fn presenter_selection_never_uses_an_overlapping_neighbor_or_ambiguous_match() {
        let target = DesktopRect {
            left: 0,
            top: 0,
            right: 100,
            bottom: 100,
        };
        let spanning = DesktopRect {
            right: 200,
            ..target
        };
        let neighbor = EngineWindow {
            hwnd: 10usize as HWND,
            class_name: "WPEDesktopDX11Window".into(),
            title: "WPELiveWallpaper".into(),
            rect: DesktopRect {
                left: 90,
                ..spanning
            },
            visible: true,
        };
        assert_eq!(presenter_overlap(&neighbor, target), Err("wrong-display"));
        let window = |id, rect| CaptureWindow {
            presenter: id as HWND,
            presenter_rect: rect,
            root: 100usize as HWND,
            source_rect: spanning,
        };
        let exact = window(1usize, target);
        let span = window(2usize, spanning);
        let PresenterAttempt::Ready(chosen) = choose_presenter(vec![span, exact], target) else {
            panic!("a unique exact presenter wins over a span");
        };
        assert_eq!(chosen.presenter, 1usize as HWND);
        assert!(matches!(
            choose_presenter(vec![window(1, target), window(2, target)], target),
            PresenterAttempt::Retry(reason) if reason.starts_with("Multiple")
        ));
    }

    #[test]
    fn video_window_class_is_eligible_without_a_scene_title() {
        let target = DesktopRect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let mut video = EngineWindow {
            hwnd: 1usize as HWND,
            class_name: "WPEVideoWallpaper".into(),
            title: "WPEVideoWallpaper".into(),
            rect: target,
            visible: true,
        };
        assert_eq!(presenter_overlap(&video, target), Ok(1920 * 1080));
        video.class_name.push_str("Helper");
        assert_eq!(presenter_overlap(&video, target), Err("not-a-presenter"));
        video.class_name = "WPEDesktopDX11Window".into();
        assert_eq!(presenter_overlap(&video, target), Ok(1920 * 1080));
        video.class_name = "WPEEventWindow".into();
        video.title = "WPELiveWallpaper".into();
        assert_eq!(presenter_overlap(&video, target), Err("not-a-presenter"));
    }
}

#[cfg(test)]
#[path = "engine_capture/owned_fixture.rs"]
mod owned_fixture;

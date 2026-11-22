use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_EXTENDED_FRAME_BOUNDS};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, GetAncestor, GetClassNameW, GetDesktopWindow, GetWindowRect, GetWindowTextW,
    GetWindowThreadProcessId, IsWindowVisible, GA_ROOT,
};

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
        || (!window.class_name.starts_with("WPEDesktop") && window.title != "WPELiveWallpaper")
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
}

pub(crate) fn capture_for_apply(
    target: DesktopRect,
    operation: u64,
    display: &str,
    phase: &'static str,
    endpoint: (usize, u32),
    overlay_windows: &[usize],
) -> Result<FrozenFrame, String> {
    let details = ApplyCapture {
        operation,
        display: display.into(),
        phase,
        endpoint,
        overlay_windows: overlay_windows.to_vec(),
    };
    crate::apply_trace::event_for_lazy(operation, "transition-engine-capture-start", || {
        format!("display={display:?} phase={phase} endpoint={endpoint:?} target={target:?}")
    });
    let result = capture_frame_with_worker(
        target,
        &CAPTURE_BUSY,
        &CAPTURE_WORKER,
        CAPTURE_DEADLINE,
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
            CaptureSubmissionError::Deadline => {
                "Wallpaper Engine capture worker was not ready before its 5-second deadline"
                    .to_string()
            }
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
            mpsc::RecvTimeoutError::Timeout => {
                "Wallpaper Engine frame capture exceeded its 5-second deadline".to_string()
            }
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
            None,
            |rx| {
                thread = Some(std::thread::spawn(move || {
                    respond_to_capture(rx.recv().unwrap(), |_, _, _| {
                        release_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                        Ok(frame())
                    });
                    settled_tx.send(()).unwrap();
                    respond_to_capture(rx.recv().unwrap(), |_, _, _| Ok(frame()));
                }));
                Ok(())
            },
        );
        assert!(result.unwrap_err().contains("exceeded"));
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
                        capture_frame_wgc_on_worker(target, deadline, &details)
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
    loop {
        if Instant::now() >= deadline {
            return Err("The presenter did not become ready before the capture deadline".into());
        }
        match attempt()? {
            PresenterAttempt::Ready(ready) => return Ok(ready),
            PresenterAttempt::Retry(reason) => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining <= Duration::from_millis(20) {
                    return Err(reason);
                }
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
    match choose_presenter(eligible, target) {
        PresenterAttempt::Ready(window)
            if shares_capture_root(window.root, target, &details.overlay_windows) =>
        {
            if log_candidates {
                crate::apply_trace::event_lazy("transition-engine-shared-capture-root", || {
                    format!(
                        "display={:?} phase={} candidate={window:?} overlays={:?}",
                        details.display, details.phase, details.overlay_windows
                    )
                });
            }
            Ok(PresenterAttempt::Retry(
                "The Wallpaper Engine capture host also contains the visible transition surface"
                    .into(),
            ))
        }
        result => Ok(result),
    }
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

fn validate_capture_target(target: DesktopRect, details: &ApplyCapture) -> Result<(), String> {
    let current = crate::native_wallpaper::monitor_rect_in_current_apartment(&details.display)?;
    if current != target {
        return Err("The target display geometry changed during Wallpaper Engine capture".into());
    }
    Ok(())
}

fn capture_frame_wgc_on_worker(
    target: DesktopRect,
    capture_deadline: Instant,
    details: &ApplyCapture,
) -> Result<FrozenFrame, String> {
    use windows::core::{factory, Interface};
    use windows::Graphics::Capture::GraphicsCaptureItem;
    use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
    use windows::Win32::Foundation::{HMODULE, HWND as WinHwnd};
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, D3D11_CREATE_DEVICE_BGRA_SUPPORT,
        D3D11_SDK_VERSION,
    };
    use windows::Win32::Graphics::Dxgi::IDXGIDevice;
    use windows::Win32::System::WinRT::Direct3D11::CreateDirect3D11DeviceFromDXGIDevice;
    use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
    fn create_device(
        driver: windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE,
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
    let dxgi_device: IDXGIDevice = device
        .cast()
        .map_err(|error| format!("Could not access the DXGI capture device: {error}"))?;
    let inspectable = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi_device) }
        .map_err(|error| format!("Could not bridge the capture device to WinRT: {error}"))?;
    let winrt_device: IDirect3DDevice = inspectable
        .cast()
        .map_err(|error| format!("Could not access the WinRT capture device: {error}"))?;

    let interop = factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
        .map_err(|error| format!("Windows Graphics Capture is unavailable: {error}"))?;
    // WGC needs a top-level window. Resolve and revalidate its exact root
    // under the confirmed control process before creating the capture item.
    let mut attempts = 0usize;
    wait_for_presenter(capture_deadline, || {
        let log_candidates = attempts.is_multiple_of(8);
        attempts += 1;
        let window = match current_capture_window(target, details, log_candidates)? {
            PresenterAttempt::Ready(window) => window,
            PresenterAttempt::Retry(reason) => return Ok(PresenterAttempt::Retry(reason)),
        };
        validate_capture_target(target, details)?;
        let current = || {
            window.current(target, details.endpoint)
                && !shares_capture_root(window.root, target, &details.overlay_windows)
        };
        if !current() {
            return Ok(PresenterAttempt::Retry(
                "The Wallpaper Engine capture host moved or was replaced".into(),
            ));
        }
        let captured: windows::core::Result<GraphicsCaptureItem> =
            unsafe { interop.CreateForWindow(WinHwnd(window.root)) };
        let item = match captured {
            Ok(item) => item,
            Err(error) if current() => {
                return Err(format!(
                    "Could not target Wallpaper Engine's render window: {error}"
                ));
            }
            Err(_) => {
                return Ok(PresenterAttempt::Retry(
                    "The Wallpaper Engine presenter was replaced before capture".into(),
                ))
            }
        };
        crate::apply_trace::event_lazy("transition-engine-presenter-selected", || {
            format!(
                "display={:?} phase={} candidate={window:?}",
                details.display, details.phase
            )
        });
        let deadline = (Instant::now() + Duration::from_secs(2)).min(capture_deadline);
        let frame =
            match read_capture_item(&item, &winrt_device, &device, &context, deadline, &current) {
                Ok(PresenterAttempt::Ready(frame)) => frame,
                Ok(PresenterAttempt::Retry(reason)) => return Ok(PresenterAttempt::Retry(reason)),
                Err(error) if !current() => return Ok(PresenterAttempt::Retry(error)),
                Err(error) => return Err(error),
            };
        validate_capture_target(target, details)?;
        if !current() {
            return Ok(PresenterAttempt::Retry(
                "Wallpaper Engine's capture root changed while reading its frame".into(),
            ));
        }
        let frame = match crop_to_monitor(&frame, window.source_rect, target) {
            Ok(frame) => frame,
            Err(reason) => return Ok(PresenterAttempt::Retry(reason)),
        };
        Ok(if has_visible_detail(&frame.rgba) {
            PresenterAttempt::Ready(frame)
        } else {
            PresenterAttempt::Retry(
                "Wallpaper Engine returned a blank frame for the target monitor".into(),
            )
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
            if let Ok(frame) = pool.TryGetNextFrame() {
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
            std::thread::sleep(remaining.min(Duration::from_millis(10)));
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
        };
        match current_capture_window(monitor.rect, &details, true).unwrap() {
            PresenterAttempt::Ready(selected) => eprintln!("selected={selected:?}"),
            PresenterAttempt::Retry(reason) => panic!("{reason}"),
        }
    }

    #[test]
    #[ignore = "requires a running non-solid Wallpaper Engine wallpaper"]
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
        assert!(unavailable.is_err());
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
}

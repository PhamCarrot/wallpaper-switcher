use std::ptr::null;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, OnceLock};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumChildWindows, FindWindowW, GetAncestor, GetClassNameW, GetDesktopWindow, GetWindowRect,
    GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, GA_ROOT,
};

use crate::monitor::DesktopRect;
use crate::transition::FrozenFrame;

#[derive(Debug, Clone)]
pub struct EngineWindow {
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

pub fn windows_for_running_engine() -> Result<Vec<EngineWindow>, String> {
    let event_class = wide("WPEEventWindow");
    let event_window = unsafe { FindWindowW(event_class.as_ptr(), null()) };
    if event_window.is_null() {
        return Err("Wallpaper Engine's event window was not found".to_string());
    }
    let mut process_id = 0u32;
    unsafe { GetWindowThreadProcessId(event_window, &mut process_id) };
    if process_id == 0 {
        return Err("Wallpaper Engine's process identity was unavailable".to_string());
    }
    let mut search = Search {
        process_id,
        windows: Vec::new(),
    };
    unsafe {
        EnumChildWindows(
            GetDesktopWindow(),
            Some(collect_engine_window),
            &mut search as *mut Search as LPARAM,
        )
    };
    search.windows.sort_by_key(|window| {
        -((window.rect.right - window.rect.left).max(0) as i64
            * (window.rect.bottom - window.rect.top).max(0) as i64)
    });
    Ok(search.windows)
}

pub fn window_for_monitor(target: DesktopRect) -> Result<EngineWindow, String> {
    windows_for_running_engine()?
        .into_iter()
        .filter(|window| {
            window.visible
                && window.class_name != "WPEEventWindow"
                && (window.class_name.starts_with("WPEDesktop")
                    || window.title == "WPELiveWallpaper")
                && window.overlap_area(target) > 0
        })
        .max_by_key(|window| window.overlap_area(target))
        .ok_or_else(|| {
            "No rendered Wallpaper Engine window overlaps the target monitor".to_string()
        })
}

pub fn capture_frame(target: DesktopRect) -> Result<FrozenFrame, String> {
    capture_frame_wgc(target)
}

type CaptureResponse = Result<FrozenFrame, String>;
type CaptureRequest = (DesktopRect, mpsc::SyncSender<CaptureResponse>);

static CAPTURE_WORKER: OnceLock<Result<mpsc::SyncSender<CaptureRequest>, String>> = OnceLock::new();
static CAPTURE_BUSY: AtomicBool = AtomicBool::new(false);
const CAPTURE_DEADLINE: Duration = Duration::from_secs(5);

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

fn capture_frame_wgc(target: DesktopRect) -> Result<FrozenFrame, String> {
    let deadline = Instant::now() + CAPTURE_DEADLINE;
    let sender = CAPTURE_WORKER.get_or_init(|| {
        let (sender, receiver) = mpsc::sync_channel::<CaptureRequest>(0);
        std::thread::Builder::new()
            .name("wallpaper-engine-capture".to_string())
            .spawn(move || capture_worker(receiver))
            .map_err(|error| format!("Could not start the capture worker: {error}"))?;
        Ok(sender)
    });
    let sender = sender.as_ref().map_err(Clone::clone)?;
    CAPTURE_BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| {
            "The Wallpaper Engine capture worker is still completing an earlier capture".to_string()
        })?;
    let (response_sender, response_receiver) = mpsc::sync_channel(1);
    if let Err(error) = submit_capture_request(sender, (target, response_sender), deadline) {
        CAPTURE_BUSY.store(false, Ordering::Release);
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
    response_receiver
        .recv_timeout(deadline.saturating_duration_since(Instant::now()))
        .map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => {
                "Wallpaper Engine frame capture exceeded its 5-second deadline".to_string()
            }
            mpsc::RecvTimeoutError::Disconnected => {
                "The Wallpaper Engine capture worker did not return a frame".to_string()
            }
        })?
}

#[cfg(test)]
mod capture_submission_tests {
    use super::*;

    #[test]
    fn submission_retries_until_the_rendezvous_worker_is_receiving() {
        let (sender, receiver) = mpsc::sync_channel(0);
        let target = DesktopRect {
            left: 10,
            top: 20,
            right: 30,
            bottom: 40,
        };
        let (response_sender, _response_receiver) = mpsc::sync_channel(1);
        let request = (target, response_sender);
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
    for (target, response) in receiver {
        let result = match &initialized {
            Ok(_) => window_for_monitor(target)
                .and_then(|window| capture_frame_wgc_on_worker(&window, target)),
            Err(error) => Err(error.clone()),
        };
        CAPTURE_BUSY.store(false, Ordering::Release);
        let _ = response.send(result);
    }
}

fn capture_frame_wgc_on_worker(
    window: &EngineWindow,
    target: DesktopRect,
) -> Result<FrozenFrame, String> {
    use windows::core::{factory, Interface};
    use windows::Graphics::Capture::{Direct3D11CaptureFramePool, GraphicsCaptureItem};
    use windows::Graphics::DirectX::Direct3D11::IDirect3DDevice;
    use windows::Graphics::DirectX::DirectXPixelFormat;
    use windows::Win32::Foundation::{HMODULE, HWND as WinHwnd};
    use windows::Win32::Graphics::Direct3D::{D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP};
    use windows::Win32::Graphics::Direct3D11::{
        D3D11CreateDevice, ID3D11Device, ID3D11DeviceContext, ID3D11Texture2D,
        D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_MAPPED_SUBRESOURCE,
        D3D11_MAP_READ, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC, D3D11_USAGE_STAGING,
    };
    use windows::Win32::Graphics::Dxgi::IDXGIDevice;
    use windows::Win32::System::WinRT::Direct3D11::{
        CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
    };
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
    // Wallpaper Engine's DX11 presenter is a child of the wallpaper WorkerW.
    // Graphics Capture only accepts a top-level HWND, so capture that exact
    // root host; unlike desktop duplication, this excludes foreground apps.
    let root = unsafe { GetAncestor(window.hwnd, GA_ROOT) };
    let capture_window = if root.is_null() { window.hwnd } else { root };
    let mut root_rect: RECT = unsafe { std::mem::zeroed() };
    if unsafe { GetWindowRect(capture_window, &mut root_rect) } == 0 {
        return Err(last_error(
            "Could not read the wallpaper capture host geometry",
        ));
    }
    let source_rect = DesktopRect {
        left: root_rect.left,
        top: root_rect.top,
        right: root_rect.right,
        bottom: root_rect.bottom,
    };
    let item: GraphicsCaptureItem = unsafe { interop.CreateForWindow(WinHwnd(capture_window)) }
        .map_err(|error| format!("Could not target Wallpaper Engine's render window: {error}"))?;
    let size = item
        .Size()
        .map_err(|error| format!("Could not read the capture size: {error}"))?;
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &winrt_device,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        2,
        size,
    )
    .map_err(|error| format!("Could not create the capture frame pool: {error}"))?;
    let session = pool
        .CreateCaptureSession(&item)
        .map_err(|error| format!("Could not create the capture session: {error}"))?;
    let _ = session.SetIsCursorCaptureEnabled(false);
    let _ = session.SetIsBorderRequired(false);
    session
        .StartCapture()
        .map_err(|error| format!("Could not start Wallpaper Engine capture: {error}"))?;

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let frame = loop {
        match pool.TryGetNextFrame() {
            Ok(frame) => break frame,
            Err(_) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            Err(error) => {
                let _ = session.Close();
                let _ = pool.Close();
                return Err(format!("No Wallpaper Engine frame arrived: {error}"));
            }
        }
    };

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
    unsafe { context.CopyResource(&staging, &texture) };
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    unsafe { context.Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped)) }
        .map_err(|error| format!("Could not read the captured texture: {error}"))?;
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

    // Release every object obtained from the frame before closing its pool.
    // Repeatedly closing a pool while its frame texture is still live can
    // make the Windows capture runtime tear down that texture out from under
    // the client on a later capture.
    drop(staging);
    drop(texture);
    drop(access);
    drop(surface);
    let _ = frame.Close();
    drop(frame);
    let _ = session.Close();
    let _ = pool.Close();

    if !has_visible_detail(&rgba) {
        return Err("Windows Graphics Capture returned a blank frame".to_string());
    }
    let frame = FrozenFrame {
        width: description.Width,
        height: description.Height,
        rgba,
    };
    crop_to_monitor(&frame, source_rect, target)
}

fn crop_to_monitor(
    source: &FrozenFrame,
    source_rect: DesktopRect,
    target: DesktopRect,
) -> Result<FrozenFrame, String> {
    let left = target.left - source_rect.left;
    let top = target.top - source_rect.top;
    if left < 0
        || top < 0
        || left as u32 + target.width() > source.width
        || top as u32 + target.height() > source.height
    {
        return Err(
            "Wallpaper Engine's render frame does not cover the target monitor".to_string(),
        );
    }
    let mut rgba = Vec::with_capacity((target.width() * target.height() * 4) as usize);
    for y in 0..target.height() {
        let start = (((top as u32 + y) * source.width + left as u32) * 4) as usize;
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

unsafe extern "system" fn collect_engine_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let search = &mut *(lparam as *mut Search);
    let mut process_id = 0u32;
    GetWindowThreadProcessId(hwnd, &mut process_id);
    if process_id != search.process_id {
        return 1;
    }

    let mut rect: RECT = std::mem::zeroed();
    if GetWindowRect(hwnd, &mut rect) == 0 {
        return 1;
    }
    search.windows.push(EngineWindow {
        hwnd,
        class_name: window_string(hwnd, GetClassNameW),
        title: window_string(hwnd, GetWindowTextW),
        rect: DesktopRect {
            left: rect.left,
            top: rect.top,
            right: rect.right,
            bottom: rect.bottom,
        },
        visible: IsWindowVisible(hwnd) != 0,
    });
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

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires a running Wallpaper Engine wallpaper"]
    fn report_render_windows_for_running_engine() {
        let windows = windows_for_running_engine().unwrap();
        for window in &windows {
            eprintln!("{window:?}");
        }
        let monitor = crate::native_wallpaper::monitors()
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let selected = window_for_monitor(monitor.rect).unwrap();
        eprintln!("selected={selected:?}");
    }

    #[test]
    #[ignore = "requires a running non-solid Wallpaper Engine wallpaper"]
    fn captures_the_actual_engine_render_window() {
        let monitor = crate::native_wallpaper::monitors()
            .unwrap()
            .into_iter()
            .next()
            .unwrap();
        let frame = capture_frame(monitor.rect).unwrap();
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

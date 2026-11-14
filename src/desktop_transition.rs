use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{
    GetLastError, SetLastError, BOOL, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, InvalidateRect, StretchDIBits, UpdateWindow, BITMAPINFO,
    BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, PAINTSTRUCT, SRCCOPY,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, EnumWindows, FindWindowExW,
    FindWindowW, GetParent, GetWindowLongPtrW, GetWindowRect, IsWindow, PeekMessageW,
    RegisterClassExW, SendMessageTimeoutW, SetParent, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, GWL_STYLE, HTTRANSPARENT, HWND_BOTTOM,
    HWND_TOP, MSG, PM_REMOVE, SMTO_ABORTIFHUNG, SWP_NOACTIVATE, SW_SHOW, WM_ERASEBKGND,
    WM_NCCREATE, WM_NCDESTROY, WM_NCHITTEST, WM_PAINT, WNDCLASSEXW, WS_CHILD, WS_CLIPCHILDREN,
    WS_CLIPSIBLINGS, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TRANSPARENT, WS_POPUP, WS_VISIBLE,
};

use crate::d3d11_transition::D3d11TransitionRenderer;
use crate::monitor::DesktopRect;
use crate::transition::{render_effect, FrozenFrame, TransitionEffect};

const SPAWN_WORKERW: u32 = 0x052c;
const CLASS_NAME: &str = "WallpaperSwitcherTransition";

struct WindowState {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
    #[cfg(test)]
    drops: Option<std::sync::Arc<std::sync::atomic::AtomicUsize>>,
}

impl WindowState {
    fn new(frame: &FrozenFrame) -> Self {
        let mut state = Self {
            width: frame.width,
            height: frame.height,
            bgra: Vec::with_capacity(frame.rgba.len()),
            #[cfg(test)]
            drops: None,
        };
        state.set_frame(frame);
        state
    }

    fn set_frame(&mut self, frame: &FrozenFrame) {
        self.width = frame.width;
        self.height = frame.height;
        self.bgra.clear();
        for pixel in frame.rgba.chunks_exact(4) {
            self.bgra
                .extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
        }
    }
}

#[cfg(test)]
impl Drop for WindowState {
    fn drop(&mut self) {
        if let Some(drops) = &self.drops {
            drops.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
}

// The production create/callback seam also permits controlled ownership tests
// without constructing a desktop window or invoking any provider.
struct WindowCreation {
    // This creation argument lives only for the synchronous CreateWindowExW
    // call. Userdata receives WindowState, never a pointer to this stack value.
    state: Option<Box<WindowState>>,
}

fn create_transition_window(
    state: WindowState,
    create: impl FnOnce(*mut c_void) -> HWND,
) -> Result<HWND, String> {
    let mut creation = WindowCreation {
        state: Some(Box::new(state)),
    };
    let hwnd = create((&mut creation as *mut WindowCreation).cast());
    if hwnd.is_null() {
        return Err(last_error(
            "Could not create the wallpaper transition window",
        ));
    }
    Ok(hwnd)
}

// Safety: parameter is the live creation argument supplied by the wrapper;
// publish transfers the unique state to userdata only on success. On failure
// it must not retain or destroy the pointer. On success window teardown owns it.
unsafe fn accept_created_window(
    parameter: *mut c_void,
    publish: impl FnOnce(*mut WindowState) -> bool,
) -> bool {
    let Some(creation) = parameter.cast::<WindowCreation>().as_mut() else {
        return false;
    };
    let Some(state) = creation.state.take() else {
        return false;
    };
    let state = Box::into_raw(state);
    if publish(state) {
        true
    } else {
        creation.state = Some(Box::from_raw(state));
        false
    }
}

// Safety: take removes the unique accepted state from this window's userdata.
unsafe fn destroy_created_window(take: impl FnOnce() -> *mut WindowState) {
    let state = take();
    if !state.is_null() {
        drop(Box::from_raw(state));
    }
}

pub struct DesktopTransitionSession {
    hwnd: HWND,
    outgoing: FrozenFrame,
    gpu: Option<D3d11TransitionRenderer>,
}

impl DesktopTransitionSession {
    pub fn show(outgoing: FrozenFrame, target: DesktopRect) -> Result<Self, String> {
        crate::apply_trace::event(
            "transition-window-start",
            format!(
                "left={} top={} width={} height={}",
                target.left,
                target.top,
                target.width(),
                target.height()
            ),
        );
        let result = Self::show_inner(outgoing, target);
        match &result {
            Ok(_) => crate::apply_trace::event(
                "transition-window-complete",
                format!(
                    "left={} top={} width={} height={}",
                    target.left,
                    target.top,
                    target.width(),
                    target.height()
                ),
            ),
            Err(error) => {
                crate::apply_trace::event("transition-window-failure", format!("error={error:?}"))
            }
        }
        result
    }

    fn show_inner(outgoing: FrozenFrame, target: DesktopRect) -> Result<Self, String> {
        if outgoing.width != target.width() || outgoing.height != target.height() {
            return Err("The frozen frame does not match the target monitor".to_string());
        }
        let worker_host = wallpaper_host()?;
        register_window_class()?;

        let class_name = wide(CLASS_NAME);
        let title = wide("Wallpaper Switcher Transition");
        let instance = unsafe { GetModuleHandleW(null()) };
        let hwnd = create_transition_window(WindowState::new(&outgoing), |parameter| unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT,
                class_name.as_ptr(),
                title.as_ptr(),
                WS_POPUP | WS_CLIPSIBLINGS | WS_CLIPCHILDREN,
                target.left,
                target.top,
                target.width() as i32,
                target.height() as i32,
                null_mut(),
                null_mut(),
                instance,
                parameter,
            )
        })?;
        unsafe {
            SetWindowLongPtrW(
                hwnd,
                GWL_STYLE,
                (WS_CHILD | WS_CLIPSIBLINGS | WS_CLIPCHILDREN) as isize,
            );
            let progman_class = wide("Progman");
            let progman = FindWindowW(progman_class.as_ptr(), null());
            let mut host = null_mut();
            // Progman is stable across Wallpaper Engine scene changes. A
            // WorkerW can be replaced during a provider handoff, which would
            // destroy its child and invalidate the transition frame storage.
            for candidate in [progman, worker_host] {
                if candidate.is_null() {
                    continue;
                }
                SetParent(hwnd, candidate);
                if GetParent(hwnd) == candidate {
                    host = candidate;
                    break;
                }
            }
            if host.is_null() {
                DestroyWindow(hwnd);
                return Err(last_error(
                    "Windows refused to place the transition at the wallpaper layer",
                ));
            }
            let mut host_rect: RECT = std::mem::zeroed();
            if GetWindowRect(host, &mut host_rect) == 0 {
                DestroyWindow(hwnd);
                return Err(last_error("Could not read wallpaper host geometry"));
            }
            SetWindowLongPtrW(
                hwnd,
                GWL_STYLE,
                (WS_CHILD | WS_VISIBLE | WS_CLIPSIBLINGS | WS_CLIPCHILDREN) as isize,
            );
            let def_view = wide("SHELLDLL_DefView");
            let insert_after =
                if FindWindowExW(host, null_mut(), def_view.as_ptr(), null()).is_null() {
                    HWND_TOP
                } else {
                    // Progman owns the desktop-icon view. Keep the transition below
                    // every icon child so only the wallpaper pixels are replaced.
                    HWND_BOTTOM
                };
            SetWindowPos(
                hwnd,
                insert_after,
                target.left - host_rect.left,
                target.top - host_rect.top,
                target.width() as i32,
                target.height() as i32,
                SWP_NOACTIVATE,
            );
            ShowWindow(hwnd, SW_SHOW);
            UpdateWindow(hwnd);
        }
        crate::apply_trace::event(
            "d3d-create-start",
            format!("width={} height={}", outgoing.width, outgoing.height),
        );
        let gpu = match D3d11TransitionRenderer::new(hwnd, &outgoing) {
            Ok(renderer) => {
                crate::apply_trace::event(
                    "d3d-create-complete",
                    format!("width={} height={}", outgoing.width, outgoing.height),
                );
                Some(renderer)
            }
            Err(error) => {
                crate::apply_trace::event("d3d-create-fallback", format!("error={error:?}"));
                crate::log_debug(&format!(
                    "D3D11 transition unavailable; using bounded CPU upload fallback: {error}"
                ));
                None
            }
        };
        Ok(Self {
            hwnd,
            outgoing,
            gpu,
        })
    }

    pub fn play(
        &mut self,
        incoming: &FrozenFrame,
        effect: TransitionEffect,
        duration: Duration,
    ) -> Result<(), String> {
        if effect == TransitionEffect::Off {
            return Ok(());
        }
        let started = Instant::now();
        let frame_interval = Duration::from_millis(16);
        loop {
            let elapsed = started.elapsed();
            let progress = if duration.is_zero() {
                1.0
            } else {
                (elapsed.as_secs_f32() / duration.as_secs_f32()).min(1.0)
            };
            self.render(incoming, effect, progress)?;
            self.present()?;
            if progress >= 1.0 {
                break;
            }
            std::thread::sleep(frame_interval.saturating_sub(started.elapsed() - elapsed));
        }
        Ok(())
    }

    pub fn render(
        &mut self,
        incoming: &FrozenFrame,
        effect: TransitionEffect,
        progress: f32,
    ) -> Result<(), String> {
        if unsafe { IsWindow(self.hwnd) } == 0 {
            return Err(
                "Windows destroyed the transition surface during the wallpaper handoff".into(),
            );
        }

        let gpu_error = self
            .gpu
            .as_mut()
            .and_then(|renderer| renderer.render(incoming, effect, progress).err());
        if let Some(error) = gpu_error {
            crate::log_debug(&format!(
                "D3D11 transition render failed; continuing with CPU fallback: {error}"
            ));
            self.gpu = None;
        }
        if self.gpu.is_some() {
            return Ok(());
        }

        let frame = render_effect(effect, &self.outgoing, incoming, progress)?;
        unsafe {
            let state = GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) as *mut WindowState;
            if state.is_null() {
                return Err("The transition surface no longer owns a frame buffer".to_string());
            }
            (*state).set_frame(&frame);
        }
        Ok(())
    }

    pub fn present(&self) -> Result<(), String> {
        if let Some(renderer) = &self.gpu {
            renderer.present()?;
        } else {
            unsafe {
                InvalidateRect(self.hwnd, null(), 0);
                UpdateWindow(self.hwnd);
            }
        }
        pump_messages();
        Ok(())
    }

    #[cfg(test)]
    pub fn uses_gpu(&self) -> bool {
        self.gpu.is_some()
    }
}

impl Drop for DesktopTransitionSession {
    fn drop(&mut self) {
        self.gpu = None;
        if !self.hwnd.is_null() {
            unsafe { DestroyWindow(self.hwnd) };
            self.hwnd = null_mut();
        }
        pump_messages();
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_NCCREATE => {
            let create = &*(lparam as *const CREATESTRUCTW);
            accept_created_window(create.lpCreateParams, |state| {
                // A zero previous value is normal for initial userdata. Clear
                // last error so zero-success is distinct from failed storage.
                SetLastError(0);
                let previous = SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
                previous != 0 || GetLastError() == 0
            }) as LRESULT
        }
        WM_NCDESTROY => {
            destroy_created_window(|| {
                let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut WindowState;
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                state
            });
            DefWindowProcW(hwnd, message, wparam, lparam)
        }
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            let state = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const WindowState;
            let mut paint: PAINTSTRUCT = std::mem::zeroed();
            let dc = BeginPaint(hwnd, &mut paint);
            if !state.is_null() {
                let state = &*state;
                let mut info: BITMAPINFO = std::mem::zeroed();
                info.bmiHeader = BITMAPINFOHEADER {
                    biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: state.width as i32,
                    biHeight: -(state.height as i32),
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB,
                    biSizeImage: (state.width * state.height * 4),
                    ..std::mem::zeroed()
                };
                StretchDIBits(
                    dc,
                    0,
                    0,
                    state.width as i32,
                    state.height as i32,
                    0,
                    0,
                    state.width as i32,
                    state.height as i32,
                    state.bgra.as_ptr().cast(),
                    &info,
                    DIB_RGB_COLORS,
                    SRCCOPY,
                );
            }
            EndPaint(hwnd, &paint);
            0
        }
        _ => DefWindowProcW(hwnd, message, wparam, lparam),
    }
}

fn register_window_class() -> Result<(), String> {
    let instance = unsafe { GetModuleHandleW(null()) };
    if instance.is_null() {
        return Err(last_error("Could not access the application module"));
    }
    let class_name = wide(CLASS_NAME);
    let class = WNDCLASSEXW {
        cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
        style: CS_HREDRAW | CS_VREDRAW,
        lpfnWndProc: Some(window_proc),
        hInstance: instance,
        lpszClassName: class_name.as_ptr(),
        ..unsafe { std::mem::zeroed() }
    };
    let atom = unsafe { RegisterClassExW(&class) };
    if atom == 0 {
        let error = std::io::Error::last_os_error();
        // ERROR_CLASS_ALREADY_EXISTS means another transition already registered it.
        if error.raw_os_error() != Some(1410) {
            return Err(format!("Could not register the transition window: {error}"));
        }
    }
    Ok(())
}

fn wallpaper_host() -> Result<HWND, String> {
    let progman_class = wide("Progman");
    let progman = unsafe { FindWindowW(progman_class.as_ptr(), null()) };
    if progman.is_null() {
        return Err("Windows desktop host (Progman) was not found".to_string());
    }
    let mut ignored = 0usize;
    unsafe {
        SendMessageTimeoutW(
            progman,
            SPAWN_WORKERW,
            0,
            0,
            SMTO_ABORTIFHUNG,
            1_000,
            &mut ignored,
        );
    }
    let mut host: HWND = null_mut();
    unsafe { EnumWindows(Some(find_wallpaper_host), &mut host as *mut HWND as LPARAM) };
    if host.is_null() {
        let worker_class = wide("WorkerW");
        host = unsafe { FindWindowW(worker_class.as_ptr(), null()) };
    }
    if host.is_null() {
        Err("Windows wallpaper WorkerW was not found".to_string())
    } else {
        Ok(host)
    }
}

unsafe extern "system" fn find_wallpaper_host(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let def_view = wide("SHELLDLL_DefView");
    let worker_class = wide("WorkerW");
    if !FindWindowExW(hwnd, null_mut(), def_view.as_ptr(), null()).is_null() {
        let worker = FindWindowExW(null_mut(), hwnd, worker_class.as_ptr(), null());
        if !worker.is_null() {
            *(lparam as *mut HWND) = worker;
            return 0;
        }
    }
    1
}

fn pump_messages() {
    unsafe {
        let mut message: MSG = std::mem::zeroed();
        while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
            DispatchMessageW(&message);
        }
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn last_error(context: &str) -> String {
    format!("{context}: {}", std::io::Error::last_os_error())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    use std::cell::Cell;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn counted_state(drops: &Arc<AtomicUsize>) -> WindowState {
        let frame = FrozenFrame {
            width: 64,
            height: 32,
            rgba: vec![11; 64 * 32 * 4],
        };
        let mut state = WindowState::new(&frame);
        assert_eq!(state.bgra.len(), frame.rgba.len());
        state.drops = Some(drops.clone());
        state
    }

    // An opaque fake handle only; never passed to a native API.
    fn created_handle() -> HWND {
        1usize as HWND
    }

    #[test]
    fn transition_creation_failure_before_acceptance_drops_caller_frame() {
        let drops = Arc::new(AtomicUsize::new(0));
        let result = create_transition_window(counted_state(&drops), |_| null_mut());
        assert!(result.is_err());
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn transition_creation_userdata_install_failure_keeps_caller_ownership() {
        let drops = Arc::new(AtomicUsize::new(0));
        let accepted = Cell::new(true);
        let result = create_transition_window(counted_state(&drops), |parameter| {
            accepted.set(unsafe { accept_created_window(parameter, |_| false) });
            null_mut()
        });
        assert!(result.is_err());
        assert!(
            !accepted.get(),
            "failed publication must reject WM_NCCREATE"
        );
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn transition_creation_partial_accept_then_destroy_releases_exactly_once() {
        let drops = Arc::new(AtomicUsize::new(0));
        let userdata = Cell::new(null_mut());
        let result = create_transition_window(counted_state(&drops), |parameter| {
            assert!(unsafe {
                accept_created_window(parameter, |state| {
                    userdata.set(state);
                    true
                })
            });
            assert_eq!(drops.load(Ordering::SeqCst), 0);
            unsafe { destroy_created_window(|| userdata.replace(null_mut())) };
            assert_eq!(drops.load(Ordering::SeqCst), 1);
            null_mut()
        });
        assert!(result.is_err());
        assert!(userdata.get().is_null());
        assert_eq!(
            drops.load(Ordering::SeqCst),
            1,
            "null return must not free accepted state again"
        );
    }

    #[test]
    fn transition_creation_success_retains_state_until_window_teardown() {
        let drops = Arc::new(AtomicUsize::new(0));
        let userdata = Cell::new(null_mut());
        let result = create_transition_window(counted_state(&drops), |parameter| {
            assert!(unsafe {
                accept_created_window(parameter, |state| {
                    userdata.set(state);
                    true
                })
            });
            created_handle()
        });
        assert_eq!(result.unwrap(), created_handle());
        assert_eq!(drops.load(Ordering::SeqCst), 0);
        let state = userdata.get();
        assert!(!state.is_null());
        // Exercise the same retained frame object used by CPU render/paint.
        let next = FrozenFrame {
            width: 64,
            height: 32,
            rgba: vec![23; 64 * 32 * 4],
        };
        unsafe {
            (*state).set_frame(&next);
            assert_eq!((*state).bgra.len(), 64 * 32 * 4);
            assert!((*state).bgra.iter().all(|value| *value == 23));
            destroy_created_window(|| userdata.replace(null_mut()));
            destroy_created_window(|| userdata.replace(null_mut()));
        }
        assert_eq!(drops.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn transition_creation_repeated_injected_failures_do_not_retain_frames() {
        let drops = Arc::new(AtomicUsize::new(0));
        for _ in 0..64 {
            assert!(create_transition_window(counted_state(&drops), |_| null_mut()).is_err());
            let userdata = Cell::new(null_mut());
            assert!(
                create_transition_window(counted_state(&drops), |parameter| {
                    assert!(unsafe {
                        accept_created_window(parameter, |state| {
                            userdata.set(state);
                            true
                        })
                    });
                    unsafe { destroy_created_window(|| userdata.replace(null_mut())) };
                    null_mut()
                })
                .is_err()
            );
        }
        assert_eq!(
            drops.load(Ordering::SeqCst),
            128,
            "every allocated frame must be released"
        );
    }

    #[test]
    fn transition_creation_failed_attempt_does_not_destroy_surviving_frame() {
        let survivor_drops = Arc::new(AtomicUsize::new(0));
        let failed_drops = Arc::new(AtomicUsize::new(0));
        let survivor = Cell::new(null_mut());
        create_transition_window(counted_state(&survivor_drops), |parameter| {
            assert!(unsafe {
                accept_created_window(parameter, |state| {
                    survivor.set(state);
                    true
                })
            });
            created_handle()
        })
        .unwrap();
        assert!(create_transition_window(counted_state(&failed_drops), |_| null_mut()).is_err());
        assert_eq!(failed_drops.load(Ordering::SeqCst), 1);
        assert_eq!(survivor_drops.load(Ordering::SeqCst), 0);
        unsafe {
            assert_eq!((*survivor.get()).bgra.len(), 64 * 32 * 4);
            destroy_created_window(|| survivor.replace(null_mut()));
        }
        assert_eq!(survivor_drops.load(Ordering::SeqCst), 1);
    }

    struct WallpaperRestore {
        device_id: String,
        original: PathBuf,
    }

    impl Drop for WallpaperRestore {
        fn drop(&mut self) {
            let _ = crate::native_wallpaper::restore_wallpaper_for_device(
                &self.original,
                &self.device_id,
            );
        }
    }

    #[test]
    fn windows_wallpaper_host_is_discoverable() {
        assert!(!wallpaper_host().unwrap().is_null());
    }

    #[test]
    #[ignore = "changes and restores the actual primary-monitor wallpaper"]
    fn runtime_local_transition_restores_wallpaper() {
        assert!(
            !crate::engine_ipc::is_running(),
            "stop Wallpaper Engine before the native transition runtime check"
        );
        let monitor = crate::native_wallpaper::monitors()
            .unwrap()
            .into_iter()
            .next()
            .expect("at least one desktop monitor");
        let original = crate::native_wallpaper::get_wallpaper_for_device(&monitor.device_id)
            .expect("current wallpaper path");
        eprintln!(
            "recorded wallpaper state: device={} path={}",
            monitor.device_id,
            original.display()
        );
        let restore = WallpaperRestore {
            device_id: monitor.device_id.clone(),
            original: original.clone(),
        };

        let test_dir = crate::config::Config::cache_dir().join("runtime-transition-test");
        std::fs::create_dir_all(&test_dir).unwrap();
        let first_path = test_dir.join("first.bmp");
        let second_path = test_dir.join("second.bmp");
        image::RgbaImage::from_pixel(64, 64, image::Rgba([35, 33, 54, 255]))
            .save(&first_path)
            .unwrap();
        image::RgbaImage::from_pixel(64, 64, image::Rgba([235, 111, 146, 255]))
            .save(&second_path)
            .unwrap();

        crate::native_wallpaper::set_wallpaper_for_device(&first_path, &monitor.device_id).unwrap();
        let outgoing =
            FrozenFrame::from_path_cover(&first_path, monitor.rect.width(), monitor.rect.height())
                .unwrap();
        let incoming =
            FrozenFrame::from_path_cover(&second_path, monitor.rect.width(), monitor.rect.height())
                .unwrap();
        let mut session = DesktopTransitionSession::show(outgoing, monitor.rect).unwrap();
        crate::native_wallpaper::set_wallpaper_for_device(&second_path, &monitor.device_id)
            .unwrap();
        session
            .play(
                &incoming,
                TransitionEffect::Crossfade,
                Duration::from_millis(180),
            )
            .unwrap();
        drop(session);
        let current =
            crate::native_wallpaper::get_wallpaper_for_device(&monitor.device_id).unwrap();
        assert_eq!(
            std::fs::canonicalize(current).unwrap(),
            std::fs::canonicalize(&second_path).unwrap()
        );
        drop(restore);
        let restored =
            crate::native_wallpaper::get_wallpaper_for_device(&monitor.device_id).unwrap();
        assert_eq!(
            std::fs::canonicalize(restored).unwrap(),
            std::fs::canonicalize(original).unwrap(),
            "the original per-monitor wallpaper was not restored"
        );
        let _ = std::fs::remove_dir_all(test_dir);
    }
}

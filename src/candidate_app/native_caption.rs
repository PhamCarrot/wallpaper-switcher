//! Keep activation painting out of the transparent, undecorated client surface.

use super::Message;
use iced::{window, Task};

pub(super) fn install(id: window::Id) -> Task<Message> {
    window::run(id, |window| {
        use iced::window::raw_window_handle::RawWindowHandle;
        if let Ok(handle) = window.window_handle() {
            if let RawWindowHandle::Win32(handle) = handle.as_raw() {
                // iced runs this callback on the owning window/event-loop thread.
                let installed = unsafe { install_on_window(handle.hwnd.get() as _) };
                crate::log_debug(&format!(
                    "Native caption activation filter installed={installed}"
                ));
            }
        }
    })
    .discard()
}

use windows_sys::Win32::{
    Foundation::{GetLastError, SetLastError, HWND, LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{
        CallWindowProcW, DefWindowProcW, GetPropW, GetWindowLongPtrW, IsIconic, RemovePropW,
        SetPropW, SetWindowLongPtrW, GWLP_WNDPROC, GWL_STYLE, WM_NCACTIVATE, WM_NCDESTROY,
        WS_CAPTION,
    },
};

const ORIGINAL_PROC: *const u16 = windows_sys::w!("WallpaperSwitcher.NativeCaption.OriginalProc");

unsafe fn install_on_window(hwnd: HWND) -> bool {
    // Caller owns this live HWND on its event-loop thread; the box belongs to
    // the window property until installation fails or WM_NCDESTROY completes.
    if !unsafe { GetPropW(hwnd, ORIGINAL_PROC) }.is_null() {
        return true;
    }
    let original = unsafe { GetWindowLongPtrW(hwnd, GWLP_WNDPROC) };
    if original == 0 {
        return false;
    }
    // Publish before replacing the procedure so reentrant messages can forward.
    let state = Box::into_raw(Box::new(original));
    if unsafe { SetPropW(hwnd, ORIGINAL_PROC, state.cast()) } == 0 {
        unsafe { drop(Box::from_raw(state)) };
        return false;
    }
    unsafe { SetLastError(0) };
    let previous =
        unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, caption_proc as *const () as isize) };
    if previous == 0 && unsafe { GetLastError() } != 0 {
        unsafe {
            RemovePropW(hwnd, ORIGINAL_PROC);
            drop(Box::from_raw(state));
        }
        return false;
    }
    // Keep the exact opaque value returned by user32, for CallWindowProcW.
    unsafe { *state = previous };
    true
}

unsafe extern "system" fn caption_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state = unsafe { GetPropW(hwnd, ORIGINAL_PROC) }.cast::<isize>();
    if state.is_null() {
        return unsafe { DefWindowProcW(hwnd, message, wparam, lparam) };
    }
    let original = unsafe { *state };
    let mut forwarded_lparam = lparam;
    if message == WM_NCACTIVATE
        && unsafe { GetWindowLongPtrW(hwnd, GWL_STYLE) } & WS_CAPTION as isize == 0
        && unsafe { IsIconic(hwnd) } == 0
    {
        // Preserve winit's activation/focus routing, but suppress DefWindowProc's
        // nonclient repaint into our transparent surface (WM_NCACTIVATE contract).
        forwarded_lparam = -1;
        let _ = std::panic::catch_unwind(|| {
            crate::apply_trace::event_lazy("ui-native-caption-activation", || {
                format!(
                    "hwnd={} active={} original_lparam={lparam} repaint=false",
                    hwnd as usize,
                    wparam != 0
                )
            });
        });
    }
    // Do not replace a newer procedure above us in the chain. Keep our state
    // available until the original finishes destruction and any nested messages.
    let result = unsafe {
        CallWindowProcW(
            std::mem::transmute(original),
            hwnd,
            message,
            wparam,
            forwarded_lparam,
        )
    };
    if message == WM_NCDESTROY {
        unsafe {
            RemovePropW(hwnd, ORIGINAL_PROC);
            drop(Box::from_raw(state));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, SendMessageW, GWLP_USERDATA, WM_CLOSE, WM_KEYDOWN,
        WS_MINIMIZE, WS_POPUP,
    };

    #[derive(Default)]
    struct Observation {
        message: u32,
        wparam: usize,
        lparam: isize,
        filter_at_destroy: bool,
        original: isize,
    }

    unsafe extern "system" fn observe(
        hwnd: HWND,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        let observed =
            unsafe { &mut *(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut Observation) };
        observed.message = message;
        observed.wparam = wparam;
        observed.lparam = lparam;
        if message == WM_NCDESTROY {
            observed.filter_at_destroy = !unsafe { GetPropW(hwnd, ORIGINAL_PROC) }.is_null();
        }
        // Keep the hidden test window alive through WM_CLOSE for the next assertion.
        if message == WM_CLOSE || message == WM_NCACTIVATE || message == WM_KEYDOWN {
            return 1;
        }
        unsafe {
            CallWindowProcW(
                std::mem::transmute(observed.original),
                hwnd,
                message,
                wparam,
                lparam,
            )
        }
    }

    struct OwnedWindow(HWND);
    impl Drop for OwnedWindow {
        fn drop(&mut self) {
            unsafe { DestroyWindow(self.0) };
        }
    }

    #[test]
    fn caption_filter_preserves_activation_input_close_and_decorated_paint() {
        assert!(!unsafe { install_on_window(std::ptr::null_mut()) });
        for style in [WS_POPUP, WS_CAPTION, WS_POPUP | WS_MINIMIZE] {
            let mut observed = Observation::default();
            let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class.as_ptr(),
                    std::ptr::null(),
                    style,
                    0,
                    0,
                    64,
                    64,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                )
            };
            assert!(!hwnd.is_null());
            let owned = OwnedWindow(hwnd);
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, &mut observed as *mut _ as isize) };
            observed.original =
                unsafe { SetWindowLongPtrW(hwnd, GWLP_WNDPROC, observe as *const () as isize) };
            assert_ne!(observed.original, 0);
            assert!(unsafe { install_on_window(hwnd) });
            assert!(
                unsafe { install_on_window(hwnd) },
                "installation is idempotent"
            );
            for active in [0, 1, 0, 1] {
                assert_eq!(unsafe { SendMessageW(hwnd, WM_NCACTIVATE, active, 17) }, 1);
                assert_eq!(observed.message, WM_NCACTIVATE);
                assert_eq!(observed.wparam, active);
                assert_eq!(observed.lparam, if style == WS_POPUP { -1 } else { 17 });
            }
            for message in [WM_KEYDOWN, WM_CLOSE] {
                assert_eq!(unsafe { SendMessageW(hwnd, message, 42, 73) }, 1);
                assert_eq!(
                    (observed.message, observed.wparam, observed.lparam),
                    (message, 42, 73)
                );
            }
            assert!(!unsafe { GetPropW(hwnd, ORIGINAL_PROC) }.is_null());
            drop(owned);
            assert_eq!(observed.message, WM_NCDESTROY);
            assert!(
                observed.filter_at_destroy,
                "original retained throughout destruction forwarding"
            );
            assert!(unsafe { GetPropW(hwnd, ORIGINAL_PROC) }.is_null());
        }
    }
}

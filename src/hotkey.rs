use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, WM_HOTKEY};

pub fn parse_hotkey(hotkey_str: &str) -> Option<(u32, u32)> {
    let parts: Vec<&str> = hotkey_str.split('+').map(|s| s.trim()).collect();
    let mut modifiers = MOD_NOREPEAT as u32;
    let mut vk = 0u32;

    for part in parts {
        match part.to_lowercase().as_str() {
            // Hyper = Ctrl + Shift + Win + Alt (all four modifiers at once)
            "hyper" => {
                modifiers |= MOD_CONTROL as u32 | MOD_SHIFT as u32 | MOD_WIN as u32 | MOD_ALT as u32
            }
            "ctrl" | "control" => modifiers |= MOD_CONTROL as u32,
            "alt" => modifiers |= MOD_ALT as u32,
            "shift" => modifiers |= MOD_SHIFT as u32,
            "win" | "super" | "windows" => modifiers |= MOD_WIN as u32,
            "space" => vk = 0x20,
            "tab" => vk = 0x09,
            "return" | "enter" => vk = 0x0D,
            "escape" | "esc" => vk = 0x1B,
            "backspace" | "back" => vk = 0x08,
            "delete" | "del" => vk = 0x2E,
            "insert" | "ins" => vk = 0x2D,
            "home" => vk = 0x24,
            "end" => vk = 0x23,
            "pageup" | "page_up" | "pgup" | "prior" => vk = 0x21,
            "pagedown" | "page_down" | "pgdn" | "next" => vk = 0x22,
            "up" | "arrowup" | "arrow_up" => vk = 0x26,
            "down" | "arrowdown" | "arrow_down" => vk = 0x28,
            "left" | "arrowleft" | "arrow_left" => vk = 0x25,
            "right" | "arrowright" | "arrow_right" => vk = 0x27,
            s if s.len() == 1 => {
                let c = s.chars().next().unwrap();
                if c.is_ascii_alphanumeric() {
                    vk = c.to_ascii_uppercase() as u32;
                }
            }
            s if s.starts_with('f') => {
                if let Ok(num) = s[1..].parse::<u32>() {
                    if (1..=24).contains(&num) {
                        vk = 0x70 + (num - 1);
                    }
                }
            }
            _ => {}
        }
    }

    if vk != 0 {
        crate::log_debug(&format!(
            "parse_hotkey('{}') -> modifiers={:#x}, vk={:#x}",
            hotkey_str, modifiers, vk
        ));
        Some((modifiers, vk))
    } else {
        crate::log_debug(&format!(
            "parse_hotkey('{}') FAILED: no recognized key",
            hotkey_str
        ));
        None
    }
}

pub struct GlobalHotkeyListener {
    thread_id: u32,
    handle: Option<thread::JoinHandle<()>>,
    stop: Arc<AtomicBool>,
}

impl GlobalHotkeyListener {
    pub fn spawn<F>(hotkey_str: String, on_trigger: F) -> Self
    where
        F: Fn() + Send + 'static,
    {
        let stop = Arc::new(AtomicBool::new(false));
        let (handle, rx) = Self::start(
            hotkey_str,
            on_trigger,
            stop.clone(),
            #[cfg(test)]
            None,
        );
        Self {
            thread_id: rx.recv().unwrap_or(0),
            handle: Some(handle),
            stop,
        }
    }

    fn start<F>(
        hotkey_str: String,
        on_trigger: F,
        stop: Arc<AtomicBool>,
        #[cfg(test)] control: Option<tests::StartupControl>,
    ) -> (thread::JoinHandle<()>, mpsc::Receiver<u32>)
    where
        F: Fn() + Send + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let handle = thread::spawn(move || {
            let tid = unsafe { windows_sys::Win32::System::Threading::GetCurrentThreadId() };
            #[cfg(test)]
            if let Some(control) = &control {
                control.before_queue.send(()).unwrap();
                control.release_queue.recv().unwrap();
            }
            let mut msg: MSG = unsafe { std::mem::zeroed() };
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::{PeekMessageW, PM_NOREMOVE};
                // An empty result still creates the queue; publish readiness only afterward.
                PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
            }
            let _ = tx.send(tid);
            #[cfg(test)]
            if let Some(control) = &control {
                control.release_ready.recv().unwrap();
            }

            let (modifiers, vk) = match parse_hotkey(&hotkey_str) {
                Some(pair) => pair,
                None => {
                    crate::log_debug(&format!(
                        "Hotkey '{}' failed to parse, falling back to Ctrl+Alt+G",
                        hotkey_str
                    ));
                    (
                        MOD_CONTROL as u32 | MOD_ALT as u32 | MOD_NOREPEAT as u32,
                        b'G' as u32,
                    )
                }
            };

            const HOTKEY_ID: i32 = 1001;
            unsafe {
                use windows_sys::Win32::Foundation::{GetLastError, WAIT_FAILED};
                use windows_sys::Win32::UI::WindowsAndMessaging::{
                    MsgWaitForMultipleObjectsEx, PeekMessageW, MWMO_INPUTAVAILABLE, PM_REMOVE,
                    QS_ALLINPUT, WM_QUIT,
                };

                let mut ok = RegisterHotKey(std::ptr::null_mut(), HOTKEY_ID, modifiers, vk);
                if ok == 0 {
                    let err = GetLastError();
                    crate::log_debug(&format!(
                        "RegisterHotKey with MOD_NOREPEAT failed (err {}), retrying without it...",
                        err
                    ));
                    let fallback_mod = modifiers & !(MOD_NOREPEAT as u32);
                    ok = RegisterHotKey(std::ptr::null_mut(), HOTKEY_ID, fallback_mod, vk);
                    if ok == 0 {
                        crate::log_debug(&format!(
                            "RegisterHotKey completely failed! LastError: {}",
                            GetLastError()
                        ));
                    } else {
                        crate::log_debug("RegisterHotKey succeeded (without MOD_NOREPEAT)");
                    }
                } else {
                    crate::log_debug("RegisterHotKey succeeded with MOD_NOREPEAT");
                }

                #[cfg(test)]
                if let Some(control) = &control {
                    control.registered.send(ok != 0).unwrap();
                }
                'messages: while !stop.load(Ordering::Acquire) {
                    while PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                        if msg.message == WM_QUIT || stop.load(Ordering::Acquire) {
                            break 'messages;
                        }
                        if msg.message == WM_HOTKEY && msg.wParam == HOTKEY_ID as usize {
                            crate::log_debug(
                                "WM_HOTKEY received in hotkey thread -> firing on_trigger",
                            );
                            on_trigger();
                        }
                    }
                    // Quit normally wakes this wait immediately. The timeout also observes
                    // stop if posting fails (for example, a full message queue).
                    if MsgWaitForMultipleObjectsEx(
                        0,
                        std::ptr::null(),
                        250,
                        QS_ALLINPUT,
                        MWMO_INPUTAVAILABLE,
                    ) == WAIT_FAILED
                    {
                        crate::log_debug(&format!(
                            "Hotkey message wait failed: {}",
                            GetLastError()
                        ));
                        break;
                    }
                }
                UnregisterHotKey(std::ptr::null_mut(), HOTKEY_ID);
                crate::log_debug("Hotkey listener thread exiting cleanly");
            }
        });

        (handle, rx)
    }
}

impl Drop for GlobalHotkeyListener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if self.thread_id != 0 {
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_QUIT};
                if PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) == 0 {
                    crate::log_debug(&format!(
                        "Hotkey quit post failed: {}; listener will observe stop at its bounded wait",
                        windows_sys::Win32::Foundation::GetLastError()
                    ));
                }
            }
        }
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::UI::WindowsAndMessaging::{PostThreadMessageW, WM_NULL};

    pub(super) struct StartupControl {
        pub before_queue: mpsc::SyncSender<()>,
        pub release_queue: mpsc::Receiver<()>,
        pub release_ready: mpsc::Receiver<()>,
        pub registered: mpsc::Sender<bool>,
    }

    #[test]
    fn queue_readiness_and_early_error_teardown_settle() {
        const CHILD: &str = "WSL_HOTKEY_SETTLEMENT_CHILD";
        if std::env::var_os(CHILD).is_none() {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "hotkey::tests::queue_readiness_and_early_error_teardown_settle",
                    "--nocapture",
                    "--test-threads=1",
                ])
                .env(CHILD, "1")
                .spawn()
                .expect("owned hotkey regression child");
            let deadline = Instant::now() + Duration::from_secs(15);
            loop {
                if let Some(status) = child.try_wait().unwrap() {
                    assert!(
                        status.success(),
                        "hotkey child {} failed: {status}",
                        child.id()
                    );
                    return;
                }
                if Instant::now() >= deadline {
                    child.kill().expect("stop exact owned hanging child");
                    child.wait().expect("reap exact owned hanging child");
                    panic!(
                        "hotkey child {} exceeded settlement deadline; killed and reaped",
                        child.id()
                    );
                }
                thread::sleep(Duration::from_millis(10));
            }
        }

        for failed_quit_post in [false, true] {
            let (entered_tx, entered_rx) = mpsc::sync_channel(0);
            let (queue_tx, queue_rx) = mpsc::channel();
            let (ready_tx, ready_rx) = mpsc::channel();
            let (registered_tx, registered_rx) = mpsc::channel();
            let (trigger_tx, trigger_rx) = mpsc::channel();
            let stop = Arc::new(AtomicBool::new(false));
            let (handle, readiness) = GlobalHotkeyListener::start(
                "Ctrl+Alt+Shift+Win+F24".into(),
                move || trigger_tx.send(()).unwrap(),
                stop.clone(),
                Some(StartupControl {
                    before_queue: entered_tx,
                    release_queue: queue_rx,
                    release_ready: ready_rx,
                    registered: registered_tx,
                }),
            );
            entered_rx.recv_timeout(Duration::from_secs(2)).unwrap();
            // The worker is held before the real queue call; inspect its actual readiness receiver.
            assert!(matches!(
                readiness.try_recv(),
                Err(mpsc::TryRecvError::Empty)
            ));
            queue_tx.send(()).unwrap();
            let tid = readiness.recv_timeout(Duration::from_secs(2)).unwrap();
            assert_ne!(
                unsafe { PostThreadMessageW(tid, WM_NULL, 0, 0) },
                0,
                "readiness requires a real queue"
            );
            let listener = GlobalHotkeyListener {
                thread_id: if failed_quit_post { 1 } else { tid },
                handle: Some(handle),
                stop: stop.clone(),
            };
            if failed_quit_post {
                ready_tx.send(()).unwrap();
                assert!(
                    registered_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
                    "owned test binding unavailable; no native trigger credit"
                );
                assert_ne!(unsafe { PostThreadMessageW(tid, WM_HOTKEY, 1001, 0) }, 0);
                trigger_rx.recv_timeout(Duration::from_secs(2)).unwrap();
                assert_eq!(
                    unsafe { PostThreadMessageW(1, WM_NULL, 0, 0) },
                    0,
                    "invalid test ID must exercise failed post"
                );
                drop(listener);
            } else {
                // Simulate owner cleanup on startup error while the ready worker is paused.
                let cleanup = thread::spawn(move || drop(listener));
                while !stop.load(Ordering::Acquire) {
                    thread::yield_now();
                }
                ready_tx.send(()).unwrap();
                cleanup.join().unwrap();
            }
            // Only our registration was released; never unregister another owner's binding.
            unsafe {
                assert_ne!(
                    RegisterHotKey(
                        std::ptr::null_mut(),
                        1002,
                        MOD_CONTROL | MOD_ALT | MOD_SHIFT | MOD_WIN | MOD_NOREPEAT,
                        0x87
                    ),
                    0
                );
                assert_ne!(UnregisterHotKey(std::ptr::null_mut(), 1002), 0);
            }
            eprintln!("native queue/registration/callback/teardown settled; failed_quit_post={failed_quit_post}");
        }
    }
}

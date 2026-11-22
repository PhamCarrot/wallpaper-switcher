use super::*;
use crate::desktop_transition::{window_is_above, DesktopTransitionSession};
use crate::transition::TransitionEffect;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::process::{Command, Stdio};
use std::ptr::{null, null_mut};
use windows::core::{Interface, PCWSTR};
use windows::Win32::Foundation::{HWND as WinHwnd, POINT};
use windows::Win32::Graphics::Direct3D11::{ID3D11Texture2D, D3D11_BOX};
use windows::Win32::Graphics::DirectComposition::{
    DCompositionCreateDevice, IDCompositionDevice, IDCompositionVisual,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_ALPHA_MODE_PREMULTIPLIED, DXGI_FORMAT_B8G8R8A8_UNORM,
};
use windows::Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED};
use windows_sys::Win32::Foundation::ERROR_CLASS_ALREADY_EXISTS;
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, IsWindow, RegisterClassExW,
    SetLayeredWindowAttributes, SetWindowPos, HWND_BOTTOM, HWND_TOP, LWA_ALPHA, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, WNDCLASSEXW, WS_CHILD, WS_CLIPCHILDREN,
    WS_CLIPSIBLINGS, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_NOREDIRECTIONBITMAP, WS_EX_TOOLWINDOW,
    WS_POPUP,
};

struct OwnedWindow(HWND);

const SOURCE_FIXTURE_ENV: &str = "WALLPAPER_SWITCHER_OWNED_CAPTURE_SOURCE";
const FIXTURE_TEST: &str =
    "engine_capture::owned_fixture::owned_desktop_layers_capture_new_pixels_and_play_above_renderer_below_icons";
type StripeColors = [[u8; 4]; 2];

impl OwnedWindow {
    fn show(class: PCWSTR, parent: HWND, rect: DesktopRect, extended: u32) -> Self {
        let instance = unsafe { GetModuleHandleW(null()) };
        let class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(DefWindowProcW),
            hInstance: instance,
            lpszClassName: class.as_ptr(),
            ..unsafe { std::mem::zeroed() }
        };
        let atom = unsafe { RegisterClassExW(&class) };
        assert!(
            atom != 0
                || std::io::Error::last_os_error().raw_os_error()
                    == Some(ERROR_CLASS_ALREADY_EXISTS as i32)
        );
        let styles = if parent.is_null() { WS_POPUP } else { WS_CHILD };
        let hwnd = unsafe {
            CreateWindowExW(
                extended,
                class.lpszClassName,
                null(),
                styles | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                rect.left,
                rect.top,
                rect.width() as i32,
                rect.height() as i32,
                parent,
                null_mut(),
                instance,
                null_mut(),
            )
        };
        assert!(!hwnd.is_null(), "{}", last_error("create fixture window"));
        let window = Self(hwnd);
        if extended & WS_EX_LAYERED != 0 {
            assert_ne!(
                unsafe { SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA) },
                0
            );
        }
        let behind = if parent.is_null() {
            HWND_BOTTOM
        } else {
            HWND_TOP
        };
        assert_ne!(
            unsafe {
                SetWindowPos(
                    hwnd,
                    behind,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
                )
            },
            0
        );
        window
    }
}

impl Drop for OwnedWindow {
    fn drop(&mut self) {
        unsafe { DestroyWindow(self.0) };
    }
}

fn stripes(width: u32, height: u32, colors: [[u8; 4]; 2]) -> FrozenFrame {
    let mut rgba = Vec::new();
    for _ in 0..height {
        for x in 0..width {
            rgba.extend(colors[usize::from(x >= width / 2)]);
        }
    }
    FrozenFrame {
        width,
        height,
        rgba,
    }
}

fn draw_layer(
    device: &IDCompositionDevice,
    context: &ID3D11DeviceContext,
    visual: &IDCompositionVisual,
    frame: &FrozenFrame,
) {
    let mut bgra = frame.rgba.clone();
    for pixel in bgra.chunks_exact_mut(4) {
        pixel.swap(0, 2);
    }
    unsafe {
        let surface = device
            .CreateSurface(
                frame.width,
                frame.height,
                DXGI_FORMAT_B8G8R8A8_UNORM,
                DXGI_ALPHA_MODE_PREMULTIPLIED,
            )
            .unwrap();
        let mut origin = POINT::default();
        let texture: ID3D11Texture2D = surface.BeginDraw(None, &mut origin).unwrap();
        let area = D3D11_BOX {
            left: origin.x as u32,
            top: origin.y as u32,
            front: 0,
            right: origin.x as u32 + frame.width,
            bottom: origin.y as u32 + frame.height,
            back: 1,
        };
        context.UpdateSubresource(
            &texture,
            0,
            Some(&area),
            bgra.as_ptr().cast(),
            frame.width * 4,
            0,
        );
        surface.EndDraw().unwrap();
        visual.SetContent(&surface).unwrap();
        device.Commit().unwrap();
        device.WaitForCommitCompletion().unwrap();
        assert!(DwmFlush() >= 0);
    }
}

fn frame(result: Result<PresenterAttempt<FrozenFrame>, String>) -> FrozenFrame {
    match result {
        Ok(PresenterAttempt::Ready(frame)) => frame,
        Ok(PresenterAttempt::Retry(reason)) | Err(reason) => panic!("{reason}"),
    }
}

fn pixel(frame: &FrozenFrame, x: u32, y: u32) -> [u8; 4] {
    let start = 4 * (y * frame.width + x) as usize;
    frame.rgba[start..start + 4].try_into().unwrap()
}

#[test]
#[ignore = "PrintWindow readback of owned nonactivating fixture windows; explicit invocation"]
fn isolated_shell_worker_supplies_pixels_and_rejects_unsafe_changes() {
    use std::cell::Cell;
    use windows_sys::Win32::UI::WindowsAndMessaging::SetParent;

    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.unwrap();
    let _apartment = Apartment;
    let graphics = CaptureGraphics::new().unwrap();
    let dxgi: IDXGIDevice = graphics.device.cast().unwrap();
    let composition: IDCompositionDevice = unsafe { DCompositionCreateDevice(&dxgi) }.unwrap();
    let _coordinates = PhysicalWindowCoordinates::enter().unwrap();
    let rect = |left, right| DesktopRect {
        left,
        top: 0,
        right,
        bottom: 96,
    };
    let host = OwnedWindow::show(
        windows::core::w!("OwnedCaptureShell"),
        null_mut(),
        rect(0, 128),
        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
    );
    let worker = OwnedWindow::show(
        windows::core::w!("WorkerW"),
        host.0,
        rect(0, 128),
        WS_EX_LAYERED | WS_EX_NOREDIRECTIONBITMAP,
    );
    let _sibling_icons = OwnedWindow::show(
        windows::core::w!("SHELLDLL_DefView"),
        host.0,
        rect(0, 16),
        0,
    );
    let renderer = OwnedWindow::show(
        windows::core::w!("WPEDesktopFallbackFixture"),
        worker.0,
        rect(16, 112),
        WS_EX_NOREDIRECTIONBITMAP,
    );
    let worker_surface =
        unsafe { composition.CreateTargetForHwnd(WinHwnd(worker.0), true) }.unwrap();
    let visual = unsafe { composition.CreateVisual() }.unwrap();
    unsafe { worker_surface.SetRoot(&visual) }.unwrap();
    let mut pattern = stripes(128, 96, [[1, 2, 3, 255], [64, 176, 55, 255]]);
    for row in pattern.rgba.chunks_exact_mut(128 * 4) {
        for pixel in row[..16 * 4].chunks_exact_mut(4) {
            pixel.copy_from_slice(&[64, 128, 138, 255]);
        }
    }
    draw_layer(&composition, &graphics.context, &visual, &pattern);
    let model = inspect_engine_window(renderer.0, std::process::id()).unwrap();
    let capture = CaptureWindow::from_presenter(&model).unwrap();
    let target = model.rect;
    let progress = Mutex::default();
    let reader = |hwnd, rect: DesktopRect| {
        if hwnd == renderer.0 {
            // Some Windows builds paint blank child windows opaque black.
            // Model the observed DX11 readback that leaves the DIB untouched.
            Ok(FrozenFrame {
                width: rect.width(),
                height: rect.height(),
                rgba: vec![UNWRITTEN_PIXEL; (rect.width() * rect.height() * 4) as usize],
            })
        } else {
            copy_window_pixels(hwnd, rect)
        }
    };
    let read = |shell: &dyn Fn() -> HWND| {
        read_isolated_presenter(
            &capture,
            target,
            &[],
            Instant::now() + Duration::from_secs(2),
            &|| true,
            &progress,
            shell,
            reader,
        )
    };
    let captured = frame(read(&|| host.0));
    eprintln!("selected capture: {}", progress.lock().unwrap());
    assert_eq!(pixel(&captured, 0, 48), [1, 2, 3, 255]);
    assert_eq!(pixel(&captured, 95, 48), [64, 176, 55, 255]);
    assert!(progress.lock().unwrap().contains("wallpaper WorkerW"));

    let unavailable = rect(112, 208);
    assert!(wallpaper_worker(&capture, unavailable, &[], host.0)
        .unwrap_err()
        .contains("physical target"));
    let blocker = OwnedWindow::show(
        windows::core::w!("SHELLDLL_DefView"),
        worker.0,
        rect(0, 16),
        0,
    );
    assert!(wallpaper_worker(&capture, target, &[], host.0)
        .unwrap_err()
        .contains("icons"));
    drop(blocker);
    let blocker = OwnedWindow::show(
        windows::core::w!("OwnedFixtureCover"),
        worker.0,
        rect(0, 16),
        0,
    );
    assert!(
        wallpaper_worker(&capture, target, &[blocker.0 as usize], host.0)
            .unwrap_err()
            .contains("cover")
    );
    assert!(wallpaper_worker(&capture, target, &[], host.0)
        .unwrap_err()
        .contains("another visible window"));
    drop(blocker);

    let replacement = OwnedWindow::show(windows::core::w!("WorkerW"), host.0, rect(0, 128), 0);
    let validations = Cell::new(0);
    let replaced = read(&|| {
        validations.set(validations.get() + 1);
        if validations.get() == 3 {
            assert_eq!(unsafe { SetParent(renderer.0, replacement.0) }, worker.0);
        }
        host.0
    })
    .unwrap();
    assert!(
        matches!(replaced, PresenterAttempt::Retry(reason) if reason.contains("wallpaper WorkerW") && reason.contains("changed"))
    );
}

fn serve_owned_renderer(
    composition: &IDCompositionDevice,
    context: &ID3D11DeviceContext,
    target: DesktopRect,
    client: DesktopRect,
    old: StripeColors,
    new: StripeColors,
) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, PeekMessageW, MSG, PM_REMOVE,
    };

    let host = OwnedWindow::show(
        windows::core::w!("OwnedForeignWallpaperHost"),
        null_mut(),
        target,
        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP,
    );
    let renderer = OwnedWindow::show(
        windows::core::w!("WPEVideoWallpaper"),
        host.0,
        client,
        WS_EX_LAYERED | WS_EX_NOREDIRECTIONBITMAP,
    );
    let cover = OwnedWindow::show(
        windows::core::w!("OwnedForeignWallpaperCover"),
        host.0,
        client,
        WS_EX_LAYERED,
    );
    let bind = |hwnd, colors| unsafe {
        let target = composition
            .CreateTargetForHwnd(WinHwnd(hwnd), false)
            .unwrap();
        let visual = composition.CreateVisual().unwrap();
        target.SetRoot(&visual).unwrap();
        draw_layer(composition, context, &visual, &stripes(96, 96, colors));
        target
    };
    let _renderer_target = bind(renderer.0, new);
    let _cover_target = bind(cover.0, old);
    println!(
        "owned-windows {} {} {}",
        host.0 as usize, renderer.0 as usize, cover.0 as usize
    );
    std::io::stdout().flush().unwrap();
    let (done, wait) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = std::io::stdin().read_line(&mut String::new());
        let _ = done.send(());
    });
    while wait.try_recv().is_err() {
        unsafe {
            let mut message: MSG = std::mem::zeroed();
            while PeekMessageW(&mut message, null_mut(), 0, 0, PM_REMOVE) != 0 {
                DispatchMessageW(&message);
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    }
}

fn check_foreign_presenter(
    graphics: &CaptureGraphics,
    target: DesktopRect,
    old: StripeColors,
    new: StripeColors,
) {
    let mut source = Command::new(std::env::current_exe().unwrap())
        .args(["--ignored", "--exact", FIXTURE_TEST, "--nocapture"])
        .env(SOURCE_FIXTURE_ENV, "1")
        .creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut source_input = source.stdin.take().unwrap();
    let mut source_output = BufReader::new(source.stdout.take().unwrap());
    let native_windows: Vec<usize> = (&mut source_output)
        .lines()
        .map_while(Result::ok)
        .find_map(|line| {
            line.split_once("owned-windows ")
                .map(|(_, ids)| ids.to_owned())
        })
        .expect("the owned renderer process must report its windows")
        .split_whitespace()
        .map(|hwnd| hwnd.parse().unwrap())
        .collect();
    let [root, renderer, cover]: [usize; 3] = native_windows.try_into().unwrap();
    let inspect = || inspect_engine_window(renderer as HWND, source.id());
    assert_ne!(source.id(), std::process::id());
    let presenter = CaptureWindow::from_presenter(&inspect().unwrap()).unwrap();
    assert_eq!(presenter.root as usize, root);
    assert!(shares_capture_root(presenter.root, target, &[cover]));

    let progress = Mutex::default();
    let at = || Instant::now() + Duration::from_secs(2);
    let visible = frame(graphics.read_window(presenter.root, at(), &|| true, &progress));
    assert_eq!([pixel(&visible, 22, 48), pixel(&visible, 74, 48)], old);
    let incoming = frame(read_presenter(
        &mut None,
        &presenter,
        target,
        &[cover],
        at(),
        || {
            inspect()
                .and_then(|window| CaptureWindow::from_presenter(&window))
                .as_ref()
                == Some(&presenter)
        },
        &progress,
        || null_mut(),
    ));
    let pixels = [pixel(&incoming, 22, 48), pixel(&incoming, 74, 48)];
    assert_eq!(pixels, new);
    eprintln!("cross-process presenter pixels={pixels:?} while its root showed {old:?}");
    writeln!(source_input, "done").unwrap();
    assert!(source.wait().unwrap().success());
}

#[test]
#[ignore = "Native WGC readback of owned nonactivating fixture windows; explicit invocation"]
fn owned_desktop_layers_capture_new_pixels_and_play_above_renderer_below_icons() {
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.unwrap();
    let _apartment = Apartment;
    let graphics = CaptureGraphics::new().unwrap();
    let dxgi: IDXGIDevice = graphics.device.cast().unwrap();
    let composition: IDCompositionDevice = unsafe { DCompositionCreateDevice(&dxgi) }.unwrap();
    let progress = Mutex::default();
    let at = || Instant::now() + Duration::from_secs(2);
    let old = [[24, 60, 116, 255], [166, 80, 48, 255]];
    let new = [[144, 152, 44, 255], [80, 42, 210, 255]];
    let icon = [35, 220, 151, 255];
    let target = DesktopRect {
        left: 40,
        top: 40,
        right: 136,
        bottom: 136,
    };
    let client = DesktopRect {
        left: 0,
        top: 0,
        right: 96,
        bottom: 96,
    };
    if std::env::var_os(SOURCE_FIXTURE_ENV).is_some() {
        serve_owned_renderer(&composition, &graphics.context, target, client, old, new);
        return;
    }
    check_foreign_presenter(&graphics, target, old, new);

    for shared_root in [true, false] {
        let popup = WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW | WS_EX_NOREDIRECTIONBITMAP;
        let host_style = if shared_root {
            popup
        } else {
            popup & !WS_EX_NOREDIRECTIONBITMAP
        };
        let host = OwnedWindow::show(
            windows::core::w!("WallpaperCaptureFixture"),
            null_mut(),
            target,
            host_style,
        );
        let worker =
            shared_root.then(|| OwnedWindow::show(windows::core::w!("WorkerW"), host.0, client, 0));
        let renderer = match &worker {
            Some(worker) => OwnedWindow::show(
                windows::core::w!("WPEVideoWallpaper"),
                worker.0,
                client,
                WS_EX_LAYERED | WS_EX_NOREDIRECTIONBITMAP,
            ),
            None => OwnedWindow::show(
                windows::core::w!("WPEDesktopDX11Window"),
                null_mut(),
                target,
                popup,
            ),
        };
        let icon_rect = if shared_root {
            client
        } else {
            DesktopRect {
                left: 4,
                top: 4,
                right: 16,
                bottom: 16,
            }
        };
        let icon_window = OwnedWindow::show(
            windows::core::w!("SHELLDLL_DefView"),
            host.0,
            icon_rect,
            WS_EX_LAYERED,
        );
        let bind = |hwnd| unsafe {
            let target = composition
                .CreateTargetForHwnd(WinHwnd(hwnd), false)
                .unwrap();
            let visual = composition.CreateVisual().unwrap();
            target.SetRoot(&visual).unwrap();
            (target, visual)
        };
        let (_renderer_target, renderer_visual) = bind(renderer.0);
        let (_icon_target, icon_visual) = bind(icon_window.0);
        let mut icon_frame = stripes(icon_rect.width(), icon_rect.height(), [[0; 4]; 2]);
        for y in 4..16 {
            for x in 4..16 {
                let (x, y) = (x - icon_rect.left, y - icon_rect.top);
                let start = 4 * (y * icon_frame.width as i32 + x) as usize;
                icon_frame.rgba[start..start + 4].copy_from_slice(&icon);
            }
        }
        draw_layer(&composition, &graphics.context, &icon_visual, &icon_frame);
        draw_layer(
            &composition,
            &graphics.context,
            &renderer_visual,
            &stripes(96, 96, old),
        );
        let mut overlay =
            DesktopTransitionSession::show_in_owned_host(stripes(96, 96, old), target, host.0)
                .unwrap();
        let overlay_hwnd = overlay.window_identity() as HWND;
        if let Some(worker) = &worker {
            assert!(window_is_above(icon_window.0, overlay_hwnd));
            assert!(window_is_above(overlay_hwnd, worker.0));
        }

        draw_layer(
            &composition,
            &graphics.context,
            &renderer_visual,
            &stripes(96, 96, new),
        );
        let inspect = |process| inspect_engine_window(renderer.0, process);
        let pid = std::process::id();
        assert!(inspect(pid.wrapping_add(1)).is_none());
        let model = inspect(pid).unwrap();
        assert!(presenter_overlap(&model, target).is_ok());
        let presenter = CaptureWindow::from_presenter(&model).unwrap();
        let overlays = [overlay.window_identity()];
        assert_eq!(
            shares_capture_root(presenter.root, target, &overlays),
            shared_root
        );
        let held = || frame(graphics.read_window(host.0, at(), &|| true, &progress));
        let check = |image: &FrozenFrame, colors, expected_icon| {
            let observed = [pixel(image, 22, 48), pixel(image, 74, 48)];
            assert_eq!(observed, colors, "shared_root={shared_root}");
            if let Some(expected_icon) = expected_icon {
                assert_eq!(pixel(image, 8, 8), expected_icon);
            }
        };
        check(&held(), old, Some(icon));
        let incoming = frame(read_presenter(
            &mut None,
            &presenter,
            target,
            &overlays,
            at(),
            || {
                inspect(pid)
                    .and_then(|model| CaptureWindow::from_presenter(&model))
                    .as_ref()
                    == Some(&presenter)
            },
            &progress,
            || null_mut(),
        ));
        check(&incoming, new, Some(new[0]));
        check(&held(), old, Some(icon));
        assert_ne!(unsafe { IsWindowVisible(overlay_hwnd) }, 0);

        if let Some(worker) = &worker {
            // Exercise a renderer moving ahead of the overlay after capture.
            assert_ne!(
                unsafe {
                    SetWindowPos(
                        worker.0,
                        icon_window.0,
                        0,
                        0,
                        0,
                        0,
                        SWP_NOACTIVATE | SWP_NOMOVE | SWP_NOSIZE,
                    )
                },
                0
            );
            assert!(window_is_above(worker.0, overlay_hwnd));
        }
        overlay
            .render(Some(&incoming), TransitionEffect::Crossfade, 0.5)
            .unwrap();
        overlay.present().unwrap();
        if let Some(worker) = &worker {
            assert!(window_is_above(overlay_hwnd, worker.0));
        }
        assert!(unsafe { DwmFlush() } >= 0);
        let blended = std::array::from_fn(|i| {
            std::array::from_fn(|channel| {
                ((old[i][channel] as u16 + new[i][channel] as u16) / 2) as u8
            })
        });
        check(&held(), blended, Some(icon));
        eprintln!("shared_root={shared_root} gpu={} old={old:?} incoming={new:?} intermediate={blended:?} icon={icon:?}", overlay.uses_gpu());
        overlay
            .render(Some(&incoming), TransitionEffect::WipeLeft, 0.5)
            .unwrap();
        overlay.present().unwrap();
        assert!(unsafe { DwmFlush() } >= 0);
        let wiped = [new[0], old[1]];
        check(&held(), wiped, Some(icon));
        eprintln!("shared_root={shared_root} Wipe Left intermediate={wiped:?} icon={icon:?}");
        let mut settled = new;
        if worker.is_some() {
            let live = [[30, 180, 60, 255], [210, 120, 30, 255]];
            draw_layer(
                &composition,
                &graphics.context,
                &renderer_visual,
                &stripes(96, 96, live),
            );
            overlay
                .render(None, TransitionEffect::WipeLeft, 0.5)
                .unwrap();
            overlay.present().unwrap();
            assert!(unsafe { DwmFlush() } >= 0);
            check(&held(), [live[0], old[1]], Some(icon));
            overlay
                .render(None, TransitionEffect::Crossfade, 1.0)
                .unwrap();
            overlay.present().unwrap();
            assert!(unsafe { DwmFlush() } >= 0);
            check(&held(), live, Some(icon));
            eprintln!("shared_root={shared_root} reveal live={live:?} icon={icon:?}");
            settled = live;
        }
        drop(overlay);
        assert_eq!(unsafe { IsWindow(overlay_hwnd) }, 0);
        if shared_root {
            check(&held(), settled, Some(icon));
        }
    }
}

#[test]
#[ignore = "PrintWindow readback of an owned live desktop layout with a flip-model renderer; explicit invocation"]
fn owned_live_layout_reveals_a_flip_model_renderer_through_the_cover() {
    use windows::Win32::Graphics::Dxgi::Common::DXGI_SAMPLE_DESC;
    use windows::Win32::Graphics::Dxgi::{
        IDXGIFactory2, IDXGISwapChain1, DXGI_PRESENT, DXGI_SWAP_CHAIN_DESC1,
        DXGI_SWAP_EFFECT_FLIP_DISCARD, DXGI_USAGE_RENDER_TARGET_OUTPUT,
    };

    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.unwrap();
    let _apartment = Apartment;
    let graphics = CaptureGraphics::new().unwrap();
    let dxgi: IDXGIDevice = graphics.device.cast().unwrap();
    let composition: IDCompositionDevice = unsafe { DCompositionCreateDevice(&dxgi) }.unwrap();
    let _coordinates = PhysicalWindowCoordinates::enter().unwrap();
    let old = [[24, 60, 116, 255], [166, 80, 48, 255]];
    let decoded = [[144, 152, 44, 255], [80, 42, 210, 255]];
    let icon = [35, 220, 151, 255];
    let target = DesktopRect {
        left: 40,
        top: 40,
        right: 200,
        bottom: 130,
    };
    let client = DesktopRect {
        left: 0,
        top: 0,
        right: 160,
        bottom: 90,
    };
    let host = OwnedWindow::show(
        windows::core::w!("OwnedLiveLayoutHost"),
        null_mut(),
        target,
        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
    );
    let worker = OwnedWindow::show(windows::core::w!("WorkerW"), host.0, client, 0);
    let renderer = OwnedWindow::show(
        windows::core::w!("OwnedFlipModelRenderer"),
        worker.0,
        client,
        0,
    );
    let icons = OwnedWindow::show(
        windows::core::w!("SHELLDLL_DefView"),
        host.0,
        client,
        WS_EX_LAYERED,
    );
    let icon_target = unsafe { composition.CreateTargetForHwnd(WinHwnd(icons.0), false) }.unwrap();
    let icon_visual = unsafe { composition.CreateVisual() }.unwrap();
    unsafe { icon_target.SetRoot(&icon_visual) }.unwrap();
    let mut icon_frame = stripes(160, 90, [[0; 4]; 2]);
    for y in 4..16u32 {
        for x in 4..16u32 {
            let start = 4 * (y * 160 + x) as usize;
            icon_frame.rgba[start..start + 4].copy_from_slice(&icon);
        }
    }
    draw_layer(&composition, &graphics.context, &icon_visual, &icon_frame);

    let factory: IDXGIFactory2 =
        unsafe { dxgi.GetAdapter().and_then(|adapter| adapter.GetParent()) }.unwrap();
    let chain: IDXGISwapChain1 = unsafe {
        factory.CreateSwapChainForHwnd(
            &graphics.device,
            WinHwnd(renderer.0),
            &DXGI_SWAP_CHAIN_DESC1 {
                Width: 160,
                Height: 90,
                Format: DXGI_FORMAT_B8G8R8A8_UNORM,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                BufferUsage: DXGI_USAGE_RENDER_TARGET_OUTPUT,
                BufferCount: 2,
                SwapEffect: DXGI_SWAP_EFFECT_FLIP_DISCARD,
                ..Default::default()
            },
            None,
            None,
        )
    }
    .unwrap();
    let paint_renderer = |colour: [u8; 4]| unsafe {
        let buffer: ID3D11Texture2D = chain.GetBuffer(0).unwrap();
        let mut view = None;
        graphics
            .device
            .CreateRenderTargetView(&buffer, None, Some(&mut view))
            .unwrap();
        graphics.context.ClearRenderTargetView(
            &view.unwrap(),
            &colour.map(|channel| f32::from(channel) / 255.0),
        );
        chain.Present(0, DXGI_PRESENT(0)).ok().unwrap();
        assert!(DwmFlush() >= 0);
    };
    let live = [200, 40, 90, 255];
    paint_renderer(live);

    let mut overlay =
        DesktopTransitionSession::show_in_owned_host(stripes(160, 90, old), target, host.0)
            .unwrap();
    let cover = overlay.window_identity() as HWND;
    assert!(
        overlay.uses_gpu(),
        "the reveal needs the composition renderer"
    );
    let layered = |cover| window_is_above(icons.0, cover) && window_is_above(cover, worker.0);
    assert!(
        layered(cover),
        "icons above the cover, the cover above WorkerW"
    );

    let close = |a: [u8; 4], b: [u8; 4]| (0..3).all(|channel| a[channel].abs_diff(b[channel]) <= 2);
    let read = |label: &str, left: [u8; 4], right: [u8; 4]| {
        let frame = copy_window_pixels(worker.0, client).unwrap();
        let observed = [
            pixel(&frame, 30, 45),
            pixel(&frame, 130, 45),
            pixel(&frame, 8, 8),
        ];
        eprintln!(
            "{label}: PrintWindow(WorkerW) left={:?} right={:?} icon={:?}",
            observed[0], observed[1], observed[2]
        );
        assert!(
            close(observed[0], left) && close(observed[1], right),
            "{label}: expected left={left:?} right={right:?}, observed {observed:?}"
        );
        assert!(
            close(observed[2], icon),
            "{label}: the icon view must stay above the cover"
        );
    };
    read("opaque cover before dispatch", old[0], old[1]);

    let mut reveal = |effect, progress| {
        overlay.render(None, effect, progress).unwrap();
        overlay.present().unwrap();
        assert!(unsafe { DwmFlush() } >= 0);
    };
    reveal(TransitionEffect::WipeLeft, 0.5);
    read("reveal at 50%", live, old[1]);
    let moved = [30, 180, 60, 255];
    paint_renderer(moved);
    read("renderer moved under the reveal", moved, old[1]);
    reveal(TransitionEffect::WipeLeft, 1.0);
    read("reveal finished", moved, moved);
    assert!(layered(cover));

    overlay
        .render(
            Some(&stripes(160, 90, decoded)),
            TransitionEffect::WipeLeft,
            0.5,
        )
        .unwrap();
    overlay.present().unwrap();
    assert!(unsafe { DwmFlush() } >= 0);
    read("static composite at 50%", decoded[0], old[1]);
    assert!(layered(cover));
    drop(overlay);
    assert_eq!(unsafe { IsWindow(cover) }, 0);
    read("cover removed", moved, moved);

    let cpu_session = || {
        crate::desktop_transition::FORCE_CPU_SURFACE.with(|force| force.set(true));
        let session =
            DesktopTransitionSession::show_in_owned_host(stripes(160, 90, old), target, host.0);
        crate::desktop_transition::FORCE_CPU_SURFACE.with(|force| force.set(false));
        let session = session.unwrap();
        let cover = session.window_identity() as HWND;
        assert!(!session.uses_gpu());
        assert!(layered(cover));
        read("CPU cover before dispatch", old[0], old[1]);
        (session, cover)
    };
    let frame = |session: &mut DesktopTransitionSession,
                 incoming: Option<&FrozenFrame>,
                 effect,
                 progress| {
        session.render(incoming, effect, progress).unwrap();
        session.present().unwrap();
        assert!(unsafe { DwmFlush() } >= 0);
    };
    let (mut fallback, cover) = cpu_session();
    let decoded_frame = stripes(160, 90, decoded);
    frame(
        &mut fallback,
        Some(&decoded_frame),
        TransitionEffect::WipeLeft,
        0.5,
    );
    read("CPU static composite at 50%", decoded[0], old[1]);
    drop(fallback);
    assert_eq!(unsafe { IsWindow(cover) }, 0);

    let (mut fallback, cover) = cpu_session();
    frame(&mut fallback, None, TransitionEffect::Crossfade, 0.5);
    let faded = |cover: [u8; 4]| -> [u8; 4] {
        std::array::from_fn(|channel| {
            ((f32::from(cover[channel]) * 128.0 + f32::from(moved[channel]) * 127.0) / 255.0)
                .round() as u8
        })
    };
    read("CPU uniform fade at 50%", faded(old[0]), faded(old[1]));
    frame(&mut fallback, None, TransitionEffect::Crossfade, 1.0);
    read("CPU reveal finished", moved, moved);
    assert!(layered(cover));
    drop(fallback);
    assert_eq!(unsafe { IsWindow(cover) }, 0);
}

#[test]
#[ignore = "PrintWindow readback of an owned web-like presenter subtree; explicit invocation"]
fn isolated_shell_worker_reads_visible_presenter_descendants() {
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }
    unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.unwrap();
    let _apartment = Apartment;
    let graphics = CaptureGraphics::new().unwrap();
    let dxgi: IDXGIDevice = graphics.device.cast().unwrap();
    let composition: IDCompositionDevice = unsafe { DCompositionCreateDevice(&dxgi) }.unwrap();
    let _coordinates = PhysicalWindowCoordinates::enter().unwrap();
    let rect = |left, right| DesktopRect {
        left,
        top: 0,
        right,
        bottom: 96,
    };
    let host = OwnedWindow::show(
        windows::core::w!("OwnedWebCaptureShell"),
        null_mut(),
        rect(0, 160),
        WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW,
    );
    let _icons = OwnedWindow::show(
        windows::core::w!("SHELLDLL_DefView"),
        host.0,
        rect(0, 16),
        0,
    );
    let worker = OwnedWindow::show(windows::core::w!("WorkerW"), host.0, rect(0, 160), 0);
    // A web presenter hosts visible browser and GPU child windows, as observed
    // for WPEDesktopCEFWindow on 23 November.
    let presenter = OwnedWindow::show(
        windows::core::w!("WPEDesktopCEFWindow"),
        worker.0,
        rect(16, 144),
        0,
    );
    let browser = OwnedWindow::show(
        windows::core::w!("Chrome_WidgetWin_1"),
        presenter.0,
        rect(0, 128),
        0,
    );
    let gpu = OwnedWindow::show(
        windows::core::w!("Intermediate D3D Window"),
        browser.0,
        rect(0, 128),
        WS_EX_NOREDIRECTIONBITMAP,
    );
    let gpu_target = unsafe { composition.CreateTargetForHwnd(WinHwnd(gpu.0), true) }.unwrap();
    let visual = unsafe { composition.CreateVisual() }.unwrap();
    unsafe { gpu_target.SetRoot(&visual) }.unwrap();
    let content = [[200, 30, 40, 255], [20, 160, 220, 255]];
    draw_layer(
        &composition,
        &graphics.context,
        &visual,
        &stripes(128, 96, content),
    );
    let model = inspect_engine_window(presenter.0, std::process::id()).unwrap();
    let capture = CaptureWindow::from_presenter(&model).unwrap();
    let target = model.rect;
    let progress = Mutex::default();
    // Engine presenters leave the child-window DIB untouched on this system.
    let reader = |hwnd, rect: DesktopRect| {
        if hwnd == presenter.0 {
            Ok(FrozenFrame {
                width: rect.width(),
                height: rect.height(),
                rgba: vec![UNWRITTEN_PIXEL; (rect.width() * rect.height() * 4) as usize],
            })
        } else {
            copy_window_pixels(hwnd, rect)
        }
    };
    let captured = frame(read_isolated_presenter(
        &capture,
        target,
        &[],
        Instant::now() + Duration::from_secs(2),
        &|| true,
        &progress,
        || host.0,
        reader,
    ));
    assert!(progress.lock().unwrap().contains("wallpaper WorkerW"));
    assert_eq!(
        [pixel(&captured, 14, 48), pixel(&captured, 114, 48)],
        content
    );

    for class in [
        windows::core::w!("SHELLDLL_DefView"),
        windows::core::w!("WallpaperSwitcherTransition"),
    ] {
        let nested = OwnedWindow::show(class, browser.0, rect(0, 16), 0);
        let refusal = wallpaper_worker(&capture, target, &[], host.0).unwrap_err();
        assert!(
            refusal.contains("icons") || refusal.contains("cover"),
            "{refusal}"
        );
        drop(nested);
    }
    let sibling = OwnedWindow::show(
        windows::core::w!("WPEVideoWallpaper"),
        worker.0,
        rect(0, 16),
        0,
    );
    assert!(wallpaper_worker(&capture, target, &[], host.0)
        .unwrap_err()
        .contains("another visible window"));
    drop(sibling);
    assert!(wallpaper_worker(&capture, target, &[], host.0).is_ok());
}

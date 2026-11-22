use super::*;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const SIZE: [u32; 2] = [1280, 720];
const COUNT: usize = 96;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Route {
    Slices,
    Wall,
    Studio,
    Cycle { explicit: bool },
}

impl Route {
    fn steps(self) -> usize {
        match self {
            Self::Slices => 40,
            Self::Wall => 16,
            Self::Studio => 5,
            Self::Cycle { .. } => 6,
        }
    }

    fn input(self, app: &mut GpuCandidate, step: usize) {
        match self {
            Self::Slices => app.navigate_selection(if step <= 20 { 1 } else { -1 }),
            Self::Wall => {
                let delta = if step <= 8 { -3.0 } else { 3.0 };
                let message = super::super::super::lifecycle::bind_gallery_input(
                    app,
                    GalleryMessage::Wheel(delta, None),
                );
                let _ = update(app, message);
            }
            Self::Cycle { explicit } => {
                // Next, reverse, then finish a complete circuit from Slices.
                let target = ["2", "1", "2", "3", "4", "1"][step - 1];
                let backwards = step == 2;
                let key = if explicit { target } else { "m" };
                let window = app.window.unwrap();
                let _ = update(
                    app,
                    Message::KeyPressed {
                        key: keyboard::Key::Character(key.into()),
                        modifiers: if !explicit && backwards {
                            keyboard::Modifiers::SHIFT
                        } else {
                            keyboard::Modifiers::empty()
                        },
                        event: NavigationKeyEvent {
                            event_window: window,
                            physical_key: keyboard::key::Physical::Unidentified(
                                keyboard::key::NativeCode::Unidentified,
                            ),
                            location: keyboard::Location::Standard,
                            repeat: false,
                        },
                    },
                );
            }
            Self::Studio => {
                let intent = match step {
                    1 => {
                        let _ = update(app, Message::ToggleSettingsPanel);
                        SettingsMsg::SelectTab("picker".into())
                    }
                    2 | 4 => {
                        SettingsMsg::SelectSection(view::settings::composed::PICKER_LAYOUT_SECTION)
                    }
                    _ => SettingsMsg::LeaveLayoutStudio,
                };
                let message = Message::SettingsIntent {
                    window: app.window,
                    generation: app.settings_workbench.generation,
                    intent: Intent::Settings(intent),
                };
                let _ = update(app, message);
            }
        }
    }
}

fn owned_dir(name: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(name).expect(name));
    assert!(path.is_absolute(), "{name} must be absolute");
    path.canonicalize().expect(name)
}

fn make_media(out: &Path, app: &mut GpuCandidate) {
    let media = out.join("media");
    std::fs::create_dir(&media).unwrap();
    for (index, wallpaper) in app.wallpapers.iter_mut().enumerate() {
        let path = media.join(format!("Scroll-{index:05}.png"));
        let (width, height) = [(384, 216), (384, 256), (336, 252), (256, 336)][index % 4];
        let mut image = ::image::RgbImage::from_fn(width, height, |x, y| {
            let base = (index * 37) as u8;
            let band = ((x / 80 + y / 60) % 2) as u8 * 48;
            ::image::Rgb([base.wrapping_add(band), 96 + band, 224 - band])
        });
        for digit in 0..7 {
            if index & (1 << digit) != 0 {
                let x0 = width * (2 + digit * 2) / 18;
                for x in x0..x0 + width / 18 {
                    for y in height * 2 / 5..height * 3 / 5 {
                        image.put_pixel(x, y, ::image::Rgb([255, 255, 255]));
                    }
                }
            }
        }
        image.save(&path).unwrap();
        wallpaper.file_target = path.clone();
        wallpaper.preview_path = Some(path);
        wallpaper.folder_path = media.clone();
        app.catalog[index].width = width;
        app.catalog[index].height = height;
    }
}

fn bc_renderer() -> iced::Renderer {
    drop(renderer());
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let adapter =
        iced::futures::executor::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            force_fallback_adapter: false,
            compatible_surface: None,
        }))
        .unwrap();
    println!("ADAPTER {:?} requested_features=BC", adapter.get_info());
    let (device, queue) =
        iced::futures::executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            required_features: wgpu::Features::TEXTURE_COMPRESSION_BC,
            required_limits: wgpu::Limits {
                max_bind_groups: 2,
                ..Default::default()
            },
            ..Default::default()
        }))
        .unwrap();
    let engine = iced_wgpu::Engine::new(
        &adapter,
        device,
        queue,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        Some(iced_wgpu::graphics::Antialiasing::MSAAx4),
        iced_wgpu::graphics::Shell::headless(),
    );
    iced::Renderer::new(engine, UI_FONT, iced::Pixels(16.0))
}

fn owned_output() -> PathBuf {
    let out = owned_dir("POSTER_BROWSE_OUTPUT");
    let target = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .canonicalize()
        .unwrap();
    let profile = owned_dir("WALLPAPER_SWITCHER_CONFIG_DIR");
    let temp = owned_dir("TEMP");
    assert!(
        out != target && out.starts_with(&target),
        "use an owned target subdirectory"
    );
    assert!(profile == out.join("profile") && temp == out.join("tmp"));
    assert_eq!(owned_dir("TMP"), temp);
    assert!(
        profile.read_dir().unwrap().next().is_none(),
        "use a fresh fixture profile"
    );
    assert!(
        temp.read_dir().unwrap().next().is_none(),
        "use a fresh fixture temp directory"
    );
    out
}

#[test]
#[ignore = "owned offscreen poster browse trace; explicit invocation only"]
fn poster_review_trace() {
    let out = owned_output();

    let case = std::env::var("POSTER_BROWSE_ROUTE").unwrap();
    let route = match case.as_str() {
        "slices" => Route::Slices,
        "wall" => Route::Wall,
        "studio" => Route::Studio,
        "cycle" => Route::Cycle { explicit: false },
        "cycle-explicit" => Route::Cycle { explicit: true },
        _ => panic!("unknown poster browse route {case}"),
    };
    let passes = if std::env::var_os("POSTER_BROWSE_TEN_PASSES").is_some() {
        assert!(route == Route::Slices);
        10
    } else {
        1
    };
    let (tx, mut rx) = futures_channel::mpsc::unbounded();
    *WAKE_TX.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(tx);
    let (mut app, submissions) = d4a_fixture(COUNT);
    app.viewport = SIZE.map(|n| n as f32);
    app.browse_position_restored = true;
    app.launch_policy =
        runtime::parse_launch_policy(&["app.exe".into(), "--screenshot".into()]).unwrap();
    app.poster_cache_dir = Config::config_dir().join("cache").join("posters");
    make_media(&out, &mut app);
    if std::env::var_os("POSTER_BROWSE_COLD").is_none() {
        let cache = poster_worker::cache::WriteBatch::new(&app.poster_cache_dir);
        for wallpaper in &app.wallpapers {
            gpu_gallery::decode_poster_cached(&wallpaper.file_target, 0, &cache, None, &|| true)
                .unwrap();
        }
        assert_eq!(
            app.poster_cache_dir
                .read_dir()
                .unwrap()
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rgba"))
                .count(),
            COUNT
        );
    }
    app.sort_order = SortOrder::Newest;
    app.selected = Some(40);
    app.gallery_mode = if route == Route::Wall {
        GalleryMode::Wall
    } else {
        GalleryMode::Slices
    };
    app.state.gallery.mode = gallery::gallery_kind(app.gallery_mode);
    app.state.gallery.wall.columns = 7;
    app.state.gallery.wall.rows = 5;
    if matches!(route, Route::Cycle { .. }) {
        // Wall resets to its first viewport on mode changes. Use an item that
        // can be sampled in all four modes without moving the camera.
        app.selected = Some(17);
        app.state.frontend_preferences.launch.animation = Some("rise".into());
        app.state.frontend_preferences.launch.fade_from = Some(20.0);
    }
    app.depth_frame = None;
    app.sync_gallery_presentation();
    if let Some(presentation) = &mut app.gallery_presentation {
        presentation.settle();
    }
    if let Some(wall) = &mut app.wall_presentation {
        wall.restore(40, None);
        wall.settle();
    }
    app.scene_motion.settle();
    let mut renderer = if std::env::var_os("POSTER_BROWSE_RGBA").is_some() {
        renderer()
    } else {
        bc_renderer()
    };
    app.deterministic_animation = false;
    crate::apply_trace::set_enabled(true);
    let clock = Instant::now();
    let now = || clock.elapsed().as_secs_f64() * 1000.0;
    crate::apply_trace::event_lazy("poster-browse-clock", || format!("t={}", now()));
    app.schedule_posters();
    let mut next_step = if std::env::var_os("POSTER_BROWSE_EARLY").is_some() {
        75.0
    } else {
        1500.0
    };
    let mut frame = 0;
    let mut studio_visits = 0;
    for pass in 1..=passes {
        let mut step = 0;
        loop {
            for _ in 0..256 {
                let Ok(wake) = rx.try_recv() else { break };
                let _merge = matches!(wake, Wake::PostersReady)
                    .then(|| crate::apply_trace::cpu_span("poster-browse-merge", || String::new()))
                    .flatten();
                let _ = update(&mut app, Message::Wake(wake));
            }
            if now() >= next_step && step < route.steps() {
                step += 1;
                let start = now();
                route.input(&mut app, step);
                println!(
                    "INPUT {}",
                    serde_json::json!({"step":step,"t":start,"id":app.selected,"studio":app.settings_workbench.layout_studio(),"pass":pass})
                );
                next_step = start
                    + match route {
                        Route::Studio => 850.0,
                        Route::Cycle { .. } => 1250.0,
                        _ => 65.0,
                    };
            }
            let start = now();
            let vis = app.scene_motion.opacity();
            if route == Route::Studio && vis > 0.95 && app.settings_workbench.layout_studio() {
                studio_visits |= 1 << step;
            }
            let [width, height] = app.viewport;
            let mut clip = [0.0, 0.0, width, height];
            if let Some(wall) = app
                .wall_presentation
                .as_ref()
                .filter(|_| app.gallery_mode == GalleryMode::Wall)
            {
                clip = wall.clip(
                    app.viewport,
                    gallery::gallery_layout_params_for_app(&app).wall,
                );
            }
            if app.settings_workbench.layout_studio() {
                clip[0] = crate::donor_folio::source_ui::picker_layout_studio_width(
                    width,
                    view::settings::bridge::effective_ui_scale(&app),
                ) + 36.0;
            }
            let cards: Vec<_> = app
                .ordinary_frame()
                .into_iter()
                .filter_map(|entry| {
                    let card = entry.card;
                    let [x, y] = card.center;
                    (card.opacity >= 0.5
                        && x >= clip[0].max(0.0)
                        && y >= clip[1].max(0.0)
                        && x < clip[2].min(width)
                        && y < clip[3].min(height))
                    .then_some((card.index, card.layer, [x, y]))
                })
                .collect();
            let bytes = Scene::new(&app, &renderer).readback(&mut renderer, &app);
            let end = now();
            if matches!(route, Route::Cycle { .. }) {
                let mode = app.gallery_mode;
                let selected_card = app.selected.and_then(|selected| {
                    if mode == GalleryMode::Depth {
                        return app
                            .depth_frame
                            .as_ref()?
                            .cards
                            .iter()
                            .find(|entry| entry.card.index == selected)
                            .map(|entry| entry.card);
                    }
                    let entry = app
                        .ordinary_frame()
                        .into_iter()
                        .find(|entry| entry.card.index == selected)?;
                    let mut card = entry.card;
                    if entry.hex_style || entry.source_style {
                        donor_scene::launch_card(
                            &mut card,
                            entry.entrance,
                            entry.launch,
                            app.viewport,
                        );
                    }
                    Some(card)
                });
                let sample = selected_card.map(|card| {
                    let mut read = Vec::new();
                    for dx in [-0.09, 0.0, 0.09] {
                        for dy in [-0.16, 0.0, 0.16] {
                            let x = (card.center[0] + dx * card.size[0]).round();
                            let y = (card.center[1] + dy * card.size[1]).round();
                            if (0.0..SIZE[0] as f32).contains(&x)
                                && (0.0..SIZE[1] as f32).contains(&y)
                            {
                                let offset = (y as usize * SIZE[0] as usize + x as usize) * 4;
                                read.push([bytes[offset], bytes[offset + 1], bytes[offset + 2]]);
                            }
                        }
                    }
                    read
                });
                let recognizable =
                    selected_card
                        .zip(sample.as_ref())
                        .is_some_and(|(card, read)| {
                            let base = (card.index * 37) as u8;
                            let colors = [[base, 96, 224], [base.wrapping_add(48), 144, 176]];
                            read.iter()
                                .filter(|value| {
                                    colors.iter().any(|expected| {
                                        value
                                            .iter()
                                            .zip(expected)
                                            .all(|(a, b)| a.abs_diff(*b) <= 32)
                                    })
                                })
                                .count()
                                >= 2
                        });
                println!(
                    "CYCLE {}",
                    serde_json::json!({
                        "step":step, "start":start, "t":end, "mode":mode.label(),
                        "gate":app.atlas_entry.is_some(), "entrance":app.gallery_entrance_opacity(),
                        "index":app.selected, "center":selected_card.map(|card| card.center),
                        "eligible":app.atlas_entry.is_none() && vis > 0.0 && selected_card.is_some(),
                        "recognizable":recognizable, "sample":sample,
                    })
                );
            }
            println!(
                "OBS {}",
                serde_json::json!({"f":frame,"start":start,"t":end,"step":step,"id":app.selected,"vis":vis,"cards":cards,"npost":app.posters.len(),"generation":app.poster_generation.load(Ordering::Acquire),"pass":pass})
            );
            let screenshot = match (route, step) {
                (Route::Slices, 20) | (Route::Wall, 8) => Some("turn.png"),
                (Route::Studio, 2) if vis > 0.95 => Some("studio.png"),
                _ => None,
            };
            if let Some(screenshot) = screenshot
                .map(|name| out.join(name))
                .filter(|path| !path.exists())
            {
                ::image::save_buffer_with_format(
                    screenshot,
                    &bytes,
                    SIZE[0],
                    SIZE[1],
                    ::image::ColorType::Rgba8,
                    ::image::ImageFormat::Png,
                )
                .unwrap();
            }
            frame += 1;
            if step == route.steps() && now() > next_step + 4500.0 {
                let pool_bytes = (passes > 1).then(|| {
                    assert!(!app.poster_loading, "pass {pass} still has pending decode");
                    app.poster_worker
                        .as_ref()
                        .unwrap()
                        .pool_checked_out_bytes()
                        .unwrap()
                });
                println!(
                    "PASS {}",
                    serde_json::json!({"pass":pass,"t":now(),"posters":app.posters.len(),"pool_bytes":pool_bytes})
                );
                next_step = now() + 65.0;
                break;
            }
            assert!(now() < 25_000.0 * passes as f64, "bounded replay timed out");
            let wait = 16.0 - (now() - start);
            if wait > 0.0 {
                std::thread::sleep(Duration::from_secs_f64(wait / 1000.0));
            }
        }
    }
    if route == Route::Studio {
        assert_eq!(
            studio_visits,
            (1 << 2) | (1 << 4),
            "both Studio visits must reveal the scene"
        );
    }
    assert_eq!(submissions.load(Ordering::Acquire), 0);
    println!("END route={case} frames={frame} provider_submissions=0");
}

#[test]
#[ignore = "owned five-file BC Depth trace; explicit invocation only"]
fn five_file_depth_draw_trace() {
    let out = owned_output();
    let media = out.join("media");
    std::fs::create_dir(&media).unwrap();
    let files: [(&str, &[u8]); 5] = [
        (
            "01-landscape.png",
            include_bytes!("../../../tests/fixtures/triangle-orientation/I1-A-LANDSCAPE.png"),
        ),
        (
            "02-portrait.png",
            include_bytes!("../../../tests/fixtures/triangle-orientation/I1-B-PORTRAIT.png"),
        ),
        (
            "03-square.png",
            include_bytes!("../../../tests/fixtures/triangle-orientation/I1-C-SQUARE.png"),
        ),
        (
            "red-blue.mp4",
            include_bytes!("../../../tests/fixtures/video/red-blue.mp4"),
        ),
        ("10-corrupt.png", b"not a real PNG"),
    ];
    let (tx, mut rx) = futures_channel::mpsc::unbounded();
    *WAKE_TX.get_or_init(|| Mutex::new(None)).lock().unwrap() = Some(tx);
    let (mut app, submissions) = d4a_fixture(files.len());
    app.viewport = SIZE.map(|n| n as f32);
    app.launch_policy =
        runtime::parse_launch_policy(&["app.exe".into(), "--screenshot".into()]).unwrap();
    app.browse_position_restored = true;
    app.poster_cache_dir = Config::config_dir().join("cache").join("posters");
    for (index, (name, bytes)) in files.into_iter().enumerate() {
        let path = media.join(name);
        std::fs::write(&path, bytes).unwrap();
        let wallpaper = &mut app.wallpapers[index];
        wallpaper.folder_path = media.clone();
        wallpaper.file_target = path.clone();
        if index == 3 {
            wallpaper.source = WallpaperSource::LocalVideo;
            wallpaper.wallpaper_type = "video".into();
            wallpaper.preview_path = None;
        } else {
            wallpaper.preview_path = Some(path.clone());
        }
        let (width, height) = ::image::image_dimensions(&path).unwrap_or((128, 72));
        app.catalog[index].width = width;
        app.catalog[index].height = height;
    }
    {
        let cache = poster_worker::cache::WriteBatch::new(&app.poster_cache_dir);
        for wallpaper in &app.wallpapers[..3] {
            gpu_gallery::decode_poster_cached(&wallpaper.file_target, 0, &cache, None, &|| true)
                .unwrap();
        }
        let video = &app.wallpapers[3];
        let source = poster_worker::PosterSource {
            index: 3,
            layer: 3,
            item_id: video.id.clone(),
            kind: poster_worker::PosterKind::Video,
            still: None,
            video_stamp: video.video_stamp(),
            path: video.file_target.clone(),
        };
        gpu_gallery::decode_video_poster(
            &source,
            &cache,
            &gpu_gallery::PosterDecoder::new(8),
            &|| true,
        )
        .unwrap();
        assert_eq!(
            app.poster_cache_dir
                .read_dir()
                .unwrap()
                .flatten()
                .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "rgba"))
                .count(),
            4
        );
    }
    app.selected = Some(2);
    app.reset_depth_transients();
    app.rebuild_depth_frame();
    app.scene_motion.settle();
    let mut renderer = bc_renderer();
    crate::apply_trace::set_enabled(true);
    app.schedule_posters();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(3) {
        for _ in 0..256 {
            let Ok(wake) = rx.try_recv() else { break };
            let _ = update(&mut app, Message::Wake(wake));
        }
        let _ = Scene::new(&app, &renderer).readback(&mut renderer, &app);
        std::thread::sleep(Duration::from_millis(12));
    }
    assert!(!app.poster_loading);
    assert_eq!(
        app.poster_failed_indices
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [4]
    );
    assert_eq!(submissions.load(Ordering::Acquire), 0);
    Scene::new(&app, &renderer).save(&mut renderer, &app, &out.join("depth.png"));
    println!("END route=depth provider_submissions=0");
}

#[test]
#[ignore = "owned WGPU artwork readback, run explicitly"]
fn parallax_intermediate_artwork_pixels() {
    let out = owned_output();
    let (mut app, submissions) = d4a_fixture(9);
    app.viewport = SIZE.map(|n| n as f32);
    app.selected = Some(4);
    app.depth_frame = None;
    app.browse_position_restored = true;
    app.launch_policy =
        runtime::parse_launch_policy(&["app.exe".into(), "--screenshot".into()]).unwrap();
    app.scene_motion.settle();
    app.state.gallery.geometric.rows = 1;
    app.state.gallery.geometric.columns = 5;
    app.state.gallery.geometric.radius = 115.0;
    app.state.gallery.geometric.curve = StateGeometricCurve::Flat;
    app.state.gallery.geometric.shape = StateGeometricShape::Hexagon;

    let slices = &mut app.state.gallery.slices;
    slices.width = 145.0;
    slices.expanded_width = 230.0;
    slices.spacing = 25.0;
    slices.height = 420.0;
    slices.visible_count = 7;
    slices.skew_distance = 0.0;
    slices.wobble = false;

    let (width, height) = (480, 230);
    let gradient = ::image::RgbImage::from_fn(width, height, |x, y| {
        ::image::Rgb([
            (24 + 192 * x / width) as u8,
            (28 + 184 * y / height) as u8,
            83,
        ])
    });
    let generation = app.poster_generation.load(Ordering::Acquire);
    let catalog_generation = app.catalog_generation.load(Ordering::Acquire);
    for (index, item) in app.wallpapers.iter_mut().enumerate() {
        let source = out.join(format!("parallax-gradient-{index}.png"));
        gradient.save(&source).unwrap();
        item.file_target = source.clone();
        item.preview_path = Some(source.clone());
        app.catalog[index].width = width;
        app.catalog[index].height = height;
        app.poster_slots.insert(
            index,
            PosterSlot {
                layer: index as u32,
                kind: poster_worker::PosterKind::Still,
                generation,
                last_requested: generation,
                catalog_generation,
                item_id: item.id.clone(),
                source: source.clone(),
            },
        );
        Arc::make_mut(&mut app.posters)
            .push(gpu_gallery::decode_poster(&source, index as u32).unwrap());
    }

    let mut renderer = renderer();
    for mode in [GalleryMode::Slices, GalleryMode::Geometric] {
        app.gallery_mode = mode;
        app.state.gallery.mode = gallery::gallery_kind(mode);
        let set = |app: &mut GpuCandidate, enabled| {
            app.state.gallery.slices.parallax = Some(enabled);
            app.state.gallery.geometric.parallax = Some(enabled);
            app.sync_gallery_presentation();
        };
        let settle = |app: &mut GpuCandidate| match mode {
            GalleryMode::Slices => app.gallery_presentation.as_mut().unwrap().settle(),
            _ => app.hex_presentation.as_mut().unwrap().settle(),
        };
        let advance = |app: &mut GpuCandidate| match mode {
            GalleryMode::Slices => app.gallery_presentation.as_mut().unwrap().tick(0.05),
            _ => app.hex_presentation.as_mut().unwrap().tick(0.05),
        };
        let read = |app: &GpuCandidate, renderer: &mut iced::Renderer| {
            Scene::new(app, renderer).readback(renderer, app)
        };

        set(&mut app, false);
        settle(&mut app);
        let _ = read(&app, &mut renderer);
        let off = read(&app, &mut renderer);
        let frame = app.ordinary_frame();
        let cards: Vec<_> = frame.iter().map(|entry| entry.card).collect();
        set(&mut app, true);
        assert!(advance(&mut app));
        let enabling = read(&app, &mut renderer);
        settle(&mut app);
        let on = read(&app, &mut renderer);
        set(&mut app, false);
        assert!(advance(&mut app));
        let disabling = read(&app, &mut renderer);
        settle(&mut app);
        let off_again = read(&app, &mut renderer);

        assert!(frame.iter().all(|entry| entry.card.layer.is_some()));
        for (stage, rgba) in [
            ("off", &off),
            ("enabling", &enabling),
            ("on", &on),
            ("disabling", &disabling),
        ] {
            ::image::save_buffer_with_format(
                out.join(format!("{}-{stage}.png", mode.label())),
                rgba,
                SIZE[0],
                SIZE[1],
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
        }
        let mut samples = 0;
        let mut contrasting = 0;
        for card in cards
            .iter()
            .filter(|card| !card.selected && card.opacity > 0.8)
        {
            for dx in [-0.16, 0.0, 0.16] {
                for dy in [-0.18, 0.0, 0.18] {
                    let x = card.center[0] + dx * card.size[0];
                    let y = card.center[1] + dy * card.size[1];
                    if !(1.0..SIZE[0] as f32 - 1.0).contains(&x)
                        || !(1.0..SIZE[1] as f32 - 1.0).contains(&y)
                        || gpu_gallery::hit_test(&cards, x, y) != Some(card.index)
                    {
                        continue;
                    }
                    let offset = (y as usize * SIZE[0] as usize + x as usize) * 4;
                    for channel in [0, 1] {
                        let i = offset + channel;
                        if off[i].abs_diff(on[i]) < 8 {
                            continue;
                        }
                        contrasting += 1;
                        let low = off[i].min(on[i]);
                        let high = off[i].max(on[i]);
                        if (low + 1..high).contains(&enabling[i])
                            && (low + 1..high).contains(&disabling[i])
                        {
                            assert!(
                                off_again[i].abs_diff(off[i]) <= 1,
                                "{mode:?} restored artwork endpoint"
                            );
                            samples += 1;
                        }
                    }
                }
            }
        }
        assert!(
            samples >= 4,
            "{mode:?} needs intermediate artwork in both directions, got {samples} of {contrasting} contrasting channel samples"
        );
        println!("PARALLAX mode={mode:?} intermediate_channel_samples={samples}");
    }
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
#[ignore = "Owned mounted WGPU replay of same-gallery profile switches"]
fn same_gallery_presets_keep_patterned_artwork_through_reversal() {
    use candidate_state::preferences::BindingAction;
    use view::settings::folio::intent::Message as SettingsIntent;

    let out = owned_output();
    let mut renderer = renderer();
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
        GalleryMode::Depth,
    ] {
        let (mut app, submissions) = d4a_fixture(3);
        app.viewport = SIZE.map(|n| n as f32);
        app.selected = Some(1);
        app.browse_position_restored = true;
        app.gallery_mode = mode;
        let kind = gallery::gallery_kind(mode);
        app.state.gallery.mode = kind;
        app.state.gallery.slices.wobble = false;
        app.state.gallery.geometric.rows = 1;
        app.state.gallery.geometric.columns = 3;
        app.state.gallery.wall.columns = 3;
        app.state.gallery.wall.rows = 1;
        app.state.gallery.depth.visible_count = 3.0;
        app.state.frontend_preferences.launch.animation = Some("none".into());
        app.state
            .frontend_preferences
            .bindings
            .insert(BindingAction::PresetNext, "k".into());
        app.state
            .frontend_preferences
            .bindings
            .insert(BindingAction::PresetPrev, "j".into());
        app.depth_frame = None;
        for index in 0..3 {
            let path = out.join(format!("preset-{}-{index}.png", mode.label()));
            let image = ::image::RgbImage::from_fn(240, 180, |x, _| {
                let mut rgb = [24 + 56 * u8::from(x >= 120); 3];
                rgb[index] = 222;
                ::image::Rgb(rgb)
            });
            image.save(&path).unwrap();
            let wallpaper = &mut app.wallpapers[index];
            wallpaper.file_target = path.clone();
            wallpaper.preview_path = Some(path.clone());
            app.catalog[index].width = 240;
            app.catalog[index].height = 180;
            app.poster_slots.insert(
                index,
                PosterSlot {
                    layer: index as u32,
                    kind: poster_worker::PosterKind::Still,
                    generation: app.poster_generation.load(Ordering::Acquire),
                    last_requested: 0,
                    catalog_generation: app.catalog_generation.load(Ordering::Acquire),
                    item_id: wallpaper.id.clone(),
                    source: path.clone(),
                },
            );
            Arc::make_mut(&mut app.posters)
                .push(gpu_gallery::decode_poster(&path, index as u32).unwrap());
        }
        let prepare = |app: &mut GpuCandidate, width| {
            app.state.gallery.slices.expanded_width = width;
            app.state.gallery.geometric.radius = width * 0.5;
            app.state.gallery.wall.card_width = width;
            app.state.gallery.depth.width = width;
            app.sync_gallery_presentation();
            match mode {
                GalleryMode::Slices => app.gallery_presentation.as_mut().unwrap().settle(),
                GalleryMode::Geometric => app.hex_presentation.as_mut().unwrap().settle(),
                GalleryMode::Wall => app.wall_presentation.as_mut().unwrap().settle(),
                GalleryMode::Depth => {
                    app.depth_presentation = Some(DepthPresentation::target(&app.state));
                    app.reset_depth_transients();
                    app.rebuild_depth_frame();
                }
            }
            app.scene_motion.settle();
        };
        prepare(&mut app, 160.0);
        app.save_gallery_preset().unwrap();
        let small = app.state.mode_profiles.selected_id(kind).unwrap();
        prepare(&mut app, 260.0);
        app.save_gallery_preset().unwrap();
        let large = app.state.mode_profiles.selected_id(kind).unwrap();

        let show = |app: &GpuCandidate, renderer: &mut iced::Renderer, stage| {
            let card = if mode == GalleryMode::Depth {
                app.depth_frame
                    .as_ref()
                    .unwrap()
                    .cards
                    .iter()
                    .find(|entry| entry.card.selected)
                    .unwrap()
                    .card
            } else {
                app.ordinary_frame()
                    .into_iter()
                    .find(|entry| entry.card.selected)
                    .unwrap()
                    .card
            };
            assert_eq!(card.index, 1);
            let pixels = Scene::new(app, renderer).readback(renderer, app);
            let index = (card.center[1] as usize * SIZE[0] as usize + card.center[0] as usize) * 4;
            let color = &pixels[index..index + 3];
            assert!(
                color[1] > color[0].saturating_add(40) && color[1] > color[2].saturating_add(40),
                "{mode:?} {stage}: selected patterned poster is green at {:?}, got {color:?}",
                card.center
            );
            eprintln!(
                "{mode:?} {stage}: selected artwork matches, size={:?}",
                card.size
            );
            card.size[0]
        };
        let before = show(&app, &mut renderer, "large");
        let _ = update(&mut app, Message::ToggleSettingsPanel);
        app.settings_workbench.tab = "picker".into();
        p3_profile_intent(&mut app, SettingsIntent::ApplyPreset(kind, small));
        let _ = update(&mut app, Message::ToggleSettingsPanel);
        app.scene_motion.settle();
        assert_eq!(app.state.mode_profiles.selected_id(kind), Some(small));
        let tick = |app: &mut GpuCandidate| match mode {
            GalleryMode::Slices => app.gallery_presentation.as_mut().unwrap().tick(0.016),
            GalleryMode::Geometric => app.hex_presentation.as_mut().unwrap().tick(0.016),
            GalleryMode::Wall => app.wall_presentation.as_mut().unwrap().tick(0.016),
            GalleryMode::Depth => d4a_motion_tick(app, 0.016),
        };
        assert!(tick(&mut app));
        let during = show(&app, &mut renderer, "shrinking");
        let key = |app: &mut GpuCandidate, value: &str| {
            let event_window = app.window.unwrap();
            let _ = update(
                app,
                Message::KeyPressed {
                    key: keyboard::Key::Character(value.into()),
                    modifiers: keyboard::Modifiers::empty(),
                    event: NavigationKeyEvent {
                        event_window,
                        physical_key: keyboard::key::Physical::Unidentified(
                            keyboard::key::NativeCode::Unidentified,
                        ),
                        location: keyboard::Location::Standard,
                        repeat: false,
                    },
                },
            );
        };
        key(&mut app, "k");
        assert_eq!(app.state.mode_profiles.selected_id(kind), Some(large));
        tick(&mut app);
        show(&app, &mut renderer, "reversing");
        key(&mut app, "j");
        assert_eq!(app.state.mode_profiles.selected_id(kind), Some(small));
        for _ in 0..160 {
            if !tick(&mut app) {
                break;
            }
        }
        let after = show(&app, &mut renderer, "small");
        assert!(
            after < during && during < before,
            "{mode:?}: {after} < {during} < {before}"
        );
        assert_eq!(submissions.load(Ordering::Acquire), 0);
    }
}

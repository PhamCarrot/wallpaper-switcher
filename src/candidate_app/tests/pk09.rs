use super::pk07::{renderer, Scene};
use super::*;
use crate::candidate_app::gallery_presentation::{launch_entrance, SceneMotion};
use donor_scene::LaunchAnimation;
use view::settings::{composed::keys::selector, folio::intent};

fn fixture(mode: GalleryMode) -> (GpuCandidate, Arc<std::sync::atomic::AtomicUsize>) {
    let (mut app, submissions) = d4a_fixture(3);
    app.gallery_mode = mode;
    app.state.gallery.mode = gallery_kind(mode);
    app.state.gallery.variant = None;
    app.browse_position_restored = true;
    app.gallery_engaged = true;
    app.state.gallery.wall.card_width = 90.0;
    app.state.gallery.wall.card_height = 60.0;
    app.panel_state.settings_category = SettingsCategory::Picker;
    app.config.ui.show_top_bar = true;
    app.rail_reveal = 1.0;
    app.state.frontend_preferences.launch.animation = Some("none".into());
    app.sync_gallery_presentation();
    (app, submissions)
}

fn settings(app: &mut GpuCandidate, message: intent::SettingsMsg) {
    let window = app.window;
    let generation = app.settings_workbench.generation;
    let _ = update(
        app,
        Message::SettingsIntent {
            window,
            generation,
            intent: intent::Message::Settings(message),
        },
    );
}

fn advance(app: &mut GpuCandidate, dt: f32) -> bool {
    let mut scene = app.scene_motion;
    let mut moving = scene.tick(dt);
    let mut hex = app.hex_presentation.clone();
    if let Some(p) = &mut hex {
        p.inset = scene.inset(p.viewport[0]);
        moving |= p.tick(dt);
    }
    let state = app
        .depth_camera
        .unwrap_or_else(|| GalleryMotionState::absolute_camera(0.0, 0.0, 250.0, 1.0, 1.0));
    let _ = update(
        app,
        Message::Wake(Wake::MotionFrame {
            generation: app.motion_generation.load(Ordering::Acquire),
            value: 0.0,
            velocity: 0.0,
            state,
            selection: app.depth_selection.clone(),
            presentation: app.depth_presentation,
            filter_wave: None,
            ordinary: app.gallery_presentation.clone(),
            hex,
            wall: app.wall_presentation.clone(),
            scene: Some(scene),
        }),
    );
    moving
}

fn settle(app: &mut GpuCandidate) {
    for _ in 0..1000 {
        if !advance(app, 0.016) {
            return;
        }
    }
    panic!("scene never settled");
}

#[test]
fn timing_preserves_independent_targets_and_snap() {
    for (fade, inset) in [(180.0, 250.0), (250.0, 250.0), (450.0, 250.0)] {
        let mut scene = SceneMotion::default();
        assert!(scene.retarget(false, 440.0, fade, inset));
        assert!(scene.tick(0.016));
        let opacity = scene.opacity();
        let geometry = scene.composition(640.0, false);
        assert!((0.0..1.0).contains(&opacity));
        assert!(geometry[0] > 320.0);
        assert!(scene.retarget(true, 0.0, fade, inset));
        assert_eq!(scene.opacity(), opacity);
        assert_eq!(scene.composition(640.0, false), geometry);
        assert!(scene.tick(0.016));
        assert!(scene.opacity() > opacity);
        assert!(scene.retarget(false, 440.0, 0.0, 0.0));
        assert_eq!(scene.opacity(), 0.0);
        assert!(!scene.tick(0.016));
        assert_eq!(scene.composition(640.0, false), [512.0, 256.0]);
        assert_eq!(scene.composition(640.0, true), [128.0, 256.0]);
    }
    assert!(launch_entrance(LaunchAnimation::None, false, 0.2, 450.0).settled());
    assert!(launch_entrance(LaunchAnimation::Fade, false, 0.2, 0.0).settled());
    let mut entrance = launch_entrance(LaunchAnimation::Fade, false, 0.2, 450.0);
    assert_eq!(entrance.value(), 0.2);
    assert!(entrance.tick(0.016));
    assert!((0.2..1.0).contains(&entrance.value()));
}

#[test]
fn modal_handoff_keeps_current_motion_and_owns_input() {
    let (mut app, submissions) = fixture(GalleryMode::Slices);
    let selected = app.selected;
    let _ = update(&mut app, Message::TogglePlaylistChooser);
    assert!(app.playlist_chooser);
    assert!(advance(&mut app, 0.032));
    let opacity = app.scene_motion.opacity();
    assert!((0.0..1.0).contains(&opacity));
    let owner = app.playlist_owner().unwrap();
    let generation = app.motion_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::PlaylistBrowse(owner, playlists::Browse::Edit),
    );
    assert!(app.playlist_panel_open && !app.playlist_chooser);
    assert_eq!(app.scene_motion.opacity(), opacity);
    assert_eq!(app.motion_generation.load(Ordering::Acquire), generation);
    let _ = update(
        &mut app,
        Message::DonorRail(donor_rail::RailIntent::ToggleRandomRotation),
    );
    let _ = update(
        &mut app,
        Message::Gallery(GalleryMessage::Wheel(-1.0, None)),
    );
    assert_eq!(app.selected, selected);
    assert!(!app.state.automation_intent().random_enabled);
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.playlist_panel_open);
    assert_eq!(app.scene_motion.opacity(), opacity);
    assert!(advance(&mut app, 0.016));
    assert!(app.scene_motion.opacity() > opacity);
    settle(&mut app);
    assert_eq!(app.scene_motion.opacity(), 1.0);

    let _ = update(&mut app, Message::ToggleSettingsPanel);
    assert!(app.settings_workbench.folio_entrance.is_some());
    settle(&mut app);
    assert_eq!(app.scene_motion.opacity(), 0.0);
    d3_key(&mut app, keyboard::key::Named::Escape);
    settle(&mut app);
    assert_eq!(app.scene_motion.opacity(), 1.0);
    assert_eq!(app.selected, selected);
    assert_eq!(submissions.load(Ordering::SeqCst), 0);
}

fn center(app: &GpuCandidate) -> [f32; 2] {
    if app.gallery_mode == GalleryMode::Depth {
        let frame = app.depth_frame.as_ref().unwrap();
        let card = frame.cards.iter().find(|e| e.card.selected).unwrap();
        let [x, y] = card.card.center;
        assert_eq!(frame.hit(x, y).unwrap().id, card.id);
        card.card.center
    } else {
        let entries = app.ordinary_frame();
        let card = entries.iter().find(|e| e.card.selected).unwrap_or_else(|| {
            panic!(
                "{:?} selected {:?} missing from {:?}",
                app.gallery_mode,
                app.selected,
                entries.iter().map(|e| &e.id).collect::<Vec<_>>()
            )
        });
        let hit = donor_scene::presented_hit_card(card).unwrap();
        assert_eq!(hit.center, card.card.center);
        assert_eq!(
            gpu_gallery::hit_test(&[hit], hit.center[0], hit.center[1]),
            Some(hit.index)
        );
        hit.center
    }
}

#[test]
fn studio_reveals_each_direction_between_layout_families_and_changed_presets() {
    let modes = [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
        GalleryMode::Depth,
    ];
    for from in modes {
        let (mut app, _) = fixture(from);
        let _ = update(&mut app, Message::ToggleSettingsPanel);
        settings(&mut app, intent::SettingsMsg::SelectTab("picker".into()));
        settings(
            &mut app,
            intent::SettingsMsg::SelectSection(view::settings::composed::PICKER_LAYOUT_SECTION),
        );
        settle(&mut app);
        for to in modes.into_iter().filter(|to| *to != from) {
            let _ = update(&mut app, Message::SetGalleryMode(to));
            assert_eq!(app.scene_motion.opacity(), 0.0, "{from:?} to {to:?}");
            assert!(advance(&mut app, 0.05));
            assert!((0.0..1.0).contains(&app.scene_motion.opacity()));
            settle(&mut app);
            assert_eq!(app.scene_motion.opacity(), 1.0);
            let _ = update(&mut app, Message::SetGalleryMode(from));
            settle(&mut app);
        }

        app.save_gallery_preset().unwrap();
        let saved = app
            .state
            .mode_profiles
            .selected_id(gallery_kind(from))
            .unwrap();
        let _ = match from {
            GalleryMode::Slices => update(
                &mut app,
                Message::SetGalleryNumber(GalleryNumberField::SliceWidth, 185.0),
            ),
            GalleryMode::Geometric => update(
                &mut app,
                Message::SetGeometricCurve(StateGeometricCurve::Wave),
            ),
            GalleryMode::Wall => update(&mut app, Message::SetWallLayout(StateWallLayout::Masonry)),
            GalleryMode::Depth => {
                let shadows = !app.state.gallery.depth.shadows;
                update(&mut app, Message::SetDepthShadows(shadows))
            }
        };
        settle(&mut app);
        let before = view::settings::profile_projection::capture(&app);
        let snapshot = app
            .state
            .mode_profiles
            .snapshot(gallery_kind(from), saved)
            .unwrap();
        assert_ne!(
            before, snapshot,
            "{from:?} needs different geometry before applying the preset"
        );
        app.apply_gallery_preset(gallery_kind(from), saved).unwrap();
        assert_eq!(app.scene_motion.opacity(), 0.0, "{from:?} preset");
        assert!(advance(&mut app, 0.05));
        assert!((0.0..1.0).contains(&app.scene_motion.opacity()));
        settle(&mut app);
        assert_eq!(app.scene_motion.opacity(), 1.0);
    }
}

#[test]
fn studio_uses_live_geometry_and_settles_in_all_modes() {
    for (mode, circle) in [
        (GalleryMode::Slices, false),
        (GalleryMode::Geometric, false),
        (GalleryMode::Geometric, true),
        (GalleryMode::Wall, false),
        (GalleryMode::Depth, false),
    ] {
        for rtl in [false, true] {
            let (mut app, _) = fixture(mode);
            app.viewport = [760.0, 650.0];
            if circle {
                app.state.gallery.geometric.shape = StateGeometricShape::Circle;
            }
            if rtl {
                app.settings_locale = localization::Catalog::with_embedded_rtl_for_test();
            }
            assert_eq!(app.settings_locale.language.rtl, rtl);
            let _ = update(&mut app, Message::ToggleSettingsPanel);
            settings(&mut app, intent::SettingsMsg::SelectTab("picker".into()));
            app.schedule_posters();
            settle(&mut app);
            let before = center(&app)[0];
            settings(
                &mut app,
                intent::SettingsMsg::SelectSection(view::settings::composed::PICKER_LAYOUT_SECTION),
            );
            assert!(app.settings_workbench.layout_studio());
            assert_eq!(center(&app)[0], before);
            assert!(advance(&mut app, 0.032));
            let middle = center(&app)[0];
            assert!((0.0..1.0).contains(&app.scene_motion.opacity()));
            settle(&mut app);
            let endpoint = center(&app)[0];
            match (mode, rtl) {
                (GalleryMode::Wall, _) => assert_eq!((middle, endpoint), (before, before)),
                (_, true) => assert!(endpoint < middle && middle < before, "{mode:?}"),
                _ => assert!(before < middle && middle < endpoint, "{mode:?}"),
            }
            assert_eq!(app.scene_motion.opacity(), 1.0);
            settings(&mut app, intent::SettingsMsg::LeaveLayoutStudio);
            assert_eq!(center(&app)[0], endpoint);
            settle(&mut app);
            assert_eq!(center(&app)[0], before);
            assert_eq!(app.scene_motion.opacity(), 0.0);
        }
    }
}

fn corners(app: &GpuCandidate) -> [f32; 4] {
    app.ordinary_base_frame()
        .into_iter()
        .find(|e| e.card.selected)
        .unwrap()
        .radii
}

fn tick_slices(app: &mut GpuCandidate) {
    app.gallery_presentation.as_mut().unwrap().tick(0.016);
}

#[test]
fn corners_reverse_and_reload_from_raw_settings() {
    let (mut app, _) = fixture(GalleryMode::Slices);
    app.state.gallery.slices.skew_distance = 0.0;
    app.state.gallery.slices.edge_tilt_distance = 0.0;
    app.state.gallery.slices.height = 70.0;
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    settings(&mut app, intent::SettingsMsg::SelectTab("picker".into()));
    settings(
        &mut app,
        intent::SettingsMsg::SelectSection(view::settings::composed::PICKER_LAYOUT_SECTION),
    );
    let toggle = |app: &mut GpuCandidate, on| {
        settings(
            app,
            intent::SettingsMsg::Toggle(selector::ROUND_CORNERS.into(), on),
        )
    };
    for (key, value) in [
        (selector::CORNER_TL, "5"),
        (selector::CORNER_TR, "23"),
        (selector::CORNER_BR, "75"),
    ] {
        settings(
            &mut app,
            intent::SettingsMsg::Input(key.into(), value.into()),
        );
        settings(&mut app, intent::SettingsMsg::Commit);
    }
    toggle(&mut app, true);
    for _ in 0..100 {
        tick_slices(&mut app);
    }
    let rounded = corners(&app);
    assert_eq!(
        rounded,
        [5.0, 23.0, 34.0, app.state.gallery.slices.corner_radius]
    );
    let hit = |app: &GpuCandidate| {
        let entries = app.ordinary_base_frame();
        let entry = entries.iter().find(|e| e.card.selected).unwrap();
        let [x, y] = entry.card.center;
        let [w, h] = entry.card.size;
        donor_scene::presented_slice_contains(
            entry,
            &gpu_gallery::GalleryColors::default(),
            app.viewport,
            [x + w * 0.5 - 5.0, y + h * 0.5 - 5.0],
        )
    };
    assert!(!hit(&app));
    toggle(&mut app, false);
    assert_eq!(corners(&app), rounded);
    tick_slices(&mut app);
    let middle = corners(&app);
    assert!(middle.iter().zip(rounded).all(|(&r, t)| r > 0.0 && r < t));
    toggle(&mut app, true);
    assert_eq!(corners(&app), middle);
    tick_slices(&mut app);
    assert!(corners(&app).iter().zip(middle).all(|(&r, t)| r > t));
    toggle(&mut app, false);
    for _ in 0..100 {
        tick_slices(&mut app);
    }
    assert_eq!(corners(&app), [0.0; 4]);
    assert!(hit(&app));
    app.state = CandidateState::load(&app.state_path);
    use view::settings::composed::SettingsSource;
    assert_eq!(
        (view::settings::bridge::Source { app: &app }).number(selector::CORNER_BR),
        75.0
    );
    app.gallery_presentation = None;
    app.sync_gallery_presentation();
    assert_eq!(corners(&app), [0.0; 4]);
    toggle(&mut app, true);
    for _ in 0..100 {
        tick_slices(&mut app);
    }
    assert_eq!(corners(&app), rounded);
}

#[test]
#[ignore = "PK-09 production motion frames; explicit headless invocation"]
fn production_motion_compositions() {
    let out = std::path::PathBuf::from(
        std::env::var_os("P9_LAYOUT_OUTPUT").expect("owned frame directory"),
    );
    std::fs::create_dir_all(&out).unwrap();
    let mut renderer = renderer();
    let mut fonts = iced::advanced::graphics::text::font_system()
        .write()
        .unwrap();
    fonts.load_font(std::borrow::Cow::Borrowed(
        localization::Catalog::with_embedded_rtl_for_test().font_bytes(),
    ));
    drop(fonts);
    for (width, height, scale, lang) in [
        (1920, 1080, 1.0, "en-US"),
        (640, 600, 0.5, "ar-SA"),
        (2560, 1440, 1.0, "en-US"),
    ] {
        for mode in [
            GalleryMode::Slices,
            GalleryMode::Geometric,
            GalleryMode::Wall,
            GalleryMode::Depth,
        ] {
            let (mut app, _) = fixture(mode);
            app.viewport = [width as f32, height as f32];
            app.state.frontend_preferences.interface.ui_scale = Some(scale);
            if lang == "ar-SA" {
                app.settings_locale = localization::Catalog::with_embedded_rtl_for_test();
            }
            assert_eq!(app.settings_locale.language.tag, lang);
            let mut posters = Vec::new();
            let generation = app.poster_generation.load(Ordering::Acquire);
            let catalog_generation = app.catalog_generation.load(Ordering::Acquire);
            for (index, (item, color)) in app
                .wallpapers
                .iter_mut()
                .zip([[0, 50, 250, 255], [250, 35, 0, 255], [90, 95, 100, 255]])
                .enumerate()
            {
                let path = app
                    .state_path
                    .with_file_name(format!("fixture-{index}.png"));
                ::image::RgbaImage::from_pixel(160, 90, ::image::Rgba(color))
                    .save(&path)
                    .unwrap();
                item.file_target = path.clone();
                item.preview_path = Some(path.clone());
                item.source = WallpaperSource::LocalImage;
                let layer = index as u32;
                posters.push(gpu_gallery::decode_poster(&path, layer).unwrap());
                app.poster_slots.insert(
                    index,
                    PosterSlot {
                        layer,
                        kind: crate::poster_worker::PosterKind::Still,
                        generation,
                        catalog_generation,
                        item_id: item.id.clone(),
                        source: path,
                    },
                );
            }
            app.posters = Arc::new(posters);
            app.schedule_posters();
            let mut save = |app: &GpuCandidate, name: &str| {
                Scene::new(app, &renderer).save(
                    &mut renderer,
                    app,
                    &out.join(format!("{width}x{height}-{lang}-{mode:?}-{name}.png")),
                );
            };
            if mode == GalleryMode::Slices {
                save(&app, "gallery");
                let _ = update(&mut app, Message::TogglePlaylistChooser);
                advance(&mut app, 0.032);
                save(&app, "chooser-mid");
                settle(&mut app);
                save(&app, "chooser-settled");
                let owner = app.playlist_owner().unwrap();
                let _ = update(
                    &mut app,
                    Message::PlaylistBrowse(owner, playlists::Browse::Edit),
                );
                save(&app, "manager");
                d3_key(&mut app, keyboard::key::Named::Escape);
                advance(&mut app, 0.032);
                save(&app, "manager-return-mid");
                settle(&mut app);
            }
            let _ = update(&mut app, Message::ToggleSettingsPanel);
            settings(&mut app, intent::SettingsMsg::SelectTab("picker".into()));
            app.settings_workbench.folio_entrance = None;
            settle(&mut app);
            save(&app, "settings-settled");
            settings(
                &mut app,
                intent::SettingsMsg::SelectSection(view::settings::composed::PICKER_LAYOUT_SECTION),
            );
            app.settings_workbench.tab_reveal = None;
            advance(&mut app, 0.032);
            save(&app, "studio-entry-mid");
            settle(&mut app);
            save(&app, "studio-settled");
            if mode == GalleryMode::Slices {
                settings(
                    &mut app,
                    intent::SettingsMsg::Toggle(selector::ROUND_CORNERS.into(), false),
                );
                tick_slices(&mut app);
                save(&app, "corners-mid");
                for _ in 0..100 {
                    tick_slices(&mut app);
                }
                save(&app, "corners-off");
            }
            settings(&mut app, intent::SettingsMsg::LeaveLayoutStudio);
            app.settings_workbench.tab_reveal = None;
            advance(&mut app, 0.032);
            save(&app, "studio-return-mid");
        }
    }
}

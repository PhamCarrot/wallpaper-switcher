use super::*;
use std::time::Instant;

fn fixture(count: usize, mode: GalleryMode) -> GpuCandidate {
    fixture_with_counter(count, mode).0
}

fn fixture_with_counter(
    count: usize,
    mode: GalleryMode,
) -> (GpuCandidate, Arc<std::sync::atomic::AtomicUsize>) {
    let (mut app, submissions) = d4a_fixture(count);
    for (i, w) in app.wallpapers.iter_mut().enumerate() {
        let owned = app
            .state_path
            .parent()
            .unwrap()
            .join("absent-owned-media")
            .join(format!("{i}.png"));
        w.file_target = owned.clone();
        w.preview_path = Some(owned);
    }
    app.poster_worker =
        Some(poster_worker::PosterWorker::spawn(app.poster_generation.clone(), |_| true).unwrap());
    app.deterministic_animation = true;
    app.gallery_mode = mode;
    app.depth_frame = None;
    app.browse_position_restored = true;
    app.selected = Some(count / 2);
    app.sync_gallery_presentation();
    if let Some(p) = &mut app.gallery_presentation {
        p.settle();
    }
    if let Some(p) = &mut app.hex_presentation {
        p.settle();
    }
    if let Some(p) = &mut app.wall_presentation {
        p.settle();
    }
    app.ordinary_frame();
    (app, submissions)
}

#[test]
fn p55_unchanged_layout_ticks_visit_a_bounded_window_at_all_library_sizes() {
    for count in [250, 2_000, 10_000] {
        for mode in [
            GalleryMode::Slices,
            GalleryMode::Geometric,
            GalleryMode::Wall,
        ] {
            let mut app = fixture(count, mode);
            let before = donor_layout::layout_work_snapshot();
            let started = Instant::now();
            for _ in 0..20 {
                assert!(!app.sync_gallery_presentation());
                if let Some(p) = &mut app.gallery_presentation {
                    assert!(!p.tick(1.0 / 60.0));
                }
                if let Some(p) = &mut app.hex_presentation {
                    assert!(!p.tick(1.0 / 60.0));
                }
                if let Some(p) = &mut app.wall_presentation {
                    assert!(!p.tick(1.0 / 60.0));
                }
                assert!(!app.ordinary_frame().is_empty());
                assert!(!app.ordinary_poster_demand().0.is_empty());
            }
            let after = donor_layout::layout_work_snapshot();
            let work = std::array::from_fn::<_, 3, _>(|i| after[i] - before[i]);
            println!("P55 size={count} mode={mode:?} ticks=20 input_visits={} placement_build_visits={} candidates={} elapsed_us={}",
                work[0], work[1], work[2], started.elapsed().as_micros());
            assert_eq!(
                &work[..2],
                &[0, 0],
                "unchanged ticks rebuilt static inputs/placements"
            );
            assert!(
                work[2] > 0 && work[2] < 20 * 500,
                "full-list traversal: {work:?}"
            );
        }
    }
}

#[test]
fn p55_wall_index_matches_full_scan_for_six_layouts_stage_camera_and_aspects() {
    use gpu_gallery::{GalleryLayoutParams, WallLayout, WallLayoutWindow, WallParams};
    for count in [250, 2_000, 10_000] {
        let aspects: Vec<_> = (0..count).map(|i| [0.4, 0.9, 1.7, 2.8][i % 4]).collect();
        for layout in [
            WallLayout::Uniform,
            WallLayout::Brick,
            WallLayout::Masonry,
            WallLayout::Justified,
            WallLayout::Editorial,
            WallLayout::Cylinder,
        ] {
            for rotation in [0.0, 34.0, 120.0, 180.0] {
                let mut p = WallParams {
                    layout,
                    columns: 7,
                    rows: 5,
                    card_width: 91.25,
                    card_height: 64.5,
                    gap_x: 7.2,
                    gap_y: 8.6,
                    scatter: 0.7,
                    scale_variance: 0.5,
                    selected_scale: 1.8,
                    user_engaged: true,
                    ..Default::default()
                };
                p.stage.rotation = rotation;
                p.stage.shear_x = 0.3;
                p.stage.shear_y = -0.2;
                p.stage.scale = 0.85;
                p.stage.perspective = -0.6;
                p.stage.depth_angle = 60.0;
                p.stage.offset_x = 0.1;
                p.stage.offset_y = -0.15;
                let cached = WallLayoutWindow::new(count, p, &aspects);
                for camera in [0.0, 101.25, cached.max_scroll * 0.5, f32::MAX] {
                    p.scroll_offset = camera;
                    for viewport in [[1920.0, 1080.0], [800.0, 650.0]] {
                        p.rail_width = Some(170.0);
                        p.rail_rtl = viewport[0] < 1000.0;
                        let actual = cached.cards(count / 2, viewport, p, &aspects);
                        let oracle = gpu_gallery::layout_gallery_with_aspects(
                            GalleryMode::Wall,
                            count,
                            count / 2,
                            viewport,
                            GalleryLayoutParams {
                                wall: p,
                                ..Default::default()
                            },
                            &aspects,
                        );
                        assert_eq!(
                            format!("{actual:?}"),
                            format!("{oracle:?}"),
                            "size={count} {layout:?} rotation={rotation} camera={camera}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn p55_filter_catalog_geometry_and_dynamic_identity_invalidate_at_their_owners() {
    let mut app = fixture(250, GalleryMode::Wall);
    let ids = app.wall_presentation.as_ref().unwrap().ids.clone();
    let before = donor_layout::layout_work_snapshot();
    app.refresh_filter();
    app.sync_gallery_presentation();
    app.wall_presentation.as_mut().unwrap().settle();
    app.ordinary_frame();
    let after_filter = donor_layout::layout_work_snapshot();
    assert_eq!(
        after_filter[0], before[0],
        "no-op filter retains immutable inputs"
    );
    assert!(Arc::ptr_eq(
        &ids,
        &app.wall_presentation.as_ref().unwrap().ids
    ));
    let revision = app.catalog_generation.load(Ordering::Acquire);
    app.sort_order = SortOrder::Newest;
    app.refresh_filter();
    app.sync_gallery_presentation();
    assert_eq!(app.catalog_generation.load(Ordering::Acquire), revision);
    assert_ne!(
        ids.as_ref(),
        app.wall_presentation.as_ref().unwrap().ids.as_ref()
    );
    app.state.gallery.wall.layout = StateWallLayout::Masonry;
    app.sync_gallery_presentation();
    app.wall_presentation.as_mut().unwrap().settle();
    let reordered = app.wall_presentation.as_ref().unwrap().ids.clone();
    let changed = app.filtered_indices[0];
    app.catalog[changed].width = 71;
    app.catalog[changed].height = 999;
    app.catalog_generation.fetch_add(1, Ordering::AcqRel);
    assert!(app.sync_gallery_presentation());
    assert_eq!(
        reordered.as_ref(),
        app.wall_presentation.as_ref().unwrap().ids.as_ref()
    );
    app.wall_presentation.as_mut().unwrap().settle();
    let state = app.wall_presentation.as_ref().unwrap();
    let oracle = gpu_gallery::layout_gallery_with_aspects(
        GalleryMode::Wall,
        app.filtered_indices.len(),
        app.filtered_indices
            .iter()
            .position(|i| Some(*i) == app.selected)
            .unwrap_or(0),
        app.viewport,
        gpu_gallery::GalleryLayoutParams {
            wall: state.frame_params(gallery_layout_params_for_app(&app).wall),
            ..Default::default()
        },
        &filtered_poster_aspects(&app.catalog, &app.filtered_indices),
    );
    let expected: Vec<_> = oracle
        .iter()
        .map(|c| (app.filtered_indices[c.index], c.center, c.size))
        .collect();
    let actual: Vec<_> = app
        .ordinary_frame()
        .iter()
        .map(|e| (e.card.index, e.card.center, e.card.size))
        .collect();
    assert_eq!(
        actual, expected,
        "same-ID dimensions must rebuild Masonry geometry"
    );
    let before_material_edit = donor_layout::layout_work_snapshot();
    app.state.gallery.wall.border_width =
        Some(app.state.gallery.wall.border_width.unwrap_or(1.0) + 1.0);
    app.state.gallery.wall.selected_scale = 1.7;
    app.sync_gallery_presentation();
    app.wall_presentation.as_mut().unwrap().settle();
    app.ordinary_frame();
    assert_eq!(
        &donor_layout::layout_work_snapshot()[..2],
        &before_material_edit[..2],
        "border and selection focus must not rebuild immutable placements"
    );
    let frame = app.ordinary_frame();
    assert!(!frame.is_empty());
    let index = frame[0].card.index;
    app.wallpapers[index].preview_path = Some("p55-replaced-source.png".into());
    let before_material = donor_layout::layout_work_snapshot();
    let current = app.ordinary_frame();
    let entry = current.iter().find(|e| e.card.index == index).unwrap();
    assert_eq!(
        entry.source,
        std::path::PathBuf::from("p55-replaced-source.png")
    );
    assert_eq!(entry.card.layer, None);
    assert_eq!(
        &donor_layout::layout_work_snapshot()[..2],
        &before_material[..2]
    );
    let old_demand = app.ordinary_poster_demand().0;
    let p = app.wall_presentation.as_mut().unwrap();
    p.wheel(5000.0);
    p.settle();
    assert_ne!(app.ordinary_poster_demand().0, old_demand);
    app.viewport = [900.0, 650.0];
    app.scene_motion.retarget(true, 200.0, 0.0, 0.0);
    app.sync_gallery_presentation();
    for e in app.ordinary_frame() {
        assert_eq!(e.id, app.wallpapers[e.card.index].id);
        assert!(app.ordinary_poster_indices().contains(&e.card.index));
    }
}

#[test]
fn p55_axis_window_keeps_exact_boundary_reversed_collapsed_and_nonfinite_cases() {
    for stride in [-4.25, 0.0, 4.25, f32::NAN] {
        let value = |i| 20.0 + i as f32 * stride;
        let expected: Vec<_> = (0..100)
            .filter(|&i| !(value(i) < -7.0 || value(i) > 100.0))
            .collect();
        let actual: Vec<_> = donor_layout::monotone_window(100, value, -7.0, 100.0)
            .filter(|&i| !(value(i) < -7.0 || value(i) > 100.0))
            .collect();
        assert_eq!(actual, expected);
    }
}

#[test]
fn p55_wall_empty_offscreen_composition_does_not_panic() {
    let mut p = gpu_gallery::WallParams {
        columns: 1,
        rows: 1,
        card_width: 10.0,
        card_height: 10.0,
        rail_height: Some(300.0),
        rail_gap: 0.0,
        ..Default::default()
    };
    p.stage.offset_y = -1.0;
    let cached = gpu_gallery::WallLayoutWindow::new(1, p, &[]);
    let actual = cached.cards(0, [100.0, 100.0], p, &[]);
    let oracle = gpu_gallery::layout_gallery_with_aspects(
        GalleryMode::Wall,
        1,
        0,
        [100.0, 100.0],
        gpu_gallery::GalleryLayoutParams {
            wall: p,
            ..Default::default()
        },
        &[],
    );
    assert_eq!(format!("{actual:?}"), format!("{oracle:?}"));
}

#[test]
fn p55_wall_negative_gaps_keep_clamped_frame_and_draw_camera_contracts() {
    for (gap_x, gap_y) in [(-2000.0, 0.0), (0.0, -2000.0), (-2000.0, -2000.0)] {
        for rotation in [0.0, 90.0, 180.0] {
            let mut p = gpu_gallery::WallParams {
                columns: 2,
                rows: 2,
                card_width: 10.0,
                card_height: 10.0,
                gap_x,
                gap_y,
                ..Default::default()
            };
            p.stage.rotation = rotation;
            let cached = gpu_gallery::WallLayoutWindow::new(8, p, &[]);
            for camera in [0.0, 10000.0] {
                p.scroll_offset = camera;
                let actual = cached.cards(1, [100.0, 100.0], p, &[]);
                let oracle = gpu_gallery::layout_gallery_with_aspects(
                    GalleryMode::Wall,
                    8,
                    1,
                    [100.0, 100.0],
                    gpu_gallery::GalleryLayoutParams {
                        wall: p,
                        ..Default::default()
                    },
                    &[],
                );
                assert_eq!(
                    format!("{actual:?}"),
                    format!("{oracle:?}"),
                    "gaps={gap_x},{gap_y} rotation={rotation} camera={camera}"
                );
            }
        }
    }
}

#[test]
fn p55_slices_window_tracks_original_accumulation_with_sparse_widths() {
    let mut worst = 0.0_f32;
    for count in [250, 2_000, 10_000] {
        let mut app = fixture(count, GalleryMode::Slices);
        app.state.gallery.slices.width = 135.137;
        app.state.gallery.slices.spacing = 3.333;
        app.state.gallery.slices.expanded_width = 923.627;
        app.state.gallery.slices.skew_distance = 18.29;
        app.sync_gallery_presentation();
        app.gallery_presentation.as_mut().unwrap().settle();
        for outgoing in [None, Some(5), Some(count / 2 - 3)] {
            let state = app.gallery_presentation.as_mut().unwrap();
            state.widths.clear();
            if let Some(i) = outgoing {
                state.widths.insert(
                    state.ids[i].clone(),
                    GalleryMotionState::source(417.875, 135.137, 250.0, 1.0),
                );
            }
            let [center, available] = app.gallery_composition();
            let state = app.gallery_presentation.as_ref().unwrap();
            let p = &state.slices;
            let layout = |width: f32| (width - p.skew_distance.abs()).max(1.0);
            let stride = layout(p.width) + p.spacing;
            let half = (layout(p.expanded_width)
                + p.visible_count.saturating_sub(1) as f32 * stride)
                .min(available - 40.0)
                .max(1.0)
                * 0.5;
            let cx = center + app.viewport[0] * p.offset_x * 0.5;
            let mut acc = 0.0;
            let mut expected = Vec::new();
            for (index, id) in state.ids.iter().enumerate() {
                let width = state
                    .widths
                    .get(id)
                    .map_or(
                        if *id == state.selected {
                            p.expanded_width
                        } else {
                            p.width
                        },
                        |s| s.value(),
                    )
                    .max(1.0);
                let x = cx - state.camera.value() + acc + layout(width) * 0.5;
                acc += layout(width) + p.spacing;
                let opacity = crate::upstream_slices_adapter::slice_opacity(
                    x,
                    cx,
                    half,
                    layout(p.expanded_width),
                    stride,
                );
                if opacity > 0.01 {
                    expected.push((index, x, width));
                }
            }
            let actual = app.ordinary_frame();
            assert_eq!(actual.len(), expected.len());
            for (index, x, width) in expected {
                let card = actual.iter().find(|e| e.card.index == index).unwrap().card;
                worst = worst.max((card.center[0] - x).abs());
                assert!(
                    (card.center[0] - x).abs() <= 0.125,
                    "size={count} index={index} outgoing={outgoing:?} x={} oracle={x}",
                    card.center[0]
                );
                assert_eq!(card.size[0], width);
            }
        }
    }
    println!("P55 Slices original accumulation maximum coordinate delta={worst}px");
}

#[test]
fn p56_terminal_motion_frame_preserves_demand_and_navigation_restarts() {
    let mut app = fixture(16, GalleryMode::Slices);
    app.poster_worker =
        Some(poster_worker::PosterWorker::spawn(app.poster_generation.clone(), |_| true).unwrap());
    app.schedule_posters_with_still_selector(false, |_, _| None);
    let generation = app.motion_generation.load(Ordering::Acquire);
    let ordinary = app.gallery_presentation.clone();
    let ids = ordinary.as_ref().unwrap().ids.clone();
    let _ = update(
        &mut app,
        Message::Wake(Wake::MotionFrame {
            generation,
            value: 0.0,
            velocity: 0.0,
            state: GalleryMotionState::with_velocity(0.0, 0.0, 0.0, 250),
            selection: Default::default(),
            presentation: None,
            filter_wave: None,
            ordinary,
            hex: None,
            wall: None,
            scene: None,
        }),
    );
    assert_eq!(app.motion_generation.load(Ordering::Acquire), generation);
    assert!(Arc::ptr_eq(
        &ids,
        &app.gallery_presentation.as_ref().unwrap().ids
    ));
    app.navigate_selection(1);
    assert_eq!(
        app.motion_generation.load(Ordering::Acquire),
        generation + 1
    );
    assert_eq!(app.selected, Some(9));
}

/// Owns the wake route and real producer threads; run alone with one test thread.
#[test]
#[ignore = "owned no-input motion runtime probe; run exact with --test-threads=1 --nocapture"]
fn p56_owned_producers_publish_terminal_frames_stop_and_restart() {
    struct RestoreWake(Option<UnboundedSender<Wake>>);
    impl Drop for RestoreWake {
        fn drop(&mut self) {
            *WAKE_TX.get().unwrap().lock().unwrap() = self.0.take();
        }
    }
    fn drain(
        app: &mut GpuCandidate,
        rx: &mut futures_channel::mpsc::UnboundedReceiver<Wake>,
    ) -> usize {
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        let mut last = Instant::now();
        let mut frames = 0;
        let mut terminal = false;
        loop {
            assert!(Instant::now() < deadline, "producer failed to settle");
            match rx.try_recv() {
                Ok(wake) => {
                    if matches!(wake, Wake::MotionReady) {
                        frames += 1;
                        last = Instant::now();
                    }
                    if matches!(wake, Wake::MotionReady) {
                        let frame = app
                            .motion_mailbox
                            .take()
                            .expect("motion marker without payload");
                        if let Wake::MotionFrame {
                            state,
                            scene,
                            filter_wave,
                            ..
                        } = &frame
                        {
                            terminal = state.settled()
                                && scene.is_none_or(|s| !s.unsettled())
                                && filter_wave.is_none_or(|wave| wave >= 1.6);
                        }
                        let _ = update(app, Message::Wake(frame));
                    } else {
                        let _ = update(app, Message::Wake(wake));
                    }
                }
                Err(_) => {
                    if frames > 0
                        && terminal
                        && app.motion_mailbox.active_producers() == 0
                        && last.elapsed() >= std::time::Duration::from_millis(250)
                    {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(2));
                }
            }
        }
        assert!(
            app.motion_mailbox.take().is_none(),
            "terminal payload was not drained"
        );
        if let Some(p) = &mut app.gallery_presentation {
            assert!(!p.tick(1.0 / 60.0));
        }
        if let Some(p) = &mut app.hex_presentation {
            assert!(!p.tick(1.0 / 60.0));
        }
        if let Some(p) = &mut app.wall_presentation {
            assert!(!p.tick(1.0 / 60.0));
        }
        frames
    }
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
    ] {
        let (mut app, submissions) = fixture_with_counter(10_000, mode);
        app.deterministic_animation = false;
        let (tx, mut rx) = unbounded();
        let _restore = RestoreWake(
            WAKE_TX
                .get_or_init(|| Mutex::new(None))
                .lock()
                .unwrap()
                .replace(tx),
        );
        app.start_motion(0.0);
        let first = drain(&mut app, &mut rx);
        let generation = app.motion_generation.load(Ordering::Acquire);
        app.navigate_selection(1);
        assert!(app.motion_generation.load(Ordering::Acquire) > generation);
        let navigation = drain(&mut app, &mut rx);
        if mode == GalleryMode::Wall {
            let _ = update(
                &mut app,
                Message::Gallery(GalleryMessage::Hovered(Some(5001))),
            );
            drain(&mut app, &mut rx);
            for (menu, target) in [
                (Message::ToggleSettingsPanel, Some(5001)),
                (Message::TogglePlaylistChooser, None),
            ] {
                let _ = update(&mut app, menu.clone());
                assert!(drain(&mut app, &mut rx) > 0);
                assert_eq!(app.scene_motion.opacity(), 0.0);
                let _ = update(&mut app, menu);
                let close = app.motion_generation.load(Ordering::Acquire);
                super::pk09::wall_hover_burst(&mut app, target);
                assert_eq!(app.motion_generation.load(Ordering::Acquire), close);
                let frames = drain(&mut app, &mut rx);
                assert!(frames > 0);
                assert_eq!(app.motion_generation.load(Ordering::Acquire), close);
                assert_eq!(app.scene_motion.opacity(), 1.0);
                assert_eq!(app.scene_motion.settings_opacity(), 0.0);
                assert_eq!(app.scene_motion.playlist_opacity(), 0.0);
                println!("W4 close_gen={close} target={target:?} frames={frames} terminal=true quiet_ms=250");

                let next = if target.is_some() { None } else { Some(5001) };
                super::pk09::wall_hover_burst(&mut app, next);
                assert_eq!(app.motion_generation.load(Ordering::Acquire), close + 1);
                assert!(drain(&mut app, &mut rx) > 0);
            }
        }
        app.viewport[0] -= 100.0;
        if mode == GalleryMode::Wall {
            app.state.gallery.wall.card_width += 25.0;
        }
        app.schedule_posters();
        let layout = drain(&mut app, &mut rx);
        // The same producer advances filter waves even with a settled camera.
        app.ordinary_roll = Some(ordinary_roll::OrdinaryRoll {
            wave: 0.0,
            old: Vec::new(),
            incoming: HashMap::new(),
            poster_wait_deadline: None,
        });
        app.start_motion(0.0);
        let filter = drain(&mut app, &mut rx);
        app.visible = false;
        app.stop_gallery_transients("owned-p56-hide");
        app.visible = true;
        app.restart_gallery_entrance();
        app.start_motion(0.0);
        let reopen = drain(&mut app, &mut rx);
        println!("P56 mode={mode:?} initial={first} navigation={navigation} layout={layout} filter={filter} reopen={reopen} quiet_ms=250 terminal_drained=true");
        app.stop_gallery_transients("owned-p56-stop");
        assert_eq!(submissions.load(Ordering::Acquire), 0);
    }
}

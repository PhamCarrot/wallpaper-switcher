use super::*;
use crate::candidate_app::detail_motion::advance_detail;

fn fixture(mode: GalleryMode, count: usize) -> GpuCandidate {
    let (mut app, _) = d4a_fixture(count);
    app.browse_position_restored = true;
    app.set_gallery_mode(mode);
    app.selected = Some(3.min(count - 1));
    app.sync_gallery_presentation();
    if let Some(p) = &mut app.gallery_presentation {
        p.settle();
    }
    if let Some(p) = &mut app.hex_presentation {
        p.settle();
    }
    if let Some(p) = &mut app.wall_presentation {
        p.settle();
        p.entrance = GalleryMotionState::source(1.0, 1.0, 250.0, 1.0);
    }
    if mode == GalleryMode::Depth {
        app.reset_depth_transients();
    }
    app.depth_presentation = Some(DepthPresentation::target(&app.state));
    app.rebuild_depth_frame();
    app
}
fn scene(app: &GpuCandidate) -> DonorSceneState {
    DonorSceneState {
        stage: Default::default(),
        camera: 0.0,
        camera_target: 0.0,
        camera_velocity: 0.0,
        detail_progress: app.detail_progress,
        external_detail: matches!(app.gallery_mode, GalleryMode::Wall | GalleryMode::Geometric)
            || app.depth_panel_open(),
        flip_options: app.flip_presentation(),
        detail_source: app.detail_source,
        detail_card: app.external_detail_card(),
        viewport: app.viewport,
        time: 0.0,
        visibility: 1.0,
    }
}
fn raw(app: &GpuCandidate) -> Vec<donor_scene::InstanceRaw> {
    if let Some(f) = &app.depth_frame {
        donor_scene::depth_instances(f, &GalleryColors::default(), 1.0, &scene(app))
    } else {
        donor_scene::presented_instances(
            &app.ordinary_frame(),
            &GalleryColors::default(),
            1.0,
            &scene(app),
        )
    }
}
fn tick(app: &mut GpuCandidate, dt: f32) {
    let mut spring = app.depth_detail.unwrap();
    let external = matches!(app.gallery_mode, GalleryMode::Geometric | GalleryMode::Wall);
    let (value, done) = advance_detail(&mut spring, app.detail_progress, external, 250.0, dt);
    let generation = app.detail_generation.load(Ordering::Acquire);
    let _ = update(
        app,
        Message::Wake(Wake::DetailFrame {
            generation,
            value,
            close_when_done: done && spring.target == 0.0,
            spring: Some(spring),
        }),
    );
}

#[test]
fn p10_shared_twelve_effects_reach_all_four_real_shader_consumers() {
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
        GalleryMode::Depth,
    ] {
        let mut app = fixture(mode, 20);
        for effect in candidate_state::DEPTH_EFFECTS {
            let _ = update(&mut app, Message::SetDepthEffect(effect.into()));
            app.start_detail(true, "p10-effects");
            app.detail_progress = 0.74;
            app.rebuild_depth_frame();
            let id = candidate_state::depth_effect_id(effect).unwrap();
            let selected = app.selected.unwrap();
            let expected = donor_scene::card_flip_shader_payload(0.74 / 0.82, selected, id);
            assert!(
                raw(&app).iter().any(|r| r.flip == expected),
                "{mode:?} {effect}"
            );
            app.cancel_detail("next-effect");
        }
        let element = view::settings::motion_settings_content(&app);
        assert!(!iced::advanced::widget::Tree::new(&element)
            .children
            .is_empty());
    }
}

#[test]
fn p10_source_spring_reverse_phase_boundaries_and_external_standard_ramp_are_distinct() {
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
        GalleryMode::Depth,
    ] {
        let mut app = fixture(mode, 20);
        app.state.card_flip_mut().duration_ms = 1500;
        app.start_detail(true, "p10-open");
        assert_eq!(app.depth_detail.unwrap().target, 1.0);
        let mut checked = 0;
        for boundary in [0.5, 0.74, 0.82, 0.94] {
            while app.detail_progress < boundary {
                tick(&mut app, 1.0 / 120.0);
            }
            let p = donor_scene::card_flip_phases(app.detail_progress, true, true);
            assert!(p.shader >= boundary / 0.82_f32.max(boundary));
            let before = app.depth_detail.unwrap();
            let value = app.detail_progress;
            let generation = app.detail_generation.load(Ordering::Acquire);
            app.start_detail(false, "p10-close");
            assert_eq!(
                (
                    app.depth_detail.unwrap().x,
                    app.depth_detail.unwrap().velocity
                ),
                (before.x, before.velocity)
            );
            tick(&mut app, 1.0 / 120.0);
            app.start_detail(true, "p10-reopen");
            let current = app.detail_progress;
            let _ = update(
                &mut app,
                Message::Wake(Wake::DetailFrame {
                    generation,
                    value: 0.0,
                    close_when_done: true,
                    spring: None,
                }),
            );
            assert_eq!(
                app.detail_progress, current,
                "stale close must not win a reopen"
            );
            assert!(app.detail_open);
            assert!(value >= boundary);
            checked += 1;
        }
        assert_eq!(checked, 4);
        app.start_detail(false, "p10-final-close");
        for _ in 0..1000 {
            if !app.detail_open {
                break;
            }
            tick(&mut app, 1.0 / 120.0);
        }
        assert!(!app.detail_open);
        assert_eq!(app.detail_progress, 0.0);
        assert!(app.depth_detail.is_none());
        let generation = app.detail_generation.load(Ordering::Acquire);
        let _ = update(&mut app, Message::SetDepthEffect("Ash".into()));
        assert!(!app.detail_open && app.depth_detail.is_none() && app.detail_source.is_none());
        assert_eq!(app.detail_generation.load(Ordering::Acquire), generation);
    }
    let mut slow = GalleryMotionState::source(0.0, 1.0, 4000.0, 1.0);
    let mut fast = GalleryMotionState::source(0.0, 1.0, 100.0, 1.0);
    let (a, _) = advance_detail(&mut slow, 0.0, true, 250.0, 0.05);
    let (b, _) = advance_detail(&mut fast, 0.0, true, 250.0, 0.05);
    assert_eq!(a, b);
    assert!((a - 1.0 / 7.5).abs() < 0.00001);
    assert_ne!(slow.value(), fast.value());
    let mut spring = GalleryMotionState::source(0.0, 1.0, 4000.0, 1.0);
    let mut progress = 0.0;
    let mut done = false;
    for _ in 0..60 {
        (progress, done) = advance_detail(&mut spring, progress, true, 250.0, 1.0 / 120.0);
    }
    assert_eq!(progress, 1.0);
    assert!(
        !done && !spring.settled(),
        "configured external spring remains clocked after Standard travel"
    );
    for _ in 0..1000 {
        if done {
            break;
        }
        (progress, done) = advance_detail(&mut spring, progress, true, 250.0, 1.0 / 120.0);
    }
    assert!(done && spring.settled());
    assert_eq!(progress, 1.0);
    spring = GalleryMotionState::source(1.0, 0.0, 4000.0, 1.0);
    progress = 1.0;
    for _ in 0..60 {
        (progress, done) = advance_detail(&mut spring, progress, true, 250.0, 1.0 / 120.0);
        if done {
            break;
        }
    }
    assert_eq!(progress, 0.0);
    assert!(
        done && !spring.settled(),
        "external close still releases at Standard det_p0"
    );
}

#[test]
fn p10_shared_toggle_defaults_payload_and_depth_real_frame_shadow_controls() {
    for (mode, shader, back) in [
        (GalleryMode::Slices, false, false),
        (GalleryMode::Geometric, false, true),
        (GalleryMode::Wall, true, false),
        (GalleryMode::Depth, true, true),
    ] {
        let mut app = fixture(mode, 20);
        assert!(app.state.card_flip().animate_shader && app.state.card_flip().animate_back);
        assert_eq!(app.state.card_flip().duration_ms, 1500);
        let _ = update(&mut app, Message::SetDepthShader(shader));
        let _ = update(&mut app, Message::SetDepthBack(back));
        app.start_detail(true, "p10-flags");
        if !shader && !back {
            assert_eq!(app.detail_progress, 1.0);
        } else {
            app.detail_progress = 0.9;
        }
        app.rebuild_depth_frame();
        let p = donor_scene::card_flip_phases(app.detail_progress, shader, back);
        let expected = donor_scene::card_flip_shader_payload(p.shader, app.selected.unwrap(), 0);
        assert!(raw(&app).iter().any(|r| r.flip == expected), "{mode:?}");
        if !shader && !back {
            let _ = update(&mut app, Message::SetDepthEffect("Ash".into()));
            assert_eq!(app.detail_progress, 1.0);
            assert_eq!(app.depth_detail.unwrap().value(), 1.0);
        }
    }
    let mut app = fixture(GalleryMode::Depth, 20);
    assert!(!app.state.gallery.depth.selection_frame && !app.state.gallery.depth.shadows);
    let normal = raw(&app);
    let _ = update(&mut app, Message::SetDepthFrame(true));
    let _ = update(&mut app, Message::SetDepthShadows(true));
    app.rebuild_depth_frame();
    let enabled = raw(&app);
    assert!(enabled.len() > normal.len());
    assert!(enabled
        .iter()
        .any(|r| r.params[1] > 0.0 && r.border[3] > 0.0));
}

fn seed(app: &mut GpuCandidate) {
    let entries = app.ordinary_base_frame();
    let generation = app.catalog_generation.load(Ordering::Acquire);
    app.poster_slots.clear();
    let mut posters = Vec::new();
    for (layer, e) in entries.iter().enumerate() {
        app.poster_slots.insert(
            e.card.index,
            PosterSlot {
                layer: layer as u32,
                kind: crate::poster_worker::PosterKind::Still,
                generation: 0,
                catalog_generation: generation,
                item_id: e.id.clone(),
                source: e.source.clone(),
            },
        );
        posters.push(gpu_gallery::video_placeholder_poster(
            &e.source,
            layer as u32,
        ));
    }
    app.posters = Arc::new(posters);
}

#[test]
fn p10_ordinary_filter_cut_owns_outgoing_images_and_rejects_stale_hidden_frames() {
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
    ] {
        let mut app = fixture(mode, 24);
        app.filtered_indices = (0..8).collect();
        app.presentation_inputs = None;
        app.selected = Some(3);
        app.sync_gallery_presentation();
        if let Some(p) = &mut app.gallery_presentation {
            p.settle();
        }
        if let Some(p) = &mut app.hex_presentation {
            p.settle();
        }
        seed(&mut app);
        let captured = app.capture_ordinary_roll().unwrap().unwrap();
        assert!(!captured.old.is_empty());
        let poster = captured.old[0].pin.poster.clone();
        let original = captured.old[0].entry.captured_raw.clone().unwrap();
        app.filtered_indices = (8..16).collect();
        app.presentation_inputs = None;
        app.selected = Some(10);
        app.sync_gallery_presentation();
        app.ordinary_roll = Some(captured);
        app.reconcile_ordinary_roll();
        let roll = app.ordinary_roll.as_mut().unwrap();
        roll.advance_to(0.5);
        let frame = app.ordinary_frame();
        assert!(frame.iter().any(|e| e.outgoing));
        assert!(frame
            .iter()
            .filter(|e| e.outgoing)
            .all(|e| donor_scene::presented_hit_card(e).is_none()));
        assert!(app
            .gallery_pins()
            .iter()
            .any(|g| g.poster.same_resource(&poster)));
        assert_eq!(
            app.ordinary_roll.as_ref().unwrap().old[0]
                .entry
                .captured_raw
                .as_ref()
                .unwrap()[0]
                .rect,
            original[0].rect
        );
        let generation = app.motion_generation.load(Ordering::Acquire);
        app.stop_gallery_transients("p10-hide");
        assert!(app.ordinary_roll.is_none());
        let _ = update(
            &mut app,
            Message::Wake(Wake::MotionFrame {
                generation,
                value: 1.0,
                velocity: 0.0,
                state: GalleryMotionState::source(1.0, 1.0, 250.0, 1.0),
                selection: HashMap::new(),
                presentation: None,
                filter_wave: Some(0.6),
                ordinary: None,
                hex: None,
                wall: None,
                scene: None,
            }),
        );
        assert!(app.ordinary_roll.is_none());
    }
    let mut raw = donor_scene::InstanceRaw {
        rect: [100.0, 80.0, 40.0, 30.0],
        crop: [0.1, 0.2, 0.6, 0.5],
        ..bytemuck::Zeroable::zeroed()
    };
    donor_scene::roll_raw(&mut raw, 0.25, false);
    assert_eq!(raw.rect, [90.0, 80.0, 30.0, 30.0]);
    assert!((raw.crop[0] - 0.25).abs() < 0.00001);
    assert_eq!(raw.shape[1], 0.0);
}

#[test]
fn p10_wall_unchanged_cells_keep_visible_hits_and_64_plus_64_union_is_pinned() {
    let mut app = fixture(GalleryMode::Wall, 160);
    app.state.gallery.wall.columns = 8;
    app.state.gallery.wall.rows = 8;
    app.state.gallery.wall.card_width = 80.0;
    app.state.gallery.wall.card_height = 60.0;
    app.state.gallery.wall.gap_x = 7.0;
    app.state.gallery.wall.gap_y = 10.0;
    app.state.gallery.wall.selected_scale = 1.0;
    app.filtered_indices = (0..64).collect();
    app.presentation_inputs = None;
    app.sync_gallery_presentation();
    app.wall_presentation.as_mut().unwrap().settle();
    seed(&mut app);
    let old = app.capture_ordinary_roll().unwrap().unwrap();
    assert_eq!(old.old.len(), 64);
    app.ordinary_roll = Some(old.clone());
    app.reconcile_ordinary_roll();
    assert!(app.ordinary_roll.as_ref().unwrap().old.is_empty());
    assert!(app
        .ordinary_frame()
        .iter()
        .all(|e| e.roll == 1.0 && donor_scene::presented_hit_card(e).is_some()));
    app.filtered_indices = (64..128).collect();
    app.presentation_inputs = None;
    app.selected = Some(67);
    app.sync_gallery_presentation();
    app.ordinary_roll = Some(old);
    app.reconcile_ordinary_roll();
    assert_eq!(app.gallery_pins().len(), 64);
    let requested: Vec<_> = app
        .ordinary_base_frame()
        .iter()
        .map(|e| PosterRequestIdentity {
            index: e.card.index,
            item_id: e.id.clone(),
            source: e.source.clone(),
            kind: crate::poster_worker::PosterKind::Still,
        })
        .collect();
    let sources = gallery::poster_sources_bounded(
        &requested,
        &app.poster_slots,
        &app.gallery_pins(),
        app.catalog_generation.load(Ordering::Acquire),
        app.ordinary_union_limit(),
    );
    assert_eq!(sources.len(), 64);
    let layers: HashSet<_> = sources
        .iter()
        .map(|s| s.layer)
        .chain(app.gallery_pins().iter().map(|g| g.slot.layer))
        .collect();
    assert_eq!(
        layers.len(),
        128,
        "incoming cannot overwrite immutable outgoing slots"
    );
}

#[test]
fn p10_transition_refusal_survives_catalog_status_and_ready_then_recovers() {
    let mut app = fixture(GalleryMode::Wall, 8);
    app.ordinary_transition_issue = Some(
        "Filter roll requires too many simultaneous preview images; reduce layout and retry."
            .into(),
    );
    app.update_catalog_status();
    let refusal = app.status.clone();
    let _ = update(&mut app, Message::Wake(Wake::PostersReady));
    assert_eq!(app.status, refusal);
    app.refresh_filter();
    assert!(app.ordinary_transition_issue.is_none());
    assert_ne!(app.status, refusal);
}

#[test]
fn p10_wall_depth_entrance_current_geometry_matches_gpu_and_hits() {
    for mode in [GalleryMode::Wall, GalleryMode::Depth] {
        for launch in [
            donor_scene::LaunchAnimation::Rise,
            donor_scene::LaunchAnimation::Zoom,
        ] {
            let mut app = fixture(mode, 20);
            if mode == GalleryMode::Wall {
                let p = app.wall_presentation.as_mut().unwrap();
                p.launch = launch;
                p.entrance = GalleryMotionState::source(0.5, 0.5, 250.0, 1.0);
            } else {
                let p = app.depth_presentation.as_mut().unwrap();
                p.launch = launch;
                p.entrance = GalleryMotionState::source(0.5, 0.5, 250.0, 1.0);
                app.rebuild_depth_frame();
            }
            let card = if mode == GalleryMode::Wall {
                app.ordinary_frame()
                    .iter()
                    .find_map(donor_scene::presented_hit_card)
                    .unwrap()
            } else {
                app.depth_frame.as_ref().unwrap().cards[0].card
            };
            let instances = raw(&app);
            assert!(
                instances
                    .iter()
                    .any(|r| (r.rect[0] - card.center[0]).abs() < 0.001
                        && (r.rect[1] - card.center[1]).abs() < 0.001
                        && (r.rect[2] * 2.0 - card.size[0]).abs() < 0.001),
                "{mode:?} {launch:?}"
            );
            assert_eq!(
                gpu_gallery::hit_test(&[card], card.center[0], card.center[1]),
                Some(card.index)
            );
            if mode == GalleryMode::Wall {
                app.stop_gallery_transients("entrance-interrupted");
                assert_eq!(
                    app.wall_presentation.as_ref().unwrap().entrance.value(),
                    1.0
                );
            }
        }
    }
}

#[test]
fn p10_external_detail_retains_selected_source_when_current_frame_moves_away() {
    for mode in [GalleryMode::Wall, GalleryMode::Geometric] {
        let mut app = fixture(mode, 100);
        seed(&mut app);
        app.start_detail(true, "capture");
        let source = app.detail_source.unwrap();
        app.detail_progress = 0.74;
        if mode == GalleryMode::Wall {
            let p = app.wall_presentation.as_mut().unwrap();
            p.camera = GalleryMotionState::source(50000.0, 50000.0, 250.0, 1.0);
        } else {
            let p = app.hex_presentation.as_mut().unwrap();
            p.camera = GalleryMotionState::source(50000.0, 50000.0, 250.0, 1.0);
        }
        assert!(!app.ordinary_frame().iter().any(|e| e.card.selected));
        let scene = scene(&app);
        assert!(scene.detail_card.is_some());
        assert_eq!(
            app.ordinary_poster_indices().first(),
            app.selected.as_ref(),
            "off-frame external detail remains first-priority demand"
        );

        let instances =
            donor_scene::presented_instances(&[], &GalleryColors::default(), 1.0, &scene);
        assert_eq!(instances.len(), 3);
        let geometry = donor_detail::external_body(source, app.viewport, app.detail_progress);
        assert_eq!(instances[2].rect[0], geometry.center[0]);
        let snapshot = gpu_gallery::GallerySnapshot {
            ordinary: Some(Vec::new()),
            cards: Vec::new(),
            posters: app.posters.clone(),
            colors: GalleryColors::default(),
            scene,
            depth: None,
            wall_clip: None,
        };
        let resident = snapshot.ordinary_resident_instances(&app.posters, 1.0);
        assert!(resident[2].misc[0] > 0, "accepted fallback texture is real");
        let missing = snapshot.ordinary_resident_instances(&[], 1.0);
        assert_eq!(
            missing[2].misc[0], 0,
            "failed reupload must not sample old layer"
        );

        let element = view::view(&app, window::Id::unique());
        assert!(!iced::advanced::widget::Tree::new(&element)
            .children
            .is_empty());
    }
}

#[test]
fn p10_repeated_filter_cells_share_retained_resources_without_false_overflow() {
    let mut app = fixture(GalleryMode::Wall, 8);
    seed(&mut app);
    let mut roll = app.capture_ordinary_roll().unwrap().unwrap();
    let ghost = roll.old[0].clone();
    roll.old.clear();
    for i in 0..257 {
        let mut g = ghost.clone();
        g.entry.card.center = [i as f32 * 1000.0, 50.0];
        roll.old.push(g);
    }
    app.ordinary_roll = Some(roll);
    let captured = app
        .capture_ordinary_roll()
        .expect("visual cells sharing a poster do not exhaust the atlas")
        .unwrap();
    assert!(captured.old.len() > gpu_gallery::MAX_ORDINARY_POSTERS);
    assert!(
        captured
            .old
            .iter()
            .map(|g| g.pin.slot.layer)
            .collect::<HashSet<_>>()
            .len()
            <= 8
    );
    assert!(app.ordinary_transition_issue.is_none());
}

#[test]
fn p10_depth_reset_preserves_live_filter_frame_while_actual_entry_owns_entrance() {
    let mut app = fixture(GalleryMode::Depth, 8);
    app.state.card_flip_mut().animate_shader = false;
    app.state.card_flip_mut().animate_back = false;
    app.start_detail(true, "both-off");
    app.start_detail(false, "both-off");
    assert!(!app.detail_open && app.depth_detail.is_some());
    app.edit_depth(|d| d.width += 10.0);
    assert!(
        !app.detail_open,
        "closed geometry edits must not resume a finished flip"
    );
    app.depth_presentation = None;
    app.reset_depth_transients();
    app.rebuild_depth_frame();
    assert!(
        !app.depth_frame.as_ref().unwrap().cards.is_empty(),
        "filter/ownership reset is not an entrance"
    );
    app.set_gallery_mode(GalleryMode::Slices);
    app.set_gallery_mode(GalleryMode::Depth);
    assert_eq!(
        app.depth_presentation.unwrap().entrance.value(),
        0.0,
        "actual entry starts source entrance"
    );
    d4a_motion_tick(&mut app, 0.016);
    assert!(!app.depth_frame.as_ref().unwrap().cards.is_empty());
}

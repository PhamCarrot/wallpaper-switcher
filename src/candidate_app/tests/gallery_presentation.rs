use super::*;

#[test]
fn stable_poster_layer_reservation_keeps_priority_and_exact_reuse() {
    let requests: Vec<_> = [2, 0, 1]
        .into_iter()
        .map(|index| PosterRequestIdentity {
            index,
            item_id: format!("item-{index}"),
            source: format!("test-{index}.png").into(),
            kind: poster_worker::PosterKind::Still,
        })
        .collect();
    let slots = requests[1..]
        .iter()
        .map(|request| {
            (
                request.index,
                PosterSlot {
                    layer: request.index as u32,
                    item_id: request.item_id.clone(),
                    source: request.source.clone(),
                    kind: request.kind,
                    generation: 4,
                    last_requested: 4,
                    catalog_generation: 3,
                },
            )
        })
        .collect();
    for (capacity, expected) in [(3, vec![(2, 2), (0, 0), (1, 1)]), (2, vec![(2, 1), (0, 0)])] {
        let sources = gallery::poster_sources_bounded(&requests, &slots, &[], 3, capacity, true);
        assert_eq!(
            sources
                .iter()
                .map(|source| (source.index, source.layer))
                .collect::<Vec<_>>(),
            expected
        );
    }
    let sources = gallery::poster_sources_bounded(&requests, &slots, &[], 4, 3, true);
    assert_eq!(
        sources
            .iter()
            .map(|source| source.layer)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
}

#[test]
fn ordinary_layers_use_empty_space_then_evict_least_recent_unrequested_slot() {
    let request = |index| PosterRequestIdentity {
        index,
        item_id: format!("item-{index}"),
        source: format!("test-{index}.png").into(),
        kind: poster_worker::PosterKind::Still,
    };
    let requests = [21, 22, 12, 23].map(request);
    let slots = [(10, 0, 8), (11, 1, 2), (12, 2, 1)]
        .into_iter()
        .map(|(index, layer, last_requested)| {
            let request = request(index);
            (
                index,
                PosterSlot {
                    layer,
                    kind: request.kind,
                    generation: 1,
                    last_requested,
                    catalog_generation: 3,
                    item_id: request.item_id,
                    source: request.source,
                },
            )
        })
        .collect();
    let sources = gallery::poster_sources_bounded(&requests, &slots, &[], 3, 4, true);
    assert_eq!(
        sources
            .into_iter()
            .map(|source| (source.index, source.layer))
            .collect::<Vec<_>>(),
        [(21, 3), (22, 1), (12, 2), (23, 0)],
    );
}
use crate::candidate_app::gallery_presentation::GalleryPresentation;
use crate::candidate_state::mode_profiles::Snapshot;
use crate::donor_scene::{CardReadiness, SurfacePayload};

fn fixture(count: usize) -> GpuCandidate {
    let (mut app, _) = d4a_fixture(count);
    app.gallery_mode = GalleryMode::Slices;
    app.depth_frame = None;
    app.selected = Some(5.min(count - 1));
    app.state.gallery.slices.spacing = 0.0;
    app.sync_gallery_presentation();
    app.gallery_presentation.as_mut().unwrap().settle();
    app
}

#[test]
fn wall_prefetch_follows_viewport_rows_and_reverses_without_changing_selection() {
    let mut app = fixture(160);
    app.gallery_mode = GalleryMode::Wall;
    app.selected = Some(0);
    let wall = &mut app.state.gallery.wall;
    wall.layout = StateWallLayout::Uniform;
    wall.columns = 7;
    wall.rows = 5;
    wall.card_width = 90.0;
    wall.card_height = 60.0;
    wall.gap_x = 7.0;
    wall.gap_y = 10.0;
    app.sync_gallery_presentation();
    app.wall_presentation
        .as_mut()
        .unwrap()
        .restore(0, Some(630.0));
    app.wall_presentation.as_mut().unwrap().settle();
    app.wall_presentation.as_mut().unwrap().wheel(70.0);
    let (forward, visible) = app.ordinary_poster_demand();
    app.wall_presentation.as_mut().unwrap().wheel(-140.0);
    let (back, back_visible) = app.ordinary_poster_demand();
    assert_eq!(app.selected, Some(0));
    assert_eq!(visible, back_visible);
    assert_eq!(&forward[..visible], &back[..visible]);
    assert_eq!(visible, 35);
    assert_eq!(
        forward[visible..].iter().copied().collect::<HashSet<_>>(),
        (56..63).chain(98..105).collect()
    );
    assert!(forward[visible..visible + 7]
        .iter()
        .all(|i| (98..105).contains(i)));
    assert!(back[visible..visible + 7]
        .iter()
        .all(|i| (56..63).contains(i)));
    assert!(app.ordinary_capacity_issue().is_none());
}

#[test]
fn geometric_circle_keeps_both_edge_columns_outside_visible_decode() {
    let mut app = fixture(90);
    app.gallery_mode = GalleryMode::Geometric;
    app.selected = Some(45);
    let settings = &mut app.state.gallery.geometric;
    settings.shape = StateGeometricShape::Circle;
    settings.rows = 7;
    settings.columns = 10;
    settings.radius = 40.0;
    app.sync_gallery_presentation();
    app.hex_presentation.as_mut().unwrap().settle();
    let (requests, visible) = app.ordinary_poster_demand();
    assert_eq!(visible, 70);
    assert_eq!(
        requests[..visible].iter().copied().collect::<HashSet<_>>(),
        (7..77).collect()
    );
    assert_eq!(
        &requests[visible..],
        &(77..84).chain(0..7).collect::<Vec<_>>()
    );
    assert!(app.ordinary_capacity_issue().is_none());
}

#[test]
fn geometric_negative_pitch_prefetches_the_travel_side_first() {
    let mut app = fixture(180);
    app.gallery_mode = GalleryMode::Geometric;
    app.selected = Some(90);
    let settings = &mut app.state.gallery.geometric;
    settings.shape = StateGeometricShape::Hexagon;
    settings.curve = StateGeometricCurve::Flat;
    settings.rows = 3;
    settings.columns = 3;
    settings.radius = 40.0;
    settings.aspect = 1.0;
    settings.gap_x = -100.0;
    app.sync_gallery_presentation();
    let hex = app.hex_presentation.as_mut().unwrap();
    hex.settle();
    assert!(hex.params.step_x() < 0.0);
    let current = hex.camera.value();
    hex.camera.retarget_source(current - 40.0);
    let left = hex.prefetch_indices();
    hex.camera.retarget_source(current + 40.0);
    let right = hex.prefetch_indices();
    assert_eq!(left.len(), 6);
    assert_eq!(right.len(), 6);
    assert_eq!(&left[..3], &right[3..]);
    assert_eq!(&right[..3], &left[3..]);
    assert!(left[0] > right[0]);
}

#[test]
fn gallery_settings_changed_restarts_motion_for_ordinary_modes() {
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
    ] {
        let mut app = fixture(30);
        app.gallery_mode = mode;
        let previous = app.state.gallery.clone();
        match mode {
            GalleryMode::Slices => app.state.gallery.slices.spacing += 1.0,
            GalleryMode::Geometric => app.state.gallery.geometric.scatter += 1.0,
            GalleryMode::Wall => app.state.gallery.wall.scatter += 1.0,
            GalleryMode::Depth => unreachable!(),
        }
        let generation = app.motion_generation.load(Ordering::Acquire);
        app.gallery_settings_changed(&previous);
        assert!(
            app.motion_generation.load(Ordering::Acquire) > generation,
            "{mode:?} settings changes restart the presentation motion"
        );
    }
}

#[test]
fn p7_source_springs_match_independent_substeps_and_retain_reversal_velocity() {
    for wobble in [false, true] {
        let mut app = fixture(30);
        app.state.gallery.slices.wobble = wobble;
        app.gallery_presentation = None;
        app.sync_gallery_presentation();
        app.gallery_presentation.as_mut().unwrap().settle();
        app.selected = Some(6);
        app.sync_gallery_presentation();
        let p = app.gallery_presentation.as_mut().unwrap();
        let old = p.ids[5].clone();
        let new = p.ids[6].clone();
        assert_eq!(p.widths[&old].value(), 924.0);
        assert_eq!(p.widths[&new].value(), 135.0);
        let mut x = 135.0f32;
        let mut v = 0.0;
        let omega = 6.64 / 0.250;
        let zeta = if wobble { 0.5 } else { 1.0 };
        for frame in 0..16 {
            if frame > 0 {
                for _ in 0..4 {
                    v += (-omega * omega * (x - 924.0) - 2.0 * omega * zeta * v) / 240.0;
                    x += v / 240.0;
                }
                p.tick(1.0 / 60.0);
            }
            assert!(
                (p.widths[&new].x - x).abs() < 0.002,
                "wobble={wobble} frame={frame}"
            );
        }
        let before = p.widths[&new];
        let camera = p.camera;
        p.sync(
            p.ids.to_vec(),
            old.clone(),
            p.target.clone(),
            p.viewport,
            p.duration,
        );
        assert_eq!(
            (p.widths[&new].x, p.widths[&new].velocity),
            (before.x, before.velocity)
        );
        assert_eq!((p.camera.x, p.camera.velocity), (camera.x, camera.velocity));
        assert_eq!(p.widths[&old].target, 924.0);
        for _ in 0..120 {
            p.tick(1.0 / 60.0);
        }
        assert_eq!(p.widths[&old].value(), 924.0);
        assert!(!p.widths.contains_key(&new));
    }
}

#[test]
fn p7_current_frame_wobble_corners_crop_detail_and_demand_agree() {
    let mut app = fixture(60);
    app.state.gallery.slices.wobble = true;
    app.state.gallery.slices.parallax = Some(true);
    app.state.gallery.slices.round_corners = Some(true);
    app.state.gallery.slices.configured_corners = Some([4.0, 8.0, 12.0, 16.0]);
    app.gallery_presentation = None;
    app.sync_gallery_presentation();
    app.gallery_presentation.as_mut().unwrap().settle();
    app.selected = Some(9);
    app.sync_gallery_presentation();
    app.gallery_presentation.as_mut().unwrap().tick(0.05);
    let frame = app.ordinary_frame();
    let scene = donor_scene::DonorSceneState {
        stage: Default::default(),
        camera: 0.0,
        camera_target: 0.0,
        camera_velocity: 0.0,
        detail_progress: 0.0,
        external_detail: false,
        flip_options: Default::default(),
        detail_source: None,
        detail_card: None,
        viewport: app.viewport,
        time: 0.0,
        visibility: 1.0,
    };
    let raw = donor_scene::presented_instances(&frame, &GalleryColors::default(), 1.0, &scene);
    assert_eq!(raw.len(), frame.len() * 2);
    for (entry, pair) in frame.iter().zip(raw.chunks_exact(2)) {
        assert_eq!(entry.id, app.wallpapers[entry.card.index].id);
        assert_eq!(pair[1].radii, entry.radii);
        assert_eq!(pair[1].misc[3] & 32, 32);
        assert_eq!(pair[1].misc[3] & 2, 0);
        let SurfacePayload::Wobble {
            pad,
            bend,
            parallax,
        } = entry.payload
        else {
            panic!()
        };
        assert!((pad - 1.35).abs() < 0.0001);
        assert_eq!(pair[1].flip, [0.0, parallax, pad, bend]);
        assert!(bend.abs() > 0.001);
        assert_eq!(pair[1].rect[2], entry.card.size[0] * 0.5 * pad);
        assert_eq!(pair[0].misc[0], 0);
        assert_eq!(pair[0].flip, [0.0; 4]);
    }
    let demand = app.ordinary_poster_indices();
    for entry in &frame {
        assert!(demand.contains(&entry.card.index));
    }
    let selected = frame.iter().find(|e| e.card.selected).unwrap();
    assert_eq!(
        gpu_gallery::hit_test(
            &frame.iter().map(|e| e.card).collect::<Vec<_>>(),
            selected.card.center[0],
            selected.card.center[1]
        ),
        Some(selected.card.index)
    );
    app.detail_open = true;
    app.detail_progress = 0.5;
    let detail = app.ordinary_frame();
    assert!(matches!(
        detail.iter().find(|e| e.card.selected).unwrap().payload,
        SurfacePayload::Detail
    ));
    let raw = donor_scene::presented_instances(
        &detail,
        &GalleryColors::default(),
        1.0,
        &donor_scene::DonorSceneState {
            detail_progress: 0.5,
            ..scene
        },
    );
    assert_eq!(raw.last().unwrap().misc[3] & 32, 0);
    assert!(raw.last().unwrap().flip[0] > 0.0);
}

#[test]
fn p7_flag2_depends_on_loading_readiness_and_never_selection() {
    let app = fixture(12);
    let mut entry = app
        .ordinary_frame()
        .into_iter()
        .find(|e| e.card.selected)
        .unwrap();
    entry.shadows = false;
    let scene = donor_scene::DonorSceneState {
        stage: Default::default(),
        camera: 0.0,
        camera_target: 0.0,
        camera_velocity: 0.0,
        detail_progress: 0.0,
        external_detail: false,
        flip_options: Default::default(),
        detail_source: None,
        detail_card: None,
        viewport: app.viewport,
        time: 0.0,
        visibility: 1.0,
    };
    for eligible in [false, true] {
        for readiness in [
            CardReadiness::Ready,
            CardReadiness::Loading,
            CardReadiness::Unavailable,
        ] {
            for selected in [false, true] {
                entry.loading_eligible = eligible;
                entry.readiness = readiness;
                entry.card.selected = selected;
                let raw = donor_scene::presented_instances(
                    &[entry.clone()],
                    &GalleryColors::default(),
                    1.0,
                    &scene,
                );
                assert_eq!(
                    raw[0].misc[3] & 2,
                    if eligible && readiness == CardReadiness::Loading {
                        2
                    } else {
                        0
                    }
                );
            }
        }
    }
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
    ] {
        let mut current = fixture(12);
        current.gallery_mode = mode;
        current.poster_loading = true;
        current.poster_request_key = Some(
            current
                .filtered_indices
                .iter()
                .map(|i| PosterRequestIdentity {
                    index: *i,
                    item_id: current.wallpapers[*i].id.clone(),
                    source: poster_source_for_item(&current.wallpapers[*i]),
                    kind: crate::poster_worker::PosterKind::Still,
                })
                .collect(),
        );
        let frame = current.ordinary_frame();
        assert!(frame.iter().all(|e| e.loading_eligible));
        assert!(frame.iter().all(|e| e.readiness == CardReadiness::Loading));
        current.poster_loading = false;
        assert!(current
            .ordinary_frame()
            .iter()
            .all(|e| e.readiness == CardReadiness::Unavailable));
    }
}

#[test]
fn p32_failed_card_becomes_unavailable_before_healthy_neighbour_settles() {
    let mut app = fixture(3);
    app.gallery_mode = GalleryMode::Wall;
    app.state.gallery.mode = GalleryKind::Wall;
    app.viewport = [1920.0, 1080.0];
    app.state.gallery.wall.columns = 3;
    app.state.gallery.wall.rows = 1;
    app.sync_gallery_presentation();
    app.wall_presentation.as_mut().unwrap().settle();
    app.poster_slots.clear();
    app.posters = Arc::new(Vec::new());
    let sources: Vec<_> = app
        .wallpapers
        .iter()
        .enumerate()
        .map(|(index, item)| poster_worker::PosterSource {
            index,
            layer: index as u32,
            item_id: item.id.clone(),
            path: poster_source_for_item(item),
            kind: crate::poster_worker::PosterKind::Still,
            video_stamp: None,
            still: None,
        })
        .collect();
    let generation = app.poster_generation.fetch_add(1, Ordering::AcqRel) + 1;
    app.poster_mailbox
        .expect(generation, sources.clone())
        .unwrap();
    app.poster_request_key = Some(
        sources
            .iter()
            .map(|source| PosterRequestIdentity {
                index: source.index,
                item_id: source.item_id.clone(),
                source: source.path.clone(),
                kind: source.kind,
            })
            .collect(),
    );
    app.poster_loading = true;
    let readiness = |app: &GpuCandidate, source: &poster_worker::PosterSource| {
        app.ordinary_frame()
            .into_iter()
            .find(|entry| entry.id == source.item_id)
            .unwrap()
            .readiness
    };
    let failed = |generation, source| poster_worker::PosterCompletion {
        generation,
        cursor: 1,
        finished: false,
        posters: Vec::new(),
        failures: vec![poster_worker::DecodeFailed {
            source,
            tiers: [gpu_gallery::PosterTier::Far, gpu_gallery::PosterTier::Near],
            error: Arc::new(gpu_gallery::PosterDecodeError::InvalidPixels),
        }],
    };
    let first = &sources[0];
    let healthy = &sources[1];
    assert_eq!(readiness(&app, first), CardReadiness::Loading);
    let mut wrong = first.clone();
    wrong.item_id.push_str("-different");
    for stale in [
        failed(generation - 1, first.clone()),
        failed(generation, wrong),
    ] {
        app.poster_mailbox
            .publish(stale, || panic!("stale failure woke the gallery"))
            .unwrap();
    }
    assert!(app.poster_mailbox.take().unwrap().is_none());
    app.poster_mailbox
        .publish(failed(generation, first.clone()), || true)
        .unwrap();
    let _ = update(&mut app, Message::Wake(Wake::PostersReady));
    assert!(app.poster_loading);
    assert_eq!(readiness(&app, first), CardReadiness::Unavailable);
    assert_eq!(readiness(&app, healthy), CardReadiness::Loading);

    app.poster_mailbox
        .publish(
            poster_worker::PosterCompletion {
                generation,
                cursor: 2,
                finished: false,
                posters: vec![poster_worker::DecodedPoster {
                    source: healthy.clone(),
                    poster: gpu_gallery::video_placeholder_poster(&healthy.path, healthy.layer),
                }],
                failures: Vec::new(),
            },
            || true,
        )
        .unwrap();
    app.poster_mailbox
        .publish(
            poster_worker::PosterCompletion {
                generation,
                cursor: 3,
                finished: true,
                posters: Vec::new(),
                failures: Vec::new(),
            },
            || panic!("the healthy result and terminal cursor should share a wake"),
        )
        .unwrap();
    let _ = update(&mut app, Message::Wake(Wake::PostersReady));
    assert!(!app.poster_loading);
    assert!(!app.poster_failure);
    assert_eq!(readiness(&app, first), CardReadiness::Unavailable);
    assert_eq!(readiness(&app, healthy), CardReadiness::Ready);
    app.invalidate_poster_work();
    assert!(app.poster_failed_indices.is_empty());
}

#[test]
fn p7_source_regular_compact_fixtures_and_live_controls_preserve_profile_values() {
    for (viewport, width, expanded, height, count, skew) in [
        ([1920.0, 1080.0], 135.0, 924.0, 520.0, 12, 35.0),
        ([1024.0, 768.0], 90.0, 600.0, 360.0, 8, 25.0),
    ] {
        let mut app = fixture(24);
        let Snapshot::Slices(mut p) = view::settings::profile_projection::capture(&app) else {
            panic!()
        };
        p.width = width;
        p.expanded_width = expanded;
        p.height = height;
        p.visible_count = count;
        p.skew_distance = skew;
        p.spacing = 0.0;
        let state = GalleryPresentation::new(
            (0..24).map(|i| i.to_string()).collect::<Vec<_>>(),
            "5".into(),
            p.clone(),
            viewport,
            250.0,
        );
        assert_eq!(state.slices, p);
        app.viewport = viewport;
        // Shader padding and position zoom never escape the decoded image content.
        for aspect in [0.5625, 1.777778] {
            let crop = donor_scene::poster_crop_uv(aspect, [expanded, height]);
            let moved =
                crate::candidate_app::gallery_presentation::position_crop(aspect, crop, 1.0, 1.4);
            let content = if aspect > 1.0 {
                [0.0, (1.0 - 1.0 / aspect) * 0.5, 1.0, 1.0 / aspect]
            } else {
                [(1.0 - aspect) * 0.5, 0.0, aspect, 1.0]
            };
            assert!(moved[0] >= content[0] && moved[1] >= content[1]);
            assert!(moved[0] + moved[2] <= content[0] + content[2] + 0.00001);
            assert!(moved[1] + moved[3] <= content[1] + content[3] + 0.00001);
        }
    }
}

#[test]
fn p7_entrance_uses_source_spring_and_base_hits_survive_shader_only_motion() {
    let mut app = fixture(12);
    app.state.frontend_preferences.launch.animation = Some("rise".into());
    app.restart_gallery_entrance();
    let p = app.gallery_presentation.as_mut().unwrap();
    assert_eq!(p.entrance.value(), 0.0);
    p.tick(0.05);
    let mid = p.entrance.value();
    assert!(mid > 0.0 && mid < 1.0);
    let frame = app.ordinary_frame();
    let scene = donor_scene::DonorSceneState {
        stage: Default::default(),
        camera: 0.0,
        camera_target: 0.0,
        camera_velocity: 0.0,
        detail_progress: 0.0,
        external_detail: false,
        flip_options: Default::default(),
        detail_source: None,
        detail_card: None,
        viewport: app.viewport,
        time: 0.0,
        visibility: 1.0,
    };
    let raw = donor_scene::presented_instances(&frame, &GalleryColors::default(), 1.0, &scene);
    let entry = frame.last().unwrap();
    assert!(
        (raw.last().unwrap().rect[1]
            - entry.card.center[1]
            - (1.0 - mid).powi(2) * app.viewport[1] * 0.03)
            .abs()
            < 0.0001
    );
    assert!((raw.last().unwrap().params[2] - entry.card.opacity * mid).abs() < 0.0001);
    app.stop_gallery_transients("p7-test-hide");
    assert_eq!(
        app.gallery_presentation.as_ref().unwrap().entrance.value(),
        1.0
    );
}

#[test]
fn p7_captured_click_rejects_replaced_catalog_source_and_filtered_identity() {
    let mut app = fixture(12);
    let entry = app
        .ordinary_frame()
        .into_iter()
        .find(|e| e.card.index == 6)
        .unwrap();
    let click = || {
        Message::Gallery(GalleryMessage::PresentedClick {
            index: entry.card.index,
            id: entry.id.clone(),
            source: entry.source.clone(),
            catalog_generation: entry.catalog_generation,
            sequence: None,
        })
    };
    app.catalog_generation.fetch_add(1, Ordering::AcqRel);
    let _ = update(&mut app, click());
    assert_eq!(app.selected, Some(5));
    app.catalog_generation
        .store(entry.catalog_generation, Ordering::Release);
    app.wallpapers[6].preview_path = Some("replacement.png".into());
    let _ = update(&mut app, click());
    assert_eq!(app.selected, Some(5));
    app.wallpapers[6].preview_path = Some(entry.source.clone());
    app.filtered_indices.retain(|i| *i != 6);
    app.presentation_inputs = None;
    let _ = update(&mut app, click());
    assert_eq!(app.selected, Some(5));
    app.filtered_indices.push(6);
    app.presentation_inputs = None;
    let _ = update(&mut app, click());
    assert_eq!(app.selected, Some(6));
}

#[test]
fn p7_source_strength_getter_clamps_without_rewriting_legacy_configuration() {
    let mut app = fixture(24);
    app.state.gallery.slices.wobble = true;
    app.state.gallery.slices.wobble_strength = 5.0;
    app.gallery_presentation = None;
    app.sync_gallery_presentation();
    app.gallery_presentation.as_mut().unwrap().settle();
    assert_eq!(app.state.gallery.slices.wobble_strength, 5.0);
    assert_eq!(
        app.gallery_presentation
            .as_ref()
            .unwrap()
            .slices
            .wobble_strength,
        2.0
    );
    let Snapshot::Slices(configured) = view::settings::profile_projection::capture(&app) else {
        panic!()
    };
    assert_eq!(configured.wobble_strength, 5.0);
    let SurfacePayload::Wobble { pad, .. } = app.ordinary_frame().last().unwrap().payload else {
        panic!()
    };
    assert!((pad - 1.65).abs() < 0.0001);
}

#[test]
fn p7_dense_visible_source_frame_requires_every_distinct_image() {
    let mut app = fixture(100);
    app.viewport = [1920.0, 1080.0];
    app.selected = Some(50);
    let p = &mut app.state.gallery.slices;
    p.width = 32.0;
    p.expanded_width = 200.0;
    p.skew_distance = 0.0;
    p.spacing = 0.0;
    p.visible_count = 64;
    app.gallery_presentation = None;
    app.sync_gallery_presentation();
    app.gallery_presentation.as_mut().unwrap().settle();
    let frame = app.ordinary_frame();
    let visible: Vec<_> = frame
        .iter()
        .filter(|e| {
            e.card.center[0] + e.card.size[0] * 0.5 >= 0.0
                && e.card.center[0] - e.card.size[0] * 0.5 <= app.viewport[0]
        })
        .collect();
    assert_eq!(frame.len(), 65);
    assert_eq!(visible.len(), 55);
    let demand = app.ordinary_poster_indices();
    let missing: Vec<_> = visible
        .iter()
        .filter(|e| !demand.contains(&e.card.index))
        .map(|e| e.card.index)
        .collect();
    assert!(
        missing.is_empty(),
        "current visible image identities absent from demand: {missing:?}; requested {} of {}",
        demand.len(),
        visible.len()
    );
    let requests: Vec<_> = demand
        .iter()
        .map(|i| PosterRequestIdentity {
            index: *i,
            item_id: app.wallpapers[*i].id.clone(),
            source: poster_source_for_item(&app.wallpapers[*i]),
            kind: crate::poster_worker::PosterKind::Still,
        })
        .collect();
    let sources = crate::candidate_app::gallery::poster_sources_bounded(
        &requests,
        &app.poster_slots,
        &[],
        app.catalog_generation.load(Ordering::Acquire),
        gpu_gallery::ordinary_poster_limit(),
        true,
    );
    assert_eq!(sources.len(), 65);
    assert_eq!(
        sources
            .iter()
            .map(|s| s.layer)
            .collect::<HashSet<_>>()
            .len(),
        65
    );
    let mailbox = poster_worker::PosterMailbox::default();
    mailbox
        .expect_bounded(7, sources.clone(), gpu_gallery::ordinary_poster_limit())
        .unwrap();
    mailbox
        .publish(
            poster_worker::PosterCompletion {
                generation: 7,
                cursor: 1,
                finished: true,
                failures: Vec::new(),
                posters: sources
                    .iter()
                    .map(|s| poster_worker::DecodedPoster {
                        source: s.clone(),
                        poster: gpu_gallery::video_placeholder_poster(&s.path, s.layer),
                    })
                    .collect(),
            },
            || true,
        )
        .unwrap();
    let completion = mailbox.take().unwrap().unwrap();
    assert_eq!(completion.posters.len(), 65);
    for decoded in completion.posters {
        app.poster_slots.insert(
            decoded.source.index,
            PosterSlot {
                layer: decoded.source.layer,
                kind: crate::poster_worker::PosterKind::Still,
                generation: 7,
                last_requested: 7,
                catalog_generation: app.catalog_generation.load(Ordering::Acquire),
                item_id: decoded.source.item_id,
                source: decoded.source.path,
            },
        );
        Arc::make_mut(&mut app.posters).push(decoded.poster);
    }
    assert!(app
        .ordinary_frame()
        .iter()
        .all(|e| e.card.layer.is_some() && e.readiness == CardReadiness::Ready));
    let legacy = gallery::poster_sources_bounded(
        &requests,
        &app.poster_slots,
        &[],
        app.catalog_generation.load(Ordering::Acquire),
        gpu_gallery::MAX_POSTERS,
        false,
    );
    assert_eq!(legacy.len(), 32, "Depth keeps its current 32-layer budget");
    assert!(app.ordinary_capacity_issue().is_none());
}

#[test]
fn p7_over_limit_layout_reports_resource_gap_without_changing_saved_geometry() {
    let mut app = fixture(400);
    app.viewport = [1920.0, 1080.0];
    app.selected = Some(200);
    let p = &mut app.state.gallery.slices;
    p.width = 1.0;
    p.expanded_width = 10.0;
    p.spacing = 0.0;
    p.skew_distance = 0.0;
    p.visible_count = 256;
    app.gallery_presentation = None;
    app.sync_gallery_presentation();
    app.gallery_presentation.as_mut().unwrap().settle();
    assert!(app.ordinary_frame().len() > 256);
    assert_eq!(app.ordinary_poster_indices().len(), 256);
    assert!(app
        .ordinary_capacity_issue()
        .unwrap()
        .contains("device/memory limit is 256"));
    assert_eq!(app.state.gallery.slices.width, 1.0);
}

#[test]
fn motion_repair_elapsed_clock_counts_work_time_and_caps_long_stalls() {
    use crate::candidate_app::gallery::motion_tick_elapsed;
    use std::time::{Duration, Instant};
    let origin = Instant::now();
    let mut previous = origin;
    assert_eq!(motion_tick_elapsed(&mut previous, origin), 0.0);
    let mut integrated = 0.0_f32;
    for tick in 1..=60 {
        integrated += motion_tick_elapsed(&mut previous, origin + Duration::from_millis(tick * 17));
    }
    assert!((integrated - 1.020).abs() < 0.00001);
    assert_eq!(
        motion_tick_elapsed(&mut previous, origin + Duration::from_secs(2)),
        0.05
    );
    assert!(
        (motion_tick_elapsed(&mut previous, origin + Duration::from_millis(2017)) - 0.017).abs()
            < 0.000001
    );
}

#[test]
fn motion_repair_deadlines_keep_phase_and_skip_missed_ticks_without_bursts() {
    use crate::candidate_app::gallery::motion_tick_deadline;
    use std::time::{Duration, Instant};
    let origin = Instant::now();
    let interval = Duration::from_millis(8);
    let next = origin + interval;
    assert_eq!(
        motion_tick_deadline(next, interval, origin + Duration::from_millis(2)),
        next
    );
    assert_eq!(
        motion_tick_deadline(next, interval, origin + Duration::from_millis(11)),
        origin + Duration::from_millis(16)
    );
    assert_eq!(
        motion_tick_deadline(next, interval, origin + Duration::from_millis(16)),
        origin + Duration::from_millis(24)
    );
    assert_eq!(
        motion_tick_deadline(next, interval, origin + Duration::from_millis(1001)),
        origin + Duration::from_millis(1008)
    );
}

#[test]
fn motion_repair_unchanged_poster_demand_does_not_recapture_discarded_stills() {
    let (mut app, _) = d4a_fixture(4);
    app.poster_worker =
        Some(poster_worker::PosterWorker::spawn(app.poster_generation.clone(), |_| true).unwrap());
    let mut captures = 0;
    app.schedule_posters_with_still_selector(false, |_, _| {
        captures += 1;
        None
    });
    assert!(captures > 0);
    assert!(app.poster_request_key.is_some());
    assert!(app.poster_assignment_key.is_some());
    let generation = app.poster_generation.load(Ordering::Acquire);
    app.schedule_posters_with_still_selector(false, |_, _| {
        panic!("unchanged demand must not recapture still metadata")
    });
    assert_eq!(app.poster_generation.load(Ordering::Acquire), generation);
    app.invalidate_poster_work();
    let mut replacement_captures = 0;
    app.schedule_posters_with_still_selector(false, |_, _| {
        replacement_captures += 1;
        None
    });
    assert!(
        replacement_captures > 0,
        "invalidated work still captures current descriptors"
    );
}

#[test]
fn motion_repair_frame_clones_share_ids_and_navigation_keeps_one_generation() {
    let mut app = fixture(16);
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
    ] {
        app.gallery_mode = mode;
        app.selected = Some(5);
        app.engage_gallery();
        app.sync_gallery_presentation();
        let ids = match mode {
            GalleryMode::Slices => {
                let p = app.gallery_presentation.as_ref().unwrap();
                assert!(Arc::ptr_eq(&p.ids, &p.clone().ids));
                p.ids.clone()
            }
            GalleryMode::Geometric => {
                let p = app.hex_presentation.as_ref().unwrap();
                assert!(Arc::ptr_eq(&p.ids, &p.clone().ids));
                p.ids.clone()
            }
            GalleryMode::Wall => {
                let p = app.wall_presentation.as_ref().unwrap();
                assert!(Arc::ptr_eq(&p.ids, &p.clone().ids));
                p.ids.clone()
            }
            _ => unreachable!(),
        };
        let generation = app.motion_generation.load(Ordering::Acquire);
        app.navigate_selection(-1);
        assert_eq!(
            app.motion_generation.load(Ordering::Acquire),
            generation + 1,
            "stable navigation already syncs before poster scheduling"
        );
        let current = match mode {
            GalleryMode::Slices => &app.gallery_presentation.as_ref().unwrap().ids,
            GalleryMode::Geometric => &app.hex_presentation.as_ref().unwrap().ids,
            GalleryMode::Wall => &app.wall_presentation.as_ref().unwrap().ids,
            _ => unreachable!(),
        };
        assert!(
            Arc::ptr_eq(&ids, current),
            "retargeting must not replace immutable catalog IDs"
        );
    }
}

#[test]
fn poster_navigation_captures_each_visible_card_source_once() {
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
    ] {
        let mut app = fixture(80);
        app.gallery_mode = mode;
        app.selected = Some(40);
        app.sync_gallery_presentation();
        let mut captures = 0;
        let frame = app.ordinary_frame_with_source_selector(|wallpaper| {
            captures += 1;
            gallery::poster_media_for_item(wallpaper)
        });
        assert!(!frame.is_empty());
        assert_eq!(
            captures,
            frame.len(),
            "{mode:?}: one current source per card"
        );
        assert_eq!(
            app.ordinary_capacity_issue(),
            app.ordinary_capacity_issue_for_frame(&frame),
        );
    }
}

#[test]
fn poster_navigation_shared_frame_capacity_counts_distinct_current_cards() {
    let app = fixture(80);
    let frame = app.ordinary_frame();
    let mut excess: Vec<_> = (0..=gpu_gallery::ordinary_poster_limit())
        .map(|index| {
            let mut entry = frame[0].clone();
            entry.card.index = index;
            entry
        })
        .collect();
    assert!(app.ordinary_capacity_issue_for_frame(&excess).is_some());
    excess.pop();
    assert!(app.ordinary_capacity_issue_for_frame(&excess).is_none());
}

#[test]
fn p53_real_held_worker_and_current_receipt_control_dense_entry() {
    use std::sync::{mpsc, Mutex};
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
    let mut app = fixture(3);
    app.clear_posters();
    app.state.gallery.mode = GalleryKind::Slices;
    let wall = &mut app.state.gallery.wall;
    wall.layout = StateWallLayout::Uniform;
    wall.columns = 3;
    wall.rows = 1;
    wall.card_width = 160.0;
    wall.card_height = 110.0;
    let (held_tx, held_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release_rx = Mutex::new(release_rx);
    let (finished_tx, finished_rx) = mpsc::channel();
    let mailbox = app.poster_mailbox.clone();
    let worker = poster_worker::PosterWorker::spawn_with_builder(
        std::thread::Builder::new(),
        app.poster_generation.clone(),
        move |completion| {
            let finished = completion.finished;
            let result = mailbox.publish(completion, || true).is_ok();
            if finished {
                let _ = finished_tx.send(());
            }
            result
        },
        move |source, _| {
            if source.index == 1 {
                held_tx.send(()).unwrap();
                release_rx.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
            }
            Some(gpu_gallery::video_placeholder_poster(
                &source.path,
                source.layer,
            ))
        },
    )
    .unwrap();
    app.poster_worker = Some(worker);
    app.set_gallery_mode(GalleryMode::Wall);
    held_rx.recv_timeout(TIMEOUT).unwrap();
    assert_eq!(app.atlas_entry, Some(GalleryMode::Wall));
    assert!(app.poster_loading);
    let _ = update(&mut app, Message::Wake(Wake::PostersReady));
    let held_key = app.atlas_prewarm_key();
    let held_posters = app.posters.clone();
    let _ = update(
        &mut app,
        Message::Wake(Wake::PosterGpu(gpu_gallery::PosterFeedback::Prewarmed {
            key: held_key,
            posters: held_posters,
        })),
    );
    assert!(app.atlas_entry.is_some());
    release_tx.send(()).unwrap();
    finished_rx.recv_timeout(TIMEOUT).unwrap();
    let _ = update(&mut app, Message::Wake(Wake::PostersReady));
    assert!(!app.poster_loading);
    assert_eq!(app.posters.len(), 3);
    assert!(app.atlas_entry.is_some());
    let key = app.atlas_prewarm_key();
    let posters = app.posters.clone();
    let mut wrong = key;
    wrong.selected = Some(999);
    for (receipt, resources) in [(wrong, posters.clone()), (key, Arc::new(posters.to_vec()))] {
        let _ = update(
            &mut app,
            Message::Wake(Wake::PosterGpu(gpu_gallery::PosterFeedback::Prewarmed {
                key: receipt,
                posters: resources,
            })),
        );
        assert!(app.atlas_entry.is_some());
    }
    let _ = update(
        &mut app,
        Message::Wake(Wake::PosterGpu(gpu_gallery::PosterFeedback::Prewarmed {
            key,
            posters: posters.clone(),
        })),
    );
    assert!(app.atlas_entry.is_none());
    assert!(Arc::ptr_eq(&posters, &app.posters));
    assert_eq!(
        key.generation,
        app.poster_generation.load(Ordering::Acquire)
    );
    drop(app.poster_worker.take());
}

#[test]
fn p53_invalidation_replacement_hide_and_failure_cannot_leave_or_release_an_old_hold() {
    for cancel in 0..3 {
        let mut app = fixture(3);
        app.atlas_entry = Some(GalleryMode::Wall);
        app.gallery_mode = GalleryMode::Wall;
        let key = app.atlas_prewarm_key();
        let posters = app.posters.clone();
        match cancel {
            0 => app.invalidate_poster_work(),
            1 => {
                let _ = app.set_visible(false, "p53-owned-fixture-hide");
            }
            _ => app.clear_posters(),
        }
        assert!(app.atlas_entry.is_none());
        app.atlas_entry = Some(GalleryMode::Geometric);
        app.gallery_mode = GalleryMode::Geometric;
        let _ = update(
            &mut app,
            Message::Wake(Wake::PosterGpu(gpu_gallery::PosterFeedback::Prewarmed {
                key,
                posters,
            })),
        );
        assert_eq!(app.atlas_entry, Some(GalleryMode::Geometric));
    }
    let mut app = fixture(3);
    app.poster_worker = None;
    app.set_gallery_mode(GalleryMode::Wall);
    assert!(app.poster_worker.is_none());
    assert!(app.atlas_entry.is_none());
    assert!(app.poster_failure);
}

#[test]
fn p7_profile_categorical_retarget_keeps_slice_identity_and_reverses() {
    let mut app = fixture(48);
    let before = view::settings::profile_projection::capture(&app);
    app.state.gallery.slices.visible_count = 5;
    app.state.gallery.slices.wobble = true;
    app.state.gallery.slices.parallax = Some(true);
    app.state.gallery.slices.round_corners = Some(true);
    app.state.gallery.slices.width = 240.0;
    app.sync_gallery_presentation();
    let frame_before = app.ordinary_frame();
    let ids_before: std::collections::BTreeSet<_> =
        frame_before.iter().map(|entry| entry.id.clone()).collect();
    app.gallery_presentation.as_mut().unwrap().tick(0.05);
    let mid = app.ordinary_frame();
    let ids_mid: std::collections::BTreeSet<_> = mid.iter().map(|entry| entry.id.clone()).collect();
    assert_eq!(ids_mid, ids_before);
    assert!(mid.iter().all(|entry| {
        entry.card.center.iter().all(|value| value.is_finite())
            && entry
                .card
                .size
                .iter()
                .all(|value| value.is_finite() && *value > 0.0)
    }));
    let demand = app.ordinary_poster_indices();
    assert!(mid.iter().all(|entry| demand.contains(&entry.card.index)));

    let Snapshot::Slices(original) = before else {
        panic!("slices snapshot")
    };
    app.state.gallery.slices.visible_count = original.visible_count as usize;
    app.state.gallery.slices.wobble = original.wobble;
    app.state.gallery.slices.parallax = Some(original.parallax);
    app.state.gallery.slices.round_corners = Some(original.round_corners);
    app.state.gallery.slices.width = original.width;
    app.sync_gallery_presentation();
    app.gallery_presentation.as_mut().unwrap().tick(0.05);
    app.gallery_presentation.as_mut().unwrap().settle();
    assert_eq!(app.gallery_presentation.as_ref().unwrap().slices, original);
}

#[test]
fn depth_position_uses_the_shifted_card_for_hits_and_poster_demand() {
    use view::settings::composed::{
        draft::Value,
        keys::selector::{DEPTH_STAGE_X, DEPTH_STAGE_Y},
    };
    let (mut app, calls) = d3_application_fixture(7);
    app.deterministic_animation = true;
    app.browse_position_restored = true;
    app.selected = Some(3);
    app.reset_depth_transients();
    app.state.frontend_preferences.launch.animation = Some("none".into());
    let selected = |app: &GpuCandidate| {
        app.depth_frame
            .as_ref()
            .unwrap()
            .cards
            .iter()
            .find(|entry| entry.card.selected)
            .unwrap()
            .clone()
    };

    for (viewport, inset) in [([1440.0, 900.0], 0.0), ([1060.0, 760.0], 350.0)] {
        app.viewport = viewport;
        app.scene_motion.retarget(true, inset, 0.0, 0.0);
        app.state.gallery.depth.offset_x = 0.0;
        app.state.gallery.depth.offset_y = 0.0;
        app.depth_presentation = Some(DepthPresentation::target(&app.state));
        app.restart_gallery_entrance();
        app.rebuild_depth_frame();
        let base = selected(&app);
        for (x, y) in [(-20.0, 15.0), (20.0, -15.0)] {
            let previous = app.state.gallery.clone();
            assert!(view::settings::geometry_projection::write(
                &mut app.state,
                DEPTH_STAGE_X,
                &Value::Number(x),
            )
            .unwrap());
            assert!(view::settings::geometry_projection::write(
                &mut app.state,
                DEPTH_STAGE_Y,
                &Value::Number(y),
            )
            .unwrap());
            app.gallery_settings_changed(&previous);
            let target = DepthPresentation::target(&app.state);
            let current = app.depth_presentation.as_mut().unwrap();
            assert!(current.unsettled(target));
            for _ in 0..80 {
                current.tick(target, 0.016);
            }
            assert!(!current.unsettled(target));
            app.rebuild_depth_frame();
            let shifted = selected(&app);
            assert_eq!(base.id, shifted.id);
            let [cx, cy] = shifted.card.center;
            assert!((cx - base.card.center[0] - viewport[0] * x as f32 / 200.0).abs() < 0.01);
            assert!((cy - base.card.center[1] - viewport[1] * y as f32 / 200.0).abs() < 0.01);
            let snapshot = app.depth_frame.as_ref().unwrap();
            assert_eq!(snapshot.hit(cx, cy).unwrap().id, shifted.id);
            assert!(snapshot
                .live_poster_indices
                .contains(&shifted.filtered_index));
        }
    }
    let wallpaper = &app.wallpapers[app.selected.unwrap()];
    let (source, kind) = super::super::gallery::poster_media_for_item(wallpaper);
    app.poster_slots.insert(
        app.selected.unwrap(),
        PosterSlot {
            layer: 0,
            kind,
            generation: app.poster_generation.load(Ordering::Acquire),
            last_requested: 0,
            catalog_generation: app.catalog_generation.load(Ordering::Acquire),
            item_id: wallpaper.id.clone(),
            source: source.clone(),
        },
    );
    Arc::make_mut(&mut app.posters).push(gpu_gallery::video_placeholder_poster(&source, 0));
    app.rebuild_depth_frame();
    let shifted = selected(&app);
    app.start_detail(true, "depth-position-check");
    assert_eq!(selected(&app).card.center, shifted.card.center);
    assert_eq!(app.depth_detail_id.as_deref(), Some(shifted.id.as_str()));
    app.cancel_detail("depth-position-check");
    app.save_gallery_preset().unwrap();
    let id = app
        .state
        .mode_profiles
        .selected_id(GalleryKind::Depth)
        .unwrap();
    app.state = CandidateState::load(&app.state_path);
    let Snapshot::Depth(preset) = app
        .state
        .mode_profiles
        .snapshot(GalleryKind::Depth, id)
        .unwrap()
    else {
        panic!("Depth preset expected");
    };
    assert_eq!([preset.offset_x, preset.offset_y], [0.2, -0.15]);
    let old_text = toml::to_string(&preset)
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with("offset_"))
        .collect::<Vec<_>>()
        .join("\n");
    let legacy: candidate_state::mode_profiles::DepthProfileSnapshot =
        toml::from_str(&old_text).unwrap();
    assert_eq!([legacy.offset_x, legacy.offset_y], [0.0; 2]);
    app.state.gallery.depth.offset_x = -0.1;
    app.state.gallery.depth.offset_y = 0.2;
    app.apply_gallery_preset(GalleryKind::Depth, id).1.unwrap();
    assert_eq!(
        [
            app.state.gallery.depth.offset_x,
            app.state.gallery.depth.offset_y
        ],
        [0.2, -0.15]
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

#[test]
fn image_parallax_crop_reverses_from_the_current_frame_in_both_pickers() {
    for mode in [GalleryMode::Slices, GalleryMode::Geometric] {
        let mut app = fixture(12);
        app.gallery_mode = mode;
        app.state.gallery.mode = super::super::gallery::gallery_kind(mode);
        let selected = app.selected.unwrap();
        let wallpaper = &app.wallpapers[selected];
        let source = poster_source_for_item(wallpaper);
        app.poster_slots.insert(
            selected,
            PosterSlot {
                layer: 0,
                kind: super::super::gallery::poster_media_for_item(wallpaper).1,
                generation: app.poster_generation.load(Ordering::Acquire),
                last_requested: 0,
                catalog_generation: app.catalog_generation.load(Ordering::Acquire),
                item_id: wallpaper.id.clone(),
                source: source.clone(),
            },
        );
        Arc::make_mut(&mut app.posters).push(gpu_gallery::video_placeholder_poster(&source, 0));
        let set = |app: &mut GpuCandidate, enabled| {
            app.state.gallery.slices.parallax = Some(enabled);
            app.state.gallery.geometric.parallax = Some(enabled);
            app.sync_gallery_presentation();
        };
        let settle = |app: &mut GpuCandidate| {
            if mode == GalleryMode::Slices {
                app.gallery_presentation.as_mut().unwrap().settle();
            } else {
                app.hex_presentation.as_mut().unwrap().settle();
            }
        };
        let step = |app: &mut GpuCandidate, dt| {
            if mode == GalleryMode::Slices {
                app.gallery_presentation.as_mut().unwrap().tick(dt)
            } else {
                app.hex_presentation.as_mut().unwrap().tick(dt)
            }
        };
        set(&mut app, false);
        settle(&mut app);
        let old = app.ordinary_frame();
        assert_eq!(
            old.iter()
                .find(|entry| entry.card.selected)
                .unwrap()
                .card
                .layer,
            Some(0)
        );
        set(&mut app, true);
        settle(&mut app);
        let enabled = app.ordinary_frame();
        let (off, on) = old
            .iter()
            .filter(|entry| !entry.card.selected)
            .find_map(|off| {
                enabled
                    .iter()
                    .find(|on| on.id == off.id && on.crop != off.crop)
                    .map(|on| (off, on))
            })
            .expect("off-centre cover responds to parallax");
        let current = |app: &GpuCandidate| {
            app.ordinary_frame()
                .into_iter()
                .find(|entry| entry.id == off.id)
                .unwrap()
        };
        set(&mut app, false);
        settle(&mut app);
        set(&mut app, true);
        assert_eq!(current(&app).crop, off.crop);
        assert!(step(&mut app, 0.05));
        let middle = current(&app);
        assert_ne!(middle.crop, off.crop);
        assert_ne!(middle.crop, on.crop);
        assert_eq!(middle.card.center, off.card.center);
        assert_eq!(middle.card.size, off.card.size);
        assert_eq!(middle.card.layer, off.card.layer);
        assert_eq!(middle.source, off.source);
        let middle_center = app
            .ordinary_frame()
            .into_iter()
            .find(|entry| entry.card.selected)
            .unwrap();
        let center = old.iter().find(|entry| entry.card.selected).unwrap();
        assert_eq!(middle_center.card.center, center.card.center);
        assert_eq!(middle_center.card.layer, Some(0));
        let uv_center = |crop: [f32; 4]| [crop[0] + crop[2] * 0.5, crop[1] + crop[3] * 0.5];
        let from = uv_center(center.crop);
        let to = uv_center(middle_center.crop);
        assert!(from.into_iter().zip(to).all(|(a, b)| (a - b).abs() < 1e-5));

        set(&mut app, false);
        assert_eq!(current(&app).crop, middle.crop);
        assert!(step(&mut app, 0.05));
        let reversed = current(&app);
        let distance = |crop: [f32; 4]| {
            crop.iter()
                .zip(off.crop)
                .map(|(a, b)| (a - b).abs())
                .sum::<f32>()
        };
        assert!(distance(reversed.crop) < distance(middle.crop));
        settle(&mut app);
        assert_eq!(current(&app).crop, off.crop);

        set(&mut app, true);
        settle(&mut app);
        set(&mut app, false);
        assert_eq!(current(&app).crop, on.crop);
        assert!(step(&mut app, 0.05));
        let disable_mid = current(&app).crop;
        assert_ne!(disable_mid, off.crop);
        assert_ne!(disable_mid, on.crop);
        set(&mut app, true);
        assert_eq!(current(&app).crop, disable_mid);

        set(&mut app, false);
        settle(&mut app);
        set(&mut app, true);
        assert!(step(&mut app, 0.003));
        let early = current(&app).crop;
        assert_ne!(early, off.crop);
        set(&mut app, false);
        assert_eq!(
            current(&app).crop,
            early,
            "reverse starts at the first visible crop"
        );

        app.selected = Some(selected + 1);
        app.sync_gallery_presentation();
        assert!(step(&mut app, 0.02));
        let centers = |app: &GpuCandidate| {
            app.ordinary_frame()
                .into_iter()
                .map(|entry| (entry.id, entry.card.center))
                .collect::<Vec<_>>()
        };
        let before_toggle = centers(&app);
        set(&mut app, true);
        assert_eq!(
            centers(&app),
            before_toggle,
            "{mode:?} must keep its live camera"
        );
    }
}

#[test]
#[ignore = "owned WGPU gallery input with a deliberately held worker"]
fn picker_cycle_keeps_current_input_while_decode_and_old_receipt_are_held() {
    use std::sync::{mpsc, Mutex};
    const WAIT: std::time::Duration = std::time::Duration::from_secs(10);
    let (mut app, count) = d4a_fixture(3);
    app.gallery_mode = GalleryMode::Slices;
    app.state.gallery.mode = GalleryKind::Slices;
    app.launch_policy = p1_preview_policy();
    app.selected = Some(0);
    app.browse_position_restored = true;
    app.state.frontend_preferences.launch.animation = Some("rise".into());
    app.state.frontend_preferences.launch.fade_from = Some(20.0);
    app.clear_posters();
    let renderer = super::pk07::renderer();

    let key = |app: &mut GpuCandidate, value: &str, modifiers| {
        let event_window = app.window.unwrap();
        let _ = update(
            app,
            Message::KeyPressed {
                key: keyboard::Key::Character(value.into()),
                modifiers,
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
    let click = |app: &GpuCandidate, index| {
        let frame = app.ordinary_frame();
        let entry = frame
            .iter()
            .find(|entry| entry.card.index == index)
            .unwrap();
        let mut card = entry.card;
        if entry.hex_style {
            donor_scene::launch_card(&mut card, entry.entrance, entry.launch, app.viewport);
        }
        super::pk07::Scene::new(app, &renderer)
            .click(
                &renderer,
                iced::Point::new(card.center[0], card.center[1]),
                iced::mouse::Button::Left,
            )
            .into_iter()
            .find(|message| {
                matches!(message, Message::GalleryInput {
                message: GalleryMessage::Pointer { id: Some(id), .. }, ..
            } if id == &entry.id)
            })
    };

    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let release = Mutex::new(Some(release_rx));
    let mailbox = app.poster_mailbox.clone();
    app.poster_worker = Some(
        poster_worker::PosterWorker::spawn_with_builder(
            std::thread::Builder::new(),
            app.poster_generation.clone(),
            move |completion| mailbox.publish(completion, || true).is_ok(),
            move |source, _| {
                let waiting = release.lock().unwrap().take();
                if let Some(waiting) = waiting {
                    started_tx.send(()).unwrap();
                    let _ = waiting.recv_timeout(WAIT);
                }
                Some(gpu_gallery::video_placeholder_poster(
                    &source.path,
                    source.layer,
                ))
            },
        )
        .unwrap(),
    );

    key(&mut app, "2", keyboard::Modifiers::empty());
    assert_eq!(app.gallery_mode, GalleryMode::Geometric);
    assert_eq!(
        app.atlas_entry,
        Some(GalleryMode::Geometric),
        "numeric entry keeps its gate"
    );
    started_rx.recv_timeout(WAIT).unwrap();
    assert!(
        click(&app, 0).is_none(),
        "explicit hidden target still rejects widget input"
    );
    let stale_receipt = gpu_gallery::PosterFeedback::Prewarmed {
        key: app.atlas_prewarm_key(),
        posters: app.posters.clone(),
    };

    key(&mut app, "m", keyboard::Modifiers::empty());
    assert_eq!(app.gallery_mode, GalleryMode::Wall);
    assert!(app.atlas_entry.is_none());
    assert!(app.poster_loading);
    assert!(
        !app.ordinary_frame().is_empty(),
        "target can draw before the held decode"
    );
    let stale_click = click(&app, 1).expect("current Wall can publish a hit while decode is held");
    key(&mut app, "m", keyboard::Modifiers::SHIFT);
    assert_eq!(app.gallery_mode, GalleryMode::Geometric);
    assert!(app.atlas_entry.is_none());
    assert!(
        click(&app, 0).is_some(),
        "Geometric can hit its displayed card while decode is held"
    );
    key(&mut app, "m", keyboard::Modifiers::empty());
    assert_eq!(app.gallery_mode, GalleryMode::Wall);
    let before = app.gallery_entrance_opacity();
    assert!(
        (before - 0.20).abs() < 0.001,
        "revisit starts the configured visible entrance"
    );
    let current_y = |app: &GpuCandidate| {
        app.ordinary_frame()
            .iter()
            .find(|entry| entry.card.index == 1)
            .expect("the same visible Wall card on each entrance frame")
            .card
            .center[1]
    };
    let before_y = current_y(&app);
    app.wall_presentation.as_mut().unwrap().tick(0.05);
    assert!(app.gallery_entrance_opacity() > before && app.gallery_entrance_opacity() < 1.0);
    assert!(current_y(&app) < before_y, "Rise moves the visible target");
    let _ = update(&mut app, stale_click);
    assert_eq!(
        app.selected,
        Some(0),
        "a prior Wall frame cannot select after a reversal"
    );
    let current = click(&app, 1).expect("current visible Wall hit");
    let _ = update(&mut app, current);
    assert_eq!(app.selected, Some(1));
    assert!(app.poster_loading);
    assert_eq!(count.load(Ordering::Acquire), 0);

    key(&mut app, "2", keyboard::Modifiers::empty());
    app.poster_loading = false;
    let _ = update(&mut app, Message::Wake(Wake::PosterGpu(stale_receipt)));
    assert_eq!(
        app.atlas_entry,
        Some(GalleryMode::Geometric),
        "old receipt cannot release a fresh explicit entry"
    );

    release_tx.send(()).unwrap();
    drop(app.poster_worker.take());
}

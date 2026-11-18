use super::*;
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
fn p7_flag2_depends_on_wall_loading_readiness_and_never_selection() {
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
        assert!(frame
            .iter()
            .all(|e| e.loading_eligible == (mode == GalleryMode::Wall)));
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
    let legacy = depth_poster_sources(
        &requests,
        &app.poster_slots,
        &[],
        app.catalog_generation.load(Ordering::Acquire),
    );
    assert_eq!(legacy.len(), 32, "Depth retains the old bounded union");
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
    app.schedule_posters_with_still_selector(|_, _| {
        captures += 1;
        None
    });
    assert!(captures > 0);
    assert!(app.poster_request_key.is_some());
    assert!(app.poster_assignment_key.is_some());
    let generation = app.poster_generation.load(Ordering::Acquire);
    app.schedule_posters_with_still_selector(|_, _| {
        panic!("unchanged demand must not recapture still metadata")
    });
    assert_eq!(app.poster_generation.load(Ordering::Acquire), generation);
    app.invalidate_poster_work();
    let mut replacement_captures = 0;
    app.schedule_posters_with_still_selector(|_, _| {
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
        let frame = app.ordinary_base_frame_with_source_selector(|wallpaper| {
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
fn poster_navigation_shared_frame_capacity_excludes_outgoing_cards() {
    let app = fixture(80);
    let frame = app.ordinary_base_frame();
    let mut excess: Vec<_> = (0..=gpu_gallery::ordinary_poster_limit())
        .map(|index| {
            let mut entry = frame[0].clone();
            entry.card.index = index;
            entry
        })
        .collect();
    assert!(app.ordinary_capacity_issue_for_frame(&excess).is_some());
    for entry in &mut excess {
        entry.outgoing = true;
    }
    assert!(app.ordinary_capacity_issue_for_frame(&excess).is_none());
}

#[test]
fn p53_real_held_worker_and_current_receipt_control_dense_entry() {
    use std::sync::{mpsc, Mutex};
    const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
    let mut app = fixture(3);
    app.clear_posters();
    app.state.gallery.mode = GalleryKind::Slices;
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

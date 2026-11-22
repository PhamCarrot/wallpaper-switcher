use super::*;
use crate::candidate_state::mode_profiles::Snapshot;

#[test]
fn p9_wall_engagement_is_scene_global_and_only_selection_or_restore_engages() {
    let mut app = fixture(40);
    app.state.gallery.wall.selected_scale = 1.5;
    app.sync_wall_presentation();
    app.wall_presentation.as_mut().unwrap().settle();
    let _ = update(
        &mut app,
        Message::Gallery(GalleryMessage::Wheel(-1.0, None)),
    );
    let camera = app.wall_presentation.as_ref().unwrap().camera.target;
    assert!(camera > 0.0);
    assert!(!app.gallery_engaged && !app.wall_presentation.as_ref().unwrap().engaged);
    app.state.frontend_preferences.picker.hover_selects = Some(true);
    let _ = update(&mut app, Message::Gallery(GalleryMessage::Hovered(Some(1))));
    assert_eq!(
        app.selected,
        Some(0),
        "source hover-select applies only to Slices/Hex"
    );
    assert!(!app.gallery_engaged);
    app.navigate_to_edge(false);
    assert!(app.gallery_engaged);
    assert_eq!(
        app.wall_presentation.as_ref().unwrap().camera.target,
        camera,
        "source same-index seek engages without refollowing"
    );
    app.navigate_selection(1);
    assert_eq!(app.wall_presentation.as_ref().unwrap().camera.target, 0.0);
    app.set_gallery_mode(GalleryMode::Geometric);
    app.set_gallery_mode(GalleryMode::Wall);
    assert!(
        app.wall_presentation.as_ref().unwrap().engaged,
        "mode switches retain scene engagement"
    );
    assert!(
        app.wall_presentation.as_ref().unwrap().keyboard,
        "source keyboard-current border survives a mode switch without pointer input"
    );
    assert_eq!(
        app.wall_presentation.as_ref().unwrap().camera.target,
        0.0,
        "Wall mode entry still starts at0"
    );
}

fn fixture(count: usize) -> GpuCandidate {
    let (mut app, _) = d4a_fixture(count);
    app.gallery_mode = GalleryMode::Wall;
    app.state.gallery.mode = GalleryKind::Wall;
    app.depth_frame = None;
    app.browse_position_restored = true;
    app.selected = Some(0);
    app.state.gallery.wall.columns = 4;
    app.state.gallery.wall.rows = 3;
    app.state.gallery.wall.card_width = 90.0;
    app.state.gallery.wall.card_height = 60.0;
    app.state.gallery.wall.gap_x = 7.0;
    app.state.gallery.wall.gap_y = 10.0;
    app.sync_wall_presentation();
    app
}

#[test]
fn p9_wall_entry_restore_keyboard_and_wheel_keep_absolute_camera_and_engagement() {
    let mut app = fixture(40);
    app.state.gallery.wall.selected_scale = 1.5;
    app.sync_wall_presentation();
    app.wall_presentation.as_mut().unwrap().settle();
    assert_eq!(app.wall_presentation.as_ref().unwrap().camera.value(), 0.0);
    assert!(!app.wall_presentation.as_ref().unwrap().engaged);
    assert_eq!(
        app.ordinary_frame()
            .iter()
            .find(|e| e.card.selected)
            .unwrap()
            .card
            .size,
        [90.0, 60.0]
    );
    app.selected = Some(35);
    app.sync_wall_presentation();
    assert_eq!(app.wall_presentation.as_ref().unwrap().camera.target, 0.0);
    assert!(!app.ordinary_frame().iter().any(|e| e.card.selected));
    let p = app.wall_presentation.as_mut().unwrap();
    p.restore(35, None);
    assert_eq!(p.camera.target, 490.0);
    p.restore(35, Some(77.0));
    assert_eq!(p.camera.value(), 77.0);
    p.restore(35, Some(f32::NAN));
    assert_eq!(p.camera.value(), 490.0);
    p.restore(0, Some(0.0));
    p.follow(8);
    assert_eq!(p.camera.target, 10.0); // row follows cell height, not selected center
    p.tick(0.05);
    let before = p.camera;
    p.wheel(-2000.0);
    assert_eq!(p.camera.target, 0.0);
    assert_eq!((p.camera.x, p.camera.velocity), (before.x, before.velocity));
    p.wheel(2000.0);
    assert_eq!(p.camera.target, 490.0);
    p.settle();
    assert_eq!(p.camera.value(), 490.0);
    assert!(p.engaged);
    app.navigate_to_edge(false);
    assert_eq!(app.selected, Some(0));
    assert_eq!(app.wall_presentation.as_ref().unwrap().camera.target, 0.0);
    app.navigate_to_edge(true);
    assert_eq!(app.selected, Some(39));
    assert_eq!(app.wall_presentation.as_ref().unwrap().camera.target, 490.0);
}

#[test]
fn p9_wall_six_layouts_share_current_draw_hit_and_stable_demand_with_aspects_and_rounding() {
    for layout in [
        StateWallLayout::Uniform,
        StateWallLayout::Brick,
        StateWallLayout::Masonry,
        StateWallLayout::Justified,
        StateWallLayout::Editorial,
        StateWallLayout::Cylinder,
    ] {
        let mut app = fixture(40);
        app.filtered_indices.rotate_left(7);
        app.catalog[7].width = 400;
        app.catalog[7].height = 1000;
        app.presentation_inputs = None;
        app.state.gallery.wall.layout = layout;
        app.state.gallery.wall.round_corners = Some(false);
        app.sync_wall_presentation();
        app.wall_presentation.as_mut().unwrap().settle();
        for camera in [0.0, 101.0, 100000.0] {
            let p = app.wall_presentation.as_mut().unwrap();
            p.wheel(camera - p.camera.target);
            p.settle();
            let frame = app.ordinary_frame();
            let requests = app.ordinary_poster_indices();
            let clip =
                gpu_gallery::wall_frame(app.viewport, gallery_layout_params_for_app(&app).wall)
                    .clip;
            assert!(!frame.is_empty(), "{layout:?}");
            assert!(frame
                .iter()
                .all(|e| e.radii == [0.0; 4]
                    && e.card.size.iter().all(|x| x.is_finite() && *x > 0.0)));
            for entry in &frame {
                assert_eq!(app.wallpapers[entry.card.index].id, entry.id);
                assert!(requests.contains(&entry.card.index), "{layout:?}");
                let c = entry.card;
                if c.center[0] >= clip[0]
                    && c.center[0] <= clip[2]
                    && c.center[1] >= clip[1]
                    && c.center[1] <= clip[3]
                {
                    assert!(gpu_gallery::hit_test(
                        &frame.iter().map(|e| e.card).collect::<Vec<_>>(),
                        c.center[0],
                        c.center[1]
                    )
                    .is_some());
                }
            }
            assert!(requests.len() <= gpu_gallery::MAX_POSTERS);
        }
    }
}

#[test]
fn p9_wall_surface_border_clip_and_rounding_are_real_material_consumers() {
    let mut app = fixture(20);
    app.state.gallery.wall.corner_radius = 200.0;
    app.state.gallery.wall.border_width = Some(100.0);
    app.sync_wall_presentation();
    app.wall_presentation.as_mut().unwrap().settle();
    let id = app.wallpapers[0].id.clone();
    let p = app.wall_presentation.as_mut().unwrap();
    p.hover = Some(id);
    p.keyboard = true;
    for _ in 0..60 {
        p.tick(1.0 / 60.0);
    }
    let frame = app.ordinary_frame();
    let raw = donor_scene::presented_instances(
        &frame,
        &gpu_gallery::GalleryColors::default(),
        1.0,
        &donor_scene::DonorSceneState {
            stage: donor_layout::StageParams::default(),
            camera: 0.0,
            camera_target: 0.0,
            camera_velocity: 0.0,
            detail_progress: 0.0,
            external_detail: true,
            flip_options: Default::default(),
            detail_source: None,
            detail_card: None,
            viewport: app.viewport,
            time: 0.0,
            visibility: 1.0,
        },
    );
    assert_eq!(raw.len(), frame.len() * 2);
    for (e, pair) in frame.iter().zip(raw.chunks_exact(2)) {
        assert_eq!(pair[0].misc[2], 1);
        assert_eq!(pair[1].misc[2], 1);
        assert_eq!(pair[0].fill[3], 0.6);
        assert!(pair[1].params[1] <= e.card.size[0].min(e.card.size[1]) * 0.5 - 1.0);
        assert!(e.radii[0] <= e.card.size[0].min(e.card.size[1]) * 0.5);
        if e.card.selected {
            assert!(pair[1].border[3] >= 0.8);
        }
    }
}

#[test]
fn p9_wall_production_rail_orientation_visibility_resize_and_rtl_use_actual_footprint() {
    let mut app = fixture(20);
    app.config.ui.show_top_bar = true;
    for viewport in [[1920.0, 1080.0], [2560.0, 1440.0], [420.0, 240.0]] {
        app.viewport = viewport;
        for vertical in [false, true] {
            app.state.frontend_preferences.filter_bar.orientation =
                Some(if vertical { "vertical" } else { "horizontal" }.into());
            let rail = gallery_rail_model(&app);
            let size = donor_rail::footprint(&rail);
            assert_eq!(rail.vertical, vertical);
            for rtl in [false, true] {
                let mut p = gallery_layout_params_for_app(&app).wall;
                p.rail_rtl = rtl;
                let f = gpu_gallery::wall_frame(viewport, p);
                if vertical {
                    assert_eq!(p.rail_width, Some(size[0]));
                    assert_eq!(p.rail_height, None);
                    assert_eq!(
                        f.center[0],
                        viewport[0] * 0.5
                            + (size[0] + p.rail_gap) * 0.5 * if rtl { -1.0 } else { 1.0 }
                    );
                } else {
                    assert_eq!(p.rail_height, Some(size[1]));
                    assert_eq!(
                        f.center[1],
                        viewport[1] * 0.5 + (size[1] + p.rail_gap) * 0.5
                    );
                }
            }
        }
    }
    app.panel_state.settings_open = true;
    let p = gallery_layout_params_for_app(&app).wall;
    assert_eq!((p.rail_height, p.rail_width), (None, None));
}

#[test]
fn p9_wall_hide_clears_hover_fades_and_real_rail_anchor_tracks_current_morph() {
    let mut app = fixture(40);
    let id = app.wallpapers[0].id.clone();
    let p = app.wall_presentation.as_mut().unwrap();
    p.hover = Some(id.clone());
    for _ in 0..60 {
        p.tick(1.0 / 60.0);
    }
    assert!(p.fades[&id].value() > 0.9);
    app.stop_gallery_transients("p9-reviewed-hide");
    assert!(app.wall_presentation.as_ref().unwrap().fades.is_empty());
    assert!(app.wall_presentation.as_ref().unwrap().hover.is_none());
    app.config.ui.show_top_bar = true;
    app.state.frontend_preferences.interface.ui_scale = Some(2.0);
    app.state.gallery.wall.card_height = 100.0;
    app.sync_wall_presentation();
    app.wall_presentation.as_mut().unwrap().tick(0.05);
    let p = gallery_layout_params_for_app(&app).wall;
    assert!(p.card_height > 60.0 && p.card_height < 100.0);
    let model = gallery_rail_model(&app);
    let size = donor_rail::footprint(&model);
    let anchor = view::gallery_rail_anchor(&app, &model, 999.0);
    let frame = gpu_gallery::wall_frame(app.viewport, p);
    assert_eq!(frame.top - anchor[1] - size[1], 12.0);
    assert!(size[1] >= 48.0);
    app.state.frontend_preferences.filter_bar.orientation = Some("vertical".into());
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Depth,
    ] {
        app.gallery_mode = mode;
        let model = gallery_rail_model(&app);
        let size = donor_rail::footprint(&model);
        let at = view::gallery_rail_anchor(&app, &model, 999.0);
        assert_eq!(at, [16.0, ((app.viewport[1] - size[1]) * 0.5).max(0.0)]);
    }
}

#[test]
fn p9_wall_start_browsing_roundtrip_uses_camera_or_changed_layout_anchor() {
    let mut app = fixture(40);
    app.state.frontend_preferences.picker.start_position = Some("browsing".into());
    app.selected = Some(35);
    app.sync_wall_presentation();
    app.wall_presentation
        .as_mut()
        .unwrap()
        .restore(35, Some(120.0));
    app.save_wall_browse_position();
    let saved = app
        .state
        .frontend_preferences
        .picker
        .browse_position
        .clone()
        .unwrap();
    let loaded = CandidateState::load(&app.state_path);
    assert_eq!(
        loaded.frontend_preferences.picker.browse_position,
        Some(saved.clone())
    );
    app.selected = Some(0);
    app.wall_presentation = None;
    app.browse_position_restored = false;
    app.gallery_engaged = false; // Fresh process for startup restoration.
    app.restore_wall_start_position();
    assert_eq!(app.selected, Some(35));
    assert_eq!(
        app.wall_presentation.as_ref().unwrap().camera.value(),
        120.0
    );
    app.state.gallery.wall.card_height = 65.0;
    app.wall_presentation = None;
    app.browse_position_restored = false;
    app.gallery_engaged = false; // Fresh process for startup restoration.
    app.restore_wall_start_position();
    assert_eq!(
        app.selected
            .and_then(|i| app.wallpapers.get(i))
            .map(|w| w.id.as_str()),
        Some(saved.anchor.as_str())
    );
    assert_ne!(
        app.wall_presentation.as_ref().unwrap().camera.value(),
        120.0
    );
    app.state.frontend_preferences.picker.browse_position = None;
    app.selected = Some(35);
    app.wall_presentation = None;
    app.browse_position_restored = false;
    app.gallery_engaged = false; // Fresh process for startup restoration.
    app.restore_wall_start_position();
    app.sync_wall_presentation();
    assert_eq!(
        app.selected,
        Some(0),
        "missing browsing record keeps source entry selection"
    );
    assert_eq!(app.wall_presentation.as_ref().unwrap().camera.value(), 0.0);
}

#[test]
fn p9_wall_required_dense_layout_requests_every_visible_image_under_approved_bounds() {
    let mut app = fixture(80);
    app.state.gallery.wall.columns = 8;
    app.state.gallery.wall.rows = 8;
    app.state.gallery.wall.card_width = 80.0;
    app.state.gallery.wall.card_height = 60.0;
    app.sync_wall_presentation();
    app.wall_presentation.as_mut().unwrap().settle();
    let visible = app.ordinary_frame().len();
    let requested = app.ordinary_poster_indices().len();
    assert_eq!(visible, 64);
    assert!(requested >= visible && requested <= gpu_gallery::ordinary_poster_limit());
    assert!(app
        .ordinary_frame()
        .iter()
        .all(|e| app.ordinary_poster_indices().contains(&e.card.index)));
    assert!(app.ordinary_capacity_issue().is_none());
    eprintln!("P9 approved capacity: 8x8 Wall has {visible} visible bodies and {requested} bounded poster requests");
}

#[test]
fn p9_wall_border_only_edit_morphs_current_material_and_mid_morph_save_restores_anchor() {
    let mut app = fixture(40);
    app.state.gallery.wall.border_width = Some(12.0);
    assert!(
        app.sync_wall_presentation(),
        "border-only edits must start the existing clock"
    );
    let width = |app: &GpuCandidate| app.ordinary_frame()[0].wall_border.unwrap().1;
    assert_eq!(width(&app), 2.0);
    app.wall_presentation.as_mut().unwrap().tick(0.05);
    assert!(width(&app) > 2.0 && width(&app) < 12.0);
    app.wall_presentation.as_mut().unwrap().settle();
    assert_eq!(width(&app), 12.0);
    app.state.frontend_preferences.picker.start_position = Some("browsing".into());
    app.selected = Some(35);
    app.sync_wall_presentation();
    app.wall_presentation
        .as_mut()
        .unwrap()
        .restore(35, Some(120.0));
    app.state.gallery.wall.card_height = 100.0;
    app.sync_wall_presentation();
    app.wall_presentation.as_mut().unwrap().tick(0.025);
    app.save_wall_browse_position();
    let saved = app
        .state
        .frontend_preferences
        .picker
        .browse_position
        .as_ref()
        .unwrap()
        .clone();
    assert_ne!(
        saved.anchor, saved.selection,
        "fixture needs a visible anchor distinct from offscreen selection"
    );
    app.wall_presentation = None;
    app.browse_position_restored = false;
    app.gallery_engaged = false; // Fresh process for startup restoration.
    app.restore_wall_start_position();
    assert_eq!(
        app.selected
            .and_then(|i| app.wallpapers.get(i))
            .map(|w| w.id.as_str()),
        Some(saved.anchor.as_str()),
        "restart target layout differs from captured intermediate layout"
    );
}

#[test]
fn p9_profile_layout_retarget_keeps_card_identity_and_reverses() {
    let mut app = fixture(40);
    let original = view::settings::profile_projection::capture(&app);
    let original_params = app.wall_presentation.as_ref().unwrap().params;
    app.state.gallery.wall.columns = 2;
    app.state.gallery.wall.rows = 5;
    app.state.gallery.wall.layout = StateWallLayout::Cylinder;
    app.sync_wall_presentation();
    {
        let p = app.wall_presentation.as_ref().unwrap();
        assert_ne!(p.params.columns, p.target.columns);
        assert_ne!(p.params.rows, p.target.rows);
        assert_ne!(p.params.layout, p.target.layout);
    }
    app.wall_presentation.as_mut().unwrap().tick(0.05);
    let frame = app.ordinary_frame();
    let ids: std::collections::BTreeSet<_> = frame.iter().map(|entry| entry.id.clone()).collect();
    assert_eq!(ids.len(), frame.len());
    assert!(frame.iter().all(|entry| {
        entry.card.center.iter().all(|value| value.is_finite())
            && entry
                .card
                .size
                .iter()
                .all(|value| value.is_finite() && *value > 0.0)
    }));
    let selected = frame.iter().find(|entry| entry.card.selected).unwrap();
    assert_eq!(
        gpu_gallery::hit_test(
            &frame.iter().map(|entry| entry.card).collect::<Vec<_>>(),
            selected.card.center[0],
            selected.card.center[1]
        ),
        Some(selected.card.index)
    );
    assert!(app
        .ordinary_poster_indices()
        .iter()
        .all(|index| *index < app.wallpapers.len()));

    let Snapshot::Wall(original) = original else {
        panic!("wall snapshot")
    };
    app.state.gallery.wall.columns = original.columns as usize;
    app.state.gallery.wall.rows = original.rows as usize;
    app.state.gallery.wall.layout = original.layout;
    app.sync_wall_presentation();
    app.wall_presentation.as_mut().unwrap().tick(0.05);
    app.wall_presentation.as_mut().unwrap().settle();
    assert_eq!(
        app.wall_presentation.as_ref().unwrap().params,
        original_params
    );
}

#[test]
fn wall_layout_reverses_after_midpoint_and_reuses_both_indexes_to_settlement() {
    let mut app = fixture(10_000);
    app.wall_presentation.as_mut().unwrap().settle();
    let original = app.state.gallery.wall.columns;
    let original_frame = app.ordinary_frame();
    app.state.gallery.wall.columns = 7;
    app.sync_wall_presentation();
    let built = donor_layout::layout_work_snapshot()[1];
    for _ in 0..3 {
        assert!(app.wall_presentation.as_mut().unwrap().tick(0.05));
        let _ = app.ordinary_frame();
    }
    let before = app.ordinary_frame();
    let composition = gallery_layout_params_for_app(&app).wall;
    let clip = app
        .wall_presentation
        .as_ref()
        .unwrap()
        .clip(app.viewport, composition);
    app.state.gallery.wall.columns = original;
    app.sync_wall_presentation();
    let reversed = app.ordinary_frame();
    let composition = gallery_layout_params_for_app(&app).wall;
    assert_eq!(
        app.wall_presentation
            .as_ref()
            .unwrap()
            .clip(app.viewport, composition),
        clip
    );
    assert_eq!(donor_layout::layout_work_snapshot()[1], built);
    let mut compared = 0;
    for card in before.iter().filter(|c| c.card.opacity > 0.01) {
        let next = reversed.iter().find(|entry| entry.id == card.id).unwrap();
        assert_eq!(
            (next.card.center, next.card.size, next.card.opacity),
            (card.card.center, card.card.size, card.card.opacity)
        );
        assert_eq!(
            (next.source.as_path(), next.readiness),
            (card.source.as_path(), card.readiness)
        );
        compared += 1;
    }
    assert!(compared > 1);
    let mut settled = false;
    for _ in 0..120 {
        if !app.wall_presentation.as_mut().unwrap().tick(1.0 / 60.0) {
            settled = true;
            break;
        }
    }
    assert!(settled);
    let geometry = |frame: Vec<donor_scene::CardPresentation>| {
        frame
            .into_iter()
            .map(|c| (c.id, c.card.center, c.card.size, c.card.opacity))
            .collect::<Vec<_>>()
    };
    assert_eq!(geometry(app.ordinary_frame()), geometry(original_frame));
    assert_eq!(donor_layout::layout_work_snapshot()[1], built);
}

#[test]
fn video_demand_and_draw_do_not_read_source_metadata_per_frame() {
    let mut app = fixture(40);
    app.wall_presentation.as_mut().unwrap().settle();
    for item in &mut app.wallpapers {
        item.source = WallpaperSource::LocalVideo;
    }
    let reads = crate::scanner::VIDEO_STAMP_READS.with(std::cell::Cell::get);
    for _ in 0..100 {
        std::hint::black_box(app.ordinary_frame());
        std::hint::black_box(app.ordinary_poster_demand());
    }
    let additional = crate::scanner::VIDEO_STAMP_READS.with(std::cell::Cell::get) - reads;
    eprintln!(
        "100 repeated video draw/demand frame pairs: {additional} source metadata observations"
    );
    assert_eq!(additional, 0);
}

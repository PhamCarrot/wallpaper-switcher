use super::*;
use crate::candidate_app::hex_presentation::HexPresentation;
use crate::candidate_state::mode_profiles::Snapshot;
use crate::donor_scene::{CardReadiness, SurfacePayload};
use view::settings::composed::SettingsSource;

fn fixture() -> GpuCandidate {
    let (mut app, _) = d4a_fixture(90);
    app.gallery_mode = GalleryMode::Geometric;
    app.depth_frame = None;
    app.viewport = [1920.0, 1080.0];
    app.selected = Some(45);
    app.sync_gallery_presentation();
    app.hex_presentation.as_mut().unwrap().settle();
    app
}

#[test]
fn p8_pointer_camera_retains_target_inside_band_and_keyboard_centers_with_source_bounds() {
    let mut app = fixture();
    let p = app.hex_presentation.as_mut().unwrap();
    p.keyboard = false;
    let origin = p.camera.target;
    let old = p.selected.clone();
    let profile = if let Snapshot::Hex(p) = view::settings::profile_projection::capture(&app) {
        p
    } else {
        panic!()
    };
    let p = app.hex_presentation.as_mut().unwrap();
    p.sync(
        p.ids.to_vec(),
        p.ids[48].clone(),
        profile.clone(),
        p.viewport,
        p.duration,
        0.0,
        false,
    );
    assert_eq!(
        p.camera.target, origin,
        "one pointer column inside the band retains camera"
    );
    p.sync(
        p.ids.to_vec(),
        p.ids[54].clone(),
        profile.clone(),
        p.viewport,
        p.duration,
        0.0,
        false,
    );
    let center = p.params.column_x(18);
    let fade = ((1920.0 - p.params.visible_band()) * 0.5).max(0.0);
    let right = 1920.0 - fade - p.params.step_x();
    assert!(
        (p.camera.target - (960.0 + center - right)).abs() < 0.001,
        "pointer follows only to the edge"
    );
    p.keyboard = true;
    p.sync(
        p.ids.to_vec(),
        old,
        profile.clone(),
        p.viewport,
        p.duration,
        0.0,
        false,
    );
    assert_eq!(p.camera.target, p.params.column_x(15));
    p.sync(
        p.ids.to_vec(),
        p.ids[0].clone(),
        profile.clone(),
        p.viewport,
        p.duration,
        0.0,
        false,
    );
    assert_eq!(p.camera.target, p.params.r);
    p.sync(
        p.ids.to_vec(),
        p.ids[89].clone(),
        profile,
        p.viewport,
        p.duration,
        0.0,
        false,
    );
    assert_eq!(p.camera.target, p.params.column_x(29));
}

#[test]
fn p8_camera_and_column_springs_keep_inflight_geometry_and_source_edge_equation() {
    let mut app = fixture();
    let Snapshot::Hex(profile) = view::settings::profile_projection::capture(&app) else {
        panic!()
    };
    let p = app.hex_presentation.as_mut().unwrap();
    let col = 18;
    let x = 960.0 - p.camera.x + p.params.column_x(col);
    let fade = ((1920.0 - p.params.visible_band()) * 0.5).max(0.0);
    let left = fade;
    let right = 1920.0 - fade;
    let dn = (x - 960.0).abs() / ((right - left) * 0.5);
    let t = ((dn - 0.7) / 0.3).clamp(0.0, 1.0);
    let cut =
        (1.0 - (left - x).max(x - right).max(0.0) / (p.params.step_x() * 1.2)).clamp(0.0, 1.0);
    assert!(
        (p.scale_target(col) - (1.0 - 0.2 * t * t * (3.0 - 2.0 * t)) * cut * cut).abs() < 0.00001
    );
    let x0 = p.camera.x;
    p.sync(
        p.ids.to_vec(),
        p.ids[54].clone(),
        profile.clone(),
        p.viewport,
        p.duration,
        0.0,
        false,
    );
    assert_eq!(p.camera.x, x0);
    let mut x = x0;
    let mut velocity = 0.0;
    let omega = 6.64 / (p.duration / 1000.0);
    let target = p.camera.target;
    for _ in 0..6 {
        for _ in 0..4 {
            velocity += (-omega * omega * (x - target) - 2.0 * omega * velocity) / 240.0;
            x += velocity / 240.0;
        }
        p.tick(1.0 / 60.0);
        assert!((p.camera.x - x).abs() < 0.004);
    }
    let before = p.camera;
    p.sync(
        p.ids.to_vec(),
        p.ids[42].clone(),
        profile,
        p.viewport,
        p.duration,
        0.0,
        false,
    );
    assert_eq!((p.camera.x, p.camera.velocity), (before.x, before.velocity));
    assert!(p.scales.values().any(|s| s.velocity.abs() > 0.001));
    for _ in 0..150 {
        p.tick(1.0 / 60.0);
    }
    assert!(p.camera.settled());
    assert!(p
        .scales
        .values()
        .all(|s| (s.value() - s.target).abs() < 0.05));
}

#[test]
fn p8_all_source_curve_and_shape_choices_preserve_non_square_hits_and_circle_compatibility() {
    let mut app = fixture();
    for curve in geometric_curve_choices().map(|(v, _)| v) {
        app.state.gallery.geometric.curve = curve;
        app.sync_gallery_presentation();
        app.hex_presentation.as_mut().unwrap().settle();
        assert!(!app.ordinary_frame().is_empty());
    }
    for shape in geometric_shape_choices()
        .map(|(v, _)| v)
        .into_iter()
        .chain([StateGeometricShape::Circle])
    {
        app.state.gallery.geometric.shape = shape;
        app.state.gallery.geometric.aspect = 1.7;
        app.sync_gallery_presentation();
        app.hex_presentation.as_mut().unwrap().settle();
        let frame = app.ordinary_frame();
        let card = &frame.iter().find(|e| e.card.selected).unwrap().card;
        assert_eq!(
            gpu_gallery::hit_test(&[*card], card.center[0], card.center[1]),
            Some(card.index)
        );
        assert_eq!(
            gpu_gallery::hit_test(
                &[*card],
                card.center[0] + card.size[0] * 0.49,
                card.center[1] + card.size[1] * 0.49
            ),
            None
        );
        assert!(frame
            .iter()
            .all(|e| e.card.size.into_iter().all(|v| v.is_finite() && v > 0.0)));
        if shape == StateGeometricShape::Circle {
            assert_eq!(card.shape, 1);
            assert!((card.size[1] - 280.0).abs() < 0.001);
        }
    }
    assert_eq!(geometric_shape_choices().len(), 4);
    assert!(!geometric_shape_choices()
        .iter()
        .any(|(s, _)| *s == StateGeometricShape::Circle));
}

#[test]
fn p8_material_order_crops_shadow_position_and_current_demand_agree() {
    let mut app = fixture();
    let before = app
        .ordinary_frame()
        .into_iter()
        .find(|e| e.card.selected)
        .unwrap();
    app.state.gallery.geometric.stage = Some(candidate_state::mode_profiles::StageTransform {
        x: 0.3,
        y: -0.15,
        scale: 1.0,
        rotation_degrees: 0.0,
        perspective: 0.0,
        shear_x: 0.0,
        shear_y: 0.0,
        depth_angle_degrees: 0.0,
    });
    app.state.gallery.geometric.parallax = Some(true);
    app.sync_gallery_presentation();
    app.hex_presentation.as_mut().unwrap().settle();
    let frame = app.ordinary_frame();
    let selected = frame.iter().find(|e| e.card.selected).unwrap();
    assert!((selected.card.center[0] - before.card.center[0] - 288.0).abs() < 0.01);
    assert!((selected.card.center[1] - before.card.center[1] + 81.0).abs() < 0.01);
    let indices = app.ordinary_poster_indices();
    assert!(frame.iter().all(|e| indices.contains(&e.card.index)));
    assert_eq!(
        gpu_gallery::hit_test(
            &frame.iter().map(|e| e.card).collect::<Vec<_>>(),
            selected.card.center[0],
            selected.card.center[1]
        ),
        Some(selected.card.index)
    );
    let scene = DonorSceneState {
        stage: Default::default(),
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
    };
    let raw = donor_scene::presented_instances(&frame, &GalleryColors::default(), 1.0, &scene);
    let last = raw.last().unwrap();
    assert_eq!(last.border[3], 1.0);
    assert_eq!(last.params[1], 3.0);
    assert_eq!(last.shape[2], 1.0);
    assert_eq!(last.misc[3] & 2, 0);
    let shadow = &raw[raw.len() - 2];
    assert_eq!(shadow.misc[0], 0);
    assert_eq!(shadow.misc[3] & 1, 1);
    assert_eq!(shadow.rect[0], last.rect[0] + 3.0);
    assert_eq!(shadow.rect[1], last.rect[1] + 7.0);
    let p = app.hex_presentation.as_mut().unwrap();
    p.hover = Some(p.ids[46].clone());
    let frame = app.ordinary_frame();
    let hover = frame.iter().find(|e| e.hovered).unwrap();
    assert_ne!(hover.id, selected.id);
    assert!(matches!(hover.payload, SurfacePayload::Plain));
    assert_eq!(hover.readiness, CardReadiness::Unavailable);
}

#[test]
fn p8_actual_wheel_and_pointer_keyboard_transitions_follow_source_rows_and_saved_step() {
    let mut app = fixture();
    app.state.gallery.geometric.scroll_step = Some(candidate_state::HexScrollStep::Integer(2));
    let original = app.selected;
    let _ = update(
        &mut app,
        Message::Gallery(GalleryMessage::Wheel(-1.0, None)),
    );
    assert_eq!(app.selected, Some(51));
    let _ = update(
        &mut app,
        Message::Gallery(GalleryMessage::Hovered(Some(52))),
    );
    assert!(!app.hex_keyboard);
    assert_eq!(app.selected, Some(52));
    app.navigate_selection(-1);
    assert!(app.hex_keyboard);
    assert_eq!(app.selected, Some(51));
    assert_ne!(app.selected, original);
    let saved = app.state.gallery.geometric.clone();
    app.stop_gallery_transients("p8-hide");
    assert_eq!(app.state.gallery.geometric, saved);
    let _ = view(&app, app.window.unwrap());
}

#[test]
fn p8_faded_columns_omit_visible_bodies_and_hits_but_preserve_prefetch() {
    let mut app = fixture();
    app.state.gallery.geometric.shape = StateGeometricShape::Triangle;
    app.state.gallery.geometric.curve = StateGeometricCurve::Arc;
    app.state.gallery.geometric.curve_strength = 0.0;
    app.state.gallery.geometric.columns = 3;
    app.sync_gallery_presentation();
    app.hex_presentation.as_mut().unwrap().settle();
    let frame = app.ordinary_frame();
    assert!(
        !frame.iter().any(|e| e.card.index == 57),
        "source skips faded column19"
    );
    // Faded bodies do not consume visible-frame capacity. Existing neighbor
    // prefetch may still use remaining slots, independently of visibility.
    assert!(frame.len() < gpu_gallery::ordinary_poster_limit());
    assert!(app.ordinary_poster_indices().contains(&57));
    let p = app.hex_presentation.as_ref().unwrap();
    assert!(frame
        .iter()
        .all(|e| p.scales[&(e.card.index / p.params.rows)].value() > 0.01));
    assert_eq!(
        gpu_gallery::hit_test(
            &frame.iter().map(|e| e.card).collect::<Vec<_>>(),
            1544.0,
            313.5
        ),
        None
    );
    assert!(frame.iter().all(|e| e.card.size.iter().all(|s| *s > 0.0)));
}

// Uses the I1 calibration bytes in an owned fixture directory; no bootstrap,
// catalogue scan, provider, timers, hooks or cache publication is installed.
fn t1_triangle_fixture() -> (GpuCandidate, Arc<std::sync::atomic::AtomicUsize>) {
    let (mut app, submissions) = d4a_fixture(3);
    app.launch_policy = p1_preview_policy();
    app.gallery_mode = GalleryMode::Geometric;
    app.state.gallery.mode = GalleryKind::Geometric;
    app.state.gallery.variant = None;
    app.depth_frame = None;
    app.viewport = [1920.0, 1080.0];
    app.selected = Some(0);
    app.hex_keyboard = false;
    app.browse_position_restored = true;
    let g = &mut app.state.gallery.geometric;
    g.radius = 190.0;
    g.rows = 1;
    g.columns = 3;
    g.shape = StateGeometricShape::Triangle;
    g.curve = StateGeometricCurve::Flat;
    g.gap_x = 30.0;
    g.gap_y = 30.0;
    g.stagger = 0.0;
    g.parallax = Some(false);
    let media: [(&str, &[u8]); 3] = [
        (
            "I1-A-LANDSCAPE",
            include_bytes!("../../../tests/fixtures/triangle-orientation/I1-A-LANDSCAPE.png"),
        ),
        (
            "I1-B-PORTRAIT",
            include_bytes!("../../../tests/fixtures/triangle-orientation/I1-B-PORTRAIT.png"),
        ),
        (
            "I1-C-SQUARE",
            include_bytes!("../../../tests/fixtures/triangle-orientation/I1-C-SQUARE.png"),
        ),
    ];
    for (i, (name, bytes)) in media.into_iter().enumerate() {
        let path = app.state_path.with_file_name(format!("{name}.png"));
        std::fs::write(&path, bytes).unwrap();
        let w = &mut app.wallpapers[i];
        w.id = format!("t1:{name}");
        w.title = name.into();
        w.file_target = path.clone();
        w.folder_path = path.parent().unwrap().into();
        w.preview_path = Some(path.clone());
        w.project_json_path = path.clone();
        app.poster_slots.insert(
            i,
            PosterSlot {
                layer: i as u32,
                is_video: false,
                generation: 0,
                catalog_generation: app.catalog_generation.load(Ordering::Acquire),
                item_id: w.id.clone(),
                source: path.clone(),
            },
        );
        Arc::make_mut(&mut app.posters).push(gpu_gallery::decode_poster(&path, i as u32).unwrap());
    }
    app.catalog = app
        .wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    app.hex_presentation = None;
    app.sync_gallery_presentation();
    app.hex_presentation.as_mut().unwrap().settle();
    (app, submissions)
}

#[test]
#[ignore = "T1 owned production WGPU orientation pixels; explicit invocation only"]
fn t1_owned_triangle_orientation_pixels() {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout,
    };
    let output =
        std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned output"));
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .expect("headless WGPU");
    let (mut app, submissions) = t1_triangle_fixture();
    for parallax in [false, true] {
        app.state.gallery.geometric.parallax = Some(parallax);
        app.sync_gallery_presentation();
        app.hex_presentation.as_mut().unwrap().settle();
        let frame = app.ordinary_frame();
        assert_eq!(frame.len(), 3);
        assert!(frame.iter().all(|e| e.readiness == CardReadiness::Ready));
        let window = app.window.unwrap();
        let theme = theme(&app, window);
        let style = application_style(&app, &theme);
        let mut element = view(&app, window);
        let size = iced::Size::new(1920.0, 1080.0);
        let bounds = iced::Rectangle::with_size(size);
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        renderer.reset(bounds);
        element.as_widget().draw(
            &tree,
            &mut renderer,
            &theme,
            &iced::advanced::renderer::Style {
                text_color: style.text_color,
            },
            Layout::new(&node),
            iced::mouse::Cursor::Unavailable,
            &bounds,
        );
        let pixels = Headless::screenshot(
            &mut renderer,
            iced::Size::new(1920, 1080),
            1.0,
            style.background_color,
        );
        ::image::save_buffer_with_format(
            output.join(format!("triangle-parallax-{parallax}.png")),
            &pixels,
            1920,
            1080,
            ::image::ColorType::Rgba8,
            ::image::ImageFormat::Png,
        )
        .unwrap();
        // Select clear source-pixel regions through the actual crop, away
        // from arrows/text and the mask edge. Positive parallax changes which
        // source region a screen coordinate sees, so fixed screen samples are
        // not a reliable orientation oracle.
        for entry in &frame {
            let card = entry.card;
            let source = ::image::open(&entry.source).unwrap().to_rgb8();
            let aspect = card.poster_aspect_ratio;
            let content = if aspect >= 1.0 {
                [0.0, (1.0 - 1.0 / aspect) * 0.5, 1.0, 1.0 / aspect]
            } else {
                [(1.0 - aspect) * 0.5, 0.0, aspect, 1.0]
            };
            let baseline = (!parallax && card.index == 1).then(|| {
                ::image::open(std::path::PathBuf::from(
                    std::env::var_os("T1_BASELINE_IMAGE").expect("owned failing baseline image"),
                ))
                .unwrap()
                .to_rgba8()
            });
            let mut samples = 0;
            let mut old_mismatches = 0;
            for ny in [0.1_f32, 0.15, 0.2, 0.25, 0.3] {
                for nx in [0.3_f32, 0.35, 0.4, 0.45, 0.55, 0.6, 0.65, 0.7] {
                    let u = (entry.crop[0] + nx * entry.crop[2] - content[0]) / content[2];
                    let v = (entry.crop[1] + ny * entry.crop[3] - content[1]) / content[3];
                    if !(0.27..0.37).contains(&v) || !(0.1..0.9).contains(&u) {
                        continue;
                    }
                    let x = (card.center[0] + (nx - 0.5) * card.size[0]).round();
                    let y = (card.center[1] + (ny - 0.5) * card.size[1]).round();
                    if [-4.0, 4.0].into_iter().any(|dx| {
                        [-4.0, 4.0].into_iter().any(|dy| {
                            gpu_gallery::hit_test(&[card], x + dx, y + dy) != Some(card.index)
                        })
                    }) {
                        continue;
                    }
                    let expected = source
                        .get_pixel(
                            (u * source.width() as f32) as u32,
                            (v * source.height() as f32) as u32,
                        )
                        .0;
                    if expected != [170, 35, 45] && expected != [30, 130, 65] {
                        continue;
                    }
                    let source_x = (u * source.width() as f32) as u32;
                    let source_y = (v * source.height() as f32) as u32;
                    if [-8_i32, 8].into_iter().any(|dx| {
                        [-8_i32, 8].into_iter().any(|dy| {
                            source
                                .get_pixel(
                                    (source_x as i32 + dx) as u32,
                                    (source_y as i32 + dy) as u32,
                                )
                                .0
                                != expected
                        })
                    }) {
                        continue;
                    }
                    let start = (y as usize * 1920 + x as usize) * 4;
                    let actual = &pixels[start..start + 3];
                    assert!(actual.iter().zip(expected).all(|(a,e)| (*a as i16-e as i16).abs()<16),
                        "upright source-pixel match {} parallax={parallax} at {x},{y}: actual={actual:?} expected={expected:?}", entry.id);
                    if let Some(old) = &baseline {
                        let old = old.get_pixel(x as u32, y as u32).0;
                        old_mismatches += usize::from(
                            old[..3]
                                .iter()
                                .zip(expected)
                                .any(|(a, e)| (*a as i16 - e as i16).abs() >= 16),
                        );
                    }
                    samples += 1;
                }
            }
            assert!(
                samples >= 2,
                "clear upper-quadrant samples for {} parallax={parallax}",
                entry.id
            );
            if baseline.is_some() {
                assert_eq!(
                    old_mismatches, samples,
                    "same pixel oracle rejects the saved old inverted B"
                );
            }
        }
        assert_eq!(submissions.load(Ordering::Acquire), 0);
    }
}

#[test]
fn t1_triangle_direction_agrees_across_frames_materials_hits_and_detail() {
    let (mut app, submissions) = t1_triangle_fixture();
    let scene = DonorSceneState {
        stage: Default::default(),
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
    };
    for parallax in [false, true] {
        app.state.gallery.geometric.parallax = Some(parallax);
        app.sync_gallery_presentation();
        app.hex_presentation.as_mut().unwrap().settle();
        let frame = app.ordinary_frame();
        assert_eq!(frame.len(), 3);
        let demand = app.ordinary_poster_indices();
        for entry in &frame {
            let direction = (entry.card.index % 2) as u8;
            assert_eq!(entry.card.triangle_direction, direction);
            assert_eq!(entry.card.rotation, 0.0);
            assert_eq!(entry.card.layer, Some(entry.card.index as u32));
            assert!(demand.contains(&entry.card.index));
            assert_eq!(entry.id, app.wallpapers[entry.card.index].id);
            let raw = donor_scene::presented_instances(
                &[entry.clone()],
                &GalleryColors::default(),
                1.0,
                &scene,
            );
            for layer in &raw {
                assert_eq!((layer.misc[3] >> 8) & 15, 1 + u32::from(direction));
                assert_eq!(
                    layer.shape[1], 0.0,
                    "body and shadow preserve upright world placement"
                );
            }
            let body = raw.last().unwrap();
            assert_eq!(body.crop, entry.crop);
            assert_eq!(body.uv, [0.0, 0.0, 1.0, 1.0]);
            assert!(entry.crop[2] > 0.0 && entry.crop[3] > 0.0);
            // Actual world rotations still inverse-map correctly. At the
            // upper/lower corner, parity changes the mask, not both UV axes.
            for angle in [0.0_f32, 0.37] {
                let mut card = entry.card;
                card.rotation = angle;
                let (sin, cos) = angle.sin_cos();
                for (y, inside_up) in [(-0.65, false), (0.65, true)] {
                    let local = [card.size[0] * 0.325, card.size[1] * y * 0.5];
                    let world = [
                        card.center[0] + local[0] * cos - local[1] * sin,
                        card.center[1] + local[0] * sin + local[1] * cos,
                    ];
                    let inside = if direction == 0 {
                        inside_up
                    } else {
                        !inside_up
                    };
                    assert_eq!(
                        gpu_gallery::hit_test(&[card], world[0], world[1]),
                        inside.then_some(card.index)
                    );
                }
            }
        }
        let fallback = gpu_gallery::layout_gallery_with_aspects(
            GalleryMode::Geometric,
            3,
            0,
            app.viewport,
            gallery_layout_params_for_app(&app),
            &[16.0 / 9.0, 9.0 / 16.0, 1.0],
        );
        for card in fallback {
            assert_eq!(card.rotation, 0.0);
            assert_eq!(card.triangle_direction, (card.index % 2) as u8);
            let raw = donor_scene::instance_for_card(&card, &GalleryColors::default(), 1.0, 0.0);
            assert_eq!(
                (raw.misc[3] >> 8) & 15,
                1 + u32::from(card.triangle_direction)
            );
        }
    }
    let captured = app.capture_ordinary_roll().unwrap().unwrap();
    let b = captured
        .old
        .iter()
        .find(|g| g.entry.card.index == 1)
        .unwrap();
    assert_eq!(b.entry.card.triangle_direction, 1);
    assert_eq!(b.pin.body.card, b.entry.card);
    let raw = b.entry.captured_raw.as_ref().unwrap();
    assert!(raw
        .iter()
        .all(|r| ((r.misc[3] >> 8) & 15) == 2 && r.shape[1] == 0.0));
    let mut outgoing = b.entry.clone();
    outgoing.roll = 0.5;
    assert!(donor_scene::presented_hit_card(&outgoing).is_none());
    let replay =
        donor_scene::presented_instances(&[outgoing], &GalleryColors::default(), 1.0, &scene);
    assert!(replay
        .iter()
        .all(|r| ((r.misc[3] >> 8) & 15) == 2 && r.shape[1] == 0.0));
    let b = app
        .ordinary_frame()
        .into_iter()
        .find(|e| e.card.index == 1)
        .unwrap();
    let index = gpu_gallery::hit_test(&[b.card], b.card.center[0], b.card.center[1]).unwrap();
    let _ = update(
        &mut app,
        Message::CardClicked {
            index,
            mouse_sequence: None,
        },
    );
    assert_eq!(app.selected, Some(1));
    app.start_detail(true, "t1-hit-selected-B");
    app.detail_progress = 0.94;
    assert_eq!(app.depth_detail_id.as_deref(), Some("t1:I1-B-PORTRAIT"));
    let detail = app.external_detail_card().unwrap();
    assert_eq!(
        (
            detail.index,
            detail.shape,
            detail.triangle_direction,
            detail.rotation
        ),
        (1, 0, 0, 0.0)
    );
    assert_eq!(detail.layer, Some(1));
    app.start_detail(false, "t1-close-B");
    app.detail_progress = 0.0;
    let returned = app
        .ordinary_frame()
        .into_iter()
        .find(|e| e.card.index == 1)
        .unwrap();
    assert_eq!(
        (
            returned.id,
            returned.card.triangle_direction,
            returned.card.rotation
        ),
        ("t1:I1-B-PORTRAIT".into(), 1, 0.0)
    );
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

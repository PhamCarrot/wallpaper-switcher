use super::*;
use canvas::Program;

#[test]
#[ignore = "opt-in CPU stage diagnosis; no window, provider or presented-frame measurement"]
fn detail_open_cpu_diagnostic() {
    use std::time::Instant;
    let mode = std::env::var("DETAIL_DIAGNOSTIC_MODE").expect("explicit mode");
    let title = std::env::var("DETAIL_DIAGNOSTIC_TITLE").expect("explicit title");
    let output = std::env::var("DETAIL_DIAGNOSTIC_OUTPUT").expect("explicit owned report path");
    {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .unwrap();
        fonts.load_font(std::borrow::Cow::Borrowed(include_bytes!(
            "../assets/fonts/RobotoCondensed-Bold-UI.ttf"
        )));
    }
    let mut p = program(mode == "depth", 0.0);
    p.model.font = Font {
        family: iced::font::Family::Name("Roboto Condensed"),
        weight: iced::font::Weight::Bold,
        ..Font::default()
    };
    p.model.title = title.clone();
    if mode == "depth" {
        p.model.card.size = [280.0, 520.0];
        p.model.card.skew_distance = 0.0;
        p.model.card.edge_tilt_distance = 0.0;
        p.model.depth = Some(Default::default());
    } else {
        p.model.card = crate::donor_detail::external_target([1920.0, 1080.0]);
    }
    let mut rows = Vec::new();
    for iteration in 0..12 {
        let started = Instant::now();
        let l = std::hint::black_box(p.layout());
        let layout_us = started.elapsed().as_micros();
        let started = Instant::now();
        for (label, value) in &p.model.fields {
            std::hint::black_box(p.rendered_text(label, l.label_size, l.title.width, false));
            std::hint::black_box(p.rendered_text(value, l.value_size, l.title.width, false));
        }
        std::hint::black_box(p.rendered_text(&p.model.tags, l.source_size, l.title.width, false));
        let fields_us = started.elapsed().as_micros();
        rows.push(serde_json::json!({"iteration":iteration,"layout_us":layout_us,"fields_us":fields_us,"title_lines":l.title_lines}));
    }
    std::fs::write(output, serde_json::to_vec_pretty(&serde_json::json!({
        "mode":mode,"title":title,"method":"actual BackProgram layout and rendered_text, same geometry, no images/GPU; first call then warm repeats", "samples":rows
    })).unwrap()).unwrap();
}
fn program(embedded: bool, rotation: f32) -> BackProgram {
    BackProgram {
        model: BackModel {
            depth: None,
            flip_options: Default::default(),
            opacity: 1.0,
            depth_revision: None,
            identity: "stable-item".into(),
            input_active: true,
            card: CardGeometry {
                center: [500.0, 400.0],
                size: if embedded {
                    [900.0, 520.0]
                } else {
                    [800.0, 600.0]
                },
                rotation,
                skew_distance: if embedded { 72.0 } else { 0.0 },
                edge_tilt_distance: if embedded { 40.0 } else { 0.0 },
            },
            embedded,
            scale: 1.0,
            progress: 1.0,
            radius: 12.0,
            title: "A long supported title".into(),
            kind: "IMAGE".into(),
            fields: vec![
                ("Resolution".into(), "3840 × 2160".into()),
                ("Size".into(), "6.3 MB".into()),
                ("Source".into(), "IMAGE".into()),
            ],
            tags: "Landscape · Forest".into(),
            favorite: false,
            actions: vec![
                Action {
                    intent: DetailIntent::Apply,
                    label: "Apply".into(),
                    enabled: true,
                },
                Action {
                    intent: DetailIntent::Playlists,
                    label: "Playlists".into(),
                    enabled: true,
                },
            ],
            palette: Palette::from_rgb([0.1; 3], [0.2; 3], [0.9; 3], [0.6; 3]),
            font: Font::default(),
            icon_font: Font::default(),
        },
    }
}

#[test]
fn d6_r1_geometry_policy_checks_readable_boundaries_and_full_clipped_rectangles() {
    let mut p = program(true, 0.0);
    p.model.card.skew_distance = 0.0;
    p.model.card.edge_tilt_distance = 0.0;
    p.model.radius = 0.0;
    p.model.card.size = [280.0, 520.0];
    assert!(donor_back_layout::depth_inline_eligible(
        p.model.card,
        0.0,
        3,
        &p.model.actions,
        1.0,
        Font::default(),
        "Title"
    ));
    for (size, skew, radius) in [
        ([40.0, 100.0], 0.0, 0.0),
        ([280.0, 520.0], 200.0, 0.0),
        ([280.0, 520.0], 0.0, 100.0),
        ([620.0, 320.0], 200.0, 100.0),
    ] {
        p.model.card.size = size;
        p.model.card.skew_distance = skew;
        assert!(
            !donor_back_layout::depth_inline_eligible(
                p.model.card,
                radius,
                3,
                &p.model.actions,
                1.0,
                Font::default(),
                "Title"
            ),
            "{size:?}/{skew}/{radius}"
        );
    }
    // Determine the height boundary from actual readable layout metrics,
    // then test both sides without assuming a saved-width threshold.
    p.model.card.size = [280.0, 100.0];
    p.model.card.skew_distance = 0.0;
    let first = (100..521)
        .find(|h| {
            p.model.card.size[1] = *h as f32;
            donor_back_layout::depth_inline_eligible(
                p.model.card,
                0.0,
                3,
                &p.model.actions,
                1.0,
                Font::default(),
                "Title",
            )
        })
        .unwrap();
    p.model.card.size[1] = first as f32 - 1.0;
    assert!(!donor_back_layout::depth_inline_eligible(
        p.model.card,
        0.0,
        3,
        &p.model.actions,
        1.0,
        Font::default(),
        "Title"
    ));
    p.model.card.size[1] = first as f32;
    assert!(donor_back_layout::depth_inline_eligible(
        p.model.card,
        0.0,
        3,
        &p.model.actions,
        1.0,
        Font::default(),
        "Title"
    ));
}

#[test]
fn d6_r1_readable_skew_text_uses_actual_physical_glyph_budget_without_scaling_fonts() {
    let mut p = program(true, 0.0);
    p.model.card.size = [280.0, 520.0];
    p.model.card.skew_distance = 30.0;
    p.model.card.edge_tilt_distance = 0.0;
    p.model.radius = 0.0;
    p.model.depth = Some(Default::default());
    assert!(donor_back_layout::depth_inline_eligible(
        p.model.card,
        0.0,
        3,
        &p.model.actions,
        1.0,
        Font::default(),
        "Title"
    ));
    let l = p.layout();
    assert_eq!(l.content_scale, 1.0);
    let long = "0123456789012345678901234567890123456789";
    for (r, size) in [
        (l.facts[0], 12.0),
        (l.title, l.title_size),
        (l.actions[0], 10.0),
    ] {
        let rendered = p.rendered_text(long, size, r.width, false);
        let left = p.model.card.point([r.x, r.y]);
        let right = p.model.card.point([r.x + r.width, r.y]);
        let physical_width = (right[0] - left[0]).hypot(right[1] - left[1]);
        assert!(donor_back_layout::text_width(&rendered, size) <= physical_width + 0.01);
        assert_ne!(rendered, donor_back_layout::ellipsize(long, size, r.width));
    }
    p.model.depth = None;
    assert_eq!(
        p.rendered_text(long, 12.0, l.facts[0].width, false),
        donor_back_layout::ellipsize(long, 12.0, l.facts[0].width)
    );
    p.model.depth = Some(Default::default());
    p.model.embedded = false;
    p.model.card = crate::donor_detail::external_target([1200.0, 900.0]);
    assert_eq!(
        p.rendered_text(long, 12.0, 119.0, false),
        donor_back_layout::ellipsize(long, 12.0, 119.0)
    );
}

#[test]
fn d6_r1_panel_back_phases_hits_disabled_apply_and_presentation_press_cancellation() {
    for (shader, back) in [(false, false), (true, false), (false, true), (true, true)] {
        let mut p = program(false, 0.0);
        p.model.card = crate::donor_detail::external_target([1200.0, 900.0]);
        p.model.radius = 0.0;
        p.model.depth_revision = Some(1);
        p.model.depth = Some(crate::donor_scene::DepthDetailPresentation {
            effect: 7,
            animate_shader: shader,
            animate_back: back,
        });
        assert_eq!(p.layout().content_scale, 1.0);
        for progress in [0.2, 0.8, 1.0] {
            p.model.progress = progress;
            let phases = crate::donor_scene::card_flip_phases(progress, shader, back);
            assert_eq!(p.phases(), (phases.surface, phases.content));
            let l = p.layout();
            for (r, intent, enabled) in p.controls(&l, 0.0) {
                let hit = p.hit_at(&l, Point::new(r.center_x(), r.center_y()), 0.0);
                assert_eq!(
                    hit,
                    if enabled && phases.content > 0.01 {
                        Some(intent)
                    } else {
                        None
                    }
                );
            }
        }
        p.model.actions[0].enabled = false;
        let l = p.layout();
        let r = l.actions[0];
        assert_eq!(
            p.hit_at(&l, Point::new(r.center_x(), r.center_y()), 0.0),
            None
        );
        let close = l.close;
        let point = Point::new(close.center_x(), close.center_y());
        let mut state = BackState::default();
        event(
            &p,
            &mut state,
            mouse::Event::ButtonPressed(mouse::Button::Left),
            point,
        );
        p.model.embedded = true;
        p.model.depth_revision = Some(2);
        assert_eq!(
            event(
                &p,
                &mut state,
                mouse::Event::ButtonReleased(mouse::Button::Left),
                point
            ),
            None
        );
    }
}
fn event(
    p: &BackProgram,
    s: &mut BackState,
    button: mouse::Event,
    point: Point,
) -> Option<DetailIntent> {
    p.update(
        s,
        &canvas::Event::Mouse(button),
        Rectangle::new(Point::ORIGIN, iced::Size::new(1200.0, 900.0)),
        mouse::Cursor::Available(point),
    )
    .and_then(|a| a.into_inner().0)
}
#[test]
fn transformed_apply_press_release_publishes_exactly_once() {
    for rotation in [-0.3, 0.0, 0.4] {
        let p = program(true, rotation);
        let mut state = BackState::default();
        let r = p.layout().actions[0];
        let a = p.model.card.point([r.center_x(), r.center_y()]);
        let point = Point::new(a[0], a[1]);
        assert_eq!(p.hit_at(&p.layout(), point, 0.0), Some(DetailIntent::Apply));
        assert_eq!(
            event(
                &p,
                &mut state,
                mouse::Event::ButtonPressed(mouse::Button::Left),
                point
            ),
            None
        );
        assert_eq!(
            event(
                &p,
                &mut state,
                mouse::Event::ButtonReleased(mouse::Button::Left),
                point
            ),
            Some(DetailIntent::Apply)
        );
        assert_eq!(
            event(
                &p,
                &mut state,
                mouse::Event::ButtonReleased(mouse::Button::Left),
                point
            ),
            None
        );
    }
}

#[test]
fn d6_r1_tiny_depth_panel_keeps_readable_actions_close_and_optional_content() {
    let fitted = crate::donor_layout::depth_layout(
        3,
        1.0,
        [80.0, 60.0],
        [40.0, 80.0],
        crate::donor_layout::DepthParams {
            width: 40.0,
            height: 100.0,
            spacing: 0.0,
            visible_count: 3,
            ..Default::default()
        },
        1.0,
    );
    let fitted_size = fitted
        .cards
        .iter()
        .find(|c| c.filtered_index == 1)
        .unwrap()
        .size;
    assert_eq!(fitted_size, [8.0, 20.0]);
    for size in [[40.0, 100.0], fitted_size, [12.0, 100.0]] {
        for count in [4, 5, 6] {
            let mut p = program(true, 0.0);
            p.model.depth = Some(crate::donor_scene::DepthDetailPresentation {
                effect: 0,
                animate_shader: false,
                animate_back: false,
            });
            p.model.depth_revision = Some(1);
            p.model.card.size = size;
            p.model.card.skew_distance = 0.0;
            p.model.card.edge_tilt_distance = 0.0;
            p.model.radius = 0.0;
            p.model.actions = [
                (DetailIntent::Apply, "Apply"),
                (DetailIntent::Hide, "Hide"),
                (DetailIntent::Playlists, "Playlists"),
                (DetailIntent::OpenFolder, "Open Folder"),
                (DetailIntent::Properties, "Engine Properties"),
                (DetailIntent::Properties, "Engine Properties"),
            ]
            .into_iter()
            .take(count)
            .map(|(intent, label)| Action {
                intent,
                label: label.into(),
                enabled: true,
            })
            .collect();
            // D6-R1 user-approved fallback supersedes tiny always-inline fit; retain containment/input coverage.
            assert!(!donor_back_layout::depth_inline_eligible(
                p.model.card,
                p.model.radius,
                p.model.fields.len(),
                &p.model.actions,
                1.0,
                Font::default(),
                "Title"
            ));
            p.model.card = crate::donor_detail::external_target([1200.0, 900.0]);
            p.model.embedded = false;
            let original = p.model.card;
            let l = p.layout();
            assert!(
                p.content_region(&l).height > 0.0,
                "tiny Depth has no scrollable fact viewport: {size:?}/{count}"
            );
            for (r, intent, enabled) in p.controls(&l, 0.0) {
                assert!(enabled);
                assert!(r.width > 0.0 && r.height > 0.0);
                let a = p.model.card.point([r.center_x(), r.center_y()]);
                let point = Point::new(a[0], a[1]);
                assert_eq!(
                    p.hit_at(&l, point, 0.0),
                    Some(intent),
                    "{size:?}/{count}/{intent:?}"
                );
                let mut state = BackState::default();
                assert_eq!(
                    event(
                        &p,
                        &mut state,
                        mouse::Event::ButtonPressed(mouse::Button::Left),
                        point
                    ),
                    None
                );
                assert_eq!(
                    event(
                        &p,
                        &mut state,
                        mouse::Event::ButtonReleased(mouse::Button::Left),
                        point
                    ),
                    Some(intent)
                );
            }
            assert_eq!(p.model.card, original);
        }
    }
}

#[test]
fn d6_r1_panel_for_rounded_skewed_cuts_keeps_controls_and_each_fact_line_reachable() {
    for (size, skew, radius) in [
        ([40.0, 100.0], -12.0, 20.0),
        ([40.0, 100.0], 12.0, 20.0),
        ([8.0, 20.0], 2.4, 4.0),
        ([12.0, 100.0], -60.0, 6.0),
        ([1.1, 100.0], 12.0, 0.55),
        ([280.0, 520.0], 0.0, 100.0),
    ] {
        let mut p = program(true, 0.0);
        p.model.depth = Some(crate::donor_scene::DepthDetailPresentation {
            animate_shader: false,
            animate_back: false,
            ..Default::default()
        });
        p.model.depth_revision = Some(1);
        p.model.card.size = size;
        p.model.card.skew_distance = skew;
        p.model.card.edge_tilt_distance = 0.0;
        p.model.radius = radius;
        p.model.actions.extend([
            Action {
                intent: DetailIntent::Hide,
                label: "Hide".into(),
                enabled: true,
            },
            Action {
                intent: DetailIntent::OpenFolder,
                label: "Open Folder".into(),
                enabled: true,
            },
            Action {
                intent: DetailIntent::Properties,
                label: "Engine Properties".into(),
                enabled: true,
            },
            Action {
                intent: DetailIntent::Properties,
                label: "Engine Properties".into(),
                enabled: true,
            },
        ]);
        assert!(!donor_back_layout::depth_inline_eligible(
            p.model.card,
            p.model.radius,
            p.model.fields.len(),
            &p.model.actions,
            1.0,
            Font::default(),
            "Title"
        ));
        p.model.card = crate::donor_detail::external_target([1200.0, 900.0]);
        p.model.embedded = false;
        p.model.radius = 0.0;
        let l = p.layout();
        let scale = l.content_scale;
        assert_eq!(scale, 1.0);
        assert!(p.content_region(&l).height >= 12.0 * scale);
        for (r, intent, _) in p.controls(&l, 0.0) {
            let q = p.model.card.point([r.center_x(), r.center_y()]);
            assert_eq!(
                p.hit_at(&l, Point::new(q[0], q[1]), 0.0),
                Some(intent),
                "{size:?}/{skew}/{radius}/{intent:?}"
            );
            for corner in [r.position(), Point::new(r.x + r.width, r.y + r.height)] {
                let q = p.model.card.point([corner.x, corner.y]);
                assert!(crate::donor_layout::depth_contains(
                    p.model.card.center,
                    p.model.card.size,
                    p.model.card.skew_distance,
                    [p.model.radius; 4],
                    q
                ));
            }
        }
        let mut offsets = vec![0.0];
        for _ in 0..64 {
            let next = p.scroll_step(&l, *offsets.last().unwrap(), true);
            if next == *offsets.last().unwrap() {
                break;
            }
            offsets.push(next);
        }
        for r in &l.facts {
            let (label, value) = l.fact_text_bounds(*r);
            assert!(
                offsets
                    .iter()
                    .any(|s| p.line_visible(&l, label.center_y() - s, label.height)),
                "label unreachable {size:?}"
            );
            assert!(
                offsets
                    .iter()
                    .any(|s| p.line_visible(&l, value.center_y() - s, value.height)),
                "value unreachable {size:?}"
            );
        }
        assert!(offsets.iter().any(|s| p.line_visible(
            &l,
            l.source_text_bounds().1.center_y() - s,
            l.source_text_bounds().1.height
        )));
        let mut state = BackState {
            identity: p.model.identity.clone(),
            focus: Some(0),
            ..Default::default()
        };
        let enter = canvas::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Enter),
            modified_key: keyboard::Key::Named(keyboard::key::Named::Enter),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::default(),
            text: None,
            repeat: false,
        });
        assert_eq!(
            p.update(
                &mut state,
                &enter,
                Rectangle::new(Point::ORIGIN, iced::Size::new(1200.0, 900.0)),
                mouse::Cursor::Unavailable
            )
            .and_then(|a| a.into_inner().0),
            Some(DetailIntent::Apply)
        );
    }
}

#[test]
fn d5_normal_depth_and_old_mode_back_layouts_keep_existing_geometry_and_text_scale() {
    for (embedded, size) in [
        (true, [900.0, 520.0]),
        (true, [280.0, 300.0]),
        (false, [800.0, 600.0]),
        (false, [40.0, 100.0]),
    ] {
        let mut p = program(embedded, 0.0);
        p.model.card.size = size;
        p.model.card.skew_distance = 0.0;
        p.model.card.edge_tilt_distance = 0.0;
        p.model.radius = 0.0;
        let expected = donor_back_layout::layout(
            p.model.card,
            embedded,
            p.model.fields.len(),
            &p.model.actions,
            1.0,
            Font::default(),
            "A long supported title",
        );
        assert_eq!(format!("{:?}", p.layout()), format!("{expected:?}"));
        assert_eq!(p.layout().content_scale, 1.0);
        if embedded {
            p.model.depth = Some(Default::default());
            assert_eq!(format!("{:?}", p.layout()), format!("{expected:?}"));
        }
    }
}
#[test]
fn browse_move_scroll_body_close_and_cancelled_drag_never_publish_apply() {
    let p = program(false, 0.0);
    let mut state = BackState::default();
    let apply = p.layout().actions[0].center();
    let body = Point::new(800.0, 400.0);
    assert_eq!(
        event(
            &p,
            &mut state,
            mouse::Event::CursorMoved { position: body },
            body
        ),
        None
    );
    assert_eq!(
        event(
            &p,
            &mut state,
            mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 }
            },
            body
        ),
        None
    );
    assert_eq!(
        event(
            &p,
            &mut state,
            mouse::Event::ButtonPressed(mouse::Button::Left),
            body
        ),
        None
    );
    assert_eq!(
        event(
            &p,
            &mut state,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            body
        ),
        None
    );
    event(
        &p,
        &mut state,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        apply,
    );
    assert_ne!(
        event(
            &p,
            &mut state,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            body
        ),
        Some(DetailIntent::Apply)
    );
    let outside = Point::new(10.0, 10.0);
    event(
        &p,
        &mut state,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        outside,
    );
    assert_eq!(
        event(
            &p,
            &mut state,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            outside
        ),
        Some(DetailIntent::Close)
    );
}
#[test]
fn selection_change_invalidates_a_pressed_action_and_early_flip_is_inert() {
    let mut p = program(true, 0.0);
    let mut state = BackState::default();
    let r = p.layout().actions[0];
    let a = p.model.card.point([r.center_x(), r.center_y()]);
    let point = Point::new(a[0], a[1]);
    event(
        &p,
        &mut state,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        point,
    );
    p.model.identity = "last-item".into();
    assert_eq!(
        event(
            &p,
            &mut state,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            point
        ),
        None
    );
    p.model.progress = 0.5;
    assert_eq!(p.hit_at(&p.layout(), point, 0.0), None);
}
#[test]
fn action_hit_rejects_bounding_box_corners() {
    let p = program(true, 0.4);
    let r = p.layout().actions[0];
    let q = p.model.card.point([r.x, r.y]);
    // One pixel outside in local coordinates stays outside after projection.
    let outside = p.model.card.point([r.x - 1.0, r.y - 1.0]);
    assert!(p.contains(r, Point::new(q[0] + 0.1, q[1] + 0.1)));
    assert!(!p.contains(r, Point::new(outside[0], outside[1])));
}
#[test]
fn optional_fact_overflow_scrolls_without_moving_apply() {
    let mut p = program(true, 0.0);
    p.model.card.size = [280.0, 300.0];
    p.model.actions.extend([
        Action {
            intent: DetailIntent::Hide,
            label: "Hide".into(),
            enabled: true,
        },
        Action {
            intent: DetailIntent::Properties,
            label: "Engine Properties".into(),
            enabled: true,
        },
        Action {
            intent: DetailIntent::OpenFolder,
            label: "Open Folder".into(),
            enabled: true,
        },
    ]);
    let l = p.layout();
    let first = l.actions[0];
    let limit = p.scroll_limit(&l);
    assert!(limit > 0.0);
    let mut s = BackState::default();
    event(
        &p,
        &mut s,
        mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: -8.0 },
        },
        Point::new(500.0, 400.0),
    );
    for _ in 0..20 {
        event(
            &p,
            &mut s,
            mouse::Event::WheelScrolled {
                delta: mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
            },
            Point::new(500.0, 400.0),
        );
    }
    assert_eq!(s.scroll, limit);
    assert_eq!(p.layout().actions[0], first);
}
#[test]
fn every_fact_value_and_folder_is_reachable_in_a_16px_content_viewport() {
    let mut p = program(true, 0.0);
    p.model.card.size = [280.0, 220.0];
    p.model.actions.extend([
        Action {
            intent: DetailIntent::Hide,
            label: "Hide".into(),
            enabled: true,
        },
        Action {
            intent: DetailIntent::Properties,
            label: "Engine Properties".into(),
            enabled: true,
        },
        Action {
            intent: DetailIntent::OpenFolder,
            label: "Open Folder".into(),
            enabled: true,
        },
    ]);
    let l = p.layout();
    assert!(p.content_region(&l).height >= 16.0 - 0.01);
    assert!(
        !l.title_lines.is_empty(),
        "tiny title must retain a measured ellipsis"
    );

    let mut offsets = vec![0.0];
    for _ in 0..30 {
        let next = p.scroll_step(&l, *offsets.last().unwrap(), true);
        if next == *offsets.last().unwrap() {
            break;
        }
        offsets.push(next);
    }
    for r in &l.facts {
        assert!(
            offsets.iter().any(|s| p.line_visible(
                &l,
                l.fact_text_bounds(*r).0.center_y() - s,
                l.fact_text_bounds(*r).0.height
            )),
            "label unreachable"
        );
        assert!(
            offsets.iter().any(|s| p.line_visible(
                &l,
                l.fact_text_bounds(*r).1.center_y() - s,
                l.fact_text_bounds(*r).1.height
            )),
            "value unreachable"
        );
    }
    assert!(
        offsets.iter().any(|s| p.line_visible(
            &l,
            l.source_text_bounds().1.center_y() - s,
            l.source_text_bounds().1.height
        )),
        "folder unreachable"
    );
}
#[test]
fn reverse_tab_brings_the_tag_action_into_a_short_detail_view() {
    let mut p = program(true, 0.0);
    p.model.card.size = [365.0, 245.0];
    p.model.fields.push(("Applied".into(), "3 times".into()));
    let layout = p.layout();
    assert!(!layout.wide);
    assert!(p.scroll_limit(&layout) > 0.0);
    let mut state = BackState::default();
    let tab = canvas::Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(keyboard::key::Named::Tab),
        modified_key: keyboard::Key::Named(keyboard::key::Named::Tab),
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Tab),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::SHIFT,
        text: None,
        repeat: false,
    });
    let bounds = Rectangle::with_size(iced::Size::new(1200.0, 900.0));
    assert!(p
        .update(&mut state, &tab, bounds, mouse::Cursor::Unavailable)
        .is_some());
    let (rect, intent, enabled) = p.controls(&layout, state.scroll)[state.focus.unwrap()];
    assert_eq!(intent, DetailIntent::LocalTags);
    assert!(enabled);
    let point = p.model.card.point([rect.center_x(), rect.center_y()]);
    assert_eq!(
        p.hit_at(&layout, Point::new(point[0], point[1]), state.scroll),
        Some(DetailIntent::LocalTags)
    );
}

#[test]
fn higher_sheet_disables_stale_detail_focus_and_tab_hands_off() {
    let mut p = program(false, 0.0);
    let mut s = BackState {
        identity: p.model.identity.clone(),
        focus: Some(0),
        ..Default::default()
    };
    let bounds = Rectangle::new(Point::ORIGIN, iced::Size::new(1200.0, 900.0));
    let enter = canvas::Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(keyboard::key::Named::Enter),
        modified_key: keyboard::Key::Named(keyboard::key::Named::Enter),
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Enter),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::default(),
        text: None,
        repeat: false,
    });
    p.model.input_active = false;
    assert!(p
        .update(&mut s, &enter, bounds, mouse::Cursor::Unavailable)
        .is_none());
    assert_eq!(s.focus, None);
    p.model.input_active = true;
    s.focus = Some(p.controls(&p.layout(), 0.0).len() - 1);
    let tab = canvas::Event::Keyboard(keyboard::Event::KeyPressed {
        key: keyboard::Key::Named(keyboard::key::Named::Tab),
        modified_key: keyboard::Key::Named(keyboard::key::Named::Tab),
        physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Tab),
        location: keyboard::Location::Standard,
        modifiers: keyboard::Modifiers::default(),
        text: None,
        repeat: false,
    });
    assert!(p
        .update(&mut s, &tab, bounds, mouse::Cursor::Unavailable)
        .is_none());
    assert_eq!(s.focus, None);
}

#[test]
fn d4a_actual_back_phases_flags_shape_and_actions_match_depth_geometry() {
    for effect in 0..12 {
        for (shader, back) in [(true, true), (false, true), (true, false), (false, false)] {
            let mut p = program(true, 0.0);
            p.model.depth = Some(crate::donor_scene::DepthDetailPresentation {
                effect,
                animate_shader: shader,
                animate_back: back,
            });
            p.model.card.edge_tilt_distance = 0.0;
            p.model.card.size = [280.0, 520.0];
            p.model.card.skew_distance = -70.0;
            p.model.radius = 60.0;
            for progress in [0.0, 0.4, 0.74, 0.82, 0.94, 1.0] {
                p.model.progress = progress;
                let expected = card_flip_phases(progress, shader, back);
                assert_eq!(p.phases(), (expected.surface, expected.content));
                let l = p.layout();
                let rect = p.action_rect(&l, 0);
                let q = p.model.card.point([rect.center_x(), rect.center_y()]);
                let mut state = BackState::default();
                let point = Point::new(q[0], q[1]);
                let hit = p.hit_at(&l, point, 0.0);
                event(
                    &p,
                    &mut state,
                    mouse::Event::ButtonPressed(mouse::Button::Left),
                    point,
                );
                let action = event(
                    &p,
                    &mut state,
                    mouse::Event::ButtonReleased(mouse::Button::Left),
                    point,
                );
                assert_eq!(action, hit);
                if progress == 1.0 {
                    assert_eq!(action, Some(DetailIntent::Apply));
                }
                if progress == 0.0 {
                    assert_eq!(action, None);
                }
            }
            // Solid silhouette probes use the actual production Depth mask.
            let [cx, cy] = p.model.card.center;
            let [w, h] = p.model.card.size;
            for point in [
                Point::new(cx, cy),
                Point::new(cx - w * 0.5, cy - h * 0.5),
                Point::new(cx + w, cy),
            ] {
                let inside = crate::donor_layout::depth_contains(
                    p.model.card.center,
                    p.model.card.size,
                    p.model.card.skew_distance,
                    [p.model.radius; 4],
                    [point.x, point.y],
                );
                assert_eq!(p.contains(p.layout().card, point), inside);
            }
            p.model.opacity = 0.00001;
            assert_eq!(p.hit_at(&p.layout(), Point::new(cx, cy), 0.0), None);
            p.model.input_active = false;
            let mut state = BackState::default();
            assert_eq!(
                event(
                    &p,
                    &mut state,
                    mouse::Event::ButtonReleased(mouse::Button::Left),
                    Point::new(cx, cy)
                ),
                None
            );
        }
    }
}

#[test]
fn d4a_stale_back_press_revision_cannot_release_apply() {
    let mut p = program(true, 0.0);
    p.model.depth = Some(crate::donor_scene::DepthDetailPresentation::default());
    p.model.depth_revision = Some(10);
    p.model.card.edge_tilt_distance = 0.0;
    let r = p.layout().actions[0];
    let q = p.model.card.point([r.center_x(), r.center_y()]);
    let point = Point::new(q[0], q[1]);
    let mut state = BackState::default();
    event(
        &p,
        &mut state,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        point,
    );
    p.model.depth_revision = Some(11);
    p.model.card.center[0] += 1.0;
    assert_eq!(
        event(
            &p,
            &mut state,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            point
        ),
        None
    );
}

#[test]
fn d4a_benign_hover_revision_keeps_current_geometry_action_reachable() {
    let mut p = program(true, 0.0);
    p.model.depth = Some(crate::donor_scene::DepthDetailPresentation::default());
    p.model.depth_revision = Some(10);
    p.model.card.edge_tilt_distance = 0.0;
    let r = p.layout().actions[0];
    let q = p.model.card.point([r.center_x(), r.center_y()]);
    let point = Point::new(q[0], q[1]);
    let mut state = BackState::default();
    event(
        &p,
        &mut state,
        mouse::Event::CursorMoved { position: point },
        point,
    );
    event(
        &p,
        &mut state,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        point,
    );
    p.model.depth_revision = Some(11); // same geometry, changed hover presentation
    event(
        &p,
        &mut state,
        mouse::Event::CursorMoved { position: point },
        point,
    );
    assert_eq!(
        event(
            &p,
            &mut state,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            point
        ),
        Some(DetailIntent::Apply)
    );
    assert_eq!(
        event(
            &p,
            &mut state,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            point
        ),
        None
    );
}

#[test]
fn d4b_cut_changes_cancel_pressed_action_and_current_cut_remains_usable() {
    let mut p = program(true, 0.0);
    p.model.depth = Some(crate::donor_scene::DepthDetailPresentation::default());
    p.model.depth_revision = Some(10);
    p.model.card.edge_tilt_distance = 0.0;
    let r = p.layout().actions[0];
    let q = p.model.card.point([r.center_x(), r.center_y()]);
    let point = Point::new(q[0], q[1]);
    let mut state = BackState::default();
    event(
        &p,
        &mut state,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        point,
    );
    p.model.depth_revision = Some(11);
    let half = p.model.card.size[0] * 0.5;
    p.model.card.center[0] += half * 0.25;
    p.model.card.size[0] *= 0.75;
    p.model.radius = p.model.radius.min(p.model.card.size[0] * 0.5);
    assert_eq!(
        event(
            &p,
            &mut state,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            point
        ),
        None
    );
    let r = p.layout().actions[0];
    let q = p.model.card.point([r.center_x(), r.center_y()]);
    let point = Point::new(q[0], q[1]);
    event(
        &p,
        &mut state,
        mouse::Event::ButtonPressed(mouse::Button::Left),
        point,
    );
    assert_eq!(
        event(
            &p,
            &mut state,
            mouse::Event::ButtonReleased(mouse::Button::Left),
            point
        ),
        Some(DetailIntent::Apply)
    );
}

#[test]
fn e2_loaded_font_titles_all_presentations_clusters_clips_and_a_b_a() {
    {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .unwrap();
        fonts.load_font(std::borrow::Cow::Borrowed(include_bytes!(
            "../assets/fonts/RobotoCondensed-Bold-UI.ttf"
        )));
    }
    let font = Font {
        family: iced::font::Family::Name("Roboto Condensed"),
        weight: iced::font::Weight::Bold,
        ..Font::default()
    };
    let exact = include_str!("fixtures/e2-workshop-title.txt");
    let titles = [
        exact.to_owned(),
        "山の風景と夕暮れ八奈見杏菜".repeat(20),
        "e\u{301}".repeat(160),
        "W".repeat(200),
        "Short title".into(),
    ];
    for (route, (embedded, depth)) in [(true, false), (false, false), (false, false), (true, true)]
        .into_iter()
        .enumerate()
    {
        for scale in [1.0, 1.25] {
            let mut p = program(embedded, if embedded { 0.025 } else { 0.0 });
            p.model.font = font;
            p.model.scale = scale;
            p.model.depth = depth.then(Default::default);
            if !embedded {
                p.model.card = crate::donor_detail::external_target(if route == 2 {
                    [768.0, 512.0]
                } else {
                    [1920.0, 1080.0]
                });
            }
            for title in &titles {
                p.model.title = title.clone();
                let l = p.layout();
                assert!(!l.title_lines.is_empty());
                assert!(l.title_lines.len() <= if embedded { 1 } else { 2 });
                let origin = p.model.card.point(p.model.card.center);
                let axis = p
                    .model
                    .card
                    .point([p.model.card.center[0] + 1.0, p.model.card.center[1]]);
                let width = l.title.width * (axis[0] - origin[0]).hypot(axis[1] - origin[1]);
                for line in &l.title_lines {
                    assert!(
                        donor_back_layout::measured_width(line, l.title_size * scale, font)
                            <= width + 0.01
                    );
                    assert!(!line.starts_with('\u{301}'));
                    if title.starts_with("e\u{301}") {
                        assert!(line.trim_end_matches('…').ends_with('\u{301}'));
                    }
                }
                if title == "Short title" {
                    assert_eq!(l.title_lines, ["Short title"]);
                } else {
                    assert!(l.title_lines.last().unwrap().ends_with('…'));
                }
                let clip = p.title_clip(&l, 0.0);
                for q in [
                    clip.position(),
                    Point::new(clip.x + clip.width, clip.y),
                    Point::new(clip.x, clip.y + clip.height),
                    Point::new(clip.x + clip.width, clip.y + clip.height),
                ] {
                    let q = p.model.card.inverse_point([q.x, q.y]).unwrap();
                    assert!(q[0] >= l.title.x - 0.01 && q[0] <= l.title.x + l.title.width + 0.01);
                    assert!(q[1] >= l.title.y - 0.01 && q[1] <= l.title.y + l.title.height + 0.01);
                }
                for fact in &l.facts {
                    let (label, value) = l.fact_text_bounds(*fact);
                    assert!(
                        label.y + label.height < value.y,
                        "metadata rows overlap: {label:?} / {value:?}"
                    );
                    assert!(
                        fact.x >= l.title.x + l.title.width || fact.y >= l.title.y + l.title.height
                    );
                }
                let label_height = 1.25 * l.label_size * scale;
                let first_action = l.actions[0];
                assert!(l.actions_label.y + label_height * 0.5 < first_action.y);
                if l.wide {
                    assert!(l.actions_label.y - label_height * 0.5 > l.rule_y + scale);
                }
                for (r, intent, enabled) in p.controls(&l, 0.0) {
                    if enabled {
                        let q = p.model.card.point([r.center_x(), r.center_y()]);
                        assert_eq!(p.hit_at(&l, Point::new(q[0], q[1]), 0.0), Some(intent));
                    }
                }
            }
            let mut state = BackState::default();
            for (identity, title) in [("A", exact), ("B", "A short B"), ("A", exact)] {
                p.model.identity = identity.into();
                p.model.title = title.into();
                let l = p.layout();
                let q = p.model.card.point([l.close.center_x(), l.close.center_y()]);
                let point = Point::new(q[0], q[1]);
                event(
                    &p,
                    &mut state,
                    mouse::Event::ButtonPressed(mouse::Button::Left),
                    point,
                );
                assert_eq!(state.identity, identity);
                assert_eq!(
                    event(
                        &p,
                        &mut state,
                        mouse::Event::ButtonReleased(mouse::Button::Left),
                        point
                    ),
                    Some(DetailIntent::Close)
                );
                assert_eq!(l.title_lines, p.layout().title_lines);
            }
        }
    }
}

use super::*;
use keyboard::key::Named::{ArrowDown, ArrowLeft, ArrowRight, ArrowUp};

#[test]
fn keyboard_parity_all_directions_and_incomplete_edges_use_reference_targets() {
    // Independently transcribed beta.24 targets for 13 items, Hex rows=3/Wall cols=3.
    let cases = [
        (
            GalleryMode::Slices,
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        ),
        (
            GalleryMode::Geometric,
            [0, 0, 1, 3, 3, 4, 6, 6, 7, 9, 9, 10, 12],
            [1, 2, 2, 4, 5, 5, 7, 8, 8, 10, 11, 11, 12],
        ),
        (
            GalleryMode::Wall,
            [0, 1, 2, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9],
            [3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 10, 11, 12],
        ),
        (
            GalleryMode::Depth,
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12],
        ),
    ];
    for (mode, up, down) in cases {
        let (mut app, submissions) = d3_application_fixture(13);
        app.gallery_mode = mode;
        app.state.gallery.geometric.rows = 3;
        app.state.gallery.wall.columns = 3;
        for i in 0usize..13 {
            for (key, expected) in [
                (ArrowLeft, i.saturating_sub(1)),
                (ArrowRight, (i + 1).min(12)),
                (ArrowUp, up[i]),
                (ArrowDown, down[i]),
            ] {
                app.selected = Some(i);
                d3_key(&mut app, key);
                assert_eq!(app.selected, Some(expected), "{mode:?} {key:?} at {i}");
            }
        }
        assert_eq!(submissions.load(Ordering::Acquire), 0);
        assert!(!app.apply_pending);
    }
}

#[test]
fn keyboard_parity_filtered_sorted_identity_empty_and_single_are_preserved() {
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
        GalleryMode::Depth,
    ] {
        let (mut app, submissions) = d3_application_fixture(13);
        app.gallery_mode = mode;
        app.state.gallery.geometric.rows = 3;
        app.state.gallery.wall.columns = 3;
        // Deliberately non-catalog order, as produced by filters and sorting.
        app.filtered_indices = vec![11, 2, 8, 5, 1];
        app.presentation_inputs = None;
        app.selected = Some(2);
        d3_key(&mut app, ArrowRight);
        assert_eq!(app.selected, Some(8));
        d3_key(&mut app, ArrowDown);
        assert_eq!(
            app.selected,
            Some(8),
            "missing vertical target at position2"
        );
        app.selected = Some(2);
        d3_key(&mut app, ArrowDown);
        assert_eq!(
            app.selected,
            Some(match mode {
                GalleryMode::Geometric => 8,
                GalleryMode::Wall => 1,
                _ => 2,
            })
        );
        d3_key(&mut app, ArrowUp);
        assert_eq!(app.selected, Some(2));
        for indices in [vec![5], vec![]] {
            app.selected = indices.first().copied();
            app.filtered_indices = indices;
            app.presentation_inputs = None;
            let before = app.selected;
            for key in [ArrowLeft, ArrowRight, ArrowUp, ArrowDown] {
                d3_key(&mut app, key);
                assert_eq!(app.selected, before);
            }
        }
        assert_eq!(submissions.load(Ordering::Acquire), 0);
    }
}

#[test]
fn keyboard_parity_binding_modifiers_and_keyboard_owners_keep_priority() {
    use candidate_state::preferences::BindingAction as B;
    let (mut app, submissions) = d3_application_fixture(13);
    app.gallery_mode = GalleryMode::Wall;
    app.state.gallery.wall.columns = 3;
    app.state
        .frontend_preferences
        .bindings
        .insert(B::NavDown, "ctrl+down, q".into());
    d3_key(&mut app, ArrowDown);
    assert_eq!(
        app.selected,
        Some(0),
        "no raw-arrow fallback for a rebound action"
    );
    let send = |app: &mut GpuCandidate, key, modifiers, event_window| {
        let _ = update(
            app,
            Message::KeyPressed {
                key,
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
    let window = app.window.unwrap();
    send(
        &mut app,
        keyboard::Key::Named(ArrowDown),
        keyboard::Modifiers::CTRL,
        window,
    );
    assert_eq!(app.selected, Some(3));
    send(
        &mut app,
        keyboard::Key::Character("q".into()),
        keyboard::Modifiers::NONE,
        window,
    );
    assert_eq!(app.selected, Some(6), "slot2 dispatches exactly one row");
    send(
        &mut app,
        keyboard::Key::Named(ArrowDown),
        keyboard::Modifiers::CTRL | keyboard::Modifiers::SHIFT,
        window,
    );
    assert_eq!(
        app.selected,
        Some(6),
        "extra modifiers are not an ordinary action"
    );
    send(
        &mut app,
        keyboard::Key::Named(ArrowDown),
        keyboard::Modifiers::CTRL,
        window::Id::unique(),
    );
    assert_eq!(app.selected, Some(6), "stale window is refused");
    for owner in 0..5 {
        app.panel_state.search_open = owner == 0;
        app.playlist_panel_open = owner == 1;
        app.engine_properties_open = owner == 2;
        app.controls_panel_open = owner == 3;
        app.folder_menu_open = owner == 4;
        send(
            &mut app,
            keyboard::Key::Named(ArrowDown),
            keyboard::Modifiers::CTRL,
            window,
        );
        assert_eq!(app.selected, Some(6), "higher keyboard owner {owner}");
    }
    for key in [ArrowLeft, ArrowRight, ArrowUp, ArrowDown] {
        assert!(
            !should_handle_key_event(
                event::Status::Captured,
                &keyboard::Key::Named(key),
                keyboard::Modifiers::NONE
            ),
            "text editor owns captured {key:?}"
        );
    }
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

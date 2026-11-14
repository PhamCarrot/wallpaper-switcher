use super::*;
use crate::candidate_state::preferences::BindingAction as B;
use view::settings::{
    composed::{
        binding_capture::Capture,
        input::{InputAction as A, Mods, MouseButton},
    },
    folio::intent::SettingsMsg,
};

fn gallery() -> (GpuCandidate, Arc<std::sync::atomic::AtomicUsize>) {
    let (mut app, count) = d3_application_fixture(8);
    app.gallery_mode = GalleryMode::Slices;
    app.selected = Some(0);
    (app, count)
}
fn key(app: &mut GpuCandidate, key: keyboard::Key, modifiers: keyboard::Modifiers) {
    let event = NavigationKeyEvent {
        event_window: app.window.unwrap(),
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        repeat: false,
    };
    let _ = update(
        app,
        Message::KeyPressed {
            key,
            modifiers,
            event,
        },
    );
}
fn character(app: &mut GpuCandidate, value: &str) {
    key(
        app,
        keyboard::Key::Character(value.into()),
        keyboard::Modifiers::empty(),
    );
}
fn named(app: &mut GpuCandidate, value: keyboard::key::Named) {
    key(
        app,
        keyboard::Key::Named(value),
        keyboard::Modifiers::empty(),
    );
}
fn pointer(app: &GpuCandidate, index: usize, button: MouseButton, mods: Mods) -> Message {
    let w = &app.wallpapers[index];
    super::super::lifecycle::bind_gallery_input(
        app,
        GalleryMessage::Pointer {
            index: Some(index),
            id: Some(w.id.clone()),
            source: Some(super::super::gallery::poster_source_for_item(w)),
            revision: None,
            presented_catalog_generation: Some(app.catalog_generation.load(Ordering::Acquire)),
            button,
            modifiers: mods,
            sequence: None,
        },
    )
}

#[test]
fn w2_second_slot_overrides_config_and_none_removes_every_old_key() {
    let (mut app, submissions) = gallery();
    app.config.keybinds.right = "n".into();
    named(&mut app, keyboard::key::Named::ArrowRight);
    assert_eq!(app.selected, Some(0));
    character(&mut app, "n");
    assert_eq!(app.selected, Some(1));
    app.state
        .frontend_preferences
        .bindings
        .insert(B::NavRight, "k, q".into());
    character(&mut app, "n");
    assert_eq!(app.selected, Some(1));
    character(&mut app, "q");
    assert_eq!(
        app.selected,
        Some(2),
        "one input dispatches exactly one navigation"
    );
    app.state
        .frontend_preferences
        .bindings
        .insert(B::NavRight, "none".into());
    character(&mut app, "q");
    character(&mut app, "k");
    character(&mut app, "n");
    named(&mut app, keyboard::key::Named::ArrowRight);
    assert_eq!(app.selected, Some(2));
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}
#[test]
fn w2_preview_apply_second_slot_cannot_dispatch_to_provider() {
    let (mut app, count) = gallery();
    app.launch_policy = p1_preview_policy();
    app.state
        .frontend_preferences
        .bindings
        .insert(B::Apply, "enter, q".into());
    character(&mut app, "q");
    assert!(!app.apply_pending);
    assert_eq!(count.load(Ordering::Acquire), 0);
    assert!(app
        .status
        .as_deref()
        .unwrap_or("")
        .to_ascii_lowercase()
        .contains("preview"));
}
#[test]
fn w2_invalid_saved_override_is_diagnosed_and_does_not_restore_config() {
    let (mut app, _) = gallery();
    app.state
        .frontend_preferences
        .bindings
        .insert(B::NavRight, "banana+q".into());
    let map = super::super::lifecycle::effective_input_bindings(&app);
    assert!(map.triggers(A::NavRight).is_empty());
    assert!(map.diagnostics.iter().any(|d| d.contains("NavRight")));
    named(&mut app, keyboard::key::Named::ArrowRight);
    character(&mut app, "q");
    assert_eq!(app.selected, Some(0));
    assert_eq!(
        app.state.frontend_preferences.bindings[&B::NavRight],
        "banana+q"
    );
}
#[test]
fn w2_capture_save_reload_and_explicit_default_preserve_precedence() {
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.config.keybinds.right = "n".into();
    p3_settings_intent(
        &mut app,
        SettingsMsg::SelectSection(view::settings::composed::PICKER_CONTROLS_SECTION),
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::KeybindCapture("keys.navRight".into(), 1),
    );
    assert!(app.settings_workbench.capture.is_some());
    character(&mut app, "q");
    assert!(app.settings_workbench.capture.as_ref().unwrap().edited);
    named(&mut app, keyboard::key::Named::Enter);
    assert!(app.settings_workbench.capture.is_none());
    let saved = CandidateState::load(&app.state_path);
    assert_eq!(saved.frontend_preferences.bindings[&B::NavRight], "n, q");
    app.state = saved;
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    app.selected = Some(0);
    character(&mut app, "q");
    assert_eq!(app.selected, Some(1));
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    view::settings::controller::reconcile(&mut app);
    p3_settings_intent(
        &mut app,
        SettingsMsg::SelectSection(view::settings::composed::PICKER_CONTROLS_SECTION),
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::KeybindCapture("keys.navRight".into(), 1),
    );
    p3_settings_intent(&mut app, SettingsMsg::KeybindCaptureDefault);
    p3_settings_intent(&mut app, SettingsMsg::KeybindCaptureApply);
    p3_settings_intent(
        &mut app,
        SettingsMsg::KeybindCapture("keys.navRight".into(), 0),
    );
    p3_settings_intent(&mut app, SettingsMsg::KeybindCaptureDefault);
    p3_settings_intent(&mut app, SettingsMsg::KeybindCaptureApply);
    app.state = CandidateState::load(&app.state_path);
    assert_eq!(
        app.state
            .frontend_preferences
            .bindings
            .get(&B::NavRight)
            .map(String::as_str),
        Some(""),
        "existing empty default sentinel is preserved"
    );
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    app.selected = Some(0);
    character(&mut app, "q");
    assert_eq!(app.selected, Some(0));
    character(&mut app, "n");
    assert_eq!(
        app.selected,
        Some(0),
        "explicit composed default overrides old Config key"
    );
    named(&mut app, keyboard::key::Named::ArrowRight);
    assert_eq!(app.selected, Some(1));
    app.selected = Some(0);
    character(&mut app, "d");
    assert_eq!(
        app.selected,
        Some(1),
        "default secondary executes the actual navigation"
    );
}
#[test]
fn w2_reserved_config_capture_is_visible_and_refused() {
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.config.keybinds.close = "q".into();
    p3_settings_intent(
        &mut app,
        SettingsMsg::SelectSection(view::settings::composed::PICKER_CONTROLS_SECTION),
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::KeybindCapture("keys.navRight".into(), 1),
    );
    character(&mut app, "q");
    let map = super::super::lifecycle::effective_input_bindings(&app);
    assert!(map
        .reserved_conflicts(&app.settings_workbench.capture.as_ref().unwrap().triggers)
        .iter()
        .any(|s| s.contains("close")));
    named(&mut app, keyboard::key::Named::Enter);
    assert!(app.settings_workbench.capture.is_some());
    assert!(!app
        .state
        .frontend_preferences
        .bindings
        .contains_key(&B::NavRight));
    assert!(app.settings_workbench.message.contains("Reserved"));
    assert_eq!(app.config.keybinds.close, "q");
}
#[test]
fn w2_editable_conflict_moves_only_trigger_and_executes_one_action() {
    let (mut app, count) = gallery();
    app.state
        .frontend_preferences
        .bindings
        .insert(B::Favourite, "f, ctrl+o".into());
    let map = super::super::lifecycle::effective_input_bindings(&app);
    let mut capture = Capture::begin(7, "keys.navRight", 1, &map).unwrap();
    capture.replace(view::settings::composed::input::Trigger::parse("f").unwrap());
    assert_eq!(capture.conflicts(&map), vec![A::Favourite]);
    let changes = capture.proposal(7, &map).unwrap();
    let mut proposed = app.state.clone();
    for change in changes {
        view::settings::preference_projection::write(
            &mut proposed.frontend_preferences,
            change.path,
            &view::settings::composed::draft::Value::Text(change.binding),
        )
        .unwrap();
    }
    app.commit_local_state_change(proposed).unwrap();
    character(&mut app, "f");
    assert_eq!(app.selected, Some(1));
    assert!(app.state.favorites.is_empty());
    assert_eq!(
        app.state.frontend_preferences.bindings[&B::Favourite],
        "ctrl+o"
    );
    assert_eq!(count.load(Ordering::Acquire), 0);
}
#[test]
fn w2_text_capture_and_literal_configured_close_keep_their_owners() {
    let (mut app, _) = gallery();
    app.state
        .frontend_preferences
        .bindings
        .insert(B::NavRight, "q".into());
    app.panel_state.search_open = true;
    character(&mut app, "q");
    assert_eq!(app.selected, Some(0));
    named(&mut app, keyboard::key::Named::Escape);
    assert!(!app.panel_state.search_open);
    assert!(app.visible);
    app.config.keybinds.close = "f10".into();
    app.detail_open = true;
    named(&mut app, keyboard::key::Named::F10);
    assert!(!app.detail_open);
    assert!(app.visible);
    let mut editor = p3_composed_editor_app(SettingsCategory::Picker);
    p3_settings_intent(
        &mut editor,
        SettingsMsg::SelectSection(view::settings::composed::PICKER_CONTROLS_SECTION),
    );
    p3_settings_intent(
        &mut editor,
        SettingsMsg::KeybindCapture("keys.navRight".into(), 1),
    );
    let selected = editor.selected;
    character(&mut editor, "q");
    assert_eq!(editor.selected, selected);
    named(&mut editor, keyboard::key::Named::Escape);
    assert!(editor.settings_workbench.capture.is_none());
    assert!(editor.panel_state.settings_open);
}
#[test]
fn w2_stale_capture_generation_cannot_publish() {
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    p3_settings_intent(
        &mut app,
        SettingsMsg::SelectSection(view::settings::composed::PICKER_CONTROLS_SECTION),
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::KeybindCapture("keys.navRight".into(), 1),
    );
    character(&mut app, "q");
    let generation = app.settings_workbench.generation;
    view::settings::controller::rotate(&mut app);
    let _ = view::settings::controller::dispatch(
        &mut app,
        generation,
        view::settings::folio::intent::Message::Settings(SettingsMsg::KeybindCaptureApply),
    );
    assert!(!app
        .state
        .frontend_preferences
        .bindings
        .contains_key(&B::NavRight));
}
#[test]
fn w2_modifier_mouse_uses_the_current_item_and_detail_reducer() {
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    p3_settings_intent(
        &mut app,
        SettingsMsg::SelectSection(view::settings::composed::PICKER_CONTROLS_SECTION),
    );
    p3_settings_intent(&mut app, SettingsMsg::KeybindCapture("keys.flip".into(), 0));
    let window = app.window.unwrap();
    let _ = update(
        &mut app,
        Message::SettingsModifiers {
            window,
            modifiers: keyboard::Modifiers::CTRL,
        },
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::KeybindCaptureClick(MouseButton::Right),
    );
    p3_settings_intent(&mut app, SettingsMsg::KeybindCaptureApply);
    app.state = CandidateState::load(&app.state_path);
    assert_eq!(
        app.state.frontend_preferences.bindings[&B::Flip],
        "ctrl+right-click"
    );
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    app.gallery_mode = GalleryMode::Slices;
    app.selected = Some(0);
    let message = pointer(&app, 0, MouseButton::Right, Mods::new(true, false, false));
    let _ = update(&mut app, message);
    assert!(app.detail_open);
    assert_eq!(app.selected, Some(0));
    let message = pointer(&app, 0, MouseButton::Right, Mods::NONE);
    let _ = update(&mut app, message);
    assert!(
        app.detail_open,
        "overridden old mouse trigger must not close detail"
    );
    let message = pointer(&app, 0, MouseButton::Right, Mods::new(true, false, false));
    let _ = update(&mut app, message);
    assert!(!app.detail_open);
    assert!(!app.apply_pending);
}
#[test]
fn w2_stale_pointer_window_selection_or_generation_cannot_mutate_current_item() {
    let (mut app, _) = gallery();
    let message = pointer(&app, 0, MouseButton::Right, Mods::NONE);
    app.visibility_generation += 1;
    let _ = update(&mut app, message);
    assert!(!app.detail_open);
    let message = pointer(&app, 0, MouseButton::Right, Mods::NONE);
    app.selected = Some(1);
    let _ = update(&mut app, message);
    assert_eq!(app.selected, Some(1));
    assert!(!app.detail_open);
    let message = pointer(&app, 1, MouseButton::Right, Mods::NONE);
    app.window = Some(window::Id::unique());
    let _ = update(&mut app, message);
    assert!(!app.detail_open);
}
#[test]
fn w2_removed_and_undelivered_actions_have_no_effective_triggers() {
    let (mut app, _) = gallery();
    app.state
        .frontend_preferences
        .bindings
        .insert(B::SourceSteam, "q".into());
    let map = super::super::lifecycle::effective_input_bindings(&app);
    for action in [
        A::SourceSteam,
        A::Downloads,
        A::ColorNext,
        A::ThemePanel,
        A::RandomApply,
        A::HiddenFolders,
        A::RandomRotate,
        A::Reveal,
        A::TagMode,
    ] {
        assert!(map.triggers(action).is_empty(), "{action:?}");
    }
    for delivered in [A::FolderToggle, A::TagCloud] {
        assert!(
            !map.triggers(delivered).is_empty(),
            "W3 delivered {delivered:?}"
        );
    }
    character(&mut app, "q");
    assert_eq!(app.selected, Some(0));
    assert_eq!(
        app.state.frontend_preferences.bindings[&B::SourceSteam],
        "q"
    );
}

#[test]
fn w2_empty_default_sentinel_and_space_key_keep_their_meaning() {
    let (mut app, _) = gallery();
    app.config.keybinds.right = "n".into();
    app.state
        .frontend_preferences
        .bindings
        .insert(B::NavRight, String::new());
    character(&mut app, "n");
    assert_eq!(app.selected, Some(0));
    named(&mut app, keyboard::key::Named::ArrowRight);
    assert_eq!(app.selected, Some(1));
    app.state
        .frontend_preferences
        .bindings
        .insert(B::NavRight, "space".into());
    named(&mut app, keyboard::key::Named::Space);
    assert_eq!(app.selected, Some(2));
}
#[test]
fn w2_navigation_preset_cannot_bypass_config_conflicts() {
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    p3_settings_intent(
        &mut app,
        SettingsMsg::SelectSection(view::settings::composed::PICKER_CONTROLS_SECTION),
    );
    let before = app.state.frontend_preferences.bindings.clone();
    p3_settings_intent(
        &mut app,
        SettingsMsg::Run(view::settings::composed::ActionId::SetNavKeys(
            view::settings::composed::input::NavKeys::Vim,
        )),
    );
    assert_eq!(app.state.frontend_preferences.bindings, before);
    assert!(app.settings_workbench.message.contains("reserved Config"));
    app.state
        .frontend_preferences
        .bindings
        .insert(B::NavLeft, String::new());
    p3_settings_intent(
        &mut app,
        SettingsMsg::Run(view::settings::composed::ActionId::SetNavKeys(
            view::settings::composed::input::NavKeys::Wasd,
        )),
    );
    assert_eq!(
        super::super::lifecycle::effective_input_bindings(&app).triggers(A::NavLeft),
        view::settings::composed::input::parse_binding("left, a").unwrap()
    );
}
#[test]
fn w2_dirty_theme_escape_keeps_the_owned_decision() {
    let mut app = theme_dirty_applied_app();
    app.config.keybinds.close = "f10".into();
    named(&mut app, keyboard::key::Named::Escape);
    assert!(app.theme_workflow.prompt.is_some());
    assert_theme_draft_and_applied_kept(&app);
}

#[test]
fn w2_targetless_left_click_preserves_existing_backdrop_close() {
    let (mut app, _) = gallery();
    app.config.ui.close_on_backdrop_click = true;
    let message = super::super::lifecycle::bind_gallery_input(
        &app,
        GalleryMessage::Pointer {
            index: None,
            id: None,
            source: None,
            revision: None,
            presented_catalog_generation: None,
            button: MouseButton::Left,
            modifiers: Mods::NONE,
            sequence: None,
        },
    );
    let _ = update(&mut app, message);
    assert!(!app.visible);
}
#[test]
fn w2_first_mouse_binding_after_settings_departure_is_usable() {
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    app.gallery_mode = GalleryMode::Slices;
    app.selected = Some(0);
    let message = pointer(&app, 0, MouseButton::Right, Mods::NONE);
    let _ = update(&mut app, message);
    assert!(app.detail_open);
}
#[test]
fn w2_ordinary_pointer_rejects_outgoing_catalog_generation() {
    let (mut app, _) = gallery();
    let mut message = pointer(&app, 0, MouseButton::Right, Mods::NONE);
    if let Message::GalleryInput {
        message:
            GalleryMessage::Pointer {
                presented_catalog_generation,
                ..
            },
        ..
    } = &mut message
    {
        *presented_catalog_generation = Some(app.catalog_generation.load(Ordering::Acquire) + 1);
    }
    let _ = update(&mut app, message);
    assert!(!app.detail_open);
    let mut changed_source = pointer(&app, 0, MouseButton::Right, Mods::NONE);
    if let Message::GalleryInput {
        message: GalleryMessage::Pointer { source, .. },
        ..
    } = &mut changed_source
    {
        *source = Some(std::path::PathBuf::from("stale-presented-source.png"));
    }
    let _ = update(&mut app, changed_source);
    assert!(
        !app.detail_open,
        "a stale source cannot be replaced by the current fallback"
    );
}
#[test]
fn w2_depth_right_pointer_keeps_select_first_then_detail() {
    let (mut app, _) = d3_application_fixture(3);
    app.selected = Some(0);
    app.rebuild_depth_frame();
    let mut first = pointer(&app, 1, MouseButton::Right, Mods::NONE);
    if let Message::GalleryInput {
        message: GalleryMessage::Pointer { revision, .. },
        ..
    } = &mut first
    {
        *revision = Some(app.depth_frame.as_ref().unwrap().revision);
    }
    let _ = update(&mut app, first);
    assert_eq!(app.selected, Some(1));
    assert!(!app.detail_open);
    app.rebuild_depth_frame();
    let mut second = pointer(&app, 1, MouseButton::Right, Mods::NONE);
    if let Message::GalleryInput {
        message: GalleryMessage::Pointer { revision, .. },
        ..
    } = &mut second
    {
        *revision = Some(app.depth_frame.as_ref().unwrap().revision);
    }
    let _ = update(&mut app, second);
    assert!(app.detail_open);
}
#[test]
fn w2_hex_stationary_pointer_releases_keyboard_camera_owner() {
    let (mut app, _) = gallery();
    app.gallery_mode = GalleryMode::Geometric;
    app.state.gallery.mode = GalleryKind::Geometric;
    app.sync_hex_presentation();
    app.hex_keyboard = true;
    app.hex_presentation.as_mut().unwrap().keyboard = true;
    let message = pointer(&app, 0, MouseButton::Left, Mods::NONE);
    let _ = update(&mut app, message);
    assert!(!app.hex_keyboard);
    assert!(!app.hex_presentation.as_ref().unwrap().keyboard);
}
#[test]
fn w2_choose_displays_opens_actual_target_owner_without_effects_or_apply() {
    let (mut app, count) = gallery();
    let message = pointer(&app, 0, MouseButton::Left, Mods::new(true, false, false));
    let _ = update(&mut app, message);
    assert_eq!(
        app.settings_workbench.windows_panel,
        Some(SettingsCategory::WallpapersDisplays)
    );
    assert_eq!(count.load(Ordering::Acquire), 0);
}

#[test]
fn w2_depth_vertical_scope_and_config_navigation_rail_priority_are_preserved() {
    let (mut app, _) = d3_application_fixture(3);
    let selected = app.selected;
    named(&mut app, keyboard::key::Named::ArrowUp);
    named(&mut app, keyboard::key::Named::ArrowDown);
    assert_eq!(app.selected, selected);
    app.config.keybinds.right = "shift+up".into();
    key(
        &mut app,
        keyboard::Key::Named(keyboard::key::Named::ArrowUp),
        keyboard::Modifiers::SHIFT,
    );
    assert!(!app.rail_dismissed);
    assert_ne!(app.selected, selected);
}

#[test]
fn retirement_legacy_studio_binding_is_inert_and_does_not_reserve_shortcuts() {
    use view::settings::composed::input::{ActiveScopes, InputAction, KeyId, MouseSpec};
    let (mut app, submissions) = gallery();
    app.state
        .frontend_preferences
        .bindings
        .insert(B::Studio, "ctrl+alt+q".into());
    let map = lifecycle::effective_input_bindings(&app);
    let scopes = ActiveScopes::default();
    assert_eq!(
        map.lookup_key(
            &KeyId::Char("q".into()),
            Mods::new(true, true, false),
            scopes
        ),
        None
    );
    assert_eq!(
        map.lookup_mouse(
            MouseSpec {
                mods: Mods::new(false, false, true),
                button: MouseButton::Right
            },
            scopes
        ),
        None
    );
    let triggers = view::settings::composed::input::parse_binding("ctrl+alt+q").unwrap();
    assert!(map.reserved_conflicts(&triggers).is_empty());
    assert!(view::settings::composed::KEY_BINDINGS
        .iter()
        .all(|d| d.path != "keys.studio"));
    assert_eq!(
        map.lookup_mouse(
            MouseSpec {
                mods: Mods::new(true, false, false),
                button: MouseButton::Left
            },
            scopes
        ),
        Some(InputAction::Effects)
    );
    key(
        &mut app,
        keyboard::Key::Character("q".into()),
        keyboard::Modifiers::CTRL | keyboard::Modifiers::ALT,
    );
    assert!(!app.panel_state.settings_open && !app.apply_pending && app.active_operation.is_none());
    assert_eq!(submissions.load(Ordering::Acquire), 0);
    app.state.save(&app.state_path).unwrap();
    let restored = CandidateState::load(&app.state_path);
    assert_eq!(
        restored
            .frontend_preferences
            .bindings
            .get(&B::Studio)
            .unwrap(),
        "ctrl+alt+q"
    );
}

#[test]
fn e2_bound_hover_owns_opening_open_closing_and_higher_sheet_without_mutation() {
    use view::settings::composed::draft::Value;
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
        GalleryMode::Depth,
    ] {
        let (mut app, submissions) = gallery();
        app.set_gallery_mode(mode);
        app.selected = Some(0);
        app.rebuild_depth_frame();
        app.settings_workbench
            .draft
            .values
            .insert("general.hoverSelects".into(), Value::Boolean(true));
        for (open, progress) in [(true, 0.02), (true, 1.0), (false, 0.5)] {
            app.detail_open = open;
            app.detail_progress = progress;
            app.gallery_keyboard = true;
            app.hex_keyboard = true;
            for index in [Some(1), Some(0), None] {
                let revision = app.depth_frame.as_ref().map_or(0, |f| f.revision);
                let input = if mode == GalleryMode::Depth {
                    GalleryMessage::DepthPointer {
                        revision,
                        item_id: index.map(|i| app.wallpapers[i].id.clone()),
                        right: false,
                        hover: true,
                    }
                } else {
                    GalleryMessage::Hovered(index)
                };
                let bound = super::super::lifecycle::bind_gallery_input(&app, input);
                let before = (
                    app.selected,
                    app.detail_open,
                    app.detail_progress,
                    app.depth_hover.clone(),
                    app.depth_frame.as_ref().map(|f| f.revision),
                );
                let _ = update(&mut app, bound);
                assert_eq!(
                    before,
                    (
                        app.selected,
                        app.detail_open,
                        app.detail_progress,
                        app.depth_hover.clone(),
                        app.depth_frame.as_ref().map(|f| f.revision)
                    )
                );
                assert!(app.gallery_keyboard && app.hex_keyboard);
            }
        }
        app.cancel_detail("e2-settled");
        app.panel_state.settings_open = true;
        let bound =
            super::super::lifecycle::bind_gallery_input(&app, GalleryMessage::Hovered(Some(1)));
        let _ = update(&mut app, bound);
        assert_eq!(app.selected, Some(0));
        app.panel_state.settings_open = false;
        app.settings_workbench
            .draft
            .values
            .insert("general.hoverSelects".into(), Value::Boolean(false));
        let bound =
            super::super::lifecycle::bind_gallery_input(&app, GalleryMessage::Hovered(Some(1)));
        let _ = update(&mut app, bound);
        assert_eq!(app.selected, Some(0));
        app.settings_workbench
            .draft
            .values
            .insert("general.hoverSelects".into(), Value::Boolean(true));
        let mut stale =
            super::super::lifecycle::bind_gallery_input(&app, GalleryMessage::Hovered(Some(1)));
        if let Message::GalleryInput {
            catalog_generation, ..
        } = &mut stale
        {
            *catalog_generation = catalog_generation.wrapping_add(1);
        }
        let _ = update(&mut app, stale);
        assert_eq!(app.selected, Some(0));
        let bound =
            super::super::lifecycle::bind_gallery_input(&app, GalleryMessage::Hovered(Some(1)));
        let _ = update(&mut app, bound);
        assert_eq!(
            app.selected,
            if matches!(mode, GalleryMode::Slices | GalleryMode::Geometric) {
                Some(1)
            } else {
                Some(0)
            }
        );
        assert_eq!(submissions.load(Ordering::Acquire), 0);
    }
}

#[test]
fn e2_bound_detail_right_click_escape_close_and_reopen_stay_usable() {
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
        GalleryMode::Depth,
    ] {
        let (mut app, count) = gallery();
        app.set_gallery_mode(mode);
        app.selected = Some(0);
        app.rebuild_depth_frame();
        for index in [0, 1, 0] {
            app.cancel_detail("e2-next");
            app.selected = Some(index);
            app.rebuild_depth_frame();
            let input = pointer(&app, index, MouseButton::Right, Mods::NONE);
            let _ = update(&mut app, input);
            assert!(app.detail_open);
            app.detail_progress = 1.0;
            let id = app.wallpapers[index].id.clone();
            let _ = update(
                &mut app,
                Message::DonorDetail(id, donor_back_layout::DetailIntent::Close),
            );
            assert!(app.depth_detail.is_some_and(|s| s.target == 0.0));
            app.start_detail(true, "e2-reopen");
            named(&mut app, keyboard::key::Named::Escape);
            assert!(app.depth_detail.is_some_and(|s| s.target == 0.0));
            assert!(app.visible);
        }
        assert_eq!(count.load(Ordering::Acquire), 0);
    }
}

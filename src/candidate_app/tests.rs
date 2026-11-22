use super::*;
#[path = "tests/catalog_convergence.rs"]
mod catalog_convergence;
mod catalog_startup;
#[path = "tests/colours.rs"]
mod colour_tests;
#[path = "tests/detail_integration.rs"]
mod detail_integration;
#[path = "tests/frontend_layout.rs"]
mod frontend_layout;
#[path = "tests/gallery_presentation.rs"]
mod gallery_presentation;
#[path = "tests/hex_presentation.rs"]
mod hex_presentation;
#[path = "tests/keyboard_navigation.rs"]
mod keyboard_navigation;
#[path = "tests/layout_window.rs"]
mod layout_window;
#[path = "tests/local_bindings.rs"]
mod local_bindings;
#[path = "tests/local_library.rs"]
mod local_library;
#[path = "tests/pending_forms.rs"]
mod pending_forms;
#[path = "tests/pk04.rs"]
mod pk04;
#[path = "tests/pk05.rs"]
mod pk05;
#[path = "tests/pk06.rs"]
mod pk06;
#[path = "tests/pk07.rs"]
mod pk07;
mod pk08;
mod pk09;
#[path = "tests/playlists.rs"]
mod playlist_tests;
#[path = "tests/rotation.rs"]
mod rotation_tests;
#[path = "tests/theme_faults.rs"]
mod theme_faults;
#[path = "tests/theme_workflow.rs"]
mod theme_workflow;
#[path = "tests/usage.rs"]
mod usage_tests;
#[path = "tests/wall_presentation.rs"]
mod wall_presentation;

#[test]
fn p4_fractional_depth_canonical_values_survive_controls_profiles_and_restart() {
    use candidate_state::mode_profiles::{Mode, Snapshot};
    use view::settings::folio::intent::Message as Intent;
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let _ = update(
        &mut app,
        Message::SetGalleryNumber(GalleryNumberField::DepthCount, 5.75),
    );
    let _ = update(
        &mut app,
        Message::SetGalleryNumber(GalleryNumberField::DepthNavigation, 1000.25),
    );
    assert_eq!(
        app.state.gallery.depth.visible_count as f32, 5.75,
        "canonical count must not be rounded for the old consumer"
    );
    assert_eq!(
        app.state.gallery.depth.navigation_ms as f32, 1000.25,
        "canonical spring duration keeps fractions"
    );
    let runtime = gallery_layout_params(&app.state, 0.0, 0.0).depth;
    assert_eq!(
        runtime.visible_count, 5,
        "beta.24 casts count only at its integer rendering seam"
    );
    let snapshot = view::settings::profile_projection::capture(&app);
    let Snapshot::Depth(depth) = snapshot else {
        panic!("Depth snapshot")
    };
    assert_eq!((depth.visible_count, depth.navigation_ms), (5.75, 1000.25));
    p3_profile_intent(&mut app, Intent::SavePreset(Mode::Depth));
    let id = app.state.mode_profiles.selected_id(Mode::Depth).unwrap();
    let _ = update(
        &mut app,
        Message::SetGalleryNumber(GalleryNumberField::DepthCount, 7.25),
    );
    p3_profile_intent(&mut app, Intent::ApplyPreset(Mode::Depth, id));
    let loaded = CandidateState::load(&app.state_path);
    assert_eq!(
        (
            loaded.gallery.depth.visible_count as f32,
            loaded.gallery.depth.navigation_ms as f32
        ),
        (5.75, 1000.25)
    );
    assert_eq!(loaded.mode_profiles, app.state.mode_profiles);
    let _ = set_gallery_number_value(&mut app.state, GalleryNumberField::DepthCount, 5.01);
    let mut presentation = DepthPresentation::target(&app.state);
    let _ = set_gallery_number_value(&mut app.state, GalleryNumberField::DepthCount, 5.99);
    assert!(
        !presentation.tick(DepthPresentation::target(&app.state), 0.016),
        "equal source integer counts have equal presentation targets"
    );
    assert_eq!(presentation.params.visible_count, 5);
}

#[test]
fn p4_profile_source_save_is_durable_and_apply_publishes_geometry_and_identity_once() {
    use candidate_state::mode_profiles::Mode;
    use view::settings::folio::intent::Message as Intent;
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.state.gallery.depth.height = 500.0;
    p3_profile_intent(&mut app, Intent::SavePreset(Mode::Depth));
    let first = app
        .state
        .mode_profiles
        .selected_id(Mode::Depth)
        .expect("Save must publish the durable profile owner");
    let saved = CandidateState::load(&app.state_path);
    assert_eq!(saved.mode_profiles.selected_id(Mode::Depth), Some(first));
    app.state.gallery.depth.height = 700.0;
    p3_profile_intent(&mut app, Intent::SavePreset(Mode::Depth));
    let second = app.state.mode_profiles.selected_id(Mode::Depth).unwrap();
    assert_ne!(first, second);
    app.state.gallery.depth.height = 900.0;
    app.persist_state_outcome().unwrap();
    let before = std::fs::read(&app.state_path).unwrap();
    p3_profile_intent(&mut app, Intent::ApplyPreset(Mode::Depth, first));
    let applied = CandidateState::load(&app.state_path);
    assert_eq!(
        app.state.mode_profiles.selected_id(Mode::Depth),
        Some(first)
    );
    assert_eq!(app.state.gallery.depth.height, 500.0);
    assert_eq!(applied.mode_profiles.selected_id(Mode::Depth), Some(first));
    assert_eq!(applied.gallery.depth.height, 500.0);
    assert_eq!(
        std::fs::read(app.state_path.with_extension("toml.bak")).unwrap(),
        before,
        "Apply must publish geometry and selected identity together through one save"
    );
}

#[test]
fn p2_retired_text_callbacks_and_hidden_values_never_reach_actual_property_submission() {
    let mut app = mutation_target_fixture(true);
    let project = app.wallpapers[0].project_json_path.clone();
    let choice = r#"quoted "value" \ path"#;
    let schema_json = serde_json::json!({"general":{"properties":{
        "retired_text":{"type":"text","value":"hidden"},
        "retired_input":{"type":"textinput","value":"hidden input"},
        "color":{"type":"color","value":"#123456","order":1},
        "choice":{"type":"combo","value":"initial","order":2,"options":[
            {"label":"Initial","value":"initial"},{"label":"Quoted","value":choice}
        ]},
        "enabled":{"type":"bool","value":true,"order":3}
    }}});
    std::fs::write(&project, serde_json::to_vec(&schema_json).unwrap()).unwrap();
    app.engine_properties_open = false;
    app.toggle_engine_properties();
    assert_eq!(app.engine_properties.len(), 3);
    assert!(app
        .engine_properties_diagnostic
        .as_deref()
        .unwrap()
        .contains("skipped 2"));
    let before = app.engine_property_values.clone();
    for message in [
        Message::EnginePropertyTextChanged("retired_text".into(), "queued".into()),
        Message::EnginePropertyValueChanged("retired_input".into(), serde_json::json!("queued")),
        Message::EnginePropertyTextChanged("enabled".into(), "wrong kind".into()),
        Message::EnginePropertyTextChanged("choice".into(), choice.into()),
        Message::EnginePropertyValueChanged("color".into(), serde_json::json!(false)),
        Message::EnginePropertyValueChanged("choice".into(), serde_json::json!("not an option")),
        Message::EnginePropertyValueChanged("unknown".into(), serde_json::json!("hidden")),
    ] {
        let _ = update(&mut app, message);
        assert_eq!(app.engine_property_values, before);
    }
    for message in [
        Message::EnginePropertyTextChanged("color".into(), "#654321".into()),
        Message::EnginePropertyValueChanged("choice".into(), serde_json::json!(choice)),
        Message::EnginePropertyValueChanged("enabled".into(), serde_json::json!(false)),
    ] {
        let _ = update(&mut app, message);
    }
    // Outgoing validation also rejects hidden data injected without passing a reducer.
    app.engine_property_values
        .insert("retired_text".into(), serde_json::json!("hidden default"));
    app.engine_property_values
        .insert("unknown".into(), serde_json::json!("hidden default"));
    let requests = mutation_target_property_requests(
        &mut app,
        mutation_target_topology(&["DISPLAY-A", "DISPLAY-B"]),
    );
    assert_eq!(requests.len(), 1);
    let EngineCommand::ApplyProperties {
        properties, target, ..
    } = &requests[0]
    else {
        panic!("expected actual ApplyProperties request")
    };
    assert_eq!(*target, ApplyTarget::DeviceId("DISPLAY-A".into()));
    assert_eq!(
        properties,
        &serde_json::from_value::<serde_json::Map<String, serde_json::Value>>(serde_json::json!({
            "color":"#654321","choice":choice,"enabled":false
        }))
        .unwrap()
    );
    let encoded = serde_json::to_string(properties).unwrap();
    assert_eq!(
        serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(&encoded).unwrap(),
        *properties
    );
    assert!(!app.apply_pending);
    app.clear_engine_properties();
    let _ = update(
        &mut app,
        Message::EnginePropertyTextChanged("color".into(), "stale closed editor".into()),
    );
    assert!(app.engine_property_values.is_empty());
}

#[test]
fn p2_existing_organized_data_and_legacy_appearance_survive_the_supported_save_owner() {
    let (mut app, _) = d3_application_fixture(1);
    app.state = toml::from_str(include_str!("../testdata/ui-organization.toml")).unwrap();
    app.state.favorites.insert(app.wallpapers[0].id.clone());
    app.state.appearance.preset = "custom".into();
    app.state.appearance.accent = "#123456".into();
    let before = app.state.clone();
    app.persist_state_outcome().unwrap();
    let restored = CandidateState::load(&app.state_path);
    assert_eq!(restored.favorites, before.favorites);
    assert_eq!(restored.playlists, before.playlists);
    assert_eq!(restored.appearance, before.appearance);
    assert_eq!(restored.schedules, before.schedules);
    assert!(!restored.slideshow_enabled);
}

fn p1_preview_policy() -> runtime::LaunchPolicy {
    runtime::parse_launch_policy(&["candidate.exe".into(), "--preview".into()]).unwrap()
}

#[test]
fn p1_preview_invocation_rejects_every_developer_family_in_both_orders() {
    for flag in [
        "--diagnostic-apply",
        "--diagnostic-target",
        "--diagnostic-unknown",
        "--scenario",
        "--scenario-item",
        "--scenario-unknown",
        "--screenshot",
        "--screenshot-mode",
        "--screenshot-unknown",
        "--benchmark",
        "--benchmark-quick",
        "--benchmark-unknown",
    ] {
        for reversed in [false, true] {
            let mut flags = vec!["--preview", flag];
            if reversed {
                flags.reverse();
            }
            let args = std::iter::once("candidate.exe")
                .chain(flags)
                .map(str::to_string)
                .collect::<Vec<_>>();
            assert!(runtime::parse_launch_policy(&args).is_err(), "{args:?}");
        }
    }
    for flag in ["--preview=false", "--preview-profile", "--preview-unknown"] {
        assert!(runtime::parse_launch_policy(&["candidate.exe".into(), flag.into()]).is_err());
    }
}

#[test]
fn p12_normal_defaults_and_compatibility_flags_preserve_preview_precedence() {
    let preview = runtime::parse_launch_policy(&[
        "candidate.exe".into(),
        "--ALLOW-APPLY".into(),
        "--PREVIEW".into(),
        "--SHELL-HOOKS".into(),
        "--SHOW".into(),
    ])
    .unwrap();
    assert!(!preview.allows_provider());
    assert!(!preview.apply_enabled(true));
    assert!(!preview.shell_hooks_enabled(true));
    assert!(preview.show_at_start(false));
    assert!(!preview.requests_reopen());
    let normal = runtime::parse_launch_policy(&["candidate.exe".into()]).unwrap();
    assert!(normal.allows_provider());
    assert!(normal.apply_enabled(false));
    assert!(normal.shell_hooks_enabled(false));
    assert!(normal.show_at_start(false));
    assert!(normal.requests_reopen());
    let compatibility = runtime::parse_launch_policy(&[
        "candidate.exe".into(),
        "--allow-apply".into(),
        "--shell-hooks".into(),
        "--show".into(),
    ])
    .unwrap();
    assert_eq!(normal, compatibility);
    assert!(compatibility.apply_enabled(true));
    assert!(compatibility.shell_hooks_enabled(true));
    assert!(compatibility.show_at_start(true));
    let developer =
        runtime::parse_launch_policy(&["candidate.exe".into(), "--benchmark".into()]).unwrap();
    assert!(!developer.requests_reopen());
    assert!(!developer.apply_enabled(false));
    assert!(!developer.shell_hooks_enabled(false));
    assert!(!developer.show_at_start(false));
    assert!(developer.apply_enabled(true));
    assert!(developer.shell_hooks_enabled(true));
    assert!(developer.show_at_start(true));
}

#[test]
fn p1_preview_profiles_are_fresh_marked_and_refuse_cross_mode_reuse() {
    let (app, _) = d3_application_fixture(0);
    let root = app.state_path.parent().unwrap().join("preview-profiles");
    let first = runtime::fresh_preview_profile(&root).unwrap();
    let second = runtime::fresh_preview_profile(&root).unwrap();
    assert_ne!(first, second);
    assert_eq!(std::fs::read_dir(&first).unwrap().count(), 1);
    runtime::resolve_launch_profile(&first, p1_preview_policy()).unwrap();
    assert!(runtime::resolve_launch_profile(&first, Default::default()).is_err());
    let marker = first.join("candidate-launch-mode.toml");
    for invalid in [
        "version = 1\nmode = 'normal'\n",
        "version = 2\nmode = 'preview'\n",
        "version = 1\nmode = 'developer'\n",
        "version = 1\nmode = 'preview'\nunknown = true\n",
        "not a marker",
    ] {
        std::fs::write(&marker, invalid).unwrap();
        assert!(runtime::resolve_launch_profile(&first, p1_preview_policy()).is_err());
        assert_eq!(std::fs::read_to_string(&marker).unwrap(), invalid);
    }
    let unmarked = root.join("unmarked-normal");
    std::fs::create_dir(&unmarked).unwrap();
    std::fs::write(unmarked.join("config.toml"), b"saved normal intent").unwrap();
    assert!(runtime::resolve_launch_profile(&unmarked, p1_preview_policy()).is_err());
    runtime::resolve_launch_profile(&unmarked, Default::default()).unwrap();
    assert_eq!(
        std::fs::read(unmarked.join("config.toml")).unwrap(),
        b"saved normal intent"
    );
    assert!(!unmarked.join("candidate-launch-mode.toml").exists());
}

#[test]
fn p1_preview_profile_errors_fail_without_creating_or_replacing_data() {
    let (app, _) = d3_application_fixture(0);
    let root = app.state_path.parent().unwrap();
    let missing = root.join("missing-preview");
    assert!(runtime::resolve_launch_profile(&missing, p1_preview_policy()).is_err());
    assert!(!missing.exists());
    assert!(runtime::resolve_launch_profile(
        std::path::Path::new("relative-preview"),
        p1_preview_policy()
    )
    .is_err());
    let file = root.join("not-directory");
    std::fs::write(&file, b"preserve").unwrap();
    assert!(runtime::resolve_launch_profile(&file, p1_preview_policy()).is_err());
    assert!(runtime::fresh_preview_profile(&file).is_err());
    assert!(runtime::fresh_preview_profile(std::path::Path::new("relative-root")).is_err());
    assert_eq!(std::fs::read(file).unwrap(), b"preserve");
}

#[test]
fn p1_preview_rejects_real_submission_paths_even_with_apply_injected_enabled() {
    let (mut app, submissions) = d3_application_fixture(1);
    app.launch_policy = p1_preview_policy();
    app.apply_enabled = true;
    app.status = None;
    app.apply_selected_with(
        false,
        || panic!("preview enumerated Apply targets"),
        |_, _, _, _| panic!("preview submitted Apply"),
    );
    assert!(app.status.as_deref().unwrap().contains("--preview"));
    app.status = None;
    app.apply_engine_properties_with(
        || panic!("preview enumerated property targets"),
        |_, _| panic!("preview submitted properties"),
    );
    assert!(app.status.as_deref().unwrap().contains("--preview"));
    for command in [
        EngineCommand::SetPaused(true),
        EngineCommand::SetMuted(true),
        EngineCommand::SetVolume(50),
        EngineCommand::ReconcileProvider {
            recovery: test_provider_recovery(991, "Monitor1", "fixture"),
        },
        EngineCommand::CancelPreflight {
            recovery: test_provider_recovery(992, "Monitor1", "fixture"),
        },
    ] {
        app.status = None;
        app.submit_engine_command_with(command, |_, _| panic!("preview submitted Engine command"));
        assert!(app.status.as_deref().unwrap().contains("--preview"));
        assert!(!app.apply_pending);
    }
    // Keep the real owner independently blocked if this regression loses its preview guard.
    app.apply_pending = true;
    app.status = None;
    assert!(!app.run_playlist(
        None,
        None,
        AutomationTraceContext {
            generation: None,
            source: "preview-test",
            schedule_id: None,
        }
    ));
    assert!(app.status.as_deref().unwrap().contains("--preview"));
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn p1_preview_tray_random_and_current_tick_reject_without_advancing_automation() {
    let (mut app, submissions) = d3_application_fixture(1);
    app.launch_policy = p1_preview_policy();
    app.apply_enabled = true;
    // A missing policy guard must fail this test without permitting a real request.
    app.apply_pending = true;
    app.state.slideshow_enabled = true;
    app.next_slideshow_unix = 1;
    app.status = None;
    let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Random)));
    assert!(app.status.as_deref().unwrap().contains("--preview"));
    app.status = None;
    let generation = app.automation_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::Wake(Wake::AutomationTick {
            generation,
            now_unix: 10_000,
            weekday: 1,
            minute_of_day: 600,
        }),
    );
    assert!(app.status.as_deref().unwrap().contains("--preview"));
    assert_eq!(app.next_slideshow_unix, 1);
    assert_eq!(app.last_automation_minute, None);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn p1_preview_focus_loss_preserves_visibility_and_saved_automation_intent() {
    let (mut app, _) = d3_application_fixture(1);
    app.launch_policy = p1_preview_policy();
    app.config.ui.close_on_focus_loss = true;
    app.state.slideshow_enabled = true;
    app.state.playlists.push(candidate_state::Playlist {
        id: "p1-playlist".into(),
        name: "Saved active playlist".into(),
        order: Default::default(),
        membership: candidate_state::PlaylistMembership::Explicit {
            wallpaper_ids: vec![app.wallpapers[0].id.clone()],
        },
    });
    app.state.active_playlist_id = Some("p1-playlist".into());
    app.state.schedules.push(candidate_state::Schedule {
        id: "p1-schedule".into(),
        name: "Saved enabled schedule".into(),
        playlist_id: "p1-playlist".into(),
        days: 127,
        minute_of_day: 600,
        enabled: true,
        ..Default::default()
    });
    let saved = app.state.clone();
    let generation = app.automation_generation.load(Ordering::Acquire);
    app.refresh_automation_timer();
    assert!(app.automation_generation.load(Ordering::Acquire) > generation);
    assert_eq!(app.state, saved);
    let id = app.window.unwrap();
    let _ = update(&mut app, Message::WindowUnfocused(id));
    assert!(app.visible);
    assert_eq!(app.window, Some(id));
    assert!(!app.exit_requested);
    assert!(app.config.ui.close_on_focus_loss);
    assert_eq!(app.state, saved);
    app.state.save(&app.state_path).unwrap();
    let reloaded = CandidateState::load(&app.state_path);
    assert!(reloaded.slideshow_enabled && reloaded.schedules[0].enabled);
}

#[test]
fn p1_normal_close_keeps_the_existing_hide_contract() {
    let (mut app, _) = d3_application_fixture(0);
    let id = app.window.unwrap();
    let _ = update(&mut app, Message::CloseRequested(id));
    assert!(!app.visible);
    assert!(!app.exit_requested);
}

#[test]
fn p1_marked_preview_duplicate_uses_the_existing_non_signalling_guard() {
    let (app, _) = d3_application_fixture(0);
    let profile =
        runtime::fresh_preview_profile(&app.state_path.parent().unwrap().join("guard-profiles"))
            .unwrap();
    let policy = p1_preview_policy();
    runtime::resolve_launch_profile(&profile, policy).unwrap();
    let guard = crate::candidate_instance::CandidateInstanceGuard::acquire(
        runtime::resolve_launch_profile(&profile, policy).unwrap(),
        policy.requests_reopen(),
    )
    .unwrap();
    assert!(matches!(
        crate::candidate_instance::CandidateInstanceGuard::acquire(
            runtime::resolve_launch_profile(&profile, policy).unwrap(),
            policy.requests_reopen()
        ),
        Err(crate::candidate_instance::CandidateInstanceError::AlreadyRunning)
    ));
    drop(guard);
    let _released = crate::candidate_instance::CandidateInstanceGuard::acquire(
        runtime::resolve_launch_profile(&profile, policy).unwrap(),
        policy.requests_reopen(),
    )
    .unwrap();
}

#[test]
fn s2_first_use_and_alias_markers_preserve_mode_and_no_signal_policy() {
    let (app, _) = d3_application_fixture(0);
    let root = app.state_path.parent().unwrap().join("s2-first-use");
    let profile = root.join("new-normal");
    assert!(!profile.exists());
    let normal = runtime::parse_launch_policy(&["candidate.exe".into(), "--show".into()]).unwrap();
    let identity = runtime::resolve_launch_profile(&profile, normal).unwrap();
    assert!(identity.canonical_path().is_absolute());
    assert!(profile.is_dir());
    assert!(!profile.join("candidate-launch-mode.toml").exists());
    std::fs::write(profile.join("config.toml"), b"preserved local intent").unwrap();
    // Existing v1 normal/preview markers are accepted only through the same
    // resolved physical directory; no marker migration or lexical lock is used.
    std::fs::write(
        profile.join("candidate-launch-mode.toml"),
        b"version = 1\nmode = 'normal'\n",
    )
    .unwrap();
    let alias = root.join("normal-junction");
    assert!(std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&alias)
        .arg(&profile)
        .status()
        .unwrap()
        .success());
    let mut owner = crate::candidate_instance::CandidateInstanceGuard::acquire(
        identity,
        normal.requests_reopen(),
    )
    .unwrap();
    let (tx, rx) = std::sync::mpsc::channel();
    owner
        .start_reopen_listener(move || {
            tx.send(()).unwrap();
        })
        .unwrap();
    assert!(runtime::resolve_launch_profile(&alias, p1_preview_policy()).is_err());
    let developer =
        runtime::parse_launch_policy(&["candidate.exe".into(), "--benchmark".into()]).unwrap();
    let dev_identity = runtime::resolve_launch_profile(&alias, developer).unwrap();
    assert!(matches!(
        crate::candidate_instance::CandidateInstanceGuard::acquire(
            dev_identity,
            developer.requests_reopen()
        ),
        Err(crate::candidate_instance::CandidateInstanceError::AlreadyRunning)
    ));
    assert!(rx
        .recv_timeout(std::time::Duration::from_millis(150))
        .is_err());
    let duplicate = runtime::resolve_launch_profile(&alias, normal).unwrap();
    assert!(matches!(
        crate::candidate_instance::CandidateInstanceGuard::acquire(
            duplicate,
            normal.requests_reopen()
        ),
        Err(crate::candidate_instance::CandidateInstanceError::AlreadyRunning)
    ));
    rx.recv_timeout(std::time::Duration::from_secs(2))
        .expect("allowed normal reopen");
    drop(owner);
    assert_eq!(
        std::fs::read(profile.join("config.toml")).unwrap(),
        b"preserved local intent"
    );
    let preview = runtime::fresh_preview_profile(&root.join("preview")).unwrap();
    let preview_alias = root.join("preview-junction");
    assert!(std::process::Command::new("cmd.exe")
        .args(["/d", "/c", "mklink", "/J"])
        .arg(&preview_alias)
        .arg(&preview)
        .status()
        .unwrap()
        .success());
    let preview_owner = crate::candidate_instance::CandidateInstanceGuard::acquire(
        runtime::resolve_launch_profile(&preview, p1_preview_policy()).unwrap(),
        false,
    )
    .unwrap();
    assert!(runtime::resolve_launch_profile(&preview_alias, normal).is_err());
    assert!(runtime::resolve_launch_profile(&preview_alias, developer).is_err());
    assert!(matches!(
        crate::candidate_instance::CandidateInstanceGuard::acquire(
            runtime::resolve_launch_profile(&preview_alias, p1_preview_policy()).unwrap(),
            false
        ),
        Err(crate::candidate_instance::CandidateInstanceError::AlreadyRunning)
    ));
    drop(preview_owner);
}

// Exercise the production consumer, substituting only its native/effect
// boundaries. No candidate boot, timer, runtime channel or provider runs.
struct AutomationFixture {
    generation: AtomicU64,
    state: CandidateState,
    minute: Option<(u8, u16)>,
    due: u64,
    effects: [usize; 5], // monitors, evaluator, playlist, request, provider
    pending: bool,
    unknown: bool,
    apply_enabled: bool,
    selected: Option<String>,
    submissions: Vec<(String, Vec<String>, String)>,
}

impl AutomationFixture {
    fn new() -> Self {
        let mut state = CandidateState::default();
        state.playlists.push(candidate_state::Playlist {
            id: "daily".into(),
            name: "Daily".into(),
            order: candidate_state::PlaybackOrder::Sequential,
            membership: candidate_state::PlaylistMembership::Explicit {
                wallpaper_ids: vec!["a".into(), "b".into()],
            },
        });
        state.active_playlist_id = Some("daily".into());
        state.schedules.push(candidate_state::Schedule {
            id: "morning".into(),
            enabled: true,
            days: 1 << 2,
            minute_of_day: 480,
            playlist_id: "daily".into(),
            ..Default::default()
        });
        Self {
            generation: AtomicU64::new(7),
            state,
            minute: None,
            due: 900,
            effects: [0; 5],
            pending: false,
            unknown: false,
            apply_enabled: true,
            selected: None,
            submissions: Vec::new(),
        }
    }

    fn tick(generation: u64) -> Wake {
        Wake::AutomationTick {
            generation,
            now_unix: 1_000,
            weekday: 2,
            minute_of_day: 480,
        }
    }

    fn deliver(&mut self, tick: Wake) {
        let Wake::AutomationTick {
            generation,
            now_unix,
            weekday,
            minute_of_day,
        } = tick
        else {
            panic!("expected automation event");
        };
        consume_automation_tick(self, generation, now_unix, weekday, minute_of_day);
    }

    fn refresh(&mut self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }
}

impl AutomationTickContext for AutomationFixture {
    fn stop_interval(&mut self) {
        if self.state.automation_intent().safely_stopped {
            return;
        }
        let mut intent = self.state.automation_intent();
        intent.playlist_playing = false;
        intent.safely_stopped = true;
        self.state.automation_intent = Some(intent);
        self.refresh();
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }
    fn state(&self) -> &CandidateState {
        &self.state
    }
    fn last_minute(&mut self) -> &mut Option<(u8, u16)> {
        &mut self.minute
    }
    fn next_due(&mut self) -> &mut u64 {
        &mut self.due
    }
    fn monitors(&mut self) -> Result<Vec<String>, String> {
        self.effects[0] += 1;
        Ok(vec!["monitor1".into()])
    }
    fn evaluate_schedule(
        &mut self,
        now_unix: u64,
        weekday: u8,
        minute_of_day: u16,
        available: &[String],
    ) -> automation::ScheduleEvaluation {
        self.effects[1] += 1;
        automation::evaluate_schedule(&self.state, now_unix, weekday, minute_of_day, available)
    }
    fn run_playlist(
        &mut self,
        playlist_id: Option<&str>,
        monitor_ids: Option<Vec<String>>,
        trace_context: AutomationTraceContext<'_>,
    ) -> bool {
        self.effects[2] += 1;
        if automation_submission_block(self.pending, self.unknown, self.apply_enabled).is_some() {
            return false;
        }
        let Some(playlist) = self
            .state
            .playlists
            .iter()
            .find(|p| Some(p.id.as_str()) == playlist_id)
        else {
            return false;
        };
        let catalog: Vec<_> = ["a", "b"]
            .into_iter()
            .map(|id| library::CatalogEntry {
                id: id.into(),
                title: id.into(),
                kind: "image".into(),
                folder: std::path::PathBuf::from("fixture"),
                width: 1920,
                height: 1080,
                modified_unix: 0,
                source_tags: Default::default(),
                tags: Default::default(),
            })
            .collect();
        let Some(next) = automation::next_playlist_wallpaper(
            playlist,
            &catalog,
            &self.state.favorites,
            self.selected.as_deref(),
            0,
        ) else {
            return false;
        };
        let targets = monitor_ids.unwrap_or_else(|| {
            vec!["monitor1".into()]
                .into_iter()
                .filter(|id| !self.state.locked_monitors.contains(id))
                .collect()
        });
        if targets.is_empty() {
            return false;
        }
        self.effects[3] += 1;
        self.effects[4] += 1;
        self.submissions
            .push((next.clone(), targets, trace_context.source.into()));
        self.selected = Some(next);
        true
    }
}

// Owned source-only application fixture. No bootstrap, native hooks,
// catalog scan, runtime channels, timers, or provider request is installed.
fn d3_application_fixture(count: usize) -> (GpuCandidate, Arc<std::sync::atomic::AtomicUsize>) {
    static FIXTURE_ID: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "d3-app-{}-{}",
        std::process::id(),
        FIXTURE_ID.fetch_add(1, Ordering::AcqRel)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let mut state = CandidateState::default();
    state.gallery.mode = GalleryKind::Depth;
    let submissions = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let completed = submissions.clone();
    let mut app = GpuCandidate {
        config: Config::default(),
        state: state,
        state_path: root.join("state.toml"),
        wallpapers: Default::default(),
        catalog: Default::default(),
        filtered_indices: Default::default(),
        presentation_inputs: None,
        source_filter: SourceFilter::All,
        filter_rule: Default::default(),
        sort_order: SortOrder::Colour,
        colours: colours::State::default(),
        show_excluded: Default::default(),
        playlist_panel_open: Default::default(),
        playlist_browsing_id: None,
        playlist_chooser: false,
        playlist_query: String::new(),
        browse_playlist_id: None,
        browse_playlist_unavailable: false,
        engine_properties_open: Default::default(),
        engine_properties: Default::default(),
        engine_property_values: Default::default(),
        engine_properties_project: Default::default(),
        engine_properties_diagnostic: Default::default(),
        playlist_name: Default::default(),
        panel_state: Default::default(),
        theme_session: appearance_theme::ThemeSession::new(),
        theme_workflow: Default::default(),
        settings_search: Default::default(),
        settings_locale: Default::default(),
        settings_workbench: Default::default(),
        controls_panel_open: Default::default(),
        folder_menu_open: Default::default(),
        schedule_name: Default::default(),
        schedule_time: Default::default(),
        schedule_days: Default::default(),
        schedule_monitors: Default::default(),
        monitor_ids: Default::default(),
        active_monitor_id: Default::default(),
        selected: Default::default(),
        catalog_loaded: true,
        catalog_fresh: true,
        catalog_snapshot_issue: None,
        poster_failure: Default::default(),
        detail_open: Default::default(),
        status: Default::default(),
        load_diagnostic: Default::default(),
        config_load_diagnostic: Default::default(),
        save_failure: Default::default(),
        backup_warning: Default::default(),
        config_save_failure: Default::default(),
        provider_state_unknown: Default::default(),
        provider_recoveries: Default::default(),
        apply_pending: Default::default(),
        active_operation: Default::default(),
        wallpaper_change: None,
        startup_checked: false,
        startup_status: None,
        rotation_notice: None,
        detail_copy_generation: Arc::new(AtomicU64::new(0)),
        startup_queue: Default::default(),
        hide_after_apply: Default::default(),
        apply_enabled: Default::default(),
        launch_policy: Default::default(),
        exit_requested: Default::default(),
        visible: true,
        visibility_generation: Default::default(),
        native_visibility: Default::default(),
        window: Some(window::Id::unique()),
        window_focused: Default::default(),
        viewport: [1200.0, 900.0],
        presentation_started: std::time::Instant::now(),
        search_input_id: iced::widget::Id::unique(),
        local_tag_input: String::new(),
        tag_drawer_item: None,
        local_tag_draft_owner: None,
        playlist_picker: false,
        playlist_session: 0,
        playlist_context: None,
        retiring_playlist: None,
        playlist_delete: None,
        tag_editor_context: None,
        tag_edit_session: 0,
        local_tag_input_id: iced::widget::Id::unique(),
        folder_path_input: String::new(),
        library_config_reconcile: false,
        search_tags_open: Default::default(),
        search_tags_sort_az: Default::default(),
        poster_cache_dir: root.join("posters"),
        catalog_generation: Default::default(),
        catalog_scan_generation: Default::default(),
        catalog_watch: None,
        cache_operation: None,
        cache_status: String::new(),
        posters: Default::default(),
        poster_slots: Default::default(),
        poster_failed_indices: Default::default(),
        poster_retry: Default::default(),
        poster_request_key: Default::default(),
        poster_assignment_key: None,
        poster_generation: Default::default(),
        poster_worker: Default::default(),
        poster_mailbox: Default::default(),
        motion_mailbox: Default::default(),
        backdrop: Default::default(),
        backdrop_source: Default::default(),
        backdrop_generation: Default::default(),
        motion_generation: Default::default(),
        motion: Default::default(),
        motion_velocity: Default::default(),
        depth_camera: Default::default(),
        gallery_presentation: None,
        hex_presentation: None,
        wall_presentation: None,
        scene_motion: super::gallery_presentation::SceneMotion::default(),
        browse_position_restored: false,
        gallery_engaged: false,
        gallery_keyboard: false,
        hex_keyboard: true,
        poster_loading: false,
        depth_selection: Default::default(),
        depth_hover: Default::default(),
        depth_wheel: Default::default(),
        preset_wheel: None,
        depth_revision: Default::default(),
        depth_frame: Default::default(),
        wall_colour_hold: None,
        depth_presentation: Default::default(),
        deterministic_animation: false,
        view_legacy_lookups: Default::default(),
        detail_generation: Default::default(),
        rail_generation: Default::default(),
        rail_dismissed: false,
        rail_reveal: Default::default(),
        detail_progress: Default::default(),
        detail_source: None,
        depth_detail: Default::default(),
        depth_detail_id: Default::default(),
        depth_detail_mode: Default::default(),
        wall_scroll_offset: Default::default(),
        automation_generation: Default::default(),
        last_automation_minute: Default::default(),
        next_slideshow_unix: Default::default(),
        automation_suspended: false,
        gallery_mode: GalleryMode::Depth,
        atlas_entry: None,
        benchmark: Default::default(),
        scenario: Default::default(),
        screenshot_path: Default::default(),
        screenshot_requested: Default::default(),
        screenshot_detail: Default::default(),
        screenshot_detail_progress: Default::default(),
        screenshot_index: Default::default(),
        screenshot_panel: Default::default(),
        screenshot_engine_schema: Default::default(),
        _hotkey: Default::default(),
        _tray: Default::default(),
        coordinator: WallpaperCoordinator::spawn(move |_| {
            completed.fetch_add(1, Ordering::AcqRel);
        }),
    };
    app.wallpapers = (0..count)
        .map(|i| test_wallpaper(&format!("item-{i:04}")))
        .collect();
    app.catalog = app
        .wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    app.filtered_indices = (0..count).collect();
    app.selected = (count > 0).then_some(0);
    app.reset_depth_transients();
    app.rebuild_depth_frame();
    (app, submissions)
}
fn d3_key(app: &mut GpuCandidate, named: keyboard::key::Named) {
    let _ = update(
        app,
        Message::KeyPressed {
            key: keyboard::Key::Named(named),
            modifiers: keyboard::Modifiers::empty(),
            event: NavigationKeyEvent {
                event_window: app.window.unwrap(),
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                repeat: false,
            },
        },
    );
}

// ---- Session theme integration (production update/consumers) ----
//
// Sentinel document from the verification plan: dark index i uses
// decimal (10+i, 70+i, 130+i), light uses (160+i, 210-i, 60+i),
// each rendered as two-digit hex. Role order below is transcribed
// independently in this test module.
const THEME_SENTINEL_KEYS: [&str; 50] = [
    "primary",
    "on_primary",
    "tertiary",
    "surface",
    "on_surface",
    "surface_variant",
    "surface_container",
    "background",
    "outline",
    "error",
    "error_container",
    "inverse_on_surface",
    "inverse_primary",
    "inverse_surface",
    "on_background",
    "on_error",
    "on_error_container",
    "on_primary_container",
    "on_primary_fixed",
    "on_primary_fixed_variant",
    "on_secondary",
    "on_secondary_container",
    "on_secondary_fixed",
    "on_secondary_fixed_variant",
    "on_surface_variant",
    "on_tertiary",
    "on_tertiary_container",
    "on_tertiary_fixed",
    "on_tertiary_fixed_variant",
    "outline_variant",
    "primary_container",
    "primary_fixed",
    "primary_fixed_dim",
    "scrim",
    "secondary",
    "secondary_container",
    "secondary_fixed",
    "secondary_fixed_dim",
    "shadow",
    "surface_bright",
    "surface_container_high",
    "surface_container_highest",
    "surface_container_low",
    "surface_container_lowest",
    "surface_dim",
    "surface_tint",
    "tertiary_container",
    "tertiary_fixed",
    "tertiary_fixed_dim",
    "source_color",
];

fn theme_sentinel_text() -> String {
    let mut colors = serde_json::Map::new();
    for (index, key) in THEME_SENTINEL_KEYS.iter().enumerate() {
        let dark = format!("#{:02x}{:02x}{:02x}", 10 + index, 70 + index, 130 + index);
        let light = format!("#{:02x}{:02x}{:02x}", 160 + index, 210 - index, 60 + index);
        colors.insert(
            (*key).to_string(),
            serde_json::json!({
                "dark": { "color": dark },
                "light": { "color": light },
            }),
        );
    }
    serde_json::json!({ "colors": colors }).to_string()
}

fn theme_sentinel_rgb(index: usize, light: bool) -> [f32; 3] {
    let (r, g, b) = if light {
        (160 + index, 210 - index, 60 + index)
    } else {
        (10 + index, 70 + index, 130 + index)
    };
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

fn theme_open_app() -> GpuCandidate {
    let (mut app, _submissions) = d3_application_fixture(0);
    app.panel_state.settings_open = true;
    app.panel_state.settings_category = SettingsCategory::Theme;
    app
}

#[test]
fn live_overlay_clear_and_background_layers_use_production_policy() {
    let mut app = theme_open_app();
    app.backdrop = Some(iced_image::Handle::from_rgba(1, 1, vec![255; 4]));
    assert_eq!(
        application_style(&app, &theme(&app, window::Id::unique()))
            .background_color
            .a,
        1.0
    );
    assert_eq!(
        iced::advanced::Widget::children(&gallery_background_layers(&app)).len(),
        2
    );
    app.state.appearance.presentation = BackgroundPresentation::LiveTransparentOverlay;
    assert_eq!(
        application_style(&app, &theme(&app, window::Id::unique())).background_color,
        Color::TRANSPARENT
    );
    assert_eq!(
        iced::advanced::Widget::children(&gallery_background_layers(&app)).len(),
        0
    );
}

#[test]
fn live_overlay_backdrop_click_retains_input_and_residency() {
    let (mut app, submissions) = d3_application_fixture(0);
    app.visible = true;
    app.config.ui.close_on_backdrop_click = true;
    app.state.appearance.presentation = BackgroundPresentation::LiveTransparentOverlay;
    let _ = update(
        &mut app,
        Message::Gallery(GalleryMessage::BackdropClicked(None)),
    );
    assert!(app.visible);
    assert!(!app.exit_requested);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
    app.state.appearance.presentation = BackgroundPresentation::WallpaperBackdrop;
    let _ = update(
        &mut app,
        Message::Gallery(GalleryMessage::BackdropClicked(None)),
    );
    assert!(!app.visible);
}

#[test]
fn live_overlay_stale_backdrop_cannot_install_after_switch() {
    let mut app = theme_open_app();
    let generation = app.backdrop_generation.load(Ordering::Acquire);
    app.state.appearance.presentation = BackgroundPresentation::LiveTransparentOverlay;
    let _ = update(
        &mut app,
        Message::Wake(Wake::BackdropLoaded {
            generation,
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        }),
    );
    assert!(app.backdrop.is_none());
}

#[test]
fn live_overlay_switch_invalidates_work_preserves_appearance_and_restores_wallpaper() {
    let mut app = theme_open_app();
    let original = app.state.appearance.clone();
    app.backdrop_source = Some("old.png".into());
    app.backdrop = Some(iced_image::Handle::from_rgba(1, 1, vec![255; 4]));
    let stale = app.backdrop_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::SetBackgroundPresentation(BackgroundPresentation::LiveTransparentOverlay),
    );
    assert!(app.backdrop.is_none());
    assert!(app.backdrop_source.is_none());
    assert!(app.backdrop_source_paths().is_empty());
    assert_ne!(app.backdrop_generation.load(Ordering::Acquire), stale);
    let _ = update(
        &mut app,
        Message::SetBackgroundPresentation(BackgroundPresentation::WallpaperBackdrop),
    );
    assert_eq!(app.state.appearance, original);
    let current = app.backdrop_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::Wake(Wake::BackdropLoaded {
            generation: stale,
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        }),
    );
    assert!(app.backdrop.is_none());
    let _ = update(
        &mut app,
        Message::Wake(Wake::BackdropLoaded {
            generation: current,
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        }),
    );
    assert!(app.backdrop.is_some());
    let loaded = CandidateState::load(&app.state_path);
    assert_eq!(loaded.appearance, original);
}

#[test]
fn live_overlay_x_escape_hide_and_reopen_keep_mode_and_shutdown_owner() {
    let mut app = theme_open_app();
    app.visible = true;
    app.state.appearance.presentation = BackgroundPresentation::LiveTransparentOverlay;
    let token = app.theme_session.session_token();
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(app.panel_state.settings_open && app.visible);
    assert!(app.settings_workbench.windows_panel.is_none());
    assert!(!lifecycle::theme_owner_active(&app));
    assert_ne!(app.theme_session.session_token(), token);
    assert!(!app.exit_requested && !app.apply_pending);
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.panel_state.settings_open);
    assert!(app.visible);
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.visible);
    assert!(!app.exit_requested);
    let _ = app.set_visible(true, "live-overlay-test-reopen");
    assert!(app.visible);
    assert_eq!(
        app.state.appearance.presentation,
        BackgroundPresentation::LiveTransparentOverlay
    );
    let _ = update(&mut app, Message::DonorRail(donor_rail::RailIntent::Close));
    assert!(!app.visible);
    assert!(!app.exit_requested);
    assert_eq!(window_settings(None).exit_on_close_request, false);
    app.visible = true;
    let id = app.window.unwrap();
    let _ = update(&mut app, Message::CloseRequested(id));
    assert!(!app.visible);
    assert!(!app.exit_requested);
}

#[test]
fn live_overlay_future_state_refuses_presentation_write() {
    let mut app = theme_open_app();
    app.state.version = candidate_state::CURRENT_STATE_VERSION + 1;
    let future = toml::to_string(&app.state).unwrap();
    std::fs::write(&app.state_path, &future).unwrap();
    let _ = update(
        &mut app,
        Message::SetBackgroundPresentation(BackgroundPresentation::LiveTransparentOverlay),
    );
    assert_eq!(std::fs::read_to_string(&app.state_path).unwrap(), future);
    assert!(app.state.write_restriction.is_some());
    assert!(app.status.as_ref().unwrap().contains("not fully saved"));
}

// Send helpers capture the revision/token values into locals
// before calling `update`, so no call ever borrows `app` mutably
// while reading `app.theme_session` in the same expression.
fn theme_variant(app: &mut GpuCandidate, variant: theme_roles::ColorVariant) {
    let editor_instance = app.theme_session.editor_instance().unwrap();
    let _ = update(
        app,
        Message::ThemeVariantSelected {
            editor_instance,
            variant,
        },
    );
}

fn theme_load_fixture(app: &mut GpuCandidate, variant: theme_roles::ColorVariant) {
    let imported =
        theme_roles::import_document(&serde_json::from_str(&theme_sentinel_text()).unwrap())
            .unwrap();
    app.theme_session
        .load_designer(imported.document, variant, "Sentinel fixture".into(), None);
}

fn theme_save_apply(app: &mut GpuCandidate) {
    theme_workflow::designer_save(app, true);
}

#[test]
fn theme_integration_applied_snapshot_routes_to_palette_snapshot_theme_and_backdrop() {
    let mut app = theme_open_app();
    // Legacy baseline first: the saved Nord background reaches the
    // production palette when no session selection exists.
    assert_eq!(
        donor_ui::rgb_components(donor_palette(&app).background),
        [
            0x2e as f32 / 255.0,
            0x34 as f32 / 255.0,
            0x40 as f32 / 255.0
        ]
    );
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    theme_workflow::designer_save(&mut app, true);
    assert!(app.theme_session.editor_active());

    let palette = donor_palette(&app);
    let expect = |index: usize| theme_sentinel_rgb(index, false);
    assert_eq!(donor_ui::rgb_components(palette.primary), expect(0));
    assert_eq!(donor_ui::rgb_components(palette.primary_text), expect(1));
    assert_eq!(donor_ui::rgb_components(palette.tertiary), expect(2));
    assert_eq!(donor_ui::rgb_components(palette.surface), expect(3));
    assert_eq!(donor_ui::rgb_components(palette.surface_text), expect(4));
    assert_eq!(donor_ui::rgb_components(palette.surface_variant), expect(5));
    assert_eq!(
        donor_ui::rgb_components(palette.surface_container),
        expect(6)
    );
    assert_eq!(donor_ui::rgb_components(palette.background), expect(7));
    assert_eq!(donor_ui::rgb_components(palette.outline), expect(8));

    // The production snapshot builder used by view: document
    // surface/primary win; unrelated fields keep legacy values.
    let snapshot = gallery_snapshot_colors(&app);
    assert_eq!(snapshot.surface, expect(3));
    assert_eq!(snapshot.accent, expect(0));
    let legacy_snapshot = gallery_snapshot_colors(&theme_open_app());
    assert_eq!(
        snapshot.unselected_brightness,
        legacy_snapshot.unselected_brightness
    );
    assert_eq!(
        snapshot.selection_border_width,
        legacy_snapshot.selection_border_width
    );
    assert_eq!(
        snapshot.show_selection_border,
        legacy_snapshot.show_selection_border
    );
    assert_eq!(snapshot.border_glow, legacy_snapshot.border_glow);
    assert_eq!(
        snapshot.border_glow_radius,
        legacy_snapshot.border_glow_radius
    );

    // The actual Iced theme callback: effective background/text/
    // primary, success follows primary, warning/danger stay fixed.
    let iced_theme = theme(&app, window::Id::unique());
    assert_eq!(
        iced_theme.palette().background,
        Color::from_rgb(expect(7)[0], expect(7)[1], expect(7)[2])
    );
    assert_eq!(
        iced_theme.palette().text,
        Color::from_rgb(expect(4)[0], expect(4)[1], expect(4)[2])
    );
    assert_eq!(
        iced_theme.palette().primary,
        Color::from_rgb(expect(0)[0], expect(0)[1], expect(0)[2])
    );
    assert_eq!(iced_theme.palette().success, iced_theme.palette().primary);
    assert_eq!(
        iced_theme.palette().warning,
        Color::from_rgb(0.94, 0.72, 0.37)
    );
    assert_eq!(
        iced_theme.palette().danger,
        Color::from_rgb(0.90, 0.36, 0.44)
    );

    // The production backdrop builder used by view.
    assert_eq!(backdrop_background_rgb(&app), expect(7));

    // Re-auditioning the same unchanged text at Light replaces the
    // dark projection everywhere.
    theme_variant(&mut app, theme_roles::ColorVariant::Light);
    theme_workflow::designer_save(&mut app, true);
    assert_eq!(
        donor_ui::rgb_components(donor_palette(&app).primary),
        theme_sentinel_rgb(0, true)
    );
    assert_eq!(backdrop_background_rgb(&app), theme_sentinel_rgb(7, true));
}

fn theme_expect_color(index: usize, light: bool, alpha: f32) -> Color {
    let [r, g, b] = theme_sentinel_rgb(index, light);
    Color::from_rgba(r, g, b, alpha)
}

#[test]
fn theme_integration_styles_and_rail_palette_follow_applied_document() {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    theme_workflow::designer_save(&mut app, true);
    let palette = donor_palette(&app);
    let expect = |index: usize, alpha: f32| theme_expect_color(index, false, alpha);

    // on_primary (1) and background (7) stay distinct: the first
    // drives primary-action text, the second drives rail active
    // labels, and they are different sentinel colors.
    assert_ne!(palette.primary_text, palette.background);

    // Real donor_ui action style: active / hovered / idle /
    // disabled / destructive colors and alphas from the document.
    let active = donor_ui::action_style(palette, true, false, button::Status::Active);
    assert_eq!(active.background, Some(expect(0, 0.92).into()));
    assert_eq!(active.text_color, expect(1, 1.0));
    assert_eq!(active.border.color, expect(0, 0.9));
    let hovered = donor_ui::action_style(palette, false, false, button::Status::Hovered);
    assert_eq!(hovered.background, Some(expect(5, 0.62).into()));
    assert_eq!(hovered.text_color, expect(4, 1.0));
    assert_eq!(hovered.border.color, expect(0, 0.9));
    let idle = donor_ui::action_style(palette, false, false, button::Status::Active);
    assert_eq!(idle.background, Some(expect(6, 0.92).into()));
    assert_eq!(idle.text_color, expect(4, 1.0));
    assert_eq!(idle.border.color, expect(8, 0.9));
    let disabled = donor_ui::action_style(palette, false, false, button::Status::Disabled);
    assert_eq!(disabled.background, None);
    assert_eq!(disabled.text_color, expect(4, 0.34));
    let destructive = donor_ui::action_style(palette, true, true, button::Status::Active);
    assert_eq!(destructive.background, Some(expect(2, 0.92).into()));
    assert_eq!(destructive.text_color, expect(1, 1.0));
    let destructive_idle = donor_ui::action_style(palette, false, true, button::Status::Active);
    assert_eq!(destructive_idle.text_color, expect(2, 1.0));

    // Real donor_ui field input style.
    let field = donor_ui::field_input_style(palette);
    assert_eq!(field.background, expect(6, 0.88).into());
    assert_eq!(field.border.color, expect(8, 0.48));
    assert_eq!(field.placeholder, expect(4, 0.35));
    assert_eq!(field.value, expect(4, 1.0));
    assert_eq!(field.selection, expect(0, 0.4));

    // Real donor_ui frame (panel) style used by production sheets.
    let frame = donor_ui::panel_style(palette, 0.96);
    assert_eq!(frame.background, Some(expect(3, 0.96).into()));
    assert_eq!(frame.border.color, expect(8, 0.58));

    // Real donor_ui rail chip style: active / hovered / disabled.
    let chip_active = donor_ui::rail_style(palette, true, button::Status::Active);
    assert_eq!(chip_active.background, Some(expect(0, 0.46).into()));
    assert_eq!(chip_active.border.color, expect(0, 0.86));
    let chip_hovered = donor_ui::rail_style(palette, false, button::Status::Hovered);
    assert_eq!(chip_hovered.background, Some(expect(6, 0.66).into()));
    assert_eq!(chip_hovered.border.color, expect(8, 0.34));
    let chip_disabled = donor_ui::rail_style(palette, false, button::Status::Disabled);
    assert_eq!(
        chip_disabled.background,
        Some(expect(6, 0.46 * 0.45).into())
    );
    assert_eq!(chip_disabled.text_color, expect(4, 0.42));

    // Production RailPalette conversion (the helper view calls):
    // Field mapping plus smoothstep reveal. R3 restores source on-primary
    // labels and the distinct idle/hover surface roles.
    let rail = rail_palette(palette, 1.0);
    assert_eq!(rail.background, expect(7, 1.0));
    assert_eq!(rail.surface, expect(3, 1.0));
    assert_eq!(rail.container, expect(6, 1.0));
    assert_eq!(rail.hover, expect(5, 1.0));
    assert_eq!(rail.active_text, expect(1, 1.0));
    assert_eq!(rail.text, expect(4, 1.0));
    assert_eq!(rail.accent, expect(0, 1.0));
    assert_eq!(rail.outline, expect(8, 1.0));
    assert_ne!(rail.background, rail.accent);
    let rail_half = rail_palette(palette, 0.5);
    assert_eq!(rail_half.background, expect(7, 0.5));
    assert_eq!(rail_half.accent, expect(0, 0.5));
    let rail_hidden = rail_palette(palette, 0.0);
    assert_eq!(rail_hidden.background, expect(7, 0.0));

    // Light variant: the same production styles and conversion
    // follow the light document values.
    theme_variant(&mut app, theme_roles::ColorVariant::Light);
    theme_workflow::designer_save(&mut app, true);
    let palette = donor_palette(&app);
    let expect_light = |index: usize, alpha: f32| theme_expect_color(index, true, alpha);
    let active = donor_ui::action_style(palette, true, false, button::Status::Active);
    assert_eq!(active.background, Some(expect_light(0, 0.92).into()));
    assert_eq!(active.text_color, expect_light(1, 1.0));
    let field = donor_ui::field_input_style(palette);
    assert_eq!(field.selection, expect_light(0, 0.4));
    let rail = rail_palette(palette, 1.0);
    assert_eq!(rail.background, expect_light(7, 1.0));
    assert_eq!(rail.accent, expect_light(0, 1.0));
}

#[test]
fn theme_integration_save_apply_cancel_and_restart_drive_consumers() {
    let mut app = theme_open_app();
    let before = donor_palette(&app);
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    theme_workflow::designer_save(&mut app, false);
    assert_eq!(donor_palette(&app).surface, before.surface);
    theme_workflow::designer_save(&mut app, true);
    assert_eq!(
        donor_ui::rgb_components(donor_palette(&app).surface),
        theme_sentinel_rgb(3, false)
    );
    theme_editor_set(&mut app, "#010203");
    let binding = crate::candidate_app::theme_workflow::Binding::current(&app).unwrap();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(crate::candidate_app::theme_workflow::Action::Load {
            binding,
            target: crate::candidate_app::theme_workflow::Load::Starter("nord"),
        }),
    );
    theme_workflow::designer_decide(
        &mut app,
        crate::candidate_app::theme_workflow::Choice::Cancel,
    );
    assert_eq!(
        donor_ui::rgb_components(donor_palette(&app).surface),
        theme_sentinel_rgb(3, false)
    );
    assert!(app.theme_session.designer_dirty());
    let restart = CandidateState::load(&app.state_path);
    assert_eq!(restart.applied_theme, app.state.applied_theme);
    // Explicit migrated legacy fixture, not an invented UseSaved UI action.
    let mut legacy = theme_open_app();
    legacy.state.applied_theme = None;
    assert_eq!(
        backdrop_background_rgb(&legacy),
        [46.0 / 255.0, 52.0 / 255.0, 64.0 / 255.0]
    );
}

fn theme_dirty_applied_app() -> GpuCandidate {
    let mut app = theme_clean_applied_app();
    theme_editor_set(&mut app, "#010203");
    assert!(app.theme_session.designer_dirty());
    app
}

fn assert_theme_draft_and_applied_kept(app: &GpuCandidate) {
    assert!(app.theme_session.editor_active());
    assert!(app.theme_session.designer_dirty());
    assert_eq!(
        app.theme_session.editor_document().unwrap().color(
            theme_roles::ThemeRole::Primary,
            theme_roles::ColorVariant::Dark
        ),
        "#010203"
    );
    assert_eq!(
        app.state.applied_theme.as_ref().unwrap().document.color(
            theme_roles::ThemeRole::Primary,
            theme_roles::ColorVariant::Dark
        ),
        "#0a4682"
    );
}

#[test]
fn theme_integration_departure_routes_prompt_and_hide_retains_draft() {
    use crate::candidate_app::theme_workflow::Choice;
    for route in 0..5 {
        let mut app = theme_dirty_applied_app();
        match route {
            0 => {
                let _ = update(
                    &mut app,
                    Message::SetSettingsCategory(SettingsCategory::Playback),
                );
            }
            1 => {
                let _ = update(&mut app, Message::TogglePlaylistPanel);
            }
            2 => {
                let _ = update(&mut app, Message::ToggleSettingsPanel);
            }
            3 => close_owned_sheet(&mut app, OwnedSheet::Settings),
            _ => d3_key(&mut app, keyboard::key::Named::Escape),
        }
        assert!(app.theme_workflow.prompt.is_some(), "route {route}");
        assert_theme_draft_and_applied_kept(&app);
        theme_workflow::designer_decide(&mut app, Choice::Cancel);
        assert!(lifecycle::theme_owner_active(&app));
        assert_theme_draft_and_applied_kept(&app);
    }
    let mut app = theme_dirty_applied_app();
    let token = app.theme_session.session_token();
    let _ = update(&mut app, Message::OpenSearch);
    assert!(app.panel_state.settings_open && !app.panel_state.search_open);
    assert!(app.theme_workflow.prompt.is_none());
    assert_eq!(app.theme_session.session_token(), token);
    app.playlist_panel_open = true;
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.playlist_panel_open);
    assert_theme_draft_and_applied_kept(&app);
    let token = app.theme_session.session_token();
    let _ = app.set_visible(false, "owned-theme-hide");
    assert_ne!(app.theme_session.session_token(), token);
    assert_theme_draft_and_applied_kept(&app);
}

#[test]
fn theme_integration_queued_visual_actions_rejected_hidden_and_reopened() {
    let mut app = theme_dirty_applied_app();
    let (i, g, r, v) = theme_editor_binding(&app);
    let binding = crate::candidate_app::theme_workflow::Binding::current(&app).unwrap();
    let applied = app.state.applied_theme.clone();
    let draft = app.theme_session.editor_document().unwrap().clone();
    let _ = app.set_visible(false, "owned-theme-hide");
    let before = app.theme_session.clone();
    let _ = update(
        &mut app,
        Message::ThemeRoleTextChanged {
            editor_instance: i,
            selection_generation: g,
            role: r,
            variant: v,
            text: "#abcdef".into(),
        },
    );
    let _ = update(
        &mut app,
        Message::ThemeDesigner(crate::candidate_app::theme_workflow::Action::Save {
            binding,
            apply: true,
        }),
    );
    assert_eq!(app.theme_session, before);
    assert_eq!(app.state.applied_theme, applied);
    let _ = app.set_visible(true, "owned-theme-reopen");
    let _ = update(
        &mut app,
        Message::ThemeRoleTextChanged {
            editor_instance: i,
            selection_generation: g,
            role: r,
            variant: v,
            text: "#abcdef".into(),
        },
    );
    assert_eq!(app.theme_session.editor_document(), Some(&draft));
    assert!(!app.theme_session.editor_pending());
    assert_eq!(
        donor_ui::rgb_components(donor_palette(&app).primary),
        theme_sentinel_rgb(0, false)
    );
}

fn theme_clean_applied_app() -> GpuCandidate {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    theme_workflow::designer_save(&mut app, true);
    assert!(!app.theme_session.designer_dirty());
    assert!(app.state.applied_theme.is_some());
    app
}

#[test]
fn theme_integration_stale_save_apply_after_clean_departure_is_rejected() {
    let mut app = theme_clean_applied_app();
    let binding = crate::candidate_app::theme_workflow::Binding::current(&app).unwrap();
    let before = app.state.clone();
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Playback),
    );
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Theme),
    );
    theme_editor_set(&mut app, "#abcdef");
    let dirty = app.theme_session.editor_document().unwrap().clone();
    let disk = std::fs::read(&app.state_path).unwrap();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(crate::candidate_app::theme_workflow::Action::Save {
            binding,
            apply: true,
        }),
    );
    assert_eq!(app.state, before);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), disk);
    assert_eq!(app.theme_session.editor_document(), Some(&dirty));
    assert!(app.theme_session.designer_dirty());
    assert_ne!(
        crate::candidate_app::theme_workflow::Binding::current(&app),
        Some(binding)
    );
    theme_editor_set(&mut app, "#abcdef");
    theme_workflow::designer_save(&mut app, true);
    assert_eq!(
        app.state.applied_theme.as_ref().unwrap().document.color(
            theme_roles::ThemeRole::Primary,
            theme_roles::ColorVariant::Dark
        ),
        "#abcdef"
    );
}

#[test]
fn theme_integration_stale_save_apply_after_clean_hide_is_rejected() {
    let mut app = theme_clean_applied_app();
    let binding = crate::candidate_app::theme_workflow::Binding::current(&app).unwrap();
    let before = app.state.clone();
    let _ = app.set_visible(false, "owned-clean-hide");
    let _ = app.set_visible(true, "owned-clean-reopen");
    theme_editor_set(&mut app, "#abcdef");
    let dirty = app.theme_session.editor_document().unwrap().clone();
    let disk = std::fs::read(&app.state_path).unwrap();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(crate::candidate_app::theme_workflow::Action::Save {
            binding,
            apply: true,
        }),
    );
    assert_eq!(app.state, before);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), disk);
    assert_eq!(app.theme_session.editor_document(), Some(&dirty));
    assert!(app.theme_session.designer_dirty());
    assert_ne!(
        crate::candidate_app::theme_workflow::Binding::current(&app),
        Some(binding)
    );
    theme_editor_set(&mut app, "#abcdef");
    theme_workflow::designer_save(&mut app, true);
    assert_eq!(
        app.state.applied_theme.as_ref().unwrap().document.color(
            theme_roles::ThemeRole::Primary,
            theme_roles::ColorVariant::Dark
        ),
        "#abcdef"
    );
}

fn theme_engine_item(app: &GpuCandidate, engine: bool) -> WallpaperItem {
    let root = app.state_path.parent().unwrap().to_path_buf();
    let project_json_path = root.join("engine-project.json");
    if engine {
        std::fs::write(
            &project_json_path,
            r#"{"general":{"properties":{"enabled":{"type":"bool","value":true,"order":1}}}}"#,
        )
        .unwrap();
    }
    WallpaperItem {
        scan_metadata: None,
        id: "theme-engine-fixture".into(),
        title: "Theme engine fixture".into(),
        folder_path: root.clone(),
        project_json_path,
        file_target: root.join("fixture.png"),
        preview_path: None,
        wallpaper_type: if engine {
            "scene".into()
        } else {
            "image".into()
        },
        source: if engine {
            WallpaperSource::WallpaperEngine
        } else {
            WallpaperSource::LocalImage
        },
    }
}

#[test]
fn theme_integration_engine_properties_open_invalidates_clean_session_token() {
    let mut app = theme_clean_applied_app();
    let item = theme_engine_item(&app, true);
    app.wallpapers.push(item);
    app.selected = Some(0);
    let binding = crate::candidate_app::theme_workflow::Binding::current(&app).unwrap();
    let before = app.state.clone();
    let token = app.theme_session.session_token();
    let _ = update(&mut app, Message::ToggleEngineProperties);
    assert!(app.engine_properties_open && !app.panel_state.settings_open);
    assert_ne!(app.theme_session.session_token(), token);
    let _ = update(&mut app, Message::ToggleEngineProperties);
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Theme),
    );
    theme_editor_set(&mut app, "#abcdef");
    let dirty = app.theme_session.editor_document().unwrap().clone();
    let disk = std::fs::read(&app.state_path).unwrap();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(crate::candidate_app::theme_workflow::Action::Save {
            binding,
            apply: true,
        }),
    );
    assert_eq!(app.state, before);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), disk);
    assert_eq!(app.theme_session.editor_document(), Some(&dirty));
    assert!(app.theme_session.designer_dirty());
    theme_save_apply(&mut app);
    assert_eq!(app.state.applied_theme.as_ref().unwrap().document, dirty);
}

#[test]
fn theme_integration_failed_engine_open_keeps_theme_ownership_and_counters() {
    for selected_engine in [None, Some(false), Some(true)] {
        let mut app = theme_dirty_applied_app();
        if let Some(engine) = selected_engine {
            let item = theme_engine_item(&app, engine);
            app.wallpapers.push(item);
            app.selected = Some(0);
            if engine {
                std::fs::write(&app.wallpapers[0].project_json_path, b"{}").unwrap();
            }
        }
        let before = app.theme_session.clone();
        let _ = update(&mut app, Message::ToggleEngineProperties);
        assert!(!app.engine_properties_open);
        assert!(app.theme_workflow.prompt.is_none());
        assert_eq!(app.theme_session, before);
        assert!(lifecycle::theme_owner_active(&app));
    }
}

#[test]
fn theme_integration_escape_closes_unrelated_category_without_touching_theme_counters() {
    let mut app = theme_clean_applied_app();
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Playback),
    );
    let before = app.theme_session.clone();
    let applied = app.state.applied_theme.clone();
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.panel_state.settings_open);
    assert_eq!(app.theme_session, before);
    assert_eq!(app.state.applied_theme, applied);
}

#[test]
fn theme_integration_escape_closing_clean_theme_invalidates_and_preserves_applied() {
    let mut app = theme_clean_applied_app();
    let applied = app.state.applied_theme.clone();
    let token = app.theme_session.session_token();
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(app.panel_state.settings_open && app.visible);
    assert!(app.settings_workbench.windows_panel.is_none());
    assert!(!lifecycle::theme_owner_active(&app));
    assert_ne!(app.theme_session.session_token(), token);
    let token = app.theme_session.session_token();
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.panel_state.settings_open);
    assert!(app.visible && !app.exit_requested && !app.apply_pending);
    assert_eq!(app.theme_session.session_token(), token);
    assert_eq!(app.state.applied_theme, applied);
}

#[test]
fn theme_integration_window_closed_invalidates_clean_session_token() {
    let mut app = theme_clean_applied_app();
    let id = app.window.unwrap();
    let binding = crate::candidate_app::theme_workflow::Binding::current(&app).unwrap();
    let before = app.state.clone();
    let token = app.theme_session.session_token();
    let _ = update(&mut app, Message::WindowClosed(id));
    assert_eq!(app.window, None);
    assert!(!app.visible);
    assert_ne!(app.theme_session.session_token(), token);
    assert_eq!(app.state, before);
    // Owner-predicate fixture restoration only, no native window/focus claim.
    app.window = Some(id);
    app.visible = true;
    theme_editor_set(&mut app, "#abcdef");
    let dirty = app.theme_session.editor_document().unwrap().clone();
    let disk = std::fs::read(&app.state_path).unwrap();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(crate::candidate_app::theme_workflow::Action::Save {
            binding,
            apply: true,
        }),
    );
    assert_eq!(app.state, before);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), disk);
    assert_eq!(app.theme_session.editor_document(), Some(&dirty));
    assert!(app.theme_session.designer_dirty());
    theme_save_apply(&mut app);
    assert_eq!(app.state.applied_theme.as_ref().unwrap().document, dirty);
}

#[test]
fn theme_integration_window_closed_retains_draft_and_applied() {
    let mut app = theme_dirty_applied_app();
    let id = app.window.unwrap();
    let draft = app.theme_session.editor_document().unwrap().clone();
    let applied = app.state.applied_theme.clone();
    let _ = update(&mut app, Message::WindowClosed(id));
    assert_eq!(app.window, None);
    assert!(!app.visible);
    assert_eq!(app.theme_session.editor_document(), Some(&draft));
    assert!(app.theme_session.designer_dirty());
    assert_eq!(app.state.applied_theme, applied);
}

#[test]
fn theme_integration_window_closed_with_mismatched_id_changes_nothing() {
    let mut app = theme_dirty_applied_app();
    let before = app.theme_session.clone();
    let owned = app.window;
    let _ = update(&mut app, Message::WindowClosed(window::Id::unique()));
    assert_eq!(app.window, owned);
    assert!(app.visible);
    assert_eq!(app.theme_session, before);
}

#[test]
fn theme_integration_window_closed_after_hide_does_not_advance_counters_again() {
    let mut app = theme_dirty_applied_app();
    let id = app.window.unwrap();
    let _ = app.set_visible(false, "owned-hide");
    let before = app.theme_session.clone();
    let _ = update(&mut app, Message::WindowClosed(id));
    assert_eq!(app.window, None);
    assert_eq!(app.theme_session, before);
    assert_theme_draft_and_applied_kept(&app);
}

fn theme_ensure_designer(app: &mut GpuCandidate) {
    app.ensure_theme_designer();
}

fn theme_editor_binding(
    app: &GpuCandidate,
) -> (u64, u64, theme_roles::ThemeRole, theme_roles::ColorVariant) {
    (
        app.theme_session.editor_instance().unwrap(),
        app.theme_session.editor_selection_generation().unwrap(),
        app.theme_session.editor_selected_role().unwrap(),
        app.theme_session.editor_selected_variant().unwrap(),
    )
}

fn theme_editor_select(
    app: &mut GpuCandidate,
    role: theme_roles::ThemeRole,
    variant: theme_roles::ColorVariant,
) {
    let (instance, ..) = theme_editor_binding(app);
    let _ = update(
        app,
        Message::ThemeRoleSelected {
            editor_instance: instance,
            role,
        },
    );
    let (instance, ..) = theme_editor_binding(app);
    let _ = update(
        app,
        Message::ThemeVariantSelected {
            editor_instance: instance,
            variant,
        },
    );
}

fn theme_editor_set(app: &mut GpuCandidate, text: &str) {
    let (instance, generation, role, variant) = theme_editor_binding(app);
    let _ = update(
        app,
        Message::ThemeRoleTextChanged {
            editor_instance: instance,
            selection_generation: generation,
            role,
            variant,
            text: text.to_string(),
        },
    );
    let (instance, generation, role, variant) = theme_editor_binding(app);
    let _ = update(
        app,
        Message::ThemeRoleSetColor {
            editor_instance: instance,
            selection_generation: generation,
            role,
            variant,
        },
    );
}

#[test]
fn theme_role_editor_starts_complete_nord_without_fabricating_legacy() {
    for legacy in [false, true] {
        let mut app = theme_open_app();
        if legacy {
            app.state.applied_theme = None;
        }
        let before = app.state.clone();
        app.ensure_theme_designer();
        assert_eq!(
            app.theme_session.editor_document(),
            Some(&candidate_state::frontend::nord_document())
        );
        assert_eq!(
            app.theme_session.editor_selected_role(),
            Some(theme_roles::ThemeRole::Primary)
        );
        assert_eq!(app.state, before);
        assert!(!app.state_path.exists());
    }
}

#[test]
fn theme_role_editor_hundred_cell_loop_through_update() {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    theme_ensure_designer(&mut app);
    let sentinel =
        theme_roles::import_document(&serde_json::from_str(&theme_sentinel_text()).unwrap())
            .unwrap()
            .document;
    let mut expected = sentinel.clone();
    for role in theme_roles::ThemeRole::ALL {
        for variant in theme_roles::ColorVariant::ALL {
            theme_editor_select(&mut app, role, variant);
            let value = format!(
                "#{:02x}{:02x}5a",
                role.index() as u8,
                match variant {
                    theme_roles::ColorVariant::Dark => 0x11,
                    theme_roles::ColorVariant::Light => 0x99,
                }
            );
            theme_editor_set(&mut app, &value);
            expected = expected.replaced(role, variant, &value);
            assert_eq!(
                app.theme_session.editor_document(),
                Some(&expected),
                "after setting {} {:?}",
                role.name(),
                variant
            );
        }
    }

    theme_variant(&mut app, theme_roles::ColorVariant::Light);
    theme_save_apply(&mut app);
    let snapshot = app.state.applied_theme.as_ref().unwrap();
    assert_eq!(snapshot.document, expected);
    assert_eq!(snapshot.variant, theme_roles::ColorVariant::Light);
    assert_eq!(
        CandidateState::load(&app.state_path).applied_theme,
        app.state.applied_theme
    );
    assert!(app.theme_session.editor_active());
    assert!(!app.theme_session.designer_dirty());
}

#[test]
fn theme_role_editor_streams_and_stale_bindings_through_update() {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    let (instance, generation, role, variant) = theme_editor_binding(&app);
    let document = app.theme_session.editor_document().unwrap().clone();
    for text in ["#0", "#01", "#010203"] {
        let _ = update(
            &mut app,
            Message::ThemeRoleTextChanged {
                editor_instance: instance,
                selection_generation: generation,
                role,
                variant,
                text: text.into(),
            },
        );
    }
    assert_eq!(app.theme_session.editor_buffer(), Some("#010203"));
    assert!(app.theme_session.editor_pending());
    assert_eq!(app.theme_session.editor_document(), Some(&document));
    theme_editor_select(&mut app, theme_roles::ThemeRole::Tertiary, variant);
    theme_editor_select(&mut app, role, variant);
    let _ = update(
        &mut app,
        Message::ThemeRoleTextChanged {
            editor_instance: instance,
            selection_generation: generation,
            role,
            variant,
            text: "#040506".into(),
        },
    );
    assert_eq!(app.theme_session.editor_document(), Some(&document));
    assert!(!app.theme_session.editor_pending());
    assert_eq!(
        app.theme_session.editor_buffer(),
        Some(document.color(role, variant))
    );
}

#[test]
fn theme_role_editor_pending_blocks_current_and_stale_save_apply() {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    let before = app.state.clone();
    let binding = crate::candidate_app::theme_workflow::Binding::current(&app).unwrap();
    let (i, g, r, v) = theme_editor_binding(&app);
    let document = app.theme_session.editor_document().unwrap().clone();
    let _ = update(
        &mut app,
        Message::ThemeRoleTextChanged {
            editor_instance: i,
            selection_generation: g,
            role: r,
            variant: v,
            text: "#010203".into(),
        },
    );
    let _ = update(
        &mut app,
        Message::ThemeDesigner(crate::candidate_app::theme_workflow::Action::Save {
            binding,
            apply: true,
        }),
    );
    theme_workflow::designer_save(&mut app, true);
    assert_eq!(app.state, before);
    assert!(!app.state_path.exists());
    assert!(app.theme_session.editor_pending());
    assert_eq!(app.theme_session.editor_document(), Some(&document));
    theme_editor_set(&mut app, "#010203");
    theme_save_apply(&mut app);
    assert_eq!(
        app.state
            .applied_theme
            .as_ref()
            .unwrap()
            .document
            .color(r, v),
        "#010203"
    );
}

#[test]
fn theme_visual_validation_keeps_local_message_and_saved_state() {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    app.theme_workflow.message = "Prior local validation message".into();
    let (i, g, r, v) = theme_editor_binding(&app);
    let before = app.state.clone();
    let _ = update(
        &mut app,
        Message::ThemeRoleTextChanged {
            editor_instance: i,
            selection_generation: g,
            role: r,
            variant: v,
            text: "invalid".into(),
        },
    );
    assert!(app.theme_session.editor_field_diagnostic().is_some());
    assert_eq!(app.theme_workflow.message, "Prior local validation message");
    assert_eq!(app.state, before);
    assert!(!app.state_path.exists());
    // The actual production widget diff/focus test below covers stable field
    // ownership; obsolete raw diagnostic/ignored-role UI slots are retired.
}

type EntryInputState =
    text_input::State<<iced::Renderer as iced::advanced::text::Renderer>::Paragraph>;

fn entry_paths(
    tree: &iced::advanced::widget::Tree,
    tag: iced::advanced::widget::tree::Tag,
) -> Vec<Vec<usize>> {
    fn visit(
        tree: &iced::advanced::widget::Tree,
        tag: iced::advanced::widget::tree::Tag,
        path: &mut Vec<usize>,
        found: &mut Vec<Vec<usize>>,
    ) {
        if tree.tag == tag {
            found.push(path.clone());
        }
        for (i, child) in tree.children.iter().enumerate() {
            path.push(i);
            visit(child, tag, path, found);
            path.pop();
        }
    }
    let mut found = Vec::new();
    visit(tree, tag, &mut Vec::new(), &mut found);
    found
}

fn entry_role_path(app: &GpuCandidate, tree: &iced::advanced::widget::Tree) -> Vec<usize> {
    fn same_shape(a: &iced::advanced::widget::Tree, b: &iced::advanced::widget::Tree) -> bool {
        a.tag == b.tag
            && a.children.len() == b.children.len()
            && a.children
                .iter()
                .zip(&b.children)
                .all(|(a, b)| same_shape(a, b))
    }
    fn visit(
        tree: &iced::advanced::widget::Tree,
        role: &iced::advanced::widget::Tree,
        path: &mut Vec<usize>,
        found: &mut Vec<Vec<usize>>,
    ) {
        if same_shape(tree, role) {
            found.push(path.clone());
        }
        for (i, child) in tree.children.iter().enumerate() {
            path.push(i);
            visit(child, role, path, found);
            path.pop();
        }
    }
    let role = theme_role_editor_content(app);
    let role_tree = iced::advanced::widget::Tree::new(role.as_widget());
    let mut found = Vec::new();
    visit(tree, &role_tree, &mut Vec::new(), &mut found);
    assert_eq!(
        found.len(),
        1,
        "production role subtree must be uniquely identifiable"
    );
    found.pop().unwrap()
}

#[test]
fn theme_entry_visual_editor_precedes_legacy_settings() {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    theme_ensure_designer(&mut app);
    let view = theme_settings_content(&app);
    let tree = iced::advanced::widget::Tree::new(view.as_widget());
    let path = entry_role_path(&app, &tree);
    assert!(
        !path.is_empty(),
        "wheel editor is mounted in the reading surface"
    );
    assert!(app.theme_session.editor_active());
}

#[test]
fn theme_entry_selected_hex_has_separate_index_and_reading_scroll_owners() {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    theme_ensure_designer(&mut app);
    let view = theme_role_editor_content(&app);
    let tree = iced::advanced::widget::Tree::new(view.as_widget());
    let fields = entry_paths(
        &tree,
        iced::advanced::widget::tree::Tag::of::<EntryInputState>(),
    );
    assert_eq!(
        fields.len(),
        1,
        "selected hex only; filter belongs to the index"
    );
    let scroll: Element<'_, Message> = scrollable(text("")).into();
    assert!(
        entry_paths(&tree, scroll.as_widget().tag()).is_empty(),
        "no duplicate role list beneath wheel"
    );
    let reading = theme_settings_content(&app);
    let reading_tree = iced::advanced::widget::Tree::new(reading.as_widget());
    assert_eq!(
        entry_paths(&reading_tree, scroll.as_widget().tag()).len(),
        1,
        "one reading scroll owner, no library scroller"
    );
    let index = view::settings::theme_designer::palette_index(&app);
    assert_eq!(
        entry_paths(
            &iced::advanced::widget::Tree::new(index.as_widget()),
            scroll.as_widget().tag()
        )
        .len(),
        1,
        "independent role index"
    );
}

#[test]
fn theme_entry_production_diff_retains_hex_focus_during_typing() {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    theme_ensure_designer(&mut app);
    let mut tree = {
        let view = theme_settings_content(&app);
        iced::advanced::widget::Tree::new(view.as_widget())
    };
    let mut path = entry_role_path(&app, &tree);
    let role = theme_role_editor_content(&app);
    let role_tree = iced::advanced::widget::Tree::new(role.as_widget());
    path.extend(
        entry_paths(
            &role_tree,
            iced::advanced::widget::tree::Tag::of::<EntryInputState>(),
        )
        .pop()
        .unwrap(),
    );
    drop(role);
    let focused = path
        .iter()
        .fold(&mut tree, |node, &i| &mut node.children[i]);
    focused.state.downcast_mut::<EntryInputState>().focus();
    let (instance, generation, role, variant) = theme_editor_binding(&app);
    let _ = update(
        &mut app,
        Message::ThemeRoleTextChanged {
            editor_instance: instance,
            selection_generation: generation,
            role,
            variant,
            text: "#010203".into(),
        },
    );
    assert!(app.theme_session.editor_pending());
    assert!(!app.theme_session.has_preview());
    {
        let view = theme_settings_content(&app);
        tree.diff(view.as_widget());
    }
    let focused = path.iter().fold(&tree, |node, &i| &node.children[i]);
    assert!(
        focused.state.downcast_ref::<EntryInputState>().is_focused(),
        "actual view diff must retain the selected field state"
    );
}

#[test]
fn theme_role_editor_captured_enter_submits_continuous_typing_without_apply() {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    let (i, g, r, v) = theme_editor_binding(&app);
    let before = app.state.clone();
    let submit = Message::ThemeRoleSetColor {
        editor_instance: i,
        selection_generation: g,
        role: r,
        variant: v,
    };
    for text in ["#f", "#f0", "#f08080"] {
        let _ = update(
            &mut app,
            Message::ThemeRoleTextChanged {
                editor_instance: i,
                selection_generation: g,
                role: r,
                variant: v,
                text: text.into(),
            },
        );
    }
    let _ = update(&mut app, submit);
    assert!(!app.theme_session.editor_pending());
    assert_eq!(
        app.theme_session.editor_document().unwrap().color(r, v),
        "#f08080"
    );
    assert_eq!(
        app.theme_session
            .designer_recent()
            .last()
            .map(String::as_str),
        Some("#f08080")
    );
    assert_eq!(app.state, before);
    assert!(!app.state_path.exists());
}

#[test]
fn theme_role_editor_reset_clears_pending_through_update() {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    let (i, g, r, v) = theme_editor_binding(&app);
    let before = app.theme_session.editor_document().unwrap().clone();
    for text in ["#010203", "#0a4682"] {
        let _ = update(
            &mut app,
            Message::ThemeRoleTextChanged {
                editor_instance: i,
                selection_generation: g,
                role: r,
                variant: v,
                text: text.into(),
            },
        );
    }
    assert!(app.theme_session.editor_pending());
    let _ = update(
        &mut app,
        Message::ThemeRoleResetColor {
            editor_instance: i,
            selection_generation: g,
            role: r,
            variant: v,
        },
    );
    assert!(!app.theme_session.editor_pending());
    assert_eq!(app.theme_session.editor_buffer(), Some("#0a4682"));
    assert_eq!(app.theme_session.editor_document(), Some(&before));
    let _ = update(
        &mut app,
        Message::ThemeRoleTextChanged {
            editor_instance: i,
            selection_generation: g,
            role: r,
            variant: v,
            text: "#040506".into(),
        },
    );
    assert!(!app.theme_session.editor_pending());
    let (i, g, r, v) = theme_editor_binding(&app);
    let counters = (
        app.theme_session.draft_revision(),
        app.theme_session.session_token(),
    );
    let _ = update(
        &mut app,
        Message::ThemeRoleResetColor {
            editor_instance: i,
            selection_generation: g,
            role: r,
            variant: v,
        },
    );
    assert_eq!(
        (
            app.theme_session.draft_revision(),
            app.theme_session.session_token()
        ),
        counters
    );
    assert_eq!(app.theme_session.editor_selection_generation(), Some(g));
}

#[test]
fn theme_role_editor_equivalent_set_ends_stream_through_update() {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    theme_ensure_designer(&mut app);
    let (instance, generation, role, variant) = theme_editor_binding(&app);
    // Type an equivalent form of the stored color and Set it.
    let _ = update(
        &mut app,
        Message::ThemeRoleTextChanged {
            editor_instance: instance,
            selection_generation: generation,
            role,
            variant,
            text: " 0A4682 ".to_string(),
        },
    );
    assert!(app.theme_session.editor_pending());
    let _ = update(
        &mut app,
        Message::ThemeRoleSetColor {
            editor_instance: instance,
            selection_generation: generation,
            role,
            variant,
        },
    );
    // The Set ended the stream: pending cleared, buffer
    // normalized, document unchanged.
    assert!(!app.theme_session.editor_pending());
    assert_eq!(app.theme_session.editor_buffer(), Some("#0a4682"));
    let sentinel =
        theme_roles::import_document(&serde_json::from_str(&theme_sentinel_text()).unwrap())
            .unwrap()
            .document;
    assert_eq!(app.theme_session.editor_document(), Some(&sentinel));
    let (_, new_generation, ..) = theme_editor_binding(&app);
    assert_ne!(new_generation, generation);
    // A queued pre-Set text event cannot reinstate pending text.
    let _ = update(
        &mut app,
        Message::ThemeRoleTextChanged {
            editor_instance: instance,
            selection_generation: generation,
            role,
            variant,
            text: "#010203".to_string(),
        },
    );
    assert!(!app.theme_session.editor_pending());
    assert_eq!(app.theme_session.editor_buffer(), Some("#0a4682"));
    // A fresh current binding still accepts legitimate typing.
    let (instance, generation, role, variant) = theme_editor_binding(&app);
    let _ = update(
        &mut app,
        Message::ThemeRoleTextChanged {
            editor_instance: instance,
            selection_generation: generation,
            role,
            variant,
            text: "#010203".to_string(),
        },
    );
    assert!(app.theme_session.editor_pending());
    assert_eq!(app.theme_session.editor_buffer(), Some("#010203"));
}

#[test]
fn theme_role_editor_delayed_load_rejected_after_departure_reentry() {
    let mut app = theme_clean_applied_app();
    let binding = crate::candidate_app::theme_workflow::Binding::current(&app).unwrap();
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Playback),
    );
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Theme),
    );
    let document = app.theme_session.editor_document().unwrap().clone();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(crate::candidate_app::theme_workflow::Action::Load {
            binding,
            target: crate::candidate_app::theme_workflow::Load::Starter("nord"),
        }),
    );
    assert_eq!(app.theme_session.editor_document(), Some(&document));
    let binding = crate::candidate_app::theme_workflow::Binding::current(&app).unwrap();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(crate::candidate_app::theme_workflow::Action::Load {
            binding,
            target: crate::candidate_app::theme_workflow::Load::Starter("nord"),
        }),
    );
    assert_eq!(
        app.theme_session.editor_document(),
        Some(&candidate_state::frontend::nord_document())
    );
}

#[test]
fn theme_role_editor_stale_save_cannot_bind_replacement_document() {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    let binding = crate::candidate_app::theme_workflow::Binding::current(&app).unwrap();
    let replacement = candidate_state::frontend::nord_document().replaced(
        theme_roles::ThemeRole::Primary,
        theme_roles::ColorVariant::Dark,
        "#abcdef",
    );
    app.theme_session.load_designer(
        replacement.clone(),
        theme_roles::ColorVariant::Light,
        "Replacement".into(),
        None,
    );
    let before = app.state.clone();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(crate::candidate_app::theme_workflow::Action::Save {
            binding,
            apply: true,
        }),
    );
    assert_eq!(app.state, before);
    assert!(!app.state_path.exists());
    assert_eq!(app.theme_session.editor_document(), Some(&replacement));
    theme_save_apply(&mut app);
    assert_eq!(
        app.state.applied_theme.as_ref().unwrap().document,
        replacement
    );
}

#[test]
fn theme_role_editor_departure_retains_draft_and_invalidates_callbacks() {
    let mut app = theme_dirty_applied_app();
    let (i, g, r, v) = theme_editor_binding(&app);
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Playback),
    );
    assert!(app.theme_workflow.prompt.is_some());
    theme_workflow::designer_decide(
        &mut app,
        crate::candidate_app::theme_workflow::Choice::Cancel,
    );
    assert_theme_draft_and_applied_kept(&app);
    let _ = app.set_visible(false, "owned-dirty-hide");
    let _ = app.set_visible(true, "owned-dirty-reopen");
    let _ = update(
        &mut app,
        Message::ThemeRoleTextChanged {
            editor_instance: i,
            selection_generation: g,
            role: r,
            variant: v,
            text: "#040506".into(),
        },
    );
    assert_theme_draft_and_applied_kept(&app);
    assert!(!app.theme_session.editor_pending());
}

#[test]
fn theme_role_editor_retained_edits_do_not_move_consumers() {
    let mut app = theme_clean_applied_app();
    let palette_before = donor_palette(&app);
    theme_editor_select(
        &mut app,
        theme_roles::ThemeRole::Error,
        theme_roles::ColorVariant::Dark,
    );
    theme_editor_set(&mut app, "#123456");
    let after = donor_palette(&app);
    assert_eq!(after.primary, palette_before.primary);
    assert_eq!(after.background, palette_before.background);
    assert_eq!(after.surface, palette_before.surface);
    assert_eq!(after.surface_text, palette_before.surface_text);
    assert_eq!(after.outline, palette_before.outline);
    assert_eq!(
        app.theme_session.editor_document().unwrap().color(
            theme_roles::ThemeRole::Error,
            theme_roles::ColorVariant::Dark
        ),
        "#123456"
    );
    theme_editor_select(
        &mut app,
        theme_roles::ThemeRole::Primary,
        theme_roles::ColorVariant::Dark,
    );
    theme_editor_set(&mut app, "#102030");
    assert_eq!(
        donor_palette(&app).primary,
        palette_before.primary,
        "mapped draft also stays local"
    );
    theme_save_apply(&mut app);
    let palette = donor_palette(&app);
    assert_eq!(
        donor_ui::rgb_components(palette.primary),
        [16.0 / 255.0, 32.0 / 255.0, 48.0 / 255.0]
    );
    assert_eq!(palette.background, palette_before.background);
    assert_eq!(palette.surface_text, palette_before.surface_text);
    assert_eq!(
        app.state.applied_theme.as_ref().unwrap().document.color(
            theme_roles::ThemeRole::Error,
            theme_roles::ColorVariant::Dark
        ),
        "#123456"
    );
}

#[test]
fn theme_integration_stale_and_off_page_messages_cannot_commit() {
    let mut app = theme_clean_applied_app();
    let binding = crate::candidate_app::theme_workflow::Binding::current(&app).unwrap();
    let before = app.state.clone();
    theme_variant(&mut app, theme_roles::ColorVariant::Light);
    let _ = update(
        &mut app,
        Message::ThemeDesigner(crate::candidate_app::theme_workflow::Action::Save {
            binding,
            apply: true,
        }),
    );
    assert_eq!(app.state, before);
    let binding = crate::candidate_app::theme_workflow::Binding::current(&app).unwrap();
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    let _ = update(
        &mut app,
        Message::ThemeDesigner(crate::candidate_app::theme_workflow::Action::Save {
            binding,
            apply: true,
        }),
    );
    assert_eq!(app.state, before);
    assert!(!app.panel_state.settings_open);
}

#[test]
fn theme_integration_legacy_messages_guarded_while_document_active() {
    let mut app = theme_clean_applied_app();
    let appearance = app.state.appearance.clone();
    let _ = update(&mut app, Message::CycleAppearancePreset);
    let _ = update(
        &mut app,
        Message::AppearanceColorChanged(AppearanceField::Accent, "#000000".into()),
    );
    assert_eq!(app.state.appearance, appearance);
    let _ = update(&mut app, Message::CycleAppearanceOpacity);
    assert_eq!(app.state.appearance.overlay_opacity, 0.95);
    // Migrated None is the authoritative existing fallback, not a new UI action.
    app.state.applied_theme = None;
    let _ = update(&mut app, Message::CycleAppearancePreset);
    assert_eq!(app.state.appearance.preset, "catppuccin-mocha");
}

#[test]
fn theme_integration_draft_hide_and_cancel_never_autosave() {
    let mut app = theme_open_app();
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    theme_editor_set(&mut app, "#010203");
    assert!(!app.state_path.exists());
    let _ = app.set_visible(false, "owned-unsaved-hide");
    assert!(!app.state_path.exists());
    let _ = app.set_visible(true, "owned-unsaved-reopen");
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Playback),
    );
    theme_workflow::designer_decide(
        &mut app,
        crate::candidate_app::theme_workflow::Choice::Cancel,
    );
    assert!(!app.state_path.exists());
    assert!(app.theme_session.designer_dirty());
    theme_workflow::designer_save(&mut app, false);
    assert!(app.state_path.exists());
    assert_eq!(
        CandidateState::load(&app.state_path).theme_library,
        app.state.theme_library
    );
}

#[test]
fn theme_integration_invalid_hex_keeps_applied_snapshot_and_usable_draft() {
    let mut app = theme_clean_applied_app();
    let before = app.state.clone();
    let (i, g, r, v) = theme_editor_binding(&app);
    for text in ["#zzzzzz", "{broken"] {
        let _ = update(
            &mut app,
            Message::ThemeRoleTextChanged {
                editor_instance: i,
                selection_generation: g,
                role: r,
                variant: v,
                text: text.into(),
            },
        );
        theme_save_apply(&mut app);
        assert_eq!(app.state, before);
        assert_eq!(app.theme_session.editor_buffer(), Some(text));
        assert!(app.theme_session.editor_pending());
        assert!(app.theme_session.editor_field_diagnostic().is_some());
        assert_eq!(
            donor_ui::rgb_components(donor_palette(&app).primary),
            theme_sentinel_rgb(0, false)
        );
    }
}

#[test]
fn d4b_filter_replacement_publishes_full_current_cards_and_closes_stale_detail() {
    let (mut app, submissions) = d4a_fixture(12);
    app.detail_open = true;
    app.detail_progress = 1.0;
    app.depth_detail_id = Some("item-0000".into());
    for order in [SortOrder::Newest, SortOrder::Colour, SortOrder::Newest] {
        app.sort_order = order;
        app.refresh_filter();
        app.rebuild_depth_frame();
        for step in 0..3 {
            if step != 0 {
                d4a_motion_tick(&mut app, 0.05);
            }
            let current = app.depth_frame.as_ref().unwrap();
            let raw = donor_scene::depth_instances(
                current,
                &GalleryColors::default(),
                1.0,
                &d4a_scene(&app),
            );
            assert_eq!(raw.len(), current.cards.len());
            assert!(!current.cards.is_empty());
            for (entry, instance) in current.cards.iter().zip(raw) {
                assert_eq!(
                    entry.id,
                    app.wallpapers[app.filtered_indices[entry.filtered_index]].id
                );
                assert_eq!(instance.rect[..2], entry.card.center);
                assert_eq!(instance.rect[2], entry.card.size[0] * 0.5);
                assert_eq!(instance.crop, entry.crop_uv);
                assert!(entry.card.size[0] > 1.0);
            }
            let selected = current.cards.last().unwrap();
            assert_eq!(
                current
                    .hit(selected.card.center[0], selected.card.center[1])
                    .unwrap()
                    .id,
                selected.id
            );
        }
    }
    assert!(!app.detail_open);
    assert!(app.depth_detail_id.is_none());
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

fn d4b_seed_resident(app: &mut GpuCandidate) {
    let indices = app
        .depth_frame
        .as_ref()
        .unwrap()
        .live_poster_indices
        .clone();
    let generation = app.poster_generation.load(Ordering::Acquire);
    let catalog_generation = app.catalog_generation.load(Ordering::Acquire);
    let mut posters = Vec::new();
    for (layer, index) in indices.into_iter().enumerate() {
        let w = &app.wallpapers[index];
        let source = poster_source_for_item(w);
        app.poster_slots.insert(
            index,
            PosterSlot {
                layer: layer as u32,
                item_id: w.id.clone(),
                source: source.clone(),
                kind: gallery::poster_kind_for_item(w),
                generation,
                last_requested: generation,
                catalog_generation,
            },
        );
        // Synthetic decoder output with distinct source identities; no file decode/provider.
        posters.push(gpu_gallery::video_placeholder_poster(&source, layer as u32));
    }
    app.posters = Arc::new(posters);
    app.rebuild_depth_frame();
}
fn d4b_requests(app: &GpuCandidate) -> Vec<PosterRequestIdentity> {
    depth_poster_requests(
        app.depth_frame.as_ref().unwrap(),
        &app.filtered_indices,
        &app.wallpapers,
    )
}
fn d4b_sources(
    app: &GpuCandidate,
    requests: &[PosterRequestIdentity],
) -> Vec<poster_worker::PosterSource> {
    gallery::poster_sources_bounded(
        requests,
        &app.poster_slots,
        &[],
        app.catalog_generation.load(Ordering::Acquire),
        gpu_gallery::MAX_POSTERS,
        false,
    )
}
fn d4b_complete(
    app: &mut GpuCandidate,
    requests: &[PosterRequestIdentity],
    fail: bool,
) -> Vec<poster_worker::PosterSource> {
    let sources = d4b_sources(app, requests);
    let generation = app.poster_generation.fetch_add(1, Ordering::AcqRel) + 1;
    app.poster_mailbox
        .expect(generation, sources.clone())
        .unwrap();
    let posters = if fail {
        Vec::new()
    } else {
        sources
            .iter()
            .map(|source| poster_worker::DecodedPoster {
                source: source.clone(),
                poster: gpu_gallery::video_placeholder_poster(&source.path, source.layer),
            })
            .collect()
    };
    app.poster_mailbox
        .publish(
            poster_worker::PosterCompletion {
                generation,
                cursor: 1,
                finished: true,
                posters,
                failures: Vec::new(),
            },
            || true,
        )
        .unwrap();
    let _ = update(app, Message::Wake(Wake::PostersReady));
    sources
}
fn d4b_disjoint_fixture(new_count: usize) -> GpuCandidate {
    let (mut app, _) = d4a_fixture(96);
    app.state.gallery.depth.visible_count = 21.0;
    app.state.gallery.depth.spacing = 100.0;
    app.filtered_indices = (0..21).collect();
    app.selected = Some(10);
    app.depth_presentation = None;
    app.reset_depth_transients();
    app.rebuild_depth_frame();
    assert_eq!(app.depth_frame.as_ref().unwrap().cards.len(), 21);
    d4b_seed_resident(&mut app);
    app.state.favorites = (30..30 + new_count)
        .map(|i| app.wallpapers[i].id.clone())
        .collect();
    app.filter_rule.favorites_only = true;
    app.selected = Some(30 + new_count / 2);
    app.refresh_filter();
    app.rebuild_depth_frame();
    app
}

#[test]
fn d5_screenshot_catalog_selection_reanchors_depth_before_requested_detail() {
    let (mut app, submissions) = d4a_fixture(32);
    app.catalog_loaded = false; // Exercise first publication, as runtime bootstrap does.
    app.visible = false;
    app.screenshot_path = Some(app.state_path.with_file_name("source-only.png"));
    app.screenshot_index = 15;
    app.screenshot_detail = true;
    app.screenshot_detail_progress = 0.9;
    let wallpapers = app.wallpapers.clone();
    let entries = app.catalog.clone();
    let generation = app.catalog_scan_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::Wake(Wake::CatalogLoaded {
            generation,
            wallpapers,
            entries,
        }),
    );
    assert_eq!(app.selected, Some(15));
    assert_eq!(app.depth_camera.unwrap().value(), 15.0);
    assert_eq!(app.depth_camera.unwrap().target, 15.0);
    assert_eq!(app.depth_selection.get("item-0015").unwrap().value(), 1.0);
    assert!(app.detail_open);
    assert_eq!(app.detail_progress, 0.9);
    app.visible = true;
    app.rebuild_depth_frame();
    assert!(app
        .depth_frame
        .as_ref()
        .unwrap()
        .cards
        .iter()
        .any(|c| c.card.selected && c.id == "item-0015"));
    assert_eq!(submissions.load(Ordering::Acquire), 0);
    assert!(!app.screenshot_path.as_ref().unwrap().exists());
}

#[test]
fn d5_late_window_open_cannot_revive_hidden_or_replace_owned_depth_window() {
    let (mut app, submissions) = d4a_fixture(12);
    let id = app.window.unwrap();
    let _ = app.set_visible(false, "d5-hide-before-open-callback");
    let poster_generation = app.poster_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::WindowOpened {
            id,
            width: 960.0,
            height: 720.0,
        },
    );
    assert!(
        !app.visible,
        "late open callback revived hidden Depth input/demand"
    );
    assert_eq!(app.window, Some(id));
    assert!(app.depth_frame.is_none());
    assert!(app.depth_camera.is_none());
    assert!(app.posters.is_empty());
    assert_eq!(
        app.poster_generation.load(Ordering::Acquire),
        poster_generation
    );

    app.visible = true;
    app.reset_depth_transients();
    app.rebuild_depth_frame();
    let viewport = app.viewport;
    let revision = app.depth_frame.as_ref().unwrap().revision;
    let wrong_id = window::Id::unique();
    let _ = update(
        &mut app,
        Message::WindowOpened {
            id: wrong_id,
            width: 320.0,
            height: 240.0,
        },
    );
    assert_eq!(app.window, Some(id));
    assert_eq!(app.viewport, viewport);
    assert_eq!(app.depth_frame.as_ref().unwrap().revision, revision);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn d5_benchmark_mode_changes_cancel_depth_ownership_without_saving_profile() {
    for (step, expected) in [
        (0, GalleryMode::Slices),
        (20, GalleryMode::Geometric),
        (40, GalleryMode::Wall),
    ] {
        let mut app = d4b_disjoint_fixture(5);
        assert!(!app.depth_frame.as_ref().unwrap().cards.is_empty());
        let saved_gallery = app.state.gallery.clone();
        let old_motion_generation = app.motion_generation.load(Ordering::Acquire);
        let old_camera = app.depth_camera.unwrap();
        app.detail_open = true;
        app.detail_progress = 0.7;
        app.depth_detail_id = app.selected.map(|i| app.wallpapers[i].id.clone());
        let old_detail_generation = app.detail_generation.load(Ordering::Acquire);
        app.wall_scroll_offset = 720.0;

        let _ = update(&mut app, Message::Wake(Wake::BenchmarkNavigate(step)));
        assert_eq!(app.gallery_mode, expected);
        assert!(app.depth_frame.is_none());
        assert!(app.depth_camera.is_none());
        assert!(app.depth_presentation.is_none());
        assert!(app.depth_detail_id.is_none());
        assert!(!app.detail_open);
        assert_eq!(app.wall_scroll_offset, 0.0);
        assert_eq!(app.state.gallery, saved_gallery);
        assert!(
            !app.state_path.exists(),
            "diagnostic navigation wrote a profile"
        );

        let _ = update(
            &mut app,
            Message::Wake(Wake::MotionFrame {
                generation: old_motion_generation,
                value: old_camera.value(),
                velocity: old_camera.velocity,
                state: old_camera,
                selection: HashMap::new(),
                presentation: Some(DepthPresentation::target(&CandidateState::default())),
                ordinary: None,
                hex: None,
                wall: None,
                scene: None,
            }),
        );
        let _ = update(
            &mut app,
            Message::Wake(Wake::DetailFrame {
                generation: old_detail_generation,
                value: 1.0,
                close_when_done: false,
                spring: Some(old_camera),
            }),
        );
        assert!(app.depth_camera.is_none());
        assert!(!app.detail_open);
        assert_eq!(app.detail_progress, 0.0);

        let _ = update(&mut app, Message::Wake(Wake::BenchmarkNavigate(60)));
        let position = app
            .filtered_indices
            .iter()
            .position(|i| Some(*i) == app.selected)
            .unwrap();
        assert_eq!(app.gallery_mode, GalleryMode::Depth);
        assert_eq!(app.depth_camera.unwrap().value(), position as f32);
        assert_eq!(app.depth_camera.unwrap().target, position as f32);
        assert_eq!(app.state.gallery, saved_gallery);
        assert!(!app.state_path.exists());
    }
}

#[test]
fn d4b_disjoint_replacements_allocate_only_current_layers_and_exact_sources() {
    for count in [11, 21] {
        let mut app = d4b_disjoint_fixture(count);
        assert!(app.posters.is_empty());
        assert!(app.poster_slots.is_empty());
        let requests = d4b_requests(&app);
        assert_eq!(requests.len(), count);
        let sources = d4b_complete(&mut app, &requests, false);
        assert_eq!(sources.len(), count);
        assert_eq!(
            app.posters
                .iter()
                .map(|p| p.layer)
                .collect::<HashSet<_>>()
                .len(),
            count
        );
        for source in sources {
            let slot = &app.poster_slots[&source.index];
            assert_eq!(slot.layer, source.layer);
            assert_eq!(slot.item_id, source.item_id);
            assert_eq!(slot.source, source.path);
            assert!(app
                .posters
                .iter()
                .any(|p| p.layer == source.layer && p.source == source.path));
        }
        let params = app.depth_presentation.unwrap().params;
        let resources = app.posters.clone();
        for _ in 0..60 {
            d4a_motion_tick(&mut app, 0.05);
        }
        let current = app.depth_presentation.unwrap().params;
        assert_eq!(
            (
                current.width,
                current.height,
                current.spacing,
                current.falloff,
                current.corner_radius,
                current.skew,
                current.visible_count
            ),
            (
                params.width,
                params.height,
                params.spacing,
                params.falloff,
                params.corner_radius,
                params.skew,
                params.visible_count
            )
        );
        assert_eq!(app.depth_frame.as_ref().unwrap().cards.len(), count);
        assert_eq!(d4b_requests(&app), requests);
        assert_eq!(app.posters.len(), count);
        assert!(resources
            .iter()
            .all(|exact| app.posters.iter().any(|p| p.same_resource(exact))));
    }
}

#[test]
fn d4b_sort_reuses_current_resource_and_rejects_changed_media() {
    let (mut app, _) = d4a_fixture(21);
    app.selected = Some(10);
    app.reset_depth_transients();
    app.rebuild_depth_frame();
    d4b_seed_resident(&mut app);
    let slot = app.poster_slots[&10].clone();
    let exact = app
        .posters
        .iter()
        .find(|p| p.layer == slot.layer)
        .unwrap()
        .clone();
    app.sort_order = SortOrder::Newest;
    app.refresh_filter();
    app.rebuild_depth_frame();
    assert_eq!(app.poster_slots[&10], slot);
    assert!(app.posters.iter().any(|p| p.same_resource(&exact)));
    app.wallpapers[10].source = WallpaperSource::LocalVideo;
    app.wallpapers[10].file_target = "changed-poster.mp4".into();
    app.rebuild_depth_frame();
    assert!(!app.poster_slots.contains_key(&10));
    assert!(!app.posters.iter().any(|p| p.same_resource(&exact)));
}

#[test]
fn d4b_rapid_filter_reversals_keep_only_current_card_ids_and_the_atlas_bound() {
    let mut app = d4b_disjoint_fixture(11);
    for iteration in 0..80 {
        d4a_motion_tick(&mut app, 0.05);
        app.sort_order = if iteration % 2 == 0 {
            SortOrder::Newest
        } else {
            SortOrder::Colour
        };
        app.refresh_filter();
        app.rebuild_depth_frame();
        let frame = app.depth_frame.as_ref().unwrap();
        assert_eq!(frame.cards.len(), 11);
        assert_eq!(
            frame
                .cards
                .iter()
                .map(|c| &c.id)
                .collect::<HashSet<_>>()
                .len(),
            11
        );
        assert!(frame
            .cards
            .iter()
            .all(|c| c.card.size[0] > 1.0 && app.filtered_indices.contains(&c.card.index)));
        let requests = d4b_requests(&app);
        assert!(requests.len() <= gpu_gallery::MAX_POSTERS);
        d4b_complete(&mut app, &requests, iteration % 7 == 0);
        assert!(app.posters.len() <= gpu_gallery::MAX_POSTERS);
    }
}

#[test]
fn d4b_catalog_hide_mode_empty_reopen_same_mode_and_stale_completion_consumers() {
    for path in 0..6 {
        let mut app = d4b_disjoint_fixture(11);
        let requests = d4b_requests(&app);
        d4b_complete(&mut app, &requests, false);
        let old_generation = app.poster_generation.load(Ordering::Acquire);
        let old_motion = app.motion_generation.load(Ordering::Acquire);
        let _ = update(&mut app, Message::SetGalleryMode(GalleryMode::Depth));
        assert_eq!(app.motion_generation.load(Ordering::Acquire), old_motion);
        match path {
            0 => {
                let _ = app.set_visible(false, "d4b-hide");
            }
            1 => {
                app.window = None;
                let _ = app.set_visible(false, "d4b-hide-without-window");
            }
            2 => {
                let id = app.window.unwrap();
                let _ = update(&mut app, Message::WindowClosed(id));
            }
            3 => {
                app.set_gallery_mode(GalleryMode::Slices);
            }
            4 => {
                app.filter_rule.text = "does-not-match-anything".into();
                app.refresh_filter();
                app.schedule_posters();
            }
            _ => {
                let generation = app.catalog_scan_generation.fetch_add(1, Ordering::AcqRel) + 1;
                let wallpapers = app.wallpapers.clone();
                let mut entries = app.catalog.clone();
                entries[0].modified_unix += 1;
                let _ = update(
                    &mut app,
                    Message::Wake(Wake::CatalogLoaded {
                        generation,
                        wallpapers,
                        entries,
                    }),
                );
                let published = app.depth_frame.as_ref().unwrap();
                assert!(!published.cards.is_empty());
                assert!(published.cards.iter().all(|card| card.card.size[0] > 1.0));
            }
        }
        if path == 4 {
            assert!(app.posters.is_empty());
            assert!(app.depth_frame.as_ref().unwrap().cards.is_empty());
        }
        let settled = app.posters.clone();
        let slots = app.poster_slots.clone();
        let source = poster_worker::PosterSource {
            index: requests[0].index,
            layer: 0,
            item_id: requests[0].item_id.clone(),
            path: requests[0].source.clone(),
            kind: requests[0].kind,
            video_stamp: None,
            still: None,
        };
        app.poster_mailbox
            .publish(
                poster_worker::PosterCompletion {
                    generation: old_generation,
                    cursor: 1,
                    finished: true,
                    failures: Vec::new(),
                    posters: vec![poster_worker::DecodedPoster {
                        poster: gpu_gallery::video_placeholder_poster(&source.path, 0),
                        source,
                    }],
                },
                || panic!("stale old completion must not queue notification"),
            )
            .unwrap();
        let _ = update(&mut app, Message::Wake(Wake::PostersReady));
        assert!(Arc::ptr_eq(&app.posters, &settled), "lifetime path {path}");
        assert_eq!(app.poster_slots, slots);
        assert!(app.motion_generation.load(Ordering::Acquire) > old_motion);
        if path <= 2 {
            let _ = app.set_visible(true, "d4b-reopen");
            assert!(app.visible);
        }
    }
}

#[test]
fn d4b_full_card_render_hit_and_detail_reject_stale_revision() {
    let mut app = d4b_disjoint_fixture(11);
    d4a_motion_tick(&mut app, 0.05);
    let stale = app.depth_frame.as_ref().unwrap().revision;
    for _ in 0..5 {
        d4a_motion_tick(&mut app, 0.05);
    }
    let frame = app.depth_frame.as_ref().unwrap();
    let raw = donor_scene::depth_instances(frame, &GalleryColors::default(), 1.0, &d4a_scene(&app));
    assert_eq!(raw.len(), frame.cards.len());
    for (body, instance) in frame.cards.iter().zip(&raw) {
        assert_eq!(instance.rect[..2], body.card.center);
        assert_eq!(instance.rect[2], body.card.size[0] * 0.5);
        assert_eq!(instance.crop, body.crop_uv);
        let expected = frame.cards.iter().rev().find(|c| {
            donor_layout::depth_contains(
                c.card.center,
                c.card.size,
                c.card.skew_distance,
                [c.card.corner_radius; 4],
                body.card.center,
            )
        });
        assert_eq!(
            frame
                .hit(body.card.center[0], body.card.center[1])
                .map(|c| &c.id),
            expected.map(|c| &c.id)
        );
    }
    let identity = app.wallpapers[app.selected.unwrap()].id.clone();
    app.detail_open = true;
    app.detail_progress = 1.0;
    app.depth_detail_id = Some(identity.clone());
    let revision = app.depth_frame.as_ref().unwrap().revision;
    assert!(detail_controls_active(&app));
    let before = app.state.favorites.contains(&identity);
    let _ = update(
        &mut app,
        Message::DepthDonorDetail(
            stale,
            identity.clone(),
            donor_back_layout::DetailIntent::Favorite,
        ),
    );
    assert_eq!(app.state.favorites.contains(&identity), before);
    let _ = update(
        &mut app,
        Message::DepthDonorDetail(
            revision,
            identity.clone(),
            donor_back_layout::DetailIntent::Favorite,
        ),
    );
    assert_eq!(app.state.favorites.contains(&identity), !before);
}

#[test]
fn d4b_stalled_gui_latest_slot_stale_rejection_and_failure_retry() {
    let mut app = d4b_disjoint_fixture(11);
    let requests = d4b_requests(&app);
    let sources = d4b_sources(&app, &requests);
    let wakes = std::sync::atomic::AtomicUsize::new(0);
    let make = |generation| poster_worker::PosterCompletion {
        generation,
        cursor: 1,
        finished: false,
        failures: Vec::new(),
        posters: sources
            .iter()
            .map(|source| poster_worker::DecodedPoster {
                source: source.clone(),
                poster: gpu_gallery::video_placeholder_poster(&source.path, source.layer),
            })
            .collect(),
    };
    let mut stale = 0;
    for _ in 0..100 {
        let generation = app.poster_generation.fetch_add(1, Ordering::AcqRel) + 1;
        app.poster_mailbox
            .expect(generation, sources.clone())
            .unwrap();
        app.poster_mailbox
            .publish(make(generation), || {
                wakes.fetch_add(1, Ordering::AcqRel);
                true
            })
            .unwrap();
        app.poster_mailbox
            .publish(make(stale), || {
                panic!("stale completion cannot republish or replace the retained current batch")
            })
            .unwrap();
        stale = generation;
    }
    assert_eq!(wakes.load(Ordering::Acquire), 1);
    let current = app.poster_generation.load(Ordering::Acquire);
    for invalid in 0..4 {
        let mut completion = make(current);
        completion.cursor = 2;
        let first = &mut completion.posters[0];
        match invalid {
            0 => first.source.item_id = "reused-id".into(),
            1 => {
                first.source.path = "reused-path.png".into();
                first.poster =
                    gpu_gallery::video_placeholder_poster(&first.source.path, first.source.layer);
            }
            2 => {
                first.source.kind = if first.source.kind == crate::poster_worker::PosterKind::Video
                {
                    crate::poster_worker::PosterKind::Still
                } else {
                    crate::poster_worker::PosterKind::Video
                }
            }
            _ => {
                first.source.layer = sources[1].layer;
                first.poster =
                    gpu_gallery::video_placeholder_poster(&first.source.path, first.source.layer);
            }
        }
        app.poster_mailbox
            .publish(completion, || {
                panic!("wrong ID/path/media/layer cannot replace current retained completion")
            })
            .unwrap();
    }
    let _ = update(&mut app, Message::Wake(Wake::PostersReady));
    assert_eq!(app.posters.len(), 11);
    assert!(app.poster_mailbox.take().unwrap().is_none());
    let generation = app.poster_generation.load(Ordering::Acquire);
    app.poster_mailbox
        .publish(
            poster_worker::PosterCompletion {
                generation,
                cursor: 2,
                finished: true,
                posters: Vec::new(),
                failures: Vec::new(),
            },
            || {
                wakes.fetch_add(1, Ordering::AcqRel);
                true
            },
        )
        .unwrap();
    assert_eq!(
        wakes.load(Ordering::Acquire),
        2,
        "atomic take/clear leaves the next producer able to wake"
    );
    app.invalidate_poster_work();
    app.poster_mailbox
        .publish(make(generation), || {
            panic!("invalidation rejects finished stale decode before retention")
        })
        .unwrap();
    let _ = update(&mut app, Message::Wake(Wake::PostersReady));
    assert!(app.poster_mailbox.take().unwrap().is_none());
    d4b_complete(&mut app, &requests, true);
    assert!(
        app.poster_failure,
        "empty completion must report failed current decoding"
    );
    assert!(app.posters.is_empty());
    app.invalidate_poster_work();
    d4b_complete(&mut app, &requests, false);
    assert!(!app.poster_failure);
    assert_eq!(app.posters.len(), 11);
}

#[test]
fn d4b_resize_geometry_style_and_detail_edits_preserve_current_camera() {
    for kind in 0..4 {
        let mut app = d4b_disjoint_fixture(11);
        d4a_motion_tick(&mut app, 0.05);
        let selection = app.selected;
        let camera = app.depth_camera.unwrap().x;
        match kind {
            0 => {
                let id = app.window.unwrap();
                let _ = update(
                    &mut app,
                    Message::WindowResized {
                        id,
                        width: 1000.0,
                        height: 800.0,
                    },
                );
            }
            1 => app.edit_depth(|d| d.width += 10.0),
            2 => app.edit_depth(|d| d.shadows = !d.shadows),
            _ => app.edit_depth(|d| d.detail.animate_back = !d.detail.animate_back),
        }
        assert_eq!(app.selected, selection);
        assert_eq!(app.depth_camera.unwrap().x, camera);
        assert!(app
            .poster_slots
            .keys()
            .all(|i| app.filtered_indices.contains(i)));
    }
}

#[test]
fn d4b_reused_middle_card_keeps_full_geometry_across_rebuilds_and_motion() {
    let (mut app, _) = d4a_fixture(21);
    app.selected = Some(10);
    app.reset_depth_transients();
    app.rebuild_depth_frame();
    d4b_seed_resident(&mut app);
    let expected = app
        .depth_frame
        .as_ref()
        .unwrap()
        .cards
        .iter()
        .find(|c| c.id == "item-0010")
        .unwrap()
        .clone();
    app.sort_order = SortOrder::Newest;
    app.refresh_filter();
    app.rebuild_depth_frame();
    app.detail_open = true;
    app.detail_progress = 1.0;
    app.depth_detail_id = Some("item-0010".into());
    let first = app.depth_frame.as_ref().unwrap().revision;
    for step in 0..6 {
        if step < 2 {
            app.rebuild_depth_frame();
        } else {
            d4a_motion_tick(&mut app, 0.05);
        }
        let frame = app.depth_frame.as_ref().unwrap();
        let current = frame.cards.iter().find(|c| c.id == "item-0010").expect(
            "same-cell exact identity remains fully presented after every current-frame rebuild",
        );
        assert_eq!(current.card.center, expected.card.center);
        assert_eq!(
            current.card.size, expected.card.size,
            "reused body never restarts incoming cut"
        );
        assert_eq!(current.crop_uv, expected.crop_uv);
        assert_eq!(
            frame
                .hit(current.card.center[0], current.card.center[1])
                .map(|c| c.id.as_str()),
            Some("item-0010")
        );
        assert_eq!(
            frame.cards.iter().filter(|c| c.id == "item-0010").count(),
            1
        );
        assert!(detail_controls_active(&app));
        assert!(frame.revision > first);
    }
}

#[test]
fn d4b_actual_view_owned_depth_frame_never_scans_end_selected_catalog() {
    let (mut app, _) = d4a_fixture(1);
    app.filtered_indices = (1..1_000_000).chain(std::iter::once(0)).collect();
    app.selected = Some(0);
    let _ = view(&app, app.window.unwrap());
    assert_eq!(app.view_legacy_lookups.load(Ordering::Relaxed),0,
            "Depth view consumes its owned <=24-card frame and never resolves a million-entry selected position");
    app.depth_frame = None;
    app.gallery_mode = GalleryMode::Slices;
    let _ = view(&app, app.window.unwrap());
    assert_eq!(
        app.view_legacy_lookups.load(Ordering::Relaxed),
        0,
        "Slices uses its stable-ID current presentation rather than the legacy selected-position layout"
    );
}

fn d4a_fixture(count: usize) -> (GpuCandidate, Arc<std::sync::atomic::AtomicUsize>) {
    let (mut app, submissions) = d3_application_fixture(count);
    app.deterministic_animation = true;
    for (index, item) in app.catalog.iter_mut().enumerate() {
        item.modified_unix = index as u64;
    }
    (app, submissions)
}
#[test]
fn d6_r1_resolved_presentation_promotes_latches_and_reassesses_after_close() {
    let (mut app, submissions) = d4a_fixture(8);
    let saved = app.state.gallery.depth.clone();
    let cards = app.depth_frame.as_ref().unwrap().cards.clone();
    app.start_detail(true, "r1-default");
    assert_eq!(
        app.depth_detail_mode,
        Some(donor_detail::DepthDetailMode::Inline)
    );
    assert_eq!(app.state.gallery.depth, saved);
    assert_eq!(
        format!("{:?}", app.depth_frame.as_ref().unwrap().cards),
        format!("{cards:?}")
    );
    let old_revision = app.depth_frame.as_ref().unwrap().revision;
    app.detail_progress = 0.8;
    let spring = app.depth_detail.unwrap();
    app.depth_frame
        .as_mut()
        .unwrap()
        .cards
        .iter_mut()
        .find(|e| e.card.selected)
        .unwrap()
        .card
        .size = [40.0, 100.0];
    app.resolve_depth_detail();
    assert!(app.depth_panel_open());
    assert_eq!(app.depth_detail.unwrap().target, spring.target);
    assert_eq!(app.detail_progress, 0.8);
    let _ = update(
        &mut app,
        Message::DepthDonorDetail(
            old_revision,
            "item-0000".into(),
            donor_back_layout::DetailIntent::Favorite,
        ),
    );
    assert!(!app.state.favorites.contains("item-0000"));
    app.rebuild_depth_frame();
    assert!(
        app.depth_panel_open(),
        "no downgrade when geometry becomes readable"
    );
    let selected = app.selected;
    let revision = app.depth_frame.as_ref().unwrap().revision;
    let _ = update(
        &mut app,
        Message::Gallery(GalleryMessage::DepthPointer {
            revision,
            item_id: Some("item-0001".into()),
            right: false,
            hover: false,
        }),
    );
    let _ = update(
        &mut app,
        Message::Gallery(GalleryMessage::Wheel(-2.0, None)),
    );
    assert_eq!(
        app.selected, selected,
        "modal panel blocks underlying input"
    );
    app.start_detail(false, "r1-close");
    assert!(app.depth_panel_open());
    app.start_detail(true, "r1-reversal");
    assert!(app.depth_panel_open());
    assert_eq!(app.depth_detail.unwrap().target, 1.0);
    app.state.card_flip_mut().animate_shader = false;
    app.state.card_flip_mut().animate_back = false;
    app.start_detail(false, "r1-snap-close");
    assert_eq!(app.depth_detail_mode, None);
    assert_eq!(app.depth_detail_id, None);
    app.start_detail(true, "r1-new-default-open");
    assert_eq!(
        app.depth_detail_mode,
        Some(donor_detail::DepthDetailMode::Inline)
    );
    app.selected = Some(1);
    app.rebuild_depth_frame();
    assert!(!app.detail_open);
    assert_eq!(app.depth_detail_mode, None);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn d6_r1_actual_fit_render_flags_and_disabled_apply_share_panel_contract() {
    for (width, height, viewport) in [
        (40.0, 100.0, [1920.0, 1080.0]),
        (280.0, 520.0, [80.0, 60.0]),
    ] {
        for effect in candidate_state::DEPTH_EFFECTS {
            for (shader, back) in [(false, false), (true, false), (false, true), (true, true)] {
                let (mut app, submissions) = d4a_fixture(3);
                app.viewport = viewport;
                app.state.gallery.depth.width = width;
                app.state.gallery.depth.height = height;
                app.state.gallery.depth.spacing = 0.0;
                app.state.card_flip_mut().effect = effect.into();
                app.state.card_flip_mut().animate_shader = shader;
                app.state.card_flip_mut().animate_back = back;
                app.depth_presentation = Some(DepthPresentation::target(&app.state));
                app.rebuild_depth_frame();
                let cards = app.depth_frame.as_ref().unwrap().cards.clone();
                app.start_detail(true, "r1-tiny");
                assert!(app.depth_panel_open());
                for progress in [0.3, 0.8, 1.0] {
                    app.detail_progress = progress;
                    let frame = app.depth_frame.as_ref().unwrap();
                    let raw = donor_scene::depth_instances(
                        frame,
                        &GalleryColors::default(),
                        1.0,
                        &d4a_scene(&app),
                    );
                    assert_eq!(
                        raw.len(),
                        cards.len() + 3,
                        "one backdrop/shadow/panel, no inline overlay"
                    );
                    let panel = raw.last().unwrap();
                    let target = donor_detail::external_target(viewport);
                    assert_eq!(
                        panel.rect,
                        [
                            target.center[0],
                            target.center[1],
                            target.size[0] * 0.5,
                            target.size[1] * 0.5
                        ]
                    );
                    assert_eq!(
                        panel.flip,
                        donor_scene::card_flip_shader_payload(
                            donor_scene::card_flip_phases(progress, shader, back).shader,
                            0,
                            candidate_state::depth_effect_id(effect).unwrap()
                        )
                    );
                    assert!(raw[..cards.len()].iter().all(|r| r.flip[0] == 0.0));
                }
                assert_eq!(
                    format!("{:?}", app.depth_frame.as_ref().unwrap().cards),
                    format!("{cards:?}")
                );
                let revision = app.depth_frame.as_ref().unwrap().revision;
                let _ = update(
                    &mut app,
                    Message::DepthDonorDetail(
                        revision,
                        "item-0000".into(),
                        donor_back_layout::DetailIntent::Apply,
                    ),
                );
                let _ = update(&mut app, Message::ApplySelected);
                assert_eq!(submissions.load(Ordering::Acquire), 0);
                app.cancel_detail("r1-cancel");
                assert_eq!(app.depth_detail_mode, None);
            }
        }
    }
}
#[test]
fn p310_motion_marker_drains_latest_and_rejects_hidden_or_stale_frames() {
    let (mut app, submissions) = d3_application_fixture(2);
    let make_frame = |generation, value| Wake::MotionFrame {
        generation,
        value,
        velocity: 0.0,
        state: GalleryMotionState::absolute_camera(value, value, 250.0, 50.0, 50.0),
        selection: Default::default(),
        presentation: None,
        ordinary: None,
        hex: None,
        wall: None,
        scene: None,
    };
    let generation = app.motion_generation.load(Ordering::Acquire);
    assert!(app.motion_mailbox.publish(
        make_frame(generation, 0.25),
        &app.motion_generation,
        || true
    ));
    assert!(app.motion_mailbox.publish(
        make_frame(generation, 0.75),
        &app.motion_generation,
        || panic!("duplicate marker")
    ));
    let _ = update(&mut app, Message::Wake(Wake::MotionReady));
    assert_eq!(app.motion, 0.75);
    assert!(app.motion_mailbox.take().is_none());
    app.visible = false;
    assert!(app.motion_mailbox.publish(
        make_frame(generation, 0.5),
        &app.motion_generation,
        || true
    ));
    let _ = update(&mut app, Message::Wake(Wake::MotionReady));
    assert_eq!(app.motion, 0.75);
    app.visible = true;
    assert!(app.motion_mailbox.publish(
        make_frame(generation, 0.5),
        &app.motion_generation,
        || true
    ));
    app.motion_generation.fetch_add(1, Ordering::AcqRel);
    let _ = update(&mut app, Message::Wake(Wake::MotionReady));
    assert_eq!(app.motion, 0.75);
    assert!(app.motion_mailbox.take().is_none());
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

fn d4a_motion_tick(app: &mut GpuCandidate, dt: f32) -> bool {
    let mut state = app.depth_camera.unwrap();
    let mut selection = app.depth_selection.clone();
    let mut presentation = app.depth_presentation;
    let moving = advance_gallery_motion(
        &mut state,
        &mut selection,
        &mut presentation,
        DepthPresentation::target(&app.state),
        dt,
    );
    let generation = app.motion_generation.load(Ordering::Acquire);
    assert!(app.motion_mailbox.publish(
        Wake::MotionFrame {
            generation,
            value: state.value(),
            velocity: state.velocity,
            state,
            selection,
            presentation,
            ordinary: None,
            hex: None,
            wall: None,
            scene: None,
        },
        &app.motion_generation,
        || true,
    ));
    let _ = update(app, Message::Wake(Wake::MotionReady));
    moving
}
fn d4a_detail_tick(app: &mut GpuCandidate, dt: f32) {
    let mut spring = app.depth_detail.unwrap();
    let moving = spring.tick(dt);
    let generation = app.detail_generation.load(Ordering::Acquire);
    let _ = update(
        app,
        Message::Wake(Wake::DetailFrame {
            generation,
            value: spring.value(),
            close_when_done: spring.target < 0.5 && !moving,
            spring: Some(spring),
        }),
    );
}
fn d4a_scene(app: &GpuCandidate) -> DonorSceneState {
    DonorSceneState {
        stage: donor_layout::StageParams::default(),
        camera: 0.0,
        camera_target: 0.0,
        camera_velocity: 0.0,
        detail_progress: app.detail_progress,
        external_detail: app.depth_panel_open(),
        flip_options: app.flip_presentation(),
        detail_source: app.detail_source,
        detail_card: app.external_detail_card(),
        viewport: app.viewport,
        time: 0.0,
        visibility: 1.0,
    }
}
#[test]
fn d4a_all_controls_actual_dispatch_ranges_nonfinite_and_reload() {
    let (mut app, submissions) = d4a_fixture(40);
    let old = (
        app.state.gallery.slices.clone(),
        app.state.gallery.geometric.clone(),
        app.state.gallery.wall.clone(),
    );
    let fields = [
        (GalleryNumberField::DepthHeight, 100.0, 1600.0, 10.0),
        (GalleryNumberField::DepthWidth, 40.0, 2400.0, 10.0),
        (GalleryNumberField::DepthSpacing, 0.0, 4000.0, 10.0),
        (GalleryNumberField::DepthCount, 3.0, 21.0, 1.0),
        (GalleryNumberField::DepthFalloff, 0.0, 1.0, 0.01),
        (GalleryNumberField::DepthCorners, 0.0, 100.0, 1.0),
        (GalleryNumberField::DepthSkew, -200.0, 200.0, 1.0),
        (GalleryNumberField::DepthNavigation, 35.0, 8000.0, 50.0),
        (GalleryNumberField::DepthFlipDuration, 100.0, 5000.0, 50.0),
    ];
    for (field, lo, hi, step) in fields {
        assert_eq!(field.step(), step);
        for (input, expected) in [
            (lo - 100.0, lo),
            (hi + 100.0, hi),
            ((lo + hi) * 0.5, (lo + hi) * 0.5),
        ] {
            let _ = update(&mut app, Message::SetGalleryNumber(field, input));
            let expected = if matches!(field, GalleryNumberField::DepthFlipDuration) {
                expected.round()
            } else {
                expected
            };
            assert_eq!(gallery_number_value(&app.state, field), expected);
            assert_eq!(app.state.gallery.variant, None);
        }
        let before = app.state.gallery.depth.clone();
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let _ = update(&mut app, Message::SetGalleryNumber(field, value));
        }
        assert_eq!(app.state.gallery.depth, before);
    }
    for message in [
        Message::SetDepthFrame(true),
        Message::SetDepthShadows(true),
        Message::SetDepthEffect("Tonal Layers".into()),
        Message::SetDepthShader(false),
        Message::SetDepthBack(false),
    ] {
        let _ = update(&mut app, message);
    }
    for _ in 0..300 {
        if !d4a_motion_tick(&mut app, 0.016) {
            break;
        }
    }
    let target = DepthPresentation::target(&app.state);
    assert!(!app.depth_presentation.unwrap().unsettled(target));
    let frame = app.depth_frame.as_ref().unwrap();
    assert!(frame.selection_frame && frame.shadows);
    assert_eq!(frame.detail.effect, 11);
    assert!(!frame.detail.animate_shader && !frame.detail.animate_back);
    assert_eq!(
        (
            app.state.gallery.slices.clone(),
            app.state.gallery.geometric.clone(),
            app.state.gallery.wall.clone()
        ),
        old
    );
    let loaded = CandidateState::load(&app.state_path);
    assert_eq!(loaded.gallery.depth, app.state.gallery.depth);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
    let _ = position_content(&app);
    let _ = motion_settings_content(&app);
}
#[test]
fn d4a_morph_frame_demand_hits_resize_and_stale_generation_share_current_geometry() {
    let (mut app, _) = d4a_fixture(100);
    let start = app.depth_presentation.unwrap();
    let old_revision = app.depth_frame.as_ref().unwrap().revision;
    let stale_generation = app.motion_generation.load(Ordering::Acquire);
    for (field, value) in [
        (GalleryNumberField::DepthWidth, 800.0),
        (GalleryNumberField::DepthHeight, 300.0),
        (GalleryNumberField::DepthSpacing, 400.0),
        (GalleryNumberField::DepthCount, 12.0),
        (GalleryNumberField::DepthFalloff, 0.05),
        (GalleryNumberField::DepthCorners, 80.0),
        (GalleryNumberField::DepthSkew, -180.0),
    ] {
        let _ = update(&mut app, Message::SetGalleryNumber(field, value));
    }
    assert_eq!(
        app.depth_presentation.unwrap().params.width,
        start.params.width
    );
    assert!(d4a_motion_tick(&mut app, 0.016));
    let current = app.depth_presentation.unwrap();
    assert!(current.params.width > start.params.width && current.params.width < 800.0);
    assert!(current.params.corner_radius > 0.0 && current.params.corner_radius < 80.0);
    let target = DepthPresentation::target(&app.state);
    let alpha = 1.0 - (-0.016_f32 / 0.0625).exp();
    assert!(
        (current.params.width - (start.params.width + (800.0 - start.params.width) * alpha)).abs()
            < 0.001
    );
    let revision = app.depth_frame.as_ref().unwrap().revision;
    assert_ne!(revision, old_revision);
    let camera = app.depth_camera.unwrap();
    let _ = update(
        &mut app,
        Message::Wake(Wake::MotionFrame {
            generation: stale_generation,
            value: 99.0,
            velocity: 0.0,
            state: camera,
            selection: HashMap::new(),
            presentation: Some(start),
            ordinary: None,
            hex: None,
            wall: None,
            scene: None,
        }),
    );
    assert_eq!(app.depth_frame.as_ref().unwrap().revision, revision);
    let id = app.window.unwrap();
    let _ = update(
        &mut app,
        Message::WindowResized {
            id,
            width: 600.0,
            height: 950.0,
        },
    );
    assert_eq!(
        app.depth_presentation.unwrap().params.width,
        current.params.width
    );
    let frame = app.depth_frame.as_ref().unwrap();
    for c in &frame.cards {
        assert!(frame
            .poster_indices(&app.filtered_indices)
            .contains(&c.card.index));
        let hit = frame.hit(c.card.center[0], c.card.center[1]);
        assert!(hit.is_some());
    }
    assert_eq!(app.state.gallery.depth.width, 800.0);
    let mut capped = current;
    let mut fifty = current;
    capped.tick(target, 5.0);
    fifty.tick(target, 0.05);
    assert_eq!(capped.params.width, fifty.params.width);
    for _ in 0..500 {
        if !d4a_motion_tick(&mut app, 0.016) {
            break;
        }
    }
    assert!(!app.depth_presentation.unwrap().unsettled(target));
    let mut falloff = target;
    falloff.params.falloff = 0.0;
    let mut near = target;
    near.params.falloff = 0.000009;
    falloff.tick(near, 0.016);
    assert_eq!(falloff.params.falloff, near.params.falloff);
    near.params.falloff = 0.00005;
    falloff.params.falloff = 0.0;
    falloff.tick(near, 0.016);
    assert!(falloff.params.falloff < near.params.falloff);
}
#[test]
fn d4a_camera_reversal_retime_and_selection_restart_actual_dispatch() {
    let (mut app, _) = d4a_fixture(12);
    d3_key(&mut app, keyboard::key::Named::ArrowRight);
    d3_key(&mut app, keyboard::key::Named::ArrowRight);
    for _ in 0..6 {
        d4a_motion_tick(&mut app, 0.016);
    }
    let before = app.depth_camera.unwrap();
    assert!(before.velocity > 0.0);
    let selected_before = app.depth_selection["item-0002"];
    d3_key(&mut app, keyboard::key::Named::ArrowLeft);
    let reversed = app.depth_camera.unwrap();
    assert_eq!(
        (reversed.value(), reversed.velocity),
        (before.value(), before.velocity)
    );
    assert_eq!(reversed.target, 1.0);
    let off = app.depth_selection["item-0002"];
    assert_eq!(
        (off.value(), off.velocity, off.target),
        (selected_before.value(), 0.0, 0.0)
    );
    let _ = update(
        &mut app,
        Message::SetGalleryNumber(GalleryNumberField::DepthNavigation, 35.0),
    );
    let retimed = app.depth_camera.unwrap();
    assert_eq!(
        (retimed.value(), retimed.target, retimed.velocity),
        (reversed.value(), 1.0, 0.0)
    );
    let mut expected = retimed;
    expected.retime_absolute(35.0);
    expected.tick(0.016);
    let mut selection = app.depth_selection["item-0001"];
    let mut standard = selection;
    standard.retime_absolute(250.0);
    selection.tick(0.016);
    standard.tick(0.016);
    assert_eq!(selection.value(), standard.value());
    let revision = app.depth_frame.as_ref().unwrap().revision;
    let _ = update(
        &mut app,
        Message::Gallery(GalleryMessage::DepthPointer {
            revision,
            item_id: Some("item-0003".into()),
            right: false,
            hover: false,
        }),
    );
    let mut newly_selected = app.depth_selection["item-0003"];
    let mut new_expected = GalleryMotionState::absolute_camera(0.0, 1.0, 35.0, 50.0, 50.0);
    newly_selected.tick(0.016);
    new_expected.tick(0.016);
    assert_eq!(newly_selected.value(), new_expected.value());
}
#[test]
fn d4a_default_and_named_preset_morph_only_depth_and_blocked_save_is_truthful() {
    let (mut app, _) = d4a_fixture(10);
    let _ = update(
        &mut app,
        Message::SetGalleryNumber(GalleryNumberField::DepthWidth, 650.0),
    );
    let _ = update(&mut app, Message::SetDepthEffect("Ash".into()));
    let _ = update(&mut app, Message::SetDepthShadows(true));
    let saved = app.state.gallery.depth.clone();
    let _ = update(&mut app, Message::SaveGalleryPreset);
    let _ = update(&mut app, Message::DepthDefault);
    assert_eq!(
        app.state.gallery.depth,
        candidate_state::DepthSettings::default()
    );
    for _ in 0..300 {
        if !d4a_motion_tick(&mut app, 0.016) {
            break;
        }
    }
    let before = app.depth_presentation.unwrap();
    let id = app
        .state
        .mode_profiles
        .selected_id(GalleryKind::Depth)
        .unwrap();
    let _ = update(
        &mut app,
        Message::ApplyGalleryPreset(GalleryKind::Depth, id),
    );
    assert_eq!(app.state.gallery.depth, saved);
    assert_eq!(
        app.depth_presentation.unwrap().params.width,
        before.params.width
    );
    d4a_motion_tick(&mut app, 0.016);
    assert!(app.depth_presentation.unwrap().params.width > before.params.width);
    assert_eq!(app.state.gallery.variant, None);
    let future = format!("version = {}\n", candidate_state::CURRENT_STATE_VERSION + 1);
    std::fs::write(&app.state_path, &future).unwrap();
    let _ = update(&mut app, Message::SetDepthFrame(true));
    assert!(app.save_failure.is_some());
    assert_eq!(std::fs::read_to_string(&app.state_path).unwrap(), future);
}
#[test]
fn d4a_all_effects_and_four_flag_combinations_use_actual_shader_payload_and_detail_phases() {
    let (mut app, submissions) = d4a_fixture(4);
    for (expected, effect) in candidate_state::DEPTH_EFFECTS.into_iter().enumerate() {
        let _ = update(&mut app, Message::SetDepthEffect(effect.into()));
        for (shader, back) in [(true, true), (false, true), (true, false), (false, false)] {
            let _ = update(&mut app, Message::SetDepthShader(shader));
            let _ = update(&mut app, Message::SetDepthBack(back));
            app.cancel_detail("test-stage-reset");
            app.start_detail(true, "test-open");
            if !shader && !back {
                assert_eq!(app.detail_progress, 1.0);
            }
            for p in [0.0, 0.4, 0.74, 0.82, 0.94, 1.0] {
                app.detail_progress = p;
                let frame = app.depth_frame.as_ref().unwrap();
                let instance = donor_scene::depth_instances(
                    frame,
                    &GalleryColors::default(),
                    1.0,
                    &d4a_scene(&app),
                );
                let i = frame.cards.iter().position(|c| c.card.selected).unwrap();
                let payload = instance[i].flip;
                if shader && p > 0.001 {
                    assert_eq!(payload[2], expected as f32);
                    assert!(payload[0] > 0.0);
                } else {
                    assert_eq!(payload, [0.0; 4]);
                }
                let phases = donor_scene::card_flip_phases(p, shader, back);
                if shader && !back {
                    assert_eq!(phases.content, f32::from(p >= 0.82));
                }
                if !shader && back && p == 0.4 {
                    assert!(phases.surface > 0.0 && phases.content > 0.0);
                }
                if shader && back && p == 0.74 {
                    assert_eq!(phases.surface, 0.0);
                    assert_eq!(phases.content, 0.0);
                }
            }
            let _ = view(&app, app.window.unwrap());
            app.start_detail(false, "test-close");
            if !shader && !back {
                assert_eq!(app.detail_progress, 0.0);
                assert!(!app.detail_open);
            }
        }
    }
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}
#[test]
fn d4a_mid_detail_reversal_retime_option_edits_identity_and_stale_callbacks() {
    let (mut app, _) = d4a_fixture(8);
    for duration in [100.0, 1500.0, 5000.0] {
        let _ = update(
            &mut app,
            Message::SetGalleryNumber(GalleryNumberField::DepthFlipDuration, duration),
        );
        for steps in [1, 6, 25] {
            app.cancel_detail("test-reset");
            app.start_detail(true, "test-open");
            for _ in 0..steps {
                d4a_detail_tick(&mut app, 0.016);
            }
            let before = app.depth_detail.unwrap();
            app.start_detail(false, "test-close");
            let closing = app.depth_detail.unwrap();
            assert_eq!(
                (closing.x, closing.velocity, closing.target),
                (before.x, before.velocity, 0.0)
            );
            d4a_detail_tick(&mut app, 0.016);
            let before = app.depth_detail;
            if before.is_none() {
                assert!(!app.detail_open && app.detail_progress == 0.0);
                assert!(app.depth_detail_id.is_none());
            }
            app.start_detail(true, "test-reopen");
            let expected = before.map_or((0.0, 0.0), |spring| (spring.x, spring.velocity));
            assert_eq!(
                (
                    app.depth_detail.unwrap().x,
                    app.depth_detail.unwrap().velocity
                ),
                expected,
                "source early close completion starts fresh; live reversal retains raw state"
            );
        }
    }
    let old_generation = app.detail_generation.load(Ordering::Acquire);
    let _ = update(&mut app, Message::SetDepthEffect("Rack Focus".into()));
    assert_eq!(
        (
            app.depth_detail.unwrap().value(),
            app.depth_detail.unwrap().velocity,
            app.depth_detail.unwrap().target
        ),
        (0.0, 0.0, 1.0),
        "source effect change restarts an open flip"
    );
    let before = app.depth_detail.unwrap();
    let _ = update(&mut app, Message::SetDepthShader(false));
    let _ = update(&mut app, Message::SetDepthBack(false));
    assert_eq!(app.detail_progress, before.value());
    let _ = update(
        &mut app,
        Message::SetGalleryNumber(GalleryNumberField::DepthFlipDuration, 100.0),
    );
    let retimed = app.depth_detail.unwrap();
    assert_eq!(
        (retimed.value(), retimed.target, retimed.velocity),
        (before.value(), before.target, 0.0)
    );
    let _ = update(
        &mut app,
        Message::Wake(Wake::DetailFrame {
            generation: old_generation,
            value: 0.0,
            close_when_done: true,
            spring: Some(before),
        }),
    );
    assert_eq!(app.detail_progress, before.value());
    app.start_detail(false, "both-off-close");
    assert_eq!(app.detail_progress, 0.0);
    app.start_detail(true, "both-off-open");
    assert_eq!(app.detail_progress, 1.0);
    app.selected = Some(3);
    app.rebuild_depth_frame();
    assert!(!app.detail_open);
    assert!(app.depth_detail.is_none());
}
#[test]
fn d4a_frame_hover_shadow_order_crop_rounding_and_actual_action_revisions() {
    let (mut app, submissions) = d4a_fixture(12);
    for (field, value) in [
        (GalleryNumberField::DepthSkew, -200.0),
        (GalleryNumberField::DepthCorners, 100.0),
        (GalleryNumberField::DepthSpacing, 0.0),
    ] {
        let _ = update(&mut app, Message::SetGalleryNumber(field, value));
    }
    for _ in 0..300 {
        if !d4a_motion_tick(&mut app, 0.016) {
            break;
        }
    }
    let _ = update(&mut app, Message::SetDepthShadows(true));
    for _ in 0..300 {
        if !d4a_motion_tick(&mut app, 0.016) {
            break;
        }
    }
    app.depth_hover = Some("item-0001".into());
    app.rebuild_depth_frame();
    for border in [false, true] {
        let _ = update(&mut app, Message::SetDepthFrame(border));
        for _ in 0..300 {
            if !d4a_motion_tick(&mut app, 0.016) {
                break;
            }
        }
        let f = app.depth_frame.as_ref().unwrap();
        let bodies =
            donor_scene::depth_instances(f, &GalleryColors::default(), 2.0, &d4a_scene(&app));
        for (entry, pair) in f.cards.iter().zip(bodies.chunks_exact(2)) {
            let (x, y, a) = if entry.card.selected {
                (4.0, 10.0, 0.5)
            } else {
                (2.0, 5.0, 0.3)
            };
            assert_eq!(pair[0].rect[0], pair[1].rect[0] + x * 2.0);
            assert_eq!(pair[0].rect[1], pair[1].rect[1] + y * 2.0);
            assert_eq!(pair[0].fill[3], a);
            assert_eq!(pair[0].misc[0], 0);
            assert_eq!(pair[0].params[2], entry.card.opacity);
            assert_eq!(pair[0].radii, pair[1].radii);
            assert_eq!(pair[0].params[0], pair[1].params[0]);
            assert_eq!(pair[1].crop, entry.crop_uv);
            if !border {
                assert_eq!(pair[1].border[3], 0.0);
                assert_eq!(pair[1].params[1], 0.0);
                assert_eq!(pair[1].misc[3] & 4, 0);
            } else {
                assert_eq!(
                    pair[1].params[1],
                    (1.0 + 2.0 * entry.selection_progress.clamp(0.0, 1.0)) * 2.0
                );
            }
        }
        assert_eq!(
            f.hit(app.viewport[0] * 0.5, app.viewport[1] * 0.5)
                .unwrap()
                .id,
            "item-0000"
        );
    }
    app.start_detail(true, "test-open");
    for _ in 0..500 {
        d4a_detail_tick(&mut app, 0.016);
    }
    let revision = app.depth_frame.as_ref().unwrap().revision;
    let _ = update(
        &mut app,
        Message::DepthDonorDetail(
            revision,
            "item-0000".into(),
            donor_back_layout::DetailIntent::Favorite,
        ),
    );
    assert!(app.state.favorites.contains("item-0000"));
    app.rebuild_depth_frame();
    let _ = update(
        &mut app,
        Message::DepthDonorDetail(
            revision,
            "item-0000".into(),
            donor_back_layout::DetailIntent::Favorite,
        ),
    );
    assert!(app.state.favorites.contains("item-0000"));
    let revision = app.depth_frame.as_ref().unwrap().revision;
    let _ = update(
        &mut app,
        Message::DepthDonorDetail(
            revision,
            "item-0007".into(),
            donor_back_layout::DetailIntent::Apply,
        ),
    );
    let _ = update(
        &mut app,
        Message::DepthDonorDetail(
            revision,
            "item-0000".into(),
            donor_back_layout::DetailIntent::Apply,
        ),
    );
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn d4a_current_card_is_undimmed_while_selection_progress_is_intermediate() {
    let (mut app, _) = d4a_fixture(10);
    let frame = app.depth_frame.as_mut().unwrap();
    for entry in &mut frame.cards {
        entry.selection_progress = 0.25;
    }
    let frame = app.depth_frame.as_ref().unwrap();
    let bodies =
        donor_scene::depth_instances(frame, &GalleryColors::default(), 1.0, &d4a_scene(&app));
    for (entry, body) in frame.cards.iter().zip(&bodies) {
        if entry.card.selected {
            assert_eq!(body.tint[3], 0.0, "logical current remains undimmed");
        } else {
            assert_eq!(body.tint[3], 0.4 * 0.75);
        }
        assert_eq!(body.border[3], 0.0);
        assert_eq!(body.params[1], 0.0);
        assert_eq!(body.misc[3] & 4, 0);
    }
}
#[test]
fn d4a_nondefault_shadows_reach_actual_instance_path() {
    let (mut app, _) = d3_application_fixture(8);
    let previous = app.state.gallery.depth.clone();
    app.state.gallery.depth.shadows = true;
    app.depth_settings_changed(&previous);
    for _ in 0..300 {
        if !d4a_motion_tick(&mut app, 0.016) {
            break;
        }
    }
    app.rebuild_depth_frame();
    let f = app.depth_frame.as_ref().unwrap();
    let raws = donor_scene::depth_instances(
        f,
        &GalleryColors::default(),
        1.0,
        &DonorSceneState {
            stage: donor_layout::StageParams::default(),
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
        },
    );
    assert_eq!(
        raws.len(),
        f.cards.len() * 2,
        "each shadow must precede its body"
    );
}
#[test]
fn d4a_both_off_request_snaps_real_detail_owner() {
    let (mut app, _) = d3_application_fixture(3);
    app.state.card_flip_mut().animate_shader = false;
    app.state.card_flip_mut().animate_back = false;
    app.start_detail(true, "d4a-test");
    assert_eq!(app.detail_progress, 1.0);
    assert_eq!(
        app.depth_detail.unwrap().value(),
        1.0,
        "both-off must settle its spring too"
    );
    app.start_detail(false, "d4a-test");
    assert_eq!(app.depth_detail.unwrap().value(), 0.0);
    assert!(!app.detail_open);
}

#[test]
fn d3_same_mode_actual_consumer_retains_detail_and_motion() {
    let (mut app, _) = d3_application_fixture(3);
    app.detail_open = true;
    app.detail_progress = 0.5;
    app.depth_hover = Some("item-0001".into());
    app.depth_wheel = 0.75;
    let generations = (
        app.motion_generation.load(Ordering::Acquire),
        app.detail_generation.load(Ordering::Acquire),
        app.poster_generation.load(Ordering::Acquire),
    );
    let revision = app.depth_frame.as_ref().unwrap().revision;
    let _ = update(&mut app, Message::SetGalleryMode(GalleryMode::Depth));
    assert!(app.detail_open, "same-mode selector must retain detail");
    assert_eq!(app.detail_progress, 0.5);
    assert_eq!(app.depth_hover.as_deref(), Some("item-0001"));
    assert_eq!(app.depth_wheel, 0.75);
    assert_eq!(
        (
            app.motion_generation.load(Ordering::Acquire),
            app.detail_generation.load(Ordering::Acquire),
            app.poster_generation.load(Ordering::Acquire)
        ),
        generations
    );
    assert_eq!(app.depth_frame.as_ref().unwrap().revision, revision);
}

#[test]
fn d3_studio_depth_entry_restarts_the_shared_entrance_transition() {
    let (mut app, _) = d3_application_fixture(3);
    app.deterministic_animation = true;
    app.gallery_mode = GalleryMode::Slices;
    app.state.gallery.mode = GalleryKind::Slices;
    app.depth_camera = None;
    app.depth_presentation = None;
    app.panel_state.settings_open = true;
    app.settings_workbench.tab = "picker".into();
    app.settings_workbench.navigation.section = view::settings::composed::PICKER_LAYOUT_SECTION;

    app.set_gallery_mode(GalleryMode::Depth);

    assert_eq!(app.gallery_mode, GalleryMode::Depth);
    assert!(app.depth_presentation.is_some());
    assert!(
        app.gallery_entrance_opacity() < 1.0,
        "Depth studio entry must use the shared entrance transition"
    );
}

#[test]
fn d3_actual_consumer_right_click_reverses_closing_target_with_velocity() {
    let (mut app, submissions) = d3_application_fixture(3);
    let pointer = |app: &GpuCandidate| {
        Message::Gallery(GalleryMessage::DepthPointer {
            revision: app.depth_frame.as_ref().unwrap().revision,
            item_id: Some("item-0000".into()),
            right: true,
            hover: false,
        })
    };
    let message = pointer(&app);
    let _ = update(&mut app, message);
    assert!(app.detail_open);
    assert_eq!(app.depth_detail.unwrap().target, 1.0);
    let message = pointer(&app);
    let _ = update(&mut app, message);
    assert_eq!(app.depth_detail.unwrap().target, 0.0);
    let mut closing = GalleryMotionState::absolute_camera(0.7, 0.0, 1500.0, 100.0, 100.0);
    closing.tick(0.016);
    let generation = app.detail_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::Wake(Wake::DetailFrame {
            generation,
            value: closing.value(),
            close_when_done: false,
            spring: Some(closing),
        }),
    );
    app.detail_open = true; // the real close path remains drawable until settled
    let message = pointer(&app);
    let _ = update(&mut app, message);
    let reopened = app.depth_detail.unwrap();
    assert_eq!(
        reopened.target, 1.0,
        "right click during closing must reopen"
    );
    assert_eq!(reopened.value(), closing.value());
    assert_eq!(reopened.velocity, closing.velocity);
    let _ = update(
        &mut app,
        Message::Wake(Wake::DetailFrame {
            generation,
            value: 0.0,
            close_when_done: true,
            spring: Some(closing),
        }),
    );
    assert!(
        app.detail_open,
        "old close callback cannot close reopened detail"
    );
    assert_eq!(submissions.load(Ordering::Acquire), 0);
    assert!(!app.apply_pending);
    assert!(app.active_operation.is_none());
}
#[test]
fn d3_actual_consumer_camera_detail_and_mailbox_lifetimes() {
    for window_present in [true, false] {
        let (mut app, submissions) = d3_application_fixture(100);
        if !window_present {
            app.window = None;
        }
        let mut camera = GalleryMotionState::absolute_camera(0.0, 99.0, 250.0, 100.0, 500.0);
        camera.tick(0.032);
        let generation = app.motion_generation.load(Ordering::Acquire);
        let _ = update(
            &mut app,
            Message::Wake(Wake::MotionFrame {
                generation,
                value: camera.value(),
                velocity: camera.velocity,
                state: camera,
                selection: HashMap::new(),
                presentation: None,
                ordinary: None,
                hex: None,
                wall: None,
                scene: None,
            }),
        );
        assert_eq!(app.depth_camera.unwrap().value(), camera.value());
        assert!(app
            .depth_frame
            .as_ref()
            .unwrap()
            .poster_indices(&app.filtered_indices)
            .contains(&(camera.value().round() as usize)));
        let revision = app.depth_frame.as_ref().unwrap().revision;
        let stale = GalleryMotionState::absolute_camera(99.0, 99.0, 250.0, 100.0, 500.0);
        let _ = update(
            &mut app,
            Message::Wake(Wake::MotionFrame {
                generation: generation.wrapping_sub(1),
                value: 99.0,
                velocity: 0.0,
                state: stale,
                selection: HashMap::new(),
                presentation: None,
                ordinary: None,
                hex: None,
                wall: None,
                scene: None,
            }),
        );
        assert_eq!(app.depth_frame.as_ref().unwrap().revision, revision);
        app.detail_open = true;
        app.detail_progress = 0.7;
        app.depth_hover = Some("item-0001".into());
        let detail_generation = app.detail_generation.load(Ordering::Acquire);
        let _ = app.set_visible(false, "d3-test-hide");
        assert!(app.depth_frame.is_none());
        assert!(app.depth_hover.is_none());
        assert!(app.depth_camera.is_none());
        assert!(!app.detail_open);
        assert_eq!(app.detail_progress, 0.0);
        assert!(app.motion_generation.load(Ordering::Acquire) > generation);
        assert!(app.detail_generation.load(Ordering::Acquire) > detail_generation);
        assert!(app.poster_mailbox.take().unwrap().is_none());
        let _ = update(
            &mut app,
            Message::Wake(Wake::MotionFrame {
                generation,
                value: 99.0,
                velocity: 0.0,
                state: stale,
                selection: HashMap::new(),
                presentation: None,
                ordinary: None,
                hex: None,
                wall: None,
                scene: None,
            }),
        );
        let _ = update(
            &mut app,
            Message::Wake(Wake::DetailFrame {
                generation: detail_generation,
                value: 1.0,
                close_when_done: false,
                spring: None,
            }),
        );
        assert!(app.depth_frame.is_none());
        assert!(!app.detail_open);
        assert_eq!(app.detail_progress, 0.0);
        assert_eq!(submissions.load(Ordering::Acquire), 0);
    }
}
#[test]
fn d3_actual_consumer_paging_overlay_wheel_empty_and_disabled_apply() {
    use keyboard::key::Named;
    for count in [0, 1, 3, 100] {
        let (mut app, submissions) = d3_application_fixture(count);
        let live = app.depth_frame.as_ref().unwrap().cards.len().max(1);
        let before = app.selected;
        app.panel_state.search_open = true;
        d3_key(&mut app, Named::PageDown);
        let _ = update(
            &mut app,
            Message::Gallery(GalleryMessage::Wheel(-2.0, None)),
        );
        assert_eq!(app.selected, before);
        assert_eq!(app.depth_wheel, 0.0);
        app.panel_state.search_open = false;
        d3_key(&mut app, Named::ArrowUp);
        d3_key(&mut app, Named::ArrowDown);
        assert_eq!(app.selected, before);
        d3_key(&mut app, Named::PageDown);
        assert_eq!(app.selected, (count > 0).then(|| live.min(count - 1)));
        d3_key(&mut app, Named::Home);
        assert_eq!(app.selected, (count > 0).then_some(0));
        for _ in 0..3 {
            let _ = update(
                &mut app,
                Message::Gallery(GalleryMessage::Wheel(-0.25, None)),
            );
        }
        assert_eq!(app.selected, (count > 0).then_some(0));
        let _ = update(
            &mut app,
            Message::Gallery(GalleryMessage::Wheel(-0.25, None)),
        );
        assert_eq!(app.selected, (count > 0).then(|| 1.min(count - 1)));
        d3_key(&mut app, Named::End);
        assert_eq!(app.selected, count.checked_sub(1));
        d3_key(&mut app, Named::Enter);
        assert!(!app.apply_pending);
        assert!(app.active_operation.is_none());
        assert_eq!(submissions.load(Ordering::Acquire), 0);
        assert!(app
            .status
            .as_deref()
            .is_some_and(|s| s.contains("Apply is disabled")));
    }
}

#[test]
fn d3_actual_matching_window_close_cancels_only_owned_depth_work() {
    let (mut app, _) = d3_application_fixture(3);
    let id = app.window.unwrap();
    app.detail_open = true;
    app.detail_progress = 0.6;
    app.depth_hover = Some("item-0001".into());
    let generation = app.motion_generation.load(Ordering::Acquire);
    let detail = app.detail_generation.load(Ordering::Acquire);
    let revision = app.depth_frame.as_ref().unwrap().revision;
    let _ = update(&mut app, Message::WindowClosed(window::Id::unique()));
    assert_eq!(app.window, Some(id));
    assert!(app.visible);
    assert!(app.detail_open);
    assert_eq!(app.depth_frame.as_ref().unwrap().revision, revision);
    assert_eq!(app.motion_generation.load(Ordering::Acquire), generation);
    let _ = update(&mut app, Message::WindowClosed(id));
    assert!(!app.visible);
    assert!(app.window.is_none());
    assert!(app.depth_frame.is_none());
    assert!(app.depth_camera.is_none());
    assert!(app.depth_hover.is_none());
    assert!(app.depth_selection.is_empty());
    assert!(!app.detail_open);
    assert_eq!(app.detail_progress, 0.0);
    assert!(app.motion_generation.load(Ordering::Acquire) > generation);
    assert!(app.detail_generation.load(Ordering::Acquire) > detail);
}
#[test]
fn idle_poster_retry_waits_for_terminal_and_retries_the_same_demand_once() {
    use std::fs;
    use std::os::windows::fs::OpenOptionsExt;
    use std::sync::atomic::AtomicBool;
    use std::sync::{mpsc, Mutex};
    use std::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(15);

    for release_before_retry in [true, false] {
        let (mut app, _) = d4a_fixture(2);
        let root = app.state_path.parent().unwrap();
        let healthy = root.join("healthy.png");
        let locked = root.join("locked.png");
        for (index, path) in [&healthy, &locked].into_iter().enumerate() {
            image::RgbaImage::from_pixel(4, 3, image::Rgba([160, 30, 40, 255]))
                .save(path)
                .unwrap();
            app.wallpapers[index].file_target = path.clone();
        }
        app.catalog = app
            .wallpapers
            .iter()
            .map(library::CatalogEntry::from_wallpaper)
            .collect();
        app.clear_posters();
        let previous = gpu_gallery::decode_poster(&locked, 2).unwrap();
        let stamp = library::media_still::Fingerprint::capture(&locked).unwrap();
        fs::OpenOptions::new()
            .write(true)
            .open(&locked)
            .unwrap()
            .set_modified(stamp.modified + Duration::from_secs(10))
            .unwrap();
        assert!(!stamp.matches(&locked));
        app.poster_slots.insert(
            1,
            PosterSlot {
                layer: 2,
                kind: poster_worker::PosterKind::Still,
                generation: app.poster_generation.load(Ordering::Acquire),
                last_requested: app.poster_generation.load(Ordering::Acquire),
                catalog_generation: app.catalog_generation.load(Ordering::Acquire),
                item_id: app.wallpapers[1].id.clone(),
                source: locked.clone(),
            },
        );
        app.posters = Arc::new(vec![previous]);

        let mut lease = Some(
            fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(&locked)
                .unwrap(),
        );

        let mailbox = Arc::clone(&app.poster_mailbox);
        let (at_terminal, terminal_blocked) = mpsc::channel();
        let (release_terminal, continue_terminal) = mpsc::channel();
        let continue_terminal = Mutex::new(continue_terminal);
        let (finished, settled) = mpsc::channel();
        let first_terminal = AtomicBool::new(true);

        let worker =
            poster_worker::PosterWorker::spawn(Arc::clone(&app.poster_generation), move |delta| {
                let generation = delta.generation;
                let terminal = delta.finished;
                if terminal && first_terminal.swap(false, Ordering::AcqRel) {
                    if at_terminal.send(generation).is_err()
                        || continue_terminal
                            .lock()
                            .unwrap()
                            .recv_timeout(TIMEOUT)
                            .is_err()
                    {
                        return false;
                    }
                }
                if mailbox.publish(delta, || true).is_err() {
                    return false;
                }
                !terminal || finished.send(generation).is_ok()
            })
            .unwrap();
        let worker_id = worker.id();
        app.poster_worker = Some(worker);

        let deliver =
            |app: &mut GpuCandidate, wake| events::update_inner(app, Message::Wake(wake)).units();
        app.schedule_posters();
        let first = app.poster_generation.load(Ordering::Acquire);
        assert_eq!(terminal_blocked.recv_timeout(TIMEOUT).unwrap(), first);
        assert_eq!(deliver(&mut app, Wake::PostersReady), 0);
        assert!(app.poster_loading);
        assert_eq!(app.poster_retry, PosterRetry::SawLock);
        assert_eq!(app.poster_slots.len(), 1);
        assert!(app.poster_slots.contains_key(&0));
        assert!(app.poster_failed_indices.contains(&1));
        let first_healthy = app
            .posters
            .iter()
            .find(|poster| poster.source == healthy)
            .unwrap()
            .clone();

        release_terminal.send(()).unwrap();
        assert_eq!(settled.recv_timeout(TIMEOUT).unwrap(), first);
        assert_eq!(deliver(&mut app, Wake::PostersReady), 1);
        assert!(!app.poster_loading);
        assert_eq!(app.poster_retry, PosterRetry::Waiting);

        let key = app.poster_request_key.clone();
        let assignment = app.poster_assignment_key.clone();
        for (wake_generation, visible, cache_operation) in [
            (first.wrapping_sub(1), true, None),
            (first, false, None),
            (first, true, Some(77)),
        ] {
            app.visible = visible;
            app.cache_operation = cache_operation;
            assert_eq!(deliver(&mut app, Wake::RetryPosters(wake_generation)), 0);
            assert_eq!(app.poster_generation.load(Ordering::Acquire), first);
            assert_eq!(app.poster_retry, PosterRetry::Waiting);
            assert_eq!(app.poster_request_key, key);
            assert_eq!(app.poster_assignment_key, assignment);
        }
        app.visible = true;
        app.cache_operation = None;
        let original_filter = app.filtered_indices.clone();
        let resident_before = Arc::clone(&app.posters);
        let slots_before = app.poster_slots.clone();
        let revision_before = app.depth_revision;
        for changed in [vec![1], Vec::new()] {
            app.filtered_indices = changed;
            assert_eq!(deliver(&mut app, Wake::RetryPosters(first)), 0);
            assert_eq!(app.poster_generation.load(Ordering::Acquire), first);
            assert_eq!(app.poster_retry, PosterRetry::Waiting);
            assert_eq!(app.poster_request_key, key);
            assert_eq!(app.poster_assignment_key, assignment);
            assert_eq!(app.depth_revision, revision_before);
            assert_eq!(app.poster_slots, slots_before);
            assert!(Arc::ptr_eq(&app.posters, &resident_before));
        }
        app.filtered_indices = original_filter;
        let assigned_layer = |app: &GpuCandidate| {
            app.poster_assignment_key
                .as_ref()
                .unwrap()
                .0
                .iter()
                .find(|(index, _)| *index == 1)
                .unwrap()
                .1
        };
        assert_eq!(assigned_layer(&app), 2);
        app.schedule_posters();
        let remap = app.poster_generation.load(Ordering::Acquire);
        assert_eq!(remap, first + 1);
        assert_eq!(assigned_layer(&app), 1);
        assert_eq!(app.poster_retry, PosterRetry::Available);
        assert_eq!(settled.recv_timeout(TIMEOUT).unwrap(), remap);
        assert_eq!(deliver(&mut app, Wake::PostersReady), 1);
        assert_eq!(app.poster_retry, PosterRetry::Waiting);
        assert_eq!(deliver(&mut app, Wake::RetryPosters(first)), 0);
        assert_eq!(app.poster_generation.load(Ordering::Acquire), remap);
        assert_eq!(app.poster_retry, PosterRetry::Waiting);
        if release_before_retry {
            drop(lease.take());
        }

        assert_eq!(deliver(&mut app, Wake::RetryPosters(remap)), 0);
        let retry = app.poster_generation.load(Ordering::Acquire);
        assert_eq!(retry, remap + 1);
        assert!(app.poster_loading);
        assert_eq!(app.poster_retry, PosterRetry::Spent);
        assert_eq!(app.poster_worker.as_ref().unwrap().id(), worker_id);
        assert_eq!(app.selected, Some(0));
        assert_eq!(settled.recv_timeout(TIMEOUT).unwrap(), retry);
        assert_eq!(deliver(&mut app, Wake::PostersReady), 0);
        assert!(!app.poster_loading);
        assert_eq!(app.poster_retry, PosterRetry::Spent);
        assert!(app.posters.iter().any(|p| p.same_resource(&first_healthy)));
        assert_eq!(app.poster_slots.contains_key(&1), release_before_retry);
        assert_eq!(
            app.poster_failed_indices.contains(&1),
            !release_before_retry
        );
        if release_before_retry {
            let still = library::media_still::MediaStill::select(
                &app.wallpapers[1],
                app.catalog_generation.load(Ordering::Acquire),
            )
            .unwrap();
            let recovered = app
                .posters
                .iter()
                .find(|poster| poster.source == locked)
                .unwrap();
            assert!(recovered.matches_source(
                poster_worker::PosterKind::Still,
                Some(poster_worker::PosterStamp::Still(still.fingerprint)),
            ));
        }

        for generation in [first, remap, retry] {
            assert_eq!(deliver(&mut app, Wake::RetryPosters(generation)), 0);
        }
        app.schedule_posters();
        assert_eq!(app.poster_generation.load(Ordering::Acquire), retry);

        if !release_before_retry {
            for former_layer in [3, 4] {
                let assignments = &mut app.poster_assignment_key.as_mut().unwrap().0;
                assignments
                    .iter_mut()
                    .find(|(index, _)| *index == 1)
                    .unwrap()
                    .1 = former_layer;
                app.schedule_posters();
                let natural = app.poster_generation.load(Ordering::Acquire);
                assert_eq!(app.poster_retry, PosterRetry::Spent);
                assert_eq!(settled.recv_timeout(TIMEOUT).unwrap(), natural);
                assert_eq!(deliver(&mut app, Wake::PostersReady), 0);
                assert_eq!(app.poster_retry, PosterRetry::Spent);
                assert!(app.poster_failed_indices.contains(&1));
            }
        }
        drop(lease);
        drop(app.poster_worker.take());
        assert!(matches!(
            settled.recv_timeout(TIMEOUT),
            Err(mpsc::RecvTimeoutError::Disconnected)
        ));
        fs::remove_file(locked).unwrap();
        fs::remove_file(healthy).unwrap();
    }
}

#[test]
fn p22_gallery_merges_first_poster_while_later_decode_is_held() {
    use std::sync::{mpsc, Mutex};
    use std::time::Duration;

    const TIMEOUT: Duration = Duration::from_secs(5);
    let (mut app, _) = d3_application_fixture(3);
    app.gallery_mode = GalleryMode::Slices;
    app.depth_frame = None;
    app.poster_slots.clear();
    app.posters = Arc::new(Vec::new());
    let generation = app.poster_generation.fetch_add(1, Ordering::AcqRel) + 1;
    let sources: Vec<_> = app
        .wallpapers
        .iter()
        .enumerate()
        .map(|(index, item)| poster_worker::PosterSource {
            index,
            layer: index as u32,
            item_id: item.id.clone(),
            path: poster_source_for_item(item),
            kind: gallery::poster_kind_for_item(item),
            video_stamp: None,
            still: None,
        })
        .collect();
    app.poster_mailbox
        .expect(generation, sources.clone())
        .unwrap();
    app.poster_loading = true;

    let (wake_tx, wake_rx) = mpsc::channel();
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
            let published = mailbox.publish(completion, || wake_tx.send(()).is_ok());
            if finished {
                finished_tx.send(()).unwrap();
            }
            published.is_ok()
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
    worker
        .submit(poster_worker::PosterBatch::new(
            generation,
            sources,
            app.poster_cache_dir.clone(),
        ))
        .unwrap();

    held_rx.recv_timeout(TIMEOUT).unwrap();
    wake_rx.recv_timeout(TIMEOUT).unwrap();
    let _ = update(&mut app, Message::Wake(Wake::PostersReady));
    assert!(app.poster_loading);
    assert_eq!(app.poster_slots.len(), 1);
    assert_eq!(app.poster_slots[&0].generation, generation);
    assert_eq!(app.posters.len(), 1);
    let first = app.posters[0].clone();
    assert!(finished_rx.try_recv().is_err());

    release_tx.send(()).unwrap();
    finished_rx.recv_timeout(TIMEOUT).unwrap();
    wake_rx.recv_timeout(TIMEOUT).unwrap();
    assert!(
        wake_rx.try_recv().is_err(),
        "one wake carries both deltas and EOF"
    );
    let _ = update(&mut app, Message::Wake(Wake::PostersReady));
    assert!(!app.poster_loading);
    assert!(!app.poster_failure);
    assert_eq!(app.poster_slots.len(), 3);
    assert_eq!(app.posters.len(), 3);
    assert!(app
        .posters
        .iter()
        .any(|poster| poster.same_resource(&first)));
}

#[test]
fn p34_retryable_poster_submissions_keep_worker_and_retry_on_motion_tick() {
    use std::sync::mpsc;
    use std::time::Duration;

    for error in [
        poster_worker::SubmitError::CacheBusy,
        poster_worker::SubmitError::Closed,
    ] {
        let (mut app, submissions) = d3_application_fixture(2);
        for item in &mut app.wallpapers {
            // This fixture exercises queue retry, so give the fake decoder
            // a real placeholder identity without a video file dependency.
            item.source = WallpaperSource::WallpaperEngine;
            item.wallpaper_type = "scene".into();
        }
        let mailbox = app.poster_mailbox.clone();
        let (finished_tx, finished_rx) = mpsc::channel();
        let worker = poster_worker::PosterWorker::spawn_with_builder(
            std::thread::Builder::new(),
            app.poster_generation.clone(),
            move |completion| {
                let finished = completion.finished;
                let published = mailbox.publish(completion, || true).is_ok();
                if finished {
                    finished_tx.send(()).unwrap();
                }
                published
            },
            |source, _| {
                gpu_gallery::placeholder_if_wanted(
                    &source.path,
                    source.layer,
                    source.kind,
                    None,
                    &|| true,
                )
            },
        )
        .unwrap();
        let worker_id = worker.id();
        worker.reject_next_submission(error);
        app.poster_worker = Some(worker);
        app.poster_loading = true;
        app.schedule_posters();
        assert_eq!(app.poster_worker.as_ref().unwrap().id(), worker_id);
        assert!(!app.poster_loading);
        assert!(!app.poster_failure);
        assert!(app.poster_request_key.is_none());
        assert!(app.poster_mailbox.take().unwrap().is_none());
        let rejected_generation = app.poster_generation.load(Ordering::Acquire);

        d4a_motion_tick(&mut app, 1.0 / 60.0);
        assert!(app.poster_loading);
        assert!(app.poster_request_key.is_some());
        assert!(app.poster_generation.load(Ordering::Acquire) > rejected_generation);
        assert_eq!(app.poster_worker.as_ref().unwrap().id(), worker_id);
        finished_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let _ = update(&mut app, Message::Wake(Wake::PostersReady));
        assert!(!app.poster_loading);
        assert!(!app.poster_failure);
        assert_eq!(app.poster_slots.len(), 2);
        assert_eq!(submissions.load(Ordering::Acquire), 0);
    }
}

#[test]
fn p34_retryable_colour_submissions_preserve_cursor_and_retry_on_motion_tick() {
    // Restore the global wake route so this fixture cannot affect other tests.
    struct RestoreWake(Option<UnboundedSender<Wake>>);
    impl Drop for RestoreWake {
        fn drop(&mut self) {
            *WAKE_TX.get().unwrap().lock().unwrap() = self.0.take();
        }
    }
    let (tx, mut rx) = unbounded();
    let _restore = RestoreWake(
        WAKE_TX
            .get_or_init(|| Mutex::new(None))
            .lock()
            .unwrap()
            .replace(tx),
    );
    let (mut app, submissions) = d3_application_fixture(3);
    for index in [0, 2] {
        let path = app
            .state_path
            .parent()
            .unwrap()
            .join(format!("p34-{index}.png"));
        image::RgbaImage::from_pixel(8, 8, image::Rgba([230, 20, 10, 255]))
            .save(&path)
            .unwrap();
        app.wallpapers[index].source = WallpaperSource::LocalImage;
        app.wallpapers[index].file_target = path.clone();
        app.wallpapers[index].preview_path = Some(path);
    }
    let worker =
        poster_worker::PosterWorker::spawn(app.poster_generation.clone(), |_| true).unwrap();
    let worker_id = worker.id();
    app.poster_worker = Some(worker);
    app.colours.active = true;
    for error in [
        poster_worker::SubmitError::CacheBusy,
        poster_worker::SubmitError::Closed,
    ] {
        app.poster_worker
            .as_ref()
            .unwrap()
            .reject_next_submission(error);
        app.schedule_colour();
        assert_eq!(app.poster_worker.as_ref().unwrap().id(), worker_id);
        assert!(app.colours.active);
        assert_eq!(app.colours.cursor, 0);
        assert_eq!(app.colours.pending, 0);
        assert_eq!(app.colours.settled, 0);
        assert_eq!(app.colours.unavailable, 0);
        assert!(app.colours.entries.is_empty());
    }
    d4a_motion_tick(&mut app, 1.0 / 60.0);
    assert_eq!(app.poster_worker.as_ref().unwrap().id(), worker_id);
    assert_eq!(app.colours.cursor, 3);
    assert_eq!(app.colours.pending, 2);
    assert_eq!(app.colours.settled, 1);
    assert_eq!(app.colours.unavailable, 1);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while app.colours.active && std::time::Instant::now() < deadline {
        if let Ok(wake) = rx.try_recv() {
            let _ = update(&mut app, Message::Wake(wake));
        } else {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    assert!(!app.colours.active);
    assert_eq!(app.colours.pending, 0);
    assert_eq!(app.colours.settled, 3);
    assert_eq!(app.colours.unavailable, 1);
    assert_eq!(app.colours.metadata.len(), 2);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn p33_worker_exit_settles_loading_and_stale_exit_preserves_replacement() {
    use std::sync::mpsc;

    let (mut app, _) = d3_application_fixture(1);
    app.wallpapers[0].source = WallpaperSource::WallpaperEngine;
    app.wallpapers[0].wallpaper_type = "scene".into();
    let source = poster_worker::PosterSource {
        index: 0,
        layer: 0,
        item_id: app.wallpapers[0].id.clone(),
        path: poster_source_for_item(&app.wallpapers[0]),
        kind: crate::poster_worker::PosterKind::Placeholder,
        video_stamp: None,
        still: None,
    };
    let (exit_tx, exit_rx) = mpsc::channel();
    let profile = app.state_path.parent().unwrap().to_owned();
    let cache_dir = app.poster_cache_dir.clone();
    let first = poster_worker::PosterWorker::spawn_with_cache(
        app.poster_generation.clone(),
        |_| panic!("controlled panic in the poster worker coordinator"),
        move |exit| exit_tx.send(exit).unwrap(),
        profile.clone(),
        None,
        Some(1),
        true,
    )
    .unwrap();
    let first_id = first.id();
    app.poster_worker = Some(first);
    app.schedule_posters();
    let generation = app.poster_generation.load(Ordering::Acquire);
    assert!(app.poster_loading);
    let request_key = app.poster_request_key.clone();
    let assignment_key = app.poster_assignment_key.clone();
    assert!(request_key.is_some());
    assert!(assignment_key.is_some());

    let timeout = std::time::Duration::from_secs(5);
    let exit = exit_rx.recv_timeout(timeout).unwrap();
    assert_eq!(exit.worker_id, first_id);
    assert_eq!(exit.failure, poster_worker::WorkerFailure::WorkerExited);
    assert_eq!(
        exit_rx.recv_timeout(timeout),
        Err(mpsc::RecvTimeoutError::Disconnected),
    );
    app.cache_operation = Some(1);
    let _ = update(&mut app, Message::Wake(Wake::PosterWorkerExited(exit)));
    assert!(!app.poster_loading);
    assert!(app.poster_worker.is_none());
    assert!(app.poster_failure);
    assert_eq!(app.cache_operation, None);
    assert_eq!(
        app.cache_status,
        "Poster worker stopped before the cache operation was acknowledged.",
    );
    assert_eq!(
        app.status_for_display().as_deref(),
        Some("Poster preview worker stopped: poster worker exited unexpectedly"),
    );
    let replacement_generation = app.poster_generation.load(Ordering::Acquire);
    assert!(replacement_generation > generation);
    for _ in 0..3 {
        app.schedule_posters();
        assert!(app.poster_worker.is_none());
        assert!(!app.poster_loading);
        assert_eq!(
            app.poster_generation.load(Ordering::Acquire),
            replacement_generation
        );
        assert_eq!(app.poster_request_key, request_key);
        assert_eq!(app.poster_assignment_key, assignment_key);
    }

    let mailbox = app.poster_mailbox.clone();
    let (finished_tx, finished_rx) = mpsc::channel();
    let (exit_tx, expected_shutdown) = mpsc::channel();
    let replacement = poster_worker::PosterWorker::spawn_with_cache(
        app.poster_generation.clone(),
        move |completion| {
            let finished = completion.finished;
            let published = mailbox.publish(completion, || true).is_ok();
            if finished {
                finished_tx.send(()).unwrap();
            }
            published
        },
        move |exit| exit_tx.send(exit).unwrap(),
        profile,
        None,
        Some(1),
        true,
    )
    .unwrap();
    let replacement_id = replacement.id();
    assert_ne!(replacement_id, first_id);
    app.poster_mailbox
        .expect(replacement_generation, vec![source.clone()])
        .unwrap();
    app.poster_loading = true;
    replacement
        .submit(poster_worker::PosterBatch::new(
            replacement_generation,
            vec![source],
            cache_dir,
        ))
        .unwrap();
    app.poster_worker = Some(replacement);
    finished_rx.recv_timeout(timeout).unwrap();
    let _ = update(&mut app, Message::Wake(Wake::PosterWorkerExited(exit)));
    assert_eq!(app.poster_worker.as_ref().unwrap().id(), replacement_id);
    assert_eq!(
        app.poster_generation.load(Ordering::Acquire),
        replacement_generation,
    );
    assert!(app.poster_loading);
    let _ = update(&mut app, Message::Wake(Wake::PostersReady));
    assert!(!app.poster_loading);
    assert!(!app.poster_failure);
    assert_eq!(app.poster_slots[&0].generation, replacement_generation);
    assert!(app.status_for_display().is_none());

    app.poster_worker = None;
    assert_eq!(
        expected_shutdown.recv_timeout(timeout),
        Err(mpsc::RecvTimeoutError::Disconnected),
    );
}

#[test]
fn d3_actual_filter_empty_mode_and_legacy_poster_consumer_reject_stale_identity() {
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
        GalleryMode::Depth,
    ] {
        let (mut app, _) = d3_application_fixture(3);
        let _ = update(&mut app, Message::SetGalleryMode(mode));
        let wallpaper = &app.wallpapers[1];
        let source = poster_worker::PosterSource {
            index: 1,
            layer: 0,
            item_id: wallpaper.id.clone(),
            path: poster_source_for_item(wallpaper),
            kind: crate::poster_worker::PosterKind::Still,
            video_stamp: None,
            still: None,
        };
        let completion = |generation: u64| poster_worker::PosterCompletion {
            generation,
            cursor: 1,
            finished: true,
            failures: Vec::new(),
            posters: vec![poster_worker::DecodedPoster {
                source: source.clone(),
                poster: gpu_gallery::video_placeholder_poster(&source.path, 0),
            }],
        };
        let generation = app.poster_generation.fetch_add(1, Ordering::AcqRel) + 1;
        app.poster_mailbox
            .expect(generation, vec![source.clone()])
            .unwrap();
        app.poster_mailbox
            .publish(completion(generation), || true)
            .unwrap();
        let _ = update(&mut app, Message::Wake(Wake::PostersReady));
        assert_eq!(app.posters.len(), 1);
        assert_eq!(app.poster_slots.get(&1).unwrap().item_id, source.item_id);
        // Catalog changes between retention and consumption still fail closed.
        let generation = app.poster_generation.fetch_add(1, Ordering::AcqRel) + 1;
        app.poster_mailbox
            .expect(generation, vec![source.clone()])
            .unwrap();
        app.poster_mailbox
            .publish(completion(generation), || true)
            .unwrap();
        app.wallpapers.swap(0, 1);
        let _ = update(&mut app, Message::Wake(Wake::PostersReady));
        assert!(app.posters.is_empty());
        assert!(app.poster_slots.is_empty());
        app.wallpapers.swap(0, 1);
        let generation = app.poster_generation.fetch_add(1, Ordering::AcqRel) + 1;
        app.poster_mailbox
            .expect(generation, vec![source.clone()])
            .unwrap();
        app.poster_mailbox
            .publish(completion(generation), || true)
            .unwrap();
        let _ = update(&mut app, Message::SetSourceFilter(SourceFilter::Videos));
        assert!(app.filtered_indices.is_empty());
        assert!(app.selected.is_none());
        assert!(app.poster_mailbox.take().unwrap().is_none());
        let _ = update(&mut app, Message::Wake(Wake::PostersReady));
        assert!(app.posters.is_empty());
        if mode == GalleryMode::Depth {
            assert!(app.depth_frame.as_ref().unwrap().cards.is_empty());
        }
        let _ = update(&mut app, Message::SetGalleryMode(GalleryMode::Wall));
        assert!(app.depth_frame.is_none());
        assert!(app.depth_hover.is_none());
    }
}

#[test]
fn d3_actual_widget_publication_and_consumer_keep_ids_and_button_semantics() {
    use iced::widget::shader::Program;
    let wallpapers = vec![test_wallpaper("master-a"), test_wallpaper("master-b")];
    let items = [
        gpu_gallery::DepthItem {
            id: "master-b",
            master_index: 1,
            source_aspect: None,
        },
        gpu_gallery::DepthItem {
            id: "master-a",
            master_index: 0,
            source_aspect: None,
        },
    ];
    let frame = gpu_gallery::DepthSnapshot::from_frame(gpu_gallery::depth_frame(
        &items,
        90,
        Some("master-b"),
        None,
        0.0,
        [1200.0, 900.0],
        [600.0, 1200.0],
        donor_layout::DepthParams::default(),
        1.0,
    ));
    let point = frame
        .cards
        .iter()
        .find(|c| c.id == "master-b")
        .unwrap()
        .card
        .center;
    let program = GalleryProgram {
        prewarm: None,
        poster_feedback: None,
        snapshot: Arc::new(GallerySnapshot {
            defer_near_restore: false,
            held_wall: None,
            ordinary: None,
            cards: frame.cards.iter().map(|c| c.card).collect(),
            wall_clip: None,
            posters: Arc::new(vec![]),
            colors: GalleryColors::default(),
            depth: Some(frame.clone()),
            scene: DonorSceneState {
                stage: donor_layout::StageParams::default(),
                camera: 0.0,
                camera_target: 0.0,
                camera_velocity: 0.0,
                detail_progress: 0.0,
                external_detail: false,
                flip_options: Default::default(),
                detail_source: None,
                detail_card: None,
                viewport: [1200.0, 900.0],
                time: 0.0,
                visibility: 1.0,
            },
        }),
    };
    for (button, right) in [
        (iced::mouse::Button::Left, false),
        (iced::mouse::Button::Right, true),
    ] {
        let action = program
            .update(
                &mut Default::default(),
                &iced::Event::Mouse(iced::mouse::Event::ButtonPressed(button)),
                iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(1200.0, 900.0)),
                iced::mouse::Cursor::Available(iced::Point::new(point[0], point[1])),
            )
            .unwrap();
        let (message, _, capture) = action.into_inner();
        let message = message.unwrap();
        let GalleryMessage::Pointer {
            id: item_id,
            revision: Some(revision),
            button: actual_button,
            ..
        } = message
        else {
            panic!("captured Depth pointer expected")
        };
        assert_eq!(
            actual_button,
            if right {
                candidate_state::binding_codec::MouseButton::Right
            } else {
                candidate_state::binding_codec::MouseButton::Left
            }
        );
        // Preserve the legacy pure Depth geometry/ownership assertions below;
        // W2 local_bindings tests separately exercise the new app reducer route.
        let message = GalleryMessage::DepthPointer {
            revision,
            item_id,
            right,
            hover: false,
        };
        assert_eq!(capture, iced::event::Status::Captured);
        assert_eq!(
            consume_depth_pointer(
                Some(&frame),
                &wallpapers,
                Some(0),
                false,
                true,
                false,
                &message
            ),
            DepthPointerIntent::Select(1)
        );
        assert_eq!(
            consume_depth_pointer(
                Some(&frame),
                &wallpapers,
                Some(1),
                false,
                true,
                false,
                &message
            ),
            DepthPointerIntent::Detail(true)
        );
        assert_eq!(
            consume_depth_pointer(
                Some(&frame),
                &wallpapers,
                Some(1),
                true,
                true,
                false,
                &message
            ),
            DepthPointerIntent::Detail(!right)
        );
        assert_eq!(
            consume_depth_pointer(
                Some(&frame),
                &wallpapers,
                Some(1),
                false,
                true,
                true,
                &message
            ),
            DepthPointerIntent::Ignore
        );
        assert_eq!(
            consume_depth_pointer(
                Some(&frame),
                &wallpapers,
                Some(1),
                false,
                false,
                false,
                &message
            ),
            DepthPointerIntent::Ignore
        );
        let mut newer = frame.clone();
        newer.revision += 1;
        assert_eq!(
            consume_depth_pointer(
                Some(&newer),
                &wallpapers,
                Some(1),
                false,
                true,
                false,
                &message
            ),
            DepthPointerIntent::Ignore
        );
        let swapped = vec![test_wallpaper("master-b"), test_wallpaper("master-a")];
        assert_eq!(
            consume_depth_pointer(
                Some(&frame),
                &swapped,
                Some(1),
                false,
                true,
                false,
                &message
            ),
            DepthPointerIntent::Ignore
        );
    }
    let leaving = program
        .update(
            &mut Default::default(),
            &iced::Event::Mouse(iced::mouse::Event::CursorLeft),
            iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(1200.0, 900.0)),
            iced::mouse::Cursor::Unavailable,
        )
        .unwrap()
        .into_inner()
        .0
        .unwrap();
    assert_eq!(
        consume_depth_pointer(
            Some(&frame),
            &wallpapers,
            Some(1),
            false,
            true,
            false,
            &leaving
        ),
        DepthPointerIntent::Hover(None)
    );
    // The shared consumer has no provider/automation action; disabled Apply
    // remains independently guarded by the existing explicit owner tests.
}
#[test]
fn d3_wheel_uses_source_fractional_and_large_delta_accumulator() {
    let mut remainder = 0.0;
    for value in [0.25, 0.25, 0.25] {
        assert_eq!(depth_wheel_steps(&mut remainder, value), 0);
    }
    assert_eq!(depth_wheel_steps(&mut remainder, 0.25), 1);
    assert_eq!(remainder, 0.0);
    assert_eq!(depth_wheel_steps(&mut remainder, -0.4), 0);
    assert_eq!(depth_wheel_steps(&mut remainder, -0.7), -1);
    assert!((remainder + 0.1).abs() < 0.0001);
    assert_eq!(depth_wheel_steps(&mut remainder, 2.6), 3);
    assert_eq!(remainder, 0.0);
    assert_eq!(depth_wheel_steps(&mut remainder, f32::NAN), 0);
}
#[test]
fn d3_depth_presets_only_restore_depth_and_empty_variants_are_safe() {
    let mut gallery = candidate_state::GallerySettings::default();
    gallery.slices.width = 777.0;
    gallery.wall.columns = 11;
    gallery.geometric.radius = 99.0;
    let legacy = (
        gallery.slices.clone(),
        gallery.geometric.clone(),
        gallery.wall.clone(),
    );
    let depth = candidate_state::DepthSettings {
        width: 555.0,
        ..Default::default()
    };
    let preset = candidate_state::GalleryPreset {
        name: "Depth named".into(),
        mode: GalleryKind::Depth,
        variant: Some(7),
        depth: Some(candidate_state::LegacyDepthSettings::try_from(&depth).unwrap()),
        slices: Default::default(),
        geometric: Default::default(),
        wall: Default::default(),
    };
    apply_saved_gallery_preset(&mut gallery, &preset);
    assert_eq!(gallery.depth, depth);
    assert_eq!(
        (
            gallery.slices.clone(),
            gallery.geometric.clone(),
            gallery.wall.clone()
        ),
        legacy
    );
    assert_eq!(gallery.variant, None);
    assert_eq!(next_gallery_variant_index(Some(5), 0), 0);
    assert!(GalleryMode::Depth.variant_labels().is_empty());
    assert_eq!(GalleryMode::Depth.variant_label(99), "Custom");
    for from in [
        GalleryKind::Slices,
        GalleryKind::Geometric,
        GalleryKind::Wall,
        GalleryKind::Depth,
    ] {
        for to in [
            GalleryKind::Slices,
            GalleryKind::Geometric,
            GalleryKind::Wall,
            GalleryKind::Depth,
        ] {
            let before = gallery.depth.clone();
            let mut effects = vec![];
            navigate_gallery_mode(&mut gallery, from, to, |e| effects.push(e));
            assert_eq!(gallery.depth, before);
            assert_eq!(
                (
                    gallery.slices.clone(),
                    gallery.geometric.clone(),
                    gallery.wall.clone()
                ),
                legacy
            );
            assert_eq!(
                effects.contains(&GalleryNavigationEffect::InvalidatePosterWork),
                from != to
            );
        }
    }
}

#[test]
fn r4_queued_obsolete_tick_is_a_noop_then_current_tick_is_eligible() {
    let mut fixture = AutomationFixture::new();
    fixture.state.slideshow_enabled = true;
    let before = (
        fixture.state.clone(),
        fixture.minute,
        fixture.due,
        fixture.selected.clone(),
    );
    let mut queue = std::collections::VecDeque::from([AutomationFixture::tick(7)]);
    fixture.refresh(); // Settings replace G with G+1 after G was queued.
    fixture.deliver(queue.pop_front().unwrap());
    assert_eq!(fixture.effects, [0; 5]);
    assert_eq!(
        (
            fixture.state.clone(),
            fixture.minute,
            fixture.due,
            fixture.selected.clone()
        ),
        before
    );
    assert!(fixture.submissions.is_empty());
    fixture.deliver(AutomationFixture::tick(8));
    assert_eq!(fixture.effects, [1, 1, 1, 1, 1]);
    assert_eq!(fixture.minute, Some((2, 480)));
    assert_eq!(fixture.due, 900); // Schedule retains priority over slideshow.
    assert_eq!(
        fixture.submissions,
        vec![("a".into(), vec!["monitor1".into()], "schedule".into())]
    );
}

#[test]
fn r4_disable_reenable_and_arbitrary_nonmatches_are_noops() {
    let mut fixture = AutomationFixture::new();
    fixture.state.slideshow_enabled = true;
    let mut queue =
        std::collections::VecDeque::from([AutomationFixture::tick(7), AutomationFixture::tick(7)]);
    fixture.state.schedules[0].enabled = false;
    fixture.state.slideshow_enabled = false;
    fixture.refresh();
    queue.push_back(AutomationFixture::tick(8));
    fixture.state.schedules[0].enabled = true;
    fixture.state.slideshow_enabled = true;
    fixture.refresh();
    queue.extend([
        AutomationFixture::tick(0),
        AutomationFixture::tick(10),
        AutomationFixture::tick(u64::MAX),
    ]);
    let before = (fixture.state.clone(), fixture.minute, fixture.due);
    for tick in queue {
        fixture.deliver(tick);
        assert_eq!(fixture.effects, [0; 5]);
        assert_eq!((fixture.state.clone(), fixture.minute, fixture.due), before);
    }
    fixture.deliver(AutomationFixture::tick(9));
    assert_eq!(fixture.effects, [1, 1, 1, 1, 1]);
}

#[test]
fn r4_current_ticks_preserve_minute_due_and_event_time_policy() {
    let mut fixture = AutomationFixture::new();
    fixture.deliver(AutomationFixture::tick(7));
    fixture.deliver(AutomationFixture::tick(7));
    assert_eq!(fixture.effects, [2, 1, 1, 1, 1]);
    fixture.state.schedules[0].enabled = false;
    fixture.state.slideshow_enabled = true;
    fixture.state.slideshow_interval_seconds = 1; // Existing minimum is ten.
    fixture.deliver(AutomationFixture::tick(7));
    assert_eq!(fixture.due, 1_010);
    assert_eq!(fixture.submissions.last().unwrap().0, "b");
    assert_eq!(fixture.submissions.last().unwrap().2, "slideshow");
    fixture.deliver(AutomationFixture::tick(7));
    assert_eq!(fixture.effects, [4, 1, 2, 2, 2]); // Same event time is not due again.
}

#[test]
fn r4_current_ticks_preserve_disabled_holdoff_locks_and_submission_gates() {
    for guard in [
        "disabled",
        "holdoff",
        "locked",
        "pending",
        "unknown",
        "apply_disabled",
    ] {
        let mut fixture = AutomationFixture::new();
        fixture.state.slideshow_enabled = true;
        match guard {
            "disabled" => {
                fixture.state.schedules[0].enabled = false;
                fixture.state.slideshow_enabled = false;
            }
            "holdoff" => fixture.state.manual_holdoff_until_unix = 1_001,
            "locked" => {
                fixture.state.locked_monitors.insert("monitor1".into());
            }
            "pending" => fixture.pending = true,
            "unknown" => fixture.unknown = true,
            "apply_disabled" => fixture.apply_enabled = false,
            _ => unreachable!(),
        }
        fixture.deliver(AutomationFixture::tick(7));
        assert_eq!(fixture.effects[3..], [0, 0], "{guard}");
        assert!(fixture.submissions.is_empty(), "{guard}");
        // PK-03: a selected schedule claims this turn even when submission is blocked.
        // Only a rejected schedule allows the interval owner to consume its due time.
        let expected_due = if !matches!(guard, "locked") {
            900
        } else {
            1_300
        };
        assert_eq!(fixture.due, expected_due, "{guard}");
        assert_eq!(fixture.minute, Some((2, 480)), "{guard}");
        let expected_playlist_calls = match guard {
            "disabled" | "holdoff" => 0,
            _ => 1,
        };
        assert_eq!(fixture.effects[2], expected_playlist_calls, "{guard}");
    }
}

#[test]
fn r4_submission_gate_preserves_production_rejection_order() {
    use AutomationSubmissionBlock::*;
    assert_eq!(
        automation_submission_block(true, true, false),
        Some(ApplyPending)
    );
    assert_eq!(
        automation_submission_block(false, true, false),
        Some(ProviderStateUnknown)
    );
    assert_eq!(
        automation_submission_block(false, false, false),
        Some(ApplyDisabled)
    );
    assert_eq!(automation_submission_block(false, false, true), None);
}

#[test]
fn persistence_diagnostics_survive_catalog_status_updates() {
    assert_eq!(
        diagnostic_status(
            None,
            None,
            None,
            Some("Recovered candidate state from the backup."),
            None,
            Some("Ready".into()),
        ),
        Some("Recovered candidate state from the backup.".into())
    );
    let save_and_load = diagnostic_status(
        Some("Candidate state changed in memory but was not fully saved."),
        None,
        None,
        Some("Recovered candidate state from the backup."),
        None,
        Some("Ready".into()),
    )
    .unwrap();
    assert!(save_and_load.contains("Candidate state changed in memory but was not fully saved."));
    assert!(save_and_load.contains("Recovered candidate state from the backup."));
    let combined = diagnostic_status(
        None,
        Some("No valid backup is available."),
        Some("Configuration did not save."),
        Some("Recovered from the backup."),
        None,
        None,
    )
    .unwrap();
    assert!(combined.contains("No valid backup is available."));
    assert!(combined.contains("Configuration did not save."));
    assert!(combined.contains("Recovered from the backup."));
}

#[test]
fn successful_engine_status_retains_f6_load_diagnostic() {
    let status = successful_engine_persistence_status(
        "Wallpaper Engine command accepted",
        "the existing backup remains available",
        None,
        None,
        Some("Recovered candidate state from the backup."),
        None,
    );

    assert!(status.contains("Wallpaper Engine command accepted"));
    assert!(status.contains("existing backup remains available"));
    assert!(status.contains("Recovered candidate state from the backup."));
}

#[test]
fn failed_engine_schema_load_cannot_retain_previous_default_controls() {
    let root = std::env::temp_dir().join(format!(
        "wallpaper-switcher-gpu-schema-state-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let valid_path = root.join("valid.json");
    std::fs::write(
        &valid_path,
        r#"{"general":{"properties":{"enabled":{"type":"bool","value":true}}}}"#,
    )
    .unwrap();
    let valid = engine_properties::read_supported(&valid_path);
    let mut properties = valid.properties;
    let mut previous_values = serde_json::Map::new();
    for property in &properties {
        previous_values.insert(property.key.clone(), property.default_value.clone());
    }
    let mut diagnostic = None;
    assert!(!previous_values.is_empty());

    let failed = engine_properties::PropertySchema {
        properties: Vec::new(),
        state: engine_properties::PropertySchemaState::InvalidJson("fixture".into()),
    };
    replace_engine_property_controls(
        &mut properties,
        &mut previous_values,
        &mut diagnostic,
        &failed,
    );

    assert!(properties.is_empty());
    assert!(previous_values.is_empty());
    assert!(diagnostic
        .as_deref()
        .is_some_and(|message| message.contains("invalid JSON")));
    assert!(engine_property_schema_status_message(&failed)
        .unwrap()
        .contains("invalid JSON"));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn import_commit_changes_live_state_only_after_save_success() {
    let root = std::env::temp_dir().join(format!(
        "wsl-import-commit-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let blocked_parent = root.join("not-a-directory");
    std::fs::write(&blocked_parent, b"blocker").unwrap();
    let blocked_path = blocked_parent.join("candidate-state.toml");
    let before = CandidateState::default();
    let mut current = before.clone();
    let mut proposed = before.clone();
    proposed.favorites.insert("imported".into());

    let error = commit_saved_state(&mut current, proposed, &blocked_path).unwrap_err();

    assert_eq!(current, before);
    assert_eq!(error.stage, candidate_state::SaveStage::CreateDirectory);

    let state_path = root.join("candidate-state.toml");
    let first = commit_saved_state(&mut current, before.clone(), &state_path).unwrap();
    assert_eq!(first.backup, candidate_state::BackupStatus::Unavailable);
    let mut replacement = before;
    replacement.favorites.insert("imported".into());
    let second = commit_saved_state(&mut current, replacement, &state_path).unwrap();
    assert_eq!(second.backup, candidate_state::BackupStatus::Published);
    assert!(current.favorites.contains("imported"));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn d2_import_fresh_proposal_cannot_bypass_current_migration_provenance() {
    let root = std::env::temp_dir().join(format!(
        "d2-import-red-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("state.toml");
    std::fs::write(&path, b"version = 1\nfavorites = ['loaded']\n").unwrap();
    let mut live = CandidateState::load(&path);
    let before = live.clone();
    let external = b"version = 1\nfavorites = ['external']\n";
    std::fs::write(&path, external).unwrap();
    let mut proposed = CandidateState::default();
    proposed.favorites.insert("import".into());
    assert!(
        commit_saved_state(&mut live, proposed, &path).is_err(),
        "import fresh proposal bypassed loaded provenance"
    );
    assert_eq!(live, before);
    assert_eq!(std::fs::read(&path).unwrap(), external);
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn r5_supported_proposals_preserve_protected_session_and_diagnostics() {
    let root = std::env::temp_dir().join(format!(
        "wsl-r5-import-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("candidate-state.toml");
    let backup = path.with_extension("toml.bak");
    let bytes = format!(
        "# future profile\nversion = {}\n[new.nested]\nfield = 'untouched'\n",
        candidate_state::CURRENT_STATE_VERSION + 1
    )
    .into_bytes();
    let backup_bytes = b"# older automation must not activate\nslideshow_enabled = true\n";
    std::fs::write(&path, &bytes).unwrap();
    std::fs::write(&backup, backup_bytes).unwrap();
    let loaded = CandidateState::load_with_diagnostics(&path);
    let mut live = loaded.state;
    let before = live.clone();
    let preview = live.clone();
    assert_eq!(preview.write_restriction, live.write_restriction);
    assert_eq!(live, before);
    for mut proposed in [preview, CandidateState::default()] {
        proposed.favorites.insert("imported".into());
        let error = commit_saved_state(&mut live, proposed, &path).unwrap_err();
        assert_eq!(live, before);
        let message =
            state_save_failure_message("Could not save proposed candidate state", &error, &path);
        assert!(message.contains("were not fully saved"));
        assert!(message.contains(&format!(
            "Unsupported candidate-state version {}",
            candidate_state::CURRENT_STATE_VERSION + 1
        )));
        assert!(!message.contains("saved successfully"));
        let browsed = diagnostic_status(
            Some(&message),
            None,
            None,
            loaded.diagnostic.as_deref(),
            None,
            Some("Catalog ready".into()),
        )
        .unwrap();
        assert!(browsed.contains("were not fully saved"));
        assert!(browsed.contains("writes are refused for this session"));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        assert_eq!(std::fs::read(&backup).unwrap(), backup_bytes);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
    }
    std::fs::remove_dir_all(&root).unwrap();
    // Default proposals cannot unlock an already protected live session.
    commit_saved_state(&mut live, CandidateState::default(), &path).unwrap_err();
    assert_eq!(live, before);
    assert!(!root.exists());
}

#[test]
fn r5_shared_ui_save_and_proposal_latch_late_future_files_without_false_success() {
    for import in [false, true] {
        let root = std::env::temp_dir().join(format!(
            "wsl-r5-late-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = root.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        let mut live = CandidateState::default();
        live.favorites.insert("existing".into());
        live.save(&path).unwrap();
        let primary_bytes = std::fs::read(&path).unwrap();
        let newer = format!(
            "# late future backup\nversion = {}\n[new.nested]\nvalue = 'keep'\n",
            candidate_state::CURRENT_STATE_VERSION + 1
        )
        .into_bytes();
        std::fs::write(&backup, &newer).unwrap();
        let before = live.clone();
        let error = if import {
            let mut proposed = CandidateState::default();
            proposed.favorites.insert("imported".into());
            commit_saved_state(&mut live, proposed, &path).unwrap_err()
        } else {
            persist_candidate_state(&mut live, &path).unwrap_err()
        };
        assert!(live.write_restriction.is_some());
        assert_eq!(live.favorites, before.favorites);
        assert_eq!(live.gallery, before.gallery);
        let message = if import {
            state_save_failure_message("Could not save proposed candidate state", &error, &path)
        } else {
            state_save_failure_message("Could not save candidate state", &error, &path)
        };
        assert!(message.contains("writes are refused"));
        assert!(message.contains("unsupported newer backup is preserved"));
        if !import {
            assert!(message.contains("were not fully saved"));
        }
        let status = diagnostic_status(
            Some(&message),
            None,
            None,
            None,
            None,
            Some("Catalog ready".into()),
        )
        .unwrap();
        assert_eq!(status, message);
        assert_eq!(std::fs::read(&path).unwrap(), primary_bytes);
        assert_eq!(std::fs::read(&backup).unwrap(), newer);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2);
        std::fs::remove_dir_all(&root).unwrap();
        persist_candidate_state(&mut live, &path).unwrap_err();
        assert!(!root.exists());
    }
}

#[test]
fn populated_render_fixture_is_parseable_with_disabled_automation() {
    let state: CandidateState = toml::from_str(include_str!("../testdata/ui-organization.toml"))
        .expect("render profile must parse instead of silently falling back");
    assert_eq!(state.playlists.len(), 2);
    assert_eq!(state.schedules.len(), 1);
    assert!(!state.slideshow_enabled && state.schedules.iter().all(|s| !s.enabled));
    assert_eq!(state.active_playlist_id.as_deref(), Some("review-explicit"));
    assert!(matches!(
        &state.playlists[1].membership,
        PlaylistMembership::Smart { .. }
    ));
}

#[test]
fn every_open_sheet_blocks_gallery_shortcuts() {
    for i in 0..6 {
        let mut panels = PanelState::default();
        panels.search_open = i == 0;
        panels.settings_open = i == 1;
        assert!(sheet_owns_keyboard(panels, i == 2, i == 3, i == 4, i == 5));
    }
    assert!(!sheet_owns_keyboard(
        PanelState::default(),
        false,
        false,
        false,
        false
    ));
}

#[test]
fn configured_close_prefers_the_top_owned_sheet() {
    let panels = PanelState {
        search_open: false,
        settings_open: true,
        settings_category: SettingsCategory::Theme,
    };
    assert_eq!(
        top_owned_sheet(panels, false, true, false, false),
        Some(OwnedSheet::EngineProperties)
    );
    assert_eq!(
        top_owned_sheet(panels, true, false, false, false),
        Some(OwnedSheet::Playlist)
    );
    assert_eq!(
        top_owned_sheet(panels, false, false, false, false),
        Some(OwnedSheet::Settings)
    );
    assert_eq!(
        top_owned_sheet(PanelState::default(), false, false, false, false),
        None
    );
}

#[test]
fn provider_unknown_blocks_mutation_but_allows_explicit_reconciliation() {
    let reconcile = EngineCommand::ReconcileProvider {
        recovery: test_provider_recovery(91, "Monitor1", "fixture-project"),
    };
    let paused = EngineCommand::SetPaused(true);

    assert!(provider_mutation_allowed(false, None));
    assert!(!provider_mutation_allowed(true, None));
    assert!(!provider_mutation_allowed(true, Some(&paused)));
    assert!(provider_mutation_allowed(true, Some(&reconcile)));
}

#[test]
fn failed_reconciliation_keeps_unknown_and_the_exit_hold() {
    let recovery = test_provider_recovery(92, "Monitor1", "fixture-project");
    let mut pending = vec![recovery.clone()];

    let still_unknown = update_provider_recovery_state(
        &mut pending,
        Some(&EngineCommand::ReconcileProvider {
            recovery: recovery.clone(),
        }),
        false,
        true,
        std::slice::from_ref(&recovery),
    );

    assert!(still_unknown);
    assert_eq!(pending, vec![recovery]);
}

#[test]
fn child_exit_without_reconciliation_keeps_provider_recovery() {
    let recovery = test_provider_recovery(921, "Monitor1", "child-exit");
    let mut pending = vec![recovery.clone()];

    // Child settlement is not an independent provider readback and must
    // not clear a retained recovery without an explicit reconciliation.
    let still_unknown = update_provider_recovery_state(&mut pending, None, false, false, &[]);

    assert!(still_unknown);
    assert_eq!(pending, vec![recovery]);
}

#[test]
fn stale_reconciliation_cannot_clear_a_newer_unresolved_operation() {
    let stale = test_provider_recovery(93, "Monitor1", "stale");
    let newer = test_provider_recovery(94, "Monitor1", "newer");
    let mut pending = vec![newer.clone()];

    let still_unknown = update_provider_recovery_state(
        &mut pending,
        Some(&EngineCommand::ReconcileProvider { recovery: stale }),
        true,
        false,
        &[],
    );

    assert!(still_unknown);
    assert_eq!(pending, vec![newer]);
}

#[test]
fn matching_reconciliation_clears_only_its_retained_operation() {
    let target = test_provider_recovery(95, "Monitor1", "target");
    let unrelated = test_provider_recovery(96, "Monitor2", "unrelated");
    let mut pending = vec![target.clone(), unrelated.clone()];

    let still_unknown = update_provider_recovery_state(
        &mut pending,
        Some(&EngineCommand::ReconcileProvider { recovery: target }),
        true,
        false,
        &[],
    );

    assert!(still_unknown);
    assert_eq!(pending, vec![unrelated]);
}

#[test]
fn recovery_action_does_not_depend_on_the_current_selection() {
    let recovery = test_provider_recovery(97, "Monitor1", "catalog-item-that-disappeared");
    let pending = vec![recovery.clone()];

    assert!(provider_recovery_action_available_with(
        &pending,
        |record| coordinator::recovery_capabilities_for(record, &pending, true)
    ));
    assert_eq!(pending[0], recovery);
}

#[test]
fn r12_ui_exit_cancels_only_owner_eligible_preflight_and_waits_for_completion() {
    let mut preflight = test_provider_recovery(1210, "display-a", "unstarted");
    preflight.phase = coordinator::RecoveryPhase::Preflight;
    preflight.expected = coordinator::ProviderPostcondition::ProviderStateReadable {
        provider: coordinator::ProviderKind::WallpaperEngine,
        target_id: "display-a".into(),
        location: None,
    };
    let mut owner_pending = vec![preflight.clone()];
    let mut ui_pending = owner_pending.clone();
    let exit = exit_action_for_ui(
        false,
        true,
        &ui_pending,
        coordinator::shutdown_action_for(&owner_pending, false, true),
    );
    assert_eq!(
        exit,
        coordinator::ShutdownAction::CancelPreflight(preflight.clone())
    );
    let cancel = EngineCommand::CancelPreflight {
        recovery: preflight.clone(),
    };
    assert!(provider_mutation_allowed(true, Some(&cancel)));
    assert_eq!(
        exit_action_for_ui(true, true, &ui_pending, coordinator::ShutdownAction::Ready),
        coordinator::ShutdownAction::WaitForActive
    );
    coordinator::settle_matching_recovery(&mut owner_pending, &preflight);
    assert!(
        matches!(
            exit_action_for_ui(false, true, &ui_pending, coordinator::ShutdownAction::Ready),
            coordinator::ShutdownAction::Blocked(_)
        ),
        "Owner release alone is insufficient before the matching UI completion"
    );
    assert!(!update_provider_recovery_state(
        &mut ui_pending,
        Some(&cancel),
        true,
        false,
        &[]
    ));
    assert_eq!(
        exit_action_for_ui(
            false,
            false,
            &ui_pending,
            coordinator::shutdown_action_for(&owner_pending, false, true)
        ),
        coordinator::ShutdownAction::Ready
    );
}

#[test]
fn r12_ui_stale_cancel_other_records_and_unsettled_children_keep_exit_held() {
    let stale = test_provider_recovery(1211, "display-a", "old");
    let newer = test_provider_recovery(1212, "display-a", "new");
    let mut ui_pending = vec![newer.clone()];
    let stale_cancel = EngineCommand::CancelPreflight { recovery: stale };
    assert!(update_provider_recovery_state(
        &mut ui_pending,
        Some(&stale_cancel),
        true,
        false,
        &[]
    ));
    assert_eq!(ui_pending, vec![newer.clone()]);
    assert!(matches!(
        exit_action_for_ui(
            false,
            true,
            &ui_pending,
            coordinator::shutdown_action_for(&ui_pending, false, true)
        ),
        coordinator::ShutdownAction::Blocked(_)
    ));
    let empty = Vec::new();
    assert!(matches!(
        exit_action_for_ui(
            false,
            false,
            &empty,
            coordinator::shutdown_action_for(&empty, false, false)
        ),
        coordinator::ShutdownAction::Blocked(_)
    ));
}

#[test]
fn r12_ui_retry_availability_and_first_executable_record_follow_owner_capability() {
    let mut unsupported = test_provider_recovery(1213, "display-a", "properties");
    unsupported.expected = coordinator::ProviderPostcondition::ReadbackUnavailable {
        capability: "Property-value readback unsupported".into(),
    };
    let retry = test_provider_recovery(1214, "display-b", "supported");
    let pending = vec![unsupported.clone(), retry.clone()];
    assert!(provider_recovery_action_available_with(
        &pending,
        |record| coordinator::recovery_capabilities_for(record, &pending, true)
    ));
    let selected = pending
        .iter()
        .find(|record| {
            coordinator::recovery_capabilities_for(record, &pending, true).retry_observation
        })
        .unwrap();
    assert_eq!(selected, &retry);
    assert!(!provider_recovery_action_available_with(
        &pending,
        |record| coordinator::recovery_capabilities_for(record, &pending, false)
    ));
    assert!(!provider_recovery_action_available_with(
        std::slice::from_ref(&unsupported),
        |record| coordinator::recovery_capabilities_for(
            record,
            std::slice::from_ref(&unsupported),
            true
        )
    ));
    assert_eq!(
        pending[0], unsupported,
        "Current selection and unsupported values cannot become observed values"
    );
}

fn test_provider_recovery(
    operation_id: u64,
    target_id: &str,
    identity: &str,
) -> coordinator::ProviderRecovery {
    coordinator::ProviderRecovery {
        operation_id,
        wallpaper_id: Some("wallpaper-id".into()),
        target_id: target_id.into(),
        provider: coordinator::ProviderKind::WallpaperEngine,
        identity: identity.into(),
        expected: coordinator::ProviderPostcondition::EngineProject {
            location: target_id.into(),
            path: std::path::PathBuf::from(format!(r"C:\fixture\{identity}.json")),
        },
        reason: "fixture uncertainty".into(),
        phase: coordinator::RecoveryPhase::DispatchedOrUnknown,
        observation: None,
    }
}

#[test]
fn absence_guard_failed_unstarted_completion_does_not_hold_exit_or_clear_unrelated_work() {
    // The production Apply completion feeds has_unknown and recoveries into
    // this same state helper, then returns ApplyResult::error_message as status.
    let mut pending = Vec::new();
    assert!(!update_provider_recovery_state(
        &mut pending,
        None,
        false,
        false,
        &[]
    ));
    assert_eq!(
        exit_action_for_ui(
            false,
            false,
            &pending,
            coordinator::shutdown_action_for(&pending, false, true)
        ),
        coordinator::ShutdownAction::Ready
    );
    let dispatched = test_provider_recovery(1251, "display-b", "actual-dispatched-uncertainty");
    pending.push(dispatched.clone());
    assert!(update_provider_recovery_state(
        &mut pending,
        None,
        false,
        false,
        &[]
    ));
    assert_eq!(pending, vec![dispatched.clone()]);
    assert!(matches!(
        exit_action_for_ui(
            false,
            true,
            &pending,
            coordinator::shutdown_action_for(&pending, false, true)
        ),
        coordinator::ShutdownAction::Blocked(_)
    ));
    let stale = test_provider_recovery(1250, "display-b", "stale");
    assert!(update_provider_recovery_state(
        &mut pending,
        Some(&EngineCommand::CancelPreflight { recovery: stale }),
        true,
        false,
        &[]
    ));
    assert_eq!(pending, vec![dispatched]);
}

#[test]
fn external_detail_suspends_overlapping_chrome_and_close_restores_it() {
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
    ] {
        assert_eq!(detail_chrome_visibility(mode, false), (true, false));
        assert_eq!(
            detail_chrome_visibility(mode, true),
            (mode == GalleryMode::Slices, false)
        );
        assert_eq!(detail_chrome_visibility(mode, false), (true, false));
    }
}

fn test_wallpaper(id: &str) -> WallpaperItem {
    WallpaperItem {
        scan_metadata: None,
        id: id.into(),
        title: id.into(),
        folder_path: std::path::PathBuf::from("C:/wallpapers"),
        project_json_path: std::path::PathBuf::new(),
        file_target: std::path::PathBuf::from(format!("C:/wallpapers/{id}.png")),
        preview_path: None,
        wallpaper_type: "image".into(),
        source: WallpaperSource::LocalImage,
    }
}

#[test]
fn opening_search_focuses_query_and_preserves_filters_and_saved_appearance() {
    let mut panels = PanelState::default();
    let mut filter = SmartRule {
        text: "forest dusk".into(),
        minimum_width: 1920,
        minimum_height: 1080,
        orientation: OrientationFilter::Landscape,
        favorites_only: true,
        ..Default::default()
    };
    filter
        .folders
        .insert(std::path::PathBuf::from("C:/wallpapers"));
    let filter_before = filter.clone();
    let mut saved = CandidateState::default();
    saved.appearance.preset = "custom".into();
    saved.appearance.accent = "#123456".into();
    saved.slideshow_interval_seconds = 900;
    let saved_before = saved.clone();

    assert_eq!(panels.open_search(), PanelEffect::FocusSearchInput);

    assert!(panels.search_open);
    assert!(!panels.settings_open);
    assert_eq!(filter, filter_before);
    assert_eq!(saved, saved_before);
}

#[test]
fn settings_navigation_uses_only_populated_in_scope_categories() {
    let mut panels = PanelState::default();
    panels.open_search();
    panels.open_settings(SettingsCategory::Theme);

    assert!(!panels.search_open);
    assert!(panels.settings_open);
    assert_eq!(panels.settings_category, SettingsCategory::Theme);
    assert_eq!(
        SettingsCategory::VISIBLE.map(SettingsCategory::label),
        [
            "Picker",
            "Catalog filters",
            "Position",
            "Motion",
            "Playback",
            "Library",
            "Automation",
            "Theme",
        ]
    );
    assert!(SettingsCategory::VISIBLE
        .iter()
        .all(|category| !category.description().is_empty()));
}

#[test]
fn settings_search_is_separate_from_catalog_search_and_stays_in_scope() {
    assert!(settings_category_matches(
        SettingsCategory::Position,
        "radius"
    ));
    assert!(settings_category_matches(
        SettingsCategory::Playback,
        "engine"
    ));
    assert!(!settings_category_matches(
        SettingsCategory::Automation,
        "radius"
    ));
    assert!(settings_category_matches(
        SettingsCategory::CatalogFilters,
        "pictures"
    ));
    assert!(SettingsCategory::VISIBLE
        .iter()
        .all(|category| !category.label().is_empty()));
}

#[test]
fn settings_search_returns_supported_rows_without_inventing_categories() {
    let hits = settings_search_hits("orbit radius");
    assert!(hits.iter().any(|(category, label, _)| {
        *category == SettingsCategory::Position && *label == "Orbit radius"
    }));
    assert!(settings_search_hits("language").is_empty());
}

#[test]
fn escape_closes_search_before_detail_and_gallery_close_behavior() {
    let mut panels = PanelState::default();
    panels.open_search();

    assert_eq!(panels.escape(false), EscapeAction::CloseSearch);
    assert!(!panels.search_open);
    assert_eq!(panels.escape(true), EscapeAction::CloseDetail);
    assert_eq!(panels.escape(false), EscapeAction::HideGallery);
}

#[test]
fn escape_closes_settings_before_detail_and_gallery() {
    let mut panels = PanelState::default();
    panels.open_settings(SettingsCategory::CatalogFilters);

    assert_eq!(panels.escape(false), EscapeAction::CloseSettings);
    assert!(!panels.settings_open);
    assert!(!panels.search_open);
    assert_eq!(panels.escape(true), EscapeAction::CloseDetail);
    assert_eq!(panels.escape(false), EscapeAction::HideGallery);
}

#[test]
fn visible_search_close_action_closes_the_active_context() {
    let mut standalone = PanelState::default();
    standalone.open_search();
    standalone.close_search_panel();
    assert!(!standalone.search_open);
    assert!(!standalone.settings_open);

    let mut settings = PanelState::default();
    settings.open_settings(SettingsCategory::CatalogFilters);
    settings.close_search_panel();
    assert!(!settings.settings_open);
    assert!(!settings.search_open);
}

#[test]
fn captured_escape_and_ctrl_f_reach_panel_keyboard_handling() {
    let escape = keyboard::Key::Named(keyboard::key::Named::Escape);
    let f = keyboard::Key::Character("f".into());

    assert!(should_handle_key_event(
        event::Status::Captured,
        &escape,
        keyboard::Modifiers::NONE
    ));
    assert!(should_handle_key_event(
        event::Status::Captured,
        &f,
        keyboard::Modifiers::CTRL
    ));
}

#[test]
fn uniform_wall_variant_keeps_the_observed_geometry_at_1080p_scale() {
    let mut wall = candidate_state::WallSettings::default();
    configure_wall_variant_geometry(&mut wall, 0, 0.75);

    assert_eq!(wall.layout, StateWallLayout::Uniform);
    assert_eq!((wall.columns, wall.rows), (6, 3));
    assert_eq!((wall.card_width, wall.card_height), (300.0, 169.0));
    assert_eq!((wall.gap_x, wall.gap_y), (8.0, 8.0));
}

fn r6_distinct_gallery_geometry() -> candidate_state::GallerySettings {
    let mut gallery = candidate_state::GallerySettings::default();
    gallery.slices.width = 211.0;
    gallery.slices.expanded_width = 611.0;
    gallery.slices.height = 477.0;
    gallery.slices.spacing = -17.0;
    gallery.slices.skew_distance = 53.0;
    gallery.slices.visible_count = 9;
    gallery.geometric.rows = 6;
    gallery.geometric.columns = 11;
    gallery.geometric.radius = 88.0;
    gallery.geometric.lens = 0.41;
    gallery.geometric.orbit = -0.22;
    gallery.geometric.twist = 19.0;
    gallery.wall.columns = 9;
    gallery.wall.rows = 5;
    gallery.wall.card_width = 267.0;
    gallery.wall.card_height = 153.0;
    gallery.wall.scatter = 12.0;
    gallery
}

#[test]
fn r6_gallery_mode_navigation_preserves_each_modes_geometry() {
    let modes = [
        GalleryKind::Slices,
        GalleryKind::Geometric,
        GalleryKind::Wall,
    ];

    for from in modes {
        for to in modes {
            if from == to {
                continue;
            }
            let mut gallery = r6_distinct_gallery_geometry();
            gallery.mode = from;
            gallery.variant = Some(2);
            let expected = gallery.clone();

            assert!(navigate_gallery_mode(&mut gallery, from, to, |_| {}));
            assert_eq!(gallery.mode, to);
            assert_eq!(gallery.slices, expected.slices, "{from:?} -> {to:?}");
            assert_eq!(gallery.geometric, expected.geometric, "{from:?} -> {to:?}");
            assert_eq!(gallery.wall, expected.wall, "{from:?} -> {to:?}");
            assert_eq!(gallery.variant, None, "{from:?} -> {to:?}");
        }
    }

    let mut gallery = r6_distinct_gallery_geometry();
    gallery.mode = GalleryKind::Slices;
    let expected = gallery.clone();
    let mut current_mode = GalleryKind::Slices;
    let mut effects = Vec::new();
    for mode in [
        GalleryKind::Geometric,
        GalleryKind::Wall,
        GalleryKind::Slices,
    ] {
        assert!(navigate_gallery_mode(
            &mut gallery,
            current_mode,
            mode,
            |effect| { effects.push(effect) }
        ));
        current_mode = mode;
    }
    assert_eq!(gallery.slices, expected.slices);
    assert_eq!(gallery.geometric, expected.geometric);
    assert_eq!(gallery.wall, expected.wall);
    assert_eq!(gallery.variant, None);
    assert_eq!(
        effects,
        [
            GalleryNavigationEffect::InvalidatePosterWork,
            GalleryNavigationEffect::ResetWallScroll,
            GalleryNavigationEffect::PersistState,
            GalleryNavigationEffect::SchedulePosters,
            GalleryNavigationEffect::InvalidatePosterWork,
            GalleryNavigationEffect::ResetWallScroll,
            GalleryNavigationEffect::PersistState,
            GalleryNavigationEffect::SchedulePosters,
            GalleryNavigationEffect::InvalidatePosterWork,
            GalleryNavigationEffect::ResetWallScroll,
            GalleryNavigationEffect::PersistState,
            GalleryNavigationEffect::SchedulePosters,
        ]
    );
}

#[test]
fn r6_same_mode_keeps_variant_and_viewport_reapplication_respects_custom() {
    let mut gallery = r6_distinct_gallery_geometry();
    gallery.mode = GalleryKind::Geometric;
    gallery.variant = Some(3);
    let expected = gallery.clone();

    let mut effects = Vec::new();
    assert!(!navigate_gallery_mode(
        &mut gallery,
        GalleryKind::Geometric,
        GalleryKind::Geometric,
        |effect| effects.push(effect)
    ));
    assert_eq!(gallery, expected);
    assert_eq!(
        effects,
        [
            GalleryNavigationEffect::PersistState,
            GalleryNavigationEffect::SchedulePosters
        ]
    );
    assert_eq!(gallery_variant_for_viewport(&gallery), Some(3));

    gallery.variant = None;
    let expected = gallery.clone();
    for viewport in [[1920.0, 1080.0], [1280.0, 720.0], [1920.0, 1080.0]] {
        apply_gallery_viewport_policy(&mut gallery, GalleryKind::Geometric, viewport);
    }
    assert_eq!(gallery, expected);

    // Benchmark navigation changes only the active runtime mode; viewport
    // adaptation must keep using that mode without rewriting Slices.
    gallery.mode = GalleryKind::Slices;
    gallery.variant = Some(1);
    let slices = gallery.slices.clone();
    let geometric = gallery.geometric.clone();
    apply_gallery_viewport_policy(&mut gallery, GalleryKind::Geometric, [1600.0, 900.0]);
    assert_eq!(gallery.slices, slices);
    assert_ne!(gallery.geometric, geometric);
}

#[test]
fn d2_legacy_preset_actions_preserve_independent_depth_even_with_payload() {
    let mut gallery = candidate_state::GallerySettings::default();
    gallery.depth.width = 777.0;
    gallery.depth.detail.effect = "Pixel Sort".into();
    let depth = gallery.depth.clone();
    for mode in [
        GalleryKind::Slices,
        GalleryKind::Geometric,
        GalleryKind::Wall,
    ] {
        let preset = candidate_state::GalleryPreset {
            name: "legacy snapshot".into(),
            mode,
            variant: None,
            slices: candidate_state::SliceSettings::default(),
            geometric: candidate_state::GeometricSettings::default(),
            wall: candidate_state::WallSettings::default(),
            depth: Some(candidate_state::LegacyDepthSettings::default()),
        };
        apply_saved_gallery_preset(&mut gallery, &preset);
        assert_eq!(gallery.depth, depth);
        activate_gallery_variant(&mut gallery, mode, 0, [1920.0, 1080.0]);
        assert_eq!(gallery.depth, depth);
    }
}
#[test]
fn d2_supported_proposal_snapshots_no_provenance_and_acknowledges_only_success() {
    let root = std::env::temp_dir().join(format!(
        "d2-import-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("state.toml");
    let bak = path.with_extension("toml.bak");
    let raw = b"# exact import original\nversion=1\nfavorites=['old']";
    std::fs::write(&path, raw).unwrap();
    let mut current = CandidateState::default();
    let mut proposed = CandidateState::default();
    proposed.favorites.insert("import".into());
    let saved = commit_saved_state(&mut current, proposed, &path).unwrap();
    assert!(current.favorites.contains("import"));
    assert!(current.pending_migration.is_none());
    assert_eq!(std::fs::read(&saved.migration[0].snapshot).unwrap(), raw);
    for _ in 0..3 {
        persist_candidate_state(&mut current, &path).unwrap();
        assert!(current.pending_migration.is_none());
    }
    assert_eq!(std::fs::read(&saved.migration[0].snapshot).unwrap(), raw);
    // Block snapshot creation in a separate exact-owned fixture, retaining live values.
    let blocked = root.join("blocked.toml");
    std::fs::write(&blocked, raw).unwrap();
    let mut loaded = CandidateState::load(&blocked);
    let before = loaded.clone();
    let hash = format!("{:x}", <sha2::Sha256 as sha2::Digest>::digest(raw));
    let snap = blocked.with_file_name(format!(
        "blocked.toml.migration-v1-to-v{}-primary-{hash}.raw",
        candidate_state::CURRENT_STATE_VERSION
    ));
    std::fs::create_dir(&snap).unwrap();
    let error = commit_saved_state(&mut loaded, CandidateState::default(), &blocked).unwrap_err();
    assert_eq!(loaded, before);
    assert!(loaded.pending_migration.is_some());
    assert_eq!(std::fs::read(&blocked).unwrap(), raw);
    let message =
        state_save_failure_message("Could not save proposed candidate state", &error, &blocked);
    assert!(message.contains("were not fully saved"));
    let status = diagnostic_status(
        Some(&message),
        None,
        None,
        None,
        None,
        Some("browsing changed".into()),
    );
    assert!(status.unwrap().contains("were not fully saved"));
    assert!(bak.exists());
    std::fs::remove_dir_all(root).unwrap();
}
#[test]
fn r6_explicit_variant_and_saved_preset_apply_geometry_then_navigation_keeps_it() {
    let mut gallery = r6_distinct_gallery_geometry();
    gallery.mode = GalleryKind::Slices;
    let before_variant = gallery.slices.clone();
    assert_eq!(next_gallery_variant_index(None, 3), 0);
    assert_eq!(next_gallery_variant_index(Some(0), 3), 1);
    assert_eq!(next_gallery_variant_index(Some(2), 3), 0);
    let next = next_gallery_variant_index(Some(0), 3);
    activate_gallery_variant(&mut gallery, GalleryKind::Slices, next, [1920.0, 1080.0]);
    assert_eq!(gallery.variant, Some(next as u8));
    assert_ne!(gallery.slices, before_variant);
    let applied_variant_geometry = gallery.slices.clone();
    assert!(navigate_gallery_mode(
        &mut gallery,
        GalleryKind::Slices,
        GalleryKind::Geometric,
        |_| {}
    ));
    assert!(navigate_gallery_mode(
        &mut gallery,
        GalleryKind::Geometric,
        GalleryKind::Slices,
        |_| {}
    ));
    assert_eq!(gallery.slices, applied_variant_geometry);

    let mut preset = candidate_state::GalleryPreset {
        depth: None,
        name: "R6 test preset".into(),
        mode: GalleryKind::Wall,
        variant: Some(2),
        slices: gallery.slices.clone(),
        geometric: gallery.geometric.clone(),
        wall: gallery.wall.clone(),
    };
    preset.wall.columns = 13;
    preset.wall.rows = 7;
    preset.wall.card_width = 333.0;
    apply_saved_gallery_preset(&mut gallery, &preset);
    assert_eq!(gallery.mode, GalleryKind::Wall);
    assert_eq!(gallery.variant, Some(2));
    assert_eq!(gallery.wall, preset.wall);
    let applied_preset_geometry = gallery.wall.clone();
    assert!(navigate_gallery_mode(
        &mut gallery,
        GalleryKind::Wall,
        GalleryKind::Slices,
        |_| {}
    ));
    assert!(navigate_gallery_mode(
        &mut gallery,
        GalleryKind::Slices,
        GalleryKind::Wall,
        |_| {}
    ));
    assert_eq!(gallery.wall, applied_preset_geometry);
    assert_eq!(gallery.variant, None);
}

#[test]
fn r6_custom_geometry_survives_profile_save_reload_and_future_write_refusal() {
    let unique = format!(
        "wallpaper-switcher-r6-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    let directory = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join("state.toml");
    let mut gallery = r6_distinct_gallery_geometry();
    gallery.mode = GalleryKind::Slices;
    let mut current_mode = GalleryKind::Slices;
    for mode in [
        GalleryKind::Geometric,
        GalleryKind::Wall,
        GalleryKind::Slices,
    ] {
        assert!(navigate_gallery_mode(
            &mut gallery,
            current_mode,
            mode,
            |_| {}
        ));
        current_mode = mode;
    }
    let mut state = CandidateState::default();
    state.gallery = gallery.clone();
    state.favorites.insert("stable-wallpaper-id".into());
    state.playlists.push(Playlist {
        id: "stable-playlist-id".into(),
        name: "Preserved playlist".into(),
        order: PlaybackOrder::Sequential,
        membership: PlaylistMembership::Explicit {
            wallpaper_ids: vec!["stable-wallpaper-id".into()],
        },
    });
    state.active_playlist_id = Some("stable-playlist-id".into());
    state.gallery.presets.push(candidate_state::GalleryPreset {
        depth: None,
        name: "Preserved preset".into(),
        mode: GalleryKind::Geometric,
        variant: Some(3),
        slices: gallery.slices.clone(),
        geometric: gallery.geometric.clone(),
        wall: gallery.wall.clone(),
    });
    state.save(&path).unwrap();
    state.save(&path).unwrap();
    let loaded = CandidateState::load(&path);
    assert_eq!(loaded.gallery.slices, gallery.slices);
    assert_eq!(loaded.gallery.geometric, gallery.geometric);
    assert_eq!(loaded.gallery.wall, gallery.wall);
    assert_eq!(loaded.gallery.variant, None);
    assert_eq!(loaded.gallery.presets[0].name, "Preserved preset");
    assert!(loaded.favorites.contains("stable-wallpaper-id"));
    assert_eq!(
        loaded.active_playlist_id.as_deref(),
        Some("stable-playlist-id")
    );

    let primary_bytes = std::fs::read(&path).unwrap();
    let backup_path = path.with_extension("toml.bak");
    let backup_bytes = std::fs::read(&backup_path).unwrap();
    let current_header = format!("version = {}", candidate_state::CURRENT_STATE_VERSION);
    let future_header = format!("version = {}", candidate_state::CURRENT_STATE_VERSION + 1);
    let future = primary_bytes
        .windows(current_header.len())
        .position(|window| window == current_header.as_bytes())
        .map(|index| {
            let mut bytes = primary_bytes.clone();
            bytes.splice(index..index + current_header.len(), future_header.bytes());
            bytes
        })
        .expect("serialized state has a version field");
    std::fs::write(&path, &future).unwrap();
    let mut protected = CandidateState::load_with_diagnostics(&path).state;
    assert!(navigate_gallery_mode(
        &mut protected.gallery,
        GalleryKind::Slices,
        GalleryKind::Geometric,
        |_| {}
    ));
    let refusal = protected.save(&path).unwrap_err();
    assert!(refusal
        .to_string()
        .contains("Unsupported candidate-state version"));
    assert_eq!(std::fs::read(&path).unwrap(), future);
    assert_eq!(std::fs::read(&backup_path).unwrap(), backup_bytes);
    std::fs::remove_dir_all(&directory).unwrap();
}

#[test]
fn rail_chip_shape_tracks_the_active_gallery_mode() {
    let colors = [0.2, 0.2, 0.2];
    let slices = rail_chip_style(
        GalleryMode::Slices,
        button::Status::Active,
        false,
        colors,
        colors,
        colors,
    );
    let geometric = rail_chip_style(
        GalleryMode::Geometric,
        button::Status::Active,
        false,
        colors,
        colors,
        colors,
    );
    let wall = rail_chip_style(
        GalleryMode::Wall,
        button::Status::Active,
        false,
        colors,
        colors,
        colors,
    );

    assert_ne!(slices.border.radius, geometric.border.radius);
    assert_ne!(geometric.border.radius, wall.border.radius);
}

#[test]
fn geometric_controls_expose_the_observed_curve_and_tile_choices() {
    assert_eq!(
        geometric_curve_choices().map(|(_, label)| label),
        ["Plane", "Bow", "Ribbon", "S sweep", "Cylinder"]
    );
    assert_eq!(
        geometric_shape_choices().map(|(_, label)| label),
        ["Triangle", "Hexagon", "Diamond", "Rhombus"]
    );
}

#[test]
fn gallery_numeric_controls_clamp_supported_values_and_mark_custom_geometry() {
    let mut state = CandidateState::default();
    assert!(set_gallery_number_value(
        &mut state,
        GalleryNumberField::SliceWidth,
        9000.0,
    ));
    assert_eq!(state.gallery.slices.width, 4096.0);
    assert_eq!(state.gallery.variant, None);

    state.gallery.mode = GalleryKind::Geometric;
    assert!(set_gallery_number_value(
        &mut state,
        GalleryNumberField::GeometricRows,
        4.6,
    ));
    assert_eq!(state.gallery.geometric.rows, 5);
    assert!(!set_gallery_number_value(
        &mut state,
        GalleryNumberField::WallRows,
        5.0,
    ));
    assert!(!set_gallery_number_value(
        &mut state,
        GalleryNumberField::GeometricRadius,
        f32::NAN,
    ));
}

#[test]
fn gallery_numeric_labels_and_wall_layout_choices_are_supported_values() {
    assert_eq!(
        GalleryNumberField::WallCylinderRadius.label(),
        "Cylinder radius"
    );
    assert_eq!(
        GalleryNumberField::GeometricOrbitRadius.label(),
        "Orbit radius"
    );
    assert_eq!(
        GalleryNumberField::WallCornerRadius.label(),
        "Corner radius"
    );
    assert_eq!(
        wall_layout_choices().map(|(_, label)| label),
        [
            "Uniform",
            "Brick",
            "Masonry",
            "Justified",
            "Editorial",
            "Cylinder"
        ]
    );
}

#[test]
fn geometric_deformation_controls_round_trip_through_the_candidate_state() {
    let mut state = CandidateState::default();
    state.gallery.mode = GalleryKind::Geometric;

    for (field, value) in [
        (GalleryNumberField::GeometricLens, 0.45),
        (GalleryNumberField::GeometricOrbit, -0.35),
        (GalleryNumberField::GeometricOrbitRadius, 780.0),
        (GalleryNumberField::GeometricTwist, 28.0),
        (GalleryNumberField::GeometricScatter, 14.0),
    ] {
        assert!(set_gallery_number_value(&mut state, field, value));
        assert!((gallery_number_value(&state, field) - value).abs() < f32::EPSILON);
    }
    assert_eq!(state.gallery.variant, None);
}

#[test]
fn wall_corner_radius_is_editable_without_crossing_gallery_modes() {
    let mut state = CandidateState::default();
    state.gallery.mode = GalleryKind::Wall;
    assert!(set_gallery_number_value(
        &mut state,
        GalleryNumberField::WallCornerRadius,
        44.0,
    ));
    assert_eq!(state.gallery.wall.corner_radius, 44.0);
    assert!(!set_gallery_number_value(
        &mut state,
        GalleryNumberField::WallCornerRadius,
        f32::NAN,
    ));
}

#[test]
fn choosing_a_geometric_curve_preserves_other_settings_and_marks_custom_only_on_change() {
    let mut gallery = candidate_state::GallerySettings::default();
    gallery.geometric.rows = 4;

    assert!(!choose_geometric_curve(
        &mut gallery,
        StateGeometricCurve::Arc
    ));
    assert_eq!(gallery.variant, Some(0));

    assert!(choose_geometric_curve(
        &mut gallery,
        StateGeometricCurve::Wave
    ));
    assert_eq!(gallery.variant, None);
    assert_eq!(gallery.geometric.curve, StateGeometricCurve::Wave);
    assert_eq!(gallery.geometric.shape, StateGeometricShape::Hexagon);
    assert_eq!(gallery.geometric.rows, 4);
}

#[test]
fn choosing_a_geometric_shape_preserves_curve_and_layout_settings() {
    let mut gallery = candidate_state::GallerySettings::default();
    gallery.geometric.curve = StateGeometricCurve::Wave;
    gallery.geometric.columns = 9;

    assert!(choose_geometric_shape(
        &mut gallery,
        StateGeometricShape::Diamond
    ));

    assert_eq!(gallery.variant, None);
    assert_eq!(gallery.geometric.shape, StateGeometricShape::Diamond);
    assert_eq!(gallery.geometric.curve, StateGeometricCurve::Wave);
    assert_eq!(gallery.geometric.columns, 9);
}

#[test]
fn gallery_navigation_selects_then_opens_detail_without_applying() {
    assert_eq!(card_click_action(Some(2), 5), CardClickAction::Select);
    assert_eq!(card_click_action(Some(5), 5), CardClickAction::OpenDetail);
}

#[test]
fn explicit_apply_plan_preserves_filtered_identity_and_active_monitor() {
    let wallpapers = vec![
        test_wallpaper("first"),
        test_wallpaper("second"),
        test_wallpaper("third"),
    ];
    let monitor_ids = vec!["DISPLAY\\MONITOR-1".to_string()];
    let plan = plan_apply_request(
        &wallpapers,
        &[2, 0],
        Some(2),
        Some("DISPLAY\\MONITOR-1"),
        &monitor_ids,
        false,
    )
    .expect("selected filtered item should produce an Apply plan");

    assert_eq!(plan.wallpaper.id, "third");
    assert_eq!(
        plan.target,
        ApplyTarget::DeviceId("DISPLAY\\MONITOR-1".into())
    );
}

// Exercise the actual mutation callers. Only topology observation and the
// final coordinator submission are substituted; no native/provider work.
fn mutation_target_fixture(engine: bool) -> GpuCandidate {
    let (mut app, _) = d3_application_fixture(1);
    app.apply_enabled = true;
    app.active_monitor_id = Some("DISPLAY-A".into());
    app.monitor_ids = vec!["DISPLAY-A".into(), "DISPLAY-B".into()];
    if engine {
        let project = app.state_path.with_file_name("task-a-engine-project.json");
        std::fs::write(
            &project,
            r#"{"type":"scene","general":{"properties":{"enabled":{"type":"bool","value":true}}}}"#,
        )
        .unwrap();
        let item = &mut app.wallpapers[0];
        item.source = WallpaperSource::WallpaperEngine;
        item.project_json_path = project;
        app.engine_properties_project = Some((item.id.clone(), item.project_json_path.clone()));
        let schema = engine_properties::read_supported(&item.project_json_path);
        (app.engine_properties, app.engine_property_values) = engine_property_defaults(&schema);
        app.engine_properties_open = true;
    }
    app
}

fn mutation_target_topology(ids: &[&str]) -> Result<Vec<native_wallpaper::NativeMonitor>, String> {
    Ok(ids
        .iter()
        .enumerate()
        .map(|(index, id)| native_wallpaper::NativeMonitor {
            device_id: (*id).into(),
            rect: monitor::DesktopRect {
                left: index as i32 * 1920,
                top: 0,
                right: (index as i32 + 1) * 1920,
                bottom: 1080,
            },
        })
        .collect())
}

fn mutation_target_wallpaper_requests(
    app: &mut GpuCandidate,
    all: bool,
    topology: Result<Vec<native_wallpaper::NativeMonitor>, String>,
) -> Vec<(String, ApplyTarget)> {
    let mut requests = Vec::new();
    app.apply_selected_with(
        all,
        || topology,
        |_, item, target, _| {
            requests.push((item.id, target));
            Err("Controlled submission boundary; no owner dispatch.".into())
        },
    );
    requests
}

fn mutation_target_property_requests(
    app: &mut GpuCandidate,
    topology: Result<Vec<native_wallpaper::NativeMonitor>, String>,
) -> Vec<EngineCommand> {
    let mut requests = Vec::new();
    app.apply_engine_properties_with(
        || topology,
        |_, command| {
            requests.push(command);
            Err("Controlled submission boundary; no owner dispatch.".into())
        },
    );
    requests
}

#[test]
fn mutation_target_wallpaper_missing_a_rejects_without_dispatch_to_b() {
    let mut app = mutation_target_fixture(false);
    let requests = mutation_target_wallpaper_requests(
        &mut app,
        false,
        mutation_target_topology(&["DISPLAY-B"]),
    );
    assert!(
        requests.is_empty(),
        "missing A must never submit to surviving B: {requests:?}"
    );
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-A"));
    let status = app.status.as_deref().unwrap();
    assert!(
        status.contains("DISPLAY-A") && status.contains("unavailable") && status.contains("select"),
        "{status}"
    );
    assert!(!app.apply_pending && app.active_operation.is_none());
    assert_eq!(app.state.manual_holdoff_until_unix, 0);
    assert!(!app.state_path.exists());
}

#[test]
fn mutation_target_properties_same_project_on_b_still_reject_missing_a() {
    let mut app = mutation_target_fixture(true);
    // Model B with the same real fixture project path. This is a controlled
    // same-project identity oracle, not a backend/provider dispatch claim.
    let project_on_b = app.wallpapers[0].project_json_path.clone();
    assert!(project_on_b.is_file() && !project_on_b.as_os_str().is_empty());
    let mut enumerations = 0;
    let mut requests = Vec::new();
    app.apply_engine_properties_with(
        || {
            enumerations += 1;
            mutation_target_topology(&["DISPLAY-B"])
        },
        |_, command| {
            if let EngineCommand::ApplyProperties {
                project_json_path,
                target: ApplyTarget::DeviceId(id),
                ..
            } = &command
            {
                assert_eq!(id, "DISPLAY-B");
                assert_eq!(project_json_path, &project_on_b);
            } else {
                panic!("unexpected command: {command:?}");
            }
            requests.push(command);
            Err("Controlled matching-project oracle; no owner dispatch.".into())
        },
    );
    assert_eq!(
        enumerations, 1,
        "project guard must pass before fresh topology is checked"
    );
    assert!(
        requests.is_empty(),
        "same-project B must not receive A's request: {requests:?}"
    );
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-A"));
    let status = app.status.as_deref().unwrap();
    assert!(
        status.contains("DISPLAY-A") && status.contains("unavailable"),
        "{status}"
    );
    assert!(!app.apply_pending && app.active_operation.is_none());
    assert!(!app.state_path.exists());
}

#[test]
fn mutation_target_wallpaper_valid_case_varied_reordered_a_preserves_item_and_target() {
    let mut app = mutation_target_fixture(false);
    let requests = mutation_target_wallpaper_requests(
        &mut app,
        false,
        mutation_target_topology(&["DISPLAY-B", "display-a"]),
    );
    assert_eq!(
        requests,
        vec![(
            app.wallpapers[0].id.clone(),
            ApplyTarget::DeviceId("DISPLAY-A".into())
        )]
    );
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-A"));
}

#[test]
fn mutation_target_properties_valid_case_varied_reordered_a_preserves_project_and_values() {
    let mut app = mutation_target_fixture(true);
    let requests = mutation_target_property_requests(
        &mut app,
        mutation_target_topology(&["DISPLAY-B", "display-a"]),
    );
    assert_eq!(requests.len(), 1);
    let EngineCommand::ApplyProperties {
        wallpaper_id,
        project_json_path,
        properties,
        target,
    } = &requests[0]
    else {
        panic!("expected properties");
    };
    assert_eq!(wallpaper_id, &app.wallpapers[0].id);
    assert_eq!(project_json_path, &app.wallpapers[0].project_json_path);
    assert_eq!(properties, &app.engine_property_values);
    assert_eq!(target, &ApplyTarget::DeviceId("DISPLAY-A".into()));
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-A"));
}

#[test]
fn mutation_target_wallpaper_empty_topology_confirms_a_unavailable() {
    let mut app = mutation_target_fixture(false);
    assert!(
        mutation_target_wallpaper_requests(&mut app, false, mutation_target_topology(&[]))
            .is_empty()
    );
    let status = app.status.as_deref().unwrap();
    assert!(
        status.contains("DISPLAY-A") && status.contains("unavailable"),
        "{status}"
    );
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-A"));
}

#[test]
fn mutation_target_properties_empty_topology_confirms_a_unavailable() {
    let mut app = mutation_target_fixture(true);
    assert!(mutation_target_property_requests(&mut app, mutation_target_topology(&[])).is_empty());
    let status = app.status.as_deref().unwrap();
    assert!(
        status.contains("DISPLAY-A") && status.contains("unavailable"),
        "{status}"
    );
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-A"));
}

#[test]
fn mutation_target_wallpaper_enumeration_failure_is_not_confirmed_absence() {
    let mut app = mutation_target_fixture(false);
    assert!(mutation_target_wallpaper_requests(
        &mut app,
        false,
        Err("synthetic-enumeration-error".into())
    )
    .is_empty());
    let status = app.status.as_deref().unwrap();
    assert!(
        status.contains("enumerate")
            && status.contains("synthetic-enumeration-error")
            && !status.contains("unavailable"),
        "{status}"
    );
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-A"));
    assert_eq!(app.monitor_ids, ["DISPLAY-A", "DISPLAY-B"]);
}

#[test]
fn mutation_target_properties_enumeration_failure_is_not_confirmed_absence() {
    let mut app = mutation_target_fixture(true);
    assert!(
        mutation_target_property_requests(&mut app, Err("synthetic-enumeration-error".into()))
            .is_empty()
    );
    let status = app.status.as_deref().unwrap();
    assert!(
        status.contains("enumerate")
            && status.contains("synthetic-enumeration-error")
            && !status.contains("unavailable"),
        "{status}"
    );
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-A"));
    assert_eq!(app.monitor_ids, ["DISPLAY-A", "DISPLAY-B"]);
}

#[test]
fn mutation_target_explicit_all_allows_b_even_when_selected_a_disappears() {
    let mut app = mutation_target_fixture(false);
    let requests = mutation_target_wallpaper_requests(
        &mut app,
        true,
        mutation_target_topology(&["DISPLAY-B"]),
    );
    assert_eq!(
        requests,
        vec![(
            app.wallpapers[0].id.clone(),
            ApplyTarget::DeviceIds(vec!["DISPLAY-B".into()])
        )]
    );
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-A"));
    app.active_monitor_id = None;
    assert_eq!(
        mutation_target_wallpaper_requests(
            &mut app,
            true,
            mutation_target_topology(&["DISPLAY-B"])
        )[0]
        .1,
        ApplyTarget::DeviceIds(vec!["DISPLAY-B".into()])
    );
}

#[test]
fn mutation_target_explicit_all_empty_topology_never_degrades_to_single_target() {
    let mut app = mutation_target_fixture(false);
    assert!(
        mutation_target_wallpaper_requests(&mut app, true, mutation_target_topology(&[]))
            .is_empty()
    );
    assert!(app
        .status
        .as_deref()
        .unwrap()
        .contains("No Windows wallpaper monitor"));
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-A"));
}

#[test]
fn mutation_target_explicit_all_enumeration_failure_rejects_without_dispatch() {
    let mut app = mutation_target_fixture(false);
    assert!(mutation_target_wallpaper_requests(
        &mut app,
        true,
        Err("synthetic-enumeration-error".into())
    )
    .is_empty());
    assert!(app
        .status
        .as_deref()
        .unwrap()
        .contains("synthetic-enumeration-error"));
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-A"));
}

#[test]
fn mutation_target_no_selected_display_requires_explicit_selection() {
    for engine in [false, true] {
        let mut app = mutation_target_fixture(engine);
        app.active_monitor_id = None;
        if engine {
            assert!(mutation_target_property_requests(
                &mut app,
                mutation_target_topology(&["DISPLAY-B"])
            )
            .is_empty());
        } else {
            assert!(mutation_target_wallpaper_requests(
                &mut app,
                false,
                mutation_target_topology(&["DISPLAY-B"])
            )
            .is_empty());
        }
        assert!(app.status.as_deref().unwrap().contains("select"));
        assert!(app.active_monitor_id.is_none());
    }
}

#[test]
fn mutation_target_browsing_refresh_retains_reopen_and_display_cycle_fallback_policy() {
    let mut app = mutation_target_fixture(false);
    app.refresh_monitors_with(
        false,
        || mutation_target_topology(&["DISPLAY-B"]),
        |_| panic!("cursor not requested"),
    );
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-B"));
    app.active_monitor_id = Some("DISPLAY-A".into());
    app.refresh_monitors_with(
        false,
        || mutation_target_topology(&["DISPLAY-B", "display-a"]),
        |_| None,
    );
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-A"));
    app.refresh_monitors_with(
        true,
        || mutation_target_topology(&["DISPLAY-B", "DISPLAY-A"]),
        |_| Some("DISPLAY-B".into()),
    );
    assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-B"));
    app.refresh_monitors_with(false, || mutation_target_topology(&[]), |_| None);
    assert!(app.active_monitor_id.is_none() && app.monitor_ids.is_empty());
}

#[test]
fn mutation_target_pending_unknown_and_disabled_guards_still_prevent_submission() {
    for engine in [false, true] {
        for guard in ["pending", "unknown", "disabled"] {
            let mut app = mutation_target_fixture(engine);
            app.apply_pending = guard == "pending";
            app.provider_state_unknown = guard == "unknown";
            app.apply_enabled = guard != "disabled";
            if engine {
                assert!(
                    mutation_target_property_requests(
                        &mut app,
                        mutation_target_topology(&["DISPLAY-A"])
                    )
                    .is_empty(),
                    "{guard}"
                );
            } else {
                assert!(
                    mutation_target_wallpaper_requests(
                        &mut app,
                        false,
                        mutation_target_topology(&["DISPLAY-A"])
                    )
                    .is_empty(),
                    "{guard}"
                );
            }
            assert_eq!(app.active_monitor_id.as_deref(), Some("DISPLAY-A"));
        }
    }
}

#[test]
fn mutation_target_changed_engine_project_rejects_before_enumeration_or_submission() {
    let mut app = mutation_target_fixture(true);
    app.engine_properties_project.as_mut().unwrap().0 = "different-project".into();
    app.apply_engine_properties_with(
        || panic!("project guard must precede enumeration"),
        |_, _| panic!("project guard must prevent submission"),
    );
    assert!(app.status.as_deref().unwrap().contains("project changed"));
}

#[test]
fn mutation_target_wallpaper_hidden_selection_rejects_before_submission() {
    let mut app = mutation_target_fixture(false);
    app.filtered_indices.clear();
    assert!(mutation_target_wallpaper_requests(
        &mut app,
        false,
        mutation_target_topology(&["DISPLAY-A"])
    )
    .is_empty());
    assert!(!app.apply_pending && app.active_operation.is_none());
}

#[test]
fn explicit_all_monitor_apply_plan_requires_a_real_monitor_set() {
    let wallpapers = vec![test_wallpaper("first")];
    let monitor_ids = vec!["DISPLAY\\MONITOR-1".to_string()];
    let plan = plan_apply_request(
        &wallpapers,
        &[0],
        Some(0),
        Some("DISPLAY\\MONITOR-1"),
        &monitor_ids,
        true,
    )
    .expect("all-monitor Apply should use the verified monitor set");
    assert_eq!(plan.wallpaper.id, "first");
    assert_eq!(plan.target, ApplyTarget::All);

    assert!(matches!(
        plan_apply_request(&wallpapers, &[0], Some(0), None, &[], true),
        Err(ApplyPlanError::NoMonitor)
    ));
}

#[test]
fn tray_open_always_requests_visible_gallery() {
    assert!(visibility_after_tray_open(false));
    assert!(visibility_after_tray_open(true));
}

#[test]
fn candidate_apply_is_opt_in() {
    let arguments = ["gpu-candidate.exe", "--show"];

    assert!(!arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case("--allow-apply")));
}

#[test]
fn candidate_apply_trace_is_opt_in() {
    let default_arguments = ["gpu-candidate.exe", "--show"];
    let trace_arguments = ["gpu-candidate.exe", "--show", "--apply-trace"];

    assert!(!default_arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case(apply_trace::FLAG)));
    assert!(trace_arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case(apply_trace::FLAG)));
}

#[test]
fn shape_chip_follows_the_observed_orientation_cycle() {
    assert_eq!(shape_chip_label(OrientationFilter::Any), "SHAPE");
    assert_eq!(shape_chip_label(OrientationFilter::Landscape), "WIDE");
    assert_eq!(shape_chip_label(OrientationFilter::Portrait), "TALL");
    assert_eq!(shape_chip_label(OrientationFilter::Square), "SQUARE");
    assert_eq!(
        next_shape_filter(OrientationFilter::Any),
        OrientationFilter::Landscape
    );
    assert_eq!(
        next_shape_filter(OrientationFilter::Landscape),
        OrientationFilter::Portrait
    );
    assert_eq!(
        next_shape_filter(OrientationFilter::Portrait),
        OrientationFilter::Square
    );
    assert_eq!(
        next_shape_filter(OrientationFilter::Square),
        OrientationFilter::Any
    );
}

#[test]
fn catalog_presentation_distinguishes_loading_empty_filters_and_poster_failures() {
    assert_eq!(
        catalog_presentation(false, 0, 0, false),
        CatalogPresentation::Loading
    );
    assert_eq!(
        catalog_presentation(true, 0, 0, false),
        CatalogPresentation::EmptyCatalog
    );
    assert_eq!(
        catalog_presentation(true, 3, 0, false),
        CatalogPresentation::EmptyFilter
    );
    assert_eq!(
        catalog_presentation(true, 3, 2, true),
        CatalogPresentation::PosterFailure
    );
    assert_eq!(
        catalog_presentation(true, 3, 2, false),
        CatalogPresentation::Ready
    );
}

#[test]
fn catalog_presentation_labels_are_honest_about_empty_states() {
    assert_eq!(CatalogPresentation::Loading.title(), "Loading wallpapers…");
    assert_eq!(
        CatalogPresentation::EmptyCatalog.title(),
        "No wallpapers found"
    );
    assert_eq!(
        CatalogPresentation::EmptyFilter.title(),
        "No wallpapers match this filter"
    );
    assert_eq!(
        CatalogPresentation::PosterFailure.title(),
        "Wallpapers found; previews unavailable"
    );
}

#[test]
fn pictures_filter_keeps_local_images_and_excludes_engine_items() {
    assert!(SourceFilter::Pictures.accepts(WallpaperSource::LocalImage));
    assert!(!SourceFilter::Pictures.accepts(WallpaperSource::LocalVideo));
    assert!(!SourceFilter::Pictures.accepts(WallpaperSource::WallpaperEngine));
    assert!(SourceFilter::WallpaperEngine.accepts(WallpaperSource::WallpaperEngine));
    assert!(!SourceFilter::WallpaperEngine.accepts(WallpaperSource::LocalImage));
}

#[test]
fn screenshot_source_filter_is_pictures_only_and_diagnostic_scoped() {
    assert_eq!(
        initial_screenshot_source_filter(true, Some("pictures")),
        SourceFilter::Pictures
    );
    assert_eq!(
        initial_screenshot_source_filter(true, Some("pic")),
        SourceFilter::Pictures
    );
    assert_eq!(
        initial_screenshot_source_filter(true, Some("unknown")),
        SourceFilter::All
    );
    assert_eq!(
        initial_screenshot_source_filter(false, Some("pictures")),
        SourceFilter::All
    );
}

#[test]
fn configured_keyboard_bindings_require_exact_modifiers() {
    let enter = keyboard::Key::Named(keyboard::key::Named::Enter);
    let left = keyboard::Key::Named(keyboard::key::Named::ArrowLeft);

    assert!(matches_candidate_key(
        &enter,
        keyboard::Modifiers::CTRL,
        "ctrl+return"
    ));
    assert!(!matches_candidate_key(
        &enter,
        keyboard::Modifiers::NONE,
        "ctrl+return"
    ));
    assert!(matches_candidate_key(
        &left,
        keyboard::Modifiers::NONE,
        "left"
    ));
    assert!(!matches_candidate_key(
        &left,
        keyboard::Modifiers::SHIFT,
        "left"
    ));
}

#[test]
fn configured_right_binding_classifies_as_selection_navigation() {
    let config = Config::default();
    let right = keyboard::Key::Named(keyboard::key::Named::ArrowRight);

    assert_eq!(
        matched_selection_navigation(&right, keyboard::Modifiers::NONE, &config.keybinds),
        Some(SelectionNavigation::Relative(1))
    );
}

#[test]
fn pointer_targeting_uses_desktop_coordinates_including_negative_origins() {
    let monitors = vec![
        native_wallpaper::NativeMonitor {
            device_id: "left".into(),
            rect: monitor::DesktopRect {
                left: -1920,
                top: 0,
                right: 0,
                bottom: 1080,
            },
        },
        native_wallpaper::NativeMonitor {
            device_id: "primary".into(),
            rect: monitor::DesktopRect {
                left: 0,
                top: 0,
                right: 2560,
                bottom: 1440,
            },
        },
    ];

    assert_eq!(
        monitor_id_at_point(&monitors, -1, 500).as_deref(),
        Some("left")
    );
    assert_eq!(
        monitor_id_at_point(&monitors, 0, 500).as_deref(),
        Some("primary")
    );
    assert_eq!(monitor_id_at_point(&monitors, 3000, 500), None);
}

#[test]
fn poster_queue_prioritizes_visible_neighbors_and_is_bounded() {
    let filtered: Vec<_> = (0..1_000).collect();
    let indices = prioritized_poster_indices(&filtered, 500);

    assert_eq!(&indices[..5], &[500, 499, 501, 498, 502]);
    assert_eq!(indices.len(), gpu_gallery::MAX_POSTERS);
    assert_eq!(
        indices.iter().copied().collect::<HashSet<_>>().len(),
        indices.len()
    );
}

#[test]
fn poster_queue_uses_filtered_catalog_order() {
    let filtered = vec![9, 4, 21, 2, 17];

    assert_eq!(prioritized_poster_indices(&filtered, 21), [21, 4, 2, 9, 17]);
}

fn wall_test_layout() -> GalleryLayoutParams {
    let mut params = GalleryLayoutParams::default();
    params.wall = WallParams {
        columns: 4,
        rows: 3,
        card_width: 80.0,
        card_height: 60.0,
        gap_x: 4.0,
        gap_y: 4.0,
        selected_scale: 1.0,
        ..WallParams::default()
    };
    params
}

#[test]
fn wall_endpoint_production_rail_frame_and_poster_demand_stay_coherent() {
    let (mut app, _) = d3_application_fixture(24);
    app.gallery_mode = GalleryMode::Wall;
    app.state.gallery.mode = GalleryKind::Wall;
    app.state.gallery.wall.columns = 4;
    app.state.gallery.wall.rows = 3;
    app.state.gallery.wall.card_width = 90.0;
    app.state.gallery.wall.card_height = 60.0;
    app.state.gallery.wall.gap_x = 7.0;
    app.state.gallery.wall.gap_y = 10.0;
    app.viewport = [900.0, 700.0];
    app.config.ui.show_top_bar = true;
    for width in [900.0, 400.0] {
        app.viewport[0] = width;
        let params = gallery_layout_params_for_app(&app);
        let rail_size = donor_rail::footprint(&gallery_rail_model(&app));
        assert_eq!(params.wall.rail_height, Some(rail_size[1]));
        let frame = gpu_gallery::wall_frame(app.viewport, params.wall);
        let at = donor_rail::anchor(donor_rail::RailMode::Wall, app.viewport, rail_size, 200.0);
        assert_eq!(frame.top - at[1] - rail_size[1], 8.0);
    }
    app.config.ui.show_top_bar = false;
    let p = gallery_layout_params_for_app(&app);
    assert_eq!(p.wall.rail_height, None);
    assert_eq!(
        gpu_gallery::wall_frame(app.viewport, p.wall).center[1],
        350.0
    );
}

#[test]
fn wall_endpoint_all_six_layouts_request_visible_stable_items_and_hit_selected_body() {
    let filtered: Vec<_> = (0..24).map(|i| 100 - i).collect();
    let aspects: Vec<_> = (0..24).map(|i| [1.5, 0.75, 2.0][i % 3]).collect();
    for layout in [
        WallLayout::Uniform,
        WallLayout::Brick,
        WallLayout::Masonry,
        WallLayout::Justified,
        WallLayout::Editorial,
        WallLayout::Cylinder,
    ] {
        for selected in [0, 9, 23] {
            let mut p = wall_test_layout();
            p.wall.layout = layout;
            let (grid, placements) = gpu_gallery::wall_placements(24, p.wall, &aspects);
            let c = placements.iter().find(|c| c.index == selected).unwrap();
            p.wall.scroll_offset = (c.y + c.height * 0.5 - grid.total_h() * 0.5).max(0.0);
            let viewport = [600.0, 400.0];
            let cards = gpu_gallery::layout_gallery_with_aspects(
                GalleryMode::Wall,
                24,
                selected,
                viewport,
                p,
                &aspects,
            );
            let requests = wall_visible_poster_indices(&filtered, selected, viewport, p, &aspects);
            let visible: Vec<_> = cards.iter().map(|c| filtered[c.index]).collect();
            assert!(visible.len() <= gpu_gallery::MAX_POSTERS);
            assert_eq!(
                &requests[..visible.len()],
                visible.as_slice(),
                "{layout:?}, selected{selected}"
            );
            let c = cards.iter().find(|c| c.selected).unwrap();
            assert_eq!(
                gpu_gallery::hit_test(&cards, c.center[0], c.center[1]),
                Some(selected),
                "{layout:?}"
            );
            assert!(requests.contains(&filtered[selected]));
        }
    }
}

#[test]
fn wall_poster_queue_tracks_a_distant_visible_region_without_selection_change() {
    let filtered: Vec<_> = (0..100).collect();
    let mut params = wall_test_layout();
    let top = poster_indices_for_view(
        GalleryMode::Wall,
        &filtered,
        Some(0),
        [420.0, 240.0],
        params,
        &[],
    );
    params.wall.scroll_offset = 1_000.0;
    let distant = poster_indices_for_view(
        GalleryMode::Wall,
        &filtered,
        Some(0),
        [420.0, 240.0],
        params,
        &[],
    );

    assert_ne!(top, distant);
    assert!(distant.iter().any(|index| *index >= 40));
    let (mut app, _) = d4a_fixture(100);
    app.gallery_mode = GalleryMode::Wall;
    app.selected = Some(0);
    app.browse_position_restored = true;
    app.sync_wall_presentation();
    let _ = update(
        &mut app,
        Message::Gallery(GalleryMessage::Wheel(-1.0, None)),
    );
    assert_eq!(app.selected, Some(0), "scrolling must not move selection");
    assert!(app.wall_presentation.as_ref().unwrap().camera.target > 0.0);
}

#[test]
fn wall_poster_queue_includes_every_visible_card_when_demand_fits() {
    let filtered: Vec<_> = (0..80).collect();
    let mut params = wall_test_layout();
    params.wall.columns = 4;
    params.wall.rows = 2;
    params.wall.scroll_offset = 500.0;
    let visible_cards = gpu_gallery::layout_gallery_with_aspects(
        GalleryMode::Wall,
        filtered.len(),
        0,
        [420.0, 240.0],
        params,
        &[],
    );
    let requested = wall_visible_poster_indices(&filtered, 0, [420.0, 240.0], params, &[]);
    let visible: Vec<_> = visible_cards
        .iter()
        .map(|card| filtered[card.index])
        .collect();

    assert!(visible.len() <= gpu_gallery::MAX_POSTERS);
    assert!(!visible.is_empty());
    assert!(visible.iter().all(|index| requested.contains(index)));
    assert!(
        requested.contains(&0),
        "selection uses remaining capacity when available"
    );
}

#[test]
fn wall_poster_queue_has_deterministic_capacity_policy_for_dense_views() {
    let filtered: Vec<_> = (0..64).collect();
    let mut params = wall_test_layout();
    params.wall.columns = 8;
    params.wall.rows = 8;
    params.wall.card_width = 40.0;
    params.wall.card_height = 40.0;
    let requested = wall_visible_poster_indices(&filtered, 63, [420.0, 420.0], params, &[]);
    let repeated = wall_visible_poster_indices(&filtered, 63, [420.0, 420.0], params, &[]);

    assert_eq!(requested, repeated);
    assert_eq!(
        requested.len(),
        64,
        "approved Wall bounds retain every visible image"
    );
    assert!((0..64).all(|i| requested.contains(&i)));
    assert_eq!(
        requested.iter().copied().collect::<HashSet<_>>().len(),
        requested.len()
    );
}

#[test]
fn wall_poster_queue_maps_filtered_order_to_stable_catalog_indices() {
    let filtered = vec![90, 40, 21, 2, 17, 63];
    let requested = poster_indices_for_view(
        GalleryMode::Wall,
        &filtered,
        Some(21),
        [420.0, 240.0],
        wall_test_layout(),
        &[],
    );

    assert!(requested.iter().all(|index| filtered.contains(index)));
    assert!(requested.contains(&21));
    assert!(!requested.contains(&0));
}

#[test]
fn wall_poster_queue_preserves_all_wall_arrangements_and_reversal() {
    let filtered: Vec<_> = (0..80).collect();
    for layout in [
        WallLayout::Uniform,
        WallLayout::Brick,
        WallLayout::Masonry,
        WallLayout::Justified,
        WallLayout::Editorial,
        WallLayout::Cylinder,
    ] {
        let mut params = wall_test_layout();
        params.wall.layout = layout;
        params.wall.scroll_offset = 500.0;
        let forward = wall_visible_poster_indices(&filtered, 0, [420.0, 240.0], params, &[]);
        params.wall.scroll_offset = 0.0;
        let reverse = wall_visible_poster_indices(&filtered, 0, [420.0, 240.0], params, &[]);
        assert!(!forward.is_empty(), "{layout:?}");
        assert!(!reverse.is_empty(), "{layout:?}");
        assert_ne!(forward, reverse, "{layout:?}");
    }
}

#[test]
fn poster_requests_coalesce_equivalent_sets_and_reject_source_changes() {
    let previous = vec![
        PosterRequestIdentity {
            index: 9,
            item_id: "nine".into(),
            source: std::path::PathBuf::from("nine.png"),
            kind: crate::poster_worker::PosterKind::Still,
        },
        PosterRequestIdentity {
            index: 4,
            item_id: "four".into(),
            source: std::path::PathBuf::from("four.png"),
            kind: crate::poster_worker::PosterKind::Still,
        },
    ];
    let reordered = vec![previous[1].clone(), previous[0].clone()];
    let changed_source = vec![
        previous[0].clone(),
        PosterRequestIdentity {
            source: std::path::PathBuf::from("four-new.png"),
            ..previous[1].clone()
        },
    ];

    assert!(!poster_request_changed(Some(&previous), &reordered));
    assert!(poster_request_changed(Some(&previous), &changed_source));
    assert!(poster_request_changed(None, &previous));
}

#[test]
fn stale_poster_completion_is_rejected_after_scroll_reversal_or_hide() {
    assert!(poster_completion_is_current(8, 8, true));
    assert!(!poster_completion_is_current(7, 8, true));
    assert!(!poster_completion_is_current(8, 8, false));
}

#[test]
fn gallery_mode_changes_invalidate_even_when_poster_identity_sets_match() {
    assert!(poster_mode_requires_invalidation(
        GalleryMode::Slices,
        GalleryMode::Wall
    ));
    assert!(poster_mode_requires_invalidation(
        GalleryMode::Geometric,
        GalleryMode::Slices
    ));
    assert!(!poster_mode_requires_invalidation(
        GalleryMode::Wall,
        GalleryMode::Wall
    ));
}

#[test]
fn poster_slots_require_stable_item_and_source_identity() {
    let item = |id: &str, path: &str| WallpaperItem {
        scan_metadata: None,
        id: id.into(),
        title: id.into(),
        folder_path: std::path::PathBuf::new(),
        project_json_path: std::path::PathBuf::new(),
        file_target: path.into(),
        preview_path: None,
        wallpaper_type: "image".into(),
        source: WallpaperSource::LocalImage,
    };
    let original = item("same", "original.png");
    let replacement = item("different", "original.png");
    let changed_path = item("same", "replacement.png");
    let slot = PosterSlot {
        layer: 3,
        kind: crate::poster_worker::PosterKind::Still,
        generation: 0,
        last_requested: 0,
        catalog_generation: 0,
        item_id: original.id.clone(),
        source: poster_source_for_item(&original),
    };

    assert!(poster_slot_matches(&slot, &original));
    assert!(!poster_slot_matches(&slot, &replacement));
    assert!(!poster_slot_matches(&slot, &changed_path));
}

#[test]
fn decoded_poster_publication_requires_the_worker_identity_contract() {
    let item = WallpaperItem {
        scan_metadata: None,
        id: "same".into(),
        title: "same".into(),
        folder_path: std::path::PathBuf::new(),
        project_json_path: std::path::PathBuf::new(),
        file_target: "original.png".into(),
        preview_path: None,
        wallpaper_type: "image".into(),
        source: WallpaperSource::LocalImage,
    };
    let decoded = poster_worker::DecodedPoster {
        source: poster_worker::PosterSource {
            index: 0,
            layer: 3,
            item_id: "same".into(),
            kind: crate::poster_worker::PosterKind::Still,
            video_stamp: None,
            still: None,
            path: "original.png".into(),
        },
        poster: gpu_gallery::video_placeholder_poster(std::path::Path::new("original.png"), 3),
    };

    assert!(poster_completion_matches(&decoded, &item));

    let mut replacement = item.clone();
    replacement.id = "different".into();
    assert!(!poster_completion_matches(&decoded, &replacement));
    replacement.id = item.id.clone();
    replacement.file_target = "replacement.png".into();
    assert!(!poster_completion_matches(&decoded, &replacement));
}

#[test]
fn empty_filtered_wall_and_missing_preview_requests_are_safe() {
    assert!(poster_indices_for_view(
        GalleryMode::Wall,
        &[],
        None,
        [420.0, 240.0],
        wall_test_layout(),
        &[],
    )
    .is_empty());

    let missing = WallpaperItem {
        scan_metadata: None,
        id: "missing".into(),
        title: "missing".into(),
        folder_path: std::path::PathBuf::new(),
        project_json_path: std::path::PathBuf::new(),
        file_target: std::path::PathBuf::from("missing.png"),
        preview_path: None,
        wallpaper_type: "image".into(),
        source: WallpaperSource::LocalImage,
    };
    assert_eq!(poster_source_for_item(&missing), missing.file_target);
}

#[test]
fn p62_p63_engine_preview_discovery_changes_poster_identity_without_broadening_stills() {
    use crate::poster_worker::PosterKind;

    let (profile, _) = poster_worker::cache::tests::fixture();
    let project = profile.join("steamapps/workshop/content/431960/123");
    std::fs::create_dir_all(&project).unwrap();
    let scene = project.join("payload.mp4");
    let preview = project.join("preview.mp4");
    std::fs::write(&scene, b"scene payload with a video extension").unwrap();
    let project_json = project.join("project.json");
    std::fs::write(
        &project_json,
        r#"{"title":"Owned scene","type":"scene","file":"payload.mp4"}"#,
    )
    .unwrap();
    let scan = || {
        scanner::scan_wallpapers_from_libraries_until(
            std::slice::from_ref(&profile),
            false,
            &|| false,
        )
        .remove(0)
    };
    let missing = scan();
    assert!(missing.preview_path.is_none());
    assert_eq!(
        gallery::poster_media_for_item(&missing),
        (scene.clone(), PosterKind::Placeholder)
    );
    let old = poster_worker::PosterSource {
        index: 0,
        layer: 0,
        item_id: missing.id.clone(),
        kind: PosterKind::Placeholder,
        path: scene.clone(),
        still: None,
        video_stamp: None,
    };
    assert!(gallery::poster_source_matches(&old, &missing));

    std::fs::write(&preview, b"owned MP4 scan fixture").unwrap();
    let discovered = scan();
    assert_eq!(missing.id, discovered.id);
    assert_eq!(discovered.preview_path.as_ref(), Some(&preview));
    assert_eq!(
        gallery::poster_media_for_item(&discovered),
        (preview.clone(), PosterKind::Video)
    );
    assert!(!gallery::poster_source_matches(&old, &discovered));
    assert!(library::media_still::MediaStill::select(&discovered, 2).is_none());

    let image = project.join("preview.png");
    std::fs::write(&image, b"corrupt image remains a still decode failure").unwrap();
    assert_eq!(
        gallery::poster_media_for_item(&scan()),
        (image, PosterKind::Still)
    );
    std::fs::write(
        &project_json,
        r#"{"title":"Owned scene","type":"scene","file":"payload.mp4","preview":"preview.mp4"}"#,
    )
    .unwrap();
    assert_eq!(
        gallery::poster_media_for_item(&scan()),
        (preview, PosterKind::Video)
    );
}

#[test]
#[ignore = "requires a Media Foundation H.264 decoder and WALLPAPER_SWITCHER_P6_FIXTURE"]
fn p62_p63_native_engine_previews_settle_and_remap_through_gallery_worker() {
    use crate::poster_worker::PosterKind;
    use std::{sync::mpsc, time::Duration};

    let clip = std::env::var_os("WALLPAPER_SWITCHER_P6_FIXTURE").unwrap();
    let (profile, _) = poster_worker::cache::tests::fixture();
    let projects = profile.join("steamapps/workshop/content/431960");
    for (id, title, preview) in [
        ("101", "Explicit frame", Some("clip.mp4")),
        ("102", "Corrupt preview", Some("preview.mp4")),
        ("103", "Missing preview", None),
    ] {
        let folder = projects.join(id);
        std::fs::create_dir_all(&folder).unwrap();
        let mut project = serde_json::json!({"title":title, "type":"scene", "file":"payload.mp4"});
        if let Some(name) = preview {
            project["preview"] = name.into();
        }
        std::fs::write(folder.join("project.json"), project.to_string()).unwrap();
        // Even a decodable scene payload must not be routed as preview media.
        std::fs::copy(&clip, folder.join("payload.mp4")).unwrap();
    }
    std::fs::copy(&clip, projects.join("101/clip.mp4")).unwrap();
    std::fs::write(projects.join("102/preview.mp4"), b"owned corrupt MP4").unwrap();

    let (mut app, submissions) = d3_application_fixture(0);
    let (finished_tx, finished_rx) = mpsc::channel();
    let mailbox = app.poster_mailbox.clone();
    app.poster_worker = Some(
        poster_worker::PosterWorker::spawn(app.poster_generation.clone(), move |completion| {
            let finished = completion.finished;
            mailbox.publish(completion, || true).is_ok()
                && (!finished || finished_tx.send(()).is_ok())
        })
        .unwrap(),
    );

    let scan = || {
        scanner::scan_wallpapers_from_libraries_until(
            std::slice::from_ref(&profile),
            false,
            &|| false,
        )
    };
    let install = |app: &mut GpuCandidate, items: Vec<WallpaperItem>, revision| {
        assert_eq!(items.len(), 3);
        app.retain_catalog_posters(&items, revision);
        app.catalog_generation.store(revision, Ordering::Release);
        app.catalog = items
            .iter()
            .map(library::CatalogEntry::from_wallpaper)
            .collect();
        app.filtered_indices = (0..items.len()).collect();
        app.wallpapers = items;
        app.selected = Some(0);
        app.rebuild_depth_frame();
    };
    let settle = |app: &mut GpuCandidate| {
        app.schedule_posters();
        assert!(app.poster_loading);
        finished_rx.recv_timeout(Duration::from_secs(15)).unwrap();
        let _ = update(app, Message::Wake(Wake::PostersReady));
        assert!(!app.poster_loading);
        assert!(app.poster_failed_indices.is_empty());
        assert_eq!(app.posters.len(), 3);
    };
    let poster = |app: &GpuCandidate, title: &str| {
        let index = app
            .wallpapers
            .iter()
            .position(|item| item.title == title)
            .unwrap();
        let slot = &app.poster_slots[&index];
        let poster = app
            .posters
            .iter()
            .find(|poster| poster.layer == slot.layer)
            .unwrap();
        assert!(poster.has_pixels());
        (slot.kind, poster.aspect_ratio, poster.source.clone())
    };

    app.gallery_mode = GalleryMode::Wall;
    install(&mut app, scan(), 1);
    settle(&mut app);
    let (kind, aspect, path) = poster(&app, "Explicit frame");
    assert_eq!(
        (kind, aspect, path),
        (PosterKind::Video, 1.5, projects.join("101/clip.mp4"))
    );
    assert_eq!(poster(&app, "Corrupt preview").0, PosterKind::Video);
    assert_eq!(poster(&app, "Corrupt preview").1, 16.0 / 9.0);
    let (kind, aspect, path) = poster(&app, "Missing preview");
    assert_eq!(
        (kind, aspect, path),
        (
            PosterKind::Placeholder,
            16.0 / 9.0,
            projects.join("103/payload.mp4"),
        )
    );

    let added = projects.join("103/preview.mp4");
    std::fs::copy(clip, &added).unwrap();
    app.gallery_mode = GalleryMode::Depth;
    install(&mut app, scan(), 2);
    assert_eq!(
        app.posters.len(),
        1,
        "only the exact successful frame survives remap"
    );
    settle(&mut app);
    assert_eq!(
        poster(&app, "Missing preview"),
        (PosterKind::Video, 1.5, added)
    );
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn catalog_refresh_preserves_selection_by_stable_identity() {
    let item = |id: &str| WallpaperItem {
        scan_metadata: None,
        id: id.into(),
        title: id.into(),
        folder_path: std::path::PathBuf::new(),
        project_json_path: std::path::PathBuf::new(),
        file_target: std::path::PathBuf::new(),
        preview_path: None,
        wallpaper_type: "image".into(),
        source: WallpaperSource::LocalImage,
    };
    let refreshed = vec![item("third"), item("first"), item("second")];

    assert_eq!(wallpaper_index_by_id(&refreshed, "second"), Some(2));
    assert_eq!(wallpaper_index_by_id(&refreshed, "removed"), None);
}

#[test]
fn active_wallpaper_selection_uses_the_first_catalog_match() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("wallpaper-switcher-f5-active-{nonce}"));
    std::fs::create_dir_all(&root).unwrap();
    let first_path = root.join("first.png");
    let active_path = root.join("active.png");
    std::fs::write(&first_path, b"first").unwrap();
    std::fs::write(&active_path, b"active").unwrap();
    let item = |id: &str, path: &std::path::Path| WallpaperItem {
        scan_metadata: None,
        id: id.into(),
        title: id.into(),
        folder_path: path.parent().unwrap().to_path_buf(),
        project_json_path: std::path::PathBuf::new(),
        file_target: path.canonicalize().unwrap(),
        preview_path: None,
        wallpaper_type: "image".into(),
        source: WallpaperSource::LocalImage,
    };
    let wallpapers = vec![item("first", &first_path), item("active", &active_path)];

    assert_eq!(
        wallpaper_index_for_active_files(
            &wallpapers,
            [
                root.join("missing.png").to_string_lossy().into_owned(),
                active_path.to_string_lossy().replace('/', "\\"),
            ],
            &root.join("native-images"),
        ),
        Some(1)
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn catalog_reload_recovers_a_converted_provider_path_without_applying() {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!("wallpaper-switcher-f5-reload-{nonce}"));
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("reload.webp");
    let cache_dir = root.join("native-images");
    std::fs::write(&source, b"catalog-reload-fixture").unwrap();
    let catalog = scanner::scan_local_images(std::slice::from_ref(&root));
    let selected = catalog
        .iter()
        .position(|item| item.file_target == source.canonicalize().unwrap())
        .unwrap();
    let provider_path =
        native_wallpaper::compatible_image_path(&catalog[selected].file_target, &cache_dir)
            .unwrap();

    assert_eq!(
        wallpaper_index_for_active_files(
            &catalog,
            [provider_path.to_string_lossy().into_owned()],
            &cache_dir,
        ),
        Some(selected)
    );
    assert!(!cache_dir.exists());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn backdrop_dimensions_preserve_aspect_and_bound_the_long_edge() {
    assert_eq!(backdrop_dimensions(1920.0, 1080.0), (1600, 900));
    assert_eq!(backdrop_dimensions(1080.0, 1920.0), (900, 1600));
    assert_eq!(backdrop_dimensions(800.0, 600.0), (800, 600));
}

#[test]
fn candidate_preserves_two_layer_per_monitor_visibility() {
    let mut config = Config::default();
    config
        .excluded_wallpapers
        .insert("Monitor0".into(), vec!["soft".into()]);
    config
        .disabled_wallpapers
        .insert("Monitor0".into(), vec!["blocked".into()]);
    config
        .disabled_wallpapers
        .insert("global".into(), vec!["global-blocked".into()]);

    assert!(!wallpaper_allowed_on_monitor(
        &config, "Monitor0", false, "soft"
    ));
    assert!(wallpaper_allowed_on_monitor(
        &config, "Monitor0", true, "soft"
    ));
    assert!(!wallpaper_allowed_on_monitor(
        &config, "Monitor0", true, "blocked"
    ));
    assert!(!wallpaper_allowed_on_monitor(
        &config,
        "Monitor1",
        true,
        "global-blocked"
    ));
    assert!(wallpaper_allowed_on_monitor(
        &config, "Monitor1", false, "soft"
    ));
}

#[test]
fn playlist_ids_are_stable_readable_and_unique() {
    let playlists = vec![Playlist {
        id: "night-city".into(),
        name: "Night City".into(),
        order: PlaybackOrder::Sequential,
        membership: PlaylistMembership::Explicit {
            wallpaper_ids: Vec::new(),
        },
    }];

    assert_eq!(unique_playlist_id("Night City", &playlists), "night-city-2");
    assert_eq!(unique_playlist_id("  ***  ", &playlists), "playlist");
}

#[test]
fn schedule_time_parser_accepts_only_real_local_times() {
    assert_eq!(parse_schedule_time("08:35"), Some(8 * 60 + 35));
    assert_eq!(parse_schedule_time("23:59"), Some(1439));
    assert_eq!(parse_schedule_time("24:00"), None);
    assert_eq!(parse_schedule_time("10:60"), None);
    assert_eq!(parse_schedule_time("soon"), None);
}

#[test]
fn schedule_ids_are_unique() {
    let schedules = vec![candidate_state::Schedule {
        id: "weekday-morning".into(),
        ..Default::default()
    }];

    assert_eq!(
        unique_schedule_id("Weekday morning", &schedules),
        "weekday-morning-2"
    );
}

#[test]
fn scenario_visibility_rejects_a_replaced_or_missing_window() {
    let captured = window::Id::unique();
    let replacement = window::Id::unique();

    assert!(scenario_window_identity_matches(captured, Some(captured)));
    assert!(!scenario_window_identity_matches(
        captured,
        Some(replacement)
    ));
    assert!(!scenario_window_identity_matches(captured, None));
}

#[test]
fn fullscreen_window_starts_unshown_until_the_owned_open_event() {
    let settings = window_settings(None);
    assert!(settings.fullscreen && !settings.visible && !settings.decorations);

    let (mut app, _) = d3_application_fixture(0);
    let id = app.window.unwrap();
    let original = app.viewport;
    let _ = app.set_visible_owned(false, "test-hide-fullscreen");
    let _ = update(
        &mut app,
        Message::WindowOpened {
            id,
            width: 400.0,
            height: 300.0,
        },
    );
    let _ = update(
        &mut app,
        Message::WindowResized {
            id,
            width: 400.0,
            height: 300.0,
        },
    );
    let _ = update(&mut app, Message::WindowFocused(id));
    assert!(!app.visible);
    assert_eq!(app.window, Some(id));
    assert_eq!(app.viewport, original);
    assert_eq!(app.window_focused, None);
}

fn explicit_test_playlist(member_ids: &[&str]) -> Playlist {
    Playlist {
        id: "snapshot-test-playlist".into(),
        name: "Snapshot test".into(),
        order: PlaybackOrder::Sequential,
        membership: PlaylistMembership::Explicit {
            wallpaper_ids: member_ids.iter().map(|id| (*id).to_owned()).collect(),
        },
    }
}

fn snapshot_test_catalog_entry(id: &str) -> library::CatalogEntry {
    library::CatalogEntry {
        id: id.to_owned(),
        title: "Snapshot fixture".into(),
        kind: "image".into(),
        folder: std::path::PathBuf::from(r"\\?\C:\snapshot-fixtures"),
        width: 1,
        height: 1,
        modified_unix: 0,
        source_tags: Default::default(),
        tags: BTreeSet::new(),
    }
}

fn snapshot_test_wallpaper(id: &str, file_target: &str, source: WallpaperSource) -> WallpaperItem {
    let file_target = std::path::PathBuf::from(file_target);
    WallpaperItem {
        scan_metadata: None,
        id: id.to_owned(),
        title: "Snapshot fixture".into(),
        folder_path: file_target
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf(),
        project_json_path: file_target.clone(),
        file_target: file_target.clone(),
        preview_path: Some(file_target),
        wallpaper_type: "image".into(),
        source,
    }
}

#[test]
fn playlist_catalog_snapshot_uses_observed_catalog_and_existing_evaluator() {
    let id = r"local:\\?\c:\snapshot-fixtures\one.png";
    let file_target = r"\\?\C:\snapshot-fixtures\one.png";
    let playlist = explicit_test_playlist(&[id]);
    let catalog = vec![snapshot_test_catalog_entry(id)];
    let wallpapers = vec![snapshot_test_wallpaper(
        id,
        file_target,
        WallpaperSource::LocalImage,
    )];
    let observation =
        explicit_playlist_catalog_observation(&playlist, &catalog, &wallpapers, &BTreeSet::new())
            .expect("explicit playlist should produce a snapshot");

    assert_eq!(observation.playlist_id, "snapshot-test-playlist");
    assert_eq!(observation.members.len(), 1);
    assert_eq!(observation.members[0].status, "resolved");
    assert_eq!(observation.members[0].catalog_match_count, 1);
    assert_eq!(
        observation.members[0].observed_catalog_member_ids,
        vec![id.to_owned()]
    );
    assert_eq!(observation.members[0].wallpaper_matches.len(), 1);
    assert_eq!(observation.members[0].wallpaper_matches[0].id, id);
    assert_eq!(
        observation.members[0].wallpaper_matches[0].source_type,
        "LocalImage"
    );
    assert_eq!(
        observation.members[0].wallpaper_matches[0].canonical_file_target,
        file_target
    );
    assert_eq!(observation.evaluator_result_ids, vec![id.to_owned()]);
}

#[test]
fn playlist_catalog_snapshot_reports_missing_and_ambiguous_members() {
    let id = r"local:\\?\c:\snapshot-fixtures\one.png";
    let playlist = explicit_test_playlist(&[id]);
    let missing = explicit_playlist_catalog_observation(&playlist, &[], &[], &BTreeSet::new())
        .expect("explicit playlist should produce a snapshot");
    assert_eq!(missing.members[0].status, "catalog_member_missing");
    assert!(missing.evaluator_result_ids.is_empty());

    let ambiguous = explicit_playlist_catalog_observation(
        &playlist,
        &[
            snapshot_test_catalog_entry(id),
            snapshot_test_catalog_entry(id),
        ],
        &[
            snapshot_test_wallpaper(
                id,
                r"\\?\C:\snapshot-fixtures\one.png",
                WallpaperSource::LocalImage,
            ),
            snapshot_test_wallpaper(
                id,
                r"\\?\C:\snapshot-fixtures\duplicate.png",
                WallpaperSource::LocalImage,
            ),
        ],
        &BTreeSet::new(),
    )
    .expect("explicit playlist should produce a snapshot");
    assert_eq!(ambiguous.members[0].status, "ambiguous");
    assert_eq!(ambiguous.members[0].catalog_match_count, 2);
    assert_eq!(ambiguous.members[0].wallpaper_matches.len(), 2);
    assert_eq!(ambiguous.evaluator_result_ids, vec![id.to_owned()]);
}

#[test]
fn catalog_snapshot_includes_only_active_and_enabled_schedule_playlists() {
    let active_id = r"local:\\?\c:\snapshot-fixtures\active.png";
    let scheduled_id = r"local:\\?\c:\snapshot-fixtures\scheduled.png";
    let unrelated_id = r"local:\\?\c:\snapshot-fixtures\unrelated.png";
    let mut state = CandidateState::default();
    state.active_playlist_id = Some("active".into());
    state.schedules.push(candidate_state::Schedule {
        id: "schedule".into(),
        enabled: true,
        playlist_id: "scheduled".into(),
        ..Default::default()
    });
    state.playlists = vec![
        Playlist {
            id: "active".into(),
            ..explicit_test_playlist(&[active_id])
        },
        Playlist {
            id: "scheduled".into(),
            ..explicit_test_playlist(&[scheduled_id])
        },
        Playlist {
            id: "unrelated".into(),
            ..explicit_test_playlist(&[unrelated_id])
        },
    ];

    let snapshot = build_catalog_playlist_snapshot(
        &state,
        &[],
        &[],
        std::path::PathBuf::from(r"C:\profiles\snapshot"),
        true,
        7,
    );
    assert!(snapshot.catalog_ready);
    assert_eq!(snapshot.catalog_generation, 7);
    assert_eq!(snapshot.profile_path, r"C:\profiles\snapshot");
    assert_eq!(
        snapshot.configured_playlist_ids,
        vec!["active".to_owned(), "scheduled".to_owned()]
    );
    assert_eq!(
        snapshot
            .explicit_playlists
            .iter()
            .map(|playlist| playlist.playlist_id.as_str())
            .collect::<Vec<_>>(),
        vec!["active", "scheduled"]
    );
    assert!(snapshot
        .explicit_playlists
        .iter()
        .all(|playlist| playlist.members[0].status == "catalog_member_missing"));
}

#[test]
fn disabled_catalog_snapshot_trace_skips_evaluation_and_serialization() {
    let previous = apply_trace::enabled();
    apply_trace::set_enabled(false);
    let evaluated = std::cell::Cell::new(false);
    let id = r"local:\\?\c:\snapshot-fixtures\one.png";
    let playlist = explicit_test_playlist(&[id]);
    let catalog = vec![snapshot_test_catalog_entry(id)];
    let wallpapers = vec![snapshot_test_wallpaper(
        id,
        r"\\?\C:\snapshot-fixtures\one.png",
        WallpaperSource::LocalImage,
    )];

    apply_trace::event_lazy(PLAYLIST_CATALOG_SNAPSHOT_STAGE, || {
        evaluated.set(true);
        let snapshot = explicit_playlist_catalog_observation(
            &playlist,
            &catalog,
            &wallpapers,
            &BTreeSet::new(),
        )
        .expect("test playlist is explicit");
        serde_json::to_string(&snapshot).expect("snapshot serializes")
    });

    apply_trace::set_enabled(previous);
    assert!(!evaluated.get());
}
#[test]
fn p3_composed_source_uses_actual_legacy_geometry_and_keeps_percent_units_explicit() {
    use composed::SettingsSource;
    use view::settings::{bridge, composed};
    let (mut app, submissions) = d3_application_fixture(2);
    app.state.gallery.geometric.curve_strength = 1.75;
    app.state.gallery.slices.wobble_strength = 0.8;
    app.state.gallery.wall.stagger = 0.37;
    app.state.gallery.wall.cylinder_bend = 0.21;
    app.state.gallery.depth.height = 710.0;
    let source = bridge::Source { app: &app };
    assert_eq!(
        source.number(composed::keys::selector::HEX_ARC_INTENSITY_X10),
        17.5
    );
    assert_eq!(
        source.number(composed::keys::selector::SLICE_WOBBLE_STRENGTH),
        80.0
    );
    assert!((source.number(composed::keys::selector::GRID_STAGGER) - 37.0).abs() < 0.001);
    assert!((source.number(composed::keys::selector::GRID_CYLINDER_BEND) - 21.0).abs() < 0.001);
    assert_eq!(source.number(composed::keys::selector::DEPTH_HEIGHT), 710.0);
    assert_eq!(source.on_battery_power(), None);
    assert!(source.graphics_devices().is_empty());
    assert_eq!(submissions.load(Ordering::SeqCst), 0);
}

#[test]
fn p3_source_corner_fields_seed_legacy_radius_and_preserve_independent_local_edits() {
    use composed::{draft::Value, SettingsSource};
    use view::settings::{bridge, composed};
    let (mut app, submissions) = d3_application_fixture(1);
    app.state.gallery.slices.corner_radius = 48.0;
    let corners = [
        composed::keys::selector::CORNER_TL,
        composed::keys::selector::CORNER_TR,
        composed::keys::selector::CORNER_BR,
        composed::keys::selector::CORNER_BL,
    ];
    for corner in corners {
        assert_eq!((bridge::Source { app: &app }).number(corner), 48.0);
        assert_eq!((bridge::Source { app: &app }).text(corner), "48");
    }
    assert!((bridge::Source { app: &app }).flag(composed::keys::selector::ROUND_CORNERS));
    app.settings_workbench
        .draft
        .values
        .insert(corners[0].into(), Value::Number(20.0));
    assert_eq!((bridge::Source { app: &app }).number(corners[0]), 20.0);
    assert_eq!((bridge::Source { app: &app }).text(corners[0]), "20");
    for corner in &corners[1..] {
        assert_eq!((bridge::Source { app: &app }).number(corner), 48.0);
    }
    assert_eq!(app.state.gallery.slices.corner_radius, 48.0);
    app.settings_workbench.draft.values.insert(
        composed::keys::selector::ROUND_CORNERS.into(),
        Value::Boolean(false),
    );
    assert!(!(bridge::Source { app: &app }).flag(composed::keys::selector::ROUND_CORNERS));
    assert_eq!(app.state.gallery.slices.corner_radius, 48.0);
    assert_eq!(submissions.load(Ordering::SeqCst), 0);
}

#[test]
fn p3_source_drafts_keep_pending_policy_separate_from_actual_automation_and_geometry() {
    use composed::{draft::Value, SettingsSource};
    use view::settings::{bridge, composed};
    let (mut app, submissions) = d3_application_fixture(2);
    app.state.slideshow_enabled = false;
    app.state.slideshow_interval_seconds = 900;
    app.state.gallery.depth.height = 630.0;
    app.settings_workbench.draft.values.insert(
        composed::keys::general::RANDOM_ROTATE.into(),
        Value::Boolean(true),
    );
    app.settings_workbench.draft.values.insert(
        composed::keys::selector::DEPTH_HEIGHT.into(),
        Value::Number(1200.0),
    );
    app.settings_workbench.draft.values.insert(
        composed::keys::playback::FULLSCREEN.into(),
        Value::Boolean(true),
    );
    let source = bridge::Source { app: &app };
    assert!(!source.flag(composed::keys::general::RANDOM_ROTATE));
    assert_eq!(
        source.number(composed::keys::general::RANDOM_INTERVAL),
        900.0
    );
    assert_eq!(source.number(composed::keys::selector::DEPTH_HEIGHT), 630.0);
    assert!(source.flag(composed::keys::playback::FULLSCREEN));
    assert!(!app.state.slideshow_enabled);
    assert!(app.state.schedules.is_empty());
    assert_eq!(submissions.load(Ordering::SeqCst), 0);
}

#[test]
fn p3_binding_projection_preserves_configured_navigation_before_source_draft_override() {
    use composed::{draft::Value, input, SettingsSource};
    use view::settings::{bridge, composed};
    let (mut app, submissions) = d3_application_fixture(1);
    app.config.keybinds.left = "f10".into();
    let source = bridge::Source { app: &app };
    assert_eq!(
        source.bindings().triggers(input::InputAction::NavLeft),
        input::parse_binding("f10").unwrap()
    );
    let descriptor = composed::KEY_BINDINGS
        .iter()
        .find(|entry| entry.action == input::InputAction::NavLeft)
        .unwrap();
    app.settings_workbench
        .draft
        .values
        .insert(descriptor.path.into(), Value::Text("ctrl+k".into()));
    let source = bridge::Source { app: &app };
    assert_eq!(
        source.bindings().triggers(input::InputAction::NavLeft),
        input::parse_binding("ctrl+k").unwrap()
    );
    assert_eq!(app.config.keybinds.left, "f10");
    assert_eq!(submissions.load(Ordering::SeqCst), 0);
}

#[test]
fn w5_motion_section_navigation_keeps_branch_and_group_state() {
    use view::settings::{
        composed::{Control, SettingsFocus, SettingsKey},
        folio::intent::{Message as Intent, SettingsMsg},
    };

    let mut app = p3_composed_editor_app(SettingsCategory::Motion);
    let cards = view::settings::controller::cards(&app);
    let section = |key| {
        cards
            .iter()
            .position(|(card, _)| card.title == app.settings_locale.text_static(key))
            .unwrap()
    };
    let shared = section("settings-section-shared-speeds");
    let launch = section("settings-section-launch");
    let picker = section("settings-section-picker-motion");
    let (row, id) = cards[shared]
        .1
        .iter()
        .enumerate()
        .find_map(|(i, r)| {
            matches!(&r.control, Control::MotionWeights { .. })
                .then(|| (i, r.control.compact_bar_id().unwrap()))
        })
        .unwrap();

    for (intent, expected) in [
        (SettingsMsg::SelectSection(launch), launch),
        (SettingsMsg::SelectSection(picker), picker),
        (SettingsMsg::Key(SettingsKey::Up), launch),
        (SettingsMsg::Key(SettingsKey::Down), picker),
        (SettingsMsg::SelectSection(shared), shared),
    ] {
        p3_settings_intent(&mut app, intent);
        let s = &app.settings_workbench;
        assert_eq!(
            (s.navigation.section, s.navigation.focus),
            (expected, SettingsFocus::Sections)
        );
        assert_eq!(s.tab_reveal.unwrap().x, 1.0);
        assert!(!s.navigation.open_bars.contains(&id));
        assert!(!s.bar_reveals.contains_key(&id));
    }

    p3_settings_intent(&mut app, SettingsMsg::ToggleBar(id.clone(), row));
    assert!(app.settings_workbench.navigation.open_bars.contains(&id));
    let now = std::time::Instant::now();
    for ms in [0, 16] {
        let generation = app.settings_workbench.generation;
        let window = app.window;
        let _ = update(
            &mut app,
            Message::SettingsIntent {
                window,
                generation,
                intent: Intent::Frame(now + std::time::Duration::from_millis(ms)),
            },
        );
    }

    let before = app.settings_workbench.bar_reveals[&id];
    assert_eq!(before.target, 1.0);
    assert!(0.0 < before.x && before.x < 1.0);
    for section in [launch, picker, shared] {
        p3_settings_intent(&mut app, SettingsMsg::SelectSection(section));
        let after = app.settings_workbench.bar_reveals[&id];
        assert_eq!((after.x, after.target), (before.x, before.target));
        assert!(app.settings_workbench.navigation.open_bars.contains(&id));
    }
}

#[test]
#[ignore = "Owned headless WGPU provider recheck; run by exact name"]
fn windows_provider_recheck_click_requires_current_preflight_and_fresh_observation() {
    use coordinator::{ApplyOperationState, RecheckFixture, RecoveryPhase};

    fn open_displays(app: &mut GpuCandidate) {
        let message = Message::SettingsOpenPage {
            window: app.window,
            generation: app.settings_workbench.generation,
            category: SettingsCategory::WallpapersDisplays,
        };
        let _ = update(app, message);
        assert_eq!(app.settings_workbench.tab, "displays");
    }

    fn click(app: &GpuCandidate, renderer: &iced::Renderer, record: &ProviderRecovery) -> Message {
        pk07::Scene::new(app, renderer).hit_label(
            renderer,
            app.settings_locale
                .text_static("settings-windows-recheck-provider"),
            |message| {
                matches!(
                    message,
                    Message::SettingsRecheckProvider { recovery, .. } if recovery == record
                )
            },
        )
    }

    let renderer = pk07::renderer();
    for phase in [RecoveryPhase::Preflight, RecoveryPhase::DispatchedOrUnknown] {
        let mut app = p3_composed_editor_app(SettingsCategory::WallpapersDisplays);
        let (fixture, owner) = RecheckFixture::new(phase);
        app.coordinator = owner;
        let record = fixture.record.clone();
        app.viewport = [1920.0, 1080.0];
        app.apply_enabled = false;
        app.provider_state_unknown = true;
        app.automation_suspended = true;
        app.provider_recoveries = vec![record.clone()];
        open_displays(&mut app);
        let probe = pk07::Scene::new(&app, &renderer).probe(&renderer);
        assert!(!probe
            .texts
            .iter()
            .any(|(text, _)| text.contains("private-fixture")));

        let bound =
            |app: &GpuCandidate, recovery: ProviderRecovery| Message::SettingsRecheckProvider {
                window: app.window,
                generation: app.settings_workbench.generation,
                recovery,
            };

        let stale = click(&app, &renderer, &record);
        let generation = app.settings_workbench.generation;
        let _ = update(
            &mut app,
            Message::SetSettingsCategory(SettingsCategory::Picker),
        );
        open_displays(&mut app);
        assert_ne!(app.settings_workbench.generation, generation);
        let _ = update(&mut app, stale);
        assert_eq!(fixture.take(), None);

        let current = click(&app, &renderer, &record);
        let mut revised = record.clone();
        revised.reason.push_str(" (new observation)");
        app.provider_recoveries[0] = revised.clone();
        let _ = update(&mut app, current.clone()); // Stale in app.
        assert_eq!(fixture.take(), None);
        let owner_stale = bound(&app, revised);
        let _ = update(&mut app, owner_stale); // Matches app, differs at coordinator.
        assert_eq!(fixture.take(), None);
        app.provider_recoveries[0] = record.clone();

        let policy = app.launch_policy;
        app.launch_policy = p1_preview_policy();
        app.apply_enabled = true;
        let _ = update(&mut app, current.clone());
        assert_eq!(fixture.take(), None);
        app.launch_policy = policy;
        app.apply_enabled = false;

        app.apply_pending = true;
        let _ = update(&mut app, current);
        assert_eq!(fixture.take(), None);
        app.apply_pending = false;

        let saved_state = app.state.clone();
        for saved_only in [true, false] {
            let action = click(&app, &renderer, &record);
            let _ = update(&mut app, action.clone());
            let queued = fixture.take().expect("mounted button queued one command");
            assert_eq!(app.active_operation, Some(queued.0));
            assert!(app.apply_pending && app.provider_state_unknown);

            // The queue is empty now, so capacity cannot conceal a duplicate.
            let _ = update(&mut app, action);
            assert_eq!(fixture.take(), None);

            let result = fixture.finish(queued, saved_only);
            assert_eq!(result.outcomes.len(), 1);
            assert_eq!(
                result.outcomes[0].state,
                if saved_only {
                    ApplyOperationState::Unknown
                } else if phase == RecoveryPhase::DispatchedOrUnknown {
                    ApplyOperationState::ProviderConfirmed
                } else {
                    ApplyOperationState::Cancelled
                },
            );
            let _ = update(
                &mut app,
                Message::Wake(Wake::CoordinatorFinished(CoordinatorResult::Engine(result))),
            );
            assert!(!app.apply_pending && app.active_operation.is_none());
            assert_eq!(app.provider_state_unknown, saved_only);
            assert_eq!(app.provider_recoveries.contains(&record), saved_only);
            assert_eq!(coordinator::provider_recheck_available(&record), saved_only);
            assert_eq!(fixture.take(), None);
            assert_eq!(app.state, saved_state);
        }
    }
}

fn p3_composed_editor_app(category: SettingsCategory) -> GpuCandidate {
    let (mut app, _) = d3_application_fixture(2);
    app.panel_state.open_settings(category);
    view::settings::controller::reconcile(&mut app);
    app
}

fn p3_settings_intent(app: &mut GpuCandidate, intent: view::settings::folio::intent::SettingsMsg) {
    let generation = app.settings_workbench.generation;
    let _ = view::settings::controller::dispatch(
        app,
        generation,
        view::settings::folio::intent::Message::Settings(intent),
    );
    view::settings::controller::reconcile(app);
}

#[test]
fn p3_controller_invalid_navigation_keeps_editor_and_escape_restores_live_geometry() {
    use view::settings::{
        composed::{keys, SettingsKey},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let original = app.state.gallery.depth.height;
    let path = keys::selector::DEPTH_HEIGHT.to_string();
    p3_settings_intent(&mut app, SettingsMsg::Input(path.clone(), "700".into()));
    assert_eq!(app.state.gallery.depth.height, 700.0);
    p3_settings_intent(&mut app, SettingsMsg::Input(path.clone(), "-".into()));
    p3_settings_intent(
        &mut app,
        SettingsMsg::Key(SettingsKey::CategoryNext { backwards: false }),
    );
    assert_eq!(app.settings_workbench.tab, "picker");
    assert_eq!(
        app.settings_workbench.navigation.input_edit.as_deref(),
        Some(path.as_str())
    );
    assert_eq!(app.settings_workbench.draft.raw_inputs[&path], "-");
    p3_settings_intent(&mut app, SettingsMsg::Key(SettingsKey::Cancel));
    assert_eq!(app.state.gallery.depth.height, original);
    assert!(!app.settings_workbench.draft.errors.contains_key(&path));
    assert!(!app.settings_workbench.draft.raw_inputs.contains_key(&path));
    assert!(app.settings_workbench.navigation.input_edit.is_none());
}

#[test]
fn p3_controller_motion_weight_tab_commits_one_field_and_keeps_next_editor() {
    use view::settings::{
        composed::{self, Control, SettingsFocus, SettingsKey},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Motion);
    let model = view::settings::controller::cards(&app);
    let (section, row, first, second) = model
        .iter()
        .enumerate()
        .find_map(|(section, (_, rows))| {
            rows.iter()
                .enumerate()
                .find_map(|(row, item)| match &item.control {
                    Control::MotionWeights { weights } if weights.len() >= 2 => {
                        Some((section, row, weights[0].1.clone(), weights[1].1.clone()))
                    }
                    _ => None,
                })
        })
        .expect("actual composed MotionWeights row");
    app.settings_workbench.navigation.section = section;
    app.settings_workbench.navigation.row = row;
    app.settings_workbench.navigation.focus = SettingsFocus::Controls;
    p3_settings_intent(&mut app, SettingsMsg::Input(first.clone(), "200".into()));
    p3_settings_intent(
        &mut app,
        SettingsMsg::Key(SettingsKey::FocusNext { backwards: false }),
    );
    assert_eq!(
        app.settings_workbench.navigation.input_edit.as_deref(),
        Some(second.as_str())
    );
    assert!(!app.settings_workbench.draft.raw_inputs.contains_key(&first));
    assert_eq!(
        app.settings_workbench.draft.values[&first],
        composed::draft::Value::Number(200.0)
    );
}

#[test]
fn p3_controller_source_category_departure_invalidates_legacy_theme_owner() {
    use view::settings::folio::intent::SettingsMsg;
    let mut app = p3_composed_editor_app(SettingsCategory::Theme);
    assert!(lifecycle::theme_owner_active(&app));
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("performance".into()));
    assert_eq!(app.settings_workbench.tab, "performance");
    assert!(!lifecycle::theme_owner_active(&app));
}

#[test]
fn compatibility_owner_p3_controller_removed_record_cannot_retarget_stale_indexed_callback() {
    use view::settings::{
        composed::{keys, ActionId},
        folio::intent::{Message as Intent, SettingsMsg},
    };
    let mut app = p3_composed_editor_app(SettingsCategory::CatalogFilters);
    app.settings_workbench.lists.action(
        ActionId::CreateResolutionPresetBand("fhd"),
        &app.settings_locale,
    );
    app.settings_workbench.lists.action(
        ActionId::CreateResolutionPresetBand("qhd"),
        &app.settings_locale,
    );
    view::settings::controller::reconcile(&mut app);
    let survivor = app.settings_workbench.lists.resolutions[1].clone();
    let path = format!("{}.0.label", keys::filter_bar::RESOLUTION_PRESETS);
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input(path.clone(), "Edited first".into()),
    );
    let old_generation = app.settings_workbench.generation;
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Run(ActionId::RemoveResolutionPreset(0)),
    );
    assert_eq!(
        app.settings_workbench.lists.resolutions,
        vec![survivor.clone()]
    );
    assert!(!app
        .settings_workbench
        .render_values
        .values()
        .any(|value| value == "Edited first"));
    let _ = view::settings::controller::dispatch(
        &mut app,
        old_generation,
        Intent::Settings(SettingsMsg::Input(path, "Stale".into())),
    );
    assert_eq!(app.settings_workbench.lists.resolutions, vec![survivor]);
}

#[test]
fn p3_actual_app_source_builds_owned_workbench_and_search_without_mutation() {
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.settings_search = "duration".into();
    view::settings::controller::reconcile(&mut app);
    let before = app.state.clone();
    let element = view::settings::bridge::view(&app);
    assert!(!iced::advanced::widget::Tree::new(&element)
        .children
        .is_empty());
    assert!(!app.settings_workbench.results.is_empty());
    assert_eq!(app.state, before);
}

#[test]
fn p3_controller_invalid_field_blocks_pointer_departure_and_other_field_input() {
    use view::settings::{composed::keys, folio::intent::SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let first = keys::selector::DEPTH_HEIGHT.to_string();
    let second = keys::selector::DEPTH_WIDTH_PX.to_string();
    let original_width = app.state.gallery.depth.width;
    p3_settings_intent(&mut app, SettingsMsg::Input(first.clone(), "-".into()));
    p3_settings_intent(&mut app, SettingsMsg::SearchOpen);
    assert!(!app.settings_workbench.navigation.search_open);
    p3_settings_intent(&mut app, SettingsMsg::LeaveLayoutStudio);
    p3_settings_intent(&mut app, SettingsMsg::Input(second.clone(), "555".into()));
    assert_eq!(
        app.settings_workbench.navigation.input_edit.as_deref(),
        Some(first.as_str())
    );
    assert_eq!(app.settings_workbench.draft.raw_inputs[&first], "-");
    assert!(!app
        .settings_workbench
        .draft
        .raw_inputs
        .contains_key(&second));
    assert_eq!(app.state.gallery.depth.width, original_width);
}

#[test]
fn p3_language_pick_preserves_source_tab_query_selection_and_expires_old_callbacks() {
    use view::settings::{
        composed::keys,
        folio::intent::{Message as Intent, SettingsMsg},
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("language".into()));
    app.settings_search = "language".into();
    let selected = app.selected;
    let old = app.settings_workbench.generation;
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick(keys::general::LANGUAGE.into(), "zh-CN".into()),
    );
    assert_eq!(app.settings_locale.language.tag, "zh-CN");
    assert_eq!(app.settings_workbench.tab, "language");
    assert_eq!(app.settings_search, "language");
    assert_eq!(app.selected, selected);
    assert_ne!(app.settings_workbench.generation, old);
    let _ = view::settings::controller::dispatch(
        &mut app,
        old,
        Intent::Settings(SettingsMsg::Pick(
            keys::general::LANGUAGE.into(),
            "en-US".into(),
        )),
    );
    assert_eq!(app.settings_locale.language.tag, "zh-CN");
}

#[test]
fn p3_settings_callback_from_replaced_window_is_rejected_before_rebinding() {
    use view::settings::folio::intent::{Message as Intent, SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let old = app.settings_workbench.generation;
    app.window = Some(window::Id::unique());
    let _ = view::settings::controller::dispatch(
        &mut app,
        old,
        Intent::Settings(SettingsMsg::SelectTab("language".into())),
    );
    assert_eq!(app.settings_workbench.tab, "picker");
    view::settings::controller::reconcile(&mut app);
    assert_ne!(app.settings_workbench.generation, old);
}

#[test]
fn p3_keyboard_activation_enters_current_choice_then_applies_exact_option() {
    use view::settings::{
        composed::{Control, SettingsFocus, SettingsKey, SettingsSource},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let model = view::settings::controller::cards(&app);
    let (section, row, path, expected) = model
        .iter()
        .enumerate()
        .find_map(|(section, (_, rows))| {
            rows.iter()
                .enumerate()
                .find_map(|(row, item)| match &item.control {
                    Control::Dropdown { path, options, .. } if options.len() > 1 => {
                        Some((section, row, path.clone(), options[1].0.clone()))
                    }
                    _ => None,
                })
        })
        .expect("actual dropdown");
    app.settings_workbench.navigation.section = section;
    app.settings_workbench.navigation.row = row;
    app.settings_workbench.navigation.focus = SettingsFocus::Controls;
    p3_settings_intent(&mut app, SettingsMsg::Key(SettingsKey::Activate));
    assert!(app.settings_workbench.navigation.choice.is_some());
    app.settings_workbench.navigation.choice = Some(1);
    p3_settings_intent(&mut app, SettingsMsg::Key(SettingsKey::Activate));
    assert_eq!(
        view::settings::bridge::Source { app: &app }.text(&path),
        expected
    );
    assert!(app.settings_workbench.navigation.choice.is_none());
}

#[test]
fn p3_keyboard_search_activation_uses_composed_result_and_keeps_gallery_selection() {
    use view::settings::{
        composed::{SettingsFocus, SettingsKey},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.settings_search = "language".into();
    app.settings_workbench.navigation.search_open = true;
    view::settings::controller::reconcile(&mut app);
    let result = app.settings_workbench.results.first().cloned().unwrap();
    let selected = app.selected;
    p3_settings_intent(&mut app, SettingsMsg::Key(SettingsKey::Activate));
    assert_eq!(app.settings_workbench.tab, result.tab);
    assert_eq!(app.settings_workbench.navigation.section, result.section);
    assert_eq!(app.settings_workbench.navigation.row, result.row);
    assert_eq!(
        app.settings_workbench.navigation.focus,
        SettingsFocus::Controls
    );
    assert!(!app.settings_workbench.navigation.search_open);
    assert_eq!(app.selected, selected);
}

#[test]
fn compatibility_owner_p3_process_rule_form_retains_manual_names_locally_without_automation_activation(
) {
    use view::settings::{
        composed::{keys, ActionId, SettingsSource},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Playback);
    let state = app.state.clone();
    compatibility_owner_intent(&mut app, SettingsMsg::Run(ActionId::ChooseRunningProcess));
    assert!(app.settings_workbench.process_picker.is_some());
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::ProcessPickerManualInput("example.exe".into()),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::ProcessPickerManualAdd);
    assert_eq!(
        view::settings::bridge::Source { app: &app }.text(keys::playback::PROCESSES),
        "example.exe"
    );
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::ProcessPickerManualInput("example.exe".into()),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::ProcessPickerManualAdd);
    assert_eq!(
        app.settings_workbench
            .process_picker
            .as_ref()
            .unwrap()
            .targets,
        vec!["example.exe"]
    );
    compatibility_owner_intent(&mut app, SettingsMsg::ProcessPickerClose);
    assert!(app.settings_workbench.process_picker.is_none());
    let mut expected = state;
    expected.pending_frontend = app.state.pending_frontend.clone();
    assert_eq!(app.state, expected);
    assert_eq!(
        CandidateState::load(&app.state_path).pending_frontend,
        app.state.pending_frontend
    );
    assert!(app
        .state
        .pending_frontend
        .settings
        .iter()
        .any(|entry| entry.source_path == keys::playback::PROCESSES
            && entry.value_json == r#""example.exe""#));
}

#[test]
fn p3_settings_key_route_keeps_search_and_capture_local_and_rejects_obsolete_window() {
    use keyboard::{key::Named, Key, Modifiers};
    use view::settings::{
        composed::{
            self,
            input::{InputAction, InputMap, KeyId, Trigger},
        },
        controller,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let window = app.window.unwrap();
    let old_window = iced::window::Id::unique();
    assert!(controller::route_key(
        &mut app,
        &Key::Named(Named::Escape),
        Modifiers::empty(),
        old_window,
        false
    )
    .is_none());
    let _ = controller::route_key(
        &mut app,
        &Key::Character("f".into()),
        Modifiers::CTRL,
        window,
        true,
    );
    assert!(app.settings_workbench.navigation.search_open);
    assert!(!app.panel_state.search_open);
    let descriptor = composed::KEY_BINDINGS
        .into_iter()
        .find(|entry| entry.action == InputAction::Apply)
        .unwrap();
    app.settings_workbench.capture = composed::binding_capture::Capture::begin(
        app.settings_workbench.generation,
        descriptor.path,
        0,
        &InputMap::default(),
    );
    let selected = app.selected;
    let _ = controller::route_key(
        &mut app,
        &Key::Named(Named::F10),
        Modifiers::CTRL,
        window,
        true,
    );
    let capture = app.settings_workbench.capture.as_ref().unwrap();
    assert!(
        matches!(&capture.triggers[0],Trigger::Key(spec)if spec.id==KeyId::Function(10)&&spec.mods.ctrl)
    );
    assert_eq!(app.selected, selected);
    assert!(app.panel_state.settings_open);
    let _ = controller::route_key(
        &mut app,
        &Key::Named(Named::Escape),
        Modifiers::empty(),
        window,
        true,
    );
    assert!(app.settings_workbench.capture.is_none());
    assert!(app.settings_workbench.navigation.search_open);
    let _ = controller::route_key(
        &mut app,
        &Key::Named(Named::Escape),
        Modifiers::empty(),
        window,
        false,
    );
    assert!(!app.settings_workbench.navigation.search_open);
    assert!(app.panel_state.settings_open);
}

#[test]
fn p3_settings_custom_close_is_distinct_from_edit_escape() {
    use keyboard::{key::Named, Key, Modifiers};
    use view::settings::{composed::keys, controller, folio::intent::SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.config.keybinds.close = "f10".into();
    let window = app.window.unwrap();
    let path = keys::selector::DEPTH_HEIGHT.to_string();
    p3_settings_intent(&mut app, SettingsMsg::Input(path.clone(), "-".into()));
    let _ = controller::route_key(
        &mut app,
        &Key::Named(Named::F10),
        Modifiers::empty(),
        window,
        false,
    );
    assert!(app.panel_state.settings_open);
    assert!(app.settings_workbench.draft.errors.contains_key(&path));
    let _ = controller::route_key(
        &mut app,
        &Key::Named(Named::Escape),
        Modifiers::empty(),
        window,
        false,
    );
    assert!(app.panel_state.settings_open);
    assert!(!app.settings_workbench.draft.errors.contains_key(&path));
    let _ = controller::route_key(
        &mut app,
        &Key::Named(Named::F10),
        Modifiers::empty(),
        window,
        false,
    );
    assert!(!app.panel_state.settings_open);
    assert!(app.visible);
}

#[test]
fn w1_retired_display_controls_refuse_callbacks_and_keep_observations() {
    use view::settings::{composed, folio::intent::SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::Automation);
    let before = app.state.clone();
    app.settings_search = "placement".into();
    view::settings::controller::reconcile(&mut app);
    assert!(app
        .settings_workbench
        .results
        .iter()
        .all(|result| result.tab != "displays"));
    let displays = view::settings::bridge::build(&app, "displays");
    assert_eq!(displays.len(), 1);
    assert_eq!(displays[0].1.len(), 1);
    assert!(matches!(
        displays[0].1[0].control,
        composed::Control::PlatformForm
    ));
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("displays".into()));
    p3_settings_intent(
        &mut app,
        SettingsMsg::BackgroundPicker("display.fixture".into()),
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::BackgroundColor("display.fixture".into(), "#123456".into()),
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::BackgroundCommit("display.fixture".into()),
    );
    assert_eq!(app.state, before);
    assert!(!app.apply_pending && app.active_operation.is_none());
}

#[test]
fn p3_capture_enter_confirms_existing_trigger_and_mouse_capture_keeps_modifiers() {
    use keyboard::{key::Named, Key, Modifiers};
    use view::settings::{
        composed::{
            self,
            input::{InputAction, InputMap, KeyId, MouseButton, Trigger},
            SettingsSource,
        },
        controller,
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let window = app.window.unwrap();
    let descriptor = composed::KEY_BINDINGS
        .into_iter()
        .find(|entry| entry.action == InputAction::Apply)
        .unwrap();
    let generation = app.settings_workbench.generation;
    app.settings_workbench.capture = composed::binding_capture::Capture::begin(
        generation,
        descriptor.path,
        0,
        &InputMap::default(),
    );
    let _ = controller::route_key(
        &mut app,
        &Key::Named(Named::F10),
        Modifiers::CTRL,
        window,
        true,
    );
    let _ = controller::route_key(
        &mut app,
        &Key::Named(Named::Enter),
        Modifiers::empty(),
        window,
        true,
    );
    assert!(app.settings_workbench.capture.is_none());
    let map = view::settings::bridge::Source { app: &app }.bindings();
    assert!(map.triggers(InputAction::Apply).iter().any(|trigger|matches!(trigger,Trigger::Key(spec)if spec.id==KeyId::Function(10)&&spec.mods.ctrl)));
    let generation = app.settings_workbench.generation;
    app.settings_workbench.capture =
        composed::binding_capture::Capture::begin(generation, descriptor.path, 0, &map);
    controller::observe_modifiers(
        &mut app,
        Modifiers::CTRL | Modifiers::SHIFT | Modifiers::LOGO,
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::KeybindCaptureClick(MouseButton::Left),
    );
    let capture = app.settings_workbench.capture.as_ref().unwrap();
    assert!(
        matches!(&capture.triggers[0],Trigger::Mouse(spec)if spec.button==MouseButton::Left&&spec.mods.ctrl&&spec.mods.shift&&spec.mods.logo)
    );
}

#[test]
fn p3_configured_navigation_key_close_precedes_generic_settings_navigation() {
    use keyboard::{key::Named, Key, Modifiers};
    for (binding, key) in [
        ("right", Named::ArrowRight),
        ("return", Named::Enter),
        ("tab", Named::Tab),
    ] {
        let mut app = p3_composed_editor_app(SettingsCategory::Picker);
        let window = app.window.unwrap();
        app.config.keybinds.close = binding.into();
        let _ = view::settings::controller::route_key(
            &mut app,
            &Key::Named(key),
            Modifiers::empty(),
            window,
            false,
        );
        assert!(!app.panel_state.settings_open, "{binding}");
        assert!(app.visible);
    }
}

#[test]
fn p3_explicit_search_close_and_escape_clear_query_and_results() {
    use view::settings::{composed::SettingsKey, folio::intent::SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    for by_escape in [false, true] {
        app.settings_search = "language".into();
        app.settings_workbench.navigation.search_open = true;
        view::settings::controller::reconcile(&mut app);
        assert!(!app.settings_workbench.results.is_empty());
        p3_settings_intent(
            &mut app,
            if by_escape {
                SettingsMsg::Key(SettingsKey::Cancel)
            } else {
                SettingsMsg::SearchClose
            },
        );
        assert!(app.settings_search.is_empty());
        assert!(app.settings_workbench.results.is_empty());
        assert!(!app.settings_workbench.navigation.search_open);
        assert!(app.panel_state.settings_open);
    }
}

#[test]
fn p3_source_hidden_geometry_projection_preserves_actual_values_and_canonical_units() {
    use view::settings::{
        bridge::Source,
        composed::{draft::Value, keys::selector as K, SettingsSource},
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.state.gallery.slices.offset_x = 0.2;
    app.state.gallery.slices.offset_y = -0.1;
    app.state.gallery.geometric.curve_strength = 1.75;
    app.state.gallery.geometric.aspect = 1.25;
    app.state.gallery.geometric.lens = 0.25;
    app.state.gallery.geometric.twist = 43.25;
    app.state.gallery.wall.selected_scale = 1.23;
    app.state.gallery.wall.flow_wave = 0.75;
    app.settings_workbench
        .draft
        .values
        .insert(K::DEPTH_HEIGHT.into(), Value::Number(999.0));
    let source = Source { app: &app };
    assert_eq!(source.number(K::SLICE_STAGE_X), 20.0);
    assert_eq!(source.number(K::SLICE_STAGE_Y), -10.0);
    assert_eq!(source.number(K::HEX_ARC_INTENSITY), 1.75);
    assert_eq!(source.number(K::HEX_ARC_INTENSITY_X10), 17.5);
    assert_eq!(source.number(K::HEX_ASPECT), 125.0);
    assert_eq!(source.number(K::HEX_LENS), 25.0);
    assert_eq!(source.number(K::HEX_TWIST), 43.25);
    assert!((source.number(K::GRID_SELECTED_SCALE) - 123.0).abs() < 0.0001);
    assert_eq!(source.number(K::GRID_FLOW_WAVE), 0.75);
    assert_eq!(
        source.text(K::DEPTH_HEIGHT),
        app.state.gallery.depth.height.to_string()
    );
    assert_eq!(source.number(K::HEX_ORBIT_RADIUS), 0.0);
}

#[test]
fn compatibility_owner_p3_process_rule_validation_keeps_invalid_manual_text_for_correction() {
    use view::settings::{composed::ActionId, folio::intent::SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::Playback);
    compatibility_owner_intent(&mut app, SettingsMsg::Run(ActionId::ChooseRunningProcess));
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::ProcessPickerManualInput("first.exe, second.exe".into()),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::ProcessPickerManualAdd);
    let picker = app.settings_workbench.process_picker.as_ref().unwrap();
    assert_eq!(picker.manual, "first.exe, second.exe");
    assert!(picker.targets.is_empty());
}

#[test]
fn p3_mode_profile_projection_has_exact_mode_scope_and_excludes_shared_flip() {
    use view::settings::{composed::keys::selector as K, profile_projection};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    for (mode, count) in [
        (GalleryMode::Slices, 18),
        (GalleryMode::Geometric, 28),
        (GalleryMode::Wall, 26),
        (GalleryMode::Depth, 12),
    ] {
        app.gallery_mode = mode;
        let before = app.state.clone();
        let snapshot = profile_projection::capture(&app);
        let values = profile_projection::source_values(&snapshot);
        assert_eq!(values.len(), count);
        let keys: std::collections::BTreeSet<_> = values.iter().map(|(key, _)| *key).collect();
        assert_eq!(keys.len(), count);
        for offset in [K::DEPTH_STAGE_X, K::DEPTH_STAGE_Y] {
            assert_eq!(keys.contains(offset), mode == GalleryMode::Depth);
        }
        for shared in [
            K::FLIP_EFFECT,
            K::FLIP_DURATION_MS,
            K::FLIP_SHADER,
            K::FLIP_BACK_REVEAL,
        ] {
            assert!(!keys.contains(shared));
        }
        app.state.card_flip_mut().duration_ms += 1;
        assert_eq!(profile_projection::capture(&app), snapshot);
        app.state = before.clone();
        assert_eq!(app.state, before);
    }
}

#[test]
fn p3_mode_profile_capture_preserves_hidden_geometry_circle_and_configured_disabled_corners() {
    use candidate_state::mode_profiles::Snapshot;
    use view::settings::{
        composed::{draft::Value, keys::selector as K},
        profile_projection,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.gallery_mode = GalleryMode::Slices;
    app.state.gallery.slices.offset_x = 0.2;
    app.state.gallery.slices.spacing = -30.0;
    app.settings_workbench
        .draft
        .values
        .insert(K::ROUND_CORNERS.into(), Value::Boolean(false));
    for (key, value) in [K::CORNER_TL, K::CORNER_TR, K::CORNER_BR, K::CORNER_BL]
        .into_iter()
        .zip([4.0, 8.0, 12.0, 16.0])
    {
        app.settings_workbench
            .draft
            .values
            .insert(key.into(), Value::Number(value));
    }
    let Snapshot::Slices(slices) = profile_projection::capture(&app) else {
        panic!("Slices")
    };
    assert_eq!(slices.offset_x, 0.2);
    assert_eq!(slices.spacing, -30.0);
    assert!(!slices.round_corners);
    assert_eq!(slices.corners, [4.0, 8.0, 12.0, 16.0]);
    app.gallery_mode = GalleryMode::Geometric;
    app.state.gallery.geometric.shape = StateGeometricShape::Circle;
    app.state.gallery.geometric.curve_strength = 1.75;
    app.state.gallery.geometric.lens = 0.35;
    app.state.gallery.geometric.twist = 47.0;
    let Snapshot::Hex(hex) = profile_projection::capture(&app) else {
        panic!("Hex")
    };
    assert_eq!(hex.shape, StateGeometricShape::Circle);
    assert_eq!(hex.curve_strength, 1.75);
    assert_eq!(hex.lens, 0.35);
    assert_eq!(hex.twist, 47.0);
    assert_eq!(hex.orbit_radius, 0.0);
    assert_eq!(hex.lens_radius, 620.0);
    assert_eq!(hex.stage.scale, 1.0);
    app.gallery_mode = GalleryMode::Wall;
    app.state.gallery.wall.selected_scale = 1.23;
    app.state.gallery.wall.flow_wave = 0.8;
    let Snapshot::Wall(wall) = profile_projection::capture(&app) else {
        panic!("Wall")
    };
    assert_eq!(wall.selected_scale, 1.23);
    assert_eq!(wall.flow_wave, 0.8);
    assert_eq!(wall.border_width, 2.0);
}

#[test]
fn p3_capture_reopen_expires_callbacks_from_the_previous_capture_overlay() {
    use view::settings::{
        composed::input::InputAction,
        controller,
        folio::intent::{Message as Intent, SettingsMsg},
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let path = view::settings::composed::KEY_BINDINGS
        .into_iter()
        .find(|entry| entry.action == InputAction::Apply)
        .unwrap()
        .path
        .to_owned();
    p3_settings_intent(&mut app, SettingsMsg::KeybindCapture(path.clone(), 0));
    let old = app.settings_workbench.generation;
    assert!(app.settings_workbench.capture.is_some());
    p3_settings_intent(&mut app, SettingsMsg::KeybindCaptureCancel);
    p3_settings_intent(&mut app, SettingsMsg::KeybindCapture(path, 0));
    let current = app
        .settings_workbench
        .capture
        .as_ref()
        .unwrap()
        .triggers
        .clone();
    assert_ne!(old, app.settings_workbench.generation);
    let _ = controller::dispatch(
        &mut app,
        old,
        Intent::Settings(SettingsMsg::KeybindCaptureUnbind),
    );
    assert_eq!(
        app.settings_workbench.capture.as_ref().unwrap().triggers,
        current
    );
}

fn p3_profile_intent(app: &mut GpuCandidate, intent: view::settings::folio::intent::Message) {
    let generation = app.settings_workbench.generation;
    let _ = view::settings::controller::dispatch(app, generation, intent);
    view::settings::controller::reconcile(app);
}

#[test]
fn p3_mode_profile_ui_save_rename_duplicates_apply_delete_target_exact_ids() {
    use candidate_state::mode_profiles::Mode;
    use view::settings::folio::intent::Message as Intent;
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let selected = app.selected;
    let target = app.active_monitor_id.clone();
    let shared = app.state.card_flip().clone();
    let slices = app.state.gallery.slices.clone();
    let hex = app.state.gallery.geometric.clone();
    let wall = app.state.gallery.wall.clone();
    app.state.gallery.depth.height = 500.0;
    p3_profile_intent(&mut app, Intent::SavePreset(GalleryKind::Depth));
    let first = app.state.mode_profiles.selected_id(Mode::Depth).unwrap();
    assert_eq!(
        app.state.mode_profiles.selected_name(Mode::Depth),
        Some("Style 1")
    );
    app.state.gallery.depth.height = 700.0;
    p3_profile_intent(&mut app, Intent::SavePreset(GalleryKind::Depth));
    let second = app.state.mode_profiles.selected_id(Mode::Depth).unwrap();
    assert_ne!(first, second);
    p3_profile_intent(
        &mut app,
        Intent::PresetNameInput {
            mode: Mode::Depth,
            id: second,
            raw: " Style 1 ".into(),
        },
    );
    assert_eq!(
        app.state
            .mode_profiles
            .rows(Mode::Depth)
            .iter()
            .map(|entry| entry.name.as_str())
            .collect::<Vec<_>>(),
        vec!["Style 1", "Style 1"]
    );
    p3_profile_intent(&mut app, Intent::ApplyPreset(Mode::Depth, first));
    assert_eq!(app.state.gallery.depth.height, 500.0);
    let old = app.settings_workbench.generation;
    p3_profile_intent(&mut app, Intent::ApplyPreset(Mode::Depth, second));
    assert_eq!(app.state.gallery.depth.height, 700.0);
    let _ = view::settings::controller::dispatch(
        &mut app,
        old,
        Intent::DeletePreset(Mode::Depth, second),
    );
    assert_eq!(app.state.mode_profiles.rows(Mode::Depth).len(), 2);
    p3_profile_intent(&mut app, Intent::DeletePreset(Mode::Depth, first));
    assert_eq!(app.state.mode_profiles.rows(Mode::Depth).len(), 1);
    assert_eq!(app.state.mode_profiles.rows(Mode::Depth)[0].id, second);
    assert_eq!(app.state.gallery.depth.height, 700.0);
    p3_profile_intent(
        &mut app,
        Intent::PresetNameInput {
            mode: Mode::Depth,
            id: second,
            raw: "  ".into(),
        },
    );
    assert_eq!(
        app.state.mode_profiles.selected_name(Mode::Depth),
        Some("Style 1")
    );
    assert_eq!(app.state.gallery.slices, slices);
    assert_eq!(app.state.gallery.geometric, hex);
    assert_eq!(app.state.gallery.wall, wall);
    assert_eq!(app.state.card_flip(), &shared);
    assert_eq!(app.selected, selected);
    assert_eq!(app.active_monitor_id, target);
}

#[test]
fn profile_depth_style_transition_reverses_and_keeps_preview_local() {
    let (mut app, submissions) = d4a_fixture(12);
    let original = app.depth_presentation.unwrap();
    let previous = app.state.gallery.depth.clone();
    app.state.gallery.depth.shadows = !original.shadows;
    app.state.gallery.depth.selection_frame = !original.selection_frame;
    app.depth_settings_changed(&previous);
    d4a_motion_tick(&mut app, 0.05);
    let mid = app.depth_presentation.unwrap();
    assert_eq!(app.depth_frame.as_ref().unwrap().shadows, mid.shadows);
    assert_eq!(
        app.depth_frame.as_ref().unwrap().selection_frame,
        mid.selection_frame
    );

    let previous_reversal = app.state.gallery.depth.clone();
    app.state.gallery.depth.shadows = original.shadows;
    app.state.gallery.depth.selection_frame = original.selection_frame;
    app.depth_settings_changed(&previous_reversal);
    for _ in 0..240 {
        if !d4a_motion_tick(&mut app, 1.0 / 60.0) {
            break;
        }
    }
    let settled = app.depth_presentation.unwrap();
    assert_eq!(settled.shadows, original.shadows);
    assert_eq!(settled.selection_frame, original.selection_frame);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn p3_mode_profile_ui_unavailable_record_can_rename_delete_without_applying() {
    use candidate_state::mode_profiles::{Mode, NamedModeProfile};
    use view::settings::{
        composed::{Control, SettingsSource},
        folio::intent::Message as Intent,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let before = app.state.gallery.clone();
    app.state
        .mode_profiles
        .depth
        .entries
        .push(NamedModeProfile {
            id: 40,
            name: "Legacy".into(),
            snapshot: None,
            legacy_snapshot: None,
        });
    app.state.mode_profiles.next_id = 41;
    view::settings::controller::reconcile(&mut app);
    let model = view::settings::controller::cards(&app);
    let row = model
        .iter()
        .flat_map(|(_, rows)| rows)
        .find_map(|row| {
            if let Control::Presets { items, .. } = &row.control {
                items.iter().find(|entry| entry.id == 40)
            } else {
                None
            }
        })
        .unwrap();
    assert!(!row.available);
    p3_profile_intent(&mut app, Intent::ApplyPreset(Mode::Depth, 40));
    assert_eq!(app.state.gallery, before);
    p3_profile_intent(&mut app, Intent::SelectPresetForRename(Mode::Depth, 40));
    p3_profile_intent(
        &mut app,
        Intent::PresetNameInput {
            mode: Mode::Depth,
            id: 40,
            raw: "Kept record".into(),
        },
    );
    assert_eq!(
        view::settings::bridge::Source { app: &app }
            .text(view::settings::composed::PRESET_NAME_KEY),
        "Kept record"
    );
    assert!(app.state.mode_profiles.snapshot(Mode::Depth, 40).is_none());
    assert_eq!(app.state.gallery, before);
    p3_profile_intent(&mut app, Intent::DeletePreset(Mode::Depth, 40));
    assert!(app.state.mode_profiles.rows(Mode::Depth).is_empty());
    assert_eq!(app.state.gallery, before);
}

#[test]
fn p4_mode_profile_apply_rejects_wrong_mode_and_preserves_fractional_current_values() {
    use candidate_state::mode_profiles::{Mode, Snapshot};
    use view::settings::{folio::intent::Message as Intent, profile_projection};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    p3_profile_intent(&mut app, Intent::SavePreset(GalleryKind::Depth));
    let id = app.state.mode_profiles.selected_id(Mode::Depth).unwrap();
    let before = app.state.clone();
    p3_profile_intent(&mut app, Intent::ApplyPreset(Mode::Slices, id));
    assert_eq!(app.state, before);
    let mut snapshot = profile_projection::capture(&app);
    let Snapshot::Depth(depth) = &mut snapshot else {
        panic!("Depth")
    };
    depth.visible_count = 5.5;
    depth.navigation_ms = 250.5;
    let proposed = profile_projection::geometry_proposal(&app, &snapshot).unwrap();
    assert_eq!(proposed.depth.visible_count, 5.5);
    assert_eq!(proposed.depth.navigation_ms, 250.5);
    assert_eq!(app.state, before);
}

#[test]
fn p4_shared_flip_actual_controls_all_modes_restart_and_profile_noninterference() {
    use view::settings::{
        bridge::Source,
        composed::{keys::selector as K, SettingsSource},
        profile_projection,
    };
    for (kind, mode) in [
        (GalleryKind::Slices, GalleryMode::Slices),
        (GalleryKind::Geometric, GalleryMode::Geometric),
        (GalleryKind::Wall, GalleryMode::Wall),
        (GalleryKind::Depth, GalleryMode::Depth),
    ] {
        let mut app = p3_composed_editor_app(SettingsCategory::Picker);
        app.state.gallery.mode = kind;
        app.gallery_mode = mode;
        let legacy = app.state.gallery.depth.detail.clone();
        let geometry = profile_projection::capture(&app);
        let _ = update(&mut app, Message::SetDepthEffect("Tonal Layers".into()));
        let _ = update(&mut app, Message::SetDepthShader(false));
        let _ = update(&mut app, Message::SetDepthBack(false));
        let _ = update(
            &mut app,
            Message::SetGalleryNumber(GalleryNumberField::DepthFlipDuration, 1234.0),
        );
        let shared = app.state.card_flip().clone();
        assert_eq!(
            (
                shared.effect.as_str(),
                shared.duration_ms,
                shared.animate_shader,
                shared.animate_back
            ),
            ("Tonal Layers", 1234, false, false)
        );
        assert_eq!(Source { app: &app }.text(K::FLIP_EFFECT), "Tonal Layers");
        assert_eq!(Source { app: &app }.number(K::FLIP_DURATION_MS), 1234.0);
        assert_eq!(app.state.gallery.depth.detail, legacy);
        assert_eq!(profile_projection::capture(&app), geometry);
        profile_projection::install(&mut app, Ok(&geometry), None)
            .1
            .unwrap();
        assert_eq!(app.state.card_flip(), &shared);
        let restarted = CandidateState::load(&app.state_path);
        assert_eq!(restarted.card_flip(), &shared);
        assert_eq!(restarted.gallery.depth.detail, legacy);
    }
}

#[test]
fn p4_shared_flip_future_primary_or_backup_refusal_keeps_live_value() {
    use std::fs;
    for backup in [false, true] {
        let mut app = p3_composed_editor_app(SettingsCategory::Picker);
        app.persist_state_outcome().unwrap();
        let before = app.state.card_flip().clone();
        let future = if backup {
            app.state_path.with_extension("toml.bak")
        } else {
            app.state_path.clone()
        };
        fs::write(&future, "version=5\n").unwrap();
        let raw = fs::read(&future).unwrap();
        let _ = update(&mut app, Message::SetDepthEffect("Tonal Layers".into()));
        assert_eq!(app.state.card_flip(), &before);
        assert_eq!(fs::read(&future).unwrap(), raw);
        assert!(app.state.write_restriction.is_some());
        assert!(app.save_failure.is_some());
    }
}

#[test]
fn p3_unavailable_profile_keyboard_uses_same_rename_delete_identity_as_pointer_rows() {
    use candidate_state::mode_profiles::NamedModeProfile;
    use view::settings::{
        composed::{profile_choices, Control, ProfileChoice, SettingsFocus, SettingsKey},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.state.mode_profiles.next_id = 81;
    app.state
        .mode_profiles
        .depth
        .entries
        .push(NamedModeProfile {
            id: 80,
            name: "Missing snapshot".into(),
            snapshot: None,
            legacy_snapshot: None,
        });
    let model = view::settings::controller::cards(&app);
    let (section, row, choices) = model
        .iter()
        .enumerate()
        .find_map(|(section, (_, rows))| {
            rows.iter()
                .enumerate()
                .find_map(|(row, item)| match &item.control {
                    Control::Presets { items, .. } => Some((section, row, profile_choices(items))),
                    _ => None,
                })
        })
        .unwrap();
    assert_eq!(
        choices,
        vec![
            ProfileChoice::Save,
            ProfileChoice::Apply(80),
            ProfileChoice::SelectForRename(80),
            ProfileChoice::Delete(80)
        ]
    );
    app.settings_workbench.navigation.section = section;
    app.settings_workbench.navigation.row = row;
    app.settings_workbench.navigation.focus = SettingsFocus::Controls;
    app.settings_workbench.navigation.choice = Some(0);
    p3_settings_intent(&mut app, SettingsMsg::Key(SettingsKey::Down));
    assert_eq!(app.settings_workbench.navigation.choice, Some(2)); // Disabled Apply skipped.
    let before = app.state.gallery.clone();
    p3_settings_intent(&mut app, SettingsMsg::Key(SettingsKey::Activate));
    assert_eq!(app.state.mode_profiles.depth.selected_id, Some(80));
    assert_eq!(app.state.gallery, before);
    app.settings_workbench.navigation.choice = Some(3);
    p3_settings_intent(&mut app, SettingsMsg::Key(SettingsKey::Activate));
    assert!(app.state.mode_profiles.depth.entries.is_empty());
    assert_eq!(app.state.gallery, before);
}

#[test]
fn p3_mode_profile_install_rejects_invalid_geometry_without_state_or_draft_publication() {
    use candidate_state::mode_profiles::Snapshot;
    use view::settings::profile_projection;
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let original = profile_projection::capture(&app);
    for invalid in [-1.0, f32::NAN, f32::INFINITY, 9000.0] {
        let mut proposal = original.clone();
        let Snapshot::Depth(depth) = &mut proposal else {
            panic!("Depth")
        };
        depth.height = invalid;
        let before = app.state.clone();
        let values = app.settings_workbench.draft.values.clone();
        assert!(profile_projection::install(&mut app, Ok(&proposal), None)
            .1
            .is_err());
        assert_eq!(app.state, before);
        assert_eq!(app.settings_workbench.draft.values, values);
    }
    let mut proposal = original;
    let Snapshot::Depth(depth) = &mut proposal else {
        panic!("Depth")
    };
    depth.visible_count = -1.0;
    assert!(profile_projection::geometry_proposal(&app, &proposal).is_err());
    app.state.gallery.mode = GalleryKind::Geometric;
    app.gallery_mode = GalleryMode::Geometric;
    let mut proposal = profile_projection::capture(&app);
    let Snapshot::Hex(hex) = &mut proposal else {
        panic!("Hex")
    };
    hex.rows = u64::MAX;
    let before = app.state.clone();
    assert!(profile_projection::install(&mut app, Ok(&proposal), None)
        .1
        .is_err());
    assert_eq!(app.state, before);
}

#[test]
fn p3_mode_profile_stale_save_cannot_capture_a_different_current_mode() {
    use view::settings::folio::intent::Message as Intent;
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let generation = app.settings_workbench.generation;
    app.state.gallery.mode = GalleryKind::Slices;
    app.gallery_mode = GalleryMode::Slices;
    let _ = view::settings::controller::dispatch(
        &mut app,
        generation,
        Intent::SavePreset(GalleryKind::Depth),
    );
    assert!(app.state.mode_profiles.slices.entries.is_empty());
    assert!(app.state.mode_profiles.depth.entries.is_empty());
}

#[test]
fn p3_exact_mode_choices_route_to_existing_geometry_owners() {
    use view::settings::{
        composed::keys::{motion as M, selector as K},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick(K::DISPLAY_MODE.into(), "hex".into()),
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick(K::HEX_CURVE.into(), "wave".into()),
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick(K::HEX_SHAPE.into(), "diamond".into()),
    );
    assert_eq!(app.state.gallery.geometric.curve, StateGeometricCurve::Wave);
    assert_eq!(
        app.state.gallery.geometric.shape,
        StateGeometricShape::Diamond
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick(K::DISPLAY_MODE.into(), "wall".into()),
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick(K::GRID_LAYOUT.into(), "cylinder".into()),
    );
    assert_eq!(app.state.gallery.wall.layout, StateWallLayout::Cylinder);
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick(M::FILTER_SWAP_SPEED.into(), "fast".into()),
    );
    // The composed Picker intentionally moves this control into Motion.
    // A callback from a hidden category must be rejected by the real guard.
    assert_eq!(
        app.state.gallery_motion_speed,
        candidate_state::GalleryMotionSpeed::Slow
    );
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("motion".into()));
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick(M::FILTER_SWAP_SPEED.into(), "fast".into()),
    );
    assert_eq!(
        app.state.gallery_motion_speed,
        candidate_state::GalleryMotionSpeed::Fast
    );
}

#[test]
fn p3_actual_profile_apply_restores_each_mode_without_cross_mode_or_shared_setting_changes() {
    use view::settings::folio::intent::Message as Intent;
    for mode in [
        GalleryKind::Slices,
        GalleryKind::Geometric,
        GalleryKind::Wall,
        GalleryKind::Depth,
    ] {
        let mut app = p3_composed_editor_app(SettingsCategory::Picker);
        app.state.gallery.mode = mode;
        app.gallery_mode = match mode {
            GalleryKind::Slices => GalleryMode::Slices,
            GalleryKind::Geometric => GalleryMode::Geometric,
            GalleryKind::Wall => GalleryMode::Wall,
            GalleryKind::Depth => GalleryMode::Depth,
        };
        let mut saved = app.state.gallery.clone();
        p3_profile_intent(&mut app, Intent::SavePreset(mode));
        let id = app.state.mode_profiles.selected_id(mode).unwrap();
        // Source-only fields now share the durable current mode owner. The
        // expected legacy geometry and every other mode remain unchanged.
        use candidate_state::mode_profiles::Snapshot;
        match app.state.mode_profiles.snapshot(mode, id).unwrap() {
            Snapshot::Slices(g) => {
                saved.slices.shadows = Some(g.shadows);
                saved.slices.parallax = Some(g.parallax);
                saved.slices.round_corners = Some(g.round_corners);
                saved.slices.configured_corners = Some(g.corners);
            }
            Snapshot::Hex(g) => {
                saved.geometric.parallax = Some(g.parallax);
                saved.geometric.scroll_step =
                    Some(candidate_state::HexScrollStep::from_profile(g.scroll_step).unwrap());
                saved.geometric.arc = Some(g.arc);
                saved.geometric.lens_radius = Some(g.lens_radius);
                saved.geometric.stage = Some(g.stage.clone());
            }
            Snapshot::Wall(g) => {
                saved.wall.round_corners = Some(g.round_corners);
                saved.wall.border_width = Some(g.border_width);
                saved.wall.stage = Some(g.stage.clone());
            }
            Snapshot::Depth(_) => {}
        }
        app.state.card_flip_mut().duration_ms += 17;
        let shared_after_edit = app.state.card_flip().clone();
        match mode {
            GalleryKind::Slices => {
                app.state.gallery.slices.width += 35.0;
                app.state.gallery.wall.card_width += 24.0;
            }
            GalleryKind::Geometric => {
                app.state.gallery.geometric.radius += 35.0;
                app.state.gallery.slices.width += 24.0;
            }
            GalleryKind::Wall => {
                app.state.gallery.wall.card_width += 35.0;
                app.state.gallery.geometric.radius += 24.0;
            }
            GalleryKind::Depth => {
                app.state.gallery.depth.height += 35.0;
                app.state.gallery.wall.card_width += 24.0;
            }
        }
        let mut expected = app.state.gallery.clone();
        match mode {
            GalleryKind::Slices => expected.slices = saved.slices,
            GalleryKind::Geometric => expected.geometric = saved.geometric,
            GalleryKind::Wall => expected.wall = saved.wall,
            GalleryKind::Depth => {
                let shared = expected.depth.detail.clone();
                expected.depth = saved.depth;
                expected.depth.detail = shared;
            }
        }
        expected.variant = None;
        let selected = app.selected;
        let target = app.active_monitor_id.clone();
        p3_profile_intent(&mut app, Intent::ApplyPreset(mode, id));
        assert_eq!(app.state.gallery, expected, "{mode:?}");
        assert_eq!(app.state.card_flip(), &shared_after_edit);
        assert_eq!(app.state.mode_profiles.selected_id(mode), Some(id));
        assert_eq!(app.selected, selected);
        assert_eq!(app.active_monitor_id, target);
        assert_eq!(app.state.gallery.mode, mode);
    }
}

#[test]
fn p3_mode_profile_name_buffer_is_owned_by_exact_mode_and_selected_id() {
    use candidate_state::mode_profiles::Mode;
    use view::settings::{
        composed::{keys::selector as K, SettingsSource},
        folio::intent::{Message as Intent, SettingsMsg},
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    p3_profile_intent(&mut app, Intent::SavePreset(Mode::Depth));
    let depth = app.state.mode_profiles.selected_id(Mode::Depth).unwrap();
    p3_profile_intent(
        &mut app,
        Intent::PresetNameInput {
            mode: Mode::Depth,
            id: depth,
            raw: " Depth raw ".into(),
        },
    );
    assert_eq!(
        view::settings::bridge::Source { app: &app }
            .text(view::settings::composed::PRESET_NAME_KEY),
        " Depth raw "
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick(K::DISPLAY_MODE.into(), "hex".into()),
    );
    assert_eq!(
        view::settings::bridge::Source { app: &app }
            .text(view::settings::composed::PRESET_NAME_KEY),
        ""
    );
    p3_profile_intent(&mut app, Intent::SavePreset(Mode::Geometric));
    let hex = app
        .state
        .mode_profiles
        .selected_id(Mode::Geometric)
        .unwrap();
    p3_profile_intent(
        &mut app,
        Intent::PresetNameInput {
            mode: Mode::Geometric,
            id: hex,
            raw: "Hex label".into(),
        },
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick(K::DISPLAY_MODE.into(), "depth".into()),
    );
    assert_eq!(
        view::settings::bridge::Source { app: &app }
            .text(view::settings::composed::PRESET_NAME_KEY),
        "Depth raw"
    );
    let generation = app.settings_workbench.generation;
    let _ = view::settings::controller::dispatch(
        &mut app,
        generation,
        Intent::PresetNameInput {
            mode: Mode::Geometric,
            id: hex,
            raw: "Wrong record".into(),
        },
    );
    assert_eq!(
        app.state.mode_profiles.selected_name(Mode::Geometric),
        Some("Hex label")
    );
    assert_eq!(
        view::settings::bridge::Source { app: &app }
            .text(view::settings::composed::PRESET_NAME_KEY),
        "Depth raw"
    );
}

#[test]
fn p3_root_settings_gateway_rejects_stale_window_generation_and_inactive_child_messages() {
    use view::settings::folio::intent::{Message as Intent, SettingsMsg};
    let (mut app, _) = d3_application_fixture(2);
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    assert!(app.settings_workbench.active);
    let window = app.window;
    let generation = app.settings_workbench.generation;
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation,
            intent: Intent::Settings(SettingsMsg::SelectTab("language".into())),
        },
    );
    assert_eq!(app.settings_workbench.tab, "language");
    let current = app.settings_workbench.generation;
    assert_ne!(current, generation);
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation,
            intent: Intent::Settings(SettingsMsg::SelectTab("picker".into())),
        },
    );
    assert_eq!(app.settings_workbench.tab, "language");
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window: Some(window::Id::unique()),
            generation: current,
            intent: Intent::ToggleSettings,
        },
    );
    assert!(app.panel_state.settings_open);
    let _ = update(
        &mut app,
        Message::SettingsPageAction {
            window,
            generation: current,
            message: Box::new(Message::ScheduleNameChanged("inactive child".into())),
        },
    );
    assert!(app.schedule_name.is_empty());
    let _ = update(
        &mut app,
        Message::SettingsOpenPage {
            window,
            generation: current,
            category: SettingsCategory::Automation,
        },
    );
    let child = app.settings_workbench.generation;
    let _ = update(
        &mut app,
        Message::SettingsPageAction {
            window,
            generation: child,
            message: Box::new(Message::ScheduleNameChanged("local schedule".into())),
        },
    );
    assert_eq!(app.schedule_name, "local schedule");
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation: child,
            intent: Intent::Settings(SettingsMsg::SelectTab("language".into())),
        },
    );
    assert!(app.settings_workbench.windows_panel.is_none());
    assert!(app.panel_state.settings_open);
    let _ = update(
        &mut app,
        Message::SettingsPageAction {
            window,
            generation: child,
            message: Box::new(Message::ScheduleNameChanged("stale".into())),
        },
    );
    assert_eq!(app.schedule_name, "local schedule");
}

#[test]
fn p3_root_settings_keys_keep_local_search_modifiers_and_gallery_search_departure() {
    let (mut app, _) = d3_application_fixture(2);
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    let window = app.window.unwrap();
    let _ = update(
        &mut app,
        Message::SettingsCapturedKey {
            window,
            key: keyboard::Key::Character("f".into()),
            modifiers: keyboard::Modifiers::CTRL,
        },
    );
    assert!(app.settings_workbench.navigation.search_open);
    assert!(!app.panel_state.search_open);
    let _ = update(
        &mut app,
        Message::SettingsModifiers {
            window,
            modifiers: keyboard::Modifiers::CTRL | keyboard::Modifiers::SHIFT,
        },
    );
    assert!(app.settings_workbench.capture_modifiers.ctrl);
    assert!(app.settings_workbench.capture_modifiers.shift);
    let _ = update(
        &mut app,
        Message::SettingsModifiers {
            window: window::Id::unique(),
            modifiers: keyboard::Modifiers::empty(),
        },
    );
    assert!(app.settings_workbench.capture_modifiers.ctrl);
    let old = app.settings_workbench.generation;
    let _ = update(&mut app, Message::OpenSearch);
    assert!(app.settings_workbench.active);
    assert!(!app.panel_state.search_open);
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    let _ = update(&mut app, Message::OpenSearch);
    assert!(!app.settings_workbench.active);
    assert!(app.panel_state.search_open);
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window: Some(window),
            generation: old,
            intent: view::settings::folio::intent::Message::ToggleSettings,
        },
    );
    assert!(!app.panel_state.settings_open);
    assert!(app.panel_state.search_open);
}

#[test]
fn p3_root_theme_child_departure_invalidates_session_and_keeps_settings_usable() {
    let mut app = theme_open_app();
    let _ = update(&mut app, Message::SettingsSearchChanged(String::new()));
    assert!(lifecycle::theme_owner_active(&app));
    assert_eq!(
        app.settings_workbench.windows_panel,
        Some(SettingsCategory::Theme)
    );
    let window = app.window;
    let generation = app.settings_workbench.generation;
    let _ = update(&mut app, Message::SettingsThemeClose { window, generation });
    assert!(!lifecycle::theme_owner_active(&app));
    assert!(app.panel_state.settings_open);
    let generation = app.settings_workbench.generation;
    let _ = update(
        &mut app,
        Message::SettingsOpenPage {
            window,
            generation,
            category: SettingsCategory::Theme,
        },
    );
    assert!(lifecycle::theme_owner_active(&app));
    let element = view(&app, app.window.unwrap());
    assert!(!iced::advanced::widget::Tree::new(&element)
        .children
        .is_empty());
}

#[test]
fn p3_root_all_modes_block_gallery_wheel_under_each_sheet_and_resume_after_close() {
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
        GalleryMode::Depth,
    ] {
        for owner in 0..6 {
            // Wall scrolling needs content taller than its configured frame.
            let (mut app, submissions) = d3_application_fixture(40);
            app.set_gallery_mode(mode);
            app.panel_state.settings_open = owner == 0;
            app.panel_state.search_open = owner == 1;
            app.playlist_panel_open = owner == 2;
            app.engine_properties_open = owner == 3;
            app.controls_panel_open = owner == 4;
            app.folder_menu_open = owner == 5;
            let selected = app.selected;
            let offset = app.wall_scroll_offset;
            let wall_camera = app.wall_presentation.as_ref().map(|p| p.camera.target);
            let wheel = app.depth_wheel;
            let _ = update(
                &mut app,
                Message::Gallery(GalleryMessage::Wheel(-2.0, None)),
            );
            assert_eq!(
                (app.selected, app.wall_scroll_offset, app.depth_wheel),
                (selected, offset, wheel),
                "{mode:?} owner={owner}"
            );
            assert_eq!(
                app.wall_presentation.as_ref().map(|p| p.camera.target),
                wall_camera
            );
            app.panel_state = PanelState::default();
            app.playlist_panel_open = false;
            app.engine_properties_open = false;
            app.controls_panel_open = false;
            app.folder_menu_open = false;
            let _ = update(
                &mut app,
                Message::Gallery(GalleryMessage::Wheel(-2.0, None)),
            );
            assert!(
                app.selected != selected
                    || app.wall_presentation.as_ref().map(|p| p.camera.target) != wall_camera,
                "{mode:?} wheel must resume"
            );
            assert_eq!(submissions.load(Ordering::Acquire), 0);
        }
    }
}

#[test]
fn p3_root_studio_exposed_card_and_backdrop_close_one_owner_without_selection_or_hide() {
    for backdrop in [false, true] {
        for child in [false, true] {
            let (mut app, submissions) = d3_application_fixture(3);
            let _ = update(&mut app, Message::ToggleSettingsPanel);
            if child {
                let generation = app.settings_workbench.generation;
                let window = app.window;
                let _ = update(
                    &mut app,
                    Message::SettingsOpenPage {
                        window,
                        generation,
                        category: SettingsCategory::Theme,
                    },
                );
            }
            let selected = app.selected;
            let detail = app.detail_open;
            let message = if backdrop {
                Message::Gallery(GalleryMessage::BackdropClicked(None))
            } else {
                Message::CardClicked {
                    index: 2,
                    mouse_sequence: None,
                }
            };
            let _ = update(&mut app, message);
            assert_eq!(app.panel_state.settings_open, child);
            assert!(app.settings_workbench.windows_panel.is_none());
            assert!(app.visible);
            assert_eq!(app.selected, selected);
            assert_eq!(app.detail_open, detail);
            assert_eq!(submissions.load(Ordering::Acquire), 0);
        }
    }
}

#[test]
fn p3_root_settings_hides_and_restores_latent_gallery_search_without_clearing_other_owners() {
    let (mut app, _) = d3_application_fixture(3);
    let _ = update(&mut app, Message::OpenSearch);
    app.filter_rule.text = "sunset".into();
    app.playlist_panel_open = true;
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    assert!(
        app.panel_state.settings_open && app.panel_state.search_open && app.playlist_panel_open
    );
    assert_eq!(app.filter_rule.text, "sunset");
    let generation = app.settings_workbench.generation;
    let window = app.window;
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation,
            intent: view::settings::folio::intent::Message::ToggleSettings,
        },
    );
    assert!(
        app.panel_state.settings_open,
        "underlying stale callbacks must be blocked"
    );
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    assert!(!app.panel_state.settings_open);
    assert!(app.panel_state.search_open && app.playlist_panel_open);
    assert_eq!(app.filter_rule.text, "sunset");
}

#[test]
fn p3_root_captured_tab_respects_folders_and_slash_search_respects_text_capture() {
    let (mut app, _) = d3_application_fixture(2);
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    let window = app.window.unwrap();
    app.folder_menu_open = true;
    let focus = app.settings_workbench.navigation.focus;
    let _ = update(
        &mut app,
        Message::SettingsCapturedKey {
            window,
            key: keyboard::Key::Named(keyboard::key::Named::Tab),
            modifiers: keyboard::Modifiers::empty(),
        },
    );
    assert_ne!(
        app.settings_workbench.navigation.focus, focus,
        "hidden folder state must not block Settings"
    );
    let window_id = app.window;
    let generation = app.settings_workbench.generation;
    let _ = update(
        &mut app,
        Message::SettingsOpenPage {
            window: window_id,
            generation,
            category: SettingsCategory::Library,
        },
    );
    let focus = app.settings_workbench.navigation.focus;
    let _ = update(
        &mut app,
        Message::SettingsCapturedKey {
            window,
            key: keyboard::Key::Named(keyboard::key::Named::Tab),
            modifiers: keyboard::Modifiers::empty(),
        },
    );
    assert_eq!(
        app.settings_workbench.navigation.focus, focus,
        "visible Library folder owns input"
    );
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.folder_menu_open);
    assert_eq!(app.settings_workbench.tab, "library");
    view::settings::gateway::close(&mut app);
    app.folder_menu_open = false;
    let _ = view::settings::gateway::route_key(
        &mut app,
        &keyboard::Key::Character("/".into()),
        keyboard::Modifiers::empty(),
        window,
        true,
    );
    assert!(!app.settings_workbench.navigation.search_open);
    let _ = view::settings::gateway::route_key(
        &mut app,
        &keyboard::Key::Character("/".into()),
        keyboard::Modifiers::empty(),
        window,
        false,
    );
    assert!(app.settings_workbench.navigation.search_open);
    assert!(!app.panel_state.search_open);
}

#[test]
fn p3_root_departure_keeps_invalid_buffer_reachable_and_source_tab_on_reopen() {
    use view::settings::{composed::keys, folio::intent::SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let path = keys::selector::DEPTH_HEIGHT.to_string();
    let original = app.state.gallery.depth.height;
    p3_settings_intent(&mut app, SettingsMsg::Input(path.clone(), "700".into()));
    p3_settings_intent(&mut app, SettingsMsg::Input(path.clone(), "-".into()));
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    assert_eq!(
        app.settings_workbench.navigation.input_edit.as_deref(),
        Some(path.as_str())
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::Key(view::settings::composed::SettingsKey::Cancel),
    );
    assert_eq!(app.state.gallery.depth.height, original);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("performance".into()));
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    assert_eq!(app.settings_workbench.tab, "performance");
}

/// Explicit hardware fixture: renders the actual root view without a window,
/// hooks, provider, timers or a display/DPI change. Ordinary suites stay offline.
#[test]
#[ignore = "Owned P3 WGPU layout capture; requires P3_LAYOUT_OUTPUT and explicit invocation"]
fn p3_owned_production_layout_captures_1080p_1440p_locales_and_studio() {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout,
    };
    let output = std::path::PathBuf::from(
        std::env::var_os("P3_LAYOUT_OUTPUT").expect("owned output required"),
    );
    assert!(output.is_absolute());
    std::fs::create_dir_all(&output).unwrap();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .expect("headless WGPU adapter required");
    let (mut app, submissions) = d3_application_fixture(3);
    {
        let mut system = iced::advanced::graphics::text::font_system()
            .write()
            .unwrap();
        system.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
        system.load_font(std::borrow::Cow::Borrowed(
            include_bytes!("../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
        ));
    }
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    let mut captures = Vec::new();
    for locale in ["en-US", "fr-FR", "ar-SA", "ja-JP"] {
        let next = localization::Catalog::new(locale, locale).unwrap();
        localization::reload_ui_font(&app.settings_locale, &next).unwrap();
        app.settings_locale = next;
        for [width, height] in [[1920u32, 1080u32], [2560, 1440]] {
            app.viewport = [width as f32, height as f32];
            for tab in [
                "picker",
                "filter",
                "position",
                "displays",
                "motion",
                "playback",
                "performance",
                "library",
                "sources",
                "automation",
                "theme",
                "integrations",
                "language",
                "studio",
            ] {
                p3_settings_intent(
                    &mut app,
                    view::settings::folio::intent::SettingsMsg::SelectTab(
                        if tab == "studio" { "picker" } else { tab }.into(),
                    ),
                );
                if tab == "studio" {
                    p3_settings_intent(
                        &mut app,
                        view::settings::folio::intent::SettingsMsg::SelectSection(1),
                    );
                }
                // Settle the real app-owned tweens through the guarded root frame route.
                let origin = std::time::Instant::now();
                for frame in 0..=50 {
                    let window = app.window;
                    let generation = app.settings_workbench.generation;
                    let _ = update(
                        &mut app,
                        Message::SettingsIntent {
                            window,
                            generation,
                            intent: view::settings::folio::intent::Message::Frame(
                                origin + std::time::Duration::from_millis(frame * 50),
                            ),
                        },
                    );
                }
                assert!(app
                    .settings_workbench
                    .folio_entrance
                    .as_ref()
                    .unwrap()
                    .settled());
                assert!(app
                    .settings_workbench
                    .tab_reveal
                    .as_ref()
                    .unwrap()
                    .settled());
                let window = app.window.unwrap();
                let theme = theme(&app, window);
                let background = application_style(&app, &theme).background_color;
                let mut element = view(&app, window);
                let mut tree = Tree::new(&element);
                let size = iced::Size::new(width as f32, height as f32);
                let node = element.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(size, size),
                );
                assert_eq!(node.size(), size);
                fn finite(node: &layout::Node) {
                    let b = node.bounds();
                    assert!([b.x, b.y, b.width, b.height]
                        .into_iter()
                        .all(f32::is_finite));
                    for child in node.children() {
                        finite(child)
                    }
                }
                finite(&node);
                let bounds = iced::Rectangle::with_size(size);
                renderer.reset(bounds);
                element.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &theme,
                    &iced::advanced::renderer::Style::default(),
                    Layout::new(&node),
                    iced::mouse::Cursor::Unavailable,
                    &bounds,
                );
                let bytes = Headless::screenshot(
                    &mut renderer,
                    iced::Size::new(width, height),
                    1.0,
                    background,
                );
                assert_eq!(bytes.len(), width as usize * height as usize * 4);
                assert!(
                    bytes.chunks_exact(4).any(|pixel| pixel != &bytes[..4]),
                    "blank {locale}/{tab}"
                );
                let path = output.join(format!("{locale}-{width}x{height}-{tab}.png"));
                ::image::save_buffer_with_format(
                    &path,
                    &bytes,
                    width,
                    height,
                    ::image::ColorType::Rgba8,
                    ::image::ImageFormat::Png,
                )
                .unwrap();
                captures.push(serde_json::json!({"locale":locale,"tab":tab,"width":width,"height":height,"scaleFactor":1.0,"path":path,"rootBounds":[node.bounds().x,node.bounds().y,node.bounds().width,node.bounds().height]}));
            }
        }
    }
    assert_eq!(submissions.load(Ordering::Acquire), 0);
    assert_eq!(captures.len(), 112);
    std::fs::write(
        output.join("captures.json"),
        serde_json::to_vec_pretty(&captures).unwrap(),
    )
    .unwrap();
}

#[test]
fn p3_root_visible_sources_folder_accepts_own_callback_and_blocks_underlying_child() {
    let (mut app, _) = d3_application_fixture(3);
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    let window = app.window;
    let generation = app.settings_workbench.generation;
    let _ = update(
        &mut app,
        Message::SettingsOpenPage {
            window,
            generation,
            category: SettingsCategory::Library,
        },
    );
    app.folder_menu_open = true;
    let generation = app.settings_workbench.generation;
    let _ = update(
        &mut app,
        Message::SettingsPageAction {
            window,
            generation,
            message: Box::new(Message::ScheduleNameChanged("blocked".into())),
        },
    );
    assert!(app.schedule_name.is_empty());
    assert!(app.folder_menu_open);
    let _ = update(
        &mut app,
        Message::SettingsPageAction {
            window,
            generation,
            message: Box::new(Message::SetFolderFilter(None)),
        },
    );
    assert!(!app.folder_menu_open);
    assert!(app.panel_state.settings_open);
    assert_eq!(app.settings_workbench.tab, "library");
    app.folder_menu_open = true;
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation,
            intent: view::settings::folio::intent::Message::Backdrop,
        },
    );
    assert!(!app.folder_menu_open);
    assert!(app.panel_state.settings_open);
}

#[test]
fn p3_root_stale_keys_and_latent_search_cannot_dismiss_current_visible_owner() {
    fn key(app: &mut GpuCandidate, event_window: window::Id, named: keyboard::key::Named) {
        let _ = update(
            app,
            Message::KeyPressed {
                key: keyboard::Key::Named(named),
                modifiers: keyboard::Modifiers::empty(),
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
    }
    let (mut app, _) = d3_application_fixture(2);
    let _ = update(&mut app, Message::OpenSearch);
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    app.engine_properties_open = true;
    let current = app.window.unwrap();
    app.config.keybinds.close = "F10".into();
    key(&mut app, window::Id::unique(), keyboard::key::Named::Escape);
    key(&mut app, window::Id::unique(), keyboard::key::Named::F10);
    assert!(
        app.engine_properties_open && app.panel_state.settings_open && app.panel_state.search_open
    );
    key(&mut app, current, keyboard::key::Named::Escape);
    assert!(!app.engine_properties_open);
    assert!(app.panel_state.settings_open && app.panel_state.search_open);
    key(&mut app, current, keyboard::key::Named::F10);
    assert!(!app.panel_state.settings_open);
    assert!(app.panel_state.search_open && app.visible);
    key(&mut app, current, keyboard::key::Named::Escape);
    assert!(!app.panel_state.search_open);
    assert!(app.visible);
}

#[test]
fn p3_root_restored_search_escape_closes_playlist_before_search_and_keeps_latent_controls_usable() {
    let (mut app, _) = d3_application_fixture(3);
    let _ = update(&mut app, Message::OpenSearch);
    app.playlist_panel_open = true;
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.playlist_panel_open);
    assert!(app.panel_state.search_open && app.visible);
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.panel_state.search_open);
    assert!(app.visible);
    app.controls_panel_open = true;
    app.folder_menu_open = true;
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    let window = app.window;
    let generation = app.settings_workbench.generation;
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation,
            intent: view::settings::folio::intent::Message::Settings(
                view::settings::folio::intent::SettingsMsg::SelectTab("language".into()),
            ),
        },
    );
    assert_eq!(app.settings_workbench.tab, "language");
    assert!(app.controls_panel_open && app.folder_menu_open);
    let selected = app.selected;
    let _ = update(
        &mut app,
        Message::CardClicked {
            index: 2,
            mouse_sequence: None,
        },
    );
    assert!(!app.panel_state.settings_open);
    assert!(app.controls_panel_open && app.folder_menu_open);
    assert_eq!(app.selected, selected);
    assert!(app.visible);
}

#[test]
fn p3_f32_geometry_readout_is_short_without_changing_state_or_local_f64_precision() {
    use view::settings::{
        bridge::Source,
        composed::{draft::Value, keys, SettingsSource},
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let before = app.state.gallery.clone();
    let path = keys::selector::DEPTH_FALLOFF_FACTOR;
    assert_eq!(Source { app: &app }.text(path), "0.05");
    assert_eq!(
        Source { app: &app }.number(path),
        f64::from(before.depth.falloff)
    );
    assert_eq!(app.state.gallery, before);
    let value = 1.234567890123_f64;
    app.settings_workbench
        .draft
        .values
        .insert(keys::general::UI_SCALE.into(), Value::Number(value));
    assert_eq!(
        Source { app: &app }.text(keys::general::UI_SCALE),
        value.to_string()
    );
    assert_eq!(Source { app: &app }.number(keys::general::UI_SCALE), value);
}

#[test]
fn p3_folio_motion_uses_existing_root_frame_owner_and_source_tiers_on_open_tab_and_reopen() {
    use view::settings::{
        bridge::Source,
        composed::SettingsSource,
        folio::intent::{Message as Intent, SettingsMsg},
    };
    fn frame(app: &mut GpuCandidate, now: std::time::Instant, generation: u64) {
        let window = app.window;
        let _ = update(
            app,
            Message::SettingsIntent {
                window,
                generation,
                intent: Intent::Frame(now),
            },
        );
    }
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    let generation = app.settings_workbench.generation;
    assert_eq!(app.settings_workbench.folio_entrance.unwrap().x, 0.0);
    assert_eq!(
        app.settings_workbench.folio_entrance.unwrap().duration_ms(),
        Source { app: &app }.motion_standard_ms()
    );
    let now = std::time::Instant::now();
    frame(&mut app, now, generation);
    frame(
        &mut app,
        now + std::time::Duration::from_millis(40),
        generation,
    );
    let x = app.settings_workbench.folio_entrance.unwrap().x;
    assert!(x > 0.0 && x < 1.0);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("language".into()));
    let next = app.settings_workbench.generation;
    assert_eq!(app.settings_workbench.tab_reveal.unwrap().x, 0.0);
    assert_eq!(
        app.settings_workbench.tab_reveal.unwrap().duration_ms(),
        Source { app: &app }.motion_fast_ms()
    );
    frame(
        &mut app,
        now + std::time::Duration::from_millis(80),
        generation,
    );
    assert_eq!(
        app.settings_workbench.tab_reveal.unwrap().x,
        0.0,
        "stale frame must not animate new tab"
    );
    frame(&mut app, now + std::time::Duration::from_millis(80), next);
    frame(&mut app, now + std::time::Duration::from_millis(120), next);
    let tab_x = app.settings_workbench.tab_reveal.unwrap().x;
    assert!(tab_x > 0.0 && tab_x < 1.0);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("language".into()));
    assert_eq!(
        app.settings_workbench.tab_reveal.unwrap().x,
        tab_x,
        "same canonical tab must not restart reveal"
    );
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    assert_eq!(app.settings_workbench.folio_entrance.unwrap().x, 0.0);
    assert_eq!(app.settings_workbench.tab, "language");
    let generation = app.settings_workbench.generation;
    for step in 0..=50 {
        frame(
            &mut app,
            now + std::time::Duration::from_millis(200 + step * 50),
            generation,
        )
    }
    assert!(app.settings_workbench.folio_entrance.unwrap().settled());
    assert!(app.settings_workbench.tab_reveal.unwrap().settled());
}

#[test]
fn p3_root_folio_frames_settle_beneath_higher_input_owners_and_stop_when_hidden() {
    use view::settings::folio::intent::Message as Intent;
    for playlist in [false, true] {
        let (mut app, submissions) = d3_application_fixture(3);
        let _ = update(&mut app, Message::ToggleSettingsPanel);
        app.playlist_panel_open = playlist;
        app.engine_properties_open = !playlist;
        assert!(view::settings::bridge::wants_frames(&app));
        let window = app.window;
        let generation = app.settings_workbench.generation;
        let before = app.state.gallery.clone();
        let _ = update(
            &mut app,
            Message::SettingsIntent {
                window,
                generation,
                intent: Intent::ToggleSettings,
            },
        );
        assert!(
            app.panel_state.settings_open,
            "higher owner still blocks interactive callbacks"
        );
        let now = std::time::Instant::now();
        for step in 0..=50 {
            let _ = update(
                &mut app,
                Message::SettingsIntent {
                    window,
                    generation,
                    intent: Intent::Frame(now + std::time::Duration::from_millis(step * 50)),
                },
            );
        }
        assert!(app.settings_workbench.folio_entrance.unwrap().settled());
        assert!(!view::settings::bridge::wants_frames(&app));
        assert_eq!(app.state.gallery, before);
        assert_eq!(submissions.load(Ordering::Acquire), 0);
        app.settings_workbench
            .folio_entrance
            .as_mut()
            .unwrap()
            .run(0.0, 1.0);
        assert!(view::settings::bridge::wants_frames(&app));
        app.visible = false;
        assert!(!view::settings::bridge::wants_frames(&app));
        let _ = update(
            &mut app,
            Message::SettingsIntent {
                window,
                generation,
                intent: Intent::Frame(now + std::time::Duration::from_secs(10)),
            },
        );
        assert_eq!(app.settings_workbench.folio_entrance.unwrap().x, 0.0);
    }
}

#[test]
fn p3_depth_actual_pointer_route_dismisses_studio_only_for_current_left_click() {
    let (mut app, submissions) = d3_application_fixture(3);
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    p3_settings_intent(
        &mut app,
        view::settings::folio::intent::SettingsMsg::SelectSection(1),
    );
    let selected = app.selected;
    let detail = app.detail_open;
    for (stale, right, hover) in [
        (true, false, false),
        (false, true, false),
        (false, false, true),
    ] {
        let revision = app.depth_frame.as_ref().unwrap().revision;
        let _ = update(
            &mut app,
            Message::Gallery(GalleryMessage::DepthPointer {
                revision: if stale {
                    revision.wrapping_add(1)
                } else {
                    revision
                },
                item_id: None,
                right,
                hover,
            }),
        );
        assert!(app.panel_state.settings_open);
        assert_eq!(app.selected, selected);
        assert_eq!(app.detail_open, detail);
    }
    let revision = app.depth_frame.as_ref().unwrap().revision;
    let _ = update(
        &mut app,
        Message::Gallery(GalleryMessage::DepthPointer {
            revision,
            item_id: None,
            right: false,
            hover: false,
        }),
    );
    assert!(!app.panel_state.settings_open);
    assert!(app.visible);
    assert_eq!(app.selected, selected);
    assert_eq!(app.detail_open, detail);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn p3_root_literal_escape_and_custom_close_keep_distinct_visible_search_folder_order() {
    let (mut app, _) = d3_application_fixture(3);
    app.config.keybinds.close = "F10".into();
    let _ = update(&mut app, Message::OpenSearch);
    let _ = update(&mut app, Message::ToggleFolderMenu);
    assert!(app.panel_state.search_open && app.folder_menu_open);
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.panel_state.search_open);
    assert!(app.folder_menu_open && app.visible);
    let _ = update(&mut app, Message::ToggleFolderMenu);
    let _ = update(&mut app, Message::OpenSearch);
    let _ = update(&mut app, Message::ToggleFolderMenu);
    d3_key(&mut app, keyboard::key::Named::F10);
    assert!(!app.folder_menu_open);
    assert!(app.panel_state.search_open && app.visible);
    app.controls_panel_open = true;
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.panel_state.search_open);
    assert!(app.controls_panel_open && app.visible);
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.controls_panel_open);
    assert!(app.visible);
}

#[test]
fn p3_folio_stash_uses_existing_lifetime_owner_and_rejects_stale_generation() {
    use view::settings::{
        composed::SettingsSource,
        folio::intent::{Message as Intent, SettingsMsg},
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("language".into()));
    let window = app.window;
    let generation = app.settings_workbench.generation;
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation: generation.wrapping_sub(1),
            intent: Intent::Stash,
        },
    );
    assert!(app.visible);
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation,
            intent: Intent::Stash,
        },
    );
    assert!(!app.visible);
    assert!(!app.exit_requested);
    assert!(app.panel_state.settings_open);
    assert_eq!(app.settings_workbench.tab, "language");
    assert!(!view::settings::bridge::wants_frames(&app));
    let _ = app.set_visible(true, "p3-stash-reopen");
    view::settings::controller::reconcile(&mut app);
    assert!(app.visible && app.settings_workbench.active);
    assert_eq!(app.settings_workbench.tab, "language");
    assert_ne!(app.settings_workbench.generation, generation);
    app.launch_policy = p1_preview_policy();
    let window = app.window;
    let generation = app.settings_workbench.generation;
    assert!(!(view::settings::bridge::Source { app: &app }).stash_available());
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation,
            intent: Intent::Stash,
        },
    );
    assert!(
        !app.exit_requested && app.visible,
        "disabled Preview Hide must neither exit nor hide"
    );
    assert!(app.panel_state.settings_open);
    assert_eq!(app.settings_workbench.tab, "language");
    assert_eq!(
        app.settings_workbench.message,
        "Hide unavailable in preview"
    );
}

#[test]
fn p4_language_page_exposes_successful_publication_persistence_in_every_declared_locale() {
    use view::settings::{bridge, composed::SettingsSource};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    for language in localization::LANGUAGES {
        app.settings_locale = localization::Catalog::new(language.tag, language.tag).unwrap();
        let source = bridge::Source { app: &app };
        let cards = bridge::build(&app, "language");
        assert_eq!(cards.len(), 1);
        assert_eq!(
            cards[0].0.subtitle,
            source.language_persistence_note().unwrap()
        );
        assert!(cards[0].0.subtitle.contains("saved for the next launch"));
        assert!(cards[0].0.subtitle.contains("after successful publication"));
    }
}

#[test]
fn p3_root_category_binding_starts_keyboard_at_effective_tab_and_keeps_reopened_section() {
    use view::settings::folio::intent::{Message as Intent, SettingsMsg};
    fn next(app: &mut GpuCandidate, backwards: bool) {
        let window = app.window;
        let generation = app.settings_workbench.generation;
        let _ = update(
            app,
            Message::SettingsIntent {
                window,
                generation,
                intent: Intent::Settings(SettingsMsg::Key(
                    view::settings::composed::SettingsKey::CategoryNext { backwards },
                )),
            },
        );
    }
    let (mut app, _) = d3_application_fixture(2);
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    assert_eq!(app.settings_workbench.tab, "filter");
    assert_eq!(app.settings_workbench.navigation.category, 1);
    next(&mut app, false);
    assert_eq!(app.settings_workbench.tab, "position");
    next(&mut app, true);
    assert_eq!(app.settings_workbench.tab, "filter");
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Library),
    );
    assert_eq!(app.settings_workbench.tab, "library");
    assert_eq!(app.settings_workbench.navigation.category, 7);
    next(&mut app, false);
    assert_eq!(app.settings_workbench.tab, "automation");
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("picker".into()));
    p3_settings_intent(&mut app, SettingsMsg::SelectSection(1));
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    app.settings_workbench.navigation.category = 12;
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    assert_eq!(app.settings_workbench.tab, "picker");
    assert_eq!(app.settings_workbench.navigation.category, 0);
    assert_eq!(app.settings_workbench.navigation.section, 1);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("performance".into()));
    let section = app.settings_workbench.navigation.section;
    app.settings_workbench.navigation.category = 0;
    app.window = Some(window::Id::unique());
    view::settings::controller::reconcile(&mut app);
    assert_eq!(app.settings_workbench.tab, "performance");
    assert_eq!(app.settings_workbench.navigation.category, 6);
    assert_eq!(app.settings_workbench.navigation.section, section);
}

#[test]
fn p4_profile_identity_changes_refuse_late_future_files_without_installing_proposals() {
    use candidate_state::mode_profiles::Mode;
    use view::settings::folio::intent::Message as Intent;
    for action in 0..4 {
        for future_backup in [false, true] {
            let mut app = p3_composed_editor_app(SettingsCategory::Picker);
            app.state.gallery.depth.height = 500.0;
            p3_profile_intent(&mut app, Intent::SavePreset(Mode::Depth));
            let first = app.state.mode_profiles.selected_id(Mode::Depth).unwrap();
            app.state.gallery.depth.height = 700.0;
            p3_profile_intent(&mut app, Intent::SavePreset(Mode::Depth));
            let second = app.state.mode_profiles.selected_id(Mode::Depth).unwrap();
            app.state.gallery.depth.height = 900.0;
            app.persist_state_outcome().unwrap();
            let before = app.state.clone();
            let backup = app.state_path.with_extension("toml.bak");
            std::fs::copy(
                &app.state_path,
                app.state_path.with_extension("before-future-primary.raw"),
            )
            .unwrap();
            std::fs::copy(
                &backup,
                app.state_path.with_extension("before-future-backup.raw"),
            )
            .unwrap();
            let future = format!(
                "# owned late future fixture\nversion={}\ngallery='future typed meaning'\n",
                candidate_state::CURRENT_STATE_VERSION + 1
            );
            std::fs::write(
                if future_backup {
                    &backup
                } else {
                    &app.state_path
                },
                future,
            )
            .unwrap();
            let observed = (
                std::fs::read(&app.state_path).unwrap(),
                std::fs::read(&backup).unwrap(),
            );
            let intent = match action {
                0 => Intent::SavePreset(Mode::Depth),
                1 => Intent::PresetNameInput {
                    mode: Mode::Depth,
                    id: second,
                    raw: "edited draft name".into(),
                },
                2 => Intent::ApplyPreset(Mode::Depth, first),
                _ => Intent::DeletePreset(Mode::Depth, second),
            };
            p3_profile_intent(&mut app, intent);
            assert_eq!(app.state.mode_profiles, before.mode_profiles);
            assert_eq!(app.state.gallery, before.gallery);
            assert_eq!(app.state.card_flip, before.card_flip);
            assert_eq!(app.state.applied_theme, before.applied_theme);
            assert_eq!(app.state.pending_frontend, before.pending_frontend);
            assert_eq!(
                (
                    std::fs::read(&app.state_path).unwrap(),
                    std::fs::read(&backup).unwrap()
                ),
                observed
            );
            assert!(app.state.write_restriction.is_some());
            assert!(app
                .save_failure
                .as_deref()
                .unwrap()
                .contains("proposal was not installed"));
            assert!(!app.apply_pending);
            if action == 1 {
                assert_eq!(
                    app.settings_workbench.preset_name_raw.as_ref().unwrap().2,
                    "edited draft name"
                );
            }
        }
    }
}

#[test]
fn p4_frontend_preferences_actual_toggle_language_and_restart_without_dispatch() {
    use view::settings::{
        bridge::Source,
        composed::{keys, SettingsSource},
        folio::intent::SettingsMsg,
    };
    let (mut app, submissions) = d3_application_fixture(2);
    // The shared historical fixture switches to Depth from a fresh Slices state.
    // Depth has no built-in preset index; begin with its valid canonical state.
    app.state.gallery.variant = None;
    app.panel_state.open_settings(SettingsCategory::Picker);
    view::settings::controller::reconcile(&mut app);
    let before = app.state.gallery.clone();
    let value = !Source { app: &app }.flag(keys::general::CLOSE_ON_SELECTION);
    p3_settings_intent(
        &mut app,
        SettingsMsg::Toggle(keys::general::CLOSE_ON_SELECTION.into(), value),
    );
    assert!(
        app.state_path.exists(),
        "The real local preference control must publish through the state owner"
    );
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("language".into()));
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick(keys::general::LANGUAGE.into(), "zh-CN".into()),
    );
    assert_eq!(app.settings_locale.language.tag, "zh-CN");
    app.state = CandidateState::load(&app.state_path);
    app.settings_workbench = Default::default();
    assert_eq!(
        Source { app: &app }.flag(keys::general::CLOSE_ON_SELECTION),
        value
    );
    assert_eq!(Source { app: &app }.text(keys::general::LANGUAGE), "zh-CN");
    assert_eq!(app.state.gallery, before);
    assert!(!app.state.slideshow_enabled);
    assert!(app.state.schedules.is_empty());
    assert_eq!(submissions.load(std::sync::atomic::Ordering::Acquire), 0);
}

#[test]
fn p4_frontend_preference_save_refusal_retains_numeric_buffer_and_blocks_departure() {
    use std::fs;
    use view::settings::{composed::keys, folio::intent::SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.persist_state_outcome().unwrap();
    let future = app.state_path.with_extension("toml.bak");
    fs::write(&future, "version=5\n").unwrap();
    let before = app.state.clone();
    let path = keys::general::UI_SCALE;
    p3_settings_intent(&mut app, SettingsMsg::Input(path.into(), "1.25".into()));
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    assert!(app.settings_workbench.draft.raw_inputs.contains_key(path));
    assert_eq!(app.state.frontend_preferences, before.frontend_preferences);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("motion".into()));
    assert_eq!(app.settings_workbench.tab, "picker");
    assert_eq!(fs::read(&future).unwrap(), b"version=5\n");
    assert!(app.save_failure.is_some());
}

#[test]
fn p4_frontend_preference_cross_script_failed_publication_keeps_font_and_catalog() {
    use view::settings::{
        bridge::Source,
        composed::{keys, SettingsSource},
        folio::intent::SettingsMsg,
    };
    fn faces() -> Vec<String> {
        let mut system = iced::advanced::graphics::text::font_system()
            .write()
            .unwrap();
        system
            .raw()
            .db_mut()
            .faces()
            .map(|face| format!("{:?}:{:?}", face.id, face.families))
            .collect()
    }
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.persist_state_outcome().unwrap();
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("language".into()));
    let backup = app.state_path.with_extension("toml.bak");
    std::fs::write(&backup, "version=5\n").unwrap();
    let before = faces();
    let saved = app.state.frontend_preferences.clone();
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick(keys::general::LANGUAGE.into(), "zh-CN".into()),
    );
    assert_eq!(app.settings_locale.language.tag, "en-US");
    assert_eq!(
        Source { app: &app }.text(keys::general::LANGUAGE),
        "zh-CN",
        "Failed language choice stays editable while committed captions/fonts remain unchanged"
    );
    assert_eq!(
        faces(),
        before,
        "Rejected publication must not replace any font face"
    );
    assert_eq!(app.state.frontend_preferences, saved);
    assert_eq!(std::fs::read(&backup).unwrap(), b"version=5\n");
    assert!(app.save_failure.is_some());
}

#[test]
fn p4_frontend_preferences_binding_absent_default_unbound_reset_and_restart() {
    use view::settings::{
        bridge::Source,
        composed::{
            self,
            draft::Value,
            input::{self, InputAction},
            keys, ActionId, SettingsSource,
        },
        folio::intent::SettingsMsg,
        preference_projection,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.config.keybinds.apply_wallpaper = "f10".into();
    assert_eq!(
        Source { app: &app }.bindings().triggers(InputAction::Apply),
        input::parse_binding("f10").unwrap()
    );
    let mut saved = app.state.frontend_preferences.clone();
    preference_projection::write(
        &mut saved,
        keys::keybind::APPLY,
        &Value::Text(String::new()),
    )
    .unwrap();
    app.state.frontend_preferences = saved.clone();
    assert_eq!(
        Source { app: &app }.bindings().triggers(InputAction::Apply),
        input::parse_binding(InputAction::Apply.default_binding()).unwrap()
    );
    preference_projection::write(
        &mut saved,
        keys::keybind::APPLY,
        &Value::Text("none".into()),
    )
    .unwrap();
    app.state.frontend_preferences = saved.clone();
    app.persist_state_outcome().unwrap();
    app.state = CandidateState::load(&app.state_path);
    assert!(Source { app: &app }
        .bindings()
        .triggers(InputAction::Apply)
        .is_empty());
    assert!(preference_projection::write(
        &mut saved,
        keys::keybind::APPLY,
        &Value::Text("invented+invalid+key".into())
    )
    .is_err());
    assert_eq!(saved, app.state.frontend_preferences);
    p3_settings_intent(&mut app, SettingsMsg::Run(ActionId::ResetKeybinds));
    assert!(
        Source { app: &app }
            .bindings()
            .triggers(InputAction::Apply)
            .is_empty(),
        "First press only arms reset"
    );
    p3_settings_intent(&mut app, SettingsMsg::Run(ActionId::ResetKeybinds));
    for descriptor in composed::KEY_BINDINGS {
        let expected = descriptor
            .action
            .local_available()
            .then(|| Value::Text(String::new()));
        assert_eq!(
            preference_projection::read(&app.state.frontend_preferences, descriptor.path),
            expected,
            "reset publication for {}",
            descriptor.path
        );
    }
    app.state = CandidateState::load(&app.state_path);
    app.settings_workbench.draft.values.clear();
    assert_eq!(
        Source { app: &app }.bindings().triggers(InputAction::Apply),
        input::parse_binding(InputAction::Apply.default_binding()).unwrap()
    );
    assert_eq!(app.config.keybinds.apply_wallpaper, "f10");
}

#[test]
fn global_config_shortcuts_are_exposed_to_the_shared_capture_map() {
    use view::settings::{
        bridge::Source,
        composed::{input::InputAction, keys, SettingsSource},
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.config.keybinds.apply_to_all = "ctrl+shift+a".into();
    app.config.keybinds.switch_monitor = "alt+tab".into();
    let map = Source { app: &app }.bindings();
    assert_eq!(
        map.triggers(InputAction::ConfigApplyToAll),
        crate::candidate_app::view::settings::composed::input::parse_binding("ctrl+shift+a")
            .unwrap()
    );
    assert_eq!(
        map.triggers(InputAction::ConfigSwitchMonitor),
        crate::candidate_app::view::settings::composed::input::parse_binding("alt+tab").unwrap()
    );
    assert_eq!(
        Source { app: &app }.text(keys::keybind::CONFIG_APPLY_TO_ALL),
        "ctrl+shift+a"
    );
}

#[test]
fn compatibility_owner_p4_frontend_preferences_resolution_actual_add_edit_remove_restart_and_refusal(
) {
    use view::settings::{
        bridge,
        composed::{keys, ActionId},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    compatibility_owner_intent(&mut app, SettingsMsg::SelectTab("filter".into()));
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Run(ActionId::CreateResolutionPresetBand("fhd")),
    );
    assert_eq!(app.state.frontend_preferences.resolution_presets.len(), 1);
    let path = format!("{}.0.label", keys::filter_bar::RESOLUTION_PRESETS);
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input(path.clone(), "Owned band".into()),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(
        app.state.frontend_preferences.resolution_presets[0].label,
        "Owned band"
    );
    app.state = CandidateState::load(&app.state_path);
    app.settings_workbench = bridge::Session::from_state(&app.state);
    view::settings::controller::reconcile(&mut app);
    assert_eq!(
        app.settings_workbench.lists.resolutions,
        app.state.frontend_preferences.resolution_presets
    );
    compatibility_owner_intent(&mut app, SettingsMsg::SelectTab("filter".into()));
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Run(ActionId::RemoveResolutionPreset(0)),
    );
    assert!(CandidateState::load(&app.state_path)
        .frontend_preferences
        .resolution_presets
        .is_empty());
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Run(ActionId::CreateResolutionPresetBand("qhd")),
    );
    let before = app.state.frontend_preferences.resolution_presets.clone();
    std::fs::write(app.state_path.with_extension("toml.bak"), "version=5\n").unwrap();
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input(path.clone(), "Keep draft".into()),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(app.state.frontend_preferences.resolution_presets, before);
    assert_eq!(app.settings_workbench.draft.raw_inputs[&path], "Keep draft");
    assert!(app.save_failure.is_some());
}

#[test]
fn p4_frontend_preference_desired_monitor_does_not_pin_explicit_session_target() {
    use crate::{monitor::DesktopRect, native_wallpaper::NativeMonitor};
    fn monitors(ids: &[&str]) -> Vec<NativeMonitor> {
        ids.iter()
            .map(|id| NativeMonitor {
                device_id: (*id).into(),
                rect: DesktopRect {
                    left: 0,
                    top: 0,
                    right: 1920,
                    bottom: 1080,
                },
            })
            .collect()
    }
    let (mut app, submissions) = d3_application_fixture(2);
    app.state.frontend_preferences.picker.desired_monitor_id = Some("A".into());
    app.active_monitor_id = Some("B".into());
    app.refresh_monitors_with(
        false,
        || Ok(monitors(&["A", "B", "C"])),
        |_| panic!("No cursor policy requested"),
    );
    assert_eq!(app.active_monitor_id.as_deref(), Some("B"));
    app.refresh_monitors_with(false, || Ok(monitors(&["A", "C"])), |_| None);
    assert_eq!(app.active_monitor_id.as_deref(), Some("A"));
    app.refresh_monitors_with(true, || Ok(monitors(&["A", "C"])), |_| Some("C".into()));
    assert_eq!(app.active_monitor_id.as_deref(), Some("C"));
    assert_eq!(
        app.state
            .frontend_preferences
            .picker
            .desired_monitor_id
            .as_deref(),
        Some("A")
    );
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn p4_source_only_mode_controls_commit_and_restart_without_flattening_corners() {
    use view::settings::{
        bridge::{self, Source},
        composed::{keys::selector as K, SettingsSource},
        folio::intent::SettingsMsg,
    };
    let (mut app, submissions) = d3_application_fixture(2);
    app.state.gallery.mode = GalleryKind::Slices;
    app.gallery_mode = GalleryMode::Slices;
    app.state.gallery.variant = None;
    app.panel_state.open_settings(SettingsCategory::Picker);
    view::settings::controller::reconcile(&mut app);
    let scalar = app.state.gallery.slices.corner_radius;
    p3_settings_intent(
        &mut app,
        SettingsMsg::Toggle(K::SLICE_SHADOWS.into(), false),
    );
    assert!(
        app.state_path.exists(),
        "Source-only local mode controls must publish through the existing owner"
    );
    for (path, value) in [(K::CORNER_TL, "27"), (K::CORNER_TR, "38")] {
        p3_settings_intent(&mut app, SettingsMsg::Input(path.into(), value.into()));
        p3_settings_intent(&mut app, SettingsMsg::Commit);
    }
    p3_settings_intent(
        &mut app,
        SettingsMsg::Toggle(K::ROUND_CORNERS.into(), false),
    );
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("position".into()));
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(K::SLICE_STAGE_X.into(), "17.5".into()),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    app.state = CandidateState::load(&app.state_path);
    app.settings_workbench = bridge::Session::from_state(&app.state);
    assert!(!Source { app: &app }.flag(K::SLICE_SHADOWS));
    assert!(!Source { app: &app }.flag(K::ROUND_CORNERS));
    assert_eq!(Source { app: &app }.number(K::CORNER_TL), 27.0);
    assert_eq!(Source { app: &app }.number(K::CORNER_TR), 38.0);
    assert_eq!(Source { app: &app }.number(K::CORNER_BR), f64::from(scalar));
    assert_eq!(Source { app: &app }.number(K::SLICE_STAGE_X), 17.5);
    assert_eq!(app.state.gallery.slices.corner_radius, scalar);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn p4_geometry_preview_cancel_and_commit_have_distinct_disk_publication() {
    use view::settings::{
        composed::{keys::selector as K, SettingsKey},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.state.gallery.variant = None;
    app.persist_state_outcome().unwrap();
    let baseline = std::fs::read(&app.state_path).unwrap();
    let height = app.state.gallery.depth.height;
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(K::DEPTH_HEIGHT.into(), "700".into()),
    );
    assert_eq!(
        app.state.gallery.depth.height, 700.0,
        "Valid input remains a live preview"
    );
    assert_eq!(
        std::fs::read(&app.state_path).unwrap(),
        baseline,
        "Typing must not publish a saved geometry change"
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(K::DEPTH_HEIGHT.into(), "-".into()),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(
        app.settings_workbench.draft.raw_inputs[K::DEPTH_HEIGHT],
        "-"
    );
    assert_eq!(std::fs::read(&app.state_path).unwrap(), baseline);
    p3_settings_intent(&mut app, SettingsMsg::Key(SettingsKey::Cancel));
    assert_eq!(app.state.gallery.depth.height, height);
    assert_eq!(
        std::fs::read(&app.state_path).unwrap(),
        baseline,
        "Escape restores preview without publishing"
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(K::DEPTH_HEIGHT.into(), "710".into()),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(
        CandidateState::load(&app.state_path).gallery.depth.height,
        710.0
    );
}

#[test]
fn p4_profile_capture_preserves_canonical_hidden_integer_and_stage_bits() {
    use candidate_state::mode_profiles::{Snapshot, StageTransform};
    use view::settings::profile_projection;
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.gallery_mode = GalleryMode::Geometric;
    app.state.gallery.mode = GalleryKind::Geometric;
    let stage = StageTransform {
        x: 1.0 / 3.0,
        shear_y: 0.12345679,
        ..Default::default()
    };
    app.state.gallery.geometric.stage = Some(stage.clone());
    app.state.gallery.geometric.scroll_step =
        Some(candidate_state::HexScrollStep::Integer(16_777_217));
    let Snapshot::Hex(snapshot) = profile_projection::capture(&app) else {
        panic!("Hex")
    };
    assert_eq!(snapshot.scroll_step, 16_777_217);
    assert_eq!(snapshot.stage, stage);
    use view::settings::{
        bridge::Source,
        composed::{keys::selector as K, SettingsSource},
        folio::intent::Message as Intent,
    };
    app.state.gallery.variant = None;
    view::settings::controller::reconcile(&mut app);
    p3_profile_intent(&mut app, Intent::SavePreset(GalleryKind::Geometric));
    let id = app
        .state
        .mode_profiles
        .selected_id(GalleryKind::Geometric)
        .unwrap();
    app.state.gallery.geometric.scroll_step = Some(candidate_state::HexScrollStep::Integer(2));
    app.state.gallery.geometric.stage = Some(StageTransform::default());
    p3_profile_intent(&mut app, Intent::ApplyPreset(GalleryKind::Geometric, id));
    assert_eq!(
        app.state.gallery.geometric.scroll_step,
        Some(candidate_state::HexScrollStep::Integer(16_777_217))
    );
    assert_eq!(app.state.gallery.geometric.stage.as_ref(), Some(&stage));
    assert_eq!(
        Source { app: &app }.number(K::HEX_SCROLL_STEP),
        16_777_217.0
    );
    app.state = CandidateState::load(&app.state_path);
    app.settings_workbench = view::settings::bridge::Session::from_state(&app.state);
    assert_eq!(
        app.state.gallery.geometric.scroll_step,
        Some(candidate_state::HexScrollStep::Integer(16_777_217))
    );
    assert_eq!(app.state.gallery.geometric.stage.as_ref(), Some(&stage));
    assert_eq!(
        Source { app: &app }.number(K::HEX_SCROLL_STEP),
        16_777_217.0
    );
}

#[test]
fn p4_unrepresentable_geometry_edit_remains_invalid_and_cannot_depart() {
    use view::settings::{composed::keys::selector as K, folio::intent::SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.gallery_mode = GalleryMode::Slices;
    app.state.gallery.mode = GalleryKind::Slices;
    app.state.gallery.variant = None;
    view::settings::controller::reconcile(&mut app);
    app.persist_state_outcome().unwrap();
    let bytes = std::fs::read(&app.state_path).unwrap();
    let width = app.state.gallery.slices.width;
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(K::SLICE_WIDTH.into(), "1e100".into()),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("motion".into()));
    assert_eq!(app.settings_workbench.tab, "picker");
    assert!(app
        .settings_workbench
        .draft
        .errors
        .contains_key(K::SLICE_WIDTH));
    assert_eq!(
        app.settings_workbench.draft.raw_inputs[K::SLICE_WIDTH],
        "1e100"
    );
    assert_eq!(app.state.gallery.slices.width, width);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), bytes);
}

#[test]
fn p4_unrelated_save_excludes_preview_and_queued_geometry_finishes_current_edit() {
    use view::settings::{composed::keys::selector as K, folio::intent::SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.state.gallery.variant = None;
    app.persist_state_outcome().unwrap();
    let old_height = app.state.gallery.depth.height;
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(K::DEPTH_HEIGHT.into(), "700".into()),
    );
    app.state.favorites.insert("owned".into());
    app.persist_state_outcome().unwrap();
    let saved = CandidateState::load(&app.state_path);
    assert_eq!(saved.gallery.depth.height, old_height);
    assert!(saved.favorites.contains("owned"));
    assert_eq!(app.state.gallery.depth.height, 700.0);
    let _ = update(
        &mut app,
        Message::SetGalleryNumber(GalleryNumberField::DepthWidth, 420.0),
    );
    let saved = CandidateState::load(&app.state_path);
    assert_eq!(
        (saved.gallery.depth.height, saved.gallery.depth.width),
        (700.0, 420.0)
    );
    assert!(app.settings_workbench.geometry_preview_baseline.is_none());
    assert!(app.settings_workbench.draft.raw_inputs.is_empty());
}

#[test]
fn p4_inactive_mode_position_preserves_active_preset_and_profile_identity() {
    use view::settings::{composed::keys::selector as K, folio::intent::SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.state.gallery.mode = GalleryKind::Slices;
    app.gallery_mode = GalleryMode::Slices;
    app.state.gallery.variant = Some(2);
    app.panel_state.open_settings(SettingsCategory::Position);
    view::settings::controller::reconcile(&mut app);
    let slices = app.state.gallery.slices.clone();
    let profiles = app.state.mode_profiles.clone();
    let flip = app.state.card_flip().clone();
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(K::HEX_STAGE_X.into(), "33.5".into()),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(app.state.gallery.variant, Some(2));
    assert_eq!(app.state.gallery.slices, slices);
    assert_eq!(app.state.mode_profiles, profiles);
    assert_eq!(app.state.card_flip(), &flip);
    assert_eq!(app.state.gallery.geometric.stage.as_ref().unwrap().x, 0.335);
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(K::SLICE_STAGE_X.into(), "8".into()),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(app.state.gallery.variant, None);
}

#[test]
fn p4_source_only_huge_corner_edit_never_installs_nonfinite_geometry() {
    use view::settings::{
        composed::{keys::selector as K, SettingsKey},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.gallery_mode = GalleryMode::Slices;
    app.state.gallery.mode = GalleryKind::Slices;
    app.state.gallery.variant = None;
    view::settings::controller::reconcile(&mut app);
    app.persist_state_outcome().unwrap();
    let saved = std::fs::read(&app.state_path).unwrap();
    let gallery = app.state.gallery.clone();
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(K::CORNER_TL.into(), "1e100".into()),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(app.state.gallery, gallery);
    assert!(app
        .settings_workbench
        .draft
        .errors
        .contains_key(K::CORNER_TL));
    assert_eq!(std::fs::read(&app.state_path).unwrap(), saved);
    p3_settings_intent(&mut app, SettingsMsg::Key(SettingsKey::Cancel));
    assert!(app.settings_workbench.draft.errors.is_empty());
    assert_eq!(app.state.gallery, gallery);
}

#[test]
fn r2_geometry_and_profiles_return_the_actual_pending_preview_task_once() {
    use view::settings::{composed::draft::Value, folio::intent};

    for action in [
        "geometry",
        "geometry-noop",
        "profile",
        "missing-profile",
        "pointer",
    ] {
        let mut app = p3_composed_editor_app(SettingsCategory::Picker);
        app.settings_workbench.navigation.section =
            view::settings::composed::PICKER_CONTROLS_SECTION;
        assert!(!app.settings_workbench.layout_studio());
        let png = app.state_path.with_file_name("preview.png");
        image::RgbImage::from_pixel(4, 4, image::Rgb([10, 50, 200]))
            .save(&png)
            .unwrap();
        app.wallpapers[0].source = WallpaperSource::LocalImage;
        app.wallpapers[0].file_target = png.clone();
        app.wallpapers[0].preview_path = Some(png);
        app.selected = Some(0);
        app.save_gallery_preset().unwrap();
        let id = app
            .state
            .mode_profiles
            .selected_id(GalleryKind::Depth)
            .unwrap();
        let path = "appearance.presentation";
        app.settings_workbench
            .draft
            .raw_inputs
            .insert(path.into(), "live".into());
        app.settings_workbench
            .draft
            .values
            .insert(path.into(), Value::Text("live".into()));
        app.settings_workbench.navigation.input_edit = Some(path.into());
        let width = app.state.gallery.depth.width;
        let task = match action {
            "geometry" => update(
                &mut app,
                Message::SetGalleryNumber(GalleryNumberField::DepthWidth, 421.0),
            ),
            "geometry-noop" => update(
                &mut app,
                Message::SetGalleryNumber(GalleryNumberField::DepthWidth, width),
            ),
            "missing-profile" => update(
                &mut app,
                Message::ApplyGalleryPreset(GalleryKind::Depth, u64::MAX),
            ),
            "pointer" => {
                let generation = app.settings_workbench.generation;
                view::settings::controller::dispatch(
                    &mut app,
                    generation,
                    intent::Message::ApplyPreset(GalleryKind::Depth, id),
                )
            }
            _ => update(
                &mut app,
                Message::ApplyGalleryPreset(GalleryKind::Depth, id),
            ),
        };
        assert_eq!(task.units(), 1, "{action}");
        assert!(
            app.settings_workbench.preview_completion().is_some(),
            "{action}"
        );
        assert!(
            app.settings_workbench.navigation.input_edit.is_none(),
            "{action}"
        );
        assert!(!app.settings_workbench.draft.raw_inputs.contains_key(path));
        let saved = CandidateState::load(&app.state_path);
        assert_eq!(
            saved.appearance.presentation,
            BackgroundPresentation::LiveTransparentOverlay,
            "{action}"
        );
        assert_eq!(
            saved.gallery.depth.width,
            if action == "geometry" { 421.0 } else { width },
            "{action}"
        );
        assert_eq!(
            saved.mode_profiles.selected_id(GalleryKind::Depth),
            Some(id),
            "{action}"
        );
        if action == "missing-profile" {
            assert!(app
                .settings_workbench
                .message
                .contains("no available geometry"));
        }
    }
}

#[test]
fn p4_profile_owner_rejects_invalid_preview_before_geometry_or_selected_id_publication() {
    use view::settings::{
        composed::keys::selector as K, folio::intent::SettingsMsg, profile_projection,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.state.gallery.variant = None;
    let profile = profile_projection::capture(&app);
    app.save_gallery_preset().unwrap();
    let id = app
        .state
        .mode_profiles
        .selected_id(GalleryKind::Depth)
        .unwrap();
    let bytes = std::fs::read(&app.state_path).unwrap();
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(K::DEPTH_HEIGHT.into(), "700".into()),
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(K::DEPTH_HEIGHT.into(), "-".into()),
    );
    assert!(
        profile_projection::install(&mut app, Ok(&profile), Some(id))
            .1
            .is_err()
    );
    assert_eq!(std::fs::read(&app.state_path).unwrap(), bytes);
    assert_eq!(
        app.settings_workbench.draft.raw_inputs[K::DEPTH_HEIGHT],
        "-"
    );
    assert_eq!(app.state.gallery.depth.height, 700.0);
    let _ = update(
        &mut app,
        Message::SetGalleryNumber(GalleryNumberField::DepthWidth, 420.0),
    );
    assert_eq!(std::fs::read(&app.state_path).unwrap(), bytes);
    assert_ne!(app.state.gallery.depth.width, 420.0);
}

#[test]
fn p4_both_unrelated_save_facades_keep_preview_out_of_the_saved_gallery() {
    use view::settings::{
        composed::{keys::selector as K, SettingsKey},
        folio::intent::SettingsMsg,
    };
    for cloned in [false, true] {
        let mut app = p3_composed_editor_app(SettingsCategory::Picker);
        app.state.gallery.variant = None;
        app.persist_state_outcome().unwrap();
        let baseline = app.state.gallery.clone();
        p3_settings_intent(
            &mut app,
            SettingsMsg::Input(K::DEPTH_HEIGHT.into(), "700".into()),
        );
        if cloned {
            let mut proposed = app.state.clone();
            proposed.favorites.insert("owned-clone".into());
            app.commit_local_state_change(proposed).unwrap();
        } else {
            app.state.favorites.insert("owned-inplace".into());
            app.persist_state_outcome().unwrap();
        }
        assert_eq!(CandidateState::load(&app.state_path).gallery, baseline);
        assert_eq!(app.state.gallery.depth.height, 700.0);
        p3_settings_intent(&mut app, SettingsMsg::Key(SettingsKey::Cancel));
        assert_eq!(app.state.gallery, baseline);
        assert_eq!(CandidateState::load(&app.state_path).gallery, baseline);
    }
}

#[test]
fn p4_geometry_edit_returning_to_original_preserves_curated_identity_without_publication() {
    use view::settings::{composed::keys::selector as K, folio::intent::SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.state.gallery.mode = GalleryKind::Slices;
    app.gallery_mode = GalleryMode::Slices;
    app.state.gallery.variant = Some(2);
    view::settings::controller::reconcile(&mut app);
    app.persist_state_outcome().unwrap();
    let saved = std::fs::read(&app.state_path).unwrap();
    let original = app.state.gallery.slices.width;
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(K::SLICE_WIDTH.into(), "700".into()),
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(K::SLICE_WIDTH.into(), original.to_string()),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(app.state.gallery.variant, Some(2));
    assert_eq!(std::fs::read(&app.state_path).unwrap(), saved);
    assert!(app.settings_workbench.draft.raw_inputs.is_empty());
    assert!(app.settings_workbench.geometry_preview_baseline.is_none());
}

#[test]
fn p4_hidden_hex_scroll_raw_fraction_restart_and_effective_profile_boundary() {
    use view::settings::{
        bridge::{self, Source},
        composed::{draft::Value, keys::selector as K, SettingsSource},
        folio::intent::Message as Intent,
        geometry_projection,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.gallery_mode = GalleryMode::Geometric;
    app.state.gallery.mode = GalleryKind::Geometric;
    app.state.gallery.variant = None;
    view::settings::controller::reconcile(&mut app);
    // The reference has no visible control for this hidden setting. Exercise
    // its actual storage adapter and real profile entry points, not a forged UI.
    geometry_projection::write(&mut app.state, K::HEX_SCROLL_STEP, &Value::Number(3.75)).unwrap();
    app.persist_state_outcome().unwrap();
    assert_eq!(Source { app: &app }.number(K::HEX_SCROLL_STEP), 3.75);
    app.state = CandidateState::load(&app.state_path);
    app.settings_workbench = bridge::Session::from_state(&app.state);
    view::settings::controller::reconcile(&mut app);
    assert_eq!(Source { app: &app }.number(K::HEX_SCROLL_STEP), 3.75);
    p3_profile_intent(&mut app, Intent::SavePreset(GalleryKind::Geometric));
    let id = app
        .state
        .mode_profiles
        .selected_id(GalleryKind::Geometric)
        .unwrap();
    let candidate_state::mode_profiles::Snapshot::Hex(profile) = app
        .state
        .mode_profiles
        .snapshot(GalleryKind::Geometric, id)
        .unwrap()
    else {
        panic!("Hex")
    };
    assert_eq!(profile.scroll_step, 3);
    assert_eq!(
        Source { app: &app }.number(K::HEX_SCROLL_STEP),
        3.75,
        "Save captures effective geometry without overwriting raw current configuration"
    );
    geometry_projection::write(&mut app.state, K::HEX_SCROLL_STEP, &Value::Number(8.25)).unwrap();
    p3_profile_intent(&mut app, Intent::ApplyPreset(GalleryKind::Geometric, id));
    assert_eq!(Source { app: &app }.number(K::HEX_SCROLL_STEP), 3.0);
    assert_eq!(
        CandidateState::load(&app.state_path).mode_profiles,
        app.state.mode_profiles
    );
}

#[test]
fn p4_hidden_hex_scroll_finite_raw_values_preserve_source_effective_rules() {
    use view::settings::{
        bridge::Source,
        composed::{draft::Value, keys::selector as K, SettingsSource},
        geometry_projection, profile_projection,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.gallery_mode = GalleryMode::Geometric;
    app.state.gallery.mode = GalleryKind::Geometric;
    app.state.gallery.variant = None;
    for (raw, effective) in [
        (3.75, 3),
        (0.0, 1),
        (-5.0, 1),
        (0.75, 1),
        (16_777_217.0, 16_777_217),
        (1e100, usize::MAX as u64),
    ] {
        geometry_projection::write(&mut app.state, K::HEX_SCROLL_STEP, &Value::Number(raw))
            .unwrap();
        app.persist_state_outcome().unwrap();
        app.state = CandidateState::load(&app.state_path);
        assert_eq!(Source { app: &app }.number(K::HEX_SCROLL_STEP), raw);
        let candidate_state::mode_profiles::Snapshot::Hex(snapshot) =
            profile_projection::capture(&app)
        else {
            panic!("Hex")
        };
        assert_eq!(snapshot.scroll_step, effective);
    }
    let before = app.state.clone();
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(geometry_projection::write(
            &mut app.state,
            K::HEX_SCROLL_STEP,
            &Value::Number(invalid)
        )
        .is_err());
        assert_eq!(app.state, before);
    }
}

#[test]
fn p4_hex_profile_apply_preserves_exact_integer_storage_above_f64_precision() {
    use view::settings::{folio::intent::Message as Intent, profile_projection};
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    app.gallery_mode = GalleryMode::Geometric;
    app.state.gallery.mode = GalleryKind::Geometric;
    app.state.gallery.variant = None;
    view::settings::controller::reconcile(&mut app);
    let candidate_state::mode_profiles::Snapshot::Hex(mut profile) =
        profile_projection::capture(&app)
    else {
        panic!("Hex")
    };
    let exact = 9_007_199_254_740_993_u64;
    profile.scroll_step = exact;
    let id = app
        .state
        .mode_profiles
        .save(
            candidate_state::mode_profiles::Snapshot::Hex(profile.clone()),
            |index| format!("Owned {index}"),
        )
        .unwrap();
    app.persist_state_outcome().unwrap();
    p3_profile_intent(&mut app, Intent::ApplyPreset(GalleryKind::Geometric, id));
    assert_eq!(
        app.state.gallery.geometric.scroll_step,
        Some(candidate_state::HexScrollStep::Integer(exact as i64))
    );
    for _ in 0..2 {
        app.persist_state_outcome().unwrap();
        let raw: toml::Value =
            toml::from_str(&std::fs::read_to_string(&app.state_path).unwrap()).unwrap();
        assert_eq!(
            raw["gallery"]["geometric"]["scroll_step"].as_integer(),
            Some(exact as i64)
        );
        app.state = CandidateState::load(&app.state_path);
        assert_eq!(
            app.state.mode_profiles.snapshot(GalleryKind::Geometric, id),
            Some(candidate_state::mode_profiles::Snapshot::Hex(
                profile.clone()
            ))
        );
    }
    // Numeric presentation/effective consumption has the reference f64 seam;
    // it cannot justify rewriting either exact stored integer above.
}

#[test]
fn compatibility_owner_p4_pending_playback_forms_publish_only_valid_intent_and_restart_inert() {
    use view::settings::{
        bridge::{self, Source},
        composed::{draft::Value, keys, SettingsKey, SettingsSource},
        folio::intent::SettingsMsg,
    };
    let (mut app, submissions) = d3_application_fixture(2);
    app.state.gallery.variant = None;
    app.panel_state.open_settings(SettingsCategory::Playback);
    view::settings::controller::reconcile(&mut app);
    let gallery = app.state.gallery.clone();
    let config = toml::to_string(&app.config).unwrap();
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Toggle(keys::playback::PROCESS_ENABLED.into(), true),
    );
    assert!(
        app.state_path.exists(),
        "A valid pending toggle must publish durable intent without executing it"
    );
    let before = std::fs::read(&app.state_path).unwrap();
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input(keys::playback::RESUME_DELAY.into(), "1.25".into()),
    );
    assert_eq!(
        std::fs::read(&app.state_path).unwrap(),
        before,
        "Numeric edit stages until Commit"
    );
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    let accepted = app.state.pending_frontend.clone();
    assert!(accepted.settings.iter().any(|entry| entry.source_path
        == keys::playback::RESUME_DELAY
        && entry.value_json == "1.25"));
    let saved = std::fs::read(&app.state_path).unwrap();
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input(keys::playback::RESUME_DELAY.into(), "-".into()),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    compatibility_owner_intent(&mut app, SettingsMsg::SelectTab("motion".into()));
    assert_eq!(app.settings_workbench.tab, "playback");
    assert_eq!(app.state.pending_frontend, accepted);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), saved);
    compatibility_owner_intent(&mut app, SettingsMsg::Key(SettingsKey::Cancel));
    app.state = CandidateState::load(&app.state_path);
    app.settings_workbench = bridge::Session::from_state(&app.state);
    view::settings::controller::reconcile(&mut app);
    assert!(Source { app: &app }.flag(keys::playback::PROCESS_ENABLED));
    assert_eq!(
        Source { app: &app }.number(keys::playback::RESUME_DELAY),
        1.25
    );
    for capability in [
        keys::features::MATUGEN,
        keys::features::STEAM,
        keys::semantic::ENABLED,
    ] {
        app.settings_workbench
            .draft
            .values
            .insert(capability.into(), Value::Boolean(true));
    }
    view::settings::controller::reconcile(&mut app);
    assert_eq!(app.state.pending_frontend, accepted);
    assert_eq!(app.state.gallery, gallery);
    assert_eq!(toml::to_string(&app.config).unwrap(), config);
    assert!(!app.state.slideshow_enabled);
    assert!(app.state.schedules.is_empty());
    assert!(app.active_operation.is_none() && !app.apply_pending);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn compatibility_owner_p4_pending_process_picker_add_remove_restart_and_fault_retains_draft() {
    use view::settings::{
        bridge::{self, Source},
        composed::{keys, ActionId, SettingsSource},
        folio::intent::SettingsMsg,
    };
    let (mut app, submissions) = d3_application_fixture(2);
    app.state.gallery.variant = None;
    app.panel_state.open_settings(SettingsCategory::Playback);
    view::settings::controller::reconcile(&mut app);
    compatibility_owner_intent(&mut app, SettingsMsg::Run(ActionId::ChooseRunningProcess));
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::ProcessPickerManualInput("owned-example.exe".into()),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::ProcessPickerManualAdd);
    assert_eq!(
        CandidateState::load(&app.state_path).pending_frontend,
        app.state.pending_frontend
    );
    let persisted = app.state.pending_frontend.clone();
    assert!(persisted
        .settings
        .iter()
        .any(|entry| entry.source_path == keys::playback::PROCESSES
            && entry.value_json == r#""owned-example.exe""#));
    app.state = CandidateState::load(&app.state_path);
    app.settings_workbench = bridge::Session::from_state(&app.state);
    view::settings::controller::reconcile(&mut app);
    assert_eq!(
        Source { app: &app }.text(keys::playback::PROCESSES),
        "owned-example.exe"
    );
    compatibility_owner_intent(&mut app, SettingsMsg::Run(ActionId::ChooseRunningProcess));
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::ProcessPickerRemove("owned-example.exe".into()),
    );
    assert_eq!(Source { app: &app }.text(keys::playback::PROCESSES), "");
    assert!(
        CandidateState::load(&app.state_path)
            .pending_frontend
            .settings
            .iter()
            .any(|entry| entry.source_path == keys::playback::PROCESSES
                && entry.value_json == r#""""#)
    );
    let backup = app.state_path.with_extension("toml.bak");
    let future = format!(
        "# owned future backup\nversion={}\n",
        candidate_state::CURRENT_STATE_VERSION + 1
    );
    std::fs::write(&backup, &future).unwrap();
    let primary = std::fs::read(&app.state_path).unwrap();
    let before = app.state.pending_frontend.clone();
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::ProcessPickerManualInput("retained-example.exe".into()),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::ProcessPickerManualAdd);
    assert_eq!(app.state.pending_frontend, before);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), primary);
    assert_eq!(std::fs::read(&backup).unwrap(), future.as_bytes());
    assert_eq!(
        Source { app: &app }.text(keys::playback::PROCESSES),
        "retained-example.exe"
    );
    assert_eq!(
        app.settings_workbench
            .process_picker
            .as_ref()
            .unwrap()
            .manual,
        "retained-example.exe"
    );
    assert!(app.save_failure.is_some());
    assert!(!app.state.slideshow_enabled && app.state.schedules.is_empty());
    assert!(app.active_operation.is_none() && !app.apply_pending);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn compatibility_owner_p4_pending_scalar_failed_commit_retains_raw_and_refuses_late_future_roles() {
    use view::settings::{composed::keys, folio::intent::SettingsMsg};
    for backup_role in [false, true] {
        let mut app = p3_composed_editor_app(SettingsCategory::Playback);
        app.persist_state_outcome().unwrap();
        let backup = app.state_path.with_extension("toml.bak");
        std::fs::copy(&app.state_path, &backup).unwrap();
        let future = format!(
            "# owned future file\nversion={}\n",
            candidate_state::CURRENT_STATE_VERSION + 1
        );
        std::fs::write(
            if backup_role {
                &backup
            } else {
                &app.state_path
            },
            &future,
        )
        .unwrap();
        let before = (
            std::fs::read(&app.state_path).unwrap(),
            std::fs::read(&backup).unwrap(),
        );
        let saved = app.state.pending_frontend.clone();
        compatibility_owner_intent(
            &mut app,
            SettingsMsg::Input(keys::playback::RESUME_DELAY.into(), "2.75".into()),
        );
        compatibility_owner_intent(&mut app, SettingsMsg::Commit);
        assert_eq!(app.state.pending_frontend, saved);
        assert_eq!(
            (
                std::fs::read(&app.state_path).unwrap(),
                std::fs::read(&backup).unwrap()
            ),
            before
        );
        assert_eq!(
            app.settings_workbench
                .draft
                .raw_inputs
                .get(keys::playback::RESUME_DELAY)
                .map(String::as_str),
            Some("2.75")
        );
        assert!(app.state.write_restriction.is_some() && app.save_failure.is_some());
        compatibility_owner_intent(&mut app, SettingsMsg::SelectTab("motion".into()));
        assert_eq!(app.settings_workbench.tab, "playback");
    }
}

#[test]
fn compatibility_owner_p4_pending_five_reviewed_source_fields_use_actual_commit_and_pick_routes() {
    use candidate_state::frontend::DeferredCapability as C;
    use view::settings::{
        bridge::{self, Source},
        composed::SettingsSource,
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Automation);
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input("schedule.longitude".into(), "west-edge".into()),
    );
    assert!(!app.state_path.exists(), "Text edits stage until Commit");
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    for (tab, path, choice, owner) in [
        (
            "automation",
            "schedule.longitude",
            "west-edge",
            C::ExpandedScheduling,
        ),
        (
            "library",
            "performance.imageOptimizeResolution",
            "4k",
            C::Optimization,
        ),
        (
            "library",
            "wallhaven.defaults.topRange",
            "1y",
            C::SourceBrowsing,
        ),
        ("library", "steam.defaults.sort", "21", C::SourceBrowsing),
        (
            "library",
            "steam.defaults.trendDays",
            "3",
            C::SourceBrowsing,
        ),
    ] {
        compatibility_owner_intent(&mut app, SettingsMsg::SelectTab(tab.into()));
        if path != "schedule.longitude" {
            compatibility_owner_intent(&mut app, SettingsMsg::Pick(path.into(), choice.into()));
        }
        let entry = app
            .state
            .pending_frontend
            .settings
            .iter()
            .find(|entry| entry.source_path == path && entry.capability == owner)
            .unwrap_or_else(|| panic!("Missing published record {path}"));
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&entry.value_json).unwrap(),
            serde_json::json!(choice)
        );
        assert!(
            candidate_state::pending::normalize(owner, path, serde_json::Value::Bool(false))
                .is_err()
        );
        if path != "schedule.longitude" {
            assert!(candidate_state::pending::normalize(
                owner,
                path,
                serde_json::json!("invented-value")
            )
            .is_err());
        }
    }
    let pending = app.state.pending_frontend.clone();
    app.state = CandidateState::load(&app.state_path);
    app.settings_workbench = bridge::Session::from_state(&app.state);
    view::settings::controller::reconcile(&mut app);
    assert_eq!(app.state.pending_frontend, pending);
    for (path, expected) in [
        ("schedule.longitude", "west-edge"),
        ("performance.imageOptimizeResolution", "4k"),
        ("wallhaven.defaults.topRange", "1y"),
        ("steam.defaults.sort", "21"),
        ("steam.defaults.trendDays", "3"),
    ] {
        assert_eq!(Source { app: &app }.text(path), expected);
    }
    assert!(!app.state.slideshow_enabled && app.state.schedules.is_empty());
    assert!(!app.apply_pending && app.active_operation.is_none());
}

fn p4_pending_collection_json(app: &GpuCandidate, root: &str) -> serde_json::Value {
    serde_json::from_str(
        &app.state
            .pending_frontend
            .settings
            .iter()
            .find(|entry| entry.source_path == root)
            .unwrap_or_else(|| panic!("Missing {root}"))
            .value_json,
    )
    .unwrap()
}

#[test]
fn compatibility_owner_p4_pending_collections_restore_full_records_and_publish_real_form_triggers()
{
    use candidate_state::frontend::{PendingOnly, PendingSetting};
    use serde_json::json;
    use view::settings::{
        bridge::{self, Source},
        composed::{ActionId, SettingsSource},
        folio::intent::SettingsMsg,
    };
    let (mut app, submissions) = d3_application_fixture(2);
    app.state.gallery.variant = None;
    let config = toml::to_string(&app.config).unwrap();
    let integration = json!([{"name":"same","template":"owned-template","output":"owned-output","enabled":true,"reload":"literal inert command","livePreview":true,"future":{"nullable":null,"mixed":[true,9007199254740993u64,"x"]}},{"name":"same","template":"other-template","output":"other-output"}]);
    let commands = json!([{"type":"all","command":"literal old command","retained":null}]);
    let models =
        json!([{"name":"owned-model","manifest":"missing-owned-manifest.json","extra":[null,1]}]);
    for (root, value) in [
        ("integrations", integration.clone()),
        ("postProcessing", commands.clone()),
        ("semantic.models", models.clone()),
    ] {
        app.state.pending_frontend.settings.push(PendingSetting {
            capability: candidate_state::pending::root_owner(root).unwrap(),
            source_path: root.into(),
            value_json: value.to_string(),
            status: PendingOnly::Pending,
        });
    }
    app.persist_state_outcome().unwrap();
    app.state = CandidateState::load(&app.state_path);
    app.settings_workbench = bridge::Session::from_state(&app.state);
    app.panel_state.open_settings(SettingsCategory::Theme);
    view::settings::controller::reconcile(&mut app);
    assert_eq!(app.settings_workbench.lists.integrations.len(), 2);
    assert!(Source { app: &app }.flag("integrations.0.livePreview"));
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Toggle("integrations.0.enabled".into(), false),
    );
    let mut expected = integration;
    expected[0]["enabled"] = json!(false);
    assert_eq!(p4_pending_collection_json(&app, "integrations"), expected);
    let before = std::fs::read(&app.state_path).unwrap();
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input(
            "integrations.0.reload".into(),
            "literal revised command".into(),
        ),
    );
    assert_eq!(std::fs::read(&app.state_path).unwrap(), before);
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    expected[0]["reload"] = json!("literal revised command");
    assert_eq!(p4_pending_collection_json(&app, "integrations"), expected);
    compatibility_owner_intent(&mut app, SettingsMsg::Run(ActionId::RemoveIntegration(1)));
    expected.as_array_mut().unwrap().remove(1);
    assert_eq!(p4_pending_collection_json(&app, "integrations"), expected);
    compatibility_owner_intent(&mut app, SettingsMsg::Run(ActionId::AddIntegration));
    expected
        .as_array_mut()
        .unwrap()
        .push(json!({"name":"","template":"","output":""}));
    assert_eq!(
        p4_pending_collection_json(&app, "integrations"),
        expected,
        "Add preserves source optional absence"
    );
    compatibility_owner_intent(&mut app, SettingsMsg::SelectTab("automation".into()));
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Pick("postProcessing.0.type".into(), "video".into()),
    );
    let mut expected_commands = commands;
    expected_commands[0]["type"] = json!("video");
    assert_eq!(
        p4_pending_collection_json(&app, "postProcessing"),
        expected_commands
    );
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input(
            "postProcessing.0.command".into(),
            "literal revised text".into(),
        ),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    expected_commands[0]["command"] = json!("literal revised text");
    assert_eq!(
        p4_pending_collection_json(&app, "postProcessing"),
        expected_commands
    );
    compatibility_owner_intent(&mut app, SettingsMsg::SelectTab("filter".into()));
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input(
            "semantic.models.0.manifest".into(),
            "owned-other-manifest.json".into(),
        ),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    let mut expected_models = models;
    expected_models[0]["manifest"] = json!("owned-other-manifest.json");
    assert_eq!(
        p4_pending_collection_json(&app, "semantic.models"),
        expected_models
    );
    let pending = app.state.pending_frontend.clone();
    let loaded = CandidateState::load_with_diagnostics(&app.state_path);
    assert!(loaded
        .diagnostic
        .as_deref()
        .unwrap()
        .contains("unrecognized record members were preserved"));
    assert_eq!(loaded.state.pending_frontend, pending);
    app.state = loaded.state;
    app.settings_workbench = bridge::Session::from_state(&app.state);
    assert_eq!(
        app.settings_workbench.lists.commands[0].command,
        "literal revised text"
    );
    assert!(app.settings_workbench.lists.integrations[0].live_preview == Some(true));
    assert_eq!(
        app.settings_workbench.lists.semantic_models[0].manifest,
        "owned-other-manifest.json"
    );
    assert_eq!(toml::to_string(&app.config).unwrap(), config);
    assert!(!app.state.slideshow_enabled && app.state.schedules.is_empty());
    assert!(!app.apply_pending && app.active_operation.is_none());
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn p4_pending_incompatible_collection_roots_are_retained_and_cannot_be_overwritten_by_add() {
    use candidate_state::frontend::{PendingOnly, PendingSetting};
    use serde_json::json;
    use view::settings::{bridge, composed::ActionId, folio::intent::SettingsMsg};
    for (root, value, category, action) in [
        (
            "integrations",
            json!([{"name":"owned","template":"owned","output":"owned","enabled":null}]),
            SettingsCategory::Theme,
            ActionId::AddIntegration,
        ),
        (
            "integrations",
            json!([{"name":"owned","template":"owned","output":"owned","reload":null}]),
            SettingsCategory::Theme,
            ActionId::AddIntegration,
        ),
        (
            "postProcessing",
            json!([{"type":"we-scene","command":"literal retained"}]),
            SettingsCategory::Automation,
            ActionId::AddPostCommand,
        ),
        (
            "semantic.models",
            json!([{"name":"owned","manifest":null}]),
            SettingsCategory::CatalogFilters,
            ActionId::AddSemanticModel,
        ),
    ] {
        let mut app = p3_composed_editor_app(category);
        app.state.pending_frontend.settings.push(PendingSetting {
            capability: candidate_state::pending::root_owner(root).unwrap(),
            source_path: root.into(),
            value_json: value.to_string(),
            status: PendingOnly::Pending,
        });
        app.persist_state_outcome().unwrap();
        let raw = std::fs::read(&app.state_path).unwrap();
        let loaded = CandidateState::load_with_diagnostics(&app.state_path);
        assert!(loaded.diagnostic.unwrap().contains("retained inert"));
        app.state = loaded.state;
        app.settings_workbench = bridge::Session::from_state(&app.state);
        view::settings::controller::reconcile(&mut app);
        p3_settings_intent(&mut app, SettingsMsg::Run(action));
        assert_eq!(std::fs::read(&app.state_path).unwrap(), raw);
        assert_eq!(p4_pending_collection_json(&app, root), value);
        assert!(app.settings_workbench.message.contains("retained"));
        assert_eq!(app.settings_workbench.lists.len(root), 0);
    }
}

#[test]
fn compatibility_owner_p4_pending_collection_publication_failure_retains_useful_local_records_without_success(
) {
    use view::settings::{composed::ActionId, folio::intent::SettingsMsg};
    for backup_role in [false, true] {
        let mut app = p3_composed_editor_app(SettingsCategory::Automation);
        app.persist_state_outcome().unwrap();
        let backup = app.state_path.with_extension("toml.bak");
        std::fs::copy(&app.state_path, &backup).unwrap();
        std::fs::write(
            if backup_role {
                &backup
            } else {
                &app.state_path
            },
            format!("version={}\n", candidate_state::CURRENT_STATE_VERSION + 1),
        )
        .unwrap();
        let before = (
            std::fs::read(&app.state_path).unwrap(),
            std::fs::read(&backup).unwrap(),
        );
        let pending = app.state.pending_frontend.clone();
        compatibility_owner_intent(&mut app, SettingsMsg::Run(ActionId::AddPostCommand));
        assert_eq!(app.state.pending_frontend, pending);
        assert_eq!(app.settings_workbench.lists.commands.len(), 1);
        assert!(!app.settings_workbench.message.starts_with("Saved pending."));
        assert!(app.save_failure.is_some());
        assert_eq!(
            (
                std::fs::read(&app.state_path).unwrap(),
                std::fs::read(&backup).unwrap()
            ),
            before
        );
        assert!(!app.state.slideshow_enabled && app.state.schedules.is_empty());
        assert!(!app.apply_pending && app.active_operation.is_none());
    }
}

#[test]
fn compatibility_owner_p4_pending_integration_absent_flags_keep_source_defaults_without_inserting_values(
) {
    use view::settings::{
        bridge::{self, Source},
        composed::{ActionId, SettingsSource},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Theme);
    compatibility_owner_intent(&mut app, SettingsMsg::Run(ActionId::AddIntegration));
    assert!(Source { app: &app }.flag("integrations.0.enabled"));
    assert!(Source { app: &app }.flag("integrations.0.livePreview"));
    let value = p4_pending_collection_json(&app, "integrations");
    assert!(value[0].get("enabled").is_none());
    assert!(value[0].get("livePreview").is_none());
    let raw = std::fs::read(&app.state_path).unwrap();
    app.state = CandidateState::load(&app.state_path);
    app.settings_workbench = bridge::Session::from_state(&app.state);
    view::settings::controller::reconcile(&mut app);
    assert!(Source { app: &app }.flag("integrations.0.enabled"));
    assert!(Source { app: &app }.flag("integrations.0.livePreview"));
    assert_eq!(p4_pending_collection_json(&app, "integrations"), value);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), raw);
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Toggle("integrations.0.livePreview".into(), false),
    );
    assert!(!Source { app: &app }.flag("integrations.0.livePreview"));
    assert_eq!(
        p4_pending_collection_json(&app, "integrations")[0]["livePreview"],
        serde_json::json!(false)
    );
}

fn e3_seed_inert_source_motion(app: &mut GpuCandidate) {
    app.state.gallery.variant = None;
    app.state.frontend_preferences.source_motion = toml::from_str(
        r#"enabled = true
shader = "sand-donut"
duration_ms = 55.25
fps = 0.0
preview = true
preview_fps = 29.75
future_source = { note = "keep raw metadata" }
"#,
    )
    .unwrap();
}

#[test]
fn e3_inert_source_motion_load_roundtrip_and_stale_public_messages_never_write() {
    use view::settings::{bridge, composed::keys::transition as T, folio::intent::SettingsMsg};
    for compatibility in [false, true] {
        let mut app = p3_composed_editor_app(SettingsCategory::Motion);
        e3_seed_inert_source_motion(&mut app);
        app.persist_state_outcome().unwrap();
        let saved = app.state.frontend_preferences.clone();
        let raw = std::fs::read(&app.state_path).unwrap();
        assert!(String::from_utf8_lossy(&raw).contains("future_source"));
        app.state = CandidateState::load(&app.state_path);
        assert_eq!(
            app.state.frontend_preferences.source_motion.retained["future_source"]["note"].as_str(),
            Some("keep raw metadata")
        );
        app.settings_workbench = bridge::Session::from_state(&app.state);
        view::settings::controller::reconcile(&mut app);
        assert_eq!(app.state.frontend_preferences, saved);
        assert_eq!(
            std::fs::read(&app.state_path).unwrap(),
            raw,
            "load is read-only"
        );
        let draft = format!("{:?}", app.settings_workbench.draft);
        let generation = app.settings_workbench.generation;
        for path in [
            T::ENABLED,
            T::SHADER,
            T::DURATION_MS,
            T::FPS,
            T::PREVIEW,
            T::PREVIEW_FPS,
            T::FAMILY,
            T::SAND_FPS,
            T::SAND_PRIMARY,
            T::SAND_QUALITY,
            T::SAND_SCOPE,
            T::SAND_SHARP,
            T::SHADER_SCOPES,
            "transition.shaderScopes.random",
            "transition.shaderScopes.sand-donut",
            "transition.shaderScopes.future-shader.child",
        ] {
            for message in [
                SettingsMsg::Input(path.into(), "99".into()),
                SettingsMsg::Toggle(path.into(), false),
                SettingsMsg::Pick(path.into(), "fade".into()),
            ] {
                if compatibility {
                    compatibility_owner_intent(&mut app, message);
                } else {
                    p3_settings_intent(&mut app, message);
                }
                assert_eq!(app.state.frontend_preferences, saved, "{path}");
                assert_eq!(std::fs::read(&app.state_path).unwrap(), raw, "{path}");
                assert_eq!(
                    format!("{:?}", app.settings_workbench.draft),
                    draft,
                    "no staged authoring {path}"
                );
                assert_eq!(app.settings_workbench.generation, generation);
            }
        }
        app.persist_state_outcome().unwrap();
        assert_eq!(
            CandidateState::load(&app.state_path).frontend_preferences,
            saved,
            "unknown metadata survives publication"
        );
    }
}

#[test]
fn e3_retired_family_does_not_publish_or_stage_against_future_primary_or_backup() {
    use view::settings::{composed::keys::transition as T, folio::intent::SettingsMsg};
    for backup_role in [false, true] {
        let mut app = p3_composed_editor_app(SettingsCategory::Motion);
        e3_seed_inert_source_motion(&mut app);
        app.persist_state_outcome().unwrap();
        let backup = app.state_path.with_extension("toml.bak");
        std::fs::copy(&app.state_path, &backup).unwrap();
        std::fs::write(
            if backup_role {
                &backup
            } else {
                &app.state_path
            },
            format!("version={}\n", candidate_state::CURRENT_STATE_VERSION + 1),
        )
        .unwrap();
        let raw = (
            std::fs::read(&app.state_path).unwrap(),
            std::fs::read(&backup).unwrap(),
        );
        let saved = app.state.frontend_preferences.clone();
        compatibility_owner_intent(&mut app, SettingsMsg::Pick(T::FAMILY.into(), "fade".into()));
        assert_eq!(app.state.frontend_preferences, saved);
        assert!(!app.settings_workbench.draft.values.contains_key(T::SHADER));
        assert_eq!(
            (
                std::fs::read(&app.state_path).unwrap(),
                std::fs::read(&backup).unwrap()
            ),
            raw
        );
        assert!(
            app.persist_state_outcome().is_err(),
            "retained publisher still refuses future data"
        );
        assert_eq!(
            (
                std::fs::read(&app.state_path).unwrap(),
                std::fs::read(&backup).unwrap()
            ),
            raw
        );
    }
}

#[test]
fn e3_manual_and_automation_capture_native_config_despite_different_inert_source_motion() {
    let mut app = mutation_target_fixture(false);
    app.wallpapers[0].file_target = app.state_path.with_file_name("e3-owned-still.png");
    ::image::RgbaImage::from_pixel(2, 2, ::image::Rgba([24, 60, 90, 255]))
        .save(&app.wallpapers[0].file_target)
        .unwrap();
    app.catalog = app
        .wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    assert!(usage::media_applicable(&app.wallpapers[0]));
    e3_seed_inert_source_motion(&mut app);
    app.config.transition.effect = transition::TransitionEffect::WipeRight;
    app.config.transition.duration_ms = 1234;
    let expected = toml::to_string(&app.config.transition).unwrap();
    let mut manual = None;
    app.apply_selected_with(
        false,
        || mutation_target_topology(&["DISPLAY-A"]),
        |_, _, _, captured| {
            manual = Some(toml::to_string(&captured).unwrap());
            Err("Captured only; no coordinator dispatch".into())
        },
    );
    assert_eq!(manual.as_deref(), Some(expected.as_str()));
    let mut automated = None;
    app.run_playlist_with(
        None,
        Some(vec!["DISPLAY-A".into()]),
        AutomationTraceContext {
            source: "e3-test",
            generation: None,
            schedule_id: None,
        },
        || mutation_target_topology(&["DISPLAY-A"]),
        |_, _, _, captured| {
            automated = Some(toml::to_string(&captured).unwrap());
            Err("Captured only; no coordinator dispatch".into())
        },
    );
    assert_eq!(automated.as_deref(), Some(expected.as_str()));
    assert!(app.active_operation.is_none() && !app.apply_pending);
}

#[test]
fn wallpaper_transition_save_notes_the_live_fallback_for_incoming_transforming_effects() {
    use view::settings::{
        bridge::{SaveNotice, LIVE_FALLBACK_NOTE},
        composed::{keys::wallpaper_transition as T, transitions},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Motion);
    let config_path = Config::config_path();
    let owned = std::env::var_os("WALLPAPER_SWITCHER_CONFIG_DIR").expect("isolated config fixture");
    assert!(config_path.starts_with(std::path::Path::new(&owned)));
    let backup = config_path.with_extension("toml.bak");
    let previous_config = std::fs::read(&config_path).ok();
    let previous_backup = std::fs::read(&backup).ok();
    let note = |app: &GpuCandidate| SaveNotice::status(app).note;
    let saved = |app: &GpuCandidate| {
        matches!(
            app.settings_workbench.save_notice,
            Some(SaveNotice::Saved("Wallpaper transition"))
                | Some(SaveNotice::BackupWarning("Wallpaper transition"))
        )
    };
    let pick = |app: &mut GpuCandidate, path: &str, value: &str| {
        p3_settings_intent(app, SettingsMsg::Pick(path.into(), value.into()));
    };
    let choose = |app: &mut GpuCandidate, effect: transition::TransitionEffect| {
        pick(app, T::FAMILY, transitions::native_family_of(effect).key());
        pick(app, T::EFFECT, effect.key());
        assert_eq!(app.config.transition.effect, effect);
    };

    for effect in transition::TRANSITION_CATALOG {
        choose(&mut app, effect);
        assert!(
            saved(&app),
            "{effect:?}: {:?}",
            app.settings_workbench.save_notice
        );
        assert_eq!(
            note(&app).as_deref(),
            effect.needs_incoming_frame().then_some(LIVE_FALLBACK_NOTE),
            "{effect:?}"
        );
    }
    pick(&mut app, T::FAMILY, "random");
    assert_eq!(
        app.config.transition.effect,
        transition::TransitionEffect::Random
    );
    assert!(saved(&app));
    assert_eq!(
        note(&app),
        None,
        "Random draws mask effects on live targets"
    );
    choose(&mut app, transition::TransitionEffect::Glitch);
    assert_eq!(note(&app).as_deref(), Some(LIVE_FALLBACK_NOTE));
    p3_settings_intent(&mut app, SettingsMsg::Toggle(T::ENABLED.into(), false));
    assert_eq!(
        app.config.transition.effect,
        transition::TransitionEffect::Off
    );
    assert!(saved(&app));
    assert_eq!(note(&app), None, "Off");

    for family in [
        transitions::Family::Fade,
        transitions::Family::Wipe,
        transitions::Family::Warp,
        transitions::Family::Break,
    ] {
        choose(&mut app, transition::TransitionEffect::Swirl);
        assert_eq!(note(&app).as_deref(), Some(LIVE_FALLBACK_NOTE));
        pick(&mut app, T::FAMILY, family.key());
        let default = transitions::native_default_effect_for_family(family).unwrap();
        assert_eq!(app.config.transition.effect, default, "{family:?}");
        assert_eq!(
            note(&app).as_deref(),
            default.needs_incoming_frame().then_some(LIVE_FALLBACK_NOTE),
            "{family:?} default {default:?}"
        );
    }

    choose(&mut app, transition::TransitionEffect::ZoomBlur);
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(T::DURATION_MS.into(), "900".into()),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(app.config.transition.duration_ms, 900);
    assert_eq!(
        note(&app).as_deref(),
        Some(LIVE_FALLBACK_NOTE),
        "a duration save keeps the flagged effect"
    );
    let launch = match app
        .settings_workbench
        .draft
        .values
        .get(view::settings::composed::keys::launch::ANIMATION)
    {
        Some(view::settings::composed::draft::Value::Text(current)) if current == "zoom" => "rise",
        _ => "zoom",
    };
    pick(
        &mut app,
        view::settings::composed::keys::launch::ANIMATION,
        launch,
    );
    assert!(app.settings_workbench.save_notice.is_some());
    assert!(!saved(&app), "{:?}", app.settings_workbench.save_notice);
    assert_eq!(note(&app), None, "an unrelated save clears the note");

    choose(&mut app, transition::TransitionEffect::Displacement);
    assert_eq!(note(&app).as_deref(), Some(LIVE_FALLBACK_NOTE));
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("playback".into()));
    assert_eq!(app.settings_workbench.save_notice, None);
    assert_eq!(app.settings_workbench.save_note, None);

    for (path, previous) in [(config_path, previous_config), (backup, previous_backup)] {
        if let Some(previous) = previous {
            std::fs::write(path, previous).unwrap();
        } else {
            crate::file_publication::remove_if_present(&path).unwrap();
        }
    }
}

#[test]
fn compatibility_owner_p4_pending_targets_and_exact_scopes_preserve_other_records_restart_and_reject_stale_actions(
) {
    use candidate_state::frontend::{PendingOnly, PendingSetting};
    use serde_json::json;
    use view::settings::{
        bridge::{self, Source},
        composed::{keys::transition as T, ActionId, SettingsSource},
        folio::intent::{Message as Intent, SettingsMsg},
    };
    let (mut app, submissions) = d3_application_fixture(2);
    app.state.gallery.variant = None;
    let config = toml::to_string(&app.config).unwrap();
    let applied = app.state.applied_theme.clone();
    for (root, value) in [
        (
            "theme.targets",
            json!(["future-provider", "noctalia", "future-provider"]),
        ),
        (
            "transition.shaderScopes",
            json!({"iris":"primary","future-shader":"all"}),
        ),
    ] {
        app.state.pending_frontend.settings.push(PendingSetting {
            capability: candidate_state::pending::root_owner(root).unwrap(),
            source_path: root.into(),
            value_json: value.to_string(),
            status: PendingOnly::Pending,
        });
    }
    app.persist_state_outcome().unwrap();
    app.panel_state.open_settings(SettingsCategory::Motion);
    view::settings::controller::reconcile(&mut app);
    compatibility_owner_intent(&mut app, SettingsMsg::Pick(T::FAMILY.into(), "sand".into()));
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Pick(
            "transition.shaderScopes.sand-bloom".into(),
            "primary".into(),
        ),
    );
    assert_eq!(
        p4_pending_collection_json(&app, "transition.shaderScopes"),
        json!({"iris":"primary","future-shader":"all"})
    );
    let old_generation = app.settings_workbench.generation;
    let window = app.window;
    compatibility_owner_intent(&mut app, SettingsMsg::Pick(T::FAMILY.into(), "fade".into()));
    compatibility_owner_intent(&mut app, SettingsMsg::SelectSection(1));
    assert_ne!(app.settings_workbench.generation, old_generation);
    let before = std::fs::read(&app.state_path).unwrap();
    let pending = app.state.pending_frontend.clone();
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation: old_generation,
            intent: Intent::Settings(SettingsMsg::Pick(
                "transition.shaderScopes.sand-bloom".into(),
                "all".into(),
            )),
        },
    );
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Pick("transition.shaderScopes.sand-bloom".into(), "all".into()),
    );
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Pick(
            "transition.shaderScopes.sand-made-up".into(),
            "primary".into(),
        ),
    );
    assert_eq!(app.state.pending_frontend, pending);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), before);
    compatibility_owner_intent(&mut app, SettingsMsg::SelectTab("theme".into()));
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Run(ActionId::SetThemeTarget("noctalia", false)),
    );
    assert_eq!(
        p4_pending_collection_json(&app, "theme.targets"),
        json!(["future-provider", "future-provider"])
    );
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Run(ActionId::SetThemeTarget("dms", true)),
    );
    assert_eq!(
        p4_pending_collection_json(&app, "theme.targets"),
        json!(["future-provider", "future-provider", "dms"])
    );
    let pending = app.state.pending_frontend.clone();
    app.state = CandidateState::load(&app.state_path);
    app.settings_workbench = bridge::Session::from_state(&app.state);
    view::settings::controller::reconcile(&mut app);
    assert_eq!(app.state.pending_frontend, pending);
    assert_eq!(Source { app: &app }.array_len("theme.targets"), 3);
    assert_eq!(Source { app: &app }.text("theme.targets.2"), "dms");
    assert_eq!(
        Source { app: &app }.text("transition.shaderScopes.iris"),
        "primary"
    );
    assert!(app.settings_workbench.message.contains("unfamiliar"));
    assert_eq!(app.state.applied_theme, applied);
    assert_eq!(toml::to_string(&app.config).unwrap(), config);
    assert!(!app.apply_pending && app.active_operation.is_none());
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn compatibility_owner_p4_pending_target_scope_refusal_retains_drafts_and_raw_primary_backup() {
    use view::settings::{
        bridge::Source,
        composed::{ActionId, SettingsSource},
        folio::intent::SettingsMsg,
    };
    for target_operation in [false, true] {
        for backup_role in [false, true] {
            let mut app = p3_composed_editor_app(SettingsCategory::Motion);
            compatibility_owner_intent(
                &mut app,
                SettingsMsg::SelectTab(if target_operation { "theme" } else { "motion" }.into()),
            );
            app.persist_state_outcome().unwrap();
            let backup = app.state_path.with_extension("toml.bak");
            std::fs::copy(&app.state_path, &backup).unwrap();
            std::fs::write(
                if backup_role {
                    &backup
                } else {
                    &app.state_path
                },
                format!("version={}\n", candidate_state::CURRENT_STATE_VERSION + 1),
            )
            .unwrap();
            let raw = (
                std::fs::read(&app.state_path).unwrap(),
                std::fs::read(&backup).unwrap(),
            );
            let pending = app.state.pending_frontend.clone();
            let config = toml::to_string(&app.config).unwrap();
            if target_operation {
                compatibility_owner_intent(
                    &mut app,
                    SettingsMsg::Run(ActionId::SetThemeTarget("noctalia", true)),
                );
                assert_eq!(
                    app.settings_workbench.theme_targets_draft.as_deref(),
                    Some(["noctalia".to_string()].as_slice())
                );
                assert_eq!(Source { app: &app }.text("theme.targets.0"), "noctalia");
            } else {
                compatibility_owner_intent(
                    &mut app,
                    SettingsMsg::Pick("transition.shaderScopes.random".into(), "primary".into()),
                );
                assert_eq!(
                    Source { app: &app }.text("transition.shaderScopes.random"),
                    ""
                );
            }
            assert_eq!(app.state.pending_frontend, pending);
            assert_eq!(
                (
                    std::fs::read(&app.state_path).unwrap(),
                    std::fs::read(&backup).unwrap()
                ),
                raw
            );
            if !target_operation {
                assert!(app.persist_state_outcome().is_err());
            }
            assert!(app.save_failure.is_some() && app.state.write_restriction.is_some());
            assert!(!app.settings_workbench.message.starts_with("Saved"));
            assert_eq!(toml::to_string(&app.config).unwrap(), config);
            assert!(!app.apply_pending && app.active_operation.is_none());
        }
    }
}

#[test]
fn p4_pending_incompatible_target_scope_roots_block_leaf_replacement() {
    use candidate_state::frontend::{PendingOnly, PendingSetting};
    use serde_json::json;
    use view::settings::{bridge, composed::ActionId, folio::intent::SettingsMsg};
    for (root, value) in [
        ("theme.targets", json!(["owned-target", null])),
        (
            "transition.shaderScopes",
            json!({"future-shader":null,"random":"all"}),
        ),
    ] {
        let mut app = p3_composed_editor_app(SettingsCategory::Motion);
        app.state.pending_frontend.settings.push(PendingSetting {
            capability: candidate_state::pending::root_owner(root).unwrap(),
            source_path: root.into(),
            value_json: value.to_string(),
            status: PendingOnly::Pending,
        });
        app.persist_state_outcome().unwrap();
        app.state = CandidateState::load(&app.state_path);
        app.settings_workbench = bridge::Session::from_state(&app.state);
        view::settings::controller::reconcile(&mut app);
        p3_settings_intent(
            &mut app,
            SettingsMsg::SelectTab(
                if root == "theme.targets" {
                    "theme"
                } else {
                    "motion"
                }
                .into(),
            ),
        );
        let before = std::fs::read(&app.state_path).unwrap();
        if root == "theme.targets" {
            p3_settings_intent(
                &mut app,
                SettingsMsg::Run(ActionId::SetThemeTarget("noctalia", true)),
            );
        } else {
            p3_settings_intent(
                &mut app,
                SettingsMsg::Pick("transition.shaderScopes.random".into(), "primary".into()),
            );
        }
        assert_eq!(p4_pending_collection_json(&app, root), value);
        assert_eq!(std::fs::read(&app.state_path).unwrap(), before);
        assert!(app.settings_workbench.message.contains("retained"));
    }
}

#[test]
fn compatibility_owner_p4_recovery_hold_explains_valid_draft_and_requires_explicit_reentry() {
    use view::settings::{composed::keys, folio::intent::SettingsMsg};
    let (mut app, submissions) = d3_application_fixture(2);
    app.state.gallery.variant = None;
    app.panel_state.open_settings(SettingsCategory::Playback);
    view::settings::controller::reconcile(&mut app);
    app.persist_state_outcome().unwrap();
    let backup = app.state_path.with_extension("toml.bak");
    let original = std::fs::read(&app.state_path).unwrap();
    std::fs::write(
        &backup,
        format!("version={}\n", candidate_state::CURRENT_STATE_VERSION + 1),
    )
    .unwrap();
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input(keys::playback::RESUME_DELAY.into(), "3".into()),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    assert!(app.state.write_restriction.is_some());
    std::fs::copy(&backup, backup.with_extension("future-owned.raw")).unwrap();
    std::fs::write(&backup, &original).unwrap();
    let window = app.window;
    let generation = app.settings_workbench.generation;
    let _ = update(
        &mut app,
        Message::SettingsReloadSavedState { window, generation },
    );
    assert!(app.state.write_restriction.is_none());
    let pair = (
        std::fs::read(&app.state_path).unwrap(),
        std::fs::read(&backup).unwrap(),
    );
    let hold = app.settings_workbench.draft.errors[keys::playback::RESUME_DELAY].clone();
    assert!(hold.contains("Re-enter this field") && hold.contains("Escape restores"));
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(app.settings_workbench.message, hold);
    assert_eq!(
        app.settings_workbench.draft.raw_inputs[keys::playback::RESUME_DELAY],
        "3"
    );
    assert_eq!(
        (
            std::fs::read(&app.state_path).unwrap(),
            std::fs::read(&backup).unwrap()
        ),
        pair
    );
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input(keys::playback::RESUME_DELAY.into(), "3".into()),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    assert!(app.settings_workbench.message.starts_with("Saved pending."));
    assert_eq!(
        CandidateState::load(&app.state_path).pending_frontend,
        app.state.pending_frontend
    );
    assert!(app
        .state
        .pending_frontend
        .settings
        .iter()
        .any(
            |setting| setting.source_path == keys::playback::RESUME_DELAY
                && serde_json::from_str::<serde_json::Value>(&setting.value_json)
                    .unwrap()
                    .as_f64()
                    == Some(3.0)
        ));
    assert!(!app
        .settings_workbench
        .draft
        .raw_inputs
        .contains_key(keys::playback::RESUME_DELAY));
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input(keys::playback::RESUME_DELAY.into(), "-".into()),
    );
    let valid_pair = (
        std::fs::read(&app.state_path).unwrap(),
        std::fs::read(&backup).unwrap(),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(app.settings_workbench.message, "Enter a finite number.");
    assert_eq!(
        (
            std::fs::read(&app.state_path).unwrap(),
            std::fs::read(&backup).unwrap()
        ),
        valid_pair
    );
    assert_eq!(submissions.load(Ordering::Acquire), 0);
    assert!(app.active_operation.is_none() && !app.apply_pending);
    assert!(!app.state.slideshow_enabled && app.state.schedules.is_empty());
}

#[test]
fn compatibility_owner_p4_saved_local_recovery_is_bound_read_only_retains_raw_edits_and_rebases_escape(
) {
    use candidate_state::frontend::{DeferredCapability, PendingOnly, PendingSetting};
    use view::settings::{
        bridge::Source,
        composed::{keys, SettingsKey, SettingsSource},
        folio::intent::SettingsMsg,
    };
    for raw in ["2.75", "-"] {
        let (mut app, submissions) = d3_application_fixture(2);
        app.state.gallery.variant = None;
        app.panel_state.open_settings(SettingsCategory::Playback);
        view::settings::controller::reconcile(&mut app);
        app.persist_state_outcome().unwrap();
        let backup = app.state_path.with_extension("toml.bak");
        let original = std::fs::read(&app.state_path).unwrap();
        std::fs::write(
            &backup,
            format!("version={}\n", candidate_state::CURRENT_STATE_VERSION + 1),
        )
        .unwrap();
        compatibility_owner_intent(
            &mut app,
            SettingsMsg::Toggle(keys::playback::PROCESS_ENABLED.into(), true),
        );
        assert!(app.save_failure.is_some() && app.state.write_restriction.is_some());
        compatibility_owner_intent(
            &mut app,
            SettingsMsg::Input(keys::playback::RESUME_DELAY.into(), raw.into()),
        );
        std::fs::copy(&backup, backup.with_extension("future-backup.raw")).unwrap();
        std::fs::write(&backup, &original).unwrap();
        // This owned fixture represents the supported publication found by an
        // explicit reload. Protocol partial-publication faults are tested at
        // the real saver seam; this case exercises the actual app entry point.
        let mut saved = app.state.clone();
        saved.gallery.mode = GalleryKind::Wall;
        saved.frontend_preferences.interface.language = Some("sv-SE".into());
        saved.pending_frontend.settings.push(PendingSetting {
            capability: DeferredCapability::MediaPlaybackPolicy,
            source_path: keys::playback::RESUME_DELAY.into(),
            value_json: "9.25".into(),
            status: PendingOnly::Pending,
        });
        std::fs::write(&app.state_path, toml::to_string_pretty(&saved).unwrap()).unwrap();
        let before = (
            std::fs::read(&app.state_path).unwrap(),
            std::fs::read(&backup).unwrap(),
        );
        let old_generation = app.settings_workbench.generation;
        let window = app.window;
        let old_state = app.state.clone();
        let _ = update(
            &mut app,
            Message::SettingsReloadSavedState {
                window: Some(window::Id::unique()),
                generation: old_generation,
            },
        );
        assert_eq!(
            app.state, old_state,
            "Another window cannot reconcile this owner"
        );
        let _ = update(
            &mut app,
            Message::SettingsReloadSavedState {
                window,
                generation: old_generation,
            },
        );
        assert_eq!(
            (
                std::fs::read(&app.state_path).unwrap(),
                std::fs::read(&backup).unwrap()
            ),
            before
        );
        assert!(
            app.state.write_restriction.is_none()
                && app.state.reconciliation_required.is_none()
                && app.save_failure.is_none()
        );
        assert_eq!(app.gallery_mode, GalleryMode::Wall);
        assert_eq!(app.settings_locale.language.tag, "en-US");
        assert_eq!(
            app.settings_workbench
                .draft
                .raw_inputs
                .get(keys::playback::RESUME_DELAY)
                .map(String::as_str),
            Some(raw)
        );
        assert!(app
            .settings_workbench
            .draft
            .errors
            .contains_key(keys::playback::RESUME_DELAY));
        assert_ne!(app.settings_workbench.generation, old_generation);
        let _ = update(
            &mut app,
            Message::SettingsIntent {
                window,
                generation: old_generation,
                intent: view::settings::folio::intent::Message::Settings(SettingsMsg::Input(
                    keys::playback::RESUME_DELAY.into(),
                    "99".into(),
                )),
            },
        );
        assert_eq!(
            app.settings_workbench
                .draft
                .raw_inputs
                .get(keys::playback::RESUME_DELAY)
                .map(String::as_str),
            Some(raw)
        );
        compatibility_owner_intent(&mut app, SettingsMsg::Commit);
        assert_eq!(
            app.settings_workbench.message,
            app.settings_workbench.draft.errors[keys::playback::RESUME_DELAY],
            "Commit must explain the actual recovery hold instead of calling a valid retained value invalid"
        );
        assert_eq!(
            std::fs::read(&app.state_path).unwrap(),
            before.0,
            "Reload did not authorize an automatic retry"
        );
        compatibility_owner_intent(&mut app, SettingsMsg::Key(SettingsKey::Cancel));
        assert_eq!(
            Source { app: &app }.number(keys::playback::RESUME_DELAY),
            9.25,
            "Escape returns to the reloaded committed number, including a first invalid input"
        );
        assert!(!app
            .settings_workbench
            .draft
            .raw_inputs
            .contains_key(keys::playback::RESUME_DELAY));
        assert_eq!(std::fs::read(&app.state_path).unwrap(), before.0);
        compatibility_owner_intent(
            &mut app,
            SettingsMsg::Input(keys::playback::RESUME_DELAY.into(), "3.5".into()),
        );
        compatibility_owner_intent(&mut app, SettingsMsg::Commit);
        assert_eq!(
            CandidateState::load(&app.state_path).pending_frontend,
            app.state.pending_frontend
        );
        assert_eq!(app.state.gallery.mode, GalleryKind::Wall);
        assert!(!app.state.slideshow_enabled && app.state.schedules.is_empty());
        assert!(app.active_operation.is_none() && !app.apply_pending);
        assert_eq!(submissions.load(Ordering::Acquire), 0);
    }
}

#[test]
fn compatibility_owner_p4_saved_local_recovery_refuses_future_missing_and_newer_nonlocal_or_collection_data(
) {
    use candidate_state::frontend::{DeferredCapability, PendingOnly, PendingSetting};
    use view::settings::{composed::keys, folio::intent::SettingsMsg};
    let raw = |path: &std::path::Path| match std::fs::read(path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => panic!("Unexpected owned fixture read failure: {error}"),
    };
    for case in 0..6 {
        let mut app = p3_composed_editor_app(SettingsCategory::Playback);
        app.persist_state_outcome().unwrap();
        let mut backup = app.state_path.with_extension("toml.bak");
        let original = std::fs::read(&app.state_path).unwrap();
        std::fs::write(
            &backup,
            format!("version={}\n", candidate_state::CURRENT_STATE_VERSION + 1),
        )
        .unwrap();
        compatibility_owner_intent(
            &mut app,
            SettingsMsg::Toggle(keys::playback::PROCESS_ENABLED.into(), true),
        );
        std::fs::copy(&backup, backup.with_extension("future-backup.raw")).unwrap();
        let mut saved = app.state.clone();
        match case {
            0 => {} // a future backup still refuses even with a supported primary
            1 => {
                std::fs::write(&app.state_path, "version=1000000\n").unwrap();
                std::fs::write(&backup, &original).unwrap();
            }
            2 => {
                std::fs::write(&app.state_path, "not a supported TOML profile").unwrap();
                std::fs::write(&backup, "not a supported backup").unwrap();
            }
            3 => {
                // PK-03 explicitly owns timer reload; a newer library record remains nonlocal.
                saved.favorites.insert("newer-nonlocal-favorite".into());
                std::fs::write(&backup, &original).unwrap();
                std::fs::write(&app.state_path, toml::to_string(&saved).unwrap()).unwrap();
            }
            4 => {
                saved.pending_frontend.settings.push(PendingSetting {
                    capability: DeferredCapability::ExpandedScheduling,
                    source_path: "postProcessing".into(),
                    value_json: r#"[{"type":"all","command":"newer literal record"}]"#.into(),
                    status: PendingOnly::Pending,
                });
                std::fs::write(&backup, &original).unwrap();
                std::fs::write(&app.state_path, toml::to_string(&saved).unwrap()).unwrap();
            }
            _ => {}
        }
        if case == 5 {
            // Represent an absent owned pair without deleting retained evidence.
            app.state_path = app.state_path.with_file_name("owned-absent-state.toml");
            backup = app.state_path.with_extension("toml.bak");
            assert!(!app.state_path.exists() && !backup.exists());
        }
        let state = app.state.clone();
        let before = (raw(&app.state_path), raw(&backup));
        let window = app.window;
        let generation = app.settings_workbench.generation;
        let _ = update(
            &mut app,
            Message::SettingsReloadSavedState { window, generation },
        );
        assert_eq!(
            app.state, state,
            "Refusal must keep the existing saved state and protection"
        );
        assert_eq!((raw(&app.state_path), raw(&backup)), before);
        assert!(app.save_failure.is_some());
        assert!(!app
            .settings_workbench
            .message
            .starts_with("Saved local settings reloaded"));
        assert!(!app.state.slideshow_enabled && app.state.schedules.is_empty());
    }
}

#[test]
fn compatibility_owner_p4_saved_local_recovery_accepts_the_published_collection_draft_without_replaying_it(
) {
    use candidate_state::frontend::{DeferredCapability, PendingOnly, PendingSetting};
    use view::settings::{composed::ActionId, folio::intent::SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::Automation);
    app.persist_state_outcome().unwrap();
    let backup = app.state_path.with_extension("toml.bak");
    let original = std::fs::read(&app.state_path).unwrap();
    std::fs::write(
        &backup,
        format!("version={}\n", candidate_state::CURRENT_STATE_VERSION + 1),
    )
    .unwrap();
    compatibility_owner_intent(&mut app, SettingsMsg::Run(ActionId::AddPostCommand));
    assert_eq!(app.settings_workbench.lists.commands.len(), 1);
    std::fs::copy(&backup, backup.with_extension("future-backup.raw")).unwrap();
    std::fs::write(&backup, &original).unwrap();
    let mut published = app.state.clone();
    published.pending_frontend.settings.push(PendingSetting {
        capability: DeferredCapability::ExpandedScheduling,
        source_path: "postProcessing".into(),
        value_json: serde_json::to_string(&app.settings_workbench.lists.commands).unwrap(),
        status: PendingOnly::Pending,
    });
    std::fs::write(&app.state_path, toml::to_string(&published).unwrap()).unwrap();
    let before = (
        std::fs::read(&app.state_path).unwrap(),
        std::fs::read(&backup).unwrap(),
    );
    let window = app.window;
    let generation = app.settings_workbench.generation;
    let _ = update(
        &mut app,
        Message::SettingsReloadSavedState { window, generation },
    );
    assert!(app.save_failure.is_none() && app.state.write_restriction.is_none());
    assert_eq!(
        app.settings_workbench.lists.commands.len(),
        1,
        "Recovery must not run Add again"
    );
    assert_eq!(
        p4_pending_collection_json(&app, "postProcessing"),
        serde_json::json!([{"type":"all","command":""}])
    );
    assert_eq!(
        (
            std::fs::read(&app.state_path).unwrap(),
            std::fs::read(&backup).unwrap()
        ),
        before
    );
    assert!(!app.state.slideshow_enabled && app.state.schedules.is_empty());
}

#[test]
fn compatibility_owner_p4_source_browser_layout_and_visibility_save_restart_without_changing_scan_or_provider(
) {
    use view::settings::{
        bridge::{self, Source},
        composed::{keys, SettingsKey, SettingsSource},
        folio::intent::SettingsMsg,
    };
    let (mut app, submissions) = d3_application_fixture(2);
    app.state.gallery.variant = None;
    app.panel_state.open_settings(SettingsCategory::Library);
    view::settings::controller::reconcile(&mut app);
    let config = toml::to_string(&app.config).unwrap();
    let gallery = app.state.gallery.clone();
    let fields = [
        (keys::selector::WALLHAVEN_COLUMNS, 6.25),
        (keys::selector::WALLHAVEN_ROWS, 3.75),
        (keys::selector::WALLHAVEN_THUMB_WIDTH, 301.125),
        (keys::selector::WALLHAVEN_THUMB_HEIGHT, 169.875),
        (keys::selector::STEAM_COLUMNS, 7.25),
        (keys::selector::STEAM_ROWS, 4.75),
        (keys::selector::STEAM_THUMB_WIDTH, 311.125),
        (keys::selector::STEAM_THUMB_HEIGHT, 171.875),
    ];
    app.persist_state_outcome().unwrap();
    for (path, value) in fields {
        let before = std::fs::read(&app.state_path).unwrap();
        compatibility_owner_intent(&mut app, SettingsMsg::Input(path.into(), value.to_string()));
        assert_eq!(std::fs::read(&app.state_path).unwrap(), before);
        compatibility_owner_intent(&mut app, SettingsMsg::Commit);
        assert_eq!(Source { app: &app }.number(path), value);
    }
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Toggle(keys::features::STEAM.into(), false),
    );
    let pending = app.state.pending_frontend.clone();
    app.state = CandidateState::load(&app.state_path);
    app.settings_workbench = bridge::Session::from_state(&app.state);
    view::settings::controller::reconcile(&mut app);
    for (path, value) in fields {
        assert_eq!(Source { app: &app }.number(path), value);
    }
    assert!(!Source { app: &app }.flag(keys::features::STEAM));
    let before = std::fs::read(&app.state_path).unwrap();
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input(keys::selector::STEAM_COLUMNS.into(), "inf".into()),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    assert!(app
        .settings_workbench
        .draft
        .errors
        .contains_key(keys::selector::STEAM_COLUMNS));
    assert_eq!(std::fs::read(&app.state_path).unwrap(), before);
    compatibility_owner_intent(&mut app, SettingsMsg::Key(SettingsKey::Cancel));
    assert_eq!(
        Source { app: &app }.number(keys::selector::STEAM_COLUMNS),
        7.25
    );
    assert_eq!(app.state.gallery, gallery);
    assert_eq!(app.state.pending_frontend, pending);
    assert_eq!(
        toml::to_string(&app.config).unwrap(),
        config,
        "Real local Workshop/catalog configuration retains its owner"
    );
    assert!(app.active_operation.is_none() && !app.apply_pending);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn compatibility_owner_p4_pending_collection_old_generation_cannot_edit_shifted_index() {
    use view::settings::{
        composed::ActionId,
        folio::intent::{Message as Intent, SettingsMsg},
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Theme);
    compatibility_owner_intent(&mut app, SettingsMsg::SelectTab("theme".into()));
    for _ in 0..2 {
        compatibility_owner_intent(&mut app, SettingsMsg::Run(ActionId::AddIntegration));
    }
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::Input("integrations.1.name".into(), "surviving row".into()),
    );
    compatibility_owner_intent(&mut app, SettingsMsg::Commit);
    let old_generation = app.settings_workbench.generation;
    let window = app.window;
    compatibility_owner_intent(&mut app, SettingsMsg::Run(ActionId::RemoveIntegration(0)));
    assert_ne!(app.settings_workbench.generation, old_generation);
    let before = std::fs::read(&app.state_path).unwrap();
    let pending = app.state.pending_frontend.clone();
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation: old_generation,
            intent: Intent::Settings(SettingsMsg::Input(
                "integrations.0.output".into(),
                "obsolete edit".into(),
            )),
        },
    );
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation: old_generation,
            intent: Intent::Settings(SettingsMsg::Run(ActionId::RemoveIntegration(0))),
        },
    );
    assert_eq!(app.state.pending_frontend, pending);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), before);
    assert_eq!(app.settings_workbench.lists.integrations.len(), 1);
    assert_eq!(
        app.settings_workbench.lists.integrations[0].name,
        "surviving row"
    );
    assert_eq!(app.settings_workbench.lists.integrations[0].output, "");
}

#[test]
fn w1_rail_projection_preserves_gallery_config_and_dismissal() {
    let (mut app, _) = d3_application_fixture(2);
    let before = app.state.gallery.clone();
    app.state.frontend_preferences.filter_bar.visual_style = Some("wall".into());
    app.state.frontend_preferences.filter_bar.always_visible = Some(true);
    app.state.frontend_preferences.visibility.type_video = Some(false);
    app.state.frontend_preferences.visibility.folder = Some(false);
    let m = view::gallery_rail_model(&app);
    assert_eq!(m.mode, donor_rail::RailMode::Wall);
    assert_eq!(m.reveal, 1.0);
    assert!(!m.visibility.sources[2] && !m.visibility.folder);
    assert_eq!(m.show_close, app.config.ui.show_close_button);
    assert!(m.title.is_none());
    for legacy_title in [false, true] {
        app.config.ui.show_title = legacy_title;
        assert!(view::gallery_rail_model(&app).title.is_none());
    }
    app.rail_dismissed = true;
    assert_eq!(view::gallery_rail_model(&app).reveal, 0.0);
    assert_eq!(app.state.gallery, before);
    app.gallery_mode = GalleryMode::Wall;
    app.state.gallery.mode = GalleryKind::Wall;
    app.rail_dismissed = false;
    for viewport in [[1920.0, 1080.0], [2560.0, 1440.0]] {
        app.viewport = viewport;
        for orientation in ["horizontal", "vertical"] {
            app.state.frontend_preferences.filter_bar.orientation = Some(orientation.into());
            app.state.frontend_preferences.filter_bar.visual_style = Some("match".into());
            let baseline = gallery_layout_params_for_app(&app).wall;
            let frame = gpu_gallery::wall_frame(app.viewport, baseline);
            for style in ["slices", "hex", "wall"] {
                app.state.frontend_preferences.filter_bar.visual_style = Some(style.into());
                let params = gallery_layout_params_for_app(&app).wall;
                let actual = gpu_gallery::wall_frame(app.viewport, params);
                assert_eq!(
                    (params.rail_height, params.rail_width),
                    (baseline.rail_height, baseline.rail_width)
                );
                assert_eq!(
                    (actual.center, actual.left, actual.top, actual.clip),
                    (frame.center, frame.left, frame.top, frame.clip),
                    "forced {style}/{orientation} moves Wall geometry"
                );
                let model = gallery_rail_model(&app);
                let size = donor_rail::footprint(&model);
                let at =
                    view::gallery_rail_anchor(&app, &model, app.state.gallery.wall.card_height);
                assert!(at.iter().all(|value| value.is_finite()));
                assert!(size[0] <= viewport[0] && size[1] <= viewport[1]);
                println!("W1_WALL_RAIL_EDGE {style}/{orientation} viewport={viewport:?} rail={at:?}+{size:?} frame_left={} frame_top={}",frame.left,frame.top);
            }
        }
    }
}

#[test]
fn r3_display_projection_keeps_physical_target_and_saved_visibility() {
    let (mut app, submissions) = d3_application_fixture(2);
    app.config.ui.show_monitor_tabs = true;
    app.monitor_ids = vec![
        "MONITOR\\first-physical".into(),
        "MONITOR\\second-physical".into(),
    ];
    app.active_monitor_id = Some(app.monitor_ids[1].clone());
    let target = app.active_monitor_id.clone();
    for (mode, skin) in [
        (GalleryMode::Slices, donor_rail::RailMode::Slices),
        (GalleryMode::Geometric, donor_rail::RailMode::Geometric),
        (GalleryMode::Wall, donor_rail::RailMode::Wall),
        (GalleryMode::Depth, donor_rail::RailMode::Slices),
    ] {
        app.gallery_mode = mode;
        let model = gallery_rail_model(&app);
        assert_eq!(model.mode, skin);
        let footprint = donor_rail::footprint(&model);
        assert_eq!(app.active_monitor_id, target);
        app.active_monitor_id = Some("missing physical target".into());
        assert_eq!(donor_rail::footprint(&gallery_rail_model(&app)), footprint);
        assert_eq!(
            app.active_monitor_id.as_deref(),
            Some("missing physical target")
        );
        app.active_monitor_id.clone_from(&target);
    }
    app.state.frontend_preferences.filter_bar.visual_style = Some("hex".into());
    app.state.frontend_preferences.filter_bar.orientation = Some("vertical".into());
    app.state.frontend_preferences.interface.ui_scale = Some(1.25);
    app.state.frontend_preferences.visibility.colors = Some(false);
    app.state.frontend_preferences.visibility.theme = Some(false);
    app.state.frontend_preferences.visibility.tagcloud = Some(false);
    // Exercise the saved projection, not a second rail-only visibility owner.
    app.state = toml::from_str(&toml::to_string(&app.state).unwrap()).unwrap();
    let model = gallery_rail_model(&app);
    assert_eq!(model.mode, donor_rail::RailMode::Geometric);
    assert!(model.vertical);
    assert_eq!(model.scale, 1.25);
    assert!(!model.visibility.colors && !model.visibility.tagcloud);
    assert_eq!(app.state.frontend_preferences.visibility.theme, Some(false));
    assert_eq!(app.active_monitor_id, target);
    app.viewport = [960.0, 640.0];
    app.gallery_mode = GalleryMode::Slices;
    app.state.frontend_preferences.interface.ui_scale = Some(2.0);
    app.state.frontend_preferences.visibility.colors = Some(true);
    app.state.frontend_preferences.visibility.theme = Some(true);
    app.state.frontend_preferences.visibility.tagcloud = Some(true);
    app.filter_rule
        .folders
        .insert(std::path::PathBuf::from("J:/abcdefghijklmnopqrst"));
    let fitted = gallery_rail_model(&app);
    let at = view::gallery_rail_anchor(&app, &fitted, app.state.gallery.slices.height);
    let size = donor_rail::footprint(&fitted);
    assert!(at[0] + size[0] <= app.viewport[0] && at[1] + size[1] <= app.viewport[1]);
    for viewport in [[1920.0, 1080.0], [2560.0, 1440.0]] {
        app.viewport = viewport;
        app.gallery_mode = GalleryMode::Wall;
        app.state.frontend_preferences.interface.ui_scale = Some(1.25);
        let model = gallery_rail_model(&app);
        let size = donor_rail::footprint(&model);
        let at = view::gallery_rail_anchor(&app, &model, app.state.gallery.wall.card_height);
        assert!(at[0] + size[0] <= viewport[0] + 0.01 && at[1] + size[1] <= viewport[1] + 0.01);
    }
    assert!(
        submissions.load(Ordering::Acquire) == 0,
        "rail presentation never applies wallpaper"
    );
}

#[test]
fn rail_search_and_retained_settings_theme_reach_existing_owners() {
    let (mut app, submissions) = d3_application_fixture(2);
    app.window_focused = Some(true);
    let search_id = app.search_input_id.clone();
    let tag_id = app.local_tag_input_id.clone();
    let _ = update(
        &mut app,
        Message::DonorRail(donor_rail::RailIntent::OpenSearch),
    );
    let _ = update(&mut app, Message::ToggleSearchTags);
    assert!(app.panel_state.search_open && app.search_tags_open);
    assert_eq!(app.search_input_id, search_id);
    assert_eq!(app.local_tag_input_id, tag_id);
    let _ = update(&mut app, Message::ToggleSearchTags);
    assert!(app.panel_state.search_open && !app.search_tags_open);
    let _ = update(&mut app, Message::CloseSearchPanel);
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Theme),
    );
    assert!(app.panel_state.settings_open && app.theme_session.designer_active());
    assert_eq!(
        app.settings_workbench.windows_panel,
        Some(SettingsCategory::Theme)
    );
    let session = app.theme_session.draft_revision();
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Theme),
    );
    assert_eq!(app.theme_session.draft_revision(), session);
    assert!(submissions.load(Ordering::Acquire) == 0);
}

#[test]
fn w1_library_folder_relocation_keeps_current_and_stale_input_ownership() {
    let (mut app, _) = d3_application_fixture(2);
    app.panel_state.open_settings(SettingsCategory::Library);
    view::settings::controller::reconcile(&mut app);
    app.folder_menu_open = true;
    let generation = app.settings_workbench.generation;
    assert!(view::settings::gateway::folder_owns_input(&app));
    assert!(!view::settings::gateway::valid(
        &app, app.window, generation
    ));
    let message = Message::SetFolderFilter(None);
    assert!(view::settings::gateway::page_valid(
        &app, app.window, generation, &message
    ));
    assert!(!view::settings::gateway::page_valid(
        &app,
        app.window,
        generation.wrapping_add(1),
        &message
    ));
}

#[test]
fn w1_unsupported_language_and_absence_round_trip_without_reset() {
    let mut state = CandidateState::default();
    assert_eq!(state.frontend_preferences.interface.language, None);
    let encoded = toml::to_string(&state).unwrap();
    let absent: CandidateState = toml::from_str(&encoded).unwrap();
    assert_eq!(absent.frontend_preferences.interface.language, None);
    state.frontend_preferences.interface.language = Some("sv-SE".into());
    let encoded = toml::to_string(&state).unwrap();
    let decoded: CandidateState = toml::from_str(&encoded).unwrap();
    assert_eq!(
        decoded.frontend_preferences.interface.language.as_deref(),
        Some("sv-SE")
    );
    assert_eq!(
        localization::Catalog::new("sv-SE", "zh-CN")
            .unwrap()
            .language
            .tag,
        "en-US"
    );
    for raw in ["zh-CN", "zh-Hans", "zh-SG"] {
        assert_eq!(
            localization::Catalog::new("auto", raw)
                .unwrap()
                .language
                .tag,
            "zh-CN"
        );
    }
    for raw in ["zh-TW", "zh-Hant", "zh-HK", "zh-MO", "ja-JP"] {
        assert_eq!(
            localization::Catalog::new("auto", raw)
                .unwrap()
                .language
                .tag,
            "en-US"
        );
    }
    for saved_language in [None, Some("sv-SE".to_string())] {
        let (mut app, submissions) = d3_application_fixture(2);
        app.state.frontend_preferences.interface.language = saved_language.clone();
        app.settings_locale =
            localization::Catalog::new(saved_language.as_deref().unwrap_or("auto"), "en-US")
                .unwrap();
        app.persist_state_outcome().unwrap();
        let _ = update(&mut app, Message::CycleAppearanceOpacity);
        assert_eq!(
            CandidateState::load(&app.state_path)
                .frontend_preferences
                .interface
                .language,
            saved_language
        );
        assert_eq!(submissions.load(Ordering::Acquire), 0);
    }
}

#[test]
fn w1_explicit_rail_toggle_respects_owned_modals_and_stale_windows() {
    fn toggle(app: &mut GpuCandidate, event_window: window::Id) {
        let _ = update(
            app,
            Message::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::ArrowUp),
                modifiers: keyboard::Modifiers::SHIFT,
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
    }
    let (mut app, _) = d3_application_fixture(2);
    let current = app.window.unwrap();
    toggle(&mut app, window::Id::unique());
    assert!(!app.rail_dismissed);
    app.controls_panel_open = true;
    toggle(&mut app, current);
    assert!(!app.rail_dismissed);
    app.controls_panel_open = false;
    app.folder_menu_open = true;
    toggle(&mut app, current);
    assert!(!app.rail_dismissed);
    app.folder_menu_open = false;
    toggle(&mut app, current);
    assert!(app.rail_dismissed);
    toggle(&mut app, current);
    assert!(!app.rail_dismissed);
    let (mut close_app, _) = d3_application_fixture(2);
    close_app.config.keybinds.close = "shift+up".into();
    let close_window = close_app.window.unwrap();
    toggle(&mut close_app, close_window);
    assert!(!close_app.rail_dismissed);
    assert!(
        !close_app.visible || close_app.exit_requested,
        "Configured Close retains its original effect"
    );
    let (mut nav_app, _) = d3_application_fixture(2);
    nav_app.config.keybinds.right = "shift+up".into();
    let nav_window = nav_app.window.unwrap();
    let selected = nav_app.selected;
    toggle(&mut nav_app, nav_window);
    assert!(!nav_app.rail_dismissed);
    assert_ne!(
        nav_app.selected, selected,
        "Configured navigation retains its original effect"
    );
}

// These fixtures exercise retained data/reducer/publisher/recovery owners, not W1 public UI.
fn compatibility_owner_model(
    app: &GpuCandidate,
    tab: &str,
) -> Vec<(
    view::settings::composed::Card,
    Vec<view::settings::composed::Row>,
)> {
    view::settings::composed::compatibility_owner_rows(
        tab,
        &view::settings::bridge::Source { app },
        &app.monitor_ids,
        &app.settings_workbench.output_statuses,
        &app.settings_workbench.output_previews,
    )
}
fn compatibility_owner_intent(
    app: &mut GpuCandidate,
    intent: view::settings::folio::intent::SettingsMsg,
) {
    app.settings_workbench.owner_test_model = Some(compatibility_owner_model);
    p3_settings_intent(app, intent);
}

#[test]
fn compatibility_owner_p3_display_search_validation_and_background_editor_use_observed_current_model(
) {
    use view::settings::{
        composed::{self, keys, SettingsSource},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::WallpapersDisplays);
    app.settings_workbench.owner_test_model = Some(compatibility_owner_model);
    // The legacy zero alias reloads as None; use the canonical fixture so the
    // full saved-state equality below tests pending background isolation.
    app.state.gallery.variant = None;
    let monitor = native_wallpaper::NativeMonitor {
        device_id: "display.fixture".into(),
        rect: monitor::DesktopRect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        },
    };
    app.refresh_monitors_with(false, || Ok(vec![monitor]), |_| None);
    app.settings_search = "placement".into();
    view::settings::controller::reconcile(&mut app);
    let status = &app.settings_workbench.output_statuses[0];
    assert_eq!(status.target, "display.fixture");
    assert_eq!(status.logical_size(), (1920, 1080));
    assert_eq!(
        status.kind,
        composed::media::MediaKind::Other("unobserved".into())
    );
    assert!(app
        .settings_workbench
        .results
        .iter()
        .any(|result| result.tab == "displays"));
    let baseline = app.state.clone();
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::BackgroundPicker("display.fixture".into()),
    );
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::BackgroundColor("display.fixture".into(), "#123456".into()),
    );
    assert_eq!(
        view::settings::bridge::Source { app: &app }.text(&format!(
            "{}.display.fixture",
            keys::display::BACKGROUND_COLORS
        )),
        "#123456"
    );
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::BackgroundMode("display.fixture".into(), "blur".into()),
    );
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::BackgroundColor("display.fixture".into(), "#FFFFFF".into()),
    );
    assert_eq!(
        view::settings::bridge::Source { app: &app }.text(&format!(
            "{}.display.fixture",
            keys::display::BACKGROUND_COLORS
        )),
        "#123456"
    );
    compatibility_owner_intent(
        &mut app,
        SettingsMsg::BackgroundPicker("forged.target".into()),
    );
    assert!(!app
        .settings_workbench
        .bar_reveals
        .contains_key("background-picker:forged.target"));
    let mut expected = baseline;
    expected.pending_frontend = app.state.pending_frontend.clone();
    assert_eq!(
        app.state, expected,
        "pending background must not change active executable state"
    );
    let saved = CandidateState::load(&app.state_path);
    assert_eq!(saved, app.state);
    assert_eq!(
        view::settings::pending_projection::checked_root(&saved, "display.backgroundColors")
            .unwrap()
            .unwrap()["display.fixture"],
        "#123456"
    );
    assert_eq!(
        view::settings::pending_projection::checked_root(&saved, "display.backgroundModes")
            .unwrap()
            .unwrap()["display.fixture"],
        "blur"
    );
}

#[test]
fn w1_picker_appearance_uses_real_reducer_save_and_stale_refusal() {
    use view::settings::{
        bridge,
        composed::{ActionId, Control, SettingsSource},
        folio::intent::{Message as Intent, SettingsMsg},
    };
    let (mut app, submissions) = d3_application_fixture(2);
    app.state.gallery.variant = None;
    app.panel_state.open_settings(SettingsCategory::Picker);
    view::settings::controller::reconcile(&mut app);
    assert!(app.settings_workbench.owner_test_model.is_none());
    let model = bridge::build(&app, "picker");
    let (_, rows) = model
        .iter()
        .find(|(card, _)| card.title == "Appearance")
        .expect("real Picker Appearance section");
    let options = rows
        .iter()
        .find_map(|row| match &row.control {
            Control::Chips { path, options, .. } if path == "appearance.presentation" => {
                Some(options)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(
        options,
        &vec![
            ("wallpaper".into(), "Blurred wallpaper".into()),
            ("live".into(), "Live desktop-through".into())
        ]
    );
    let before_gallery = app.state.gallery.clone();
    let before_theme = app.state.applied_theme.clone();
    let generation = app.settings_workbench.generation;
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick("appearance.presentation".into(), "live".into()),
    );
    assert_eq!(
        app.state.appearance.presentation,
        BackgroundPresentation::LiveTransparentOverlay
    );
    assert_eq!(
        CandidateState::load(&app.state_path)
            .appearance
            .presentation,
        BackgroundPresentation::LiveTransparentOverlay
    );
    let _ = view::settings::controller::dispatch(
        &mut app,
        generation,
        Intent::Settings(SettingsMsg::Pick(
            "appearance.presentation".into(),
            "wallpaper".into(),
        )),
    );
    assert_eq!(
        app.state.appearance.presentation,
        BackgroundPresentation::LiveTransparentOverlay
    );
    let generation = app.settings_workbench.generation;
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window: Some(window::Id::unique()),
            generation,
            intent: Intent::Settings(SettingsMsg::Pick(
                "appearance.presentation".into(),
                "wallpaper".into(),
            )),
        },
    );
    assert_eq!(
        app.state.appearance.presentation,
        BackgroundPresentation::LiveTransparentOverlay
    );
    let opacity = app.state.appearance.overlay_opacity;
    p3_settings_intent(&mut app, SettingsMsg::Run(ActionId::CycleBackgroundOpacity));
    assert_ne!(app.state.appearance.overlay_opacity, opacity);
    assert_eq!(
        CandidateState::load(&app.state_path)
            .appearance
            .overlay_opacity,
        app.state.appearance.overlay_opacity
    );
    let backup = app.state_path.with_extension("toml.bak");
    std::fs::write(&backup, "version=5\n").unwrap();
    let primary = std::fs::read(&app.state_path).unwrap();
    let backup_raw = std::fs::read(&backup).unwrap();
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick("appearance.presentation".into(), "wallpaper".into()),
    );
    assert!(app.save_failure.is_some());
    assert_eq!(
        app.settings_workbench.message,
        app.save_failure.clone().unwrap()
    );
    assert_eq!(
        bridge::Source { app: &app }.text("appearance.presentation"),
        "wallpaper"
    );
    assert_eq!(std::fs::read(&app.state_path).unwrap(), primary);
    assert_eq!(std::fs::read(&backup).unwrap(), backup_raw);
    assert_eq!(app.state.gallery, before_gallery);
    assert_eq!(app.state.applied_theme, before_theme);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[path = "tests/library_tools.rs"]
mod library_tools;

#[test]
fn rail_search_closes_by_the_same_button_without_losing_query_or_tags() {
    for preview in [false, true] {
        let (mut app, _) = d3_application_fixture(1);
        if preview {
            app.launch_policy = p1_preview_policy();
        }
        let rail = Message::DonorRail(donor_rail::RailIntent::OpenSearch);
        assert_eq!(update(&mut app, rail.clone()).units(), 1);
        app.filter_rule.text = "untouched".into();
        app.search_tags_open = true;
        assert_eq!(update(&mut app, rail.clone()).units(), 0);
        assert!(!app.panel_state.search_open);
        assert!(app.visible);
        assert_eq!(update(&mut app, rail).units(), 1);
        assert_eq!(app.filter_rule.text, "untouched");
        assert!(app.search_tags_open);
    }
}

#[test]
fn random_rail_feedback_tracks_success_and_ignores_old_expiry() {
    let (mut app, submissions) = d3_application_fixture(1);
    app.launch_policy = p1_preview_policy();
    let rail = Message::DonorRail(donor_rail::RailIntent::ToggleRandomRotation);
    let _ = update(&mut app, rail.clone());
    let (on, label) = app.rotation_notice.unwrap();
    assert_eq!(label, candidate_state::RandomRotationState::On);
    let _ = update(&mut app, rail);
    let (off, label) = app.rotation_notice.unwrap();
    assert_eq!(label, candidate_state::RandomRotationState::Off);
    let _ = update(&mut app, Message::ExpireRotationNotice(on));
    assert_eq!(app.rotation_notice.unwrap().0, off);
    let _ = update(&mut app, Message::ExpireRotationNotice(off));
    assert!(app.rotation_notice.is_none());
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn detail_retains_apply_and_preview_close_still_exits() {
    let (mut app, submissions) = d3_application_fixture(1);
    let actions = view::detail_actions(&app, &app.wallpapers[0]);
    let labels: Vec<_> = actions.iter().map(|a| a.label.as_str()).collect();
    assert!(
        labels.contains(&"Apply")
            && !labels.contains(&"Hide")
            && labels.contains(&"Playlists")
            && labels.contains(&"Show in folder")
    );
    assert!(labels.iter().all(|s| !s.contains("Effects")));
    app.launch_policy = p1_preview_policy();
    let window = app.window.unwrap();
    let _ = update(&mut app, Message::CloseRequested(window));
    assert!(app.visible && app.exit_requested && app.theme_workflow.exit.is_some());
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn e3_motion_index_search_keyboard_and_retained_controls_stay_owned() {
    use view::settings::{
        bridge::Source,
        composed::{self, SettingsFocus, SettingsKey, SettingsSource},
        folio::intent::SettingsMsg,
    };
    for locale in ["en-US", "zh-CN"] {
        let mut app = p3_composed_editor_app(SettingsCategory::Motion);
        app.settings_locale = localization::Catalog::new(locale, locale).unwrap();
        view::settings::controller::reconcile(&mut app);
        assert_eq!(view::settings::controller::cards(&app).len(), 4);
        p3_settings_intent(&mut app, SettingsMsg::SelectSection(3));
        assert_eq!(app.settings_workbench.navigation.section, 3);
        for retired_section in [4, usize::MAX] {
            let before = app.settings_workbench.navigation.section;
            p3_settings_intent(&mut app, SettingsMsg::SelectSection(retired_section));
            assert_eq!(app.settings_workbench.navigation.section, before);
            p3_settings_intent(
                &mut app,
                SettingsMsg::OpenSearchResult("motion".into(), retired_section, 0),
            );
            assert_eq!(app.settings_workbench.navigation.section, before);
        }
        app.settings_workbench.navigation.focus = SettingsFocus::Sections;
        for _ in 0..8 {
            p3_settings_intent(&mut app, SettingsMsg::Key(SettingsKey::Down));
            assert!(app.settings_workbench.navigation.section < 4);
        }
        for (path, raw) in [
            ("motion.fastMs", "300"),
            (composed::keys::selector::FLIP_DURATION_MS, "875"),
        ] {
            p3_settings_intent(&mut app, SettingsMsg::Input(path.into(), raw.into()));
            p3_settings_intent(&mut app, SettingsMsg::Commit);
            assert_eq!(
                Source { app: &app }.number(path),
                raw.parse::<f64>().unwrap()
            );
        }
        p3_settings_intent(
            &mut app,
            SettingsMsg::Pick(composed::keys::launch::ANIMATION.into(), "rise".into()),
        );
        assert_eq!(
            Source { app: &app }.text(composed::keys::launch::ANIMATION),
            "rise"
        );
        assert_eq!(
            CandidateState::load(&app.state_path)
                .frontend_preferences
                .launch
                .animation
                .as_deref(),
            Some("rise")
        );
        for query in ["shaderScopes", "sand-donut", "previewFps"] {
            p3_settings_intent(&mut app, SettingsMsg::SearchInput(query.into()));
            assert!(
                app.settings_workbench.results.is_empty(),
                "{locale}: {query}"
            );
        }
        let transition = if locale == "zh-CN" {
            "过渡"
        } else {
            "transition"
        };
        p3_settings_intent(&mut app, SettingsMsg::SearchInput(transition.into()));
        assert!(app
            .settings_workbench
            .results
            .iter()
            .any(|hit| { hit.tab == "motion" && hit.section == 3 }));
    }
}

#[test]
fn e3_inert_source_metadata_bounds_and_numeric_validation_refuse_without_publication() {
    let mut app = p3_composed_editor_app(SettingsCategory::Motion);
    e3_seed_inert_source_motion(&mut app);
    app.persist_state_outcome().unwrap();
    let before = std::fs::read(&app.state_path).unwrap();
    let saved = app.state.clone();
    for invalid_numeric in [false, true] {
        let mut proposed = saved.clone();
        if invalid_numeric {
            proposed.frontend_preferences.source_motion.duration_ms = Some(f64::NAN);
        } else {
            proposed
                .frontend_preferences
                .source_motion
                .retained
                .insert("oversized".into(), toml::Value::String("x".repeat(65_537)));
        }
        assert!(app.commit_local_state_change(proposed).is_err());
        assert_eq!(std::fs::read(&app.state_path).unwrap(), before);
        assert_eq!(app.state.frontend_preferences, saved.frontend_preferences);
    }
}

#[test]
fn pk02_public_editor_query_snapshot_cycle_reload_and_stale_targets() {
    use crate::candidate_state::preferences::Orientation;
    use view::settings::{
        bridge,
        composed::{keys, ActionId, SettingsSource},
        folio::intent::{Message as Intent, SettingsMsg},
    };
    let (mut app, submissions) = d3_application_fixture(2);
    app.panel_state.open_settings(SettingsCategory::Picker);
    view::settings::controller::reconcile(&mut app);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("filter".into()));
    assert!(app.state.frontend_preferences.resolution_presets.is_empty());
    assert!(!gallery_rail_model(&app).visibility.resolution);
    for band in ["fhd", "qhd", "4k", "5k", "8k", "custom"] {
        p3_settings_intent(
            &mut app,
            SettingsMsg::Run(ActionId::CreateResolutionPresetBand(band)),
        );
    }
    assert_eq!(
        app.state.frontend_preferences.resolution_presets.len(),
        6,
        "mounted production actions publish each band"
    );
    assert!(app.settings_workbench.owner_test_model.is_none());
    let root = keys::filter_bar::RESOLUTION_PRESETS;
    let edit = |app: &mut GpuCandidate, field: &str, value: &str| {
        p3_settings_intent(
            app,
            SettingsMsg::Input(format!("{root}.0.{field}"), value.into()),
        );
        p3_settings_intent(app, SettingsMsg::Commit);
    };
    // Open details through the real visible control before editing its fields.
    p3_settings_intent(&mut app, SettingsMsg::SelectSection(2));
    p3_settings_intent(&mut app, SettingsMsg::ToggleDetails(format!("{root}.0"), 2));
    edit(&mut app, "label", "長い名前 Custom snapshot");
    p3_settings_intent(
        &mut app,
        SettingsMsg::ToggleDetails(format!("{root}.0.custom"), 2),
    );
    for (field, width, height) in [("from", "1080", "1920"), ("to", "1439", "2559")] {
        p3_settings_intent(
            &mut app,
            SettingsMsg::ResolutionInput(format!("{root}.0.{field}"), true, width.into()),
        );
        p3_settings_intent(
            &mut app,
            SettingsMsg::ResolutionInput(format!("{root}.0.{field}"), false, height.into()),
        );
        p3_settings_intent(&mut app, SettingsMsg::Commit);
    }
    for (entry, dims) in app.catalog.iter_mut().zip([(1920, 1080), (2560, 1440)]) {
        entry.width = dims.0;
        entry.height = dims.1;
    }
    app.refresh_filter();
    p3_settings_intent(&mut app, SettingsMsg::Run(ActionId::CycleResolution));
    let active = app.filter_rule.resolution_preset.clone().unwrap();
    assert_eq!(active.label, "長い名前 Custom snapshot");
    assert_eq!(app.filtered_indices, vec![0]);
    assert_eq!(
        bridge::Source { app: &app }.text("filterBar.activeResolution"),
        gallery_rail_model(&app).resolution_label
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::ResolutionLimit(format!("{root}.0.to"), false),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(
        app.state.frontend_preferences.resolution_presets[0].to,
        None
    );
    edit(&mut app, "from", "2560x1440");
    p3_settings_intent(
        &mut app,
        SettingsMsg::ResolutionLimit(format!("{root}.0.to"), true),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(
        app.state.frontend_preferences.resolution_presets[0].to,
        Some((2560, 1440))
    );
    assert_eq!(
        app.filter_rule.resolution_preset,
        Some(active.clone()),
        "range/limit edits retain selected snapshot"
    );
    edit(&mut app, "label", "Changed saved label");
    p3_settings_intent(
        &mut app,
        SettingsMsg::Pick(format!("{root}.0.orientation"), "tall".into()),
    );
    assert_eq!(
        app.state.frontend_preferences.resolution_presets[0].orientation,
        Orientation::Tall
    );
    assert_eq!(app.filter_rule.resolution_preset, Some(active.clone()));
    assert_eq!(
        app.filtered_indices,
        vec![0],
        "editing saved definitions never refilters snapshot"
    );
    let stale_generation = app.settings_workbench.generation;
    p3_settings_intent(
        &mut app,
        SettingsMsg::Run(ActionId::RemoveResolutionPreset(0)),
    );
    let survivor = app.state.frontend_preferences.resolution_presets[0].clone();
    let _ = view::settings::controller::dispatch(
        &mut app,
        stale_generation,
        Intent::Settings(SettingsMsg::Input(
            format!("{root}.0.label"),
            "stale".into(),
        )),
    );
    assert_eq!(
        app.state.frontend_preferences.resolution_presets[0],
        survivor
    );
    assert_eq!(app.filter_rule.resolution_preset, Some(active.clone()));
    assert!(gallery_rail_model(&app)
        .resolution_label
        .contains(&active.label));
    app.filter_rule.orientation = OrientationFilter::Portrait;
    app.refresh_filter();
    assert!(app.filtered_indices.is_empty());
    assert_eq!(app.filter_rule.resolution_preset, Some(active));
    app.state.frontend_preferences.visibility.resolution = Some(false);
    assert!(gallery_rail_model(&app).visibility.resolution);
    p3_settings_intent(&mut app, SettingsMsg::Run(ActionId::ClearResolution));
    assert!(!app.resolution_active());
    app.filter_rule.orientation = OrientationFilter::Any;
    app.refresh_filter();
    assert_eq!(app.filtered_indices.len(), 2);
    let loaded = CandidateState::load(&app.state_path);
    assert_eq!(
        loaded.frontend_preferences.resolution_presets,
        app.state.frontend_preferences.resolution_presets
    );
    assert_eq!(
        submissions.load(Ordering::Acquire),
        0,
        "browse filters/editor never dispatch Apply"
    );
}

#[test]
fn pk02_public_validation_and_publication_refusal_preserve_active_and_saved_bytes() {
    use view::settings::{
        composed::{keys, ActionId},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("filter".into()));
    p3_settings_intent(
        &mut app,
        SettingsMsg::Run(ActionId::CreateResolutionPresetBand("fhd")),
    );
    let root = keys::filter_bar::RESOLUTION_PRESETS;
    p3_settings_intent(&mut app, SettingsMsg::SelectSection(2));
    p3_settings_intent(&mut app, SettingsMsg::ToggleDetails(format!("{root}.0"), 2));
    p3_settings_intent(
        &mut app,
        SettingsMsg::ToggleDetails(format!("{root}.0.custom"), 2),
    );
    let original = app.state.frontend_preferences.resolution_presets.clone();
    let bytes = std::fs::read(&app.state_path).unwrap();
    for value in [
        "0x1080",
        "-1x1080",
        "9223372036854775808x1080",
        "4000x2160",
        "bad",
    ] {
        p3_settings_intent(
            &mut app,
            SettingsMsg::Input(format!("{root}.0.from"), value.into()),
        );
        p3_settings_intent(&mut app, SettingsMsg::Commit);
        assert_eq!(
            app.state.frontend_preferences.resolution_presets, original,
            "invalid {value}"
        );
        assert_eq!(std::fs::read(&app.state_path).unwrap(), bytes);
    }
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(format!("{root}.0.from"), "1920x1080".into()),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    let _ = update(&mut app, Message::CycleResolution);
    let active = app.filter_rule.resolution_preset.clone();
    std::fs::write(app.state_path.with_extension("toml.bak"), "version=5\n").unwrap();
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(format!("{root}.0.label"), "refused".into()),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    assert_eq!(app.state.frontend_preferences.resolution_presets, original);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), bytes);
    assert_eq!(app.filter_rule.resolution_preset, active);
    assert!(app.save_failure.is_some());
    assert_eq!(
        app.settings_workbench.draft.raw_inputs[&format!("{root}.0.label")],
        "refused"
    );
    let stale = app.settings_workbench.generation;
    p3_settings_intent(&mut app, SettingsMsg::Run(ActionId::ClearResolution));
    assert!(
        !app.resolution_active(),
        "explicit browse clear does not depend on draft publication"
    );
    let _ = view::settings::controller::dispatch(
        &mut app,
        stale,
        view::settings::folio::intent::Message::Settings(SettingsMsg::Run(
            ActionId::CycleResolution,
        )),
    );
    assert!(
        !app.resolution_active(),
        "stale browse controls remain refused before early dispatch"
    );
    assert_eq!(std::fs::read(&app.state_path).unwrap(), bytes);
    assert_eq!(
        app.settings_workbench.draft.raw_inputs[&format!("{root}.0.label")],
        "refused"
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(format!("{root}.0.from"), "0x1080".into()),
    );
    p3_settings_intent(&mut app, SettingsMsg::Run(ActionId::CycleResolution));
    assert_eq!(
        app.filter_rule.resolution_preset, active,
        "next uses saved definitions while invalid draft remains"
    );
    assert_eq!(
        app.settings_workbench.draft.raw_inputs[&format!("{root}.0.from")],
        "0x1080"
    );
    assert_eq!(std::fs::read(&app.state_path).unwrap(), bytes);
}
#[test]
fn pk02_public_collection_publication_failure_keeps_saved_query_and_active_snapshot() {
    use candidate_state::{
        commit_test::{self, Fault},
        SaveFailurePoint,
    };
    use view::settings::{composed::ActionId, folio::intent::SettingsMsg};
    let mut app = p3_composed_editor_app(SettingsCategory::CatalogFilters);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("filter".into()));
    p3_settings_intent(
        &mut app,
        SettingsMsg::Run(ActionId::CreateResolutionPresetBand("fhd")),
    );
    let _ = update(&mut app, Message::CycleResolution);
    let active = app.filter_rule.resolution_preset.clone();
    let saved = app.state.frontend_preferences.resolution_presets.clone();
    let bytes = std::fs::read(&app.state_path).unwrap();
    let (_, errors) = commit_test::scoped(
        Fault::Io(
            SaveFailurePoint::PreparePrimary,
            std::io::ErrorKind::PermissionDenied,
        ),
        || {
            p3_settings_intent(
                &mut app,
                SettingsMsg::Run(ActionId::RemoveResolutionPreset(0)),
            );
        },
    );
    assert_eq!(
        errors.len(),
        1,
        "actual production publication fault reached"
    );
    assert_eq!(app.state.frontend_preferences.resolution_presets, saved);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), bytes);
    assert!(
        app.settings_workbench.lists.resolutions.is_empty(),
        "failed draft retained for reconciliation"
    );
    assert_eq!(app.filter_rule.resolution_preset, active);
    assert!(app.save_failure.is_some());
    let _ = update(&mut app, Message::ClearResolution);
    let _ = update(&mut app, Message::CycleResolution);
    assert_eq!(
        app.filter_rule.resolution_preset, active,
        "cycle consumes saved authority, not failed editor draft"
    );
}

#[test]
fn pk02_cycle_saved_order_applicability_normalized_keys_and_legacy_clear() {
    use candidate_state::preferences::{Orientation, ResolutionPreset};
    let (mut app, submissions) = d4a_fixture(2);
    let wide = ResolutionPreset {
        retained: Default::default(),
        label: "wide".into(),
        orientation: Orientation::Wide,
        from: (1920, 1080),
        to: None,
    };
    let tall = ResolutionPreset {
        retained: Default::default(),
        label: "tall".into(),
        orientation: Orientation::Tall,
        from: (1080, 1920),
        to: None,
    };
    let next = ResolutionPreset {
        label: "next".into(),
        from: (2560, 1440),
        ..wide.clone()
    };
    app.state.frontend_preferences.resolution_presets = vec![tall.clone()];
    app.filter_rule.orientation = OrientationFilter::Landscape;
    assert!(
        !gallery_rail_model(&app).visibility.resolution,
        "no applicable idle preset"
    );
    app.filter_rule.orientation = OrientationFilter::Portrait;
    assert!(
        gallery_rail_model(&app).visibility.resolution,
        "applicable saved preset reveals idle control"
    );
    app.state.frontend_preferences.visibility.resolution = Some(false);
    assert!(
        !gallery_rail_model(&app).visibility.resolution,
        "idle switch remains respected"
    );
    app.state.frontend_preferences.visibility.resolution = None;
    app.filter_rule.orientation = OrientationFilter::Any;
    app.state.frontend_preferences.resolution_presets =
        vec![wide.clone(), tall.clone(), next.clone()];
    for expected in [
        Some(wide.clone()),
        Some(tall.clone()),
        Some(next.clone()),
        None,
    ] {
        let _ = update(&mut app, Message::CycleResolution);
        assert_eq!(app.filter_rule.resolution_preset, expected);
    }
    app.filter_rule.orientation = OrientationFilter::Landscape;
    let _ = update(&mut app, Message::CycleResolution);
    assert_eq!(app.filter_rule.resolution_preset, Some(wide.clone()));
    app.state.frontend_preferences.resolution_presets[0].label = "renamed".into();
    app.state.frontend_preferences.resolution_presets[0].from = (1080, 1920);
    assert_eq!(app.filter_rule.resolution_preset, Some(wide));
    let _ = update(&mut app, Message::CycleResolution);
    assert_eq!(app.filter_rule.resolution_preset, Some(next));
    app.filter_rule.orientation = OrientationFilter::Portrait;
    let _ = update(&mut app, Message::CycleResolution);
    assert!(
        app.filter_rule.resolution_preset.is_none(),
        "inapplicable active clears instead of jumping"
    );
    let _ = update(&mut app, Message::CycleResolution);
    assert_eq!(app.filter_rule.resolution_preset, Some(tall));
    app.filter_rule.minimum_width = 777;
    let _ = update(&mut app, Message::CycleResolution);
    assert!(
        !app.resolution_active(),
        "legacy minimum is explicitly clearable"
    );
    app.state.frontend_preferences.resolution_presets.clear();
    let _ = update(&mut app, Message::CycleResolution);
    assert!(
        !app.resolution_active(),
        "no records means no invented cycle defaults"
    );
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}
#[test]
fn pk02_explicit_smart_playlist_capture_reloads_range_and_retained_query_data() {
    use candidate_state::preferences::{Orientation, ResolutionPreset};
    let (mut app, submissions) = d4a_fixture(2);
    for (entry, (width, height)) in app.catalog.iter_mut().zip([(1920, 1080), (2560, 1440)]) {
        entry.width = width;
        entry.height = height;
    }
    let mut preset = ResolutionPreset {
        label: "captured band".into(),
        orientation: Orientation::Wide,
        from: (1920, 1080),
        to: Some((2559, 1439)),
        retained: Default::default(),
    };
    preset.retained.insert(
        "future_snapshot".into(),
        toml::Value::String("inert".into()),
    );
    app.state.frontend_preferences.resolution_presets = vec![preset.clone()];
    app.filter_rule
        .retained
        .insert("future_query".into(), toml::Value::Integer(42));
    let _ = update(&mut app, Message::CycleResolution);
    assert_eq!(app.filtered_indices, vec![0]);
    let _ = update(&mut app, Message::TogglePlaylistPanel);
    let _ = update(
        &mut app,
        Message::PlaylistNameChanged("captured range".into()),
    );
    let _ = update(&mut app, Message::CreatePlaylist { smart: true });
    assert_eq!(app.state.playlists.len(), 1);
    let loaded = CandidateState::load(&app.state_path);
    let playlist = &loaded.playlists[0];
    let PlaylistMembership::Smart { rule } = &playlist.membership else {
        panic!("smart capture")
    };
    assert_eq!(rule.resolution_preset, Some(preset.clone()));
    assert_eq!(rule.retained["future_query"], toml::Value::Integer(42));
    assert_eq!(
        loaded.frontend_preferences.resolution_presets[0].retained,
        preset.retained
    );
    assert_eq!(
        library::evaluate_playlist(playlist, &app.catalog, &loaded.favorites)
            .iter()
            .map(|e| &e.id)
            .collect::<Vec<_>>(),
        vec![&app.catalog[0].id]
    );
    app.state.frontend_preferences.resolution_presets.clear();
    let _ = update(&mut app, Message::ClearResolution);
    assert_eq!(
        library::evaluate_playlist(playlist, &app.catalog, &loaded.favorites).len(),
        1,
        "saved snapshot is independent of definition removal/browse clear"
    );
    let original = std::fs::read(&app.state_path).unwrap();
    let mut invalid = loaded.clone();
    let PlaylistMembership::Smart { rule } = &mut invalid.playlists[0].membership else {
        unreachable!()
    };
    rule.resolution_preset.as_mut().unwrap().from = (0, 1080);
    assert!(
        invalid.save(&app.state_path).is_err(),
        "saved snapshot checked by existing state trust boundary"
    );
    assert_eq!(std::fs::read(&app.state_path).unwrap(), original);
    let invalid_path = app
        .state_path
        .with_file_name("owned-invalid-smart-snapshot.toml");
    assert!(!invalid_path.exists());
    let invalid_bytes = toml::to_string(&invalid).unwrap().into_bytes();
    std::fs::write(&invalid_path, &invalid_bytes).unwrap();
    let diagnosed = CandidateState::load_with_diagnostics(&invalid_path);
    assert!(diagnosed.diagnostic.is_some());
    assert!(
        diagnosed.state.playlists.is_empty(),
        "invalid snapshots are not installed from disk"
    );
    assert_eq!(
        std::fs::read(&invalid_path).unwrap(),
        invalid_bytes,
        "load stays read-only"
    );
    let mut bounded = loaded;
    let PlaylistMembership::Smart { rule } = &mut bounded.playlists[0].membership else {
        unreachable!()
    };
    rule.resolution_preset
        .as_mut()
        .unwrap()
        .retained
        .insert("oversized".into(), toml::Value::String("x".repeat(65_537)));
    assert!(bounded.save(&app.state_path).is_err());
    assert_eq!(std::fs::read(&app.state_path).unwrap(), original);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn pk02_public_duplicate_bands_cycle_unique_predicates_then_clear() {
    use view::settings::{
        composed::{keys, ActionId},
        folio::intent::SettingsMsg,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::CatalogFilters);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("filter".into()));
    for band in ["fhd", "fhd", "qhd"] {
        p3_settings_intent(
            &mut app,
            SettingsMsg::Run(ActionId::CreateResolutionPresetBand(band)),
        );
    }
    p3_settings_intent(&mut app, SettingsMsg::SelectSection(2));
    p3_settings_intent(
        &mut app,
        SettingsMsg::ToggleDetails(format!("{}.1", keys::filter_bar::RESOLUTION_PRESETS), 3),
    );
    p3_settings_intent(
        &mut app,
        SettingsMsg::Input(
            format!("{}.1.label", keys::filter_bar::RESOLUTION_PRESETS),
            "Equivalent duplicate".into(),
        ),
    );
    p3_settings_intent(&mut app, SettingsMsg::Commit);
    let saved = app.state.frontend_preferences.resolution_presets.clone();
    assert_eq!(saved.len(), 3);
    assert_ne!(saved[0].label, saved[1].label);
    assert!(saved[0].same_range(&saved[1]));
    for expected in [Some(saved[0].clone()), Some(saved[2].clone()), None] {
        let _ = update(&mut app, Message::CycleResolution);
        assert_eq!(app.filter_rule.resolution_preset, expected);
    }
    assert_eq!(
        app.state.frontend_preferences.resolution_presets, saved,
        "all editable definitions retained"
    );
}

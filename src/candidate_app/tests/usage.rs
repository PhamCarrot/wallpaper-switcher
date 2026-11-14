//! Injected typed coordinator receipts exercise actual app owners; no provider trial.
use super::*;
use crate::candidate_app::operations::{CapturedWallpaperChange, ChangeOrigin};
use crate::candidate_state::local_library::{ConfirmedAssignment, ItemUsage, RotationPreferences};
use crate::coordinator::{
    ApplyOperationState as State, ApplyResult, ApplySubmission, MonitorApplyOutcome,
    TransitionResult,
};
use view::settings::{composed::SettingsSource, folio::intent::SettingsMsg};

fn fixture(count: usize) -> GpuCandidate {
    let (mut app, _) = d3_application_fixture(count);
    app.apply_enabled = true;
    app.monitor_ids = vec!["DISPLAY-A".into(), "DISPLAY-B".into()];
    app.active_monitor_id = Some("DISPLAY-A".into());
    for (i, item) in app.wallpapers.iter_mut().enumerate() {
        item.file_target = app.state_path.with_file_name(format!("owned-{i}.png"));
        ::image::RgbaImage::from_pixel(2, 2, ::image::Rgba([240, 20, 20, 255]))
            .save(&item.file_target)
            .unwrap();
    }
    app.catalog = app
        .wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    app.selected = (count > 0).then_some(0);
    app.refresh_filter();
    app
}

fn receipt(item: &WallpaperItem, targets: &[String]) -> ApplySubmission {
    ApplySubmission {
        operation_id: coordinator::fixture_operation_id(),
        wallpaper_id: item.id.clone(),
        target_ids: targets.to_vec(),
        state: State::CommandAccepted,
    }
}
fn arm(app: &mut GpuCandidate, all: bool) -> ApplySubmission {
    let mut accepted = None;
    app.apply_selected_with(
        all,
        || mutation_target_topology(&["DISPLAY-A", "DISPLAY-B"]),
        |_, item, target, _| {
            let targets = match target {
                ApplyTarget::DeviceId(id) => vec![id],
                ApplyTarget::DeviceIds(ids) => ids,
                other => panic!("uncaptured target {other:?}"),
            };
            let submission = receipt(&item, &targets);
            accepted = Some(submission.clone());
            Ok(submission)
        },
    );
    accepted.expect("real submission path accepted the controlled receipt")
}
fn result(submission: &ApplySubmission, states: &[State]) -> ApplyResult {
    ApplyResult {
        operation_id: submission.operation_id,
        wallpaper_id: submission.wallpaper_id.clone(),
        outcomes: submission
            .target_ids
            .iter()
            .zip(states)
            .map(|(id, state)| MonitorApplyOutcome {
                target_id: id.clone(),
                state: *state,
                error: None,
            })
            .collect(),
        recoveries: vec![],
        transition: TransitionResult::not_requested(),
    }
}
fn complete(app: &mut GpuCandidate, result: ApplyResult) {
    let _ = update(
        app,
        Message::Wake(Wake::CoordinatorFinished(CoordinatorResult::Apply(result))),
    );
}

#[test]
fn w6_actual_completion_counts_captured_item_once_and_keeps_mixed_recovery_before_exit() {
    let mut app = fixture(2);
    // A real scene/video usage binding needs no stored still/poster.
    app.wallpapers[0].source = WallpaperSource::LocalVideo;
    app.wallpapers[0].wallpaper_type = "video".into();
    app.wallpapers[0].file_target = app.state_path.with_file_name("owned-no-poster.mp4");
    std::fs::write(
        &app.wallpapers[0].file_target,
        b"owned input identity, not a decoder trial",
    )
    .unwrap();
    let submission = arm(&mut app, true);
    assert!(app.wallpaper_change.as_ref().unwrap().still.is_none());
    let item = app.wallpapers[0].clone();
    app.selected = Some(1);
    app.filter_rule.text = "now different".into();
    app.wallpapers[0].file_target = app.state_path.with_file_name("later-catalog-row.mp4");
    let mut mixed = result(&submission, &[State::ProviderConfirmed, State::Unknown]);
    let recovery =
        test_provider_recovery(submission.operation_id.get(), "DISPLAY-B", "owned-mixed");
    mixed.recoveries.push(recovery.clone());
    app.exit_requested = true;
    complete(&mut app, mixed.clone());
    assert_eq!(
        app.state.local_library.usage.items[&item.id].confirmed_count,
        1
    );
    assert_eq!(app.state.local_library.usage.targets.len(), 1);
    assert!(app.state.local_library.usage.targets["DISPLAY-A"].matches(&item));
    assert!(app.provider_state_unknown && !app.apply_pending && app.active_operation.is_none());
    assert!(app.provider_recoveries.contains(&recovery));
    assert!(app.exit_requested);
    assert_eq!(
        CandidateState::load(&app.state_path).local_library.usage,
        app.state.local_library.usage
    );
    complete(&mut app, mixed);
    assert_eq!(
        app.state.local_library.usage.items[&item.id].confirmed_count,
        1
    );
}

#[test]
fn w6_completion_dedupe_multiple_targets_restart_and_reconciliation_are_distinct() {
    let mut app = fixture(1);
    let submission = arm(&mut app, true);
    let confirmed = result(
        &submission,
        &[State::ProviderConfirmed, State::ProviderConfirmed],
    );
    complete(&mut app, confirmed.clone());
    complete(&mut app, confirmed);
    let history = app.state.local_library.usage.clone();
    assert_eq!(history.items[&submission.wallpaper_id].confirmed_count, 1);
    assert_eq!(history.targets.len(), 2);
    let recovery = test_provider_recovery(submission.operation_id.get(), "DISPLAY-B", "replay");
    app.active_operation = Some(submission.operation_id);
    app.apply_pending = true;
    let _ = update(
        &mut app,
        Message::Wake(Wake::CoordinatorFinished(CoordinatorResult::Engine(
            coordinator::EngineControlResult {
                operation_id: submission.operation_id,
                command: EngineCommand::ReconcileProvider { recovery },
                outcomes: vec![MonitorApplyOutcome {
                    target_id: "DISPLAY-B".into(),
                    state: State::ProviderConfirmed,
                    error: None,
                }],
                recoveries: vec![],
            },
        ))),
    );
    assert_eq!(app.state.local_library.usage, history);
    // Reusing a process-local numeric ID in a new actual submission lifetime is a new usage.
    let saved = CandidateState::load(&app.state_path);
    let mut restarted = fixture(1);
    restarted.state = saved;
    restarted.active_operation = Some(submission.operation_id);
    restarted.apply_pending = true;
    restarted.wallpaper_change = CapturedWallpaperChange::new(
        restarted.wallpapers[0].clone(),
        &submission,
        1,
        ChangeOrigin::Manual,
    );
    complete(
        &mut restarted,
        result(
            &submission,
            &[State::ProviderConfirmed, State::ProviderConfirmed],
        ),
    );
    assert_eq!(
        restarted.state.local_library.usage.items[&submission.wallpaper_id].confirmed_count,
        2
    );
}

#[test]
fn w6_nonconfirmation_wrong_authority_and_controls_never_create_history() {
    for state in [
        State::CommandAccepted,
        State::Unknown,
        State::Failed,
        State::Cancelled,
    ] {
        let mut app = fixture(1);
        let submission = arm(&mut app, false);
        complete(&mut app, result(&submission, &[state]));
        assert!(app.state.local_library.usage.items.is_empty(), "{state:?}");
        assert!(app.state.local_library.usage.targets.is_empty());
    }
    for wrong in ["operation", "item", "target"] {
        let mut app = fixture(1);
        let submission = arm(&mut app, false);
        let mut completion = result(&submission, &[State::ProviderConfirmed]);
        match wrong {
            "operation" => completion.operation_id = coordinator::fixture_operation_id(),
            "item" => completion.wallpaper_id = "unrelated".into(),
            _ => completion.outcomes[0].target_id = "UNRELATED-DISPLAY".into(),
        }
        complete(&mut app, completion);
        assert!(app.state.local_library.usage.items.is_empty());
        assert!(
            app.apply_pending && app.wallpaper_change.is_some(),
            "retain actual operation on {wrong}"
        );
    }
    let mut app = fixture(1);
    let operation_id = coordinator::fixture_operation_id();
    app.active_operation = Some(operation_id);
    app.apply_pending = true;
    let _ = update(
        &mut app,
        Message::Wake(Wake::CoordinatorFinished(CoordinatorResult::Engine(
            coordinator::EngineControlResult {
                operation_id,
                command: EngineCommand::SetMuted(true),
                outcomes: vec![MonitorApplyOutcome {
                    target_id: "DISPLAY-A".into(),
                    state: State::ProviderConfirmed,
                    error: None,
                }],
                recoveries: vec![],
            },
        ))),
    );
    assert!(app.state.global_muted && app.state.local_library.usage.items.is_empty());
}

#[test]
fn w6_history_failed_and_partial_publication_remain_truthful_and_never_retry_provider() {
    use candidate_state::commit_test::{self, Fault};
    for fault in [
        Fault::Io(
            candidate_state::SaveFailurePoint::PreparePrimary,
            std::io::ErrorKind::PermissionDenied,
        ),
        Fault::PublishedThenError,
    ] {
        let mut app = fixture(1);
        let submission = arm(&mut app, false);
        let before = std::fs::read(&app.state_path).unwrap();
        let ((), errors) = commit_test::scoped(fault, || {
            complete(&mut app, result(&submission, &[State::ProviderConfirmed]))
        });
        assert_eq!(errors.len(), 1);
        assert!(!app.apply_pending && app.wallpaper_change.is_none());
        assert_eq!(
            app.state.local_library.usage.items[&submission.wallpaper_id].confirmed_count,
            1
        );
        assert!(app
            .save_failure
            .as_deref()
            .unwrap()
            .contains("not fully durable"));
        assert!(app.status.as_deref().unwrap().contains("confirmed"));
        if matches!(fault, Fault::PublishedThenError) {
            assert!(app.state.reconciliation_required.is_some());
            assert_eq!(
                CandidateState::load(&app.state_path)
                    .local_library
                    .usage
                    .items[&submission.wallpaper_id]
                    .confirmed_count,
                1
            );
        } else {
            assert_eq!(std::fs::read(&app.state_path).unwrap(), before);
        }
        complete(&mut app, result(&submission, &[State::ProviderConfirmed]));
        assert_eq!(
            app.state.local_library.usage.items[&submission.wallpaper_id].confirmed_count,
            1
        );
    }
    let mut app = fixture(1);
    let submission = arm(&mut app, false);
    let future = b"# owned newer future state\nversion = 999\n";
    std::fs::write(&app.state_path, future).unwrap();
    complete(&mut app, result(&submission, &[State::ProviderConfirmed]));
    assert_eq!(std::fs::read(&app.state_path).unwrap(), future);
    assert!(app.save_failure.is_some());
}

fn run_rotation(app: &mut GpuCandidate, playlist: Option<&str>) -> Option<ApplySubmission> {
    let mut chosen = None;
    app.run_playlist_with(
        playlist,
        Some(vec!["DISPLAY-A".into()]),
        AutomationTraceContext {
            generation: None,
            source: "w6-fixture",
            schedule_id: None,
        },
        || mutation_target_topology(&["DISPLAY-A", "DISPLAY-B"]),
        |_, item, target, _| {
            let ApplyTarget::DeviceIds(ids) = target else {
                panic!("stable targets")
            };
            let submission = receipt(&item, &ids);
            chosen = Some(submission.clone());
            Ok(submission)
        },
    );
    chosen
}

#[test]
fn w6_real_automation_pool_ignores_picker_and_legacy_pending_but_respects_media_favourites_and_success(
) {
    let mut app = fixture(3);
    app.filter_rule.text = "never matches".into();
    app.colours.query.bucket = Some(crate::library::color_metadata::ColorBucket::Green);
    app.refresh_filter();
    assert!(app.filtered_indices.is_empty());
    app.state
        .frontend_preferences
        .desired_policy
        .random_include_static = Some(false);
    app.state
        .frontend_preferences
        .desired_policy
        .random_include_favourites = Some(true);
    app.state.local_library.usage.last_successful_random_id = Some(app.wallpapers[0].id.clone());
    app.selected = Some(2);
    let submission = run_rotation(&mut app, None).unwrap();
    assert_ne!(submission.wallpaper_id, app.wallpapers[0].id);
    assert_eq!(
        app.wallpaper_change.as_ref().unwrap().origin,
        ChangeOrigin::Random
    );
    complete(&mut app, result(&submission, &[State::Unknown]));
    assert_eq!(
        app.state
            .local_library
            .usage
            .last_successful_random_id
            .as_deref(),
        Some(app.wallpapers[0].id.as_str())
    );
    app.provider_state_unknown = false;
    app.state.local_library.rotation = Some(RotationPreferences {
        favourites_only: true,
        ..Default::default()
    });
    assert!(run_rotation(&mut app, None).is_none());
    app.state.favorites.insert(app.wallpapers[2].id.clone());
    let singleton = run_rotation(&mut app, None).unwrap();
    assert_eq!(singleton.wallpaper_id, app.wallpapers[2].id);
    complete(&mut app, result(&singleton, &[State::ProviderConfirmed]));
    assert_eq!(
        app.state
            .local_library
            .usage
            .last_successful_random_id
            .as_deref(),
        Some(singleton.wallpaper_id.as_str())
    );
    let repeated = run_rotation(&mut app, None).unwrap();
    assert_eq!(repeated.wallpaper_id, singleton.wallpaper_id);
    complete(&mut app, result(&repeated, &[State::Failed]));
    app.state.local_library.rotation.as_mut().unwrap().images = false;
    assert!(run_rotation(&mut app, None).is_none());
}

#[test]
fn w6_rotation_kinds_playlist_priority_and_existing_guards_are_real() {
    let mut app = fixture(3);
    app.wallpapers[1].source = WallpaperSource::LocalVideo;
    app.wallpapers[1].wallpaper_type = "video".into();
    app.wallpapers[2].source = WallpaperSource::WallpaperEngine;
    app.wallpapers[2].wallpaper_type = "scene".into();
    app.wallpapers[2].project_json_path = app.state_path.with_file_name("scene.json");
    std::fs::write(&app.wallpapers[2].project_json_path, b"{}").unwrap();
    app.state.local_library.rotation = Some(RotationPreferences {
        images: false,
        videos: true,
        we: false,
        ..Default::default()
    });
    let video = run_rotation(&mut app, None).unwrap();
    assert_eq!(video.wallpaper_id, app.wallpapers[1].id);
    complete(&mut app, result(&video, &[State::Failed]));
    app.state.local_library.rotation.as_mut().unwrap().videos = false;
    app.state.local_library.rotation.as_mut().unwrap().we = true;
    let we = run_rotation(&mut app, None).unwrap();
    assert_eq!(we.wallpaper_id, app.wallpapers[2].id);
    complete(&mut app, result(&we, &[State::Failed]));
    app.state.playlists.push(candidate_state::Playlist {
        id: "priority".into(),
        name: "Priority".into(),
        membership: candidate_state::PlaylistMembership::Explicit {
            wallpaper_ids: vec![app.wallpapers[0].id.clone()],
        },
        order: candidate_state::PlaybackOrder::Sequential,
    });
    let playlist = run_rotation(&mut app, Some("priority")).unwrap();
    assert_eq!(playlist.wallpaper_id, app.wallpapers[0].id);
    assert_eq!(
        app.wallpaper_change.as_ref().unwrap().origin,
        ChangeOrigin::Playlist
    );
    complete(&mut app, result(&playlist, &[State::Failed]));
    for guard in [
        "lock", "hidden", "disabled", "holdoff", "pending", "recovery", "missing",
    ] {
        let mut guarded = fixture(1);
        let id = guarded.wallpapers[0].id.clone();
        match guard {
            "lock" => {
                guarded.state.locked_monitors.insert("DISPLAY-A".into());
            }
            "hidden" => {
                guarded
                    .config
                    .excluded_wallpapers
                    .insert("Monitor0".into(), vec![id]);
            }
            "disabled" => {
                guarded
                    .config
                    .disabled_wallpapers
                    .insert("global".into(), vec![id]);
            }
            "holdoff" => guarded.state.manual_holdoff_until_unix = u64::MAX,
            "pending" => guarded.apply_pending = true,
            "recovery" => guarded.provider_state_unknown = true,
            _ => {
                std::fs::remove_file(&guarded.wallpapers[0].file_target).unwrap();
            }
        }
        assert!(run_rotation(&mut guarded, None).is_none(), "guard {guard}");
    }
}

#[test]
fn w6_startup_new_opt_in_is_separate_from_saved_timers_and_refuses_invalid_or_preview_inputs() {
    let mut app = fixture(1);
    app.state
        .frontend_preferences
        .desired_policy
        .restore_on_startup = Some(true);
    app.state.slideshow_enabled = true;
    app.state.local_library.usage.targets.insert(
        "DISPLAY-A".into(),
        ConfirmedAssignment::capture(&app.wallpapers[0]),
    );
    app.prepare_startup_reapply_with(|_, _, _, _| panic!("legacy pending startup cannot dispatch"));
    assert!(app.state.slideshow_enabled && !app.state.local_library.startup.reapply_enabled);
    for reason in [
        "preview", "target", "input", "mismatch", "locked", "recovery", "refused",
    ] {
        let mut app = fixture(1);
        app.state.local_library.startup.reapply_enabled = true;
        app.state.local_library.usage.targets.insert(
            "DISPLAY-A".into(),
            ConfirmedAssignment::capture(&app.wallpapers[0]),
        );
        match reason {
            "preview" => app.launch_policy = p1_preview_policy(),
            "target" => app.monitor_ids.clear(),
            "input" => {
                std::fs::remove_file(&app.wallpapers[0].file_target).unwrap();
            }
            "mismatch" => app.wallpapers[0].wallpaper_type = "other".into(),
            "locked" => {
                app.state.locked_monitors.insert("DISPLAY-A".into());
            }
            "recovery" => app.provider_state_unknown = true,
            _ => {
                app.state.write_restriction = Some(candidate_state::StateWriteRestriction {
                    path: app.state_path.clone(),
                    version: 999,
                })
            }
        }
        app.prepare_startup_reapply_with(|_, _, _, _| panic!("startup dispatched under {reason}"));
        assert!(
            !app.apply_pending && app.startup_queue.is_empty(),
            "{reason}"
        );
        assert!(app.status.as_deref().unwrap().contains("Startup reapply"));
        if reason == "preview" {
            let generation = app.automation_generation.load(Ordering::Acquire);
            app.state.slideshow_enabled = true;
            app.refresh_automation_timer();
            assert!(app.automation_generation.load(Ordering::Acquire) > generation);
            app.handle_automation_tick(generation, 1, 1, 1);
            assert!(!app.apply_pending);
        }
    }
    let mut app = fixture(1);
    app.state.local_library.startup.reapply_enabled = true;
    app.state.local_library.usage.targets.insert(
        "DISPLAY-A".into(),
        ConfirmedAssignment::capture(&app.wallpapers[0]),
    );
    app.selected = None;
    app.prepare_startup_reapply_with(|_, item, target, _| {
        assert!(matches!(&target, ApplyTarget::DeviceId(id) if id == "DISPLAY-A"));
        Ok(receipt(&item, &["DISPLAY-A".into()]))
    });
    assert!(
        app.apply_pending && app.wallpaper_change.as_ref().unwrap().origin == ChangeOrigin::Startup
    );
}

#[test]
fn w6_new_policy_controls_save_restart_without_activating_legacy_or_dispatching_preview() {
    let mut app = p3_composed_editor_app(SettingsCategory::Automation);
    app.launch_policy = p1_preview_policy();
    app.apply_enabled = true;
    app.state
        .frontend_preferences
        .desired_policy
        .restore_on_startup = Some(true);
    app.state
        .frontend_preferences
        .desired_policy
        .random_include_static = Some(false);
    assert!(!view::settings::bridge::Source { app: &app }.flag("restoreOnStartup"));
    assert!(app.automation_policy_status().contains("not activated"));
    p3_settings_intent(
        &mut app,
        SettingsMsg::Toggle("general.randomIncludeVideo".into(), false),
    );
    let policy = app.state.local_library.rotation.as_ref().unwrap();
    assert!(!policy.images && !policy.videos && policy.we && !policy.favourites_only);
    assert!(!app.state.local_library.startup.reapply_enabled);
    p3_settings_intent(
        &mut app,
        SettingsMsg::Toggle("restoreOnStartup".into(), true),
    );
    assert!(
        CandidateState::load(&app.state_path)
            .local_library
            .startup
            .reapply_enabled
    );
    app.prepare_startup_reapply_with(|_, _, _, _| panic!("preview never dispatches saved policy"));
    assert!(app.state.local_library.usage.items.is_empty() && !app.apply_pending);
    assert_eq!(
        app.state
            .frontend_preferences
            .desired_policy
            .restore_on_startup,
        Some(true)
    );
}

#[test]
fn w6_actual_history_sort_uses_known_counts_times_stable_ties_and_composed_colour_query() {
    let mut app = fixture(4);
    let ids: Vec<_> = app.wallpapers.iter().map(|i| i.id.clone()).collect();
    for (i, count, time) in [(0, 2, 10), (1, 5, 20), (2, 5, 20)] {
        app.state.local_library.usage.items.insert(
            ids[i].clone(),
            ItemUsage {
                confirmed_count: count,
                last_confirmed_unix: time,
                retained: Default::default(),
            },
        );
    }
    for order in [SortOrder::Recent] {
        app.sort_order = order;
        app.refresh_filter();
        assert_eq!(app.filtered_indices, [1, 2, 0, 3]);
        assert!(sort_label(order).contains("local confirmed"));
    }
    app.colours.query.bucket = Some(crate::library::color_metadata::ColorBucket::Red);
    app.refresh_filter();
    assert!(
        app.filtered_indices.is_empty(),
        "unknown colour still excluded while history sort active"
    );
    app.colours.query = Default::default();
    app.filter_rule.text = ids[0].clone();
    app.refresh_filter();
    assert_eq!(app.filtered_indices, [0]);
    let mut state = app.state.clone();
    state
        .local_library
        .usage
        .items
        .get_mut(&ids[0])
        .unwrap()
        .confirmed_count = u64::MAX;
    state.local_library.retained.insert(
        "future-local".into(),
        toml::Value::String("retained".into()),
    );
    let raw = toml::to_string(&state).unwrap();
    let roundtrip: CandidateState = toml::from_str(&raw).unwrap();
    assert_eq!(roundtrip.local_library, state.local_library);
}

#[test]
fn w6_saved_timer_resumption_is_normal_only_and_independent_of_reapply() {
    use crate::candidate_app::automation_integration::automation_timer_enabled;
    let mut app = fixture(1);
    assert!(!automation_timer_enabled(Default::default(), &app.state));
    app.state.slideshow_enabled = true;
    app.persist_state_outcome().unwrap();
    let mut saved = CandidateState::load(&app.state_path);
    assert!(!saved.local_library.startup.reapply_enabled);
    assert!(automation_timer_enabled(Default::default(), &saved));
    assert!(!automation_timer_enabled(p1_preview_policy(), &saved));
    let developer =
        runtime::parse_launch_policy(&["app.exe".into(), "--screenshot".into()]).unwrap();
    assert!(!automation_timer_enabled(developer, &saved));
    saved.slideshow_enabled = false;
    saved.local_library.startup.reapply_enabled = true;
    assert!(!automation_timer_enabled(Default::default(), &saved));
}

#[test]
fn w6_saturation_duplicate_target_and_unrelated_confirmation_do_not_inflate_history() {
    let mut app = fixture(1);
    let submission = arm(&mut app, false);
    app.state.local_library.usage.items.insert(
        submission.wallpaper_id.clone(),
        ItemUsage {
            confirmed_count: u64::MAX,
            last_confirmed_unix: 1,
            retained: Default::default(),
        },
    );
    let mut completion = result(&submission, &[State::ProviderConfirmed]);
    completion.outcomes.push(completion.outcomes[0].clone());
    completion.outcomes.push(MonitorApplyOutcome {
        target_id: "UNRELATED-DISPLAY".into(),
        state: State::ProviderConfirmed,
        error: None,
    });
    complete(&mut app, completion);
    assert_eq!(
        app.state.local_library.usage.items[&submission.wallpaper_id].confirmed_count,
        u64::MAX
    );
    assert_eq!(app.state.local_library.usage.targets.len(), 1);
    assert!(app
        .state
        .local_library
        .usage
        .targets
        .contains_key("DISPLAY-A"));
    assert_eq!(
        CandidateState::load(&app.state_path).local_library.usage,
        app.state.local_library.usage
    );
}

#[test]
fn w6_rotation_current_display_order_owns_exclusions_and_new_target_identity() {
    let mut app = fixture(1);
    let id = app.wallpapers[0].id.clone();
    app.config
        .excluded_wallpapers
        .insert("Monitor1".into(), vec![id]);
    let trace = AutomationTraceContext {
        generation: None,
        source: "w6-current-order",
        schedule_id: None,
    };
    assert!(!app.run_playlist_with(
        None,
        Some(vec!["DISPLAY-A".into()]),
        trace,
        || mutation_target_topology(&["DISPLAY-B", "DISPLAY-A"]),
        |_, _, _, _| panic!("A is current Monitor1; its exclusion must block dispatch")
    ));
    assert_eq!(app.monitor_ids, ["DISPLAY-B", "DISPLAY-A"]);
    app.config.excluded_wallpapers.clear();
    assert!(app.run_playlist_with(
        None,
        Some(vec!["DISPLAY-C".into()]),
        trace,
        || mutation_target_topology(&["DISPLAY-A", "DISPLAY-C"]),
        |_, item, target, _| {
            assert_eq!(target, ApplyTarget::DeviceIds(vec!["DISPLAY-C".into()]));
            Ok(receipt(&item, &["DISPLAY-C".into()]))
        }
    ));
    assert_eq!(app.monitor_ids, ["DISPLAY-A", "DISPLAY-C"]);
    assert_eq!(
        app.wallpaper_change.as_ref().unwrap().target_ids,
        ["DISPLAY-C"]
    );
}

#[test]
fn w6_catalog_loaded_and_later_status_refresh_retain_startup_refusal() {
    let mut app = fixture(1);
    app.launch_policy = p1_preview_policy();
    app.state.local_library.startup.reapply_enabled = true;
    let generation = app.catalog_generation.load(Ordering::Acquire);
    let wallpapers = app.wallpapers.clone();
    let entries = app.catalog.clone();
    let _ = update(
        &mut app,
        Message::Wake(Wake::CatalogLoaded {
            generation,
            wallpapers,
            entries,
        }),
    );
    assert!(app.startup_checked && !app.apply_pending);
    assert!(app
        .startup_status
        .as_deref()
        .unwrap()
        .contains("blocked in --preview"));
    let visible = app.status_for_display().unwrap();
    assert!(visible.contains("blocked in --preview"));
    assert!(
        visible.contains("Poster preview worker was unavailable"),
        "the actual fixture resource error must also survive: {visible}"
    );
    app.update_catalog_status();
    assert!(app
        .status_for_display()
        .unwrap()
        .contains("blocked in --preview"));
    assert!(app.state.local_library.usage.items.is_empty());
}

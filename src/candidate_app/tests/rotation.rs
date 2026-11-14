//! PK-03 production owners with an owned TEMP profile, clock and provider boundaries.
use super::*;
use crate::coordinator::{
    ApplyOperationState as State, ApplyResult, ApplySubmission, MonitorApplyOutcome,
    TransitionResult,
};
use candidate_state::{AutomationIntent, RandomRotationState as Random};
use view::settings::composed::SettingsSource;

fn fixture() -> (GpuCandidate, Arc<std::sync::atomic::AtomicUsize>) {
    let (mut app, calls) = d3_application_fixture(3);
    app.apply_enabled = true;
    for (index, item) in app.wallpapers.iter_mut().enumerate() {
        item.file_target = app
            .state_path
            .with_file_name(format!("pk03-owned-{index}.png"));
        image::RgbaImage::from_pixel(2, 2, image::Rgba([150, 50, 100, 255]))
            .save(&item.file_target)
            .unwrap();
    }
    app.catalog = app
        .wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    app.state.playlists.push(candidate_state::Playlist {
        id: "owned".into(),
        name: "Owned playlist / 長い名前".into(),
        order: PlaybackOrder::Sequential,
        membership: PlaylistMembership::Explicit {
            wallpaper_ids: vec![app.wallpapers[0].id.clone()],
        },
    });
    app.state.automation_intent = Some(AutomationIntent::default());
    (app, calls)
}
fn complete(app: &mut GpuCandidate, submission: &ApplySubmission, state: State) {
    let _ = update(
        app,
        Message::Wake(Wake::CoordinatorFinished(CoordinatorResult::Apply(
            ApplyResult {
                operation_id: submission.operation_id,
                wallpaper_id: submission.wallpaper_id.clone(),
                outcomes: submission
                    .target_ids
                    .iter()
                    .map(|id| MonitorApplyOutcome {
                        target_id: id.clone(),
                        state,
                        error: None,
                    })
                    .collect(),
                recoveries: vec![],
                transition: TransitionResult::not_requested(),
            },
        ))),
    );
}
fn run(app: &mut GpuCandidate, playlist: Option<&str>, fail: bool) -> Option<ApplySubmission> {
    let generation = app.automation_generation.load(Ordering::Acquire);
    let mut chosen = None;
    app.run_playlist_with(
        playlist,
        None,
        AutomationTraceContext {
            generation: Some(generation),
            source: "pk03-fixture",
            schedule_id: None,
        },
        || mutation_target_topology(&["DISPLAY-A", "DISPLAY-B", "DISPLAY-C"]),
        |_, item, target, _| {
            if fail {
                return Err("owned refused submission".into());
            }
            let ApplyTarget::DeviceIds(target_ids) = target else {
                panic!("stable multi-target request")
            };
            let submission = ApplySubmission {
                operation_id: coordinator::fixture_operation_id(),
                wallpaper_id: item.id,
                target_ids,
                state: State::CommandAccepted,
            };
            chosen = Some(submission.clone());
            Ok(submission)
        },
    );
    chosen
}

#[test]
fn pk03_independent_start_toggle_stop_full_intervals_and_explicit_recovery() {
    let (mut app, calls) = fixture();
    let interval = app.state.slideshow_interval_seconds;
    app.start_playlist("owned".into(), 1000);
    assert!(app.state.automation_intent().playlist_playing);
    assert_eq!(app.random_rotation_state(), Random::Off);
    assert_eq!(app.next_slideshow_unix, 1000 + interval);
    app.set_random_rotation(true, 1001);
    assert_eq!(app.random_rotation_state(), Random::PausedForPlaylist);
    assert_eq!(app.next_slideshow_unix, 1000 + interval);
    app.set_random_rotation(false, 1002);
    assert!(app.state.automation_intent().playlist_playing);
    assert_eq!(app.next_slideshow_unix, 1000 + interval);
    app.set_random_rotation(true, 1003);
    app.stop_playlist(1010);
    assert_eq!(app.random_rotation_state(), Random::On);
    assert!(!app.state.automation_intent().playlist_playing);
    assert_eq!(app.next_slideshow_unix, 1010 + interval);
    app.safely_stop_automation();
    app.stop_playlist(1020);
    assert_eq!(app.random_rotation_state(), Random::SafelyStopped);
    app.set_random_rotation(false, 1021);
    assert!(!app.state.automation_intent().playlist_playing);
    assert!(app.state.automation_intent().safely_stopped);
    app.set_random_rotation(true, 1030);
    assert_eq!(app.random_rotation_state(), Random::On);
    assert_eq!(app.next_slideshow_unix, 1030 + interval);
    let saved = CandidateState::load(&app.state_path);
    assert_eq!(saved.automation_intent(), app.state.automation_intent());
    assert!(!saved.local_library.startup.reapply_enabled);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(!app.apply_pending && app.wallpaper_change.is_none());
}

#[test]
fn pk03_random_pool_and_targets_ignore_browsing_playlist_and_picker() {
    let (mut app, calls) = fixture();
    app.start_playlist("owned".into(), 1);
    app.set_random_rotation(true, 2);
    app.stop_playlist(3);
    app.filter_rule.text = "no matches".into();
    app.source_filter = SourceFilter::Videos;
    app.colours.query.bucket = Some(library::color_metadata::ColorBucket::Green);
    app.active_monitor_id = Some("DISPLAY-C".into());
    app.state.locked_monitors.insert("DISPLAY-B".into());
    app.state.local_library.rotation = Some(candidate_state::local_library::RotationPreferences {
        favourites_only: true,
        ..Default::default()
    });
    let independent_id = app.wallpapers[2].id.clone();
    app.state.favorites.insert(independent_id.clone());
    app.refresh_filter();
    assert!(app.filtered_indices.is_empty());
    let submission = run(&mut app, None, false).expect("independent random request");
    assert_eq!(submission.wallpaper_id, independent_id);
    assert_eq!(submission.target_ids, ["DISPLAY-A", "DISPLAY-C"]);
    assert_eq!(
        app.wallpaper_change.as_ref().unwrap().origin,
        operations::ChangeOrigin::Random
    );
    complete(&mut app, &submission, State::ProviderConfirmed);
    assert_eq!(
        app.state
            .local_library
            .usage
            .last_successful_random_id
            .as_deref(),
        Some(independent_id.as_str())
    );
    assert_eq!(
        calls.load(Ordering::SeqCst),
        0,
        "injected coordinator receipt, never desktop dispatch"
    );
}

#[test]
fn pk03_failures_missing_data_and_unknown_stop_without_fallback_or_replay() {
    for reason in [
        "submit",
        "missing-list",
        "empty-list",
        "missing-file",
        "unknown",
        "failed",
    ] {
        let (mut app, calls) = fixture();
        app.set_random_rotation(true, 1);
        app.start_playlist("owned".into(), 2);
        match reason {
            "missing-list" => app.state.playlists.clear(),
            "empty-list" => {
                app.state.playlists[0].membership = PlaylistMembership::Explicit {
                    wallpaper_ids: vec![],
                }
            }
            "missing-file" => {
                std::fs::remove_file(&app.wallpapers[0].file_target).unwrap();
            }
            _ => {}
        }
        let submitted = run(&mut app, Some("owned"), reason == "submit");
        if let Some(submission) = submitted {
            assert!(matches!(reason, "unknown" | "failed"));
            complete(
                &mut app,
                &submission,
                if reason == "unknown" {
                    State::Unknown
                } else {
                    State::Failed
                },
            );
        }
        assert_eq!(
            app.random_rotation_state(),
            Random::SafelyStopped,
            "{reason}"
        );
        assert!(!app.state.automation_intent().playlist_playing, "{reason}");
        assert!(
            app.state.automation_intent().random_enabled,
            "intent preserved: {reason}"
        );
        let generation = app.automation_generation.load(Ordering::Acquire);
        let before = app.state.clone();
        consume_automation_tick(&mut app, generation.wrapping_sub(1), 9999, 2, 480);
        assert_eq!(app.state, before, "stale tick: {reason}");
        app.stop_playlist(100);
        assert_eq!(
            app.random_rotation_state(),
            Random::SafelyStopped,
            "Stop preserves failure latch: {reason}"
        );
        app.reload_local_state().unwrap();
        assert_eq!(
            app.random_rotation_state(),
            Random::SafelyStopped,
            "reload: {reason}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn pk03_failed_publication_cannot_start_and_non_durable_stop_survives_reload() {
    let (mut app, calls) = fixture();
    app.set_random_rotation(true, 1);
    let future = b"# owned future backup\nversion=999\n";
    let backup = app.state_path.with_extension("toml.bak");
    std::fs::write(&backup, future).unwrap();
    let before = app.state.clone();
    let generation = app.automation_generation.load(Ordering::Acquire);
    let due = app.next_slideshow_unix;
    app.start_playlist("owned".into(), 500);
    assert_eq!(app.state.automation_intent(), before.automation_intent());
    assert_eq!(
        app.automation_generation.load(Ordering::Acquire),
        generation
    );
    assert_eq!(app.next_slideshow_unix, due);
    app.safely_stop_automation();
    assert!(app.automation_suspended);
    assert_eq!(app.random_rotation_state(), Random::SafelyStopped);
    assert_eq!(std::fs::read(&backup).unwrap(), future);
    assert!(
        app.reload_local_state().is_err(),
        "future backup keeps existing reload refusal"
    );
    assert_eq!(app.random_rotation_state(), Random::SafelyStopped);
    app.set_random_rotation(true, 600);
    assert_eq!(
        app.random_rotation_state(),
        Random::SafelyStopped,
        "refused resume remains suspended"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn pk03_stale_completion_cannot_stop_new_explicit_owner() {
    let (mut app, calls) = fixture();
    app.start_playlist("owned".into(), 1);
    let old = run(&mut app, Some("owned"), false).unwrap();
    let old_generation = app
        .wallpaper_change
        .as_ref()
        .unwrap()
        .automation_generation
        .unwrap();
    complete(&mut app, &old, State::ProviderConfirmed);
    app.start_playlist("owned".into(), 2);
    assert_ne!(
        old_generation,
        app.automation_generation.load(Ordering::Acquire)
    );
    complete(&mut app, &old, State::Failed);
    assert!(app.state.automation_intent().playlist_playing);
    assert!(!app.state.automation_intent().safely_stopped);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn pk03_busy_holdoff_preview_and_locks_preserve_guards_without_latching() {
    for reason in ["busy", "holdoff", "preview", "locked", "startup"] {
        let (mut app, calls) = fixture();
        app.start_playlist("owned".into(), 1);
        match reason {
            "busy" => app.apply_pending = true,
            "holdoff" => app.state.manual_holdoff_until_unix = u64::MAX,
            "preview" => app.launch_policy = p1_preview_policy(),
            "locked" => app.state.locked_monitors.extend([
                "DISPLAY-A".into(),
                "DISPLAY-B".into(),
                "DISPLAY-C".into(),
            ]),
            _ => app.startup_queue.push_back((
                "DISPLAY-A".into(),
                candidate_state::local_library::ConfirmedAssignment::capture(&app.wallpapers[0]),
            )),
        }
        assert!(run(&mut app, Some("owned"), false).is_none(), "{reason}");
        assert!(
            !app.state.automation_intent().safely_stopped,
            "guard isn't failure: {reason}"
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn pk03_legacy_versions_partial_new_records_and_missing_owner_reload_truthfully() {
    let (app, _) = fixture();
    for version in [None, Some(0), Some(1), Some(2), Some(3), Some(4)] {
        for enabled in [false, true] {
            for active in [false, true] {
                for new_record in [
                    "",
                    "[automation_intent]\n",
                    "[automation_intent]\nrandom_enabled=false\nplaylist_playing=false\n",
                ] {
                    let header = version
                        .map(|v| format!("version={v}\n"))
                        .unwrap_or_default();
                    let raw = format!("{header}slideshow_enabled={enabled}\n{}[[playlists]]\nid='owned'\nname='Owned'\norder='sequential'\nmembership={{membership='explicit',wallpaper_ids=[]}}\n{new_record}", if active { "active_playlist_id='owned'\n" } else { "" });
                    let path = app.state_path.with_file_name("legacy-pk03.toml");
                    // Use a fresh owned path each iteration so prior migration snapshots are never overwritten.
                    let path = path.with_file_name(format!(
                        "{}-{}-{}-{}.toml",
                        version.unwrap_or(99),
                        enabled,
                        active,
                        new_record.len()
                    ));
                    std::fs::write(&path, &raw).unwrap();
                    let loaded = CandidateState::load(&path);
                    assert_eq!(std::fs::read(&path).unwrap(), raw.as_bytes());
                    let intent = loaded.automation_intent();
                    assert_eq!(
                        intent.random_enabled,
                        new_record.is_empty() && enabled && !active
                    );
                    assert_eq!(
                        intent.playlist_playing,
                        new_record.is_empty() && enabled && active
                    );
                    assert!(!intent.safely_stopped);
                    assert!(!loaded.local_library.startup.reapply_enabled);
                    loaded.save(&path).unwrap();
                    assert_eq!(CandidateState::load(&path).automation_intent(), intent);
                }
            }
        }
    }
    for raw in [
        "slideshow_enabled=true\nactive_playlist_id='missing'\n",
        "[automation_intent]\nrandom_enabled=true\nplaylist_playing=true\n",
    ] {
        let path = app
            .state_path
            .with_file_name(format!("missing-{}.toml", raw.len()));
        std::fs::write(&path, raw).unwrap();
        let loaded = CandidateState::load(&path);
        assert!(loaded.automation_intent().safely_stopped);
        assert!(!loaded.automation_intent().playlist_playing);
        assert_ne!(loaded.random_rotation_state(), Random::On);
    }
}

#[test]
fn pk03_saved_projection_and_recovery_actions_agree_with_intent() {
    let (mut app, calls) = fixture();
    for (random, playing, stopped, expected) in [
        (false, false, false, Random::Off),
        (true, false, false, Random::On),
        (true, true, false, Random::PausedForPlaylist),
        (true, false, true, Random::SafelyStopped),
    ] {
        app.state.active_playlist_id = Some("owned".into());
        app.state.automation_intent = Some(AutomationIntent {
            random_enabled: random,
            playlist_playing: playing,
            safely_stopped: stopped,
            ..Default::default()
        });
        let source = view::settings::bridge::Source { app: &app };
        assert_eq!(source.flag("general.randomRotate"), random);
        assert_eq!(
            source.flag("pk03.canResumeRandom"),
            expected == Random::SafelyStopped
        );
        assert_eq!(source.flag("pk03.playlistPlaying"), playing);
        assert!(source.text("w6.policyStatus").contains(expected.label()));
        assert_eq!(app.random_rotation_state(), expected);
        let path = app
            .state_path
            .with_file_name(format!("projection-{random}-{playing}-{stopped}.toml"));
        app.state.save(&path).unwrap();
        assert_eq!(
            CandidateState::load(&path).random_rotation_state(),
            expected
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn pk03_partial_publication_never_installs_or_rearms_and_failed_stop_projects_effective_activity() {
    use candidate_state::commit_test::{self, Fault};
    for fault in [
        Fault::Io(
            candidate_state::SaveFailurePoint::PreparePrimary,
            std::io::ErrorKind::PermissionDenied,
        ),
        Fault::PublishedThenError,
    ] {
        let (mut app, calls) = fixture();
        let before = app.state.automation_intent();
        let ((), errors) = commit_test::scoped(fault, || app.start_playlist("owned".into(), 100));
        assert_eq!(errors.len(), 1);
        assert_eq!(app.state.automation_intent(), before);
        assert!(!app.playlist_playing());
        assert!(!app.apply_pending);
        if matches!(fault, Fault::PublishedThenError) {
            assert!(app.state.reconciliation_required.is_some());
            assert!(
                CandidateState::load(&app.state_path)
                    .automation_intent()
                    .playlist_playing
            );
            app.reload_local_state().unwrap();
            assert!(
                !app.playlist_playing(),
                "a partly published start never activates through reload"
            );
            assert!(app.automation_suspended);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
    let (mut app, calls) = fixture();
    app.set_random_rotation(true, 1);
    app.start_playlist("owned".into(), 2);
    let ((), errors) = commit_test::scoped(
        Fault::Io(
            candidate_state::SaveFailurePoint::PreparePrimary,
            std::io::ErrorKind::PermissionDenied,
        ),
        || app.safely_stop_automation(),
    );
    assert_eq!(errors.len(), 1);
    assert!(
        app.state.automation_intent().playlist_playing,
        "raw prior saved intent is retained truthfully"
    );
    assert!(
        !app.playlist_playing(),
        "runtime suspension overrides saved activity"
    );
    assert_eq!(app.random_rotation_state(), Random::SafelyStopped);
    let source = view::settings::bridge::Source { app: &app };
    assert!(!source.flag("pk03.playlistPlaying"));
    assert!(source.flag("pk03.canResumeRandom"));
    app.set_random_rotation(true, 100);
    assert_eq!(app.random_rotation_state(), Random::On);
    assert!(
        !app.playlist_playing(),
        "Resume random cannot revive an uncertain playlist"
    );
    assert_eq!(
        app.next_slideshow_unix,
        100 + app.state.slideshow_interval_seconds
    );
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn pk03_selected_schedule_claims_failed_turn_and_missing_playlist_never_becomes_random() {
    let mut fixture = AutomationFixture::new();
    fixture.state.automation_intent = Some(AutomationIntent {
        random_enabled: true,
        ..Default::default()
    });
    fixture.pending = true;
    fixture.deliver(AutomationFixture::tick(7));
    assert_eq!(
        fixture.effects[2], 1,
        "busy selected schedule must not fall through to random"
    );
    assert_eq!(fixture.due, 900);
    assert!(fixture.submissions.is_empty());
    fixture.state.schedules[0].enabled = false;
    fixture.state.active_playlist_id = None;
    fixture
        .state
        .automation_intent
        .as_mut()
        .unwrap()
        .playlist_playing = true;
    fixture.deliver(AutomationFixture::tick(7));
    assert_eq!(
        fixture.effects[2], 1,
        "missing required playlist cannot dispatch independent pool"
    );
    assert!(fixture.state.automation_intent().safely_stopped);
}

#[test]
fn pk03_stale_start_and_stop_controls_cannot_replace_a_newer_owner() {
    let (mut app, calls) = fixture();
    let old_generation = app.automation_generation.load(Ordering::Acquire);
    app.start_playlist("owned".into(), 1);
    let before = app.state.clone();
    let due = app.next_slideshow_unix;
    let _ = update(
        &mut app,
        Message::StopPlaylist {
            generation: old_generation,
        },
    );
    let _ = update(
        &mut app,
        Message::SetActivePlaylist("owned".into(), old_generation),
    );
    assert_eq!(app.state, before);
    assert_eq!(app.next_slideshow_unix, due);
    assert!(app.playlist_playing());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}
#[test]
fn pk03_explicit_record_combinations_unknown_fields_and_absence_are_authoritative() {
    let (app, _) = fixture();
    for random in [false, true] {
        for playing in [false, true] {
            for stopped in [false, true] {
                for legacy in [false, true] {
                    let path = app
                        .state_path
                        .with_file_name(format!("new-{random}-{playing}-{stopped}-{legacy}.toml"));
                    let raw=format!("version=4\nslideshow_enabled={legacy}\nactive_playlist_id='owned'\n[[playlists]]\nid='owned'\nname='Owned'\norder='sequential'\nmembership={{membership='explicit',wallpaper_ids=[]}}\n[automation_intent]\nrandom_enabled={random}\nplaylist_playing={playing}\nsafely_stopped={stopped}\n[automation_intent.unknown]\nvalue='preserve'\n");
                    std::fs::write(&path, &raw).unwrap();
                    let loaded = CandidateState::load(&path);
                    let intent = loaded.automation_intent();
                    assert_eq!(
                        (
                            intent.random_enabled,
                            intent.playlist_playing,
                            intent.safely_stopped
                        ),
                        (random, playing, stopped)
                    );
                    assert!(intent.retained.contains_key("unknown"));
                    assert_eq!(std::fs::read(&path).unwrap(), raw.as_bytes());
                    loaded.save(&path).unwrap();
                    assert_eq!(CandidateState::load(&path).automation_intent(), intent);
                    assert!(!loaded.local_library.startup.reapply_enabled);
                }
            }
        }
    }
    for (fields, random, playing) in [
        ("random_enabled=true", true, false),
        ("playlist_playing=true", false, true),
        ("safely_stopped=true", false, false),
    ] {
        let path = app
            .state_path
            .with_file_name(format!("partial-{}.toml", fields.len()));
        let raw=format!("slideshow_enabled=true\nactive_playlist_id='owned'\n[[playlists]]\nid='owned'\nname='Owned'\norder='sequential'\nmembership={{membership='explicit',wallpaper_ids=[]}}\n[automation_intent]\n{fields}\n");
        std::fs::write(&path, &raw).unwrap();
        let loaded = CandidateState::load(&path);
        let intent = loaded.automation_intent();
        assert_eq!(
            (intent.random_enabled, intent.playlist_playing),
            (random, playing)
        );
        assert!(!loaded.local_library.startup.reapply_enabled);
    }
}

#[test]
fn pk03_stale_dispatch_and_observation_failure_obey_generation_and_safety() {
    let (mut app, calls) = fixture();
    app.set_random_rotation(true, 1);
    let generation = app.automation_generation.load(Ordering::Acquire);
    assert!(!app.run_playlist_with(
        None,
        None,
        AutomationTraceContext {
            generation: Some(generation.wrapping_sub(1)),
            source: "pk03-stale",
            schedule_id: None
        },
        || panic!("stale dispatch cannot enumerate"),
        |_, _, _, _| panic!("stale dispatch cannot submit")
    ));
    assert!(!app.run_playlist_with(
        None,
        None,
        AutomationTraceContext {
            generation: Some(generation),
            source: "pk03-observation",
            schedule_id: None
        },
        || Err("owned missing topology".into()),
        |_, _, _, _| panic!("missing topology cannot submit")
    ));
    assert_eq!(app.random_rotation_state(), Random::SafelyStopped);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn pk03_active_deletion_preserves_random_intent_stops_and_retains_disabled_schedule_records() {
    use playlists::Edit;
    let (mut app, calls) = fixture();
    app.set_random_rotation(true, 1);
    app.start_playlist("owned".into(), 2);
    app.state.schedules.push(candidate_state::Schedule {
        id: "linked".into(),
        name: "Keep settings".into(),
        enabled: true,
        playlist_id: "owned".into(),
        days: 42,
        minute_of_day: 321,
        priority: 7,
        creation_order: 9,
        monitor_ids: vec!["DISPLAY-A".into()],
    });
    app.state.save(&app.state_path).unwrap();
    app.playlist_panel_open = true;
    app.playlist_picker = false;
    let media = app.wallpapers[0].file_target.clone();
    let bytes = std::fs::read(&media).unwrap();
    let owner = app.playlist_owner().unwrap();
    let _ = update(
        &mut app,
        Message::PlaylistEdit(owner, Edit::DeleteRequest("owned".into())),
    );
    let owner = app.playlist_owner().unwrap();
    let _ = update(&mut app, Message::PlaylistEdit(owner, Edit::DeleteCancel));
    assert!(app.playlist_playing());
    let owner = app.playlist_owner().unwrap();
    let _ = update(
        &mut app,
        Message::PlaylistEdit(owner, Edit::DeleteRequest("owned".into())),
    );
    let owner = app.playlist_owner().unwrap();
    let generation = app.automation_generation.load(Ordering::Acquire);
    let _ = update(&mut app, Message::PlaylistEdit(owner, Edit::DeleteConfirm));
    assert_eq!(app.random_rotation_state(), Random::SafelyStopped);
    assert!(app.state.automation_intent().random_enabled);
    assert!(!app.playlist_playing());
    assert!(app.state.active_playlist_id.is_none());
    assert!(app.state.playlists.is_empty());
    assert_eq!(app.state.schedules.len(), 1);
    assert!(!app.state.schedules[0].enabled);
    assert_eq!(
        (
            app.state.schedules[0].days,
            app.state.schedules[0].minute_of_day,
            app.state.schedules[0].priority
        ),
        (42, 321, 7)
    );
    assert_eq!(std::fs::read(&media).unwrap(), bytes);
    assert_eq!(
        CandidateState::load(&app.state_path).automation_intent(),
        app.state.automation_intent()
    );
    consume_automation_tick(&mut app, generation, 9999, 2, 480);
    assert!(!app.apply_pending);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn pk03_reload_still_refuses_unrelated_playlist_or_schedule_changes() {
    for change in ["playlist", "schedule"] {
        let (mut app, calls) = fixture();
        app.set_random_rotation(true, 1);
        let mut disk = app.state.clone();
        if change == "playlist" {
            disk.playlists[0].name = "External newer list".into();
        } else {
            disk.schedules.push(candidate_state::Schedule {
                id: "external".into(),
                ..Default::default()
            });
        }
        disk.save(&app.state_path).unwrap();
        let before = app.state.clone();
        assert!(app.reload_local_state().is_err(), "{change}");
        assert_eq!(app.state, before);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn pk03_pending_operation_failure_survives_timer_or_pre_failure_intent_changes() {
    for mutation in [
        "random-off",
        "interval",
        "schedule-refresh",
        "start-before-failure",
        "stop-before-failure",
    ] {
        for state in [State::Failed, State::CommandAccepted] {
            let (mut app, calls) = fixture();
            app.set_random_rotation(true, 1);
            app.start_playlist("owned".into(), 2);
            let submission = run(&mut app, Some("owned"), false).unwrap();
            let timer = app.automation_generation.load(Ordering::Acquire);
            match mutation {
                "random-off" => app.set_random_rotation(false, 3),
                "interval" => app.set_automation_interval(600, 3),
                "schedule-refresh" => app.refresh_automation_timer(),
                "start-before-failure" => app.start_playlist("owned".into(), 3),
                _ => app.stop_playlist(3),
            }
            assert_ne!(app.automation_generation.load(Ordering::Acquire), timer);
            complete(&mut app, &submission, state);
            assert!(!app.playlist_playing(), "{mutation}/{state:?}");
            assert!(
                app.state.automation_intent().safely_stopped,
                "active physical request failed: {mutation}/{state:?}"
            );
            let generation = app.automation_generation.load(Ordering::Acquire);
            consume_automation_tick(&mut app, generation.wrapping_sub(1), 9999, 2, 480);
            assert!(!app.apply_pending);
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
    }
}

#[test]
fn pk03_missing_required_target_completion_is_a_safety_stop() {
    let (mut app, calls) = fixture();
    app.set_random_rotation(true, 1);
    let submission = run(&mut app, None, false).unwrap();
    let result = ApplyResult {
        operation_id: submission.operation_id,
        wallpaper_id: submission.wallpaper_id.clone(),
        outcomes: vec![MonitorApplyOutcome {
            target_id: submission.target_ids[0].clone(),
            state: State::ProviderConfirmed,
            error: None,
        }],
        recoveries: vec![],
        transition: TransitionResult::not_requested(),
    };
    let _ = update(
        &mut app,
        Message::Wake(Wake::CoordinatorFinished(CoordinatorResult::Apply(result))),
    );
    assert_eq!(app.random_rotation_state(), Random::SafelyStopped);
    assert!(!app.apply_pending);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn pk03_safety_stop_is_idempotent_and_explicit_stop_preserves_refused_durable_latch() {
    use candidate_state::commit_test::{self, Fault};
    let (mut app, calls) = fixture();
    app.set_random_rotation(true, 1);
    app.start_playlist("owned".into(), 2);
    app.state.schedules.push(candidate_state::Schedule {
        id: "unrelated-poll".into(),
        enabled: true,
        ..Default::default()
    });
    let ((), errors) = commit_test::scoped(
        Fault::Io(
            candidate_state::SaveFailurePoint::PreparePrimary,
            std::io::ErrorKind::PermissionDenied,
        ),
        || app.safely_stop_automation(),
    );
    assert_eq!(
        errors.len(),
        1,
        "first safety stop attempts durable publication"
    );
    assert!(app.automation_suspended);
    assert!(!app.playlist_playing());
    assert!(
        !app.state.automation_intent().safely_stopped,
        "raw prior saved state retained on refusal"
    );
    let generation = app.automation_generation.load(Ordering::Acquire);
    let primary = std::fs::read(&app.state_path).unwrap();
    let ((), errors) = commit_test::scoped(
        Fault::Io(
            candidate_state::SaveFailurePoint::PreparePrimary,
            std::io::ErrorKind::PermissionDenied,
        ),
        || app.safely_stop_automation(),
    );
    assert!(
        errors.is_empty(),
        "already suspended must not retry publication"
    );
    assert_eq!(
        app.automation_generation.load(Ordering::Acquire),
        generation,
        "no immediate timer restart while schedules retain normal polling"
    );
    assert_eq!(std::fs::read(&app.state_path).unwrap(), primary);
    app.stop_playlist(100);
    let fresh = CandidateState::load(&app.state_path);
    assert!(
        fresh.automation_intent().safely_stopped,
        "explicit Stop cannot durably erase prior session failure"
    );
    assert!(!fresh.automation_intent().playlist_playing);
    assert!(fresh.automation_intent().random_enabled);
    assert_eq!(fresh.random_rotation_state(), Random::SafelyStopped);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn pk03_repeated_missing_topology_does_not_restart_generation_each_poll() {
    struct MissingTopology(AutomationFixture);
    impl AutomationTickContext for MissingTopology {
        fn generation(&self) -> u64 {
            self.0.generation.load(Ordering::Acquire)
        }
        fn state(&self) -> &CandidateState {
            &self.0.state
        }
        fn last_minute(&mut self) -> &mut Option<(u8, u16)> {
            &mut self.0.minute
        }
        fn next_due(&mut self) -> &mut u64 {
            &mut self.0.due
        }
        fn monitors(&mut self) -> Result<Vec<String>, String> {
            self.0.effects[0] += 1;
            Err("owned unavailable displays".into())
        }
        fn stop_interval(&mut self) {
            self.0.stop_interval();
        }
        fn run_playlist(
            &mut self,
            _: Option<&str>,
            _: Option<Vec<String>>,
            _: AutomationTraceContext<'_>,
        ) -> bool {
            panic!("missing topology never reaches submission")
        }
    }
    let mut context = MissingTopology(AutomationFixture::new());
    context.0.state.automation_intent = Some(AutomationIntent {
        random_enabled: true,
        ..Default::default()
    });
    let original = context.generation();
    consume_automation_tick(&mut context, original, 1000, 2, 480);
    let stopped = context.generation();
    assert_ne!(stopped, original);
    assert!(context.state().automation_intent().safely_stopped);
    assert!(
        context.state().schedules[0].enabled,
        "unrelated schedule remains configured"
    );
    consume_automation_tick(&mut context, stopped, 1030, 2, 480);
    assert_eq!(
        context.generation(),
        stopped,
        "persistent observation failure cannot spin immediate replacement producers"
    );
    assert_eq!(context.0.effects, [2, 0, 0, 0, 0]);
}

#[test]
fn pk03_every_publication_carries_refused_session_stop_until_explicit_recovery() {
    use candidate_state::commit_test::{self, Fault};
    for next_save in [
        "random-off",
        "interval",
        "other-owner",
        "persist-in-place",
        "reload-then-owner",
    ] {
        let (mut app, calls) = fixture();
        app.set_random_rotation(true, 1);
        app.start_playlist("owned".into(), 2);
        let ((), errors) = commit_test::scoped(
            Fault::Io(
                candidate_state::SaveFailurePoint::PreparePrimary,
                std::io::ErrorKind::PermissionDenied,
            ),
            || app.safely_stop_automation(),
        );
        assert_eq!(errors.len(), 1);
        assert!(app.automation_suspended);
        assert!(
            CandidateState::load(&app.state_path)
                .automation_intent()
                .playlist_playing,
            "failure did not falsely publish a stop"
        );
        match next_save {
            "random-off" => app.set_random_rotation(false, 3),
            "interval" => app.set_automation_interval(600, 3),
            "persist-in-place" => {
                app.state.favorites.insert("other-memory-owner".into());
                app.persist_state_outcome().unwrap();
            }
            _ => {
                if next_save == "reload-then-owner" {
                    app.reload_local_state().unwrap();
                }
                let mut proposed = app.state.clone();
                proposed.favorites.insert("other-proposed-owner".into());
                // An unrelated proposal cannot accidentally carry stale pre-failure activation.
                proposed
                    .automation_intent
                    .as_mut()
                    .unwrap()
                    .playlist_playing = true;
                proposed.automation_intent.as_mut().unwrap().safely_stopped = false;
                app.commit_local_state_change(proposed).unwrap();
            }
        }
        let fresh = CandidateState::load(&app.state_path);
        assert!(fresh.automation_intent().safely_stopped, "{next_save}");
        assert!(!fresh.automation_intent().playlist_playing, "{next_save}");
        assert_ne!(fresh.random_rotation_state(), Random::On, "{next_save}");
        assert!(!app.playlist_playing());
        app.set_random_rotation(true, 100);
        assert_eq!(
            app.random_rotation_state(),
            Random::On,
            "only explicit recovery clears latch: {next_save}"
        );
        assert!(
            !CandidateState::load(&app.state_path)
                .automation_intent()
                .safely_stopped
        );
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

#[test]
fn pk03_public_saved_settings_reload_accepts_selected_legacy_or_new_intent_read_only() {
    for legacy in [false, true] {
        let (mut app, calls) = fixture();
        app.launch_policy = p1_preview_policy();
        app.panel_state.open_settings(SettingsCategory::Automation);
        view::settings::controller::reconcile(&mut app);
        let mut saved = app.state.clone();
        saved.slideshow_interval_seconds = 600;
        if legacy {
            saved.automation_intent = None;
            saved.slideshow_enabled = true;
            saved.active_playlist_id = None;
        } else {
            saved.active_playlist_id = Some("owned".into());
            saved.automation_intent = Some(AutomationIntent {
                random_enabled: true,
                playlist_playing: true,
                ..Default::default()
            });
        }
        saved.save(&app.state_path).unwrap();
        let bytes = std::fs::read(&app.state_path).unwrap();
        app.save_failure = Some("Owned recovery availability for public read-only reload".into());
        app.next_slideshow_unix = 1;
        let before_time = unix_now();
        let timer = app.automation_generation.load(Ordering::Acquire);
        let window = app.window;
        let generation = app.settings_workbench.generation;
        let _ = update(
            &mut app,
            Message::SettingsReloadSavedState { window, generation },
        );
        assert!(app
            .settings_workbench
            .message
            .starts_with("Saved local settings reloaded"));
        assert_eq!(app.state.automation_intent(), saved.automation_intent());
        assert_eq!(app.state.active_playlist_id, saved.active_playlist_id);
        assert_eq!(
            app.random_rotation_state(),
            if legacy {
                Random::On
            } else {
                Random::PausedForPlaylist
            }
        );
        assert_eq!(app.state.slideshow_interval_seconds, 600);
        assert!(app.next_slideshow_unix >= before_time + 600);
        assert!(app.automation_generation.load(Ordering::Acquire) > timer);
        assert_eq!(
            std::fs::read(&app.state_path).unwrap(),
            bytes,
            "reload never publishes or retries intent"
        );
        assert!(!app.state.local_library.startup.reapply_enabled);
        assert!(!app.apply_pending && app.active_operation.is_none());
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }
}

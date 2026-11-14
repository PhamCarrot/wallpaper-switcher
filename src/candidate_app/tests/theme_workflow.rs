//! P5 application-appearance and named visual workflow contracts.
use super::*;
use crate::candidate_app::theme_workflow::{Action, Binding, Choice, Departure, Load, PromptKind};

#[test]
fn p5_owned_hidden_exit_probe_watchdog_yields_instead_of_blocking_window_queries() {
    use std::{
        future::Future,
        task::{Context, Poll},
    };
    let mut receiver =
        Box::pin(diagnostics::theme_probe_delay(std::time::Duration::from_secs(1)).unwrap());
    let waker = iced::futures::task::noop_waker();
    let mut context = Context::from_waker(&waker);
    assert!(
        matches!(receiver.as_mut().poll(&mut context), Poll::Pending),
        "polling the watchdog must yield so the native hide/query task can settle"
    );
    iced::futures::executor::block_on(receiver).unwrap();
}

#[test]
fn p5_owned_hidden_exit_probe_cancelled_poll_recovers_only_its_current_owner() {
    for replacement in [false, true] {
        let mut app = designer_app();
        app.launch_policy = p1_preview_policy();
        designer_name(&mut app, "Cancelled timer preserves draft");
        app.theme_workflow.probe_armed = true;
        let _ = update(&mut app, Message::ThemeExitProbe);
        let id = app.window.unwrap();
        let generation = app.visibility_generation;
        // A Windowed observation completes the initial hide chain and queues
        // a delayed query; its cancelled timer must not query or authorize Exit.
        let _ = update(
            &mut app,
            Message::ThemeExitProbeObserved {
                window: id,
                generation,
                attempt: 0,
                mode: window::Mode::Windowed,
            },
        );
        if replacement {
            app.window = Some(window::Id::unique());
            app.visible = true;
        }
        let current_window = app.window;
        let before = app.theme_session.clone();
        let _ = update(
            &mut app,
            Message::ThemeExitProbePollFailed {
                window: id,
                generation: generation.wrapping_add(1),
            },
        );
        assert!(
            app.theme_workflow.probe.is_some(),
            "stale timer cannot clear current probe"
        );
        let _ = update(
            &mut app,
            Message::ThemeExitProbePollFailed {
                window: id,
                generation,
            },
        );
        assert!(app.visible && app.theme_workflow.probe.is_none() && !app.exit_requested);
        assert_eq!(app.window, current_window);
        assert_eq!(app.theme_session, before);
        assert!(app.theme_workflow.prompt.is_none());
        designer_name(&mut app, "Editable after cancelled timer");
        assert_eq!(
            app.theme_session.designer_name(),
            "Editable after cancelled timer"
        );
    }
}

#[test]
fn p5_owned_hidden_exit_probe_requires_explicit_preview_and_trace() {
    for args in [
        vec!["candidate", "--preview-theme-exit-probe"],
        vec!["candidate", "--preview", "--preview-theme-exit-probe"],
        vec!["candidate", "--apply-trace", "--preview-theme-exit-probe"],
        vec![
            "candidate",
            "--preview",
            "--apply-trace",
            "--preview-theme-exit-probe",
            "--scenario",
        ],
        vec![
            "candidate",
            "--preview",
            "--apply-trace",
            "--preview-theme-exit-probe-extra",
        ],
    ] {
        assert!(runtime::parse_launch_policy(
            &args.into_iter().map(str::to_owned).collect::<Vec<_>>()
        )
        .is_err());
    }
    let policy = runtime::parse_launch_policy(
        &[
            "candidate",
            "--preview-theme-exit-probe",
            "--apply-trace",
            "--preview",
        ]
        .map(str::to_owned),
    )
    .unwrap();
    assert!(policy.is_preview());
    assert!(
        !policy.allows_provider() && !policy.shell_hooks_enabled(true) && !policy.requests_reopen()
    );
    let mut ordinary = designer_app();
    ordinary.launch_policy = p1_preview_policy();
    designer_name(&mut ordinary, "Retain ordinary preview policy");
    let before = ordinary.theme_session.clone();
    let _ = update(&mut ordinary, Message::ThemeExitProbe);
    assert!(ordinary.visible && ordinary.theme_workflow.probe.is_none());
    assert_eq!(ordinary.theme_session, before);
    let _ = ordinary.set_visible(false, "ordinary-preview-hide");
    assert!(ordinary.visible && ordinary.theme_workflow.prompt.is_some());
}

#[test]
fn p5_owned_hidden_exit_probe_is_one_shot_bound_and_recovers_without_authorizing_exit() {
    for failure in [false, true] {
        let (mut app, submissions) = d3_application_fixture(0);
        app.launch_policy = p1_preview_policy();
        app.panel_state.open_settings(SettingsCategory::Theme);
        app.ensure_theme_designer();
        designer_name(&mut app, "Owned retained draft");
        let document = app.theme_session.editor_document().cloned();
        let before_instance = app.theme_session.editor_instance();
        app.theme_workflow.probe_armed = true;
        let _ = update(&mut app, Message::ThemeExitProbe);
        let id = app.window.unwrap();
        let generation = app.visibility_generation;
        assert!(!app.visible && !app.theme_workflow.probe_armed);
        assert!(app.theme_workflow.probe.is_some());
        assert_ne!(app.theme_session.editor_instance(), before_instance);
        assert_eq!(app.theme_session.editor_document().cloned(), document);
        assert!(!app.exit_requested);
        let _ = update(
            &mut app,
            Message::ThemeExitProbeObserved {
                window: id,
                generation: generation.wrapping_add(1),
                attempt: 0,
                mode: window::Mode::Hidden,
            },
        );
        assert!(
            app.theme_workflow.probe.is_some(),
            "stale observation cannot complete a newer probe"
        );
        if failure {
            let _ = update(
                &mut app,
                Message::ThemeExitProbeTimeout {
                    window: id,
                    generation,
                },
            );
            assert!(
                app.visible && app.theme_workflow.probe.is_some(),
                "timeout must retain the unfinished hide identity"
            );
            assert!(app.theme_workflow.prompt.is_none() && !app.exit_requested);
            let _ = update(
                &mut app,
                Message::ThemeExitProbeObserved {
                    window: id,
                    generation,
                    attempt: 0,
                    mode: window::Mode::Hidden,
                },
            );
            assert!(app.visible && app.theme_workflow.probe.is_none());
        } else {
            let _ = update(
                &mut app,
                Message::ThemeExitProbeObserved {
                    window: id,
                    generation,
                    attempt: 0,
                    mode: window::Mode::Hidden,
                },
            );
            assert!(app.theme_workflow.probe.is_none());
            assert!(
                !app.exit_requested && app.theme_workflow.prompt.is_none(),
                "mode observation only queues the ordinary Exit message"
            );
            let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
            assert!(app.visible && app.theme_workflow.prompt.is_some() && !app.exit_requested);
            designer_decide(&mut app, Choice::Cancel);
            assert!(app.theme_workflow.prompt.is_none());
        }
        let before = app.theme_session.clone();
        let _ = update(&mut app, Message::ThemeExitProbe);
        let _ = update(
            &mut app,
            Message::ThemeExitProbeTimeout {
                window: id,
                generation,
            },
        );
        assert!(app.visible && app.theme_workflow.probe.is_none() && !app.exit_requested);
        assert_eq!(app.theme_session, before);
        designer_name(&mut app, "Editable after probe");
        assert_eq!(app.theme_session.designer_name(), "Editable after probe");
        assert_eq!(submissions.load(Ordering::Acquire), 0);
    }
}

#[test]
fn p5_owned_hidden_exit_probe_late_hide_completion_cannot_strand_or_replace_a_decision() {
    for mode in [window::Mode::Hidden, window::Mode::Fullscreen] {
        for replacement in [false, true] {
            for prompt in [false, true] {
                let mut app = designer_app();
                app.launch_policy = p1_preview_policy();
                designer_name(&mut app, "Late hide preserves this draft");
                app.theme_workflow.probe_armed = true;
                let _ = update(&mut app, Message::ThemeExitProbe);
                let id = app.window.unwrap();
                let generation = app.visibility_generation;
                let _ = update(
                    &mut app,
                    Message::ThemeExitProbeTimeout {
                        window: id,
                        generation,
                    },
                );
                assert!(app.visible && app.theme_workflow.probe.is_some());
                let recovered_generation = app.visibility_generation;
                let _ = update(
                    &mut app,
                    Message::ThemeExitProbeTimeout {
                        window: id,
                        generation,
                    },
                );
                assert_eq!(
                    app.visibility_generation, recovered_generation,
                    "duplicate timeout cannot restart recovery"
                );
                if replacement {
                    app.window = Some(window::Id::unique());
                }
                if prompt {
                    let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
                }
                let before = app.theme_session.clone();
                let decision = app
                    .theme_workflow
                    .prompt
                    .as_ref()
                    .map(|p| (p.binding, p.decision));
                let visible_generation = app.visibility_generation;
                let current_window = app.window;
                let _ = update(
                    &mut app,
                    Message::ThemeExitProbeObserved {
                        window: id,
                        generation,
                        attempt: 0,
                        mode,
                    },
                );
                assert!(app.visible && app.theme_workflow.probe.is_none() && !app.exit_requested);
                assert_eq!(app.window, current_window);
                assert_eq!(app.theme_session, before);
                assert_eq!(
                    app.theme_workflow
                        .prompt
                        .as_ref()
                        .map(|p| (p.binding, p.decision)),
                    decision
                );
                assert_eq!(app.visibility_generation, if replacement { visible_generation } else { visible_generation.wrapping_add(1) }, "only the original surviving window receives a final reveal after hide completion");
                if prompt {
                    designer_decide(&mut app, Choice::Cancel);
                }
                designer_name(&mut app, "Still editable after late completion");
                assert_eq!(
                    app.theme_session.designer_name(),
                    "Still editable after late completion"
                );
            }
        }
    }
    let mut app = designer_app();
    app.launch_policy = p1_preview_policy();
    designer_name(&mut app, "Exhausted mode observation");
    app.theme_workflow.probe_armed = true;
    let _ = update(&mut app, Message::ThemeExitProbe);
    let id = app.window.unwrap();
    let generation = app.visibility_generation;
    let _ = update(
        &mut app,
        Message::ThemeExitProbeObserved {
            window: id,
            generation,
            attempt: 7,
            mode: window::Mode::Windowed,
        },
    );
    assert!(app.visible && app.theme_workflow.probe.is_none() && !app.exit_requested);
}

#[test]
fn p5_owned_hidden_exit_probe_reopens_after_actual_hidden_window_cleanup_before_timeout() {
    for delayed_cleanup in [false, true] {
        let mut app = designer_app();
        app.launch_policy = p1_preview_policy();
        designer_name(&mut app, "Retain draft after hidden window closes");
        let document = app.theme_session.editor_document().cloned();
        app.theme_workflow.probe_armed = true;
        let _ = update(&mut app, Message::ThemeExitProbe);
        let original = app.window.unwrap();
        let generation = app.visibility_generation;
        if delayed_cleanup {
            let _ = update(&mut app, Message::Wake(Wake::CloseHiddenWindow(generation)));
            assert!(app.window.is_none());
        }
        let _ = update(&mut app, Message::WindowClosed(original));
        assert!(app.window.is_none() && !app.visible);
        let _ = update(
            &mut app,
            Message::ThemeExitProbeTimeout {
                window: original,
                generation,
            },
        );
        let reopened = app
            .window
            .expect("recovery creates an owned surviving window");
        assert_ne!(reopened, original);
        assert!(app.visible && !app.exit_requested);
        assert_eq!(app.theme_session.editor_document().cloned(), document);
        assert_eq!(
            app.theme_session.designer_name(),
            "Retain draft after hidden window closes"
        );
        let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
        let decision = app
            .theme_workflow
            .prompt
            .as_ref()
            .map(|p| (p.binding, p.decision));
        let visible_generation = app.visibility_generation;
        let _ = update(
            &mut app,
            Message::ThemeExitProbeObserved {
                window: original,
                generation,
                attempt: 0,
                mode: window::Mode::Hidden,
            },
        );
        assert_eq!(app.window, Some(reopened));
        assert_eq!(
            app.visibility_generation, visible_generation,
            "late old-window completion cannot touch the replacement"
        );
        assert_eq!(
            app.theme_workflow
                .prompt
                .as_ref()
                .map(|p| (p.binding, p.decision)),
            decision
        );
        assert!(app.theme_workflow.probe.is_none() && !app.exit_requested);
        designer_decide(&mut app, Choice::Cancel);
        designer_name(&mut app, "Editable in reopened owned window");
        assert_eq!(
            app.theme_session.designer_name(),
            "Editable in reopened owned window"
        );
    }
}

#[test]
fn theme_visual_name_identity_detaches_on_semantic_rename_and_reset_restores_only_live_id() {
    let mut app = designer_app();
    designer_name(&mut app, "A");
    designer_save(&mut app, false);
    let id = app.state.theme_library.records[0].id;
    designer_name(&mut app, " A ");
    assert_eq!(app.theme_session.designer_loaded_id(), Some(id));
    designer_save(&mut app, false);
    assert!(app.theme_workflow.prompt.is_none());
    assert_eq!(app.state.theme_library.records.len(), 1);
    designer_name(&mut app, "B");
    designer_name(&mut app, "A");
    assert_eq!(app.theme_session.designer_loaded_id(), None);
    designer_save(&mut app, false);
    assert!(matches!(
        app.theme_workflow.prompt.as_ref().unwrap().kind,
        PromptKind::Overwrite { .. }
    ));
    designer_decide(&mut app, Choice::Cancel);
    let binding = Binding::current(&app).unwrap();
    let _ = update(&mut app, Message::ThemeDesigner(Action::Reset(binding)));
    assert_eq!(app.theme_session.designer_loaded_id(), Some(id));
    designer_save(&mut app, false);
    assert!(app.theme_workflow.prompt.is_none());
    let binding = Binding::current(&app).unwrap();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(Action::Delete { binding, id }),
    );
    designer_decide(&mut app, Choice::Confirm);
    let binding = Binding::current(&app).unwrap();
    let _ = update(&mut app, Message::ThemeDesigner(Action::Reset(binding)));
    assert_eq!(app.theme_session.designer_loaded_id(), None);
    designer_save(&mut app, false);
    assert_ne!(app.state.theme_library.records[0].id, id);
    assert!(app.theme_workflow.prompt.is_none());
}

#[test]
fn theme_visual_current_bindings_cannot_edit_or_publish_hidden_or_off_page() {
    for hidden in [false, true] {
        let (mut app, submissions) = d3_application_fixture(0);
        app.panel_state.open_settings(SettingsCategory::Theme);
        theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
        designer_save(&mut app, true);
        theme_editor_set(&mut app, "#123456");
        if hidden {
            let _ = app.set_visible(false, "owned-current-binding-hide");
        } else {
            let _ = update(
                &mut app,
                Message::SetSettingsCategory(SettingsCategory::Playback),
            );
            designer_decide(&mut app, Choice::Save);
        }
        let binding = Binding::current(&app).unwrap();
        let (i, g, r, v) = theme_editor_binding(&app);
        let before = app.theme_session.clone();
        let state = app.state.clone();
        let disk = std::fs::read(&app.state_path).unwrap();
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
            Message::ThemeDesigner(Action::Name {
                instance: i,
                text: "Must not land".into(),
            }),
        );
        let _ = update(
            &mut app,
            Message::ThemeDesigner(Action::Save {
                binding,
                apply: true,
            }),
        );
        let _ = update(
            &mut app,
            Message::ThemeDesigner(Action::Delete {
                binding,
                id: state.theme_library.records[0].id,
            }),
        );
        assert_eq!(app.theme_session, before);
        assert_eq!(app.state, state);
        assert_eq!(std::fs::read(&app.state_path).unwrap(), disk);
        assert!(app.theme_workflow.prompt.is_none());
        assert_eq!(submissions.load(Ordering::Acquire), 0);
    }
}

#[test]
fn theme_visual_dirty_valid_engine_open_decides_before_owner_reset() {
    for choice in [Choice::Save, Choice::Discard] {
        let mut app = theme_dirty_applied_app();
        app.wallpapers.push(theme_engine_item(&app, true));
        app.selected = Some(0);
        let before = app.state.applied_theme.clone();
        let draft = app.theme_session.editor_document().unwrap().clone();
        let _ = update(&mut app, Message::ToggleEngineProperties);
        assert!(lifecycle::theme_owner_active(&app));
        assert!(!app.engine_properties_open);
        assert!(app.theme_workflow.prompt.is_some());
        designer_decide(&mut app, Choice::Cancel);
        assert!(lifecycle::theme_owner_active(&app));
        assert!(!app.engine_properties_open);
        assert_eq!(app.theme_session.editor_document(), Some(&draft));
        let _ = update(&mut app, Message::ToggleEngineProperties);
        designer_decide(&mut app, choice);
        assert!(app.engine_properties_open);
        assert!(!app.panel_state.settings_open);
        assert!(!app.engine_properties.is_empty());
        assert!(!app.theme_session.designer_dirty());
        assert_eq!(app.state.applied_theme, before);
        if choice == Choice::Save {
            assert_eq!(app.state.theme_library.records[0].document, draft);
        }
        assert!(!app.apply_pending && app.active_operation.is_none());
    }
}

#[test]
fn theme_departure_respects_settings_ownership_and_retains_resolved_draft() {
    let (mut app, submissions) = d3_application_fixture(0);
    app.panel_state.open_settings(SettingsCategory::Theme);
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    designer_save(&mut app, true);
    theme_editor_set(&mut app, "#123456");
    let old = Binding::current(&app).unwrap();
    let draft = app.theme_session.editor_document().unwrap().clone();
    let _ = update(&mut app, Message::ToggleSettingsPanel);
    assert!(app.theme_workflow.prompt.is_some());
    designer_decide(&mut app, Choice::Save);
    assert!(!app.panel_state.settings_open);
    assert_eq!(app.theme_session.editor_document(), Some(&draft));
    assert!(!app.theme_session.designer_dirty());
    let state = app.state.clone();
    let disk = std::fs::read(&app.state_path).unwrap();
    assert!(!lifecycle::theme_owner_active(&app));
    assert!(!app.theme_session.designer_dirty());
    let current = Binding::current(&app).unwrap();
    assert_ne!(current, old);
    let (i, g, r, v) = theme_editor_binding(&app);
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
        Message::ThemeDesigner(Action::Name {
            instance: i,
            text: "Must not land".into(),
        }),
    );
    let _ = update(
        &mut app,
        Message::ThemeDesigner(Action::Save {
            binding: current,
            apply: true,
        }),
    );
    assert_eq!(app.theme_session, before);
    assert_eq!(app.state, state);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), disk);
    assert!(!lifecycle::theme_owner_active(&app));
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Theme),
    );
    assert!(lifecycle::theme_owner_active(&app));
    assert_eq!(app.theme_session.editor_document(), Some(&draft));
    let _ = update(
        &mut app,
        Message::ThemeDesigner(Action::Save {
            binding: old,
            apply: true,
        }),
    );
    assert_eq!(app.state, state);
    theme_editor_set(&mut app, "#abcdef");
    designer_save(&mut app, false);
    assert!(!app.theme_session.designer_dirty());
    assert_eq!(app.state.applied_theme, state.applied_theme);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
    app.clear_posters();
}

pub(super) fn designer_app() -> GpuCandidate {
    let mut app = theme_open_app();
    app.ensure_theme_designer();
    app
}
pub(super) fn designer_name(app: &mut GpuCandidate, name: &str) {
    let instance = app.theme_session.editor_instance().unwrap();
    let _ = update(
        app,
        Message::ThemeDesigner(Action::Name {
            instance,
            text: name.into(),
        }),
    );
}
pub(super) fn designer_save(app: &mut GpuCandidate, apply: bool) {
    let binding = Binding::current(app).unwrap();
    let _ = update(app, Message::ThemeDesigner(Action::Save { binding, apply }));
}
pub(super) fn designer_decide(app: &mut GpuCandidate, choice: Choice) {
    let binding = app.theme_workflow.prompt.as_ref().unwrap().binding;
    let decision = app.theme_workflow.prompt.as_ref().unwrap().decision;
    let _ = update(
        app,
        Message::ThemeDesigner(Action::Decide {
            binding,
            decision,
            choice,
        }),
    );
}
pub(super) fn designer_color(app: &mut GpuCandidate, color: &str) {
    let instance = app.theme_session.editor_instance().unwrap();
    let generation = app.theme_session.editor_selection_generation().unwrap();
    let role = app.theme_session.editor_selected_role().unwrap();
    let variant = app.theme_session.editor_selected_variant().unwrap();
    let _ = update(
        app,
        Message::ThemeRecent {
            editor_instance: instance,
            selection_generation: generation,
            role,
            variant,
            color: color.into(),
        },
    );
}
fn designer_load(app: &mut GpuCandidate, target: Load) {
    let binding = Binding::current(app).unwrap();
    let _ = update(
        app,
        Message::ThemeDesigner(Action::Load { binding, target }),
    );
}

#[test]
fn p5c_save_library_only_and_apply_snapshot_survive_actual_restart() {
    let mut app = designer_app();
    let applied = app.state.applied_theme.clone();
    designer_name(&mut app, "Library");
    designer_color(&mut app, "#123456");
    let document = app.theme_session.editor_document().unwrap().clone();
    designer_save(&mut app, false);
    assert!(app.save_failure.is_none(), "{:?}", app.save_failure);
    assert!(!app.theme_session.designer_dirty());
    assert_eq!(app.state.theme_library.records[0].document, document);
    assert_eq!(app.state.applied_theme, applied);
    let loaded = CandidateState::load_with_diagnostics(&app.state_path).state;
    assert_eq!(loaded.theme_library, app.state.theme_library);
    assert_eq!(loaded.applied_theme, applied);
    designer_save(&mut app, true);
    assert_eq!(app.state.applied_theme.as_ref().unwrap().document, document);
    assert_eq!(
        CandidateState::load(&app.state_path).applied_theme,
        app.state.applied_theme
    );
    // Updating an applied record with Save must leave the captured snapshot intact.
    designer_color(&mut app, "#abcdef");
    let applied = app.state.applied_theme.clone();
    designer_save(&mut app, false);
    assert_eq!(app.state.theme_library.records.len(), 1);
    assert_eq!(app.state.applied_theme, applied);
    let restart = CandidateState::load(&app.state_path);
    assert_eq!(restart.applied_theme, applied);
    assert_ne!(
        restart.theme_library.records[0].document,
        restart.applied_theme.unwrap().document
    );
}

#[test]
fn p5c_overwrite_requires_current_draft_and_unchanged_target() {
    let mut app = designer_app();
    designer_name(&mut app, "Existing");
    designer_save(&mut app, false);
    let first = app.state.theme_library.records[0].clone();
    designer_load(&mut app, Load::Starter("nord"));
    designer_name(&mut app, "Existing");
    designer_color(&mut app, "#123456");
    designer_save(&mut app, false);
    assert!(matches!(
        app.theme_workflow.prompt.as_ref().unwrap().kind,
        PromptKind::Overwrite { .. }
    ));
    assert_eq!(app.state.theme_library.records[0], first);
    let stale = app.theme_workflow.prompt.as_ref().unwrap().binding;
    let stale_decision = app.theme_workflow.prompt.as_ref().unwrap().decision;
    designer_name(&mut app, "New revision");
    let _ = update(
        &mut app,
        Message::ThemeDesigner(Action::Decide {
            binding: stale,
            decision: stale_decision,
            choice: Choice::Confirm,
        }),
    );
    assert_eq!(app.state.theme_library.records[0], first);
    designer_name(&mut app, "Existing");
    designer_save(&mut app, false);
    app.state.theme_library.records[0].document = first.document.replaced(
        theme_roles::ThemeRole::Surface,
        theme_roles::ColorVariant::Dark,
        "#654321",
    );
    let changed = app.state.theme_library.records[0].clone();
    designer_decide(&mut app, Choice::Confirm);
    assert_eq!(app.state.theme_library.records[0], changed);
    designer_save(&mut app, false);
    designer_decide(&mut app, Choice::Confirm);
    assert_eq!(app.state.theme_library.records.len(), 1);
    assert_eq!(app.state.theme_library.records[0].id, first.id);
    assert_eq!(
        app.state.theme_library.records[0].document,
        app.theme_session.editor_document().unwrap().clone()
    );
}

#[test]
fn p5c_custom_nord_delete_requires_confirmation_and_publishes_fallback() {
    let mut app = designer_app();
    designer_name(&mut app, "Nord");
    designer_color(&mut app, "#123456");
    designer_save(&mut app, true);
    let id = app.state.theme_library.records[0].id;
    assert!(
        matches!(app.state.applied_theme.as_ref().unwrap().origin,candidate_state::frontend::ThemeOrigin::Custom{record_id} if record_id==id)
    );
    let binding = Binding::current(&app).unwrap();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(Action::Delete { binding, id }),
    );
    assert_eq!(app.state.theme_library.records.len(), 1);
    designer_decide(&mut app, Choice::Cancel);
    assert_eq!(app.state.theme_library.records.len(), 1);
    let binding = Binding::current(&app).unwrap();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(Action::Delete { binding, id }),
    );
    designer_decide(&mut app, Choice::Confirm);
    assert!(app.state.theme_library.records.is_empty());
    assert_eq!(
        app.state.applied_theme,
        Some(candidate_state::frontend::AppliedThemeSnapshot::default())
    );
    let restart = CandidateState::load(&app.state_path);
    assert!(restart.theme_library.records.is_empty());
    assert_eq!(restart.applied_theme, app.state.applied_theme);
}

#[test]
fn p5c_dirty_load_save_invalid_hex_and_cancel_keep_usable_draft() {
    let mut app = designer_app();
    designer_name(&mut app, "Retain me");
    let i = app.theme_session.editor_instance().unwrap();
    let g = app.theme_session.editor_selection_generation().unwrap();
    let r = app.theme_session.editor_selected_role().unwrap();
    let v = app.theme_session.editor_selected_variant().unwrap();
    let _ = update(
        &mut app,
        Message::ThemeRoleTextChanged {
            editor_instance: i,
            selection_generation: g,
            role: r,
            variant: v,
            text: "#invalid".into(),
        },
    );
    designer_load(&mut app, Load::Starter("nord"));
    designer_decide(&mut app, Choice::Save);
    assert_eq!(app.theme_session.editor_buffer(), Some("#invalid"));
    assert!(app.theme_session.designer_dirty());
    assert!(app.state.theme_library.records.is_empty());
    designer_decide(&mut app, Choice::Cancel);
    assert_eq!(app.theme_session.designer_name(), "Retain me");
    designer_load(&mut app, Load::Starter("nord"));
    designer_decide(&mut app, Choice::Discard);
    assert!(!app.theme_session.designer_dirty());
    assert!(!app.theme_session.editor_pending());
    assert_eq!(
        app.theme_session.editor_document(),
        Some(&candidate_state::frontend::nord_document())
    );
}

#[test]
fn p5c_future_backup_refuses_save_and_apply_without_losing_draft() {
    let mut app = designer_app();
    designer_name(&mut app, "Failure draft");
    designer_color(&mut app, "#123456");
    let before = app.state.clone();
    let document = app.theme_session.editor_document().unwrap().clone();
    let future = b"version = 999\n# owned future backup\n";
    std::fs::write(app.state_path.with_extension("toml.bak"), future).unwrap();
    designer_save(&mut app, true);
    assert!(app.save_failure.is_some());
    assert_eq!(app.state.applied_theme, before.applied_theme);
    assert_eq!(app.state.theme_library, before.theme_library);
    assert_eq!(app.theme_session.editor_document(), Some(&document));
    assert_eq!(app.theme_session.designer_name(), "Failure draft");
    assert!(app.theme_session.designer_dirty());
    assert_eq!(
        std::fs::read(app.state_path.with_extension("toml.bak")).unwrap(),
        future
    );
    assert!(!app.state_path.exists());
}

#[test]
fn p5c_confirmation_token_cannot_authorize_replacement_or_reopened_prompt() {
    let mut app = designer_app();
    designer_name(&mut app, "First");
    designer_save(&mut app, false);
    designer_name(&mut app, "Second");
    designer_save(&mut app, false);
    let first = app.state.theme_library.records[0].id;
    let second = app.state.theme_library.records[1].id;
    let binding = Binding::current(&app).unwrap();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(Action::Delete { binding, id: first }),
    );
    let old = app.theme_workflow.prompt.clone().unwrap();
    let binding = Binding::current(&app).unwrap();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(Action::Delete {
            binding,
            id: second,
        }),
    );
    assert_ne!(
        app.theme_workflow.prompt.as_ref().unwrap().decision,
        old.decision
    );
    let _ = update(
        &mut app,
        Message::ThemeDesigner(Action::Decide {
            binding,
            decision: old.decision,
            choice: Choice::Confirm,
        }),
    );
    assert_eq!(app.state.theme_library.records.len(), 2);
    designer_decide(&mut app, Choice::Cancel);
    let binding = Binding::current(&app).unwrap();
    let _ = update(
        &mut app,
        Message::ThemeDesigner(Action::Delete { binding, id: first }),
    );
    let _ = update(
        &mut app,
        Message::ThemeDesigner(Action::Decide {
            binding: old.binding,
            decision: old.decision,
            choice: Choice::Confirm,
        }),
    );
    assert_eq!(app.state.theme_library.records.len(), 2);
    designer_decide(&mut app, Choice::Confirm);
    assert_eq!(app.state.theme_library.records.len(), 1);
    assert_eq!(app.state.theme_library.records[0].id, second);
}

#[test]
fn pk10_starter_loads_native_variant_and_all_cells_without_applying() {
    use candidate_state::frontend::{self, starter_palettes};
    use theme_roles::{ColorVariant, ThemeRole};

    let starters = starter_palettes();
    let names: Vec<_> = starters
        .iter()
        .map(|starter| starter.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "Nord",
            "Dracula",
            "Tokyo Night",
            "Tokyo Night Storm",
            "Tokyo Night Moon",
            "Catppuccin",
            "Catppuccin Macchiato",
            "Catppuccin Frappé",
            "Catppuccin Latte",
            "Gruvbox",
            "Rose Pine",
            "Rose Pine Moon",
            "Rose Pine Dawn",
            "Everforest",
            "Kanagawa",
            "Solarized Dark",
            "Solarized Light",
            "One Dark",
            "Monokai",
            "GitHub Dark",
            "GitHub Light",
            "Night Owl",
            "Palenight",
            "Synthwave '84",
            "Ayu Dark",
            "Ayu Mirage",
        ],
    );
    assert_eq!(
        starters
            .iter()
            .map(|starter| &starter.id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        26,
    );
    assert_eq!(starters[0].document, frontend::nord_document());
    let mut app = designer_app();
    let before = app.state.clone();
    for starter in starters {
        designer_load(&mut app, Load::Starter(&starter.id));
        assert_eq!(app.theme_workflow.starter, Some(starter.id.as_str()));
        assert_eq!(
            app.theme_session.editor_selected_variant(),
            Some(starter.variant)
        );
        assert_eq!(app.theme_session.editor_document(), Some(&starter.document));
        assert!(!app.theme_session.designer_dirty());
        assert_eq!(app.state, before);
    }
    assert!(!app.state_path.exists());

    designer_load(&mut app, Load::Starter("catppuccin-latte"));
    assert_eq!(
        app.theme_session.editor_selected_variant(),
        Some(ColorVariant::Light)
    );
    let starter = app.theme_session.editor_document().unwrap().clone();
    assert_eq!(
        starter.color(ThemeRole::Primary, ColorVariant::Light),
        "#8839ef"
    );
    assert_eq!(
        starter.color(ThemeRole::Background, ColorVariant::Light),
        "#eff1f5"
    );
    assert_eq!(
        starter.color(ThemeRole::SourceColor, ColorVariant::Dark),
        "#8839ef"
    );
    assert_eq!(
        starter.color(ThemeRole::SourceColor, ColorVariant::Light),
        "#8839ef"
    );
    assert_ne!(
        starter.color(ThemeRole::Primary, ColorVariant::Dark),
        "#8839ef"
    );
    designer_name(&mut app, "PK-10 Latte");
    designer_color(&mut app, "#123456");
    let expected = starter.replaced(ThemeRole::Primary, ColorVariant::Light, "#123456");
    designer_save(&mut app, false);
    let saved = CandidateState::load(&app.state_path);
    assert_eq!(saved.theme_library.records[0].document, expected);
    assert_eq!(saved.applied_theme, before.applied_theme);
    designer_save(&mut app, true);
    let applied = CandidateState::load(&app.state_path).applied_theme.unwrap();
    assert_eq!(applied.document, expected);
    assert_eq!(applied.variant, ColorVariant::Light);

    let mut restarted = designer_app();
    restarted.state.applied_theme = Some(applied);
    restarted.theme_session = Default::default();
    restarted.ensure_theme_designer();
    assert_eq!(restarted.theme_session.editor_document(), Some(&expected));
    assert_eq!(
        restarted.theme_session.editor_selected_variant(),
        Some(ColorVariant::Light)
    );
    assert_eq!(restarted.theme_workflow.starter, None);
}

#[test]
fn pk10_dirty_starter_switch_retains_captured_target_and_stales_old_bindings() {
    use theme_roles::{ColorVariant, ThemeRole};

    let mut app = designer_app();
    designer_load(&mut app, Load::Starter("dracula"));
    assert_eq!(
        app.theme_session
            .editor_document()
            .unwrap()
            .color(ThemeRole::Primary, ColorVariant::Dark),
        "#bd93f9",
    );
    let applied = app.state.applied_theme.clone();
    designer_name(&mut app, "Keep this draft");
    designer_color(&mut app, "#123456");
    let draft = app.theme_session.editor_document().unwrap().clone();
    let stale = Binding::current(&app).unwrap();
    designer_load(&mut app, Load::Starter("catppuccin-latte"));
    let prompt = app.theme_workflow.prompt.as_ref().unwrap().clone();
    assert!(matches!(
        prompt.kind,
        PromptKind::Departure(Departure::Load(Load::Starter("catppuccin-latte")))
    ));
    let _ = update(
        &mut app,
        Message::ThemeDesigner(Action::Load {
            binding: stale,
            target: Load::Starter("ayu-mirage"),
        }),
    );
    assert_eq!(
        app.theme_workflow.prompt.as_ref().unwrap().decision,
        prompt.decision
    );
    assert_eq!(app.theme_workflow.starter, Some("dracula"));
    designer_decide(&mut app, Choice::Cancel);
    assert_eq!(app.theme_session.editor_document(), Some(&draft));
    assert_eq!(app.theme_session.designer_name(), "Keep this draft");

    designer_load(&mut app, Load::Starter("catppuccin-latte"));
    designer_decide(&mut app, Choice::Save);
    assert_eq!(app.state.theme_library.records[0].document, draft);
    assert_eq!(app.theme_workflow.starter, Some("catppuccin-latte"));
    assert_eq!(
        app.theme_session.editor_selected_variant(),
        Some(ColorVariant::Light)
    );
    assert_eq!(app.theme_session.designer_loaded_id(), None);
    assert!(app.theme_session.designer_name().is_empty());
    assert!(!app.theme_session.designer_dirty());
    assert_eq!(app.state.applied_theme, applied);
}

#[test]
fn p5d_dirty_preview_close_is_a_cancellable_owned_decision() {
    let mut app = designer_app();
    app.launch_policy = p1_preview_policy();
    designer_name(&mut app, "Keep draft");
    let id = app.window.unwrap();
    let _ = update(&mut app, Message::CloseRequested(id));
    assert!(!app.exit_requested);
    assert!(app.theme_workflow.exit.is_none());
    assert!(matches!(
        app.theme_workflow.prompt.as_ref().unwrap().kind,
        PromptKind::Departure(Departure::Exit)
    ));
    assert!(app.visible);
    designer_decide(&mut app, Choice::Cancel);
    assert!(app.visible);
    assert_eq!(app.window, Some(id));
    assert_eq!(app.theme_session.designer_name(), "Keep draft");
    assert!(app.theme_session.designer_dirty());
    assert!(!app.state_path.exists());
}

#[test]
fn p5d_hidden_tray_exit_reveals_the_owner_and_cancel_retains_invalid_text() {
    let mut app = designer_app();
    designer_name(&mut app, "Hidden draft");
    let i = app.theme_session.editor_instance().unwrap();
    let g = app.theme_session.editor_selection_generation().unwrap();
    let r = app.theme_session.editor_selected_role().unwrap();
    let v = app.theme_session.editor_selected_variant().unwrap();
    let _ = update(
        &mut app,
        Message::ThemeRoleTextChanged {
            editor_instance: i,
            selection_generation: g,
            role: r,
            variant: v,
            text: "unfinished".into(),
        },
    );
    let _ = app.set_visible(false, "owned-test-hide");
    assert!(!app.visible);
    let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
    assert!(app.visible);
    assert!(app.panel_state.settings_open);
    assert_eq!(app.panel_state.settings_category, SettingsCategory::Theme);
    assert!(app.theme_workflow.prompt.is_some());
    assert!(!app.exit_requested);
    assert_eq!(app.theme_session.editor_buffer(), Some("unfinished"));
    designer_decide(&mut app, Choice::Cancel);
    assert!(app.visible);
    assert_eq!(app.theme_session.editor_buffer(), Some("unfinished"));
    assert!(app.theme_session.editor_pending());
}

#[test]
fn p5d_cancel_rejects_already_queued_exit_finalization() {
    let mut app = designer_app();
    let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
    let exit = app.theme_workflow.exit.unwrap();
    assert!(exit.queued);
    designer_decide(&mut app, Choice::Cancel);
    let _ = update(
        &mut app,
        Message::ExitCommitted {
            decision: exit.decision,
        },
    );
    assert!(app.visible);
    assert!(!app.exit_requested);
    assert!(app.theme_workflow.exit.is_none());
}

#[test]
fn p5d_newer_draft_after_save_invalidates_exit() {
    let mut app = designer_app();
    designer_name(&mut app, "Saved first");
    let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
    designer_decide(&mut app, Choice::Save);
    assert!(app.exit_requested);
    let old = app.theme_workflow.exit.unwrap();
    assert_eq!(app.state.theme_library.records[0].name, "Saved first");
    designer_name(&mut app, "Newer unsaved draft");
    assert!(!app.exit_requested);
    assert!(app.theme_workflow.exit.is_none());
    assert!(app.theme_session.designer_dirty());
    let _ = update(
        &mut app,
        Message::ExitCommitted {
            decision: old.decision,
        },
    );
    assert_eq!(app.theme_session.designer_name(), "Newer unsaved draft");
    assert!(!app.exit_requested);
}

#[test]
fn p5d_save_failure_blocks_exit_and_preserves_usable_draft() {
    let mut app = designer_app();
    designer_name(&mut app, "Failed Exit save");
    std::fs::write(app.state_path.with_extension("toml.bak"), b"version=999\n").unwrap();
    let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
    designer_decide(&mut app, Choice::Save);
    assert!(!app.exit_requested);
    assert!(app.theme_workflow.exit.is_none());
    assert!(app.theme_session.designer_dirty());
    assert_eq!(app.theme_session.designer_name(), "Failed Exit save");
    assert!(app.save_failure.is_some());
    assert!(app.visible && app.panel_state.settings_open);
}

#[test]
fn p5d_dirty_navigation_cancel_keeps_owner_and_save_then_leaves() {
    let mut app = designer_app();
    designer_name(&mut app, "Navigation draft");
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Playback),
    );
    assert_eq!(app.panel_state.settings_category, SettingsCategory::Theme);
    designer_decide(&mut app, Choice::Cancel);
    assert_eq!(app.panel_state.settings_category, SettingsCategory::Theme);
    let _ = update(
        &mut app,
        Message::SetSettingsCategory(SettingsCategory::Playback),
    );
    designer_decide(&mut app, Choice::Save);
    assert_eq!(
        app.panel_state.settings_category,
        SettingsCategory::Playback
    );
    assert_eq!(app.state.theme_library.records[0].name, "Navigation draft");
    assert!(!app.theme_session.designer_dirty());
}

#[test]
fn p5d_both_coordinator_completion_arms_cannot_revive_cancelled_exit() {
    for engine in [false, true] {
        for cancel_first in [false, true] {
            let mut app = designer_app();
            designer_name(&mut app, "Coordinator draft");
            let id = coordinator::fixture_operation_id();
            app.active_operation = Some(id);
            app.apply_pending = true;
            let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
            designer_decide(&mut app, Choice::Discard);
            let exit = app.theme_workflow.exit.unwrap();
            assert!(!exit.queued);
            if cancel_first {
                designer_decide(&mut app, Choice::Cancel);
            }
            let result = if engine {
                CoordinatorResult::Engine(coordinator::EngineControlResult {
                    operation_id: id,
                    command: EngineCommand::SetPaused(false),
                    outcomes: vec![],
                    recoveries: vec![],
                })
            } else {
                CoordinatorResult::Apply(coordinator::ApplyResult {
                    operation_id: id,
                    wallpaper_id: "owned-completion".into(),
                    outcomes: vec![],
                    recoveries: vec![],
                    transition: coordinator::TransitionResult::not_requested(),
                })
            };
            let _ = update(&mut app, Message::Wake(Wake::CoordinatorFinished(result)));
            assert!(!app.apply_pending);
            assert!(app.active_operation.is_none());
            if !cancel_first {
                assert!(app.theme_workflow.exit.as_ref().unwrap().queued);
                designer_decide(&mut app, Choice::Cancel);
            }
            let _ = update(
                &mut app,
                Message::ExitCommitted {
                    decision: exit.decision,
                },
            );
            assert!(!app.exit_requested);
            assert!(app.theme_workflow.exit.is_none());
            assert!(app.visible);
        }
    }
}

#[test]
fn p5d_normal_focus_loss_preserves_owned_decisions_and_keeps_ordinary_hide() {
    for waiting in [false, true] {
        let mut app = designer_app();
        app.config.ui.close_on_focus_loss = true;
        if !waiting {
            designer_name(&mut app, "Dirty prompt");
        }
        let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
        let prompt = app.theme_workflow.prompt.clone().unwrap();
        let id = app.window.unwrap();
        let _ = update(&mut app, Message::WindowUnfocused(id));
        assert!(app.visible);
        assert_eq!(
            app.theme_workflow.prompt.as_ref().unwrap().decision,
            prompt.decision
        );
        assert_eq!(app.exit_requested, waiting);
        designer_decide(&mut app, Choice::Cancel);
        let _ = update(&mut app, Message::WindowUnfocused(id));
        assert!(
            !app.visible,
            "ordinary normal focus-loss hide stays connected"
        );
        assert!(!app.exit_requested);
    }
}

#[test]
fn p5d_replaced_prompt_revokes_old_exit_and_survives_stale_finalization() {
    let mut app = designer_app();
    designer_name(&mut app, "Keep record");
    designer_save(&mut app, false);
    let id = app.state.theme_library.records[0].id;
    let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
    let exit = app.theme_workflow.exit.unwrap();
    assert!(exit.queued);
    let _ = update(
        &mut app,
        Message::ThemeDesigner(Action::Delete {
            binding: exit.binding,
            id,
        }),
    );
    let replacement = app.theme_workflow.prompt.clone().unwrap();
    assert!(matches!(replacement.kind, PromptKind::Delete(_)));
    assert_ne!(replacement.decision, exit.decision);
    assert!(app.theme_workflow.exit.is_none());
    assert!(!app.exit_requested);
    let _ = update(
        &mut app,
        Message::ExitCommitted {
            decision: exit.decision,
        },
    );
    assert_eq!(app.state.theme_library.records.len(), 1);
    assert_eq!(
        app.theme_workflow.prompt.as_ref().unwrap().decision,
        replacement.decision
    );
    assert!(app.visible);
    designer_decide(&mut app, Choice::Cancel);
}

fn palette_cells(p: donor_ui::Palette) -> [Color; 9] {
    [
        p.primary,
        p.primary_text,
        p.tertiary,
        p.surface,
        p.surface_text,
        p.surface_variant,
        p.surface_container,
        p.background,
        p.outline,
    ]
}

fn assert_widget_palette(app: &GpuCandidate, background: Color, text: Color, primary: Color) {
    let actual = theme(app, window::Id::unique()).palette();
    assert_eq!(actual.background, background);
    assert_eq!(actual.text, text);
    assert_eq!(actual.primary, primary);
}

#[test]
fn p5a_chrome_uses_persisted_snapshot_and_ignores_designer_audition() {
    use candidate_state::frontend::{AppliedThemeSnapshot, ThemeOrigin};
    use theme_roles::{ColorVariant, ThemeRole};
    for variant in [ColorVariant::Dark, ColorVariant::Light] {
        let mut app = theme_open_app();
        let document = candidate_state::frontend::nord_document()
            .replaced(ThemeRole::Primary, variant, "#123456")
            .replaced(ThemeRole::Background, variant, "#234567");
        let id = app
            .state
            .theme_library
            .save(None, "Applied fixture", document.clone())
            .unwrap();
        app.state.applied_theme = Some(AppliedThemeSnapshot {
            origin: ThemeOrigin::Custom { record_id: id },
            variant,
            document: document.clone(),
        });
        let expected = appearance_theme::EffectiveThemeColors::from_document(&document, variant);
        let state_before = app.state.clone();
        assert_eq!(
            palette_cells(donor_palette(&app)),
            palette_cells(expected.palette())
        );
        assert_eq!(gallery_snapshot_colors(&app).surface, expected.surface);
        assert_eq!(gallery_snapshot_colors(&app).accent, expected.primary);
        assert_eq!(backdrop_background_rgb(&app), expected.background);
        let expected_palette = expected.palette();
        assert_widget_palette(
            &app,
            expected_palette.background,
            expected_palette.surface_text,
            expected_palette.primary,
        );
        let widget = theme(&app, window::Id::unique())
            .extended_palette()
            .background
            .base
            .color;
        theme_load_fixture(&mut app, variant);
        assert!(
            app.theme_session.editor_active(),
            "The designer owns its visual draft"
        );
        assert_eq!(
            palette_cells(donor_palette(&app)),
            palette_cells(expected.palette())
        );
        assert_eq!(gallery_snapshot_colors(&app).surface, expected.surface);
        assert_eq!(backdrop_background_rgb(&app), expected.background);
        assert_widget_palette(
            &app,
            expected_palette.background,
            expected_palette.surface_text,
            expected_palette.primary,
        );
        assert_eq!(
            theme(&app, window::Id::unique())
                .extended_palette()
                .background
                .base
                .color,
            widget
        );
        assert_eq!(
            app.state, state_before,
            "Loading a visual draft cannot save or replace applied state"
        );
    }
}

#[test]
fn p5a_legacy_none_keeps_honest_appearance_while_designer_previews() {
    let mut app = theme_open_app();
    app.state.applied_theme = None;
    app.state.appearance.background = "#112233".into();
    app.state.appearance.surface = "#223344".into();
    app.state.appearance.text = "#ddeeff".into();
    app.state.appearance.accent = "#345678".into();
    let before = palette_cells(donor_palette(&app));
    let background = backdrop_background_rgb(&app);
    assert_widget_palette(&app, before[7], before[4], before[0]);
    theme_load_fixture(&mut app, theme_roles::ColorVariant::Dark);
    assert!(app.theme_session.editor_active());
    assert_eq!(palette_cells(donor_palette(&app)), before);
    assert_eq!(backdrop_background_rgb(&app), background);
    assert_widget_palette(&app, before[7], before[4], before[0]);
    assert!(app.state.applied_theme.is_none());
}

#[test]
fn p5a_fresh_default_application_uses_complete_fixed_nord() {
    let mut app = theme_open_app();
    app.state = CandidateState::default();
    let snapshot = app.state.applied_theme.as_ref().unwrap();
    let expected =
        appearance_theme::EffectiveThemeColors::from_document(&snapshot.document, snapshot.variant);
    assert_eq!(
        palette_cells(donor_palette(&app)),
        palette_cells(expected.palette())
    );
    assert_eq!(backdrop_background_rgb(&app), expected.background);
    let expected_palette = expected.palette();
    assert_widget_palette(
        &app,
        expected_palette.background,
        expected_palette.surface_text,
        expected_palette.primary,
    );
}

#[test]
fn retirement_export_probe_is_refused_before_launch_side_effects() {
    for args in [
        vec!["app", "--preview-theme-export-probe"],
        vec![
            "app",
            "--preview",
            "--apply-trace",
            "--preview-theme-export-probe",
        ],
    ] {
        assert!(runtime::parse_launch_policy(
            &args.into_iter().map(str::to_string).collect::<Vec<_>>()
        )
        .unwrap_err()
        .contains("retired"));
    }
}

#[test]
fn retirement_theme_exit_cancel_wins_before_or_after_cache_acknowledgment() {
    for cancel_first in [false, true] {
        let mut app = designer_app();
        designer_name(&mut app, "Cache departure draft");
        app.cache_operation = Some(7);
        let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
        designer_decide(&mut app, Choice::Save);
        let exit = app.theme_workflow.exit.unwrap();
        assert!(app.exit_requested && !exit.queued);
        assert_eq!(
            app.state.theme_library.records[0].name,
            "Cache departure draft"
        );
        let _ = update(
            &mut app,
            Message::Wake(Wake::CacheAcknowledged(poster_worker::cache::Report {
                epoch: 6,
                ..Default::default()
            })),
        );
        assert_eq!(
            app.cache_operation,
            Some(7),
            "stale acknowledgment cannot release Exit"
        );
        if cancel_first {
            designer_decide(&mut app, Choice::Cancel);
        }
        let _ = update(
            &mut app,
            Message::Wake(Wake::CacheAcknowledged(poster_worker::cache::Report {
                epoch: 7,
                ..Default::default()
            })),
        );
        assert!(app.cache_operation.is_none());
        if !cancel_first {
            assert!(app.theme_workflow.exit.unwrap().queued);
            designer_decide(&mut app, Choice::Cancel);
        }
        let _ = update(
            &mut app,
            Message::ExitCommitted {
                decision: exit.decision,
            },
        );
        assert!(app.visible && !app.exit_requested && app.theme_workflow.exit.is_none());
        assert!(!app.theme_session.designer_dirty());
    }
}

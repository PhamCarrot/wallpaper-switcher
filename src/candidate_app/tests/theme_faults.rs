//! Theme commands exercise all current injectable failure points and selected
//! directory/inspection failures through the one production commit owner.
//! Initial compatibility refusal is covered by the separate future-backup test.
//! These owned cases do not emulate physical full-disk or device ACL behavior.
use super::theme_workflow::{designer_color, designer_decide, designer_name, designer_save};
use super::*;
use crate::candidate_app::theme_workflow::Choice;
use crate::candidate_state::{
    commit_test::{self, Fault},
    PublicationEffect as Effect, SaveFailurePoint as Point, SaveStage,
};
use sha2::{Digest, Sha256};

fn inventory(root: &std::path::Path) -> serde_json::Value {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        if !entry.path().is_file() {
            continue;
        }
        let bytes = std::fs::read(entry.path()).unwrap();
        files.push(serde_json::json!({"path":entry.path(),"bytes":bytes.len(),"sha256":format!("{:X}",Sha256::digest(&bytes))}));
    }
    files.sort_by_key(|file| file["path"].as_str().unwrap().to_string());
    serde_json::Value::Array(files)
}

#[test]
fn p5_theme_all_save_faults_retain_draft_and_never_activate_proposal_or_exit() {
    let mut faults = Vec::new();
    for point in [
        Point::CreateSnapshot,
        Point::WriteSnapshot,
        Point::FlushSnapshot,
        Point::ReadSnapshot,
        Point::PreparePrimary,
        Point::FlushPrimary,
        Point::PrepareBackup,
        Point::FlushBackup,
        Point::PublishBackup,
        Point::PublishPrimary,
    ] {
        for kind in [
            std::io::ErrorKind::PermissionDenied,
            std::io::ErrorKind::StorageFull,
        ] {
            faults.push((
                Fault::Io(point, kind),
                matches!(
                    point,
                    Point::CreateSnapshot
                        | Point::WriteSnapshot
                        | Point::FlushSnapshot
                        | Point::ReadSnapshot
                ),
            ));
        }
    }
    for point in [Point::PublishBackup, Point::PublishPrimary] {
        for effect in [
            Effect::DestinationUnchanged,
            Effect::DestinationAbsent,
            Effect::OriginalMovedToKnownRecovery,
        ] {
            faults.push((Fault::Publication(point, effect), false));
        }
    }
    for stage in [SaveStage::PublishBackup, SaveStage::PublishPrimary] {
        for backup in [false, true] {
            faults.push((Fault::Future { stage, backup }, false));
        }
    }
    faults.push((Fault::PublishedThenError, false));
    faults.push((Fault::DirectoryParent, false));
    faults.push((Fault::UnreadableInspection { backup: false }, false));
    faults.push((Fault::UnreadableInspection { backup: true }, false));
    assert_eq!(faults.len(), 34);
    for (index, (fault, legacy)) in faults.into_iter().enumerate() {
        for operation in ["save", "save-apply", "exit-save"] {
            let (mut app, submissions) = d3_application_fixture(0);
            let root = app.state_path.parent().unwrap().to_path_buf();
            app.panel_state.open_settings(SettingsCategory::Theme);
            if legacy {
                std::fs::write(
                    &app.state_path,
                    b"# owned legacy primary\nversion=2\nfavorites=['legacy']\n",
                )
                .unwrap();
                std::fs::write(
                    app.state_path.with_extension("toml.bak"),
                    b"# owned distinct backup\nversion=2\nfavorites=['backup']\n",
                )
                .unwrap();
                app.state = CandidateState::load(&app.state_path);
                assert!(app.state.applied_theme.is_none());
                app.ensure_theme_designer();
            } else {
                app.ensure_theme_designer();
                designer_name(&mut app, "Existing");
                designer_save(&mut app, true);
                designer_save(&mut app, false);
            }
            if matches!(fault, Fault::DirectoryParent) {
                let blocker = root.join("owned-file-parent");
                std::fs::write(
                    &blocker,
                    b"owned regular file; cannot contain a child state file",
                )
                .unwrap();
                assert!(blocker.is_file());
                app.state_path = blocker.join("state.toml");
                assert!(!app.state_path.exists());
            }
            designer_name(&mut app, &format!("Fault draft {index} {operation}"));
            designer_color(&mut app, "#123456");
            let document = app.theme_session.editor_document().unwrap().clone();
            let name = app.theme_session.designer_name().to_string();
            let library = app.state.theme_library.clone();
            let applied = app.state.applied_theme.clone();
            let before = inventory(&root);
            let raw = root.join("fault-raw-baseline");
            std::fs::create_dir(&raw).unwrap();
            for path in [root.join("state.toml"), root.join("state.toml.bak")] {
                let bytes = std::fs::read(&path).unwrap();
                std::fs::write(raw.join(path.file_name().unwrap()), &bytes).unwrap();
                assert_eq!(
                    std::fs::read(raw.join(path.file_name().unwrap())).unwrap(),
                    bytes
                );
            }
            let target_before:Vec<_>=[app.state_path.clone(),app.state_path.with_extension("toml.bak")].into_iter().map(|path|{
                let observation=match std::fs::read(&path) {
                    Ok(bytes)=>serde_json::json!({"bytes":bytes.len(),"sha256":format!("{:X}",Sha256::digest(&bytes))}),
                    Err(error)=>serde_json::json!({"readErrorKind":format!("{:?}",error.kind()),"readError":error.to_string()}),
                };
                serde_json::json!({"path":path,"exists":path.exists(),"parentIsRegularFile":path.parent().is_some_and(std::path::Path::is_file),"observation":observation})
            }).collect();
            std::fs::write(
                raw.join("save-target-observations.json"),
                serde_json::to_vec_pretty(&target_before).unwrap(),
            )
            .unwrap();
            let ((), errors) = commit_test::scoped(fault, || {
                if operation == "exit-save" {
                    let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
                    designer_decide(&mut app, Choice::Save);
                } else {
                    designer_save(&mut app, operation == "save-apply");
                }
            });
            assert_eq!(
                errors.len(),
                1,
                "one typed error at the actual commit owner for {fault:?} {operation}"
            );
            let error = &errors[0];
            let expected_stage = match fault {
                Fault::Io(point, _) => match point {
                    Point::CreateSnapshot
                    | Point::WriteSnapshot
                    | Point::FlushSnapshot
                    | Point::ReadSnapshot => SaveStage::PrepareMigration,
                    Point::PreparePrimary | Point::FlushPrimary => SaveStage::PreparePrimary,
                    Point::PrepareBackup | Point::FlushBackup => SaveStage::PrepareBackup,
                    Point::PublishBackup => SaveStage::PublishBackup,
                    Point::PublishPrimary => SaveStage::PublishPrimary,
                },
                Fault::Publication(Point::PublishBackup, _) => SaveStage::PublishBackup,
                Fault::Publication(Point::PublishPrimary, _) | Fault::PublishedThenError => {
                    SaveStage::PublishPrimary
                }
                Fault::Publication(_, _) => panic!("Unselected publication point"),
                Fault::Future { stage, .. } => stage,
                Fault::DirectoryParent => SaveStage::CreateDirectory,
                Fault::UnreadableInspection { .. } => SaveStage::InspectExisting,
            };
            assert_eq!(error.stage, expected_stage, "fault {fault:?} {operation}");
            assert!(app
                .save_failure
                .as_ref()
                .unwrap()
                .contains(&error.to_string()));
            if let Fault::Publication(point, effect) = fault {
                use candidate_state::{BackupStatus, SaveFileStatus};
                assert_eq!(
                    error.primary,
                    Some(
                        if point == Point::PublishPrimary
                            && !matches!(effect, Effect::DestinationUnchanged)
                        {
                            SaveFileStatus::Missing
                        } else {
                            SaveFileStatus::Valid
                        }
                    )
                );
                if point == Point::PublishBackup && !matches!(effect, Effect::DestinationUnchanged)
                {
                    assert_eq!(error.backup, BackupStatus::Unavailable);
                }
                if matches!(effect, Effect::OriginalMovedToKnownRecovery) {
                    assert!(!error.recovery_paths().is_empty());
                    assert!(error.recovery_paths().iter().all(|path| path.exists()));
                }
            }
            assert!(app.save_failure.is_some(), "fault {fault:?} {operation}");
            assert_eq!(
                app.state.theme_library, library,
                "fault {fault:?} {operation}"
            );
            assert_eq!(app.state.applied_theme, applied);
            assert_eq!(app.theme_session.editor_document(), Some(&document));
            assert_eq!(app.theme_session.designer_name(), name);
            assert!(app.theme_session.designer_dirty());
            assert!(!app.exit_requested);
            assert!(app.theme_workflow.exit.is_none());
            assert!(app.visible && app.panel_state.settings_open);
            assert_eq!(submissions.load(Ordering::Acquire), 0);
            let after = inventory(&root);
            let failure = app.save_failure.clone();
            if matches!(fault, Fault::PublishedThenError) {
                assert!(app.state.reconciliation_required.is_some());
                let disk = CandidateState::load(&app.state_path);
                assert!(disk
                    .theme_library
                    .records
                    .iter()
                    .any(|record| record.name == name));
                if operation == "save-apply" {
                    assert_eq!(disk.applied_theme.as_ref().unwrap().document, document);
                }
                assert!(app
                    .save_failure
                    .as_ref()
                    .unwrap()
                    .contains("Disk may have changed"));
                // Retry is explicitly refused, never silently replaces a
                // partially published primary or activates a live proposal.
                designer_save(&mut app, true);
                assert_eq!(inventory(&root), after);
                assert_eq!(app.state.applied_theme, applied);
                app.reload_local_state().unwrap();
                assert!(app.state.reconciliation_required.is_none());
                assert_eq!(app.theme_session.editor_document(), Some(&document));
                assert!(app.theme_session.designer_dirty());
            }
            let applied_after_deliberate_reload = app.state.applied_theme.clone();
            let library_after_deliberate_reload = app.state.theme_library.clone();
            theme_editor_set(&mut app, "#abcdef");
            designer_name(&mut app, "Still editable after failure");
            assert_eq!(app.theme_session.editor_buffer(), Some("#abcdef"));
            assert!(!app.theme_session.editor_pending());
            assert_eq!(
                app.theme_session.designer_name(),
                "Still editable after failure"
            );
            assert!(app.theme_session.designer_dirty());
            assert_eq!(app.state.applied_theme, applied_after_deliberate_reload);
            assert_eq!(app.state.theme_library, library_after_deliberate_reload);
            assert!(!app.exit_requested && app.theme_workflow.exit.is_none());
            assert_eq!(submissions.load(Ordering::Acquire), 0);
            println!(
                "P5_THEME_FAULT_CASE {}",
                serde_json::json!({"index":index,"operation":operation,"fault":format!("{fault:?}"),"directory":root,"saveTarget":app.state_path,"targetBefore":target_before,"rawBaseline":raw,"before":before,"afterFailure":after,"afterDeliberateReload":inventory(&root),"failure":failure,"actualErrorStage":format!("{:?}",error.stage),"expectedErrorStage":format!("{expected_stage:?}"),"actualPrimary":format!("{:?}",error.primary),"actualBackup":format!("{:?}",error.backup),"actualRecoveryPaths":error.recovery_paths(),"postFailureHexAndNameEdits":true,"libraryNotActivatedOnFailure":true,"draftRetained":true,"exitNotAuthorized":true,"providerSubmissions":submissions.load(Ordering::Acquire)})
            );
        }
    }
}

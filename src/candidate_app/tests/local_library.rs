use super::*;
use crate::candidate_app::local_library::TagEdit;
use view::settings::composed::SettingsSource;

fn fixture() -> GpuCandidate {
    let (mut app, _) = d3_application_fixture(3);
    app.panel_state.search_open = true;
    app.search_tags_open = true;
    app.catalog[0].source_tags.insert("source".into());
    app.project_local_tags();
    app.reconcile_tag_editor();
    app
}
fn edit(app: &mut GpuCandidate, edit: TagEdit) {
    app.reconcile_tag_editor();
    let owner = app.tag_owner().unwrap();
    let _ = update(app, Message::LocalTag { owner, edit });
}

#[test]
fn w3_production_tag_edit_query_playlist_reload_orphan_and_source_preservation() {
    let mut app = fixture();
    let project = app.state_path.with_file_name("project.json");
    let raw = br#"{"tags":[" Source ","keep"],"unknown":42}"#;
    std::fs::write(&project, raw).unwrap();
    app.wallpapers[0].project_json_path = project.clone();
    app.catalog[0] = library::CatalogEntry::from_wallpaper(&app.wallpapers[0]);
    edit(&mut app, TagEdit::Text(" Added ".into()));
    edit(&mut app, TagEdit::Add);
    edit(&mut app, TagEdit::Remove("source".into()));
    assert!(!app.catalog[0].tags.contains("source"));
    assert!(app.catalog[0].source_tags.contains("source"));
    assert!(app.catalog[0].tags.contains("added"));
    let _ = update(&mut app, Message::SearchChanged("added".into()));
    assert_eq!(app.filtered_indices, vec![0]);
    let mut rule = SmartRule::default();
    rule.tags.insert("added".into());
    assert!(library::matches_rule(
        &app.catalog[0],
        &rule,
        &app.state.favorites
    ));
    let playlist = Playlist {
        id: "local".into(),
        name: "local".into(),
        membership: PlaylistMembership::Smart { rule },
        order: candidate_state::PlaybackOrder::Sequential,
    };
    assert_eq!(
        library::evaluate_playlist(&playlist, &app.catalog, &app.state.favorites).len(),
        1
    );
    app.state = CandidateState::load(&app.state_path);
    app.catalog.clear(); // disappearance never trims overrides
    let id = app.wallpapers[0].id.clone();
    assert!(app.state.local_library.tag_overrides.contains_key(&id));
    let entries = app
        .wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    let generation = app.catalog_generation.load(Ordering::Acquire);
    let wallpapers = app.wallpapers.clone();
    let _ = update(
        &mut app,
        Message::Wake(Wake::CatalogLoaded {
            generation,
            wallpapers,
            entries,
        }),
    );
    assert!(app.catalog[0].tags.contains("added"));
    assert!(!app.catalog[0].tags.contains("source"));
    assert_eq!(std::fs::read(&project).unwrap(), raw);
}

#[test]
fn w3_stale_item_window_catalog_and_hidden_edits_do_not_publish() {
    let mut app = fixture();
    edit(&mut app, TagEdit::Text("belongs-to-first".into()));
    let owner = app.tag_owner().unwrap();
    let state = app.state.clone();
    app.selected = Some(1);
    edit(&mut app, TagEdit::Add);
    let _ = update(
        &mut app,
        Message::LocalTag {
            owner: owner.clone(),
            edit: TagEdit::Remove("source".into()),
        },
    );
    assert_eq!(app.state, state);
    app.selected = Some(0);
    app.window = Some(window::Id::unique());
    let _ = update(
        &mut app,
        Message::LocalTag {
            owner,
            edit: TagEdit::Remove("source".into()),
        },
    );
    assert_eq!(app.state, state);
    let owner = app.tag_owner().unwrap();
    app.catalog_generation.fetch_add(1, Ordering::AcqRel);
    let _ = update(
        &mut app,
        Message::LocalTag {
            owner,
            edit: TagEdit::Remove("source".into()),
        },
    );
    assert_eq!(app.state, state);
    let owner = app.tag_owner().unwrap();
    app.visible = false;
    let _ = update(
        &mut app,
        Message::LocalTag {
            owner,
            edit: TagEdit::Remove("source".into()),
        },
    );
    assert_eq!(app.state, state);
    assert!(!app.state_path.exists());
}

#[test]
fn w3_partial_tag_publication_keeps_overlay_until_deliberate_reconciliation() {
    use candidate_state::commit_test::{scoped, Fault};
    let mut app = fixture();
    app.state.save(&app.state_path).unwrap();
    edit(&mut app, TagEdit::Text("published-on-disk".into()));
    let old = app.catalog[0].tags.clone();
    let (_, errors) = scoped(Fault::PublishedThenError, || edit(&mut app, TagEdit::Add));
    assert_eq!(errors.len(), 1);
    assert!(app.state.reconciliation_required.is_some());
    assert_eq!(app.catalog[0].tags, old);
    let disk = CandidateState::load(&app.state_path);
    assert!(disk
        .local_library
        .effective_tags(&app.wallpapers[0].id, &app.catalog[0].source_tags)
        .contains("published-on-disk"));
    edit(&mut app, TagEdit::Remove("source".into()));
    assert_eq!(app.catalog[0].tags, old);
    app.reload_local_state().unwrap();
    assert!(app.state.reconciliation_required.is_none());
    assert!(app.catalog[0].tags.contains("published-on-disk"));
    assert!(app.catalog[0].tags.contains("source"));
}

#[test]
fn w3_config_and_tag_failures_remain_separate_and_refresh_requires_config_success() {
    let mut app = fixture();
    let generation = app.catalog_generation.load(Ordering::Acquire);
    let before = app.config.catalog.local_folders.clone();
    let mut config = app.config.clone();
    config
        .catalog
        .local_folders
        .push(app.state_path.with_file_name("new-root"));
    app.publish_library_config_using(config.clone(), |_| {
        Err(crate::config::ConfigSaveError::new(
            crate::config::ConfigSaveStage::PreparePrimary,
            crate::config::ConfigFileStatus::Valid,
            crate::config::ConfigBackupStatus::Preserved,
            "owned injected failure",
        ))
    });
    assert_eq!(app.config.catalog.local_folders, before);
    assert_eq!(app.catalog_generation.load(Ordering::Acquire), generation);
    edit(&mut app, TagEdit::Text("independent".into()));
    edit(&mut app, TagEdit::Add);
    assert!(app.catalog[0].tags.contains("independent"));
    let blocker = app.state_path.with_file_name("state-blocker");
    std::fs::write(&blocker, b"retain").unwrap();
    app.state_path = blocker.join("state.toml");
    let live = app.state.local_library.clone();
    edit(&mut app, TagEdit::Remove("source".into()));
    assert_eq!(app.state.local_library, live);
    assert!(app.catalog[0].tags.contains("source"));
    app.publish_library_config_using(config.clone(), |_| {
        Ok(crate::config::ConfigSaveOutcome {
            backup: crate::config::ConfigBackupStatus::Published,
        })
    });
    assert_eq!(
        app.config.catalog.local_folders,
        config.catalog.local_folders
    );
    assert_eq!(
        app.catalog_generation.load(Ordering::Acquire),
        generation + 1
    );
    assert_eq!(std::fs::read(blocker).unwrap(), b"retain");
}

#[test]
fn w3_folder_defaults_sticky_unavailable_and_retained_mode_start_consumers() {
    let mut app = fixture();
    let unavailable = app.state_path.with_file_name("unavailable");
    app.config.catalog.local_folders = vec![unavailable.clone()];
    app.state.frontend_preferences.filter_bar.default_folder = Some("*".into());
    app.state.frontend_preferences.filter_bar.sticky = Some(true);
    let _ = update(
        &mut app,
        Message::SetFolderFilter(Some(unavailable.clone())),
    );
    let _ = update(&mut app, Message::SearchChanged("item".into()));
    assert!(app.filter_rule.folders.contains(&unavailable));
    assert!(app.available_folders().contains(&unavailable));
    assert!(!unavailable.exists());
    app.state.frontend_preferences.filter_bar.sticky = Some(false);
    let _ = update(&mut app, Message::SearchChanged("".into()));
    assert!(app.filter_rule.folders.is_empty());
    app.state.frontend_preferences.filter_bar.default_folder =
        Some(unavailable.to_string_lossy().into_owned());
    app.reset_default_folder();
    assert!(app.filter_rule.folders.contains(&unavailable));
    app.filter_rule.folders.clear();
    app.refresh_filter();
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Depth,
    ] {
        app.gallery_mode = mode;
        app.gallery_engaged = false;
        app.state.frontend_preferences.picker.start_position = Some("browsing".into());
        app.selected = Some(2);
        app.save_retained_browse_position();
        app.selected = Some(0);
        app.restore_retained_start_position();
        assert_eq!(app.selected, Some(2));
        app.state.frontend_preferences.picker.start_position = Some("beginning".into());
        app.restore_retained_start_position();
        assert_eq!(app.selected, Some(0));
    }
}

#[test]
fn w3_drawer_uses_source_sized_controls_with_media_filters_retained() {
    let mut app = fixture();
    app.wallpapers[1].source = WallpaperSource::LocalVideo;
    app.wallpapers[2].source = WallpaperSource::WallpaperEngine;
    app.wallpapers[2].wallpaper_type = "scene".into();
    assert_eq!(app.wallpapers[1].source, WallpaperSource::LocalVideo);
    assert_eq!(app.wallpapers[2].source, WallpaperSource::WallpaperEngine);
    for viewport in [[1920.0, 1080.0], [2560.0, 1440.0]] {
        app.viewport = viewport;
        app.state.frontend_preferences.tag_panel.width = Some(540.0);
        let mut limits = vec![];
        for rows in ["1", "2", "3"] {
            app.state.frontend_preferences.tag_panel.rows = Some(rows.into());
            let b = app.tag_panel_bounds(app.search_panel_width(), 145.0);
            assert_eq!(b[2], 540.0);
            assert!(
                b[0] >= 0.0
                    && b[1] >= 0.0
                    && b[0] + b[2] <= viewport[0]
                    && b[1] + b[3] <= viewport[1]
            );
            limits.push(app.search_panel_row_limit());
        }
        assert_eq!(limits, [1, 2, 3]);
        let before = app.tag_panel_bounds(app.search_panel_width(), 145.0);
        app.state.frontend_preferences.tag_panel.offset_x = Some(20.0);
        app.state.frontend_preferences.tag_panel.offset_y = Some(-20.0);
        let after = app.tag_panel_bounds(app.search_panel_width(), 145.0);
        assert_eq!(after[0], before[0] + 20.0);
        assert_eq!(after[1], before[1] - 20.0);
        app.state.frontend_preferences.tag_panel.offset_x = None;
        app.state.frontend_preferences.tag_panel.offset_y = None;
    }
}

#[test]
fn w3_actual_injected_apply_completion_hides_only_captured_confirmed_choice() {
    use coordinator::ApplyOperationState as S;
    for state in [
        S::ProviderConfirmed,
        S::CommandAccepted,
        S::Unknown,
        S::Failed,
    ] {
        for close in [false, true] {
            let mut app = mutation_target_fixture(false);
            app.state.frontend_preferences.picker.close_on_selection = Some(close);
            let id = coordinator::fixture_operation_id();
            app.apply_selected_with(
                false,
                || mutation_target_topology(&["DISPLAY-A", "DISPLAY-B"]),
                |_, w, _, _| {
                    Ok(coordinator::ApplySubmission {
                        operation_id: id,
                        wallpaper_id: w.id,
                        target_ids: vec!["DISPLAY-A".into()],
                        state: S::CommandAccepted,
                    })
                },
            );
            assert_eq!(app.hide_after_apply, close);
            assert!(app.visible);
            app.state.frontend_preferences.picker.close_on_selection = Some(!close);
            let result = coordinator::ApplyResult {
                operation_id: id,
                wallpaper_id: app.wallpapers[0].id.clone(),
                outcomes: vec![coordinator::MonitorApplyOutcome {
                    target_id: "DISPLAY-A".into(),
                    state,
                    error: (state == S::Failed).then(|| "injected failure".into()),
                }],
                recoveries: vec![],
                transition: coordinator::TransitionResult::not_requested(),
            };
            let _ = update(
                &mut app,
                Message::Wake(Wake::CoordinatorFinished(CoordinatorResult::Apply(result))),
            );
            assert_eq!(app.visible, !(close && state == S::ProviderConfirmed));
            assert!(!app.hide_after_apply);
        }
    }
}

#[test]
fn w3_effective_binding_opens_delivered_drawer_and_escape_keeps_gallery_usable() {
    use candidate_state::preferences::BindingAction;
    let mut app = fixture();
    app.panel_state.search_open = false;
    app.state
        .frontend_preferences
        .bindings
        .insert(BindingAction::TagCloud, "f12".into());
    app.window_focused = Some(true);
    d3_key(&mut app, keyboard::key::Named::F12);
    assert!(app.panel_state.search_open && app.search_tags_open);
    let selected = app.selected;
    d3_key(&mut app, keyboard::key::Named::ArrowRight);
    assert_eq!(app.selected, selected, "search/editor owns gallery keys");
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.panel_state.search_open && app.visible);
    let search = app.search_input_id.clone();
    let tag_editor = app.local_tag_input_id.clone();
    let event_window = app.window.unwrap();
    let _ = update(
        &mut app,
        Message::KeyPressed {
            key: keyboard::Key::Character("f".into()),
            modifiers: keyboard::Modifiers::CTRL,
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
    assert!(app.panel_state.search_open);
    assert_eq!(app.search_input_id, search);
    assert_eq!(app.local_tag_input_id, tag_editor);
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.panel_state.search_open && app.visible);
    app.state
        .frontend_preferences
        .bindings
        .insert(BindingAction::FolderToggle, "f11".into());
    d3_key(&mut app, keyboard::key::Named::F11);
    assert!(app.folder_menu_open);
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.folder_menu_open && app.visible);
}

#[test]
fn w3_old_same_item_drawer_session_cannot_replay_text_add_or_remove() {
    let mut app = fixture();
    edit(&mut app, TagEdit::Text("old-draft".into()));
    let owner = app.tag_owner().unwrap();
    let old = app.state.clone();
    let _ = update(&mut app, Message::ToggleSearchTags);
    let _ = update(&mut app, Message::ToggleSearchTags);
    assert_ne!(app.tag_owner().unwrap(), owner);
    assert!(app.local_tag_input.is_empty());
    for edit in [
        TagEdit::Text("stale".into()),
        TagEdit::Add,
        TagEdit::Remove("source".into()),
    ] {
        let _ = update(
            &mut app,
            Message::LocalTag {
                owner: owner.clone(),
                edit,
            },
        );
        assert_eq!(app.state, old);
        assert!(app.local_tag_input.is_empty());
    }
    let owner = app.tag_owner().unwrap();
    let _ = update(&mut app, Message::CloseSearchPanel);
    let _ = update(&mut app, Message::OpenSearch);
    assert_ne!(app.tag_owner().unwrap(), owner);
    let _ = update(
        &mut app,
        Message::LocalTag {
            owner,
            edit: TagEdit::Remove("source".into()),
        },
    );
    assert_eq!(app.state, old);
    assert!(!app.state_path.exists());
}

#[test]
fn w3_config_partial_primary_failure_requires_reconciliation_and_retains_tag_owner() {
    let mut app = fixture();
    let previous = app.config.catalog.local_folders.clone();
    let generation = app.catalog_generation.load(Ordering::Acquire);
    let mut proposed = app.config.clone();
    proposed
        .catalog
        .local_folders
        .push(app.state_path.with_file_name("proposal"));
    app.publish_library_config_using(proposed.clone(), |_| {
        Err(crate::config::ConfigSaveError::new(
            crate::config::ConfigSaveStage::PublishPrimary,
            crate::config::ConfigFileStatus::Missing,
            crate::config::ConfigBackupStatus::Preserved,
            "owned partial publication",
        ))
    });
    assert!(app.library_config_reconcile);
    app.active_monitor_id = Some("DISPLAY-A".into());
    app.monitor_ids = vec!["DISPLAY-A".into()];
    let config_before = toml::to_string(&app.config).unwrap();
    let disk_before = std::fs::read(Config::config_path()).ok();
    let failure_before = app.config_save_failure.clone();
    let _ = update(&mut app, Message::ToggleExcluded);
    assert_eq!(toml::to_string(&app.config).unwrap(), config_before);
    assert_eq!(std::fs::read(Config::config_path()).ok(), disk_before);
    assert_eq!(app.config_save_failure, failure_before);
    assert!(app.library_config_reconcile);
    assert!(app.status.as_deref().unwrap().contains("reconciliation"));
    assert_eq!(app.config.catalog.local_folders, previous);
    assert_eq!(app.catalog_generation.load(Ordering::Acquire), generation);
    app.publish_library_config_using(proposed, |_| {
        panic!("must not republish before reconciliation")
    });
    edit(&mut app, TagEdit::Text("independent-tag".into()));
    edit(&mut app, TagEdit::Add);
    assert!(app.catalog[0].tags.contains("independent-tag"));
}

#[test]
fn w3_actual_scanner_raw_config_root_and_default_reach_canonical_catalog() {
    let mut app = fixture();
    let root = app.state_path.with_file_name("ordinary-root");
    std::fs::create_dir_all(&root).unwrap();
    ::image::RgbImage::new(4, 4)
        .save(root.join("owned-image.png"))
        .unwrap();
    let raw = std::path::PathBuf::from(root.to_string_lossy().trim_start_matches(r"\\?\"));
    assert!(!raw.to_string_lossy().starts_with(r"\\?\"));
    let wallpapers = crate::scanner::scan_local_media(&[raw.clone()]);
    assert_eq!(wallpapers.len(), 1);
    let entries = wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    let canonical = root.canonicalize().unwrap();
    app.config.catalog.local_folders = vec![raw.clone()];
    let generation = app.catalog_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::Wake(Wake::CatalogLoaded {
            generation,
            wallpapers,
            entries,
        }),
    );
    let _ = update(&mut app, Message::SetFolderFilter(Some(raw.clone())));
    assert_eq!(app.filtered_indices, vec![0]);
    assert_eq!(app.filter_rule.folders.iter().next(), Some(&canonical));
    assert_eq!(
        app.available_folders()
            .iter()
            .filter(|p| **p == canonical)
            .count(),
        1
    );
    app.state.frontend_preferences.filter_bar.default_folder =
        Some(raw.to_string_lossy().into_owned());
    app.reset_default_folder();
    app.refresh_filter();
    assert_eq!(app.filtered_indices, vec![0]);
    assert_eq!(
        app.config.catalog.local_folders,
        vec![raw],
        "filter normalization never rewrites Config intent"
    );
    assert!(root.join("owned-image.png").is_file());
}

#[test]
fn w3_catalog_publication_revokes_an_editor_minted_during_the_same_scan_generation() {
    let mut app = fixture();
    edit(&mut app, TagEdit::Text("old-scan-body".into()));
    let owner = app.tag_owner().unwrap();
    let before = app.state.clone();
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
    assert_ne!(app.tag_owner().unwrap(), owner);
    let _ = update(
        &mut app,
        Message::LocalTag {
            owner,
            edit: TagEdit::Add,
        },
    );
    assert_eq!(app.state, before);
    assert!(!app.state_path.exists());
}

#[test]
fn w3_installed_folder_picker_and_retained_smart_playlist_preserve_raw_intent() {
    let mut app = fixture();
    let root = app.state_path.with_file_name("installed-style-root");
    std::fs::create_dir_all(&root).unwrap();
    let ordinary = std::path::PathBuf::from(root.to_string_lossy().trim_start_matches(r"\\?\"));
    let canonical = root.canonicalize().unwrap();
    let mut item = app.wallpapers[0].clone();
    item.source = WallpaperSource::WallpaperEngine;
    item.folder_path = ordinary.clone();
    let id = item.id.clone();
    let file = item.file_target.clone();
    let entry = library::CatalogEntry::from_wallpaper(&item);
    assert_eq!(entry.folder, ordinary);
    let generation = app.catalog_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::Wake(Wake::CatalogLoaded {
            generation,
            wallpapers: vec![item],
            entries: vec![entry],
        }),
    );
    let _ = update(&mut app, Message::SetFolderFilter(Some(ordinary.clone())));
    assert_eq!(app.filtered_indices, vec![0]);
    for folder in [ordinary.clone(), canonical.clone()] {
        let mut rule = SmartRule::default();
        rule.folders.insert(folder.clone());
        let playlist = Playlist {
            id: "retained".into(),
            name: "retained".into(),
            order: Default::default(),
            membership: PlaylistMembership::Smart { rule },
        };
        assert_eq!(
            library::evaluate_playlist(&playlist, &app.catalog, &app.state.favorites).len(),
            1
        );
        let PlaylistMembership::Smart { rule } = playlist.membership else {
            unreachable!()
        };
        assert!(rule.folders.contains(&folder));
    }
    app.catalog[0].folder = canonical;
    let mut rule = SmartRule::default();
    rule.folders.insert(ordinary);
    assert!(library::matches_rule(
        &app.catalog[0],
        &rule,
        &app.state.favorites
    ));
    assert_eq!(app.wallpapers[0].id, id);
    assert_eq!(app.wallpapers[0].file_target, file);
}

#[test]
fn r1_expanded_drawer_retains_editor_identity_draft_and_focus_guards() {
    let mut app = fixture();
    app.window_focused = Some(true);
    edit(&mut app, TagEdit::Text("owned draft".into()));
    let owner = app.tag_owner().unwrap();
    let search = app.search_input_id.clone();
    let input = app.local_tag_input_id.clone();
    for viewport in [[1920.0, 1080.0], [2560.0, 1440.0]] {
        app.viewport = viewport;
        let _ = update(&mut app, Message::SheetBackdrop);
        app.reconcile_tag_editor();
        assert_eq!(app.tag_owner(), Some(owner.clone()));
        assert_eq!(app.local_tag_input, "owned draft");
        assert_eq!(app.search_input_id, search);
        assert_eq!(app.local_tag_input_id, input);
    }
    let selected = app.selected;
    d3_key(&mut app, keyboard::key::Named::ArrowRight);
    assert_eq!(app.selected, selected);
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.panel_state.search_open && app.visible);
    let before = app.state.clone();
    let _ = update(
        &mut app,
        Message::LocalTag {
            owner,
            edit: TagEdit::Add,
        },
    );
    assert_eq!(
        app.state, before,
        "closed editor cannot publish stale draft"
    );
}

#[test]
fn search_tags_captured_drawer_survives_filter_and_reorder_without_retargeting() {
    let mut app = fixture();
    let item = app.wallpapers[0].id.clone();
    let _ = update(&mut app, Message::OpenLocalTags(item.clone()));
    let owner = app.tag_owner().unwrap();
    edit(&mut app, TagEdit::Text("  夜空 Tag  ".into()));
    edit(&mut app, TagEdit::Add);
    assert!(app.catalog[0].tags.contains("夜空 tag"));
    let _ = update(&mut app, Message::SearchTag("source".into(), false));
    assert_eq!(app.filtered_indices, vec![0]);
    edit(&mut app, TagEdit::Remove("source".into()));
    assert!(app.filtered_indices.is_empty());
    assert_eq!(app.selected, None);
    assert_eq!(app.tag_owner(), Some(owner.clone()));
    assert_eq!(app.tag_drawer_item.as_deref(), Some(item.as_str()));
    assert!(app.catalog[0].source_tags.contains("source"));
    assert!(app.state.local_library.tag_overrides[&item]
        .removed
        .contains("source"));
    let _ = update(&mut app, Message::ClearSearchTags);
    let _ = update(&mut app, Message::CycleSort);
    app.selected = Some(1);
    edit(&mut app, TagEdit::Text("captured only".into()));
    edit(&mut app, TagEdit::Add);
    assert!(app.catalog[0].tags.contains("captured only"));
    assert!(!app.catalog[1].tags.contains("captured only"));
    assert_eq!(app.tag_owner(), Some(owner));
    app.reload_local_state().unwrap();
    assert!(app.catalog[0].tags.contains("夜空 tag"));
    assert!(!app.catalog[0].tags.contains("source"));
}

#[test]
fn search_tags_drawer_dismiss_reopen_catalog_window_hide_reject_stale_edits() {
    let mut app = fixture();
    let item = app.wallpapers[0].id.clone();
    for revoke in 0..5 {
        app.visible = true;
        app.panel_state.search_open = true;
        let _ = update(&mut app, Message::OpenLocalTags(item.clone()));
        let owner = app.tag_owner().unwrap();
        edit(&mut app, TagEdit::Text("stale draft".into()));
        let before = app.state.clone();
        match revoke {
            0 => {
                let _ = update(&mut app, Message::CloseSearchPanel);
            }
            1 => {
                let _ = update(&mut app, Message::BackToSearchFilters);
            }
            2 => {
                app.catalog_generation.fetch_add(1, Ordering::AcqRel);
            }
            3 => {
                app.window = Some(window::Id::unique());
            }
            _ => {
                app.visible = false;
            }
        }
        for edit in [
            TagEdit::Add,
            TagEdit::Remove("source".into()),
            TagEdit::Text("late".into()),
        ] {
            let _ = update(
                &mut app,
                Message::LocalTag {
                    owner: owner.clone(),
                    edit,
                },
            );
        }
        assert_eq!(app.state, before, "revoke {revoke}");
        assert!(app.local_tag_input.is_empty());
    }
}

#[test]
fn search_tags_empty_duplicate_cjk_and_publish_failure_keep_source_and_live_truth() {
    use candidate_state::commit_test::{scoped, Fault};
    let mut app = fixture();
    let item = app.wallpapers[0].id.clone();
    let _ = update(&mut app, Message::OpenLocalTags(item));
    edit(&mut app, TagEdit::Text("  ".into()));
    let before = app.state.clone();
    edit(&mut app, TagEdit::Add);
    assert_eq!(app.state, before);
    assert_eq!(app.local_tag_input, "  ");
    for raw in ["  夜空 TAG  ", "夜空 tag"] {
        edit(&mut app, TagEdit::Text(raw.into()));
        edit(&mut app, TagEdit::Add);
    }
    assert_eq!(
        app.catalog[0]
            .tags
            .iter()
            .filter(|t| t.as_str() == "夜空 tag")
            .count(),
        1
    );
    let before = app.catalog[0].tags.clone();
    edit(&mut app, TagEdit::Text("failed".into()));
    let (_, errors) = scoped(Fault::PublishedThenError, || edit(&mut app, TagEdit::Add));
    assert_eq!(errors.len(), 1);
    assert_eq!(app.catalog[0].tags, before);
    assert_eq!(app.local_tag_input, "failed");
    assert!(app.state.reconciliation_required.is_some());
    assert!(app.catalog[0].source_tags.contains("source"));
    app.reload_local_state().unwrap();
    assert!(app.catalog[0].tags.contains("failed"));
}

#[test]
fn search_tags_detail_action_opens_current_captured_editor_without_apply() {
    let mut app = fixture();
    app.panel_state.search_open = false;
    app.detail_open = true;
    app.detail_progress = 1.0;
    app.gallery_mode = GalleryMode::Wall;
    let item = app.wallpapers[0].id.clone();
    let _ = update(
        &mut app,
        Message::DonorDetail(
            "stale-item".into(),
            donor_back_layout::DetailIntent::LocalTags,
        ),
    );
    assert!(app.tag_drawer_item.is_none());
    let _ = update(
        &mut app,
        Message::DonorDetail(item.clone(), donor_back_layout::DetailIntent::LocalTags),
    );
    assert_eq!(app.tag_drawer_item, Some(item));
    assert!(app.detail_open && app.panel_state.search_open);
    let selected = app.selected;
    d3_key(&mut app, keyboard::key::Named::Enter);
    assert_eq!(app.selected, selected);
    assert!(!app.apply_pending);
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.panel_state.search_open);
    assert!(app.detail_open);
}

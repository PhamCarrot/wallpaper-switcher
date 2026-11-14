use super::*;
use crate::candidate_app::playlists::Edit;

fn fixture(picker: bool) -> GpuCandidate {
    let (mut app, _) = d3_application_fixture(3);
    app.state.save(&app.state_path).unwrap();
    app.state = CandidateState::load(&app.state_path);
    app.launch_policy = p1_preview_policy();
    app.selected = Some(0);
    app.gallery_mode = GalleryMode::Geometric;
    app.detail_open = picker;
    let item = app.wallpapers[0].id.clone();
    let _ = update(
        &mut app,
        if picker {
            Message::DonorDetail(item, donor_back_layout::DetailIntent::Playlists)
        } else {
            Message::TogglePlaylistPanel
        },
    );
    assert!(app.playlist_panel_open);
    assert_eq!(app.playlist_picker, picker);
    app
}

fn edit(app: &mut GpuCandidate, action: Edit) {
    let owner = app.playlist_owner().unwrap();
    let _ = update(app, Message::PlaylistEdit(owner, action));
}
fn create(app: &mut GpuCandidate, name: &str) -> String {
    edit(app, Edit::Name(name.into()));
    edit(app, Edit::Create);
    app.state.playlists.last().unwrap().id.clone()
}
fn delete(app: &mut GpuCandidate, id: &str) {
    edit(app, Edit::DeleteRequest(id.into()));
    assert!(app.playlist_delete.is_some());
    edit(app, Edit::DeleteConfirm);
}

#[test]
fn playlist_picker_detail_route_creation_membership_idempotence_and_reload() {
    let mut app = fixture(true);
    let id = create(&mut app, "Current card");
    let item = app.wallpapers[0].id.clone();
    let owner = app.playlist_owner().unwrap();
    let remove = Message::PlaylistEdit(
        owner.clone(),
        Edit::Membership {
            playlist: id.clone(),
            included: false,
        },
    );
    let _ = update(&mut app, remove.clone());
    let _ = update(&mut app, remove);
    let stale_add = Message::PlaylistEdit(
        owner,
        Edit::Membership {
            playlist: id.clone(),
            included: true,
        },
    );
    let _ = update(&mut app, stale_add);
    assert!(
        matches!(&app.state.playlists[0].membership, PlaylistMembership::Explicit { wallpaper_ids } if wallpaper_ids.is_empty())
    );
    edit(
        &mut app,
        Edit::Membership {
            playlist: id,
            included: true,
        },
    );
    assert!(
        matches!(&CandidateState::load(&app.state_path).playlists[0].membership, PlaylistMembership::Explicit { wallpaper_ids } if wallpaper_ids == &vec![item])
    );
    assert!(app.state.active_playlist_id.is_none());
    assert!(!app.state.slideshow_enabled && !app.apply_pending);
    let _ = view::view(&app, app.window.unwrap());
}

#[test]
fn playlist_picker_stale_selection_window_catalog_visibility_and_closed_callbacks_refuse() {
    for change in 0..6 {
        let mut app = fixture(true);
        let id = create(&mut app, "Stable");
        let owner = app.playlist_owner().unwrap();
        let before = app.state.clone();
        let disk = std::fs::read(&app.state_path).unwrap();
        match change {
            0 => app.selected = Some(1),
            1 => app.window = Some(window::Id::unique()),
            2 => {
                app.catalog_generation.fetch_add(1, Ordering::AcqRel);
            }
            3 => app.visibility_generation += 1,
            4 => {
                edit(&mut app, Edit::Close);
                let _ = update(&mut app, Message::OpenPlaylistPicker);
            }
            _ => app.panel_state.search_open = true,
        }
        let _ = update(
            &mut app,
            Message::PlaylistEdit(
                owner.clone(),
                Edit::Membership {
                    playlist: id,
                    included: false,
                },
            ),
        );
        let _ = update(
            &mut app,
            Message::PlaylistEdit(owner.clone(), Edit::Name("stale".into())),
        );
        let _ = update(&mut app, Message::PlaylistEdit(owner, Edit::Create));
        assert_eq!(app.state, before, "context {change}");
        assert_eq!(std::fs::read(&app.state_path).unwrap(), disk);
    }
}

#[test]
fn playlist_picker_smart_lists_refuse_manual_membership_and_dismiss_preserves_detail() {
    let mut app = fixture(true);
    app.state.playlists.push(Playlist {
        id: "smart".into(),
        name: "Smart".into(),
        order: PlaybackOrder::Sequential,
        membership: PlaylistMembership::Smart {
            rule: SmartRule::default(),
        },
    });
    let before = app.state.clone();
    edit(
        &mut app,
        Edit::Membership {
            playlist: "smart".into(),
            included: true,
        },
    );
    assert_eq!(app.state, before);
    edit(&mut app, Edit::Close);
    assert!(!app.playlist_panel_open && !app.playlist_picker && app.detail_open);
    app.selected = None;
    app.detail_open = false;
    let _ = update(&mut app, Message::TogglePlaylistPanel);
    assert!(app.playlist_panel_open && !app.playlist_picker);
    let _ = view::view(&app, app.window.unwrap());
}

#[test]
fn playlist_delete_active_disables_retained_references_reloads_and_revokes_old_timer() {
    let mut app = fixture(false);
    let id = create(&mut app, "Delete active");
    let keep = create(&mut app, "Keep");
    app.state.active_playlist_id = Some(id.clone());
    app.state.slideshow_enabled = true;
    app.state.schedules = vec![
        candidate_state::Schedule {
            id: "linked".into(),
            name: "Keep schedule settings".into(),
            enabled: true,
            playlist_id: id.clone(),
            days: 42,
            minute_of_day: 321,
            priority: 7,
            creation_order: 99,
            monitor_ids: vec!["display".into()],
        },
        candidate_state::Schedule {
            id: "other".into(),
            enabled: true,
            playlist_id: keep,
            ..Default::default()
        },
    ];
    let mut expected = app.state.clone();
    expected.playlists.retain(|p| p.id != id);
    expected.active_playlist_id = None;
    expected.slideshow_enabled = false;
    expected.automation_intent = Some(candidate_state::AutomationIntent {
        safely_stopped: true,
        ..Default::default()
    });
    expected.schedules[0].enabled = false;
    let generation = app.automation_generation.load(Ordering::Acquire);
    delete(&mut app, &id);
    assert_eq!(app.state, expected);
    assert_eq!(CandidateState::load(&app.state_path), expected);
    assert!(app.automation_generation.load(Ordering::Acquire) > generation);
    assert!(!app.apply_pending);
}

#[test]
fn playlist_delete_nonactive_last_and_cancel_preserve_other_data_and_media() {
    let mut app = fixture(false);
    let id = create(&mut app, "Ordinary");
    let keep = create(&mut app, "Keep active");
    app.state.active_playlist_id = Some(keep.clone());
    app.state.slideshow_enabled = true;
    app.state
        .local_library
        .tag_overrides
        .insert("orphan".into(), Default::default());
    let media = app.state_path.with_file_name("owned-media.jpg");
    std::fs::write(&media, b"retained media bytes").unwrap();
    let before = app.state.clone();
    edit(&mut app, Edit::DeleteRequest(id.clone()));
    edit(&mut app, Edit::DeleteCancel);
    assert_eq!(app.state, before);
    let mut expected = before;
    expected.playlists.retain(|p| p.id != id);
    delete(&mut app, &id);
    assert_eq!(app.state, expected);
    delete(&mut app, &keep);
    assert!(app.state.playlists.is_empty());
    assert!(app.state.active_playlist_id.is_none() && !app.state.slideshow_enabled);
    assert_eq!(std::fs::read(media).unwrap(), b"retained media bytes");
    let _ = view::view(&app, app.window.unwrap());
}

#[test]
fn playlist_delete_changed_record_or_references_and_dismissed_confirm_refuse() {
    for change in 0..3 {
        let mut app = fixture(false);
        let id = create(&mut app, "Original");
        edit(&mut app, Edit::DeleteRequest(id.clone()));
        let owner = app.playlist_owner().unwrap();
        match change {
            0 => app.state.playlists[0].name = "Newer".into(),
            1 => app.state.schedules.push(candidate_state::Schedule {
                id: "new".into(),
                playlist_id: id,
                ..Default::default()
            }),
            _ => edit(&mut app, Edit::DeleteCancel),
        }
        let before = app.state.clone();
        let _ = update(&mut app, Message::PlaylistEdit(owner, Edit::DeleteConfirm));
        assert_eq!(app.state, before);
        assert!(app.playlist_panel_open);
    }
}

#[test]
fn playlist_delete_prompt_session_rejects_old_confirm_for_new_target_and_underlay_edits() {
    let mut app = fixture(false);
    let first = create(&mut app, "First");
    let second = create(&mut app, "Second");
    let underlay = app.playlist_owner().unwrap();
    edit(&mut app, Edit::DeleteRequest(first));
    let old_confirm = app.playlist_owner().unwrap();
    let before = app.state.clone();
    let _ = update(&mut app, Message::PlaylistEdit(underlay, Edit::Create));
    let _ = update(&mut app, Message::TogglePlaylistMembership(second.clone()));
    assert_eq!(app.state, before);
    edit(&mut app, Edit::DeleteCancel);
    edit(&mut app, Edit::DeleteRequest(second.clone()));
    let _ = update(
        &mut app,
        Message::PlaylistEdit(old_confirm, Edit::DeleteConfirm),
    );
    assert_eq!(app.state, before);
    assert_eq!(app.playlist_delete.as_ref().unwrap().playlist.id, second);
    edit(&mut app, Edit::DeleteConfirm);
    assert_eq!(app.state.playlists.len(), 1);
    assert!(!app.state.playlists.iter().any(|p| p.id == second));
}

#[test]
fn playlist_edit_future_and_partial_publication_never_install_failed_proposals() {
    use candidate_state::commit_test::{scoped, Fault};
    for fault in [
        Fault::Future {
            stage: candidate_state::SaveStage::InspectExisting,
            backup: true,
        },
        Fault::PublishedThenError,
    ] {
        let mut app = fixture(false);
        let id = create(&mut app, "Protected");
        app.state.save(&app.state_path).unwrap();
        let before = app.state.clone();
        let (((), ()), errors) = scoped(fault, || {
            edit(&mut app, Edit::DeleteRequest(id));
            edit(&mut app, Edit::DeleteConfirm);
            ((), ())
        });
        assert!(!errors.is_empty());
        assert_eq!(app.state.playlists, before.playlists);
        assert_eq!(app.state.active_playlist_id, before.active_playlist_id);
        assert!(app.save_failure.is_some() && app.playlist_panel_open);
        edit(&mut app, Edit::DeleteCancel);
        edit(&mut app, Edit::Name("Retained failed draft".into()));
        edit(&mut app, Edit::Create);
        assert_eq!(app.state.playlists, before.playlists);
        assert_eq!(app.playlist_name, "Retained failed draft");
        let bytes = std::fs::read(&app.state_path).unwrap();
        let _ = CandidateState::load_with_diagnostics(&app.state_path);
        assert_eq!(
            std::fs::read(&app.state_path).unwrap(),
            bytes,
            "load stays read only"
        );
    }
    let mut app = fixture(true);
    let id = create(&mut app, "Retained");
    let before = app.state.playlists.clone();
    let (_, errors) = scoped(Fault::PublishedThenError, || {
        edit(
            &mut app,
            Edit::Membership {
                playlist: id,
                included: false,
            },
        )
    });
    assert!(!errors.is_empty());
    assert_eq!(app.state.playlists, before);
    assert!(app.state.reconciliation_required.is_some());
}

//! Playlist edits share the existing saved-state publication owner.
use super::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Owner {
    window: Option<window::Id>,
    visibility: u64,
    catalog: u64,
    item: Option<String>,
    picker: bool,
    browse: bool,
    session: u64,
}

#[derive(Debug, Clone)]
pub(super) enum Browse {
    Query(String),
    Choose(Option<String>),
    Edit,
    Close,
}

#[derive(Debug, Clone)]
pub(super) enum Edit {
    Name(String),
    Create,
    Membership { playlist: String, included: bool },
    Close,
    DeleteRequest(String),
    DeleteCancel,
    DeleteConfirm,
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Deletion {
    pub(super) playlist: Playlist,
    pub(super) active: bool,
    intent: candidate_state::AutomationIntent,
    pub(super) schedules: Vec<candidate_state::Schedule>,
}

impl Deletion {
    fn capture(state: &CandidateState, id: &str) -> Option<Self> {
        Some(Self {
            playlist: state.playlists.iter().find(|p| p.id == id)?.clone(),
            active: state.active_playlist_id.as_deref() == Some(id),
            intent: state.automation_intent(),
            schedules: state
                .schedules
                .iter()
                .filter(|s| s.playlist_id == id)
                .cloned()
                .collect(),
        })
    }
}

impl GpuCandidate {
    pub(super) fn playlist_owner(&self) -> Option<Owner> {
        if !self.visible
            || self.exit_requested
            || !self.playlist_panel_open
            || self.window.is_none()
            || self.engine_properties_open
            || self.panel_state.search_open
            || self.panel_state.settings_open
            || self.controls_panel_open
            || self.folder_menu_open
        {
            return None;
        }
        let item = (!self.playlist_chooser)
            .then(|| {
                self.selected
                    .filter(|i| self.filtered_indices.contains(i))
                    .and_then(|i| self.wallpapers.get(i))
                    .map(|w| w.id.clone())
            })
            .flatten();
        if self.playlist_picker && (!self.detail_open || item.is_none()) {
            return None;
        }
        Some(Owner {
            window: self.window,
            visibility: self.visibility_generation,
            catalog: self.catalog_generation.load(Ordering::Acquire),
            item,
            picker: self.playlist_picker,
            browse: self.playlist_chooser,
            session: self.playlist_session,
        })
    }

    pub(super) fn reconcile_playlist_editor(&mut self) {
        if !self.playlist_panel_open {
            self.playlist_chooser = false;
        }
        let current = self.playlist_owner().map(|mut o| {
            o.session = 0;
            o
        });
        if current != self.playlist_context {
            let departing_picker = self.playlist_context.as_ref().is_some_and(|o| o.picker);
            self.playlist_context = current;
            self.playlist_session = self.playlist_session.wrapping_add(1);
            self.playlist_delete = None;
            self.playlist_name.clear();
            if departing_picker {
                self.playlist_picker = false;
                self.playlist_panel_open = false;
                self.playlist_context = None;
            }
        }
    }

    pub(super) fn browse_playlist(&mut self, owner: Owner, action: Browse) {
        if !self.playlist_chooser || self.playlist_owner().as_ref() != Some(&owner) {
            return;
        }
        match action {
            Browse::Query(query) => {
                self.playlist_query = query;
                self.playlist_session = self.playlist_session.wrapping_add(1);
            }
            Browse::Choose(id) => {
                // A deliberate browse choice owns selection over unconsumed startup positioning.
                self.gallery_engaged = true;
                if id
                    .as_ref()
                    .is_some_and(|id| !self.state.playlists.iter().any(|p| &p.id == id))
                {
                    self.browse_playlist_unavailable = true;
                    self.browse_playlist_id = None;
                } else {
                    self.browse_playlist_id = id;
                    self.browse_playlist_unavailable = false;
                }
                self.playlist_panel_open = false;
                self.playlist_chooser = false;
                self.refresh_filter();
                self.schedule_posters();
            }
            Browse::Edit => {
                self.playlist_chooser = false;
                self.playlist_browsing_id = self.browse_playlist_id.clone();
            }
            Browse::Close => {
                self.playlist_panel_open = false;
                self.playlist_chooser = false;
            }
        }
    }

    pub(super) fn create_playlist(&mut self, smart: bool) {
        let name = self.playlist_name.trim().to_string();
        if name.is_empty() {
            return;
        }
        let id = unique_playlist_id(&name, &self.state.playlists);
        let membership = if smart {
            PlaylistMembership::Smart {
                rule: self.filter_rule.clone(),
            }
        } else {
            PlaylistMembership::Explicit {
                wallpaper_ids: self
                    .selected
                    .filter(|i| self.filtered_indices.contains(i))
                    .and_then(|i| self.wallpapers.get(i))
                    .map_or_else(Vec::new, |w| vec![w.id.clone()]),
            }
        };
        let mut proposed = self.state.clone();
        proposed.playlists.push(Playlist {
            id: id.clone(),
            name,
            order: PlaybackOrder::Sequential,
            membership,
        });
        if self.commit_local_state_change(proposed).is_ok() {
            self.playlist_session = self.playlist_session.wrapping_add(1);
            self.playlist_browsing_id = Some(id);
            self.playlist_name.clear();
        }
    }

    pub(super) fn set_playlist_membership(&mut self, playlist: &str, item: &str, included: bool) {
        let mut proposed = self.state.clone();
        let Some(Playlist {
            membership: PlaylistMembership::Explicit { wallpaper_ids },
            ..
        }) = proposed.playlists.iter_mut().find(|p| p.id == playlist)
        else {
            return;
        };
        if included == wallpaper_ids.iter().any(|id| id == item) {
            return;
        }
        wallpaper_ids.retain(|id| id != item);
        if included {
            wallpaper_ids.push(item.to_string());
        }
        if self.commit_local_state_change(proposed).is_ok() {
            self.playlist_session = self.playlist_session.wrapping_add(1);
            if self.browse_playlist_id.is_some() {
                self.refresh_filter();
                self.schedule_posters();
            }
        }
    }

    pub(super) fn edit_playlist(&mut self, owner: Owner, edit: Edit) {
        if owner.browse || self.playlist_owner().as_ref() != Some(&owner) {
            return;
        }
        if self.playlist_delete.is_some()
            && !matches!(edit, Edit::DeleteCancel | Edit::DeleteConfirm | Edit::Close)
        {
            return;
        }
        match edit {
            Edit::Name(name) => self.playlist_name = name,
            Edit::Create => self.create_playlist(false),
            Edit::Membership { playlist, included } => {
                if let Some(item) = owner.item {
                    self.set_playlist_membership(&playlist, &item, included);
                }
            }
            Edit::Close => {
                self.playlist_panel_open = false;
                self.playlist_picker = false;
            }
            Edit::DeleteRequest(id) if !owner.picker => {
                self.playlist_delete = Deletion::capture(&self.state, &id);
                self.playlist_session = self.playlist_session.wrapping_add(1);
            }
            Edit::DeleteCancel => {
                self.playlist_delete = None;
                self.playlist_session = self.playlist_session.wrapping_add(1);
            }
            Edit::DeleteConfirm if !owner.picker => {
                let Some(target) = self.playlist_delete.clone() else {
                    return;
                };
                let id = &target.playlist.id;
                if Deletion::capture(&self.state, id).as_ref() != Some(&target) {
                    self.playlist_delete = None;
                    self.status = Some(
                        "Playlist or rotation references changed. Request Delete again.".into(),
                    );
                    return;
                }
                let mut proposed = self.state.clone();
                proposed.playlists.retain(|p| &p.id != id);
                if target.active {
                    proposed.active_playlist_id = None;
                    proposed.slideshow_enabled = false;
                    let mut intent = self.state.automation_intent();
                    intent.playlist_playing = false;
                    intent.safely_stopped = true;
                    proposed.automation_intent = Some(intent);
                }
                for schedule in &mut proposed.schedules {
                    if &schedule.playlist_id == id {
                        schedule.enabled = false;
                    }
                }
                if self.commit_local_state_change(proposed).is_ok() {
                    if target.active {
                        self.automation_suspended = true;
                    }
                    self.sync_automation_controls();
                    if self.browse_playlist_id.is_some() {
                        self.refresh_filter();
                        self.schedule_posters();
                    }
                    self.playlist_browsing_id = None;
                    self.playlist_delete = None;
                    self.playlist_session = self.playlist_session.wrapping_add(1);
                    self.refresh_automation_timer();
                    self.status = Some("Playlist deleted. Referencing schedules retained and disabled; wallpaper files unchanged. Active deletion safely stops rotation until explicit Resume random or Start playlist.".into());
                } else if self.state.reconciliation_required.is_some() && target.active {
                    self.automation_suspended = true;
                    self.refresh_automation_timer();
                    self.sync_automation_controls();
                }
            }
            Edit::DeleteConfirm => {}
            Edit::DeleteRequest(_) => {}
        }
    }
}

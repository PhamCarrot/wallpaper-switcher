//! W3 local edits reuse the existing publication, catalog and sheet owners.
use super::*;
use view::settings::composed::SettingsSource;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct TagOwner {
    window: Option<window::Id>,
    visibility: u64,
    catalog: u64,
    item: String,
    session: u64,
}
#[derive(Debug, Clone)]
pub(super) enum TagEdit {
    Text(String),
    Add,
    Remove(String),
}

impl TagOwner {
    pub(super) fn item_id(&self) -> &str {
        &self.item
    }
}

impl GpuCandidate {
    pub(super) fn tag_owner(&self) -> Option<TagOwner> {
        Some(TagOwner {
            window: self.window,
            visibility: self.visibility_generation,
            catalog: self.catalog_generation.load(Ordering::Acquire),
            item: match &self.tag_drawer_item {
                Some(item) => item.clone(),
                None => self.wallpapers.get(self.selected?)?.id.clone(),
            },
            session: self.tag_edit_session,
        })
    }

    pub(super) fn reconcile_tag_editor(&mut self) {
        if !self.visible || self.exit_requested || !self.panel_state.search_open {
            self.tag_drawer_item = None;
        }
        let current = (self.visible
            && !self.exit_requested
            && self.panel_state.search_open
            && (self.search_tags_open || self.tag_drawer_item.is_some())
            && !self.panel_state.settings_open
            && !self.playlist_panel_open
            && !self.engine_properties_open
            && !self.controls_panel_open
            && !self.folder_menu_open)
            .then(|| self.tag_owner())
            .flatten()
            .map(|mut owner| {
                owner.session = 0;
                owner
            });
        if self.tag_editor_context != current {
            self.tag_editor_context = current;
            self.invalidate_tag_editor();
        }
    }

    pub(super) fn invalidate_tag_editor(&mut self) {
        self.tag_edit_session = self.tag_edit_session.wrapping_add(1);
        self.local_tag_draft_owner = None;
        self.local_tag_input.clear();
    }

    pub(super) fn revoke_catalog_tag_callbacks(&mut self) {
        let owner = self.tag_owner();
        let keep = owner.as_ref().is_some_and(|owner| {
            self.tag_editor_context.is_some()
                && self.local_tag_draft_owner.as_ref() == Some(owner)
                && self.catalog.iter().any(|entry| entry.id == owner.item)
        });
        self.tag_edit_session = self.tag_edit_session.wrapping_add(1);
        self.local_tag_draft_owner = keep.then(|| self.tag_owner()).flatten();
        if !keep {
            self.local_tag_input.clear();
        }
    }

    pub(super) fn project_local_tags(&mut self) {
        for entry in &mut self.catalog {
            entry.tags = self
                .state
                .local_library
                .effective_tags(&entry.id, &entry.source_tags);
        }
    }

    pub(super) fn edit_local_tag(&mut self, owner: TagOwner, edit: TagEdit) {
        if !self.visible
            || self.exit_requested
            || self.window.is_none()
            || self.tag_owner().as_ref() != Some(&owner)
            || !self.panel_state.search_open
            || (!self.search_tags_open && self.tag_drawer_item.is_none())
            || self.panel_state.settings_open
            || self.playlist_panel_open
            || self.engine_properties_open
            || self.controls_panel_open
            || self.folder_menu_open
            || !self.catalog.iter().any(|e| e.id == owner.item)
        {
            return;
        }
        let (raw, add) = match edit {
            TagEdit::Text(raw) => {
                self.local_tag_draft_owner = Some(owner.clone());
                self.local_tag_input = raw;
                return;
            }
            TagEdit::Add => {
                if self.local_tag_draft_owner.as_ref() != Some(&owner) {
                    return;
                }
                (self.local_tag_input.clone(), true)
            }
            TagEdit::Remove(tag) => (tag, false),
        };
        let mut proposed = self.state.clone();
        if let Err(error) = proposed.local_library.edit(&owner.item, &raw, add) {
            self.status = Some(error);
            return;
        }
        if self.commit_local_state_change(proposed).is_ok() {
            self.local_tag_input.clear();
            self.local_tag_draft_owner = None;
            self.project_local_tags();
            self.refresh_filter();
            self.schedule_posters();
            self.status = Some("Local tags saved. Source files unchanged.".into());
        }
    }

    pub(super) fn add_local_root(&mut self) {
        let path = std::path::PathBuf::from(self.folder_path_input.trim());
        if !path.is_absolute() || path.as_os_str().len() > 4096 {
            self.status = Some("Enter an absolute local folder path (at most 4096 characters). Unavailable folders may be retained.".into());
            return;
        }
        let mut config = self.config.clone();
        if !config.catalog.local_folders.contains(&path) {
            config.catalog.local_folders.push(path);
        }
        self.publish_library_config(config);
    }

    pub(super) fn publish_library_config(&mut self, proposed: Config) {
        self.publish_library_config_using(proposed, |config| config.save_with_diagnostics());
    }

    pub(super) fn publish_library_config_using(
        &mut self,
        proposed: Config,
        save: impl FnOnce(
            &Config,
        )
            -> Result<crate::config::ConfigSaveOutcome, crate::config::ConfigSaveError>,
    ) {
        if self.library_config_reconcile {
            self.status = Some("Config publication needs reconciliation. Close and reopen this owned profile before another folder edit; recovery files were retained.".into());
            return;
        }
        let result = save(&proposed);
        self.settings_workbench
            .announce_save(view::settings::bridge::SaveNotice::config_result(
                "Library folders",
                result.as_ref(),
            ));
        match result {
            Ok(outcome) => {
                self.config = proposed;
                self.config_save_failure = (!outcome.backup.is_available()).then(|| {
                    outcome.backup_description(&Config::config_path().with_extension("toml.bak"))
                });
                self.folder_path_input.clear();
                self.refresh_catalog();
                let refresh_status = self.status.take().unwrap_or_default();
                self.status = Some(format!(
                    "Library Config saved. {refresh_status} {}. Unlisted roots are never deleted.",
                    outcome.backup_description(&Config::config_path().with_extension("toml.bak"))
                ));
            }
            Err(error) => {
                self.library_config_reconcile =
                    matches!(error.stage, crate::config::ConfigSaveStage::PublishPrimary);
                let message = format!("Library Config save failed: {error}. Live roots and tags were retained; no refresh requested.{}", if self.library_config_reconcile { " Disk may have changed; preserve recovery files and close/reopen to reconcile." } else { "" });
                self.config_save_failure = Some(message.clone());
                self.status = Some(message);
            }
        }
    }

    pub(super) fn reset_default_folder(&mut self) {
        let raw = (view::settings::bridge::Source { app: self }).text("filterBar.defaultFolder");
        self.filter_rule.folders.clear();
        let folder = match raw.as_str() {
            "*" => None,
            "" => self.config.catalog.local_folders.first().cloned(),
            _ => Some(raw.into()),
        };
        if let Some(folder) = folder {
            self.filter_rule
                .folders
                .insert(catalog::folder_filter_path(folder));
        }
    }
    pub(super) fn reset_nonsticky_folder(&mut self) {
        if !(view::settings::bridge::Source { app: self }).flag("filterBar.sticky") {
            self.reset_default_folder();
        }
    }

    pub(super) fn search_tag_entries(&self) -> Vec<(&str, usize)> {
        // Gallery indices already reflect the live browse playlist, other filters
        // and all exclusions. Only gallery Any admits additional positive matches.
        let required = self.filter_rule.tags_match_any.then(|| {
            self.filter_rule
                .tags
                .iter()
                .map(|tag| tag.trim().to_lowercase())
                .collect::<Vec<_>>()
        });
        let mut counts = std::collections::BTreeMap::<&str, usize>::new();
        for entry in self
            .filtered_indices
            .iter()
            .filter_map(|&index| self.catalog.get(index))
        {
            if required
                .as_ref()
                .is_some_and(|tags| !tags.iter().all(|tag| entry.tags.contains(tag)))
            {
                continue;
            }
            for tag in &entry.tags {
                *counts.entry(tag.as_str()).or_default() += 1;
            }
        }
        let included = &self.filter_rule.tags;
        let excluded = &self.filter_rule.excluded_tags;
        for tag in included.iter().chain(excluded) {
            counts.entry(tag.as_str()).or_default();
        }
        let mut entries = counts.into_iter().collect::<Vec<_>>();
        let active = |tag: &str| included.contains(tag) || excluded.contains(tag);
        entries.sort_by(|(a, count_a), (b, count_b)| {
            active(b).cmp(&active(a)).then_with(|| {
                if self.search_tags_sort_az {
                    a.cmp(b)
                } else {
                    count_b.cmp(count_a).then_with(|| a.cmp(b))
                }
            })
        });
        entries
    }

    pub(super) fn search_panel_width(&self) -> f32 {
        let scale = view::settings::bridge::effective_ui_scale(self);
        let width = 760.0 * scale;
        let (_, content_width) = crate::donor_search::footprint(&self.state.gallery);
        let desired = content_width.map_or(width, |w| w.clamp(width, 874.0 * scale));
        desired.min((self.viewport[0] - 40.0).max(1.0))
    }

    pub(super) fn search_panel_row_limit(&self) -> usize {
        2
    }

    pub(super) fn tag_panel_bounds(&self, width: f32, height: f32) -> [f32; 4] {
        crate::donor_search::bounds(
            self.viewport,
            &self.state.gallery,
            [width, height.min((self.viewport[1] - 40.0).max(1.0))],
            [0.0, 0.0],
        )
    }

    pub(super) fn restore_retained_start_position(&mut self) {
        if self.gallery_engaged
            || self.detail_open
            || self.screenshot_path.is_some()
            || self.scenario.is_some()
        {
            return;
        }
        let start = (view::settings::bridge::Source { app: self })
            .text("components.wallpaperSelector.startPosition");
        let id = match start.as_str() {
            "applied" => self
                .active_wallpaper_index()
                .and_then(|i| self.wallpapers.get(i))
                .map(|w| w.id.clone()),
            "browsing" => self
                .state
                .frontend_preferences
                .picker
                .browse_position
                .as_ref()
                .map(|p| p.selection.clone()),
            _ => None,
        };
        self.selected = id
            .and_then(|id| {
                self.filtered_indices
                    .iter()
                    .copied()
                    .find(|i| self.wallpapers.get(*i).is_some_and(|w| w.id == id))
            })
            .or_else(|| self.filtered_indices.first().copied());
    }

    pub(super) fn save_retained_browse_position(&mut self) {
        if self.scenario.is_some()
            || self.screenshot_path.is_some()
            || (view::settings::bridge::Source { app: self })
                .text("components.wallpaperSelector.startPosition")
                != "browsing"
        {
            return;
        }
        let Some(selection) = self
            .selected
            .and_then(|i| self.wallpapers.get(i))
            .map(|w| w.id.clone())
        else {
            return;
        };
        self.state.frontend_preferences.picker.browse_position =
            Some(candidate_state::preferences::BrowsePosition {
                anchor: selection.clone(),
                selection,
                layout: gallery_kind(self.gallery_mode).key().into(),
                camera: 0.0,
            });
        self.persist_state();
    }
}

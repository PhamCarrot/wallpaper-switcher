use super::*;

pub(super) fn diagnostic_status(
    save_failure: Option<&str>,
    backup_warning: Option<&str>,
    config_save_failure: Option<&str>,
    load_diagnostic: Option<&str>,
    config_load_diagnostic: Option<&str>,
    catalog_status: Option<String>,
) -> Option<String> {
    let issues = [
        save_failure,
        backup_warning,
        config_save_failure,
        load_diagnostic,
        config_load_diagnostic,
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();
    if issues.is_empty() {
        catalog_status
    } else {
        Some(issues.join(" Persistence: "))
    }
}

pub(super) fn successful_engine_persistence_status(
    message: &str,
    backup_description: &str,
    save_failure: Option<&str>,
    config_save_failure: Option<&str>,
    load_diagnostic: Option<&str>,
    config_load_diagnostic: Option<&str>,
) -> String {
    let saved_message = format!("{message} Persistence: {backup_description}.");
    match diagnostic_status(
        save_failure,
        None,
        config_save_failure,
        load_diagnostic,
        config_load_diagnostic,
        None,
    ) {
        Some(issue) => format!("{saved_message} Persistence: {issue}"),
        None => saved_message,
    }
}

pub(super) fn state_save_failure_message(
    context: &str,
    error: &SaveError,
    path: &std::path::Path,
) -> String {
    format!(
        "{context}: changes remain in memory and were not fully saved: {error}. {}.",
        error.backup_description(&path.with_extension("toml.bak"))
    )
}

fn retain_automation_stop(state: &mut CandidateState) {
    let mut intent = state.automation_intent();
    intent.playlist_playing = false;
    intent.safely_stopped = true;
    state.automation_intent = Some(intent);
}

impl GpuCandidate {
    pub(super) fn local_state_recovery_available(&self) -> bool {
        self.save_failure.is_some()
            || self.state.reconciliation_required.is_some()
            || self.state.write_restriction.is_some()
    }
    /// Explicit, read-only reconciliation through the existing loader. No
    /// proposal is retried and no operational policy is silently installed.
    pub(super) fn reload_local_state(&mut self) -> Result<(), String> {
        use candidate_state::StateLoadSource;
        fn field(
            app: &GpuCandidate,
            path: &str,
            exemplar: Option<&view::settings::composed::draft::Value>,
        ) -> view::settings::composed::draft::Value {
            use view::settings::{
                bridge::Source,
                composed::{
                    draft::Value,
                    schema::{self, ValueKind},
                    SettingsSource,
                },
            };
            let source = Source { app };
            if matches!(exemplar, Some(Value::Number(_)))
                || schema::value_kind(path) == Some(ValueKind::Number)
            {
                Value::Number(source.number(path))
            } else if matches!(exemplar, Some(Value::Boolean(_)))
                || schema::value_kind(path) == Some(ValueKind::Boolean)
            {
                Value::Boolean(source.flag(path))
            } else {
                Value::Text(source.text(path))
            }
        }
        fn saved_root(state: &CandidateState, path: &str) -> Option<serde_json::Value> {
            let owner = candidate_state::pending::root_owner(path)?;
            state
                .pending_frontend
                .settings
                .iter()
                .find(|entry| entry.capability == owner && entry.source_path == path)
                .and_then(|entry| serde_json::from_str(&entry.value_json).ok())
        }
        if self.active_operation.is_some() || self.apply_pending || self.exit_requested {
            return Err("Finish the current operation before reloading saved local settings. Drafts and recovery files were retained.".into());
        }
        let loaded = CandidateState::load_with_diagnostics(&self.state_path);
        if !matches!(
            loaded.source,
            StateLoadSource::Primary | StateLoadSource::BackupRecovery
        ) || loaded.state.write_restriction.is_some()
        {
            return Err(format!(
                "Saved settings were not reloaded: {} Drafts and original files were retained.",
                loaded.diagnostic.unwrap_or_else(|| format!(
                    "No supported saved state was available ({:?}).",
                    loaded.source
                ))
            ));
        }
        // These are the selected local owners. Every other serialized field
        // must still match this session, so recovery cannot overwrite newer
        // library, protection, Engine or automation work on the next save.
        let mut local = self.state.clone();
        local.version = loaded.state.version;
        local.gallery = loaded.state.gallery.clone();
        local.gallery_motion_speed = loaded.state.gallery_motion_speed;
        local.appearance = loaded.state.appearance.clone();
        local.theme_library = loaded.state.theme_library.clone();
        local.applied_theme = loaded.state.applied_theme.clone();
        local.mode_profiles = loaded.state.mode_profiles.clone();
        local.pending_frontend = loaded.state.pending_frontend.clone();
        local.frontend_preferences = loaded.state.frontend_preferences.clone();
        local.card_flip = loaded.state.card_flip.clone();
        local.local_library = loaded.state.local_library.clone();
        // PK-03 explicitly owns these saved controls; unrelated schedules/lists stay protected.
        local.active_playlist_id = loaded.state.active_playlist_id.clone();
        local.automation_intent = loaded.state.automation_intent.clone();
        local.slideshow_enabled = loaded.state.slideshow_enabled;
        local.slideshow_interval_seconds = loaded.state.slideshow_interval_seconds;
        local.retained = loaded.state.retained.clone();
        if toml::to_string(&local)
            .map_err(|_| "Could not compare the current saved-state scope.")?
            != toml::to_string(&loaded.state)
                .map_err(|_| "Could not compare supported saved settings.")?
        {
            return Err("Saved data outside local settings changed. Recovery did not install it or clear the save protection; reconcile that change explicitly. Drafts and original files were retained.".into());
        }
        // Whole-record editors must not later serialize an old retained array
        // over newer independent records. A published version of this draft,
        // or the original committed version, can be reconciled safely.
        self.settings_workbench
            .pending_forms
            .check_reload(&loaded.state)?;
        for root in view::settings::pending_projection::COLLECTION_ROOTS {
            let saved = saved_root(&self.state, root);
            let next = saved_root(&loaded.state, root);
            let draft = match root {
                "postProcessing" => serde_json::to_value(&self.settings_workbench.lists.commands),
                "integrations" => serde_json::to_value(&self.settings_workbench.lists.integrations),
                _ => serde_json::to_value(&self.settings_workbench.lists.semantic_models),
            }
            .map_err(|_| "Could not compare a retained collection draft.")?;
            if next != saved && next.as_ref() != Some(&draft) {
                return Err(format!("Saved {root} has newer records that differ from the retained draft. Files, draft and save protection were retained; reconcile those records explicitly."));
            }
        }
        if loaded.state.frontend_preferences.resolution_presets
            != self.state.frontend_preferences.resolution_presets
            && loaded.state.frontend_preferences.resolution_presets
                != self.settings_workbench.lists.resolutions
        {
            return Err("Saved resolution presets differ from the retained draft. Files, draft and save protection were retained; reconcile those records explicitly.".into());
        }
        let saved_targets = saved_root(&self.state, "theme.targets");
        let next_targets = saved_root(&loaded.state, "theme.targets");
        let draft_targets = self
            .settings_workbench
            .theme_targets_draft
            .as_ref()
            .map(|targets| serde_json::json!(targets));
        if next_targets != saved_targets && next_targets != draft_targets {
            return Err("Saved Theme targets differ from the retained draft. Files, draft and save protection were retained; reconcile those records explicitly.".into());
        }
        let next_locale = localization::Catalog::new(
            loaded
                .state
                .frontend_preferences
                .interface
                .language
                .as_deref()
                .unwrap_or("auto"),
            localization::automatic_tag(),
        )?;
        localization::publish_locale_change(
            self.settings_locale.language.script,
            &next_locale,
            || Ok(()),
        )?;
        // Retain edits, rather than old successful display overrides, before
        // replacing their saved source. No choice buffer is auto-published.
        let values = std::mem::take(&mut self.settings_workbench.draft.values);
        let saved_lists = view::settings::bridge::Session::from_state(&self.state).lists;
        let retained_lists = std::mem::replace(&mut self.settings_workbench.lists, saved_lists);
        let retained_values = values
            .into_iter()
            .filter(|(path, value)| {
                self.settings_workbench.draft.raw_inputs.contains_key(path)
                    || *value != field(self, path, Some(value))
            })
            .collect();
        self.settings_workbench.lists = retained_lists;
        self.settings_workbench.draft.values = retained_values;
        self.stop_gallery_transients("saved-local-settings-reloaded");
        let watch_changed =
            self.state.frontend_preferences.library != loaded.state.frontend_preferences.library;
        self.automation_suspended |= self.state.automation_intent().safely_stopped;
        self.state = loaded.state;
        self.next_slideshow_unix =
            unix_now().saturating_add(self.state.slideshow_interval_seconds.max(10));
        self.refresh_automation_timer();
        self.sync_automation_controls();
        self.settings_workbench
            .pending_forms
            .rebase_after_reload(&self.state);
        self.settings_locale = next_locale;
        self.gallery_mode = gallery_mode(self.state.gallery.mode);
        self.wall_scroll_offset = 0.0;
        self.reset_depth_transients();
        self.invalidate_poster_work();
        self.schedule_posters();
        self.settings_workbench.geometry_preview_baseline = None;
        // Keep useful local buffers and lists. A raw field must be re-entered
        // (or escaped) before navigation can auto-commit it. Choice drafts have
        // no raw-input commit trigger; only a new explicit choice can save one.
        let paths: Vec<_> = self
            .settings_workbench
            .draft
            .raw_inputs
            .keys()
            .cloned()
            .collect();
        let retained_values = std::mem::take(&mut self.settings_workbench.draft.values);
        let saved_lists = view::settings::bridge::Session::from_state(&self.state).lists;
        let retained_lists = std::mem::replace(&mut self.settings_workbench.lists, saved_lists);
        let mut baselines = Vec::new();
        {
            for path in &paths {
                let value = field(self, path, retained_values.get(path));
                baselines.push((path.clone(), value));
            }
        }
        self.settings_workbench.lists = retained_lists;
        self.settings_workbench.draft.values = retained_values;
        for (path, value) in baselines {
            self.settings_workbench.draft.rebase_input(&path, value);
            self.settings_workbench.draft.errors.insert(path,"Draft retained after reload. Re-enter this field, then press Enter to save; Escape restores the saved value.".into());
        }
        view::settings::controller::rotate(self);
        self.settings_workbench.navigation.input_edit = paths.into_iter().min();
        self.load_diagnostic = loaded.diagnostic;
        self.save_failure = None;
        self.backup_warning = None;
        self.project_local_tags();
        self.refresh_filter();
        if watch_changed {
            self.refresh_catalog();
        }
        self.settings_workbench.message = format!("Saved local settings reloaded from {:?}. Recovery files and local edits were retained. No edit was retried and no pending operation was activated.{}",loaded.source,self.load_diagnostic.as_ref().map(|text|format!(" {text}")).unwrap_or_default());
        Ok(())
    }

    pub(super) fn commit_local_state_change(
        &mut self,
        proposed: CandidateState,
    ) -> Result<SaveOutcome, SaveError> {
        self.commit_local_state_change_inner(proposed, false)
    }

    /// Only explicit Start/Resume may publish intent that clears a known safety stop.
    pub(super) fn commit_automation_resume(
        &mut self,
        proposed: CandidateState,
    ) -> Result<SaveOutcome, SaveError> {
        self.commit_local_state_change_inner(proposed, true)
    }

    fn commit_local_state_change_inner(
        &mut self,
        mut proposed: CandidateState,
        resume: bool,
    ) -> Result<SaveOutcome, SaveError> {
        if !resume && (self.automation_suspended || self.state.automation_intent().safely_stopped) {
            retain_automation_stop(&mut proposed);
        }
        let preview = self
            .settings_workbench
            .geometry_preview_baseline
            .as_ref()
            .map(|baseline| {
                proposed.gallery = baseline.clone();
                self.state.gallery.clone()
            });
        match commit_saved_state(&mut self.state, proposed, &self.state_path) {
            Ok(outcome) => {
                if let Some(preview) = preview {
                    self.state.gallery = preview;
                }
                self.record_save_outcome(&outcome);
                Ok(outcome)
            }
            Err(error) => {
                if let Some(baseline) = self.settings_workbench.geometry_preview_baseline.clone() {
                    let previous = std::mem::replace(&mut self.state.gallery, baseline);
                    self.gallery_settings_changed(&previous);
                }
                let recovery = if self.state.reconciliation_required.is_some() {
                    " Disk may have changed; preserve recovery files and deliberately reload before retrying."
                } else {
                    ""
                };
                let message=format!("Could not save local state: the proposal was not installed in the live saved state: {error}.{recovery} {}.", error.backup_description(&self.state_path.with_extension("toml.bak")));
                self.save_failure = Some(message.clone());
                self.status = Some(message);
                Err(error)
            }
        }
    }
    pub(super) fn persistence_status(&self) -> Option<String> {
        diagnostic_status(
            self.save_failure.as_deref(),
            self.backup_warning.as_deref(),
            self.config_save_failure.as_deref(),
            self.load_diagnostic.as_deref(),
            self.config_load_diagnostic.as_deref(),
            None,
        )
    }

    pub(super) fn status_with_persistence_issue(&self, message: &str) -> String {
        match diagnostic_status(
            self.save_failure.as_deref(),
            self.backup_warning.as_deref(),
            self.config_save_failure.as_deref(),
            self.load_diagnostic.as_deref(),
            self.config_load_diagnostic.as_deref(),
            None,
        ) {
            Some(issue) => format!("{message} Persistence: {issue}"),
            None => message.to_string(),
        }
    }

    fn record_save_outcome(&mut self, outcome: &SaveOutcome) {
        self.save_failure = None;
        self.backup_warning = (!outcome.backup.is_available()).then(|| {
            format!(
                "Candidate state saved, but {}",
                outcome.backup_description(&self.state_path.with_extension("toml.bak"))
            )
        });
        if let Some(warning) = &self.backup_warning {
            self.status = Some(match self.status.take() {
                Some(existing) => format!("{existing} Persistence warning: {warning}"),
                None => warning.clone(),
            });
        }
    }

    fn record_save_failure(&mut self, context: &str, error: &SaveError) {
        let message = state_save_failure_message(context, error, &self.state_path);
        self.save_failure = Some(message.clone());
        self.status = Some(message);
    }

    pub(super) fn persist_state(&mut self) {
        let _ = self.persist_state_outcome();
    }

    pub(super) fn persist_state_outcome(&mut self) -> Result<SaveOutcome, SaveError> {
        // Every publisher must carry an unsaved session stop into the next durable record.
        if self.automation_suspended || self.state.automation_intent().safely_stopped {
            retain_automation_stop(&mut self.state);
        }
        let preview = self
            .settings_workbench
            .geometry_preview_baseline
            .as_ref()
            .map(|baseline| std::mem::replace(&mut self.state.gallery, baseline.clone()));
        match persist_candidate_state(&mut self.state, &self.state_path) {
            Ok(outcome) => {
                if let Some(preview) = preview {
                    self.state.gallery = preview;
                }
                self.record_save_outcome(&outcome);
                Ok(outcome)
            }
            Err(error) => {
                if let Some(preview) = preview {
                    self.gallery_settings_changed(&preview);
                }
                self.record_save_failure("Could not save candidate state", &error);
                Err(error)
            }
        }
    }
}

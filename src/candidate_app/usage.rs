//! Confirmed completion and startup policy consumers; the coordinator remains the only writer.
use super::operations::{CapturedWallpaperChange, ChangeOrigin};
use super::*;
use candidate_state::local_library::{ConfirmedAssignment, RotationPreferences};

pub(super) fn media_applicable(item: &WallpaperItem) -> bool {
    let input = if item.source == WallpaperSource::WallpaperEngine {
        item.engine_target()
    } else {
        &item.file_target
    };
    input.is_absolute()
        && input.is_file()
        && match item.source {
            WallpaperSource::LocalImage => library::media_still::supported(&item.file_target),
            WallpaperSource::LocalVideo => item.wallpaper_type.eq_ignore_ascii_case("video"),
            WallpaperSource::WallpaperEngine => matches!(
                item.wallpaper_type.to_ascii_lowercase().as_str(),
                "image" | "video" | "scene" | "web"
            ),
        }
}

impl GpuCandidate {
    pub(super) fn status_for_display(&self) -> Option<String> {
        let mut status = match (self.status.as_deref(), self.startup_status.as_deref()) {
            (Some(status), Some(startup)) if !status.contains(startup) => {
                Some(format!("{status} {startup}"))
            }
            (Some(status), _) => Some(status.to_owned()),
            (None, Some(startup)) => Some(self.status_with_persistence_issue(startup)),
            (None, None) => None,
        };
        for message in [
            (self.catalog_loaded && !self.catalog_fresh)
                .then_some("Cached catalog · stale · refreshing…"),
            self.catalog_snapshot_issue.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            status = Some(match status {
                Some(status) => format!("{status} {message}"),
                None => message.to_string(),
            });
        }
        status
    }

    fn set_startup_status(&mut self, message: String) {
        self.startup_status = Some(message.clone());
        self.status = Some(self.status_with_persistence_issue(&message));
    }

    /// The context is consumed once per actual submission lifetime, never persisted as an ID log.
    pub(super) fn record_confirmed_completion(
        &mut self,
        result: &coordinator::ApplyResult,
        now: u64,
    ) {
        let Some(captured) = self.wallpaper_change.take() else {
            return;
        };
        let changes = captured.confirmed(result);
        if captured.origin == ChangeOrigin::Startup {
            self.set_startup_status(format!(
                "Startup reapply last request: {} of {} captured targets confirmed. {}",
                changes.len(),
                captured.target_ids.len(),
                result.error_message().unwrap_or_default()
            ));
        }
        if changes.is_empty() {
            return;
        }
        let history = &mut self.state.local_library.usage;
        let count = history.items.entry(captured.item.id.clone()).or_default();
        count.confirmed_count = count.confirmed_count.saturating_add(1);
        count.last_confirmed_unix = count.last_confirmed_unix.max(now);
        if captured.origin == ChangeOrigin::Random {
            history.last_successful_random_id = Some(captured.item.id);
        }
        for mut change in changes {
            // Unknown fields belong to the target record and survive a new confirmed assignment.
            if let Some(old_key) = history
                .targets
                .keys()
                .find(|id| id.eq_ignore_ascii_case(&change.target_id))
                .cloned()
            {
                if let Some(old) = history.targets.remove(&old_key) {
                    change.assignment.retained = old.retained;
                }
            }
            apply_trace::event_for_lazy(
                change.operation_id.get(),
                "confirmed-wallpaper-change",
                || {
                    format!(
                        "item_id={:?} target_id={:?} origin={:?} still_available={}",
                        change.item_id,
                        change.target_id,
                        change.origin,
                        change.still.is_some()
                    )
                },
            );
            history.targets.insert(change.target_id, change.assignment);
        }
        // The existing publisher retains in-memory changes, partial publication and refusal facts.
        // Saving history never retries the already completed provider request.
        if let Err(error) = self.persist_state_outcome() {
            let message = format!("Wallpaper provider change confirmed; local usage history is not fully durable: {error}. {}",
                self.save_failure.as_deref().unwrap_or("Saved files retained."));
            self.save_failure = Some(message);
        }
        self.refresh_filter();
    }

    /// Legacy choices are offered for deliberate new save only; runtime never reads desired_policy.
    pub(super) fn rotation_edit_preferences(&self) -> RotationPreferences {
        self.state
            .local_library
            .rotation
            .clone()
            .unwrap_or_else(|| {
                let desired = &self.state.frontend_preferences.desired_policy;
                RotationPreferences {
                    images: desired.random_include_static.unwrap_or(true),
                    videos: desired.random_include_video.unwrap_or(true),
                    we: desired.random_include_we.unwrap_or(true),
                    favourites_only: desired.random_include_favourites.unwrap_or(false),
                    retained: Default::default(),
                }
            })
    }

    pub(super) fn automation_policy_status(&self) -> String {
        let rotation = if self.state.local_library.rotation.is_some() {
            "Rotation choices: active saved policy."
        } else {
            "Rotation choices: not activated. Runtime defaults are Images/Videos/WE on, Favourites only off. Displayed legacy choices remain pending until Save."
        };
        format!("{}; playlist: {}. {rotation} Startup reapply: {}. Legacy restore/notification wishes remain inactive. Saved enabled timers resume separately in normal mode. Preview saves local policy and blocks every provider dispatch.",
            self.random_rotation_state().label(),
            if self.automation_suspended || self.state.automation_intent().safely_stopped { "safely stopped; explicit Resume random or Start playlist required" } else if self.playlist_playing() { "playing; first/next change follows the full interval" } else { "stopped" },
            if self.state.local_library.startup.reapply_enabled { "opted in" } else { "off" })
    }

    pub(super) fn item_allowed_for_targets(
        &self,
        item: &WallpaperItem,
        targets: &[String],
    ) -> bool {
        media_applicable(item)
            && !targets.is_empty()
            && targets.iter().all(|target| {
                self.monitor_ids
                    .iter()
                    .position(|id| id.eq_ignore_ascii_case(target))
                    .is_some_and(|index| {
                        wallpaper_allowed_on_monitor(
                            &self.config,
                            &format!("Monitor{index}"),
                            false,
                            &item.id,
                        )
                    })
                    && !self
                        .state
                        .locked_monitors
                        .iter()
                        .any(|id| id.eq_ignore_ascii_case(target))
            })
    }

    pub(super) fn prepare_startup_reapply(&mut self) {
        if !self.catalog_fresh {
            return;
        }
        if !self.startup_checked
            && self.state.local_library.startup.reapply_enabled
            && self.launch_policy.allows_automatic_provider()
        {
            self.refresh_monitors(false);
        }
        self.prepare_startup_reapply_with(|owner, item, target, transition| {
            owner.submit(item, target, transition)
        });
    }

    pub(super) fn prepare_startup_reapply_with(
        &mut self,
        submit: impl FnOnce(
            &WallpaperCoordinator,
            WallpaperItem,
            ApplyTarget,
            transition::TransitionConfig,
        ) -> Result<coordinator::ApplySubmission, String>,
    ) {
        if !self.catalog_fresh || self.startup_checked {
            return;
        }
        self.startup_checked = true;
        if !self.state.local_library.startup.reapply_enabled {
            return;
        }
        if !self.launch_policy.allows_automatic_provider() {
            self.set_startup_status(
                "Startup reapply requires normal mode; automatic provider execution is blocked in --preview."
                    .into(),
            );
            return;
        }
        if !self.apply_enabled
            || self.apply_pending
            || self.provider_state_unknown
            || !self.provider_recoveries.is_empty()
            || self.state.write_restriction.is_some()
            || self.state.reconciliation_required.is_some()
            || !self.state.automation_allowed(unix_now())
            || self.exit_requested
        {
            self.set_startup_status("Startup reapply skipped: execution, ownership, saved-state recovery or manual holdoff is restricted.".into());
            return;
        }
        let bindings = &self.state.local_library.usage.targets;
        if bindings.is_empty() {
            self.set_startup_status(
                "Startup reapply skipped: no confirmed local target assignment.".into(),
            );
            return;
        }
        for (target, assignment) in bindings {
            let Some(item) = self.wallpapers.iter().find(|item| assignment.matches(item)) else {
                self.set_startup_status(format!("Startup reapply skipped: confirmed input for {target} is missing or mismatched."));
                return;
            };
            if !self.item_allowed_for_targets(item, std::slice::from_ref(target)) {
                self.set_startup_status(format!("Startup reapply skipped: target/input {target} is unavailable, hidden, excluded or locked."));
                return;
            }
        }
        self.startup_queue = bindings
            .iter()
            .map(|(target, assignment)| (target.clone(), assignment.clone()))
            .collect();
        self.advance_startup_reapply_with(submit);
    }

    pub(super) fn advance_startup_reapply(&mut self) {
        if !self.catalog_fresh {
            return;
        }
        if !self.startup_queue.is_empty() && self.launch_policy.allows_automatic_provider() {
            self.refresh_monitors(false);
        }
        self.advance_startup_reapply_with(|owner, item, target, transition| {
            owner.submit(item, target, transition)
        });
    }

    pub(super) fn advance_startup_reapply_with(
        &mut self,
        submit: impl FnOnce(
            &WallpaperCoordinator,
            WallpaperItem,
            ApplyTarget,
            transition::TransitionConfig,
        ) -> Result<coordinator::ApplySubmission, String>,
    ) {
        if !self.catalog_fresh || self.startup_queue.is_empty() {
            return;
        }
        if !self.launch_policy.allows_automatic_provider()
            || !self.apply_enabled
            || self.apply_pending
            || self.provider_state_unknown
            || !self.provider_recoveries.is_empty()
            || self.exit_requested
            || self.state.write_restriction.is_some()
            || self.state.reconciliation_required.is_some()
            || !self.state.automation_allowed(unix_now())
        {
            self.startup_queue.clear();
            self.set_startup_status(
                "Startup reapply stopped: execution/ownership/recovery is restricted.".into(),
            );
            return;
        }
        let (target, assignment): (String, ConfirmedAssignment) =
            self.startup_queue.pop_front().unwrap();
        let Some(item) = self
            .wallpapers
            .iter()
            .find(|item| assignment.matches(item))
            .cloned()
            .filter(|item| self.item_allowed_for_targets(item, std::slice::from_ref(&target)))
        else {
            self.startup_queue.clear();
            self.set_startup_status(
                "Startup reapply stopped: captured target/input is no longer eligible.".into(),
            );
            return;
        };
        match submit(
            &self.coordinator,
            item.clone(),
            ApplyTarget::DeviceId(target),
            self.config.transition.clone(),
        ) {
            Ok(submission) => {
                self.wallpaper_change = CapturedWallpaperChange::new(
                    item,
                    &submission,
                    self.catalog_generation.load(Ordering::Acquire),
                    ChangeOrigin::Startup,
                );
                self.active_operation = Some(submission.operation_id);
                self.apply_pending = true;
                self.hide_after_apply = false;
                self.set_startup_status("Reapplying confirmed startup assignment…".into());
            }
            Err(error) => {
                self.startup_queue.clear();
                self.set_startup_status(format!("Startup reapply refused: {error}"));
            }
        }
    }
}

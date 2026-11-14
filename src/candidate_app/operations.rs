use super::lifecycle::PanelState;
use super::*;
use crate::{apply_trace, coordinator, engine_properties, native_wallpaper, transition};
use view::settings::composed::SettingsSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ChangeOrigin {
    Manual,
    Playlist,
    Random,
    Startup,
}

#[derive(Debug, Clone)]
pub(super) struct CapturedWallpaperChange {
    pub operation_id: coordinator::OperationId,
    pub item: WallpaperItem,
    pub target_ids: Vec<String>,
    pub still: Option<library::media_still::MediaStill>,
    pub origin: ChangeOrigin,
    pub automation_generation: Option<u64>,
}

#[derive(Debug, Clone)]
pub(super) struct ConfirmedWallpaperChange {
    pub operation_id: coordinator::OperationId,
    pub item_id: String,
    pub target_id: String,
    pub assignment: candidate_state::local_library::ConfirmedAssignment,
    pub still: Option<library::media_still::MediaStill>,
    pub origin: ChangeOrigin,
}

impl CapturedWallpaperChange {
    pub fn new(
        item: WallpaperItem,
        submission: &coordinator::ApplySubmission,
        revision: u64,
        origin: ChangeOrigin,
    ) -> Option<Self> {
        if submission.wallpaper_id != item.id || submission.target_ids.is_empty() {
            return None;
        }
        Some(Self {
            operation_id: submission.operation_id,
            still: library::media_still::MediaStill::select(&item, revision),
            item,
            target_ids: submission.target_ids.clone(),
            origin,
            automation_generation: None,
        })
    }
    pub fn accepts(&self, result: &coordinator::ApplyResult) -> bool {
        self.operation_id == result.operation_id
            && self.item.id == result.wallpaper_id
            && (result.outcomes.is_empty()
                || result.outcomes.iter().any(|o| {
                    self.target_ids
                        .iter()
                        .any(|id| id.eq_ignore_ascii_case(&o.target_id))
                }))
    }
    pub fn confirmed(&self, result: &coordinator::ApplyResult) -> Vec<ConfirmedWallpaperChange> {
        if !self.accepts(result) {
            return Vec::new();
        }
        let mut seen = std::collections::BTreeSet::new();
        result
            .outcomes
            .iter()
            .filter_map(|outcome| {
                let target = self
                    .target_ids
                    .iter()
                    .find(|id| id.eq_ignore_ascii_case(&outcome.target_id))?;
                // ProviderConfirmed is minted by the coordinator only after its current S1 readback.
                if outcome.state != coordinator::ApplyOperationState::ProviderConfirmed
                    || outcome.error.is_some()
                    || !seen.insert(target.to_ascii_lowercase())
                {
                    return None;
                }
                Some(ConfirmedWallpaperChange {
                    operation_id: self.operation_id,
                    item_id: self.item.id.clone(),
                    target_id: target.clone(),
                    assignment: candidate_state::local_library::ConfirmedAssignment::capture(
                        &self.item,
                    ),
                    still: self.still.clone(),
                    origin: self.origin,
                })
            })
            .collect()
    }
}

pub(super) fn provider_mutation_allowed(
    provider_state_unknown: bool,
    command: Option<&EngineCommand>,
) -> bool {
    !provider_state_unknown
        || matches!(
            command,
            Some(EngineCommand::ReconcileProvider { .. } | EngineCommand::CancelPreflight { .. })
        )
}

pub(super) fn update_provider_recovery_state(
    pending: &mut Vec<ProviderRecovery>,
    command: Option<&EngineCommand>,
    operation_succeeded: bool,
    has_unknown: bool,
    returned_recoveries: &[ProviderRecovery],
) -> bool {
    if has_unknown {
        for recovery in returned_recoveries {
            if let Some(existing) = pending
                .iter_mut()
                .find(|existing| coordinator::same_recovery(existing, recovery))
            {
                existing.reason = recovery.reason.clone();
                existing.observation = recovery.observation.clone();
            } else {
                pending.push(recovery.clone());
            }
        }
    }
    if operation_succeeded {
        if let Some(
            EngineCommand::ReconcileProvider { recovery }
            | EngineCommand::CancelPreflight { recovery },
        ) = command
        {
            coordinator::settle_matching_recovery(pending, recovery);
        }
    }
    !pending.is_empty()
}

pub(super) fn provider_recovery_action_available(
    pending: &[ProviderRecovery],
    _selected: Option<usize>,
) -> bool {
    provider_recovery_action_available_with(pending, coordinator::recovery_capabilities)
}

pub(super) fn provider_recovery_action_available_with(
    pending: &[ProviderRecovery],
    capability: impl Fn(&ProviderRecovery) -> coordinator::RecoveryCapabilities,
) -> bool {
    pending
        .iter()
        .any(|recovery| capability(recovery).retry_observation)
}

pub(super) fn engine_property_schema_status_message(
    schema: &engine_properties::PropertySchema,
) -> Option<String> {
    fn skipped_summary(skipped: &engine_properties::SkippedPropertyDefinitions) -> Option<String> {
        if skipped.is_empty() {
            return None;
        }
        let examples = skipped.examples.iter().take(3).cloned().collect::<Vec<_>>();
        let sample = (!examples.is_empty()).then(|| format!(" ({})", examples.join("; ")));
        Some(format!(
            "skipped {} invalid or unsupported definition{}{}",
            skipped.count,
            if skipped.count == 1 { "" } else { "s" },
            sample.unwrap_or_default()
        ))
    }

    match &schema.state {
        engine_properties::PropertySchemaState::ReadFailure(detail) => Some(format!(
            "Engine property schema could not be read: {detail}"
        )),
        engine_properties::PropertySchemaState::InvalidJson(detail) => Some(format!(
            "Engine property schema contains invalid JSON: {detail}"
        )),
        engine_properties::PropertySchemaState::MalformedSchema(detail) => Some(format!(
            "Engine property schema has malformed structure: {detail}"
        )),
        engine_properties::PropertySchemaState::ValidNoPropertySection => {
            Some("This project has no Engine property section.".into())
        }
        engine_properties::PropertySchemaState::ValidNoSupportedProperties { skipped } => {
            Some(match skipped_summary(skipped) {
                Some(summary) => {
                    format!("This project has no supported Engine properties; {summary}.")
                }
                None => "This project has no supported Engine properties.".into(),
            })
        }
        engine_properties::PropertySchemaState::Supported { skipped } => skipped_summary(skipped)
            .map(|summary| {
                format!(
                    "Loaded {} supported Engine properties; {summary}.",
                    schema.properties.len()
                )
            }),
    }
}

pub(super) fn engine_property_defaults(
    schema: &engine_properties::PropertySchema,
) -> (
    Vec<engine_properties::EngineProperty>,
    serde_json::Map<String, serde_json::Value>,
) {
    let properties = schema.properties.clone();
    let values = properties
        .iter()
        .map(|property| (property.key.clone(), property.default_value.clone()))
        .collect();
    (properties, values)
}

pub(super) fn replace_engine_property_controls(
    properties: &mut Vec<engine_properties::EngineProperty>,
    values: &mut serde_json::Map<String, serde_json::Value>,
    diagnostic: &mut Option<String>,
    schema: &engine_properties::PropertySchema,
) {
    properties.clear();
    values.clear();
    *diagnostic = engine_property_schema_status_message(schema);
    let (new_properties, new_values) = engine_property_defaults(schema);
    *properties = new_properties;
    *values = new_values;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ApplyPlanError {
    NoSelection,
    SelectionNotVisible,
    NoMonitor,
}

#[derive(Debug, Clone)]
pub(super) struct ApplyRequestPlan {
    pub(super) wallpaper: WallpaperItem,
    pub(super) target: ApplyTarget,
}

/// Resolve the explicit Apply request at the frontend/backend boundary.
///
/// Browse and detail paths do not call this function. Keeping the filtered
/// position resolution here prevents a reordered view from submitting a
/// different stable item while leaving the coordinator as the only mutation
/// owner.
pub(super) fn plan_apply_request(
    wallpapers: &[WallpaperItem],
    filtered_indices: &[usize],
    selected_index: Option<usize>,
    active_monitor_id: Option<&str>,
    monitor_ids: &[String],
    all_monitors: bool,
) -> Result<ApplyRequestPlan, ApplyPlanError> {
    let selected_index = selected_index.ok_or(ApplyPlanError::NoSelection)?;
    let filtered_position = filtered_indices
        .iter()
        .position(|index| *index == selected_index)
        .ok_or(ApplyPlanError::SelectionNotVisible)?;
    let wallpaper = resolve_item(wallpapers, filtered_indices, filtered_position)
        .cloned()
        .ok_or(ApplyPlanError::SelectionNotVisible)?;
    let target = if all_monitors && !monitor_ids.is_empty() {
        ApplyTarget::All
    } else {
        active_monitor_id
            .map(str::to_owned)
            .map(ApplyTarget::DeviceId)
            .ok_or(ApplyPlanError::NoMonitor)?
    };
    Ok(ApplyRequestPlan { wallpaper, target })
}

pub(super) fn monitor_id_at_point(
    monitors: &[native_wallpaper::NativeMonitor],
    x: i32,
    y: i32,
) -> Option<String> {
    monitors
        .iter()
        .find(|monitor| {
            x >= monitor.rect.left
                && x < monitor.rect.right
                && y >= monitor.rect.top
                && y < monitor.rect.bottom
        })
        .map(|monitor| monitor.device_id.clone())
}

pub(super) fn cursor_monitor_id(monitors: &[native_wallpaper::NativeMonitor]) -> Option<String> {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;

    let mut point = POINT { x: 0, y: 0 };
    (unsafe { GetCursorPos(&mut point) } != 0)
        .then(|| monitor_id_at_point(monitors, point.x, point.y))
        .flatten()
}

#[cfg(test)]
thread_local! {
    // One target cycle consumes the controlled observation on this test thread.
    pub(super) static TARGET_MONITORS_FOR_TEST: std::cell::RefCell<Option<Vec<native_wallpaper::NativeMonitor>>> =
        const { std::cell::RefCell::new(None) };
}

impl GpuCandidate {
    pub(super) fn toggle_engine_properties(&mut self) {
        if self.engine_properties_open {
            self.clear_engine_properties();
            return;
        }
        self.clear_engine_properties();
        let Some(wallpaper) = self.selected.and_then(|index| self.wallpapers.get(index)) else {
            return;
        };
        if wallpaper.source != WallpaperSource::WallpaperEngine {
            self.status =
                Some("Engine properties are available for installed Engine projects.".into());
            return;
        }
        let schema = engine_properties::read_supported(&wallpaper.project_json_path);
        replace_engine_property_controls(
            &mut self.engine_properties,
            &mut self.engine_property_values,
            &mut self.engine_properties_diagnostic,
            &schema,
        );
        if self.engine_properties.is_empty() {
            if let Some(message) = self.engine_properties_diagnostic.clone() {
                self.status = Some(self.status_with_persistence_issue(&message));
            }
            return;
        }
        if super::lifecycle::theme_owner_active(self) && self.theme_session.designer_dirty() {
            let _ = self.request_theme_departure(super::theme_workflow::Departure::Message(
                Box::new(Message::ToggleEngineProperties),
            ));
            return;
        }
        self.engine_properties_project =
            Some((wallpaper.id.clone(), wallpaper.project_json_path.clone()));
        self.engine_properties_open = true;
        self.panel_state = PanelState::default();
        self.playlist_panel_open = false;
        self.folder_menu_open = false;
        self.controls_panel_open = false;
        if let Some(message) = self.engine_properties_diagnostic.clone() {
            self.status = Some(self.status_with_persistence_issue(&message));
        }
    }

    pub(super) fn clear_engine_properties(&mut self) {
        self.engine_properties_open = false;
        self.engine_properties.clear();
        self.engine_property_values.clear();
        self.engine_properties_project = None;
        self.engine_properties_diagnostic = None;
    }

    pub(super) fn apply_engine_properties(&mut self) {
        self.apply_engine_properties_with(native_wallpaper::monitors, |owner, command| {
            owner.submit_engine_command(command)
        });
    }

    pub(super) fn apply_engine_properties_with(
        &mut self,
        enumerate: impl FnOnce() -> Result<Vec<native_wallpaper::NativeMonitor>, String>,
        submit: impl FnOnce(
            &WallpaperCoordinator,
            EngineCommand,
        ) -> Result<coordinator::OperationId, String>,
    ) {
        if !self.launch_policy.allows_provider() {
            self.status = Some("Provider execution is unavailable in --preview.".into());
            return;
        }
        if self.apply_pending || self.engine_property_values.is_empty() {
            return;
        }
        // Only the captured supported definitions may contribute outgoing values.
        let properties = self
            .engine_properties
            .iter()
            .filter_map(|property| {
                self.engine_property_values
                    .get(&property.key)
                    .filter(|value| engine_properties::accepts_value(property, value))
                    .map(|value| (property.key.clone(), value.clone()))
            })
            .collect::<serde_json::Map<_, _>>();
        if properties.is_empty() {
            self.status = Some("No supported Engine property values are available.".into());
            return;
        }
        let Some((wallpaper_id, project_json_path)) = self.engine_properties_project.clone() else {
            self.status = Some("Reopen Engine properties for the selected project.".into());
            return;
        };
        let Some(wallpaper) = self.selected.and_then(|index| self.wallpapers.get(index)) else {
            return;
        };
        if wallpaper.source != WallpaperSource::WallpaperEngine
            || wallpaper.id != wallpaper_id
            || wallpaper.project_json_path != project_json_path
        {
            self.status = Some(
                "The selected Engine project changed. Reopen its properties before applying."
                    .into(),
            );
            return;
        }
        if let Err(error) = self.refresh_monitors_for_apply(false, enumerate) {
            self.status = Some(error);
            return;
        }
        let Some(target) = self.active_monitor_id.clone().map(ApplyTarget::DeviceId) else {
            self.status = Some("No Windows wallpaper monitor is available.".into());
            return;
        };
        self.submit_engine_command_with(
            EngineCommand::ApplyProperties {
                wallpaper_id,
                project_json_path,
                properties,
                target,
            },
            submit,
        );
    }

    pub(super) fn submit_engine_command(&mut self, command: EngineCommand) {
        self.submit_engine_command_with(command, |owner, command| {
            owner.submit_engine_command(command)
        });
    }

    pub(super) fn submit_engine_command_with(
        &mut self,
        command: EngineCommand,
        submit: impl FnOnce(
            &WallpaperCoordinator,
            EngineCommand,
        ) -> Result<coordinator::OperationId, String>,
    ) {
        if !self.launch_policy.allows_provider() {
            self.status = Some("Provider execution is unavailable in --preview.".into());
            return;
        }
        if self.apply_pending {
            return;
        }
        if !provider_mutation_allowed(self.provider_state_unknown, Some(&command)) {
            self.status = Some(
                "Wallpaper Engine provider state is unresolved; further provider changes remain blocked."
                    .into(),
            );
            return;
        }
        if !self.apply_enabled
            && !matches!(
                command,
                EngineCommand::ReconcileProvider { .. } | EngineCommand::CancelPreflight { .. }
            )
        {
            self.status = Some(
                "Candidate provider controls are disabled. Launch with --allow-apply only for an approved runtime session."
                    .into(),
            );
            return;
        }
        match submit(&self.coordinator, command) {
            Ok(operation_id) => {
                self.apply_pending = true;
                self.active_operation = Some(operation_id);
                self.hide_after_apply = false;
                self.status = Some("Processing provider request…".into());
            }
            Err(error) => self.status = Some(error),
        }
    }

    pub(super) fn active_monitor_key(&self) -> Option<String> {
        let active = self.active_monitor_id.as_ref()?;
        self.monitor_ids
            .iter()
            .position(|device_id| device_id.eq_ignore_ascii_case(active))
            .map(|index| format!("Monitor{index}"))
    }

    pub(super) fn selected_item_trace_fields(&self) -> String {
        let Some(index) = self.selected else {
            return format!(
                "selected_index=none visible={} window_id={:?}",
                self.visible, self.window
            );
        };
        let Some(wallpaper) = self.wallpapers.get(index) else {
            return format!(
                "selected_index={index} selected_item=unavailable visible={} window_id={:?}",
                self.visible, self.window
            );
        };
        format!(
            "selected_index={index} selected_id={:?} selected_title={:?} selected_source={:?} selected_type={:?} project_json_path={:?} file_target={:?} visible={} window_id={:?}",
            wallpaper.id,
            wallpaper.title,
            wallpaper.source,
            wallpaper.wallpaper_type,
            wallpaper.project_json_path,
            wallpaper.file_target,
            self.visible,
            self.window
        )
    }

    pub(super) fn refresh_monitors(&mut self, follow_cursor: bool) {
        self.refresh_monitors_with(follow_cursor, native_wallpaper::monitors, cursor_monitor_id);
    }

    /// Mutation-time discovery must not repair the user's selected identity.
    /// Browsing/reopen/cycling keep their separate fallback in refresh_monitors.
    fn refresh_monitors_for_apply(
        &mut self,
        all_monitors: bool,
        enumerate: impl FnOnce() -> Result<Vec<native_wallpaper::NativeMonitor>, String>,
    ) -> Result<(), String> {
        let intended = self.active_monitor_id.clone();
        let monitors = enumerate().map_err(|error| {
            format!(
                "Could not enumerate Windows wallpaper monitors: {error}. Retry display discovery before applying; no request was submitted."
            )
        })?;
        view::settings::bridge::observe_monitors(&mut self.settings_workbench, &monitors);
        self.monitor_ids = monitors
            .into_iter()
            .map(|monitor| monitor.device_id)
            .collect();
        if all_monitors {
            return if self.monitor_ids.is_empty() {
                Err("No Windows wallpaper monitor is available. Connect a display and retry explicit All.".into())
            } else {
                Ok(())
            };
        }
        let Some(intended) = intended else {
            return Err(
                "No target display is selected. Explicitly select a display before applying."
                    .into(),
            );
        };
        if self
            .monitor_ids
            .iter()
            .any(|id| id.eq_ignore_ascii_case(&intended))
        {
            Ok(())
        } else {
            Err(format!(
                "Selected display {intended:?} is unavailable. Reconnect it or explicitly select another display before applying."
            ))
        }
    }

    pub(super) fn refresh_monitors_with(
        &mut self,
        follow_cursor: bool,
        enumerate: impl FnOnce() -> Result<Vec<native_wallpaper::NativeMonitor>, String>,
        cursor_target: impl FnOnce(&[native_wallpaper::NativeMonitor]) -> Option<String>,
    ) {
        let monitors = enumerate().unwrap_or_default();
        view::settings::bridge::observe_monitors(&mut self.settings_workbench, &monitors);
        let monitor_ids: Vec<_> = monitors
            .iter()
            .map(|monitor| monitor.device_id.clone())
            .collect();
        let cursor_target = follow_cursor.then(|| cursor_target(&monitors)).flatten();
        let existing_is_available = self.active_monitor_id.as_ref().is_some_and(|active| {
            monitor_ids
                .iter()
                .any(|device_id| device_id.eq_ignore_ascii_case(active))
        });
        self.active_monitor_id = cursor_target
            .or_else(|| {
                existing_is_available
                    .then(|| self.active_monitor_id.clone())
                    .flatten()
            })
            .or_else(|| {
                self.state
                    .frontend_preferences
                    .picker
                    .desired_monitor_id
                    .as_ref()
                    .and_then(|desired| {
                        monitor_ids
                            .iter()
                            .find(|id| id.eq_ignore_ascii_case(desired))
                            .cloned()
                    })
            })
            .or_else(|| monitor_ids.first().cloned());
        self.monitor_ids = monitor_ids;
    }

    pub(super) fn cycle_target_monitor(&mut self) {
        #[cfg(test)]
        let fixture = TARGET_MONITORS_FOR_TEST.with(|slot| slot.borrow_mut().take());
        #[cfg(not(test))]
        let fixture: Option<Vec<native_wallpaper::NativeMonitor>> = None;
        if let Some(monitors) = fixture {
            self.refresh_monitors_with(false, || Ok(monitors), |_| None);
        } else {
            self.refresh_monitors(false);
        }
        if self.monitor_ids.is_empty() {
            self.status = Some("No Windows wallpaper monitor is available.".into());
            return;
        }
        let current = self.active_monitor_id.as_ref().and_then(|active| {
            self.monitor_ids
                .iter()
                .position(|device_id| device_id.eq_ignore_ascii_case(active))
        });
        let next = current.map_or(0, |index| (index + 1) % self.monitor_ids.len());
        self.active_monitor_id = self.monitor_ids.get(next).cloned();
        self.selected = self.active_wallpaper_index();
        self.refresh_filter();
        self.refresh_backdrop();
        self.schedule_posters();
        self.status = Some(format!(
            "Targeting display {} of {}.",
            next + 1,
            self.monitor_ids.len()
        ));
    }

    pub(super) fn apply_selected(&mut self, all_monitors: bool) {
        self.apply_selected_with(
            all_monitors,
            native_wallpaper::monitors,
            |owner, item, target, transition| owner.submit(item, target, transition),
        );
    }

    pub(super) fn apply_selected_with(
        &mut self,
        all_monitors: bool,
        enumerate: impl FnOnce() -> Result<Vec<native_wallpaper::NativeMonitor>, String>,
        submit: impl FnOnce(
            &WallpaperCoordinator,
            WallpaperItem,
            ApplyTarget,
            transition::TransitionConfig,
        ) -> Result<coordinator::ApplySubmission, String>,
    ) {
        if !self.launch_policy.allows_provider() {
            self.status = Some("Provider execution is unavailable in --preview.".into());
            return;
        }
        apply_trace::event_lazy("ui-apply-intent", || {
            format!(
                "source=gallery-control outcome=requested all_monitors={all_monitors} {}",
                self.selected_item_trace_fields()
            )
        });
        if self.apply_pending {
            apply_trace::event_lazy("ui-apply-decision", || {
                format!(
                    "outcome=rejected reason=apply_pending {}",
                    self.selected_item_trace_fields()
                )
            });
            return;
        }
        if !provider_mutation_allowed(self.provider_state_unknown, None) {
            apply_trace::event_lazy("ui-apply-decision", || {
                format!(
                    "outcome=rejected reason=provider_state_unknown {}",
                    self.selected_item_trace_fields()
                )
            });
            self.status = Some(
                "Wallpaper provider state is unresolved; further provider changes remain blocked."
                    .into(),
            );
            return;
        }
        if !self.apply_enabled {
            apply_trace::event_lazy("ui-apply-decision", || {
                format!(
                    "outcome=rejected reason=apply_disabled {}",
                    self.selected_item_trace_fields()
                )
            });
            self.status = Some(
                "Candidate Apply is disabled. Launch with --allow-apply only for an approved runtime session."
                    .into(),
            );
            return;
        }
        if let Err(error) = self.refresh_monitors_for_apply(all_monitors, enumerate) {
            apply_trace::event_lazy("ui-apply-decision", || {
                format!(
                    "outcome=rejected reason=target_observation_failed error={error:?} {}",
                    self.selected_item_trace_fields()
                )
            });
            self.status = Some(error);
            return;
        }
        let plan = match plan_apply_request(
            &self.wallpapers,
            &self.filtered_indices,
            self.selected,
            self.active_monitor_id.as_deref(),
            &self.monitor_ids,
            all_monitors,
        ) {
            Ok(plan) => plan,
            Err(ApplyPlanError::NoMonitor) => {
                apply_trace::event_lazy("ui-apply-decision", || {
                    format!(
                        "outcome=rejected reason=target_unavailable {}",
                        self.selected_item_trace_fields()
                    )
                });
                self.status = Some("No Windows wallpaper monitor is available.".into());
                return;
            }
            Err(ApplyPlanError::NoSelection) => {
                apply_trace::event_lazy("ui-apply-decision", || {
                    format!(
                        "outcome=rejected reason=no_selection {}",
                        self.selected_item_trace_fields()
                    )
                });
                return;
            }
            Err(ApplyPlanError::SelectionNotVisible) => {
                apply_trace::event_lazy("ui-apply-decision", || {
                    format!(
                        "outcome=rejected reason=selection_not_visible {}",
                        self.selected_item_trace_fields()
                    )
                });
                return;
            }
        };
        let trace_submission = apply_trace::enabled().then(|| {
            format!(
                "source=gallery-control all_monitors={all_monitors} item_id={:?} item_title={:?} item_source={:?} item_type={:?} project_json_path={:?} file_target={:?} target={:?}",
                plan.wallpaper.id,
                plan.wallpaper.title,
                plan.wallpaper.source,
                plan.wallpaper.wallpaper_type,
                plan.wallpaper.project_json_path,
                plan.wallpaper.file_target,
                plan.target
            )
        });
        let captured_item = plan.wallpaper.clone();
        match submit(
            &self.coordinator,
            plan.wallpaper,
            if matches!(plan.target, ApplyTarget::All) {
                ApplyTarget::DeviceIds(self.monitor_ids.clone())
            } else {
                plan.target
            },
            self.config.transition.clone(),
        ) {
            Ok(submission) => {
                apply_trace::event_for_lazy(
                    submission.operation_id.get(),
                    "ui-apply-submission",
                    || trace_submission.unwrap_or_else(|| "item_identity=not_captured".into()),
                );
                self.apply_pending = true;
                self.active_operation = Some(submission.operation_id);
                self.wallpaper_change = CapturedWallpaperChange::new(
                    captured_item,
                    &submission,
                    self.catalog_generation.load(Ordering::Acquire),
                    ChangeOrigin::Manual,
                );
                self.hide_after_apply =
                    (view::settings::bridge::Source { app: self }).flag("general.closeOnSelection");
                self.status = Some("Applying wallpaper…".into());
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |duration| duration.as_secs());
                self.state.begin_manual_holdoff(now);
                self.persist_state();
            }
            Err(error) => {
                apply_trace::event_lazy("ui-apply-decision", || {
                    format!(
                        "outcome=rejected reason=coordinator_submit_failed error={error:?} {}",
                        self.selected_item_trace_fields()
                    )
                });
                self.status = Some(error);
            }
        }
    }
}

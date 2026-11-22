use super::*;
use crate::{candidate_scenario, library};

/// One explicitly armed preview observation, never a resident tray or provider.
#[derive(Debug)]
pub(super) struct ThemeExitProbe {
    window: window::Id,
    generation: u64,
    recovering: bool,
}

/// Never sleep while polling an Iced future: window queries must be able to
/// complete before their watchdog. The thread only signals this owned probe.
pub(super) fn theme_probe_delay(
    duration: std::time::Duration,
) -> std::io::Result<futures_channel::oneshot::Receiver<()>> {
    let (sender, receiver) = futures_channel::oneshot::channel();
    std::thread::Builder::new()
        .name("gpu-theme-probe-delay".into())
        .spawn(move || {
            std::thread::sleep(duration);
            let _ = sender.send(());
        })?;
    Ok(receiver)
}

impl GpuCandidate {
    pub(super) fn begin_theme_exit_probe(&mut self) -> Task<Message> {
        if !self.launch_policy.is_preview()
            || !self.theme_workflow.probe_armed
            || !self.visible
            || !lifecycle::theme_owner_active(self)
            || !self.theme_session.designer_dirty()
            || self.theme_workflow.prompt.is_some()
            || self.theme_workflow.exit.is_some()
        {
            return Task::none();
        }
        let Some(id) = self.window else {
            return Task::none();
        };
        self.theme_workflow.probe_armed = false;
        let hide = self.set_visible_owned(false, "owned-theme-exit-probe");
        let generation = self.visibility_generation;
        self.theme_workflow.probe = Some(ThemeExitProbe {
            window: id,
            generation,
            recovering: false,
        });
        crate::apply_trace::event_lazy("ui-theme-probe-hide", || {
            format!(
                "window={id:?} generation={generation} internal_visible={}",
                self.visible
            )
        });
        let observe =
            hide.chain(
                window::mode(id).map(move |mode| Message::ThemeExitProbeObserved {
                    window: id,
                    generation,
                    attempt: 0,
                    mode,
                }),
            );
        let timeout = move |_| Message::ThemeExitProbeTimeout {
            window: id,
            generation,
        };
        let recover = match theme_probe_delay(std::time::Duration::from_secs(1)) {
            Ok(receiver) => Task::perform(receiver, timeout),
            Err(error) => {
                log_debug(&format!("Owned Theme probe watchdog unavailable: {error}"));
                Task::done(Message::ThemeExitProbeTimeout {
                    window: id,
                    generation,
                })
            }
        };
        Task::batch([observe, recover])
    }

    fn theme_exit_probe_matches(&self, id: window::Id, generation: u64) -> bool {
        self.theme_workflow
            .probe
            .as_ref()
            .is_some_and(|probe| probe.window == id && probe.generation == generation)
    }

    pub(super) fn finish_theme_exit_probe_failure(
        &mut self,
        id: window::Id,
        generation: u64,
    ) -> Task<Message> {
        if !self.theme_exit_probe_matches(id, generation) {
            return Task::none();
        }
        let probe = self.theme_workflow.probe.as_mut().expect("matched above");
        if probe.recovering {
            return Task::none();
        }
        // A timeout cannot cancel the already-issued Windowed->Hidden chain.
        // Keep its original completion identity until the late observation so
        // that a late Hidden command is followed by a final reveal.
        probe.recovering = true;
        self.reveal_failed_theme_exit_probe(id, generation)
    }

    fn reveal_failed_theme_exit_probe(&mut self, id: window::Id, generation: u64) -> Task<Message> {
        self.theme_workflow.message =
            "Hidden Exit probe did not settle; draft retained. No Exit was authorized.".into();
        crate::apply_trace::event_lazy("ui-theme-probe-recovery", || {
            format!("window={id:?} generation={generation} current_window={:?} current_generation={} internal_visible={}", self.window, self.visibility_generation, self.visible)
        });
        if self.window == Some(id) || self.window.is_none() {
            self.set_visible(true, "owned-theme-exit-probe-recovery")
        } else {
            // An old hide command cannot affect a replacement window.
            Task::none()
        }
    }

    pub(super) fn finish_theme_exit_probe_poll_failure(
        &mut self,
        id: window::Id,
        generation: u64,
    ) -> Task<Message> {
        if !self.theme_exit_probe_matches(id, generation) {
            return Task::none();
        }
        // Poll delays only start after the hide chain has completed. There is
        // no outstanding hide to retain when its timer fails or is cancelled.
        self.theme_workflow.probe = None;
        self.reveal_failed_theme_exit_probe(id, generation)
    }

    pub(super) fn observe_theme_exit_probe(
        &mut self,
        id: window::Id,
        generation: u64,
        attempt: u8,
        mode: window::Mode,
    ) -> Task<Message> {
        if !self.theme_exit_probe_matches(id, generation) {
            return Task::none();
        }
        if self
            .theme_workflow
            .probe
            .as_ref()
            .expect("matched above")
            .recovering
            || self.window != Some(id)
            || self.visibility_generation != generation
            || self.visible
        {
            self.theme_workflow.probe = None;
            return self.reveal_failed_theme_exit_probe(id, generation);
        }
        crate::apply_trace::event_lazy("ui-theme-probe-mode", || {
            format!("window={id:?} generation={generation} attempt={attempt} mode={mode:?} internal_visible=false")
        });
        if mode == window::Mode::Hidden {
            self.theme_workflow.probe = None;
            // The ordinary event reducer owns Exit/reveal/decision authority.
            // A native mode observation is not permission to abandon the draft.
            return Task::done(Message::Wake(Wake::Tray(TrayAction::Exit)));
        }
        if attempt >= 7 {
            self.theme_workflow.probe = None;
            return self.reveal_failed_theme_exit_probe(id, generation);
        }
        let receiver = match theme_probe_delay(std::time::Duration::from_millis(75)) {
            Ok(receiver) => receiver,
            Err(error) => {
                log_debug(&format!("Owned Theme probe poll unavailable: {error}"));
                return self.finish_theme_exit_probe_poll_failure(id, generation);
            }
        };
        Task::perform(receiver, |result| result.is_ok()).then(move |ready| {
            if !ready {
                return Task::done(Message::ThemeExitProbePollFailed {
                    window: id,
                    generation,
                });
            }
            window::mode(id).map(move |mode| Message::ThemeExitProbeObserved {
                window: id,
                generation,
                attempt: attempt + 1,
                mode,
            })
        })
    }
}

pub(super) const PLAYLIST_CATALOG_SNAPSHOT_STAGE: &str = "automation-playlist-catalog-snapshot";

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub(super) struct CatalogWallpaperObservation {
    pub(super) id: String,
    pub(super) source_type: String,
    pub(super) canonical_file_target: String,
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub(super) struct PlaylistMemberCatalogObservation {
    configured_member_id: String,
    configured_occurrences: usize,
    pub(super) catalog_match_count: usize,
    pub(super) observed_catalog_member_ids: Vec<String>,
    pub(super) wallpaper_matches: Vec<CatalogWallpaperObservation>,
    evaluator_result_count: usize,
    pub(super) status: &'static str,
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub(super) struct ExplicitPlaylistCatalogObservation {
    pub(super) playlist_id: String,
    pub(super) membership: &'static str,
    configured_member_ids: Vec<String>,
    pub(super) members: Vec<PlaylistMemberCatalogObservation>,
    pub(super) evaluator_result_ids: Vec<String>,
}

#[derive(Debug, serde::Serialize, PartialEq, Eq)]
pub(super) struct CatalogPlaylistSnapshot {
    pub(super) catalog_ready: bool,
    pub(super) catalog_generation: u64,
    pub(super) profile_path: String,
    catalog_entry_count: usize,
    pub(super) configured_playlist_ids: Vec<String>,
    missing_playlist_ids: Vec<String>,
    ambiguous_playlist_ids: Vec<String>,
    non_explicit_playlist_ids: Vec<String>,
    pub(super) explicit_playlists: Vec<ExplicitPlaylistCatalogObservation>,
}

pub(super) fn explicit_playlist_catalog_observation(
    playlist: &Playlist,
    catalog: &[library::CatalogEntry],
    wallpapers: &[WallpaperItem],
    favorites: &BTreeSet<String>,
) -> Option<ExplicitPlaylistCatalogObservation> {
    let PlaylistMembership::Explicit { wallpaper_ids } = &playlist.membership else {
        return None;
    };

    let evaluator_result_ids: Vec<_> = library::evaluate_playlist(playlist, catalog, favorites)
        .into_iter()
        .map(|entry| entry.id.clone())
        .collect();
    let members = wallpaper_ids
        .iter()
        .map(|configured_member_id| {
            let catalog_matches: Vec<_> = catalog
                .iter()
                .filter(|entry| entry.id == *configured_member_id)
                .collect();
            let wallpaper_matches: Vec<_> = wallpapers
                .iter()
                .filter(|wallpaper| wallpaper.id == *configured_member_id)
                .map(|wallpaper| CatalogWallpaperObservation {
                    id: wallpaper.id.clone(),
                    source_type: format!("{:?}", wallpaper.source),
                    canonical_file_target: wallpaper.file_target.to_string_lossy().into_owned(),
                })
                .collect();
            let configured_occurrences = wallpaper_ids
                .iter()
                .filter(|member_id| *member_id == configured_member_id)
                .count();
            let evaluator_result_count = evaluator_result_ids
                .iter()
                .filter(|member_id| *member_id == configured_member_id)
                .count();
            let catalog_match_count = catalog_matches.len();
            let status = if catalog_match_count == 0 {
                "catalog_member_missing"
            } else if wallpaper_matches.is_empty() {
                "wallpaper_item_missing"
            } else if configured_occurrences != 1
                || catalog_match_count != 1
                || wallpaper_matches.len() != 1
            {
                "ambiguous"
            } else if evaluator_result_count == 0 {
                "not_in_evaluator_result"
            } else if evaluator_result_count != 1 {
                "ambiguous_evaluator_result"
            } else {
                "resolved"
            };
            PlaylistMemberCatalogObservation {
                configured_member_id: configured_member_id.clone(),
                configured_occurrences,
                catalog_match_count,
                observed_catalog_member_ids: catalog_matches
                    .into_iter()
                    .map(|entry| entry.id.clone())
                    .collect(),
                wallpaper_matches,
                evaluator_result_count,
                status,
            }
        })
        .collect();

    Some(ExplicitPlaylistCatalogObservation {
        playlist_id: playlist.id.clone(),
        membership: "explicit",
        configured_member_ids: wallpaper_ids.clone(),
        members,
        evaluator_result_ids,
    })
}

pub(super) fn build_catalog_playlist_snapshot(
    state: &CandidateState,
    catalog: &[library::CatalogEntry],
    wallpapers: &[WallpaperItem],
    profile_path: std::path::PathBuf,
    catalog_ready: bool,
    catalog_generation: u64,
) -> CatalogPlaylistSnapshot {
    let mut configured_playlist_ids = BTreeSet::new();
    if let Some(active_playlist_id) = &state.active_playlist_id {
        configured_playlist_ids.insert(active_playlist_id.clone());
    }
    configured_playlist_ids.extend(
        state
            .schedules
            .iter()
            .filter(|schedule| schedule.enabled)
            .map(|schedule| schedule.playlist_id.clone()),
    );

    let mut missing_playlist_ids = Vec::new();
    let mut ambiguous_playlist_ids = Vec::new();
    let mut non_explicit_playlist_ids = Vec::new();
    let mut explicit_playlists = Vec::new();
    for playlist_id in &configured_playlist_ids {
        let matching_playlists: Vec<_> = state
            .playlists
            .iter()
            .filter(|playlist| playlist.id == *playlist_id)
            .collect();
        match matching_playlists.as_slice() {
            [] => missing_playlist_ids.push(playlist_id.clone()),
            [playlist] => {
                if let Some(snapshot) = explicit_playlist_catalog_observation(
                    playlist,
                    catalog,
                    wallpapers,
                    &state.favorites,
                ) {
                    explicit_playlists.push(snapshot);
                } else {
                    non_explicit_playlist_ids.push(playlist_id.clone());
                }
            }
            _ => ambiguous_playlist_ids.push(playlist_id.clone()),
        }
    }

    CatalogPlaylistSnapshot {
        catalog_ready,
        catalog_generation,
        profile_path: profile_path.to_string_lossy().into_owned(),
        catalog_entry_count: catalog.len(),
        configured_playlist_ids: configured_playlist_ids.into_iter().collect(),
        missing_playlist_ids,
        ambiguous_playlist_ids,
        non_explicit_playlist_ids,
        explicit_playlists,
    }
}

pub(super) fn initial_screenshot_source_filter(
    screenshot_path_present: bool,
    value: Option<&str>,
) -> SourceFilter {
    if screenshot_path_present {
        value
            .and_then(SourceFilter::from_screenshot_arg)
            .unwrap_or(SourceFilter::All)
    } else {
        SourceFilter::All
    }
}

pub(super) struct BenchmarkState {
    pub(super) started: std::time::Instant,
    pub(super) cold_open_ms: Option<f64>,
    pub(super) source_catalog_items: usize,
    pub(super) warm_cycles_requested: usize,
    pub(super) animation_seconds_requested: u64,
    pub(super) last_motion_frame: Option<std::time::Instant>,
    pub(super) frame_intervals_ms: Vec<f64>,
    pub(super) open_requested: Option<std::time::Instant>,
    pub(super) warm_open_ms: Vec<f64>,
    pub(super) report_path: std::path::PathBuf,
}

pub(super) struct ScenarioState {
    pub(super) request: candidate_scenario::ScenarioRequest,
    pub(super) report: candidate_scenario::ScenarioReport,
    pub(super) item_index: Option<usize>,
    pub(super) next_step: usize,
    pub(super) ready: bool,
    pub(super) started: std::time::Instant,
    pub(super) window_settlement: Option<ScenarioWindowSettlement>,
    pub(super) next_window_settlement_generation: u64,
}

pub(super) struct ScenarioWindowSettlement {
    tracker: candidate_scenario::WindowSettlementTracker,
    pub(super) started: std::time::Instant,
    pub(super) generation: u64,
    pub(super) visibility_generation: u64,
    window_id: window::Id,
}

pub(super) fn scenario_window_identity_matches(
    captured: window::Id,
    current: Option<window::Id>,
) -> bool {
    current == Some(captured)
}

impl GpuCandidate {
    pub(super) fn scenario_item_index(&self, requested_path: &std::path::Path) -> Option<usize> {
        self.wallpapers
            .iter()
            .position(|wallpaper| canonical_paths_equal(&wallpaper.file_target, requested_path))
    }

    pub(super) fn scenario_ready_after_posters(&mut self) {
        let Some(scenario) = self.scenario.as_mut() else {
            return;
        };
        if scenario.ready || !self.catalog_loaded || self.poster_failure {
            return;
        }
        scenario.ready = true;
        scenario.report.record(
            "catalog-ready",
            "passed",
            Some(format!(
                "catalog_items={} posters={}",
                self.wallpapers.len(),
                self.posters.len()
            )),
        );
        if let Some(tx) = WAKE_TX.get().and_then(|slot| slot.lock().ok()?.clone()) {
            let _ = tx.unbounded_send(Wake::ScenarioAdvance);
        }
    }

    fn scenario_record(&mut self, name: &str, status: &str, detail: Option<String>) {
        if let Some(scenario) = self.scenario.as_mut() {
            scenario.report.record(name, status, detail);
        }
    }

    pub(super) fn scenario_finish_failure(&mut self, error: impl Into<String>) -> Task<Message> {
        let error = error.into();
        let Some(mut scenario) = self.scenario.take() else {
            log_debug(&format!(
                "GPU candidate scenario failed without state: {error}"
            ));
            return Task::done(Message::Wake(Wake::ScenarioAbort));
        };
        let stage_name = candidate_scenario::step_names()
            .get(scenario.next_step.saturating_sub(1))
            .copied()
            .unwrap_or("scenario");
        scenario
            .report
            .record(stage_name, "failed", Some(error.clone()));
        scenario.report.finish_failure(error.clone());
        scenario
            .report
            .set_elapsed_ms(scenario.started.elapsed().as_millis());
        if let Err(write_error) = scenario.report.write(&scenario.request.output) {
            log_debug(&format!(
                "Could not write failed GPU candidate scenario report: {write_error}"
            ));
            scenario.report.emit_json();
        }
        log_debug(&format!("GPU candidate scenario failed: {error}"));
        self.set_visible(false, "scenario-failure")
            .chain(Task::done(Message::Wake(Wake::ScenarioAbort)))
    }

    fn scenario_request_exit(&mut self) -> Task<Message> {
        let Some(mut scenario) = self.scenario.take() else {
            return Task::none();
        };
        scenario.report.record(
            "tray-exit",
            "requested",
            Some("existing TrayAction::Exit handler dispatched; external supervisor must verify process termination".into()),
        );
        scenario.report.mark_exit_requested();
        scenario
            .report
            .set_elapsed_ms(scenario.started.elapsed().as_millis());
        if let Err(error) = scenario.report.write(&scenario.request.output) {
            log_debug(&format!(
                "Could not write exit-requested GPU candidate scenario report: {error}"
            ));
            scenario.report.finish_failure(error);
            scenario.report.emit_json();
            return self
                .set_visible(false, "scenario-exit")
                .chain(Task::done(Message::Wake(Wake::ScenarioAbort)));
        }
        scenario.report.emit_json();
        update(self, Message::Wake(Wake::Tray(TrayAction::Exit)))
    }

    fn scenario_follow_up(
        &self,
        step: usize,
        step_name: &'static str,
        task: Task<Message>,
    ) -> Task<Message> {
        if let Some(expected) = candidate_scenario::window_settlement_mode(step_name) {
            let requested = match expected {
                candidate_scenario::WindowSettlementMode::Hidden => window::Mode::Hidden,
                candidate_scenario::WindowSettlementMode::Fullscreen => window::Mode::Fullscreen,
                candidate_scenario::WindowSettlementMode::Windowed
                | candidate_scenario::WindowSettlementMode::Unknown => {
                    return task.chain(Task::done(Message::Wake(Wake::ScenarioAbort)));
                }
            };
            task.chain(Task::done(Message::Wake(
                Wake::ScenarioBeginWindowSettlement { step, requested },
            )))
        } else {
            task.chain(Task::done(Message::Wake(Wake::ScenarioStepReady {
                step,
                window_mode: None,
            })))
        }
    }

    pub(super) fn begin_scenario_window_settlement(
        &mut self,
        step: usize,
        requested: window::Mode,
    ) -> Task<Message> {
        let Some(window_id) = self.window else {
            return self.scenario_finish_failure(format!(
                "scenario step {step} cannot observe window settlement without a window"
            ));
        };
        let expected = match requested {
            window::Mode::Hidden => candidate_scenario::WindowSettlementMode::Hidden,
            window::Mode::Fullscreen => candidate_scenario::WindowSettlementMode::Fullscreen,
            window::Mode::Windowed => candidate_scenario::WindowSettlementMode::Windowed,
        };
        let Some(scenario) = self.scenario.as_mut() else {
            return Task::none();
        };
        if scenario.next_step != step.saturating_add(1) || scenario.window_settlement.is_some() {
            log_debug(&format!(
                "Ignoring stale scenario window-settlement request for step {step}"
            ));
            return Task::none();
        }
        scenario.next_window_settlement_generation =
            scenario.next_window_settlement_generation.saturating_add(1);
        let generation = scenario.next_window_settlement_generation;
        let visibility_generation = self.visibility_generation;
        scenario.window_settlement = Some(ScenarioWindowSettlement {
            tracker: candidate_scenario::WindowSettlementTracker::new(
                step,
                expected,
                candidate_scenario::WINDOW_SETTLEMENT_TIMEOUT_MS,
            ),
            started: std::time::Instant::now(),
            generation,
            visibility_generation,
            window_id,
        });
        log_debug(&format!(
            "[SCENARIO-VISIBILITY] settlement-begin step={} generation={} requested={:?} window_id={window_id:?} current_window={:?} visibility_generation={} internal_visible={}",
            step, generation, requested, self.window, visibility_generation, self.visible
        ));
        self.poll_scenario_window(step, generation)
    }

    pub(super) fn poll_scenario_window(&mut self, step: usize, generation: u64) -> Task<Message> {
        let (window_id, visibility_generation) = {
            let Some(scenario) = self.scenario.as_ref() else {
                return Task::none();
            };
            let Some(settlement) = scenario.window_settlement.as_ref() else {
                log_debug(&format!(
                    "Ignoring stale scenario window poll for step {step}"
                ));
                return Task::none();
            };
            if settlement.generation != generation
                || !candidate_scenario::settlement_observation_is_current(
                    step,
                    generation,
                    settlement.tracker.step(),
                    settlement.generation,
                )
            {
                log_debug(&format!(
                    "Ignoring stale scenario window poll for step {step}, generation {generation}"
                ));
                return Task::none();
            }
            (settlement.window_id, settlement.visibility_generation)
        };
        if !candidate_scenario::settlement_visibility_generation_is_current(
            visibility_generation,
            self.visibility_generation,
        ) {
            log_debug(&format!(
                "[SCENARIO-VISIBILITY] settlement-invalidated step={step} generation={generation} captured_visibility_generation={visibility_generation} current_visibility_generation={} window_id={window_id:?}",
                self.visibility_generation
            ));
            return self.scenario_finish_failure(format!(
                "scenario window visibility request changed during settlement; captured generation {visibility_generation}, current generation {}",
                self.visibility_generation
            ));
        }
        if !scenario_window_identity_matches(window_id, self.window) {
            let current_window = self.window;
            log_debug(&format!(
                "[SCENARIO-VISIBILITY] settlement-window-identity-changed step={} generation={} captured={window_id:?} current={current_window:?}",
                step, generation
            ));
            return self.scenario_finish_failure(format!(
                "scenario step {step} window identity changed during settlement; captured={window_id:?} current={current_window:?}"
            ));
        }
        log_debug(&format!(
            "[SCENARIO-VISIBILITY] settlement-query step={} generation={} window_id={window_id:?} visibility_generation={visibility_generation} internal_visible={}",
            step, generation, self.visible
        ));
        window::mode(window_id).map(move |window_mode| {
            Message::Wake(Wake::ScenarioWindowObserved {
                step,
                generation,
                window_mode: Some(window_mode),
            })
        })
    }

    fn schedule_scenario_window_poll(&self, step: usize, generation: u64) -> Task<Message> {
        let delay = std::time::Duration::from_millis(candidate_scenario::WINDOW_SETTLEMENT_POLL_MS);
        Task::perform(
            async move {
                // This delay runs on Iced's task executor, not the UI thread. It only
                // schedules a fresh native mode observation; it never assumes settlement.
                std::thread::sleep(delay);
            },
            move |_| Message::Wake(Wake::ScenarioWindowPoll { step, generation }),
        )
    }

    pub(super) fn observe_scenario_window(
        &mut self,
        step: usize,
        generation: u64,
        window_mode: Option<window::Mode>,
    ) -> Task<Message> {
        let observed = match window_mode {
            Some(window::Mode::Hidden) => candidate_scenario::WindowSettlementMode::Hidden,
            Some(window::Mode::Fullscreen) => candidate_scenario::WindowSettlementMode::Fullscreen,
            Some(window::Mode::Windowed) => candidate_scenario::WindowSettlementMode::Windowed,
            None => candidate_scenario::WindowSettlementMode::Unknown,
        };
        let Some((active_step, active_generation, visibility_generation, window_id)) = self
            .scenario
            .as_ref()
            .and_then(|scenario| scenario.window_settlement.as_ref())
            .map(|settlement| {
                (
                    settlement.tracker.step(),
                    settlement.generation,
                    settlement.visibility_generation,
                    settlement.window_id,
                )
            })
        else {
            log_debug(&format!(
                "Ignoring stale scenario window observation for step {step}, generation {generation}"
            ));
            return Task::none();
        };

        if !candidate_scenario::settlement_observation_is_current(
            step,
            generation,
            active_step,
            active_generation,
        ) {
            log_debug(&format!(
                "Ignoring stale scenario window observation for step {step}, generation {generation}; active_step={active_step} active_generation={active_generation}"
            ));
            return Task::none();
        }
        if !candidate_scenario::settlement_visibility_generation_is_current(
            visibility_generation,
            self.visibility_generation,
        ) {
            log_debug(&format!(
                "[SCENARIO-VISIBILITY] settlement-invalidated-by-later-request step={step} generation={generation} captured_visibility_generation={visibility_generation} current_visibility_generation={} window_id={window_id:?}",
                self.visibility_generation
            ));
            return self.scenario_finish_failure(format!(
                "scenario window visibility request changed during settlement; captured generation {visibility_generation}, current generation {}",
                self.visibility_generation
            ));
        }
        if !scenario_window_identity_matches(window_id, self.window) {
            let current_window = self.window;
            log_debug(&format!(
                "[SCENARIO-VISIBILITY] settlement-window-identity-changed-after-query step={step} generation={generation} captured={window_id:?} current={current_window:?}"
            ));
            return self.scenario_finish_failure(format!(
                "scenario step {step} window identity changed before the native mode observation was handled; captured={window_id:?} current={current_window:?}"
            ));
        }

        let Some((decision, detail, elapsed_ms, requested)) = self
            .scenario
            .as_mut()
            .and_then(|scenario| scenario.window_settlement.as_mut())
            .map(|settlement| {
                let elapsed_ms = settlement.started.elapsed().as_millis() as u64;
                let decision = settlement.tracker.observe(step, observed, elapsed_ms);
                (
                    decision,
                    settlement.tracker.detail(u128::from(elapsed_ms)),
                    elapsed_ms,
                    settlement.tracker.requested(),
                )
            })
        else {
            return Task::none();
        };

        log_debug(&format!(
            "[SCENARIO-VISIBILITY] settlement-observed step={} generation={} window_id={window_id:?} requested={requested:?} observed={observed:?} decision={decision:?} elapsed_ms={elapsed_ms} internal_visible={} visibility_generation={}",
            step, generation, self.visible, self.visibility_generation
        ));

        match decision {
            candidate_scenario::WindowSettlementDecision::Stale => {
                log_debug(&format!(
                    "Ignoring stale scenario window observation for step {step}, generation {generation}"
                ));
                Task::none()
            }
            candidate_scenario::WindowSettlementDecision::Pending => {
                self.schedule_scenario_window_poll(step, generation)
            }
            candidate_scenario::WindowSettlementDecision::Settled => {
                if let Some(scenario) = self.scenario.as_mut() {
                    scenario.window_settlement = None;
                }
                self.complete_scenario_step(step, window_mode, Some(detail))
            }
            candidate_scenario::WindowSettlementDecision::DeadlineExpired => self
                .scenario_finish_failure(format!(
                    "scenario window settlement expired after {elapsed_ms}ms; {detail}"
                )),
        }
    }

    pub(super) fn run_scenario_step(&mut self) -> Task<Message> {
        let Some(scenario) = self.scenario.as_ref() else {
            return Task::none();
        };
        if !scenario.ready {
            return Task::none();
        }
        let Some(item_index) = scenario.item_index else {
            return self.scenario_finish_failure("scenario item identity was not resolved");
        };
        let step = scenario.next_step;
        if step >= candidate_scenario::step_names().len() {
            return self.scenario_finish_failure("scenario advanced beyond its final step");
        }
        if let Some(scenario) = self.scenario.as_mut() {
            scenario.next_step = scenario.next_step.saturating_add(1);
        }
        let step_name = candidate_scenario::step_names()
            .get(step)
            .copied()
            .unwrap_or("scenario");

        log_debug(&format!(
            "[SCENARIO] dispatch step={} name={} window_id={:?} visible={}",
            step, step_name, self.window, self.visible
        ));

        let task = match step_name {
            "pictures-filter" => update(self, Message::SetSourceFilter(SourceFilter::Pictures)),
            "no-match-filter" => update(
                self,
                Message::SearchChanged("__gpu_candidate_ui_smoke_no_match__".into()),
            ),
            "clear-filter" => update(self, Message::SearchChanged(String::new())),
            "search-open" => update(self, Message::OpenSearch),
            "search-close" => update(self, Message::CloseSearchPanel),
            "settings-open" => update(
                self,
                Message::SetSettingsCategory(SettingsCategory::CatalogFilters),
            ),
            "settings-close" => update(self, Message::CloseSearchPanel),
            "select-item" => {
                self.selected = None;
                update(
                    self,
                    Message::CardClicked {
                        index: item_index,
                        mouse_sequence: None,
                    },
                )
            }
            "detail-open" => update(
                self,
                Message::CardClicked {
                    index: item_index,
                    mouse_sequence: None,
                },
            ),
            "hide-gallery" => update(self, Message::HideGallery),
            "tray-open-hidden" | "tray-open-visible" => {
                update(self, Message::Wake(Wake::Tray(TrayAction::Open)))
            }
            "hide-before-exit" => update(self, Message::HideGallery),
            "tray-exit" => return self.scenario_request_exit(),
            _ => return self.scenario_finish_failure("unknown scenario step"),
        };

        self.scenario_follow_up(step, step_name, task)
    }

    pub(super) fn complete_scenario_step(
        &mut self,
        step: usize,
        window_mode: Option<window::Mode>,
        settlement_detail: Option<String>,
    ) -> Task<Message> {
        let Some(scenario) = self.scenario.as_ref() else {
            return Task::none();
        };
        let Some(item_index) = scenario.item_index else {
            return self.scenario_finish_failure("scenario item identity was not resolved");
        };
        let Some(step_name) = candidate_scenario::step_names().get(step).copied() else {
            return self.scenario_finish_failure("scenario completion referred to an unknown step");
        };

        let failure = |app: &mut Self, detail: String| app.scenario_finish_failure(detail);
        let mut detail = settlement_detail;
        let valid = match step_name {
            "pictures-filter" => {
                let valid = self.filtered_indices.contains(&item_index)
                    && self.filtered_indices.iter().all(|index| {
                        self.wallpapers.get(*index).is_some_and(|wallpaper| {
                            wallpaper.source == WallpaperSource::LocalImage
                        })
                    });
                detail = Some(format!("filtered_items={}", self.filtered_indices.len()));
                valid
            }
            "no-match-filter" => {
                self.filtered_indices.is_empty()
                    && self.catalog_presentation() == CatalogPresentation::EmptyFilter
            }
            "clear-filter" => {
                let valid = self.filtered_indices.contains(&item_index);
                detail = Some(format!("filtered_items={}", self.filtered_indices.len()));
                valid
            }
            "search-open" => self.panel_state.search_open && !self.panel_state.settings_open,
            "search-close" => !self.panel_state.search_open && !self.panel_state.settings_open,
            "settings-open" => {
                self.panel_state.settings_open
                    && self.panel_state.settings_category == SettingsCategory::CatalogFilters
            }
            "settings-close" => !self.panel_state.search_open && !self.panel_state.settings_open,
            "select-item" => {
                detail = Some("selection did not submit Apply".into());
                self.selected == Some(item_index)
                    && !self.detail_open
                    && !self.apply_pending
                    && self.active_operation.is_none()
            }
            "detail-open" => {
                detail = Some("second selection opened detail without Apply".into());
                self.detail_open && !self.apply_pending && self.active_operation.is_none()
            }
            "hide-gallery" | "hide-before-exit" => {
                if detail.is_none() {
                    detail = Some(format!("window_mode={window_mode:?}"));
                }
                !self.visible && window_mode == Some(window::Mode::Hidden)
            }
            "tray-open-hidden" | "tray-open-visible" => {
                if detail.is_none() {
                    detail = Some(format!("window_mode={window_mode:?}"));
                }
                self.visible && window_mode == Some(window::Mode::Fullscreen)
            }
            "tray-exit" => false,
            _ => false,
        };

        if !valid {
            return failure(
                self,
                format!("scenario step {step_name} did not settle as expected; window_mode={window_mode:?}"),
            );
        }
        self.scenario_record(step_name, "passed", detail);
        if step + 1 != candidate_scenario::step_names().len() {
            Task::done(Message::Wake(Wake::ScenarioAdvance))
        } else {
            self.scenario_finish_failure(
                "scenario reached the end without dispatching TrayAction::Exit",
            )
        }
    }

    pub(super) fn maybe_schedule_screenshot(&mut self) {
        if self.screenshot_path.is_none()
            || self.screenshot_requested
            || self.posters.is_empty()
            || (self.backdrop_source.is_some() && self.backdrop.is_none())
        {
            return;
        }
        let Some(screenshot_tx) = WAKE_TX.get().and_then(|slot| slot.lock().ok()?.clone()) else {
            return;
        };
        self.screenshot_requested = true;
        crate::apply_trace::event_lazy("gallery-screenshot-request", || {
            format!(
                "phase=scheduled path={} posters={} depth={}",
                self.screenshot_path
                    .as_deref()
                    .map_or_else(|| "<none>".into(), |path| path.display().to_string()),
                self.posters.len(),
                self.depth_frame.is_some(),
            )
        });
        std::thread::Builder::new()
            .name("gpu-candidate-screenshot".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let _ = screenshot_tx.unbounded_send(Wake::CaptureScreenshot);
            })
            .ok();
    }

    pub(super) fn write_benchmark_report(&mut self) {
        let Some(benchmark) = &self.benchmark else {
            return;
        };
        let percentile = |samples: &[f64], percentile: f64| -> Option<f64> {
            if samples.is_empty() {
                return None;
            }
            let mut sorted = samples.to_vec();
            sorted.sort_by(f64::total_cmp);
            let index = ((sorted.len() - 1) as f64 * percentile)
                .ceil()
                .min((sorted.len() - 1) as f64) as usize;
            sorted.get(index).copied()
        };
        let mean = |samples: &[f64]| -> Option<f64> {
            (!samples.is_empty())
                .then(|| samples.iter().copied().sum::<f64>() / samples.len() as f64)
        };
        let mut kinds = std::collections::BTreeMap::<String, usize>::new();
        for wallpaper in &self.wallpapers {
            *kinds
                .entry(wallpaper.wallpaper_type.to_ascii_lowercase())
                .or_default() += 1;
        }
        let report = serde_json::json!({
            "schema_version": 2,
            "measurement": {
                "warm_cycles_requested": benchmark.warm_cycles_requested,
                "animation_seconds_requested": benchmark.animation_seconds_requested,
                "frame_metric": "Iced motion-update interval; this is frame-request cadence, not a GPU timestamp query",
                "cold_open_metric": "process boot entry to first visible-poster completion",
                "warm_open_metric": "show request to visible-poster completion"
            },
            "source_catalog_items": benchmark.source_catalog_items,
            "catalog_items": self.wallpapers.len(),
            "catalog_kinds": kinds,
            "elapsed_seconds": benchmark.started.elapsed().as_secs_f64(),
            "cold_open_ms": benchmark.cold_open_ms,
            "warm_open_samples": benchmark.warm_open_ms.len(),
            "warm_open_p95_ms": percentile(&benchmark.warm_open_ms, 0.95),
            "warm_open_mean_ms": mean(&benchmark.warm_open_ms),
            "frame_interval_samples": benchmark.frame_intervals_ms.len(),
            "frame_interval_p95_ms": percentile(&benchmark.frame_intervals_ms, 0.95),
            "frame_interval_mean_ms": mean(&benchmark.frame_intervals_ms),
        });
        let result = benchmark
            .report_path
            .parent()
            .ok_or_else(|| "benchmark report path has no parent".to_string())
            .and_then(|parent| std::fs::create_dir_all(parent).map_err(|error| error.to_string()))
            .and_then(|()| {
                serde_json::to_vec_pretty(&report)
                    .map_err(|error| error.to_string())
                    .and_then(|contents| {
                        std::fs::write(&benchmark.report_path, contents)
                            .map_err(|error| error.to_string())
                    })
            });
        match result {
            Ok(()) => log_debug(&format!(
                "GPU benchmark report written to {}",
                benchmark.report_path.display()
            )),
            Err(error) => self.status = Some(format!("Could not write benchmark report: {error}")),
        }
    }
}

use super::*;
use crate::{apply_trace, automation, candidate_state, native_wallpaper};

#[derive(Clone, Copy)]
pub(super) struct AutomationTraceContext<'a> {
    pub(super) generation: Option<u64>,
    pub(super) source: &'static str,
    pub(super) schedule_id: Option<&'a str>,
}

impl AutomationTraceContext<'_> {
    pub(super) fn fields(self) -> String {
        format!(
            "source={} generation={:?} schedule_id={:?}",
            self.source, self.generation, self.schedule_id
        )
    }
}

fn trace_automation_decision(context: AutomationTraceContext<'_>, outcome: &str, reason: &str) {
    apply_trace::event_lazy("automation-decision", || {
        format!("{} outcome={outcome} reason={reason}", context.fields())
    });
}

pub(super) fn unique_playlist_id(name: &str, playlists: &[Playlist]) -> String {
    let base: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base = if base.is_empty() { "playlist" } else { &base };
    let mut candidate = base.to_string();
    let mut suffix = 2;
    while playlists.iter().any(|playlist| playlist.id == candidate) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

pub(super) fn parse_schedule_time(value: &str) -> Option<u16> {
    let (hour, minute) = value.trim().split_once(':')?;
    let hour: u16 = hour.parse().ok()?;
    let minute: u16 = minute.parse().ok()?;
    (hour < 24 && minute < 60).then_some(hour * 60 + minute)
}

pub(super) fn unique_schedule_id(name: &str, schedules: &[candidate_state::Schedule]) -> String {
    let base: String = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let base = if base.is_empty() { "schedule" } else { &base };
    let mut candidate = base.to_string();
    let mut suffix = 2;
    while schedules.iter().any(|schedule| schedule.id == candidate) {
        candidate = format!("{base}-{suffix}");
        suffix += 1;
    }
    candidate
}

fn automation_sample() -> u64 {
    static STATE: AtomicU64 = AtomicU64::new(0x9e37_79b9_7f4a_7c15);
    let mut current = STATE.load(Ordering::Relaxed);
    loop {
        let mut next = current;
        next ^= next << 13;
        next ^= next >> 7;
        next ^= next << 17;
        match STATE.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(actual) => current = actual,
        }
    }
}

pub(super) trait AutomationTickContext {
    fn generation(&self) -> u64;
    fn interval_suspended(&self) -> bool {
        false
    }
    fn stop_interval(&mut self);
    fn state(&self) -> &CandidateState;
    fn last_minute(&mut self) -> &mut Option<(u8, u16)>;
    fn next_due(&mut self) -> &mut u64;
    fn monitors(&mut self) -> Result<Vec<String>, String>;
    fn evaluate_schedule(
        &mut self,
        now_unix: u64,
        weekday: u8,
        minute_of_day: u16,
        available: &[String],
    ) -> automation::ScheduleEvaluation {
        automation::evaluate_schedule(self.state(), now_unix, weekday, minute_of_day, available)
    }
    fn run_playlist(
        &mut self,
        playlist_id: Option<&str>,
        monitor_ids: Option<Vec<String>>,
        trace_context: AutomationTraceContext<'_>,
    ) -> bool;
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum AutomationSubmissionBlock {
    ApplyPending,
    ProviderStateUnknown,
    ApplyDisabled,
}

impl AutomationSubmissionBlock {
    fn reason_code(&self) -> &'static str {
        match self {
            Self::ApplyPending => "apply_pending",
            Self::ProviderStateUnknown => "provider_state_unknown",
            Self::ApplyDisabled => "apply_disabled",
        }
    }
}

pub(super) fn automation_submission_block(
    apply_pending: bool,
    provider_state_unknown: bool,
    apply_enabled: bool,
) -> Option<AutomationSubmissionBlock> {
    if apply_pending {
        Some(AutomationSubmissionBlock::ApplyPending)
    } else if !provider_mutation_allowed(provider_state_unknown, None) {
        Some(AutomationSubmissionBlock::ProviderStateUnknown)
    } else if !apply_enabled {
        Some(AutomationSubmissionBlock::ApplyDisabled)
    } else {
        None
    }
}

impl AutomationTickContext for GpuCandidate {
    fn interval_suspended(&self) -> bool {
        self.automation_suspended
    }
    fn stop_interval(&mut self) {
        self.safely_stop_automation();
    }

    fn generation(&self) -> u64 {
        self.automation_generation.load(Ordering::Acquire)
    }
    fn state(&self) -> &CandidateState {
        &self.state
    }
    fn last_minute(&mut self) -> &mut Option<(u8, u16)> {
        &mut self.last_automation_minute
    }
    fn next_due(&mut self) -> &mut u64 {
        &mut self.next_slideshow_unix
    }
    fn monitors(&mut self) -> Result<Vec<String>, String> {
        native_wallpaper::monitors()
            .map(|monitors| {
                monitors
                    .into_iter()
                    .map(|monitor| monitor.device_id)
                    .collect()
            })
            .map_err(|error| format!("{error:?}"))
    }
    fn run_playlist(
        &mut self,
        playlist_id: Option<&str>,
        monitor_ids: Option<Vec<String>>,
        trace_context: AutomationTraceContext<'_>,
    ) -> bool {
        GpuCandidate::run_playlist(self, playlist_id, monitor_ids, trace_context)
    }
}

pub(super) fn consume_automation_tick<C: AutomationTickContext>(
    context: &mut C,
    generation: u64,
    now_unix: u64,
    weekday: u8,
    minute_of_day: u16,
) {
    // Producer cancellation cannot retract a tick already in the event queue.
    // Reject any nonmatch before even reading/mutating business state.
    let current_generation = context.generation();
    if generation != current_generation {
        apply_trace::event_lazy("automation-decision", || {
            format!(
                "source=tick generation={generation} current_generation={current_generation} outcome=rejected reason=obsolete_generation"
            )
        });
        return;
    }
    let monitors = match context.monitors() {
        Ok(monitors) => monitors,
        Err(error) => {
            apply_trace::event_lazy("automation-decision", || {
                format!(
                    "source=schedule generation={generation} now_unix={now_unix} weekday={weekday} minute_of_day={minute_of_day} outcome=rejected reason=monitor_enumeration_failed error={error}"
                )
            });
            context.stop_interval();
            return;
        }
    };
    let available = monitors;
    let minute_key = (weekday, minute_of_day);
    let mut schedule_claimed = false;
    if *context.last_minute() == Some(minute_key) {
        trace_automation_decision(
            AutomationTraceContext {
                generation: Some(generation),
                source: "schedule",
                schedule_id: None,
            },
            "rejected",
            "same_minute_suppression",
        );
    } else {
        *context.last_minute() = Some(minute_key);
        match context.evaluate_schedule(now_unix, weekday, minute_of_day, &available) {
            automation::ScheduleEvaluation::Selected(decision) => {
                apply_trace::event_lazy("automation-decision", || {
                    format!(
                        "source=schedule generation={generation} now_unix={now_unix} weekday={weekday} minute_of_day={minute_of_day} outcome=selected reason=eligible schedule_id={:?} playlist_id={:?} monitor_ids={:?}",
                        decision.schedule_id, decision.playlist_id, decision.monitor_ids
                    )
                });
                let trace_context = AutomationTraceContext {
                    generation: Some(generation),
                    source: "schedule",
                    schedule_id: Some(&decision.schedule_id),
                };
                schedule_claimed = true;
                context.run_playlist(
                    Some(&decision.playlist_id),
                    Some(decision.monitor_ids),
                    trace_context,
                );
            }
            automation::ScheduleEvaluation::Rejected(rejection) => {
                apply_trace::event_lazy("automation-decision", || {
                    format!(
                        "source=schedule generation={generation} now_unix={now_unix} weekday={weekday} minute_of_day={minute_of_day} outcome=rejected reason={} detail={rejection:?}",
                        rejection.reason_code()
                    )
                });
            }
        }
    }
    let intent = context.state().automation_intent();
    if schedule_claimed {
        trace_automation_decision(
            AutomationTraceContext {
                generation: Some(generation),
                source: "slideshow",
                schedule_id: None,
            },
            "not_evaluated",
            "schedule_claimed",
        );
    } else if context.interval_suspended()
        || intent.safely_stopped
        || (!intent.random_enabled && !intent.playlist_playing)
    {
        trace_automation_decision(
            AutomationTraceContext {
                generation: Some(generation),
                source: "slideshow",
                schedule_id: None,
            },
            "rejected",
            "slideshow_disabled",
        );
    } else if !context.state().automation_allowed(now_unix) {
        apply_trace::event_lazy("automation-decision", || {
            format!(
                "source=slideshow generation={generation} now_unix={now_unix} outcome=rejected reason=manual_holdoff until_unix={}",
                context.state().manual_holdoff_until_unix
            )
        });
    } else if now_unix < *context.next_due() {
        apply_trace::event_lazy("automation-decision", || {
            format!(
                "source=slideshow generation={generation} now_unix={now_unix} outcome=rejected reason=slideshow_not_due next_slideshow_unix={}",
                *context.next_due()
            )
        });
    } else {
        let previous_due = *context.next_due();
        let interval_seconds = context.state().slideshow_interval_seconds.max(10);
        *context.next_due() = now_unix.saturating_add(interval_seconds);
        apply_trace::event_lazy("automation-decision", || {
            format!(
                "source=slideshow generation={generation} now_unix={now_unix} outcome=selected reason=slideshow_due previous_due_unix={previous_due} interval_seconds={interval_seconds}"
            )
        });
        let playlist_id = intent
            .playlist_playing
            .then(|| context.state().active_playlist_id.clone())
            .flatten();
        if intent.playlist_playing && playlist_id.is_none() {
            context.stop_interval();
            return;
        }
        context.run_playlist(
            playlist_id.as_deref(),
            None,
            AutomationTraceContext {
                generation: Some(generation),
                source: "slideshow",
                schedule_id: None,
            },
        );
    }
}

pub(super) fn automation_timer_enabled(
    policy: runtime::LaunchPolicy,
    state: &CandidateState,
) -> bool {
    policy.allows_automatic_provider()
        && ((!state.automation_intent().safely_stopped
            && (state.automation_intent().random_enabled
                || state.automation_intent().playlist_playing))
            || state.schedules.iter().any(|schedule| schedule.enabled))
}

impl GpuCandidate {
    pub(super) fn random_rotation_state(&self) -> candidate_state::RandomRotationState {
        if self.automation_suspended && self.state.automation_intent().random_enabled {
            candidate_state::RandomRotationState::SafelyStopped
        } else {
            self.state.random_rotation_state()
        }
    }

    pub(super) fn playlist_playing(&self) -> bool {
        let intent = self.state.automation_intent();
        intent.playlist_playing && !intent.safely_stopped && !self.automation_suspended
    }

    fn publish_automation_change(&mut self, proposed: CandidateState, resume: bool) -> bool {
        let result = if resume {
            self.commit_automation_resume(proposed)
        } else {
            self.commit_local_state_change(proposed)
        };
        if result.is_ok() {
            return true;
        }
        if self.state.reconciliation_required.is_some() {
            self.automation_suspended = true;
            self.refresh_automation_timer();
        }
        false
    }

    pub(super) fn sync_automation_controls(&self) {
        if let Some(tray) = &self._tray {
            tray.set_rotation_state(self.random_rotation_state(), self.playlist_playing());
        }
    }

    /// Safety suspension remains effective in memory even if the stop cannot be made durable.
    pub(super) fn safely_stop_automation(&mut self) {
        if self.automation_suspended {
            return;
        }
        self.automation_suspended = true;
        let mut proposed = self.state.clone();
        let mut intent = proposed.automation_intent();
        intent.playlist_playing = false;
        intent.safely_stopped = true;
        proposed.automation_intent = Some(intent);
        let _ = self.commit_local_state_change(proposed);
        self.refresh_automation_timer();
        self.sync_automation_controls();
    }

    pub(super) fn set_random_rotation(&mut self, enabled: bool, now: u64) {
        let _ = self.publish_random_rotation(enabled, now);
    }

    pub(super) fn publish_random_rotation(&mut self, enabled: bool, now: u64) -> bool {
        let mut proposed = self.state.clone();
        let mut intent = proposed.automation_intent();
        intent.random_enabled = enabled;
        if enabled && (intent.safely_stopped || self.automation_suspended) {
            intent.playlist_playing = false;
        }
        // Switching off never resumes an independently stopped or playing playlist.
        if enabled && !intent.playlist_playing {
            if self.provider_state_unknown {
                self.status =
                    Some("Resolve provider ownership before resuming random rotation.".into());
                return false;
            }
            intent.safely_stopped = false;
        }
        let reset_due = !intent.playlist_playing;
        proposed.automation_intent = Some(intent);
        if self.publish_automation_change(proposed, enabled && reset_due) {
            if enabled && reset_due {
                self.automation_suspended = false;
            }
            if reset_due {
                self.next_slideshow_unix =
                    now.saturating_add(self.state.slideshow_interval_seconds.max(10));
            }
            self.refresh_automation_timer();
            self.sync_automation_controls();
            return true;
        }
        false
    }

    pub(super) fn start_playlist(&mut self, id: String, now: u64) {
        if !self.state.playlists.iter().any(|p| p.id == id) {
            return;
        }
        if self.provider_state_unknown {
            self.status =
                Some("Resolve provider ownership before starting playlist playback.".into());
            return;
        }
        let mut proposed = self.state.clone();
        let mut intent = proposed.automation_intent();
        intent.playlist_playing = true;
        intent.safely_stopped = false;
        proposed.active_playlist_id = Some(id);
        proposed.automation_intent = Some(intent);
        if self.publish_automation_change(proposed, true) {
            self.automation_suspended = false;
            self.next_slideshow_unix =
                now.saturating_add(self.state.slideshow_interval_seconds.max(10));
            self.refresh_automation_timer();
            self.sync_automation_controls();
            self.status = Some(format!("Playlist playback started; first change after {} seconds. Enabled random rotation is paused.", self.state.slideshow_interval_seconds.max(10)));
        }
    }

    pub(super) fn stop_playlist(&mut self, now: u64) {
        let mut proposed = self.state.clone();
        let mut intent = proposed.automation_intent();
        intent.playlist_playing = false;
        intent.safely_stopped |= self.automation_suspended;
        proposed.automation_intent = Some(intent);
        if self.publish_automation_change(proposed, false) {
            self.next_slideshow_unix =
                now.saturating_add(self.state.slideshow_interval_seconds.max(10));
            self.refresh_automation_timer();
            self.sync_automation_controls();
            self.status = Some(
                if self.random_rotation_state() == candidate_state::RandomRotationState::On {
                    format!(
                        "Playlist stopped; enabled random rotation resumes after {} seconds.",
                        self.state.slideshow_interval_seconds.max(10)
                    )
                } else {
                    "Playlist stopped. Safely stopped rotation requires explicit Resume random or Start playlist.".into()
                },
            );
        }
    }

    pub(super) fn set_automation_interval(&mut self, interval: u64, now: u64) {
        let mut proposed = self.state.clone();
        proposed.slideshow_interval_seconds = interval.max(10);
        if self.publish_automation_change(proposed, false) {
            self.next_slideshow_unix = now.saturating_add(self.state.slideshow_interval_seconds);
            self.refresh_automation_timer();
        }
    }

    pub(super) fn refresh_automation_timer(&mut self) {
        self.sync_automation_controls();
        let generation = self.automation_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let enabled = automation_timer_enabled(self.launch_policy, &self.state)
            && (!self.automation_suspended || self.state.schedules.iter().any(|s| s.enabled));
        apply_trace::event_lazy("automation-timer-generation", || {
            let enabled_schedules = self
                .state
                .schedules
                .iter()
                .filter(|schedule| schedule.enabled)
                .count();
            format!(
                "generation={generation} enabled={enabled} slideshow_enabled={} enabled_schedule_count={enabled_schedules} interval_seconds=30",
                self.state.slideshow_enabled
            )
        });
        if !enabled {
            return;
        }
        let generation_clock = self.automation_generation.clone();
        let automation_tx = WAKE_TX.get().and_then(|slot| slot.lock().ok()?.clone());
        let Some(automation_tx) = automation_tx else {
            apply_trace::event_lazy("automation-timer-failure", || {
                format!("generation={generation} reason=wake_channel_unavailable")
            });
            return;
        };
        let spawn_result = std::thread::Builder::new()
            .name(format!("gpu-automation-{generation}"))
            .spawn(move || {
                apply_trace::event_lazy("automation-timer-start", || {
                    format!("generation={generation} interval_seconds=30 enabled=true")
                });
                loop {
                    let current_generation = generation_clock.load(Ordering::Acquire);
                    if generation != current_generation {
                        apply_trace::event_lazy("automation-timer-stopped", || {
                            format!(
                                "generation={generation} current_generation={current_generation} reason=superseded"
                            )
                        });
                        return;
                    }
                    let (weekday, minute_of_day) = local_schedule_time();
                    let now_unix = unix_now();
                    match automation_tx.unbounded_send(Wake::AutomationTick {
                        generation,
                        now_unix,
                        weekday,
                        minute_of_day,
                    }) {
                        Ok(()) => apply_trace::event_lazy("automation-tick-emitted", || {
                            format!(
                                "generation={generation} now_unix={now_unix} weekday={weekday} minute_of_day={minute_of_day}"
                            )
                        }),
                        Err(error) => {
                            apply_trace::event_lazy("automation-tick-send-failure", || {
                                format!(
                                    "generation={generation} now_unix={now_unix} weekday={weekday} minute_of_day={minute_of_day} error={error:?}"
                                )
                            });
                            return;
                        }
                    }
                    for _ in 0..30 {
                        std::thread::sleep(std::time::Duration::from_secs(1));
                        let current_generation = generation_clock.load(Ordering::Acquire);
                        if generation != current_generation {
                            apply_trace::event_lazy("automation-timer-stopped", || {
                                format!(
                                    "generation={generation} current_generation={current_generation} reason=superseded"
                                )
                            });
                            return;
                        }
                    }
                }
            });
        if let Err(error) = spawn_result {
            log_debug(&format!("Could not start automation timer: {error}"));
            apply_trace::event_lazy("automation-timer-failure", || {
                format!("generation={generation} reason=thread_spawn_failed error={error:?}")
            });
        }
    }

    pub(super) fn handle_automation_tick(
        &mut self,
        generation: u64,
        now_unix: u64,
        weekday: u8,
        minute_of_day: u16,
    ) {
        if generation != self.automation_generation.load(Ordering::Acquire) {
            return;
        }
        if !self.launch_policy.allows_automatic_provider() {
            self.status = Some(
                "Automatic provider execution requires normal mode; it is blocked in --preview."
                    .into(),
            );
            return;
        }
        consume_automation_tick(self, generation, now_unix, weekday, minute_of_day);
    }

    pub(super) fn run_playlist(
        &mut self,
        playlist_id: Option<&str>,
        monitor_ids: Option<Vec<String>>,
        trace_context: AutomationTraceContext<'_>,
    ) -> bool {
        self.run_playlist_with(
            playlist_id,
            monitor_ids,
            trace_context,
            native_wallpaper::monitors,
            |owner, item, target, transition| owner.submit(item, target, transition),
        )
    }

    pub(super) fn run_playlist_with(
        &mut self,
        playlist_id: Option<&str>,
        monitor_ids: Option<Vec<String>>,
        trace_context: AutomationTraceContext<'_>,
        enumerate: impl FnOnce() -> Result<Vec<native_wallpaper::NativeMonitor>, String>,
        submit: impl FnOnce(
            &WallpaperCoordinator,
            WallpaperItem,
            ApplyTarget,
            transition::TransitionConfig,
        ) -> Result<coordinator::ApplySubmission, String>,
    ) -> bool {
        if trace_context.generation.is_some_and(|generation| {
            generation != self.automation_generation.load(Ordering::Acquire)
        }) {
            return false;
        }
        if !self.launch_policy.allows_provider() {
            trace_automation_decision(trace_context, "rejected", "preview_policy");
            self.status = Some("Automatic provider execution is unavailable in --preview.".into());
            return false;
        }
        if let Some(block) = automation_submission_block(
            self.apply_pending,
            self.provider_state_unknown,
            self.apply_enabled,
        ) {
            trace_automation_decision(trace_context, "rejected", block.reason_code());
            match block {
                AutomationSubmissionBlock::ApplyPending => {}
                AutomationSubmissionBlock::ProviderStateUnknown => {
                    if trace_context.generation.is_some() {
                        self.safely_stop_automation();
                    }
                    self.status = Some(
                        "Wallpaper provider state is unresolved; further provider changes remain blocked.".into(),
                    );
                }
                AutomationSubmissionBlock::ApplyDisabled => {
                    self.status = Some(
                        "Automatic Apply is disabled in this isolated candidate. Launch with --allow-apply only for an approved runtime session."
                            .into(),
                    );
                }
            }
            return false;
        }
        if !self.startup_queue.is_empty() || !self.state.automation_allowed(unix_now()) {
            trace_automation_decision(
                trace_context,
                "rejected",
                "startup_pending_or_manual_holdoff",
            );
            return false;
        }
        let monitors = match enumerate() {
            Ok(monitors) => monitors,
            Err(error) => {
                trace_automation_decision(trace_context, "rejected", "monitor_enumeration_failed");
                self.status = Some(format!(
                    "Automation could not observe current targets: {error}"
                ));
                if trace_context.generation.is_some() {
                    self.safely_stop_automation();
                }
                return false;
            }
        };
        let monitor_count = monitors.len();
        // Config's Monitor{index} keys must use this observation's ordering.
        self.monitor_ids = monitors
            .iter()
            .map(|monitor| monitor.device_id.clone())
            .collect();
        view::settings::bridge::observe_monitors(&mut self.settings_workbench, &monitors);
        let mut locked_count = 0;
        let targets = monitor_ids.unwrap_or_else(|| {
            self.monitor_ids
                .iter()
                .filter_map(|id| {
                    if self
                        .state
                        .locked_monitors
                        .iter()
                        .any(|locked| locked.eq_ignore_ascii_case(id))
                    {
                        locked_count += 1;
                        None
                    } else {
                        Some(id.clone())
                    }
                })
                .collect()
        });
        if targets.is_empty() {
            let reason = if locked_count > 0 {
                "monitor_locked"
            } else {
                "target_unavailable"
            };
            apply_trace::event_lazy("automation-decision", || {
                format!(
                    "{} outcome=rejected reason={reason} monitor_count={monitor_count} locked_count={locked_count}",
                    trace_context.fields()
                )
            });
            if locked_count == 0 && trace_context.generation.is_some() {
                self.safely_stop_automation();
            }
            self.status = Some("Every automation target is locked or unavailable.".into());
            return false;
        }
        // Explicit schedule targets stay exact; never fall back to the picker target.
        if targets.iter().any(|target| {
            !self
                .monitor_ids
                .iter()
                .any(|id| id.eq_ignore_ascii_case(target))
        }) {
            if trace_context.generation.is_some() {
                self.safely_stop_automation();
            }
            self.status =
                Some("Automation target is unavailable; refresh displays before retrying.".into());
            return false;
        }
        let (next_id, origin) = if let Some(playlist_id) = playlist_id {
            let Some(playlist) = self
                .state
                .playlists
                .iter()
                .find(|playlist| playlist.id == playlist_id)
            else {
                trace_automation_decision(trace_context, "rejected", "playlist_unavailable");
                if trace_context.generation.is_some() {
                    self.safely_stop_automation();
                }
                self.status = Some("The active playlist is no longer available.".into());
                return false;
            };
            let current_id = self
                .selected
                .and_then(|index| self.wallpapers.get(index))
                .map(|item| item.id.as_str());
            (
                automation::next_playlist_wallpaper(
                    playlist,
                    &self.catalog,
                    &self.state.favorites,
                    current_id,
                    automation_sample(),
                ),
                operations::ChangeOrigin::Playlist,
            )
        } else {
            let eligible: Vec<_> = self
                .wallpapers
                .iter()
                .filter(|item| self.item_allowed_for_targets(item, &targets))
                .cloned()
                .collect();
            (
                automation::next_rotation_wallpaper(
                    &self.catalog,
                    &eligible,
                    &self
                        .state
                        .local_library
                        .rotation
                        .clone()
                        .unwrap_or_default(),
                    &self.state.favorites,
                    self.state
                        .local_library
                        .usage
                        .last_successful_random_id
                        .as_deref(),
                    automation_sample(),
                ),
                operations::ChangeOrigin::Random,
            )
        };
        let Some(next_id) = next_id else {
            trace_automation_decision(trace_context, "rejected", "empty_rotation_or_playlist_pool");
            if trace_context.generation.is_some() {
                self.safely_stop_automation();
            }
            self.status = Some("No eligible wallpaper matches the playlist or rotation media/Favourites policy and target restrictions.".into());
            return false;
        };
        let Some((wallpaper_index, wallpaper)) = self
            .wallpapers
            .iter()
            .enumerate()
            .find(|(_, item)| item.id == next_id)
            .map(|(index, item)| (index, item.clone()))
        else {
            if trace_context.generation.is_some() {
                self.safely_stop_automation();
            }
            return false;
        };
        if !self.item_allowed_for_targets(&wallpaper, &targets) {
            trace_automation_decision(trace_context, "rejected", "item_or_target_restricted");
            if trace_context.generation.is_some() {
                self.safely_stop_automation();
            }
            self.status = Some(
                "The playlist wallpaper/target is unavailable, hidden, excluded or locked.".into(),
            );
            return false;
        }
        let trace_submission = apply_trace::enabled().then(|| {
            format!(
                "playlist_id={playlist_id:?} item_id={:?} item_type={:?} source={:?} project_json_path={:?} file_target={:?} target_ids={targets:?}",
                wallpaper.id,
                wallpaper.wallpaper_type,
                wallpaper.source,
                wallpaper.project_json_path,
                wallpaper.file_target
            )
        });
        let captured_item = wallpaper.clone();
        match submit(
            &self.coordinator,
            wallpaper,
            ApplyTarget::DeviceIds(targets),
            self.config.transition.clone(),
        ) {
            Ok(submission) => {
                self.selected = Some(wallpaper_index);
                self.apply_pending = true;
                self.active_operation = Some(submission.operation_id);
                self.wallpaper_change = operations::CapturedWallpaperChange::new(
                    captured_item,
                    &submission,
                    self.catalog_generation.load(Ordering::Acquire),
                    origin,
                );
                if let Some(captured) = &mut self.wallpaper_change {
                    captured.automation_generation = trace_context.generation;
                }
                self.hide_after_apply = false;
                self.status = Some("Applying automation wallpaper…".into());
                if self.visible {
                    self.schedule_posters();
                }
                apply_trace::event_for_lazy(
                    submission.operation_id.get(),
                    "automation-submission",
                    || {
                        format!(
                            "{} outcome=submitted {}",
                            trace_context.fields(),
                            trace_submission.unwrap_or_else(|| "item_identity=not_captured".into())
                        )
                    },
                );
                true
            }
            Err(error) => {
                apply_trace::event_lazy("automation-decision", || {
                    format!(
                        "{} outcome=rejected reason=coordinator_submit_failed error={error:?}",
                        trace_context.fields()
                    )
                });
                if trace_context.generation.is_some() {
                    self.safely_stop_automation();
                }
                self.status = Some(error);
                false
            }
        }
    }
}

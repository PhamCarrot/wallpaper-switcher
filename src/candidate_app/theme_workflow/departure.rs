//! One draft-authorized departure and one post-reducer Exit gate.
use super::*;

#[derive(Debug, Clone, Copy)]
pub(in crate::candidate_app) struct ExitContinuation {
    pub binding: Binding,
    pub decision: u64,
    pub queued: bool,
}

impl GpuCandidate {
    pub(in crate::candidate_app) fn theme_departure_needed(&self, message: &Message) -> bool {
        lifecycle::theme_owner_active(self)
            && self.theme_session.designer_dirty()
            && match message {
                Message::SetSettingsCategory(category) => *category != SettingsCategory::Theme,
                Message::ToggleSettingsPanel => self.panel_state.settings_open,
                Message::TogglePlaylistPanel | Message::TogglePlaylistChooser => {
                    !self.playlist_panel_open
                }
                _ => false,
            }
    }

    pub(in crate::candidate_app) fn request_theme_departure(
        &mut self,
        next: Departure,
    ) -> Task<Message> {
        if self.theme_session.designer_dirty() {
            if let Some(binding) = Binding::current(self) {
                self.theme_workflow.exit = None;
                self.exit_requested = false;
                self.theme_prompt(binding, PromptKind::Departure(next));
                self.theme_workflow.message =
                    "Choose Save, Discard or Cancel. Nothing has been abandoned.".into();
                return Task::none();
            }
        }
        self.complete_theme_departure(next)
    }

    pub(super) fn complete_theme_departure(&mut self, next: Departure) -> Task<Message> {
        self.theme_workflow.prompt = None;
        match next {
            Departure::Load(target) => {
                self.load_theme_draft(target);
                Task::none()
            }
            Departure::Message(message) => events::update_inner(self, *message),
            Departure::Exit => self.authorize_theme_exit(),
        }
    }

    fn reveal_theme_decision(&mut self) -> Task<Message> {
        self.panel_state.open_settings(SettingsCategory::Theme);
        self.settings_workbench.windows_panel = Some(SettingsCategory::Theme);
        if self.visible {
            self.window
                .map(window::gain_focus)
                .unwrap_or_else(Task::none)
        } else {
            self.set_visible(true, "theme-exit-decision")
        }
    }

    pub(in crate::candidate_app) fn request_theme_exit(&mut self) -> Task<Message> {
        self.ensure_theme_designer();
        crate::apply_trace::event_lazy("ui-theme-exit-request", || {
            format!(
                "visible={} window={:?} dirty={} decision={:?} revision={}",
                self.visible,
                self.window,
                self.theme_session.designer_dirty(),
                self.theme_workflow.prompt.as_ref().map(|p| p.decision),
                self.theme_session.draft_revision()
            )
        });
        if self.theme_workflow.exit.is_some() && self.exit_requested {
            return self.reveal_theme_decision();
        }
        if self.theme_session.designer_dirty() {
            let task = self.request_theme_departure(Departure::Exit);
            return Task::batch([task, self.reveal_theme_decision()]);
        }
        self.authorize_theme_exit()
    }

    fn authorize_theme_exit(&mut self) -> Task<Message> {
        let binding = Binding::current(self).expect("Exit initializes the visual draft first");
        self.theme_prompt(binding, PromptKind::ExitWaiting);
        let prompt = self
            .theme_workflow
            .prompt
            .as_ref()
            .expect("prompt installed");
        self.theme_workflow.exit = Some(ExitContinuation {
            binding: prompt.binding,
            decision: prompt.decision,
            queued: false,
        });
        self.cancel_colours();
        self.exit_requested = true;
        self.theme_workflow.message =
            "Exit requested. Accepted work will settle first; Cancel keeps the application open."
                .into();
        self.reveal_theme_decision()
    }

    /// Called only after the current message (including Cancel) was reduced.
    /// A delivered finalization message is checked again against the same
    /// current draft and decision. Worker polling never executes this gate.
    pub(in crate::candidate_app) fn advance_exit_if_current(
        &mut self,
        delivered: Option<u64>,
    ) -> Task<Message> {
        let Some(exit) = self.theme_workflow.exit else {
            return Task::none();
        };
        let current_decision = self.theme_workflow.prompt.as_ref().is_some_and(|prompt| {
            prompt.binding == exit.binding
                && prompt.decision == exit.decision
                && matches!(prompt.kind, PromptKind::ExitWaiting)
        });
        if !self.exit_requested || !exit.binding.valid(self) || !current_decision {
            self.theme_workflow.exit = None;
            self.exit_requested = false;
            if self
                .theme_workflow
                .prompt
                .as_ref()
                .is_some_and(|prompt| prompt.decision == exit.decision)
            {
                self.theme_workflow.prompt = None;
            }
            return Task::none();
        }
        if delivered == Some(exit.decision) {
            self.theme_workflow
                .exit
                .as_mut()
                .expect("checked above")
                .queued = false;
        }
        if self.cache_operation.is_some() {
            self.theme_workflow.message =
                "Waiting for cache acknowledgment before Exit. Cancel keeps the application open."
                    .into();
            return Task::none();
        }

        match lifecycle::exit_action_for_ui(
            self.apply_pending,
            self.provider_state_unknown,
            &self.provider_recoveries,
            self.coordinator.shutdown_action(),
        ) {
            coordinator::ShutdownAction::Ready => {
                if delivered == Some(exit.decision) {
                    self.save_wall_browse_position();
                    crate::apply_trace::event_lazy("ui-theme-exit-committed", || {
                        format!(
                            "decision={} revision={} coordinator_ready=true",
                            exit.decision,
                            self.theme_session.draft_revision()
                        )
                    });
                    self.catalog_watch = None;
                    std::process::exit(0);
                }
                if !self
                    .theme_workflow
                    .exit
                    .as_ref()
                    .expect("checked above")
                    .queued
                {
                    self.theme_workflow
                        .exit
                        .as_mut()
                        .expect("checked above")
                        .queued = true;
                    return Task::done(Message::ExitCommitted {
                        decision: exit.decision,
                    });
                }
            }
            coordinator::ShutdownAction::WaitForActive => {
                self.theme_workflow.message="Finishing the active operation before Exit. Cancel keeps the application open.".into();
            }
            coordinator::ShutdownAction::CancelPreflight(recovery) => {
                if !self.apply_pending {
                    self.submit_engine_command(EngineCommand::CancelPreflight { recovery });
                }
            }
            coordinator::ShutdownAction::Blocked(reason) => {
                self.theme_workflow.message =
                    format!("Exit is held: {reason}. Cancel keeps the application open.");
            }
        }
        Task::none()
    }

    pub(in crate::candidate_app) fn theme_prompt_escape(
        &self,
        message: &Message,
    ) -> Option<Action> {
        let prompt = self.theme_workflow.prompt.as_ref()?;
        if !self.visible {
            return None;
        }
        let (key, modifiers, window) = match message {
            Message::SettingsCapturedKey {
                key,
                modifiers,
                window,
            } => (key, *modifiers, *window),
            Message::KeyPressed {
                key,
                modifiers,
                event,
            } => (key, *modifiers, event.event_window),
            _ => return None,
        };
        (self.window == Some(window) && lifecycle::matches_candidate_key(key, modifiers, "escape"))
            .then_some(Action::Decide {
                binding: prompt.binding,
                decision: prompt.decision,
                choice: Choice::Cancel,
            })
    }
}

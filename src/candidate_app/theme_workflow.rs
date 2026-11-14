//! Named Theme commands resolve identities before the one local-state publisher.
use super::*;
use crate::candidate_state::frontend::{self, AppliedThemeSnapshot, NamedThemeRecord, ThemeOrigin};

#[path = "theme_workflow/departure.rs"]
mod departure;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Binding {
    instance: u64,
    generation: u64,
    revision: u64,
    token: u64,
    variant: theme_roles::ColorVariant,
}
impl Binding {
    pub(super) fn current(app: &GpuCandidate) -> Option<Self> {
        let session = &app.theme_session;
        session.designer_active().then_some(())?;
        Some(Self {
            instance: session.editor_instance()?,
            generation: session.editor_selection_generation()?,
            revision: session.draft_revision(),
            token: session.session_token(),
            variant: session.editor_selected_variant()?,
        })
    }
    fn valid(self, app: &GpuCandidate) -> bool {
        Self::current(app) == Some(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Load {
    Starter(&'static str),
    Applied,
    Saved(u64),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Choice {
    Save,
    Discard,
    Cancel,
    Confirm,
}
#[derive(Debug, Clone)]
pub(super) enum Action {
    Name {
        instance: u64,
        text: String,
    },
    Save {
        binding: Binding,
        apply: bool,
    },
    Load {
        binding: Binding,
        target: Load,
    },
    Delete {
        binding: Binding,
        id: u64,
    },
    Decide {
        binding: Binding,
        decision: u64,
        choice: Choice,
    },
    Reset(Binding),
}
#[derive(Debug, Clone)]
pub(super) enum Departure {
    Load(Load),
    Message(Box<Message>),
    Exit,
}
#[derive(Debug, Clone)]
pub(super) enum PromptKind {
    Departure(Departure),
    ExitWaiting,
    Overwrite {
        target: NamedThemeRecord,
        apply: bool,
        then_load: Option<Departure>,
    },
    Delete(NamedThemeRecord),
}
#[derive(Debug, Clone)]
pub(super) struct Prompt {
    pub binding: Binding,
    pub decision: u64,
    pub kind: PromptKind,
}
#[derive(Debug, Default)]
pub(super) struct Workflow {
    pub prompt: Option<Prompt>,
    pub message: String,
    pub starter: Option<&'static str>,
    next_decision: u64,
    pub exit: Option<departure::ExitContinuation>,
    pub probe_armed: bool,
    pub probe: Option<diagnostics::ThemeExitProbe>,
}

impl GpuCandidate {
    fn theme_prompt(&mut self, binding: Binding, kind: PromptKind) {
        if !binding.valid(self) {
            return;
        }
        // Retain the draft, but revoke callbacks already queued by the editor
        // being replaced. The decision receives the new current authority.
        self.theme_session.on_departure();
        let binding = Binding::current(self).expect("named draft survives a decision boundary");
        self.theme_workflow.next_decision = self.theme_workflow.next_decision.wrapping_add(1);
        self.theme_workflow.prompt = Some(Prompt {
            binding,
            kind,
            decision: self.theme_workflow.next_decision,
        });
    }
    pub(super) fn ensure_theme_designer(&mut self) {
        if !self.theme_session.designer_active() {
            self.load_theme_draft(Load::Applied);
        }
    }
    fn load_theme_draft(&mut self, target: Load) {
        let (document, variant, name, id) = match target {
            Load::Saved(id) => {
                let Some(record) = self.state.theme_library.records.iter().find(|r| r.id == id)
                else {
                    self.theme_workflow.message = "Theme no longer exists.".into();
                    return;
                };
                (
                    record.document.clone(),
                    self.theme_session
                        .editor_selected_variant()
                        .unwrap_or(theme_roles::ColorVariant::Dark),
                    record.name.clone(),
                    Some(id),
                )
            }
            Load::Starter(id) => {
                let Some(preset) = frontend::starter_palettes()
                    .iter()
                    .find(|preset| preset.id == id)
                else {
                    return;
                };
                (preset.document.clone(), preset.variant, String::new(), None)
            }
            Load::Applied => match &self.state.applied_theme {
                Some(snapshot) => {
                    let id = match snapshot.origin {
                        ThemeOrigin::Custom { record_id } => Some(record_id),
                        ThemeOrigin::BuiltInNord => None,
                    };
                    let name = id
                        .and_then(|id| self.state.theme_library.records.iter().find(|r| r.id == id))
                        .map(|r| r.name.clone())
                        .unwrap_or_default();
                    (snapshot.document.clone(), snapshot.variant, name, id)
                }
                // A legacy appearance is not expanded into a synthetic document.
                None => (
                    frontend::nord_document(),
                    theme_roles::ColorVariant::Dark,
                    String::new(),
                    None,
                ),
            },
        };
        self.theme_session
            .load_designer(document, variant, name, id);
        self.theme_workflow.prompt = None;
        self.theme_workflow.starter = match target {
            Load::Starter(id) => Some(id),
            _ => None,
        };
        self.theme_workflow.message =
            "Loaded into the designer. Application appearance is unchanged.".into();
    }
    pub(super) fn reconcile_theme_prompt(&mut self) {
        if self
            .theme_workflow
            .prompt
            .as_ref()
            .is_some_and(|prompt| !prompt.binding.valid(self))
        {
            self.theme_workflow.prompt = None;
        }
    }
    pub(super) fn theme_action(&mut self, action: Action) -> Task<Message> {
        if !lifecycle::theme_owner_active(self) {
            return Task::none();
        }
        let mut task = Task::none();
        match action {
            Action::Name { instance, text } => {
                self.theme_session.designer_set_name(instance, text);
            }
            Action::Reset(binding) if binding.valid(self) => {
                self.theme_session.designer_reset(binding.instance);
            }
            Action::Save { binding, apply } if binding.valid(self) => {
                task = self.save_theme_draft(binding, apply, None, None);
            }
            Action::Load { binding, target } if binding.valid(self) => {
                if self.theme_session.designer_dirty() {
                    self.theme_prompt(binding, PromptKind::Departure(Departure::Load(target)));
                } else {
                    self.load_theme_draft(target);
                }
            }
            Action::Delete { binding, id } if binding.valid(self) => {
                if let Some(target) = self
                    .state
                    .theme_library
                    .records
                    .iter()
                    .find(|r| r.id == id)
                    .cloned()
                {
                    self.theme_prompt(binding, PromptKind::Delete(target));
                }
            }
            Action::Decide {
                binding,
                decision,
                choice,
            } if binding.valid(self) => {
                let Some(prompt) = self
                    .theme_workflow
                    .prompt
                    .clone()
                    .filter(|p| p.binding == binding && p.decision == decision)
                else {
                    return Task::none();
                };
                match (prompt.kind, choice) {
                    (kind, Choice::Cancel) => {
                        self.theme_workflow.message = if matches!(
                            kind,
                            PromptKind::ExitWaiting | PromptKind::Departure(Departure::Exit)
                        ) {
                            "Exit cancelled. Draft retained."
                        } else {
                            "Cancelled. Draft retained."
                        }
                        .into();
                        self.theme_workflow.prompt = None;
                        self.theme_workflow.exit = None;
                        self.exit_requested = false;
                        self.theme_session.designer_cancel_confirmation();
                    }
                    (PromptKind::Departure(target), Choice::Discard) => {
                        self.theme_session.designer_reset(binding.instance);
                        task = self.complete_theme_departure(target);
                    }
                    (PromptKind::Departure(target), Choice::Save) => {
                        task = self.save_theme_draft(binding, false, None, Some(target));
                    }
                    (
                        PromptKind::Overwrite {
                            target,
                            apply,
                            then_load,
                        },
                        Choice::Confirm,
                    ) => {
                        task = self.save_theme_draft(binding, apply, Some(target), then_load);
                    }
                    (PromptKind::Delete(target), Choice::Confirm) => {
                        if !self
                            .state
                            .theme_library
                            .records
                            .iter()
                            .any(|r| r == &target)
                        {
                            self.theme_workflow.message =
                                "Theme changed; request Delete again.".into();
                            self.theme_workflow.prompt = None;
                            return Task::none();
                        }
                        let Some(proposed) = self.state.propose_theme_delete(target.id) else {
                            return Task::none();
                        };
                        match self.commit_local_state_change(proposed) {
                            Ok(_) => {
                                self.theme_session.designer_forget_id(target.id);
                                self.theme_workflow.prompt = None;
                                self.theme_workflow.message =
                                    "Theme deleted. An applied deletion falls back to Nord.".into();
                                self.refresh_backdrop();
                            }
                            Err(_) => {
                                self.theme_workflow.message =
                                    self.save_failure.clone().unwrap_or_default()
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        self.reconcile_theme_prompt();
        task
    }
    fn save_theme_draft(
        &mut self,
        binding: Binding,
        apply: bool,
        confirmed: Option<NamedThemeRecord>,
        then_load: Option<Departure>,
    ) -> Task<Message> {
        if !binding.valid(self) {
            return Task::none();
        }
        if self.theme_session.editor_pending() {
            self.theme_workflow.message =
                "Set or reset the pending color before saving. The draft is retained.".into();
            return Task::none();
        }
        let name = self.theme_session.designer_name().trim().to_string();
        if name.is_empty() {
            self.theme_workflow.message = "Enter a Theme name.".into();
            return Task::none();
        }
        let Some(document) = self.theme_session.editor_document().cloned() else {
            return Task::none();
        };
        let target = self
            .state
            .theme_library
            .records
            .iter()
            .find(|record| record.name == name)
            .cloned();
        if let Some(expected) = confirmed.as_ref() {
            if target.as_ref() != Some(expected) {
                self.theme_workflow.message = "Theme changed; request Save again.".into();
                self.theme_workflow.prompt = None;
                return Task::none();
            }
        } else if let Some(target) = target.as_ref() {
            if self.theme_session.designer_loaded_id() != Some(target.id) {
                self.theme_prompt(
                    binding,
                    PromptKind::Overwrite {
                        target: target.clone(),
                        apply,
                        then_load,
                    },
                );
                return Task::none();
            }
        }
        let mut proposed = self.state.clone();
        let id = match proposed.theme_library.save(
            target.map(|record| record.id),
            &name,
            document.clone(),
        ) {
            Ok(id) => id,
            Err(error) => {
                self.theme_workflow.message = error;
                return Task::none();
            }
        };
        if apply {
            proposed.applied_theme = Some(AppliedThemeSnapshot {
                origin: ThemeOrigin::Custom { record_id: id },
                variant: binding.variant,
                document,
            });
        }
        match self.commit_local_state_change(proposed) {
            Ok(_) => {
                self.theme_session.designer_saved(id, name);
                self.theme_workflow.prompt = None;
                self.theme_workflow.message = if apply {
                    "Theme saved and applied."
                } else {
                    "Theme saved to the library. Application appearance is unchanged."
                }
                .into();
                if apply {
                    self.refresh_backdrop();
                }
                if let Some(target) = then_load {
                    return self.complete_theme_departure(target);
                }
            }
            Err(_) => {
                self.theme_workflow.message = self.save_failure.clone().unwrap_or_default();
                // No successful continuation survives a failed publication.
                self.theme_workflow.prompt = None;
                self.theme_workflow.exit = None;
                self.exit_requested = false;
            }
        }
        Task::none()
    }
}

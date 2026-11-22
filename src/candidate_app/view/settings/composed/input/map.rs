//! Narrow source presentation data; no config IO, daemon, mutation, or worker.
use super::action::{ActiveScopes, InputAction};
use super::trigger::{binding_label, parse_binding, KeyId, Mods, MouseSpec, Trigger};

#[derive(Debug, Clone)]
pub struct InputMap {
    bindings: Vec<(InputAction, Vec<Trigger>)>,
    fallbacks: Vec<(InputAction, Vec<Trigger>)>,
    pub diagnostics: Vec<String>,
    pub native_diagnostics: Vec<String>,
    pub reserved: Vec<(String, Vec<Trigger>)>,
}

fn built_in(action: InputAction) -> Vec<Trigger> {
    parse_binding(action.default_binding()).expect("built-in binding must be valid")
}

impl Default for InputMap {
    fn default() -> Self {
        Self {
            bindings: InputAction::ALL
                .into_iter()
                .map(|action| {
                    let triggers = if action.is_config_global() {
                        Vec::new()
                    } else {
                        built_in(action)
                    };
                    (action, triggers)
                })
                .collect(),
            fallbacks: InputAction::ALL
                .into_iter()
                .map(|action| {
                    let triggers = if action.is_config_global() {
                        Vec::new()
                    } else {
                        built_in(action)
                    };
                    (action, triggers)
                })
                .collect(),
            diagnostics: Vec::new(),
            native_diagnostics: Vec::new(),
            reserved: Vec::new(),
        }
    }
}

impl InputMap {
    pub fn from_bindings(bindings: impl IntoIterator<Item = (InputAction, String)>) -> Self {
        let mut map = Self::default();
        map.override_bindings(bindings);
        map
    }

    pub fn remember_fallbacks(&mut self) {
        self.fallbacks = self.bindings.clone();
    }
    pub fn fallback_triggers(&self, action: InputAction) -> &[Trigger] {
        self.fallbacks
            .iter()
            .find(|(a, _)| *a == action)
            .map_or(&[], |(_, triggers)| triggers)
    }
    pub fn reserved_conflicts(&self, triggers: &[Trigger]) -> Vec<String> {
        self.reserved
            .iter()
            .filter(|(_, keys)| keys.iter().any(|key| triggers.contains(key)))
            .map(|(label, _)| label.clone())
            .collect()
    }

    pub fn reserved_conflicts_except(
        &self,
        action: InputAction,
        triggers: &[Trigger],
    ) -> Vec<String> {
        let own_label = action.config_reserved_label();
        self.reserved
            .iter()
            .filter(|(label, keys)| {
                own_label != Some(label.as_str()) && keys.iter().any(|key| triggers.contains(key))
            })
            .map(|(label, _)| label.clone())
            .collect()
    }
    pub fn override_bindings(&mut self, bindings: impl IntoIterator<Item = (InputAction, String)>) {
        for (action, raw) in bindings {
            let triggers = self.parse_action_binding(action, &raw);
            if let Some(slot) = self
                .bindings
                .iter_mut()
                .find(|(existing, _)| *existing == action)
            {
                slot.1 = triggers;
            }
        }
    }

    fn parse_action_binding(&mut self, action: InputAction, raw: &str) -> Vec<Trigger> {
        let mut triggers = parse_binding(raw)
            .filter(|triggers| triggers.len() <= super::trigger::SLOTS)
            .unwrap_or_else(|| {
                self.diagnostics.push(format!(
                    "Invalid {action:?} binding: {raw:?}; action is unbound."
                ));
                Vec::new()
            });
        if !action.accepts_preset_wheel() {
            let count = triggers.len();
            triggers.retain(|trigger| !matches!(trigger, Trigger::Wheel(_)));
            if triggers.len() != count {
                self.diagnostics
                    .push(format!("{action:?} does not accept a wheel shortcut."));
            }
        }
        triggers
    }

    pub fn add_binding(&mut self, action: InputAction, raw: &str) {
        let triggers = self.parse_action_binding(action, raw);
        if let Some(slot) = self
            .bindings
            .iter_mut()
            .find(|(existing, _)| *existing == action)
        {
            slot.1 = triggers;
        } else {
            self.bindings.push((action, triggers));
        }
    }

    pub fn prioritize(&mut self, actions: &[InputAction]) {
        self.bindings.sort_by_key(|(action, _)| {
            actions
                .iter()
                .position(|candidate| candidate == action)
                .unwrap_or(actions.len())
        });
    }

    pub fn triggers(&self, action: InputAction) -> &[Trigger] {
        self.bindings
            .iter()
            .find(|(candidate, _)| *candidate == action)
            .map_or(&[], |(_, triggers)| triggers.as_slice())
    }

    pub fn label(&self, action: InputAction) -> String {
        binding_label(self.triggers(action))
    }

    fn active(&self, scopes: ActiveScopes) -> impl Iterator<Item = &(InputAction, Vec<Trigger>)> {
        self.bindings
            .iter()
            .filter(move |(action, _)| scopes.contains(action.scope()))
    }

    pub fn lookup_key(&self, id: &KeyId, mods: Mods, scopes: ActiveScopes) -> Option<InputAction> {
        let exact = self.active(scopes).find(|(_, triggers)| {
            triggers.iter().any(|trigger| {
                matches!(trigger, Trigger::Key(spec) if spec.id == *id && spec.mods == mods)
            })
        });
        if let Some((action, _)) = exact {
            return Some(*action);
        }
        if !mods.shift {
            return None;
        }
        let relaxed = Mods {
            shift: false,
            ..mods
        };
        self.active(scopes)
            .find(|(_, triggers)| {
                triggers.iter().any(|trigger| {
                    matches!(trigger, Trigger::Key(spec)
                        if spec.id == *id
                            && spec.mods == relaxed
                            && matches!(spec.id, KeyId::Char(_)))
                })
            })
            .map(|(action, _)| *action)
    }

    pub fn lookup_mouse(&self, spec: MouseSpec, scopes: ActiveScopes) -> Option<InputAction> {
        self.active(scopes)
            .find(|(_, triggers)| triggers.contains(&Trigger::Mouse(spec)))
            .map(|(action, _)| *action)
    }

    pub fn lookup_preset_wheel(
        &self,
        spec: super::WheelSpec,
        scopes: ActiveScopes,
    ) -> Option<InputAction> {
        self.active(scopes)
            .find(|(action, triggers)| {
                action.accepts_preset_wheel() && triggers.contains(&Trigger::Wheel(spec))
            })
            .map(|(action, _)| *action)
    }

    pub fn conflicts(&self) -> Vec<(InputAction, InputAction)> {
        let mut conflicts = Vec::new();
        for (index, (left, left_triggers)) in self.bindings.iter().enumerate() {
            for (right, right_triggers) in self.bindings.iter().skip(index + 1) {
                if left.scope().overlaps(right.scope())
                    && left_triggers
                        .iter()
                        .any(|trigger| right_triggers.contains(trigger))
                {
                    conflicts.push((*left, *right));
                }
            }
        }
        conflicts
    }

    pub fn trigger_holders<'a>(
        &'a self,
        action: InputAction,
        trigger: &'a Trigger,
    ) -> impl Iterator<Item = InputAction> + 'a {
        self.bindings
            .iter()
            .filter(move |(candidate, triggers)| {
                *candidate != action
                    && candidate.scope().overlaps(action.scope())
                    && triggers.contains(trigger)
            })
            .map(|(candidate, _)| *candidate)
    }
}

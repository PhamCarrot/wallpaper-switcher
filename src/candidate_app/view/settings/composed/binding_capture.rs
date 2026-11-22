//! Two-slot editor intent. Publishing changes and executing actions stay app-owned.
use super::{
    input::{self, InputAction, InputMap, Trigger},
    KEY_BINDINGS,
};

#[derive(Debug, Clone)]
pub struct Capture {
    pub generation: u64,
    pub path: &'static str,
    pub action: InputAction,
    pub title_key: &'static str,
    pub slot: usize,
    pub triggers: Vec<Trigger>,
    pub edited: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    pub path: &'static str,
    pub action: InputAction,
    pub binding: String,
}

impl Capture {
    pub fn begin(generation: u64, path: &str, slot: usize, map: &InputMap) -> Option<Self> {
        let descriptor = KEY_BINDINGS.into_iter().find(|entry| entry.path == path)?;
        let slot = slot.min(input::SLOTS - 1);
        Some(Self {
            generation,
            path: descriptor.path,
            action: descriptor.action,
            title_key: descriptor.title_key,
            slot,
            triggers: input::slot_triggers(map.triggers(descriptor.action), slot).to_vec(),
            edited: false,
        })
    }
    pub fn replace(&mut self, trigger: Trigger) {
        self.triggers = vec![trigger];
        self.edited = true;
    }
    pub fn unbind(&mut self) {
        self.triggers.clear();
        self.edited = true;
    }
    pub fn restore_default(&mut self) {
        let defaults =
            input::parse_binding(self.action.default_binding()).expect("source default binding");
        self.triggers = input::slot_triggers(&defaults, self.slot).to_vec();
        self.edited = true;
    }
    pub fn conflicts(&self, map: &InputMap) -> Vec<InputAction> {
        self.triggers
            .iter()
            .flat_map(|trigger| map.trigger_holders(self.action, trigger))
            .collect()
    }
    pub fn proposal(&self, current_generation: u64, map: &InputMap) -> Option<Vec<Change>> {
        if self.generation != current_generation
            || !self.edited
            || !map
                .reserved_conflicts_except(self.action, &self.triggers)
                .is_empty()
        {
            return None;
        }
        let replacement = self.triggers.first().cloned();
        let mut changes = Vec::new();
        for other in replacement
            .iter()
            .flat_map(|trigger| map.trigger_holders(self.action, trigger))
        {
            let descriptor = KEY_BINDINGS
                .into_iter()
                .find(|entry| entry.action == other)?;
            let remaining: Vec<_> = map
                .triggers(other)
                .iter()
                .filter(|existing| Some(*existing) != replacement.as_ref())
                .cloned()
                .collect();
            changes.push(Change {
                path: descriptor.path,
                action: other,
                binding: input::binding_config(&remaining),
            });
        }
        let triggers = input::with_slot(map.triggers(self.action), self.slot, replacement);
        let binding = if input::parse_binding(self.action.default_binding()).as_deref()
            == Some(triggers.as_slice())
        {
            String::new()
        } else {
            input::binding_config(&triggers)
        };
        changes.push(Change {
            path: self.path,
            action: self.action,
            binding,
        });
        Some(changes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn path(action: InputAction) -> &'static str {
        KEY_BINDINGS
            .iter()
            .find(|entry| entry.action == action)
            .unwrap()
            .path
    }
    #[test]
    fn edited_slot_preserves_other_slot_and_stale_or_untouched_capture_cannot_publish() {
        let map = InputMap::from_bindings([(InputAction::Apply, "a, ctrl+p".into())]);
        let mut capture = Capture::begin(7, path(InputAction::Apply), 1, &map).unwrap();
        assert!(capture.proposal(7, &map).is_none());
        capture.replace(Trigger::parse("ctrl+o").unwrap());
        assert!(capture.proposal(8, &map).is_none());
        let changes = capture.proposal(7, &map).unwrap();
        let own = changes
            .iter()
            .find(|change| change.action == InputAction::Apply)
            .unwrap();
        let triggers = input::parse_binding(&own.binding).unwrap();
        assert_eq!(triggers, input::parse_binding("a, ctrl+o").unwrap());
        assert!(Capture::begin(7, "invented.key", 0, &map).is_none());
    }
    #[test]
    fn overlap_conflict_proposal_removes_only_the_stolen_trigger() {
        let map = InputMap::from_bindings([
            (InputAction::Apply, "a, ctrl+p".into()),
            (InputAction::Favourite, "f, ctrl+o".into()),
        ]);
        let mut capture = Capture::begin(11, path(InputAction::Apply), 0, &map).unwrap();
        capture.replace(Trigger::parse("f").unwrap());
        assert!(capture.conflicts(&map).contains(&InputAction::Favourite));
        let changes = capture.proposal(11, &map).unwrap();
        let other = changes
            .iter()
            .find(|change| change.action == InputAction::Favourite)
            .unwrap();
        assert_eq!(
            input::parse_binding(&other.binding).unwrap(),
            input::parse_binding("ctrl+o").unwrap()
        );
        let own = changes
            .iter()
            .find(|change| change.action == InputAction::Apply)
            .unwrap();
        assert_eq!(
            input::parse_binding(&own.binding).unwrap(),
            input::parse_binding("f, ctrl+p").unwrap()
        );
        assert_eq!(
            map.label(InputAction::Favourite),
            "F · Ctrl+O",
            "proposal must not mutate live map"
        );
    }
    #[test]
    fn unbind_and_default_are_explicit_slot_proposals() {
        let map = InputMap::default();
        let mut capture = Capture::begin(2, path(InputAction::Apply), 0, &map).unwrap();
        capture.unbind();
        let changes = capture.proposal(2, &map).unwrap();
        let own = changes
            .iter()
            .find(|change| change.action == InputAction::Apply)
            .unwrap();
        assert!(input::parse_binding(&own.binding).unwrap().is_empty());
        capture.restore_default();
        let changes = capture.proposal(2, &map).unwrap();
        assert_eq!(
            changes
                .iter()
                .find(|change| change.action == InputAction::Apply)
                .unwrap()
                .binding,
            ""
        );
    }
}

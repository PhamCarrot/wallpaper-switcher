use super::{schema, Card, Control, Row};
use std::collections::{HashMap, HashSet};

const FINITE_NUMBER_ERROR: &str = "Enter a finite number.";

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Boolean(bool),
    Number(f64),
    Text(String),
}

#[derive(Debug, Default)]
pub struct Draft {
    pub values: HashMap<String, Value>,
    pub raw_inputs: HashMap<String, String>,
    pub errors: HashMap<String, String>,
    empty_numbers: HashSet<String>,
    original_inputs: HashMap<String, Option<Value>>,
    generation: u64,
    active: bool,
}

impl Draft {
    /// Capture the effective value once, before a live editor changes it.
    pub fn begin_input(&mut self, path: &str, effective: Value) {
        self.original_inputs
            .entry(path.to_owned())
            .or_insert(Some(effective));
    }

    /// After an explicit saved-state reload, Escape returns to the reloaded
    /// committed value while retaining the raw edit until the user decides.
    pub fn rebase_input(&mut self, path: &str, effective: Value) {
        if self.raw_inputs.contains_key(path) {
            self.original_inputs
                .insert(path.to_owned(), Some(effective));
        }
    }

    pub fn cancel_input(&mut self, path: &str) -> Option<Value> {
        self.raw_inputs.remove(path);
        self.errors.remove(path);
        self.empty_numbers.remove(path);
        let original = self.original_inputs.remove(path)?;
        match &original {
            Some(value) => {
                self.values.insert(path.to_owned(), value.clone());
            }
            None => {
                self.values.remove(path);
            }
        }
        original
    }

    pub fn validate_input(&mut self, path: &str) -> Result<(), String> {
        if self.empty_numbers.remove(path) {
            self.errors
                .insert(path.to_owned(), FINITE_NUMBER_ERROR.into());
        }
        self.errors
            .get(path)
            .map_or(Ok(()), |error| Err(error.clone()))
    }

    pub fn validate_empty_numbers(&mut self) {
        self.errors.extend(
            self.empty_numbers
                .drain()
                .map(|path| (path, FINITE_NUMBER_ERROR.into())),
        );
    }

    /// A failed field commit leaves its buffer and cancellation snapshot intact.
    pub fn commit_input(&mut self, path: &str) -> Result<Option<Value>, String> {
        self.validate_input(path)?;
        if self.raw_inputs.remove(path).is_none() {
            return Ok(None);
        }
        self.original_inputs.remove(path);
        Ok(self.values.get(path).cloned())
    }
    pub fn bind(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1);
        self.active = true;
        self.generation
    }

    pub fn depart(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.active = false;
    }

    pub fn is_current(&self, generation: u64) -> bool {
        self.active && self.generation == generation
    }

    pub fn input(
        &mut self,
        generation: u64,
        cards: &[(Card, Vec<Row>)],
        path: &str,
        raw: String,
    ) -> Option<Value> {
        if !self.is_current(generation) {
            return None;
        }
        let control = find_control(cards, path)?;
        self.original_inputs
            .entry(path.to_owned())
            .or_insert_with(|| self.values.get(path).cloned());
        self.raw_inputs.insert(path.to_owned(), raw.clone());
        self.empty_numbers.remove(path);
        let parsed = match control {
            Control::Number { .. } | Control::MotionWeights { .. } if raw.trim().is_empty() => {
                self.errors.remove(path);
                self.empty_numbers.insert(path.to_owned());
                return None;
            }
            Control::Number { .. } | Control::MotionWeights { .. } => raw
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|value| value.is_finite())
                .map(|value| {
                    let value = schema::find(path)
                        .and_then(|spec| spec.number_bounds)
                        .map_or(value, |bounds| value.clamp(bounds.min, bounds.max));
                    Value::Number(value)
                }),
            Control::TextField { .. } => Some(Value::Text(raw)),
            Control::Resolution { .. }
                if (path.ends_with(".to") && raw.trim().is_empty())
                    || super::catalog::parse_resolution(&raw).is_some() =>
            {
                Some(Value::Text(raw))
            }
            _ => None,
        };
        match parsed {
            Some(value) => {
                self.errors.remove(path);
                self.values.insert(path.to_owned(), value.clone());
                Some(value)
            }
            None => {
                let message = if matches!(control, Control::Resolution { .. }) {
                    "Enter positive width × height."
                } else {
                    FINITE_NUMBER_ERROR
                };
                self.errors.insert(path.to_owned(), message.into());
                None
            }
        }
    }

    pub fn choose(
        &mut self,
        generation: u64,
        cards: &[(Card, Vec<Row>)],
        path: &str,
        value: Value,
    ) -> Option<Value> {
        if !self.is_current(generation) {
            return None;
        }
        let control = find_control(cards, path)?;
        let valid = match (control, &value) {
            (Control::Toggle { .. }, Value::Boolean(_)) => true,
            (Control::Dropdown { options, .. }, Value::Text(text)) => {
                options.iter().any(|(id, _)| id == text)
            }
            (
                Control::Chips {
                    options, disabled, ..
                },
                Value::Text(text),
            ) => options.iter().any(|(id, _)| id == text) && !disabled.contains(text),
            _ => false,
        };
        if !valid {
            return None;
        }
        self.errors.remove(path);
        self.values.insert(path.to_owned(), value.clone());
        Some(value)
    }
}

fn row_control<'a>(rows: &'a [Row], path: &str) -> Option<&'a Control> {
    for row in rows {
        let matches = match &row.control {
            Control::Toggle { path: key, .. }
            | Control::Number { path: key, .. }
            | Control::TextField { path: key, .. }
            | Control::Resolution { path: key, .. }
            | Control::Dropdown { path: key, .. }
            | Control::Chips { path: key, .. }
            | Control::KeyBinding { path: key, .. } => key == path,
            Control::MotionWeights { weights } => weights.iter().any(|(_, key, _)| key == path),
            _ => false,
        };
        if matches {
            return Some(&row.control);
        }
        if let Control::Details { rows, .. } | Control::StackBar { rows, .. } = &row.control {
            if let Some(found) = row_control(rows, path) {
                return Some(found);
            }
        }
    }
    None
}
fn find_control<'a>(cards: &'a [(Card, Vec<Row>)], path: &str) -> Option<&'a Control> {
    cards.iter().find_map(|(_, rows)| row_control(rows, path))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fields() -> Vec<(Card, Vec<Row>)> {
        vec![(
            Card {
                title: "Fixture",
                subtitle: "",
            },
            vec![
                Row {
                    title: "Scale".into(),
                    desc: "".into(),
                    control: Control::Number {
                        key: "general.uiScale".into(),
                        path: "general.uiScale".into(),
                        unit: "x",
                    },
                },
                Row {
                    title: "Preference".into(),
                    desc: "".into(),
                    control: Control::Toggle {
                        path: "test.visiblePreference".into(),
                        value: false,
                    },
                },
                Row {
                    title: "Choice".into(),
                    desc: "".into(),
                    control: Control::Chips {
                        path: "test.choice".into(),
                        options: vec![
                            ("allowed".into(), "Allowed".into()),
                            ("unavailable".into(), "Unavailable".into()),
                        ],
                        current: "allowed".into(),
                        disabled: vec!["unavailable".into()],
                        background: None,
                    },
                },
            ],
        )]
    }
    #[test]
    fn cancel_restores_edit_start_after_valid_then_invalid_text_and_failed_commit() {
        let mut draft = Draft::default();
        let token = draft.bind();
        let fields = fields();
        draft.begin_input("general.uiScale", Value::Number(1.0));
        draft.input(token, &fields, "general.uiScale", "1.4".into());
        // Repeated typing must not replace the original effective value.
        draft.begin_input("general.uiScale", Value::Number(1.4));
        draft.input(token, &fields, "general.uiScale", "-".into());
        assert!(draft.commit_input("general.uiScale").is_err());
        assert_eq!(draft.raw_inputs["general.uiScale"], "-");
        assert_eq!(
            draft.cancel_input("general.uiScale"),
            Some(Value::Number(1.0))
        );
        assert_eq!(draft.values["general.uiScale"], Value::Number(1.0));
        assert!(draft.errors.is_empty());
        assert!(draft.raw_inputs.is_empty());
    }

    #[test]
    fn field_commit_keeps_other_editor_buffers_and_advances_next_edit_baseline() {
        let mut draft = Draft::default();
        let token = draft.bind();
        let fields = fields();
        draft.begin_input("general.uiScale", Value::Number(1.0));
        draft.input(token, &fields, "general.uiScale", "1.4".into());
        // An unrelated editor buffer cannot be consumed by this field's Enter.
        draft.raw_inputs.insert("other".into(), "unfinished".into());
        assert_eq!(
            draft.commit_input("general.uiScale"),
            Ok(Some(Value::Number(1.4)))
        );
        assert_eq!(draft.raw_inputs["other"], "unfinished");
        draft.begin_input("general.uiScale", Value::Number(1.4));
        draft.input(token, &fields, "general.uiScale", "1.7".into());
        assert_eq!(
            draft.cancel_input("general.uiScale"),
            Some(Value::Number(1.4))
        );
    }

    #[test]
    fn raw_invalid_edits_remain_visible_without_replacing_last_valid_local_value() {
        let mut draft = Draft::default();
        let token = draft.bind();
        let fields = fields();
        assert_eq!(
            draft.input(token, &fields, "general.uiScale", "1.4".into()),
            Some(Value::Number(1.4))
        );
        assert_eq!(
            draft.input(token, &fields, "general.uiScale", "".into()),
            None
        );
        assert_eq!(draft.raw_inputs["general.uiScale"], "");
        assert_eq!(draft.values["general.uiScale"], Value::Number(1.4));
        assert!(draft.errors.is_empty());
        assert_eq!(
            draft.commit_input("general.uiScale"),
            Err(FINITE_NUMBER_ERROR.into())
        );
        assert_eq!(draft.raw_inputs["general.uiScale"], "");
        for raw in ["NaN", "inf", "-inf", "1e9999"] {
            assert_eq!(
                draft.input(token, &fields, "general.uiScale", raw.into()),
                None
            );
            assert_eq!(draft.values["general.uiScale"], Value::Number(1.4));
        }
        assert_eq!(
            draft.input(token, &fields, "general.uiScale", "99".into()),
            Some(Value::Number(2.0))
        );
        assert_eq!(draft.raw_inputs["general.uiScale"], "99");
    }
    #[test]
    fn hidden_wrong_kind_unknown_and_expired_callbacks_cannot_edit_drafts() {
        let mut draft = Draft::default();
        let token = draft.bind();
        let fields = fields();
        assert_eq!(
            draft.choose(
                token,
                &fields,
                "test.visiblePreference",
                Value::Text("true".into())
            ),
            None
        );
        assert_eq!(
            draft.choose(
                token,
                &fields,
                "test.choice",
                Value::Text("unavailable".into())
            ),
            None
        );
        assert_eq!(
            draft.choose(
                token,
                &fields,
                "test.choice",
                Value::Text("invented".into())
            ),
            None
        );
        assert_eq!(
            draft.choose(token, &fields, "unknown", Value::Boolean(true)),
            None
        );
        assert_eq!(
            draft.input(token, &[], "general.uiScale", "1.5".into()),
            None
        );
        assert!(draft.values.is_empty());
        draft.depart();
        assert_eq!(
            draft.choose(
                token,
                &fields,
                "test.visiblePreference",
                Value::Boolean(true)
            ),
            None
        );
        let reopened = draft.bind();
        assert_ne!(token, reopened);
        assert_eq!(
            draft.choose(
                token,
                &fields,
                "test.visiblePreference",
                Value::Boolean(true)
            ),
            None
        );
        assert_eq!(
            draft.choose(
                reopened,
                &fields,
                "test.visiblePreference",
                Value::Boolean(true)
            ),
            Some(Value::Boolean(true))
        );
    }
    #[test]
    fn resolution_and_group_fields_require_their_actual_visible_shape() {
        let mut draft = Draft::default();
        let token = draft.bind();
        let root = super::super::keys::filter_bar::RESOLUTION_PRESETS;
        let from = format!("{root}.0.from");
        let to = format!("{root}.0.to");
        let cards = vec![(
            Card {
                title: "Compound",
                subtitle: "",
            },
            vec![
                Row {
                    title: "Lower".into(),
                    desc: "".into(),
                    control: Control::Resolution {
                        key: from.clone(),
                        path: from.clone(),
                        width_placeholder: "",
                        height_placeholder: "",
                    },
                },
                Row {
                    title: "Upper".into(),
                    desc: "".into(),
                    control: Control::Resolution {
                        key: to.clone(),
                        path: to.clone(),
                        width_placeholder: "",
                        height_placeholder: "",
                    },
                },
                Row {
                    title: "Speeds".into(),
                    desc: "".into(),
                    control: Control::MotionWeights {
                        weights: vec![(
                            "Fast".into(),
                            "motion.fastMs".into(),
                            super::super::ActionId::ResetMotionFast,
                        )],
                    },
                },
            ],
        )];
        assert!(draft
            .input(token, &cards, &from, "1920×1080".into())
            .is_some());
        assert!(draft
            .input(token, &cards, &from, "arbitrary".into())
            .is_none());
        assert_eq!(draft.values[&from], Value::Text("1920×1080".into()));
        assert!(draft.input(token, &cards, &from, "".into()).is_none());
        assert_eq!(
            draft.input(token, &cards, &to, "".into()),
            Some(Value::Text("".into()))
        );
        assert_eq!(
            draft.input(token, &cards, "motion.fastMs", "100".into()),
            Some(Value::Number(100.0))
        );
        assert_eq!(
            draft.input(token, &cards, "motion.fastMs", "NaN".into()),
            None
        );
        assert_eq!(
            draft.input(token, &cards, "motion.slowMs", "100".into()),
            None
        );
    }
}

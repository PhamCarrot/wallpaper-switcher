//! Local Folio focus state; the app reducer owns commits and departure.
use super::{Card, Control, Row, SettingsFocus, SettingsKey};
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    None,
    CommitInput(String),
    CancelInput(String),
    CloseSearch,
    CloseSettings,
    ActivateSearchResult,
    FocusInput(String),
    FocusResolution { path: String, height: bool },
    Category(usize),
    Activate { row: usize, choice: Option<usize> },
}

#[derive(Debug, Clone)]
pub struct Navigation {
    pub category: usize,
    pub section: usize,
    pub row: usize,
    pub focus: SettingsFocus,
    pub choice: Option<usize>,
    pub search_open: bool,
    pub input_edit: Option<String>,
    pub resolution_height: bool,
    pub expanded: HashSet<String>,
    pub open_bars: HashSet<String>,
}

impl Default for Navigation {
    fn default() -> Self {
        Self {
            category: 0,
            section: 0,
            row: 0,
            focus: SettingsFocus::Index,
            choice: None,
            search_open: false,
            input_edit: None,
            resolution_height: false,
            expanded: HashSet::new(),
            open_bars: HashSet::new(),
        }
    }
}

impl Navigation {
    pub fn select_section(&mut self, section: usize, cards: &[(Card, Vec<Row>)]) {
        self.section = section.min(cards.len().saturating_sub(1));
        self.row = 0;
        self.choice = None;
        self.focus = SettingsFocus::Sections;
    }
    pub fn select_search_result(
        &mut self,
        section: usize,
        row: usize,
        cards: &[(Card, Vec<Row>)],
    ) -> bool {
        let Some((_, rows)) = cards.get(section) else {
            return false;
        };
        let Some(setting) = rows.get(row) else {
            return false;
        };
        if !setting.control.is_focusable() {
            return false;
        }
        self.section = section;
        self.row = row;
        self.focus = SettingsFocus::Controls;
        self.choice = None;
        self.search_open = false;
        if let Control::Details { id, .. } = &setting.control {
            self.expanded.insert(id.clone());
        }
        if let Some(id) = setting.control.compact_bar_id() {
            self.open_bars.insert(id);
        }
        true
    }
    pub fn key(
        &mut self,
        key: SettingsKey,
        cards: &[(Card, Vec<Row>)],
        category_count: usize,
    ) -> Vec<Effect> {
        let rows = cards
            .get(self.section)
            .map_or(&[][..], |(_, rows)| rows.as_slice());
        let mut effects = Vec::new();
        match key {
            SettingsKey::Cancel => {
                if self.search_open {
                    self.search_open = false;
                    effects.push(Effect::CloseSearch);
                } else if let Some(path) = self.input_edit.take() {
                    effects.push(Effect::CancelInput(path));
                } else if self.choice.take().is_some() {
                } else {
                    effects.push(Effect::CloseSettings);
                }
            }
            SettingsKey::Search => {
                if let Some(path) = self.input_edit.take() {
                    effects.push(Effect::CommitInput(path));
                }
                self.choice = None;
                self.search_open = true;
                self.focus = SettingsFocus::Index;
            }
            SettingsKey::CategoryNext { backwards } => {
                if let Some(path) = self.input_edit.take() {
                    effects.push(Effect::CommitInput(path));
                }
                self.search_open = false;
                self.choice = None;
                self.focus = SettingsFocus::Index;
                if category_count > 0 {
                    self.category = if backwards {
                        (self.category + category_count - 1) % category_count
                    } else {
                        (self.category + 1) % category_count
                    };
                    self.section = 0;
                    self.row = 0;
                    effects.push(Effect::Category(self.category));
                }
            }
            SettingsKey::FocusNext { backwards } => {
                if let Some(path) = self.input_edit.clone() {
                    if path.starts_with(super::keys::filter_bar::RESOLUTION_PRESETS)
                        && (path.ends_with(".from") || path.ends_with(".to"))
                        && self.resolution_height == backwards
                    {
                        self.resolution_height = !backwards;
                        return vec![Effect::FocusResolution {
                            path,
                            height: self.resolution_height,
                        }];
                    }
                    if let Some(Row {
                        control: Control::MotionWeights { weights },
                        ..
                    }) = rows.get(self.row)
                    {
                        if let Some(current) = weights.iter().position(|(_, key, _)| key == &path) {
                            let next = if backwards {
                                current.checked_sub(1)
                            } else {
                                Some(current + 1)
                            };
                            if let Some((_, next_key, _)) =
                                next.and_then(|index| weights.get(index))
                            {
                                self.input_edit = Some(next_key.clone());
                                return vec![
                                    Effect::CommitInput(path),
                                    Effect::FocusInput(next_key.clone()),
                                ];
                            }
                        }
                    }
                }
                if let Some(path) = self.input_edit.take() {
                    effects.push(Effect::CommitInput(path));
                }
                self.search_open = false;
                self.choice = None;
                self.tab(rows, backwards);
            }
            SettingsKey::Up | SettingsKey::Down | SettingsKey::Previous | SettingsKey::Next => {
                if self.search_open {
                    return vec![];
                }
                match self.focus {
                    SettingsFocus::Index => match key {
                        SettingsKey::Up | SettingsKey::Down if category_count > 0 => {
                            let next = if matches!(key, SettingsKey::Down) {
                                (self.category + 1).min(category_count - 1)
                            } else {
                                self.category.saturating_sub(1)
                            };
                            if next != self.category {
                                self.category = next;
                                self.section = 0;
                                self.row = 0;
                                effects.push(Effect::Category(self.category));
                            }
                        }
                        SettingsKey::Next => self.focus = SettingsFocus::Sections,
                        _ => {}
                    },
                    SettingsFocus::Sections => match key {
                        SettingsKey::Up | SettingsKey::Down => {
                            let section = if matches!(key, SettingsKey::Down) {
                                self.section.saturating_add(1)
                            } else {
                                self.section.saturating_sub(1)
                            };
                            self.select_section(section, cards);
                        }
                        SettingsKey::Previous => self.focus = SettingsFocus::Index,
                        SettingsKey::Next => {
                            if let Some(row) = focusable(rows).first() {
                                self.row = *row;
                                self.focus = SettingsFocus::Controls;
                            }
                        }
                        _ => {}
                    },
                    SettingsFocus::Controls => {
                        let Some(row) = rows.get(self.row) else {
                            return effects;
                        };
                        if let Some(choice) = self.choice {
                            match key {
                                SettingsKey::Previous => self.choice = None,
                                SettingsKey::Next => effects.push(Effect::Activate {
                                    row: self.row,
                                    choice: Some(choice),
                                }),
                                SettingsKey::Up | SettingsKey::Down => {
                                    self.choice = next_choice(
                                        &row.control,
                                        choice,
                                        matches!(key, SettingsKey::Down),
                                    )
                                }
                                _ => {}
                            }
                        } else {
                            match key {
                                SettingsKey::Previous => self.focus = SettingsFocus::Sections,
                                SettingsKey::Up | SettingsKey::Down => {
                                    let positions = focusable(rows);
                                    if let Some(index) =
                                        positions.iter().position(|row| *row == self.row)
                                    {
                                        let next = if matches!(key, SettingsKey::Down) {
                                            (index + 1).min(positions.len() - 1)
                                        } else {
                                            index.saturating_sub(1)
                                        };
                                        self.row = positions[next];
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            SettingsKey::Activate => {
                if self.search_open {
                    effects.push(Effect::ActivateSearchResult);
                } else if let Some(path) = self.input_edit.take() {
                    effects.push(Effect::CommitInput(path));
                } else {
                    match self.focus {
                        SettingsFocus::Index => self.focus = SettingsFocus::Sections,
                        SettingsFocus::Sections => {
                            if let Some(row) = focusable(rows).first() {
                                self.row = *row;
                                self.focus = SettingsFocus::Controls;
                            }
                        }
                        SettingsFocus::Controls => effects.push(Effect::Activate {
                            row: self.row,
                            choice: self.choice,
                        }),
                    }
                }
            }
        }
        effects
    }
    fn tab(&mut self, rows: &[Row], backwards: bool) {
        let positions = focusable(rows);
        match self.focus {
            SettingsFocus::Index if backwards => {
                if let Some(row) = positions.last() {
                    self.row = *row;
                    self.focus = SettingsFocus::Controls;
                } else {
                    self.focus = SettingsFocus::Sections;
                }
            }
            SettingsFocus::Index => self.focus = SettingsFocus::Sections,
            SettingsFocus::Sections if backwards => self.focus = SettingsFocus::Index,
            SettingsFocus::Sections => {
                if let Some(row) = positions.first() {
                    self.row = *row;
                    self.focus = SettingsFocus::Controls;
                } else {
                    self.focus = SettingsFocus::Index;
                }
            }
            SettingsFocus::Controls => {
                let current = positions
                    .iter()
                    .position(|row| *row == self.row)
                    .unwrap_or(0);
                let next = if backwards {
                    current.checked_sub(1)
                } else {
                    Some(current + 1)
                };
                if let Some(row) = next.and_then(|index| positions.get(index)) {
                    self.row = *row;
                } else {
                    self.focus = if backwards {
                        SettingsFocus::Sections
                    } else {
                        SettingsFocus::Index
                    };
                }
            }
        }
    }
}
fn focusable(rows: &[Row]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter(|(_, row)| row.control.is_focusable())
        .map(|(index, _)| index)
        .collect()
}
fn choice_count(control: &Control) -> usize {
    match control {
        Control::Dropdown { options, .. } | Control::Chips { options, .. } => options.len(),
        Control::ActionChips { items } => items.len(),
        Control::KeyBinding { .. } => super::input::SLOTS,
        Control::Presets { items, .. } => super::profile_choices(items).len(),
        _ => 0,
    }
}
fn choice_enabled(control: &Control, index: usize) -> bool {
    match control {
        Control::Chips {
            options, disabled, ..
        } => options
            .get(index)
            .is_some_and(|(id, _)| !disabled.contains(id)),
        Control::Presets { items, .. } => match super::profile_choices(items).get(index) {
            Some(super::ProfileChoice::Apply(id)) => {
                items.iter().any(|entry| entry.id == *id && entry.available)
            }
            Some(_) => true,
            None => false,
        },
        _ => index < choice_count(control),
    }
}
fn next_choice(control: &Control, current: usize, forwards: bool) -> Option<usize> {
    let count = choice_count(control);
    (1..=count)
        .map(|step| {
            if forwards {
                (current + step) % count
            } else {
                (current + count - step % count) % count
            }
        })
        .find(|index| choice_enabled(control, *index))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cards() -> Vec<(Card, Vec<Row>)> {
        vec![(
            Card {
                title: "Section",
                subtitle: "",
            },
            vec![
                Row {
                    title: "Segment".into(),
                    desc: "".into(),
                    control: Control::Segment,
                },
                Row {
                    title: "Field".into(),
                    desc: "".into(),
                    control: Control::TextField {
                        key: "field".into(),
                        path: "field".into(),
                        placeholder: "",
                    },
                },
                Row {
                    title: "Choice".into(),
                    desc: "".into(),
                    control: Control::Chips {
                        path: "choice".into(),
                        options: vec![
                            ("a".into(), "A".into()),
                            ("b".into(), "B".into()),
                            ("c".into(), "C".into()),
                        ],
                        disabled: vec!["b".into()],
                        current: "a".into(),
                        background: None,
                    },
                },
                Row {
                    title: "Nested".into(),
                    desc: "".into(),
                    control: Control::Details {
                        id: "nested".into(),
                        summary: "".into(),
                        rows: vec![],
                    },
                },
            ],
        )]
    }
    #[test]
    fn cancel_resolves_search_edit_choice_before_settings_departure() {
        let mut nav = Navigation::default();
        let cards = cards();
        nav.search_open = true;
        nav.input_edit = Some("field".into());
        nav.choice = Some(0);
        assert_eq!(
            nav.key(SettingsKey::Cancel, &cards, 13),
            vec![Effect::CloseSearch]
        );
        assert_eq!(nav.input_edit.as_deref(), Some("field"));
        assert_eq!(
            nav.key(SettingsKey::Cancel, &cards, 13),
            vec![Effect::CancelInput("field".into())]
        );
        assert!(nav.key(SettingsKey::Cancel, &cards, 13).is_empty());
        assert_eq!(
            nav.key(SettingsKey::Cancel, &cards, 13),
            vec![Effect::CloseSettings]
        );
    }
    #[test]
    fn tab_and_arrow_focus_use_source_groups_and_skip_disabled_choices() {
        let mut nav = Navigation::default();
        let cards = cards();
        nav.key(SettingsKey::FocusNext { backwards: false }, &cards, 13);
        assert_eq!(nav.focus, SettingsFocus::Sections);
        nav.key(SettingsKey::FocusNext { backwards: false }, &cards, 13);
        assert_eq!(nav.row, 1);
        nav.key(SettingsKey::FocusNext { backwards: false }, &cards, 13);
        assert_eq!(nav.row, 2);
        nav.choice = Some(0);
        nav.key(SettingsKey::Down, &cards, 13);
        assert_eq!(nav.choice, Some(2));
        nav.key(SettingsKey::Previous, &cards, 13);
        assert_eq!(nav.choice, None);
        nav.key(SettingsKey::Previous, &cards, 13);
        assert_eq!(nav.focus, SettingsFocus::Sections);
        nav.key(SettingsKey::CategoryNext { backwards: true }, &cards, 13);
        assert_eq!(nav.category, 12);
        nav.key(SettingsKey::Down, &cards, 13);
        assert_eq!(nav.category, 12);
    }
    #[test]
    fn search_targets_expand_actual_parent_and_stale_indices_are_rejected() {
        let mut nav = Navigation::default();
        let cards = cards();
        nav.search_open = true;
        assert!(!nav.select_search_result(0, 99, &cards));
        assert!(nav.search_open);
        assert!(!nav.select_search_result(0, 0, &cards));
        assert!(nav.select_search_result(0, 3, &cards));
        assert_eq!(nav.focus, SettingsFocus::Controls);
        assert_eq!(nav.row, 3);
        assert!(nav.expanded.contains("nested"));
        assert!(!nav.search_open);
        nav.input_edit = Some("uncommitted".into());
        assert!(nav.select_search_result(0, 1, &cards));
        assert_eq!(
            nav.input_edit.as_deref(),
            Some("uncommitted"),
            "the app bridge must commit before navigation; never silently discard it here"
        );
    }
    #[test]
    fn clamped_category_arrows_preserve_section_and_enter_search_requests_first_result() {
        let mut nav = Navigation::default();
        let cards = cards();
        nav.category = 12;
        nav.section = 4;
        nav.row = 2;
        assert!(nav.key(SettingsKey::Down, &cards, 13).is_empty());
        assert_eq!((nav.section, nav.row), (4, 2));
        nav.category = 0;
        assert!(nav.key(SettingsKey::Up, &cards, 13).is_empty());
        assert_eq!((nav.section, nav.row), (4, 2));
        nav.search_open = true;
        assert_eq!(
            nav.key(SettingsKey::Activate, &cards, 13),
            vec![Effect::ActivateSearchResult]
        );
    }
    #[test]
    fn tab_stays_within_resolution_pair_and_shared_motion_group_before_leaving_control() {
        let mut nav = Navigation::default();
        let mut cards = cards();
        nav.focus = SettingsFocus::Controls;
        nav.row = 1;
        let path = format!(
            "{}.0.from",
            super::super::keys::filter_bar::RESOLUTION_PRESETS
        );
        nav.input_edit = Some(path.clone());
        assert_eq!(
            nav.key(SettingsKey::FocusNext { backwards: false }, &cards, 13),
            vec![Effect::FocusResolution {
                path: path.clone(),
                height: true
            }]
        );
        assert_eq!(nav.input_edit.as_deref(), Some(path.as_str()));
        assert_eq!(
            nav.key(SettingsKey::FocusNext { backwards: true }, &cards, 13),
            vec![Effect::FocusResolution {
                path: path.clone(),
                height: false
            }]
        );
        cards[0].1[1].control = Control::MotionWeights {
            weights: vec![
                (
                    "Fast".into(),
                    "motion.fastMs".into(),
                    super::super::ActionId::ResetMotionFast,
                ),
                (
                    "Standard".into(),
                    "motion.standardMs".into(),
                    super::super::ActionId::ResetMotionStandard,
                ),
                (
                    "Slow".into(),
                    "motion.slowMs".into(),
                    super::super::ActionId::ResetMotionSlow,
                ),
            ],
        };
        nav.input_edit = Some("motion.fastMs".into());
        assert_eq!(
            nav.key(SettingsKey::FocusNext { backwards: false }, &cards, 13),
            vec![
                Effect::CommitInput("motion.fastMs".into()),
                Effect::FocusInput("motion.standardMs".into())
            ]
        );
        assert_eq!(nav.input_edit.as_deref(), Some("motion.standardMs"));
        nav.key(SettingsKey::FocusNext { backwards: false }, &cards, 13);
        let leave = nav.key(SettingsKey::FocusNext { backwards: false }, &cards, 13);
        assert_eq!(leave, vec![Effect::CommitInput("motion.slowMs".into())]);
        assert_eq!(nav.row, 2);
        assert!(nav.input_edit.is_none());
    }
}

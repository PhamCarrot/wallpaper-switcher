//! Owned local forms. All publication remains pending through the existing saver.
use crate::candidate_app::*;
use crate::candidate_state::frontend::DeferredCapability;
use crate::candidate_state::pending::{
    forms::{self, BrowserForm},
    rules::{self, Node, Rule},
};
use iced::widget::{button, checkbox, column, pick_list, row, text, text_input, Space};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub enum Action {
    AddRule,
    RemoveRule(usize),
    MoveRule(usize, isize),
    RuleText(usize, String, String),
    RuleEnabled(usize, bool),
    RuleMode(usize, String),
    SaveRules,
    NewKind(String),
    AddNode(usize, Vec<usize>, Option<bool>),
    RemoveNode(usize, Vec<usize>),
    ResetNode(usize, Vec<usize>),
    Negate(usize, Vec<usize>, bool),
    GroupMode(usize, Vec<usize>, String),
    NodeText(usize, Vec<usize>, String, String),
    NodeCommit(usize, Vec<usize>),
    NodeChoice(usize, Vec<usize>, String, String),
    NodeChip(usize, Vec<usize>, String),
    BrowserSource(String),
    BrowserText(String, String),
    BrowserChoice(String, String),
    BrowserToggle(String, bool),
    BrowserChip(String, String),
    SaveBrowser,
    SaveWallpaperProfile,
    ProfileText(usize, String),
    ProfileEnabled(usize, bool),
    ProfilePinned(usize, bool),
    DeleteProfile(usize),
    SaveProfiles,
}
#[derive(Debug)]
pub struct Session {
    pub rules: Vec<Rule>,
    pub rules_dirty: bool,
    pub kind: String,
    pub node_inputs: BTreeMap<(usize, Vec<usize>, String), String>,
    pub browsers: Vec<BrowserForm>,
    pub browsers_dirty: bool,
    pub source: String,
    pub profiles: Vec<Value>,
    pub profiles_dirty: bool,
    pub errors: BTreeMap<String, String>,
    pub message: String,
    baselines: BTreeMap<String, Option<Value>>,
}
impl Default for Session {
    fn default() -> Self {
        Self {
            rules: Vec::new(),
            rules_dirty: false,
            kind: "Weekday".into(),
            node_inputs: BTreeMap::new(),
            browsers: forms::SOURCES.iter().map(|s| BrowserForm::new(s)).collect(),
            browsers_dirty: false,
            source: "wallhaven".into(),
            profiles: Vec::new(),
            profiles_dirty: false,
            errors: BTreeMap::new(),
            message: String::new(),
            baselines: BTreeMap::new(),
        }
    }
}
impl Session {
    pub fn from_state(state: &CandidateState) -> Self {
        let mut session = Self::default();
        for form in &mut session.browsers {
            let read_text = |path: &str| match super::pending_projection::read(state, path) {
                Some(super::composed::draft::Value::Text(v)) => Some(v),
                _ => None,
            };
            if form.source == "wallhaven" {
                for (field, path) in [
                    (&mut form.sort, "wallhaven.defaults.sort"),
                    (&mut form.range, "wallhaven.defaults.topRange"),
                    (&mut form.minimum, "wallhaven.defaults.atleast"),
                    (&mut form.maximum, "wallhaven.defaults.atmost"),
                ] {
                    if let Some(value) = read_text(path) {
                        *field = value;
                    }
                }
                if let Some(value) = read_text("wallhaven.defaults.ratios") {
                    form.ratios = split(&value);
                }
                for (field, choices) in [
                    (
                        &mut form.categories,
                        ["general", "anime", "people"].as_slice(),
                    ),
                    (&mut form.purity, ["sfw", "sketchy", "nsfw"].as_slice()),
                ] {
                    for key in choices {
                        if let Some(super::composed::draft::Value::Boolean(value)) =
                            super::pending_projection::read(
                                state,
                                &format!("wallhaven.defaults.{key}"),
                            )
                        {
                            field.retain(|entry| entry != key);
                            if value {
                                field.push(key.to_string());
                            }
                        }
                    }
                    if field.is_empty() {
                        field.push(choices[0].to_string());
                    }
                }
            } else if form.source == "steam" {
                for (field, path) in [
                    (&mut form.sort, "steam.defaults.sort"),
                    (&mut form.range, "steam.defaults.trendDays"),
                    (&mut form.steam_kind, "steam.defaults.type"),
                    (&mut form.steam_resolution, "steam.defaults.resolution"),
                    (&mut form.steam_category, "steam.defaults.category"),
                ] {
                    if let Some(value) = read_text(path) {
                        *field = value;
                    }
                }
            }
        }
        for (path, capability) in [
            ("schedule.rules", DeferredCapability::ExpandedScheduling),
            ("browser.forms", DeferredCapability::SourceBrowsing),
            (
                "theme.wallpaperProfiles",
                DeferredCapability::WallpaperBoundProfiles,
            ),
        ] {
            session
                .baselines
                .insert(path.into(), saved_root(state, path));
            let Some(record) = state
                .pending_frontend
                .settings
                .iter()
                .find(|e| e.source_path == path && e.capability == capability)
            else {
                continue;
            };
            let result = (|| -> Result<(), String> {
                let value: Value = serde_json::from_str(&record.value_json)
                    .map_err(|_| "Stored pending form is invalid; raw data retained.")?;
                match path {
                    "schedule.rules" => {
                        rules::validate_records(&value)?;
                        session.rules = serde_json::from_value(value).map_err(|_| {
                            "Stored rules cannot be edited; original records retained."
                        })?
                    }
                    "browser.forms" => {
                        forms::validate_browsers(&value)?;
                        session.browsers =
                            serde_json::from_value(value).map_err(|_| "Invalid browser forms.")?;
                    }
                    _ => {
                        forms::validate_profiles(&value)?;
                        session.profiles = value.as_array().ok_or("Invalid profiles.")?.clone();
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                session.errors.insert(path.into(), error);
            }
        }
        session
    }
    fn dirty(&self, path: &str) -> bool {
        match path {
            "schedule.rules" => self.rules_dirty || !self.node_inputs.is_empty(),
            "browser.forms" => self.browsers_dirty,
            _ => self.profiles_dirty,
        }
    }
    fn value(&self, path: &str) -> Result<Value, String> {
        match path {
            "schedule.rules" => serde_json::to_value(&self.rules),
            "browser.forms" => serde_json::to_value(&self.browsers),
            _ => Ok(Value::Array(self.profiles.clone())),
        }
        .map_err(|e| e.to_string())
    }
    pub(in crate::candidate_app) fn check_reload(
        &self,
        state: &CandidateState,
    ) -> Result<(), String> {
        for path in ROOTS {
            let next = saved_root(state, path);
            if path == "schedule.rules"
                && !self.node_inputs.is_empty()
                && next != self.baselines.get(path).cloned().flatten()
            {
                return Err("Saved schedule.rules changed while a newer raw condition draft is retained. Reconcile that draft before reloading; nothing was discarded.".into());
            }
            if self.dirty(path)
                && next != self.baselines.get(path).cloned().flatten()
                && next.as_ref() != Some(&self.value(path)?)
            {
                return Err(format!("Saved {path} has newer records that conflict with the retained local form. Draft, files and save protection were retained; reconcile the records explicitly."));
            }
        }
        Ok(())
    }
    pub(in crate::candidate_app) fn rebase_after_reload(&mut self, state: &CandidateState) {
        let mut restored = Self::from_state(state);
        for path in ROOTS {
            let next = saved_root(state, path);
            let retained = self.dirty(path) && next == self.baselines.get(path).cloned().flatten();
            if !retained {
                match path {
                    "schedule.rules" => {
                        self.rules = std::mem::take(&mut restored.rules);
                        self.rules_dirty = false;
                        self.node_inputs.clear();
                    }
                    "browser.forms" => {
                        self.browsers = std::mem::take(&mut restored.browsers);
                        self.browsers_dirty = false;
                    }
                    _ => {
                        self.profiles = std::mem::take(&mut restored.profiles);
                        self.profiles_dirty = false;
                    }
                }
                self.errors.remove(path);
                if let Some(error) = restored.errors.remove(path) {
                    self.errors.insert(path.into(), error);
                }
            }
            self.baselines.insert(path.into(), next);
        }
    }
    fn browser(&self) -> Option<&BrowserForm> {
        self.browsers.iter().find(|b| b.source == self.source)
    }
    fn browser_mut(&mut self) -> Option<&mut BrowserForm> {
        self.browsers.iter_mut().find(|b| b.source == self.source)
    }
    fn node_text(&self, index: usize, path: &[usize], slot: &str, fallback: &str) -> String {
        self.node_inputs
            .get(&(index, path.to_vec(), slot.into()))
            .cloned()
            .unwrap_or_else(|| fallback.into())
    }
    fn commit_node(&mut self, index: usize, path: &[usize]) -> Result<(), String> {
        let node = self
            .rules
            .get_mut(index)
            .and_then(|r| r.condition.root.at_mut(path))
            .ok_or("Condition is no longer current.")?;
        let raw = node.value.as_deref().ok_or("Choose a predicate.")?;
        let (kind, payload) = raw
            .split_once(':')
            .ok_or("Unsupported clause; reset it before editing.")?;
        let ((old_op, old_a), old_b) = parts(kind, payload);
        let read = |slot: &str, old: &str| {
            self.node_inputs
                .get(&(index, path.to_vec(), slot.into()))
                .map_or(old.to_string(), Clone::clone)
        };
        let op = read("op", old_op);
        let a = read("a", old_a);
        let b = read("b", old_b);
        let value = if kind == "time" && payload.contains("..") {
            format!("{kind}:{a}..{b}")
        } else {
            format!("{kind}:{op}{a}")
        };
        if !rules::valid_predicate(&value) {
            return Err("Invalid condition value. Draft retained; nothing was saved.".into());
        }
        node.value = Some(value);
        self.node_inputs
            .retain(|(i, p, _), _| *i != index || p != path);
        Ok(())
    }
    fn finish_inputs(&mut self) -> Result<(), String> {
        let keys: Vec<_> = self
            .node_inputs
            .keys()
            .map(|(i, p, _)| (*i, p.clone()))
            .collect();
        for (i, path) in keys {
            self.commit_node(i, &path)?;
        }
        Ok(())
    }
}
const ROOTS: [&str; 3] = ["schedule.rules", "browser.forms", "theme.wallpaperProfiles"];
fn saved_root(state: &CandidateState, path: &str) -> Option<Value> {
    let owner = candidate_state::pending::root_owner(path)?;
    state
        .pending_frontend
        .settings
        .iter()
        .find(|r| r.source_path == path && r.capability == owner)
        .and_then(|r| serde_json::from_str(&r.value_json).ok())
}
fn parts<'a>(kind: &str, payload: &'a str) -> ((&'a str, &'a str), &'a str) {
    if kind == "time" {
        if let Some((a, b)) = payload.split_once("..") {
            return (("", a), b);
        }
    }
    for op in [">=", "<=", ">", "<"] {
        if let Some(a) = payload.strip_prefix(op) {
            return ((op, a), "");
        }
    }
    (("", payload), "")
}
fn message(app: &GpuCandidate, action: Action) -> Message {
    Message::PendingForms {
        window: app.window,
        generation: app.settings_workbench.generation,
        action,
    }
}
fn publish(app: &mut GpuCandidate, path: &str) -> Result<(), String> {
    if saved_root(&app.state, path)
        != app
            .settings_workbench
            .pending_forms
            .baselines
            .get(path)
            .cloned()
            .flatten()
    {
        return Err(format!("Saved {path} changed since this form was opened. Draft retained; reload and reconcile before saving."));
    }
    let session = &mut app.settings_workbench.pending_forms;
    if let Some(error) = session.errors.get(path) {
        return Err(error.clone());
    }
    let value = match path {
        "schedule.rules" => {
            session.finish_inputs()?;
            for (i, rule) in session.rules.iter_mut().enumerate() {
                rule.priority = ((i + 1) * 10) as i64;
                rule.source_version();
                rule.validate()?;
            }
            serde_json::to_value(&session.rules)
        }
        "browser.forms" => {
            for form in &session.browsers {
                form.validate()?;
            }
            serde_json::to_value(&session.browsers)
        }
        "theme.wallpaperProfiles" => Ok(Value::Array(session.profiles.clone())),
        _ => return Err("Unknown pending form owner.".into()),
    }
    .map_err(|e| e.to_string())?;
    let owner = candidate_state::pending::root_owner(path).ok_or("Unknown pending root.")?;
    let mut proposed = app.state.clone();
    super::pending_projection::put(&mut proposed, owner, path, value)?;
    app.commit_local_state_change(proposed)
        .map_err(|e| app.save_failure.clone().unwrap_or_else(|| e.to_string()))?;
    let session = &mut app.settings_workbench.pending_forms;
    if path == "schedule.rules" {
        session.rules_dirty = false;
    }
    if path == "theme.wallpaperProfiles" {
        session.profiles_dirty = false;
    }
    if path == "browser.forms" {
        session.browsers_dirty = false;
    }
    session
        .baselines
        .insert(path.into(), saved_root(&app.state, path));
    session.message=match path{
        "schedule.rules"=>"Saved pending rules. Advanced scheduling is unavailable; active schedules and timers are unchanged.",
        "theme.wallpaperProfiles"=>"Saved pending wallpaper profiles. Automatic wallpaper-bound appearance is unavailable; the applied snapshot is unchanged.",
        _=>"Saved pending filters. Online search and downloads are unavailable; no request was submitted.",
    }.into();
    Ok(())
}
pub(in crate::candidate_app) fn finish_rules(app: &mut GpuCandidate) -> Result<(), String> {
    if app.settings_workbench.pending_forms.rules_dirty
        || !app.settings_workbench.pending_forms.node_inputs.is_empty()
    {
        publish(app, "schedule.rules")?;
    }
    Ok(())
}
pub(in crate::candidate_app) fn dispatch(app: &mut GpuCandidate, action: Action) {
    let category = match &action {
        Action::BrowserSource(_)
        | Action::BrowserText(..)
        | Action::BrowserChoice(..)
        | Action::BrowserToggle(..)
        | Action::BrowserChip(..)
        | Action::SaveBrowser => SettingsCategory::Sources,
        Action::SaveWallpaperProfile
        | Action::ProfileText(..)
        | Action::ProfileEnabled(..)
        | Action::ProfilePinned(..)
        | Action::DeleteProfile(..)
        | Action::SaveProfiles => SettingsCategory::Theme,
        _ => SettingsCategory::Automation,
    };
    if app.settings_workbench.windows_panel != Some(category) {
        return;
    }
    let result = (|| -> Result<(), String> {
        let pinned_settings =
            matches!(&action, Action::ProfilePinned(_, true)).then(|| profile_settings(app));
        let structural = matches!(
            &action,
            Action::AddRule
                | Action::RemoveRule(_)
                | Action::MoveRule(..)
                | Action::AddNode(..)
                | Action::RemoveNode(..)
                | Action::ResetNode(..)
                | Action::DeleteProfile(_)
                | Action::SaveWallpaperProfile
        );
        if structural && category == SettingsCategory::Automation {
            app.settings_workbench.pending_forms.finish_inputs()?;
        }
        let mut save = None;
        let session = &mut app.settings_workbench.pending_forms;
        match action {
            Action::NewKind(kind) => {
                if rules::KINDS.contains(&kind.as_str()) {
                    session.kind = kind;
                }
                return Ok(());
            }
            Action::AddRule => {
                session.rules.push(Rule::default());
                save = Some("schedule.rules");
            }
            Action::RemoveRule(i) => {
                if i < session.rules.len() {
                    session.rules.remove(i);
                    save = Some("schedule.rules");
                }
            }
            Action::MoveRule(i, delta) => {
                let to = i as isize + delta;
                if delta.abs() == 1
                    && i < session.rules.len()
                    && to >= 0
                    && (to as usize) < session.rules.len()
                {
                    session.rules.swap(i, to as usize);
                    save = Some("schedule.rules");
                }
            }
            Action::RuleText(i, field, text) => {
                if let Some(rule) = session.rules.get_mut(i) {
                    match field.as_str() {
                        "name" => rule.name = text,
                        "set" => rule.set = text,
                        _ => return Ok(()),
                    }
                    session.rules_dirty = true;
                }
                return Ok(());
            }
            Action::RuleEnabled(i, value) => {
                if let Some(rule) = session.rules.get_mut(i) {
                    rule.enabled = value;
                    save = Some("schedule.rules");
                }
            }
            Action::RuleMode(i, mode) => {
                if ["", "light", "dark"].contains(&mode.as_str()) {
                    if let Some(rule) = session.rules.get_mut(i) {
                        rule.mode = mode;
                        save = Some("schedule.rules");
                    }
                }
            }
            Action::SaveRules => save = Some("schedule.rules"),
            Action::NodeText(i, path, slot, text) => {
                session.node_inputs.insert((i, path, slot), text);
                session.rules_dirty = true;
                return Ok(());
            }
            Action::NodeCommit(i, path) => {
                session.commit_node(i, &path)?;
                save = Some("schedule.rules");
            }
            Action::NodeChoice(i, path, slot, value) => {
                session.node_inputs.insert((i, path.clone(), slot), value);
                session.commit_node(i, &path)?;
                save = Some("schedule.rules");
            }
            Action::NodeChip(i, path, choice) => {
                if let Some(node) = session
                    .rules
                    .get_mut(i)
                    .and_then(|r| r.condition.root.at_mut(&path))
                {
                    if let Some((kind, value)) =
                        node.value.as_deref().and_then(|v| v.split_once(':'))
                    {
                        let allowed = if kind == "weekday" {
                            rules::WEEKDAYS.as_slice()
                        } else if kind == "weather" {
                            rules::WEATHER.as_slice()
                        } else {
                            return Ok(());
                        };
                        if !allowed.contains(&choice.as_str()) {
                            return Ok(());
                        }
                        let mut values: Vec<String> = value.split(',').map(str::to_owned).collect();
                        if values.contains(&choice) {
                            if values.len() == 1 {
                                return Err("Select at least one condition value.".into());
                            }
                            values.retain(|v| v != &choice);
                        } else {
                            values.push(choice);
                        }
                        node.value = Some(format!("{kind}:{}", values.join(",")));
                        save = Some("schedule.rules");
                    }
                }
            }
            Action::AddNode(i, path, group) => {
                if path.len() >= 32 {
                    return Err("Source condition depth limit reached.".into());
                }
                let kind = session.kind.clone();
                if let Some(rule) = session.rules.get_mut(i) {
                    if rule.condition.root.count() >= 256 {
                        return Err("Source condition node limit reached.".into());
                    }
                    if let Some(children) = rule
                        .condition
                        .root
                        .at_mut(&path)
                        .and_then(|n| n.children.as_mut())
                    {
                        children.push(match group {
                            Some(any) => Node::group(any),
                            None => Node::predicate(&kind).ok_or("Unknown condition kind.")?,
                        });
                        save = Some("schedule.rules");
                    }
                }
            }
            Action::RemoveNode(i, mut path) => {
                if let Some(last) = path.pop() {
                    if let Some(children) = session
                        .rules
                        .get_mut(i)
                        .and_then(|r| r.condition.root.at_mut(&path))
                        .and_then(|n| n.children.as_mut())
                    {
                        if last < children.len() {
                            children.remove(last);
                            save = Some("schedule.rules");
                        }
                    }
                }
            }
            Action::ResetNode(i, path) => {
                let kind = session.kind.clone();
                if let Some(rule) = session.rules.get_mut(i) {
                    if let Some(node) = rule.condition.root.at_mut(&path) {
                        *node = if path.is_empty() {
                            Node::group(false)
                        } else {
                            Node::predicate(&kind).ok_or("Unknown condition.")?
                        };
                        rule.condition.version = 2;
                        save = Some("schedule.rules");
                    }
                }
            }
            Action::Negate(i, path, value) => {
                if let Some(node) = session
                    .rules
                    .get_mut(i)
                    .and_then(|r| r.condition.root.at_mut(&path))
                {
                    node.negated = value;
                    save = Some("schedule.rules");
                }
            }
            Action::GroupMode(i, path, mode) => {
                if ["all", "any"].contains(&mode.as_str()) {
                    if let Some(node) = session
                        .rules
                        .get_mut(i)
                        .and_then(|r| r.condition.root.at_mut(&path))
                    {
                        node.operator = Some(mode);
                        save = Some("schedule.rules");
                    }
                }
            }
            Action::BrowserSource(source) => {
                if forms::SOURCES.contains(&source.as_str()) {
                    if !session.browsers.iter().any(|b| b.source == source) {
                        session.browsers.push(BrowserForm::new(&source));
                    }
                    session.source = source;
                }
                return Ok(());
            }
            Action::BrowserText(field, value) | Action::BrowserChoice(field, value) => {
                session.browsers_dirty = true;
                let immediate = matches!(
                    field.as_str(),
                    "sort"
                        | "range"
                        | "orientation"
                        | "color"
                        | "safety"
                        | "size"
                        | "duration"
                        | "steam_kind"
                        | "steam_resolution"
                        | "steam_category"
                );
                if let Some(form) = session.browser_mut() {
                    match field.as_str() {
                        "query" => form.query = value,
                        "minimum" => form.minimum = value,
                        "maximum" => form.maximum = value,
                        "resolutions" => form.resolutions = split(&value),
                        "ratios" => form.ratios = split(&value),
                        "sort" => form.sort = value,
                        "range" => form.range = value,
                        "orientation" => form.orientation = value,
                        "color" => form.color = value,
                        "safety" => form.safety = value,
                        "size" => form.size = value,
                        "duration" => {
                            form.duration =
                                value.parse().map_err(|_| "Choose an available duration.")?
                        }
                        "steam_kind" => form.steam_kind = value,
                        "steam_resolution" => form.steam_resolution = value,
                        "steam_category" => form.steam_category = value,
                        _ => return Ok(()),
                    }
                }
                if immediate {
                    save = Some("browser.forms");
                } else {
                    return Ok(());
                }
            }
            Action::BrowserToggle(field, value) => {
                session.browsers_dirty = true;
                if let Some(form) = session.browser_mut() {
                    if field == "exact" {
                        form.exact = value;
                        save = Some("browser.forms");
                    }
                }
            }
            Action::BrowserChip(field, choice) => {
                session.browsers_dirty = true;
                if let Some(form) = session.browser_mut() {
                    let values = match field.as_str() {
                        "categories" => &mut form.categories,
                        "purity" => &mut form.purity,
                        "ratios" => &mut form.ratios,
                        _ => return Ok(()),
                    };
                    if values.contains(&choice) {
                        values.retain(|v| v != &choice);
                    } else {
                        values.push(choice);
                    }
                    save = Some("browser.forms");
                }
            }
            Action::SaveBrowser => save = Some("browser.forms"),
            Action::ProfileText(i, name) => {
                if let Some(profile) = session.profiles.get_mut(i).and_then(Value::as_object_mut) {
                    profile.insert("name".into(), name.into());
                    session.profiles_dirty = true;
                }
                return Ok(());
            }
            Action::ProfileEnabled(i, value) => {
                if let Some(profile) = session.profiles.get_mut(i).and_then(Value::as_object_mut) {
                    profile.insert("enabled".into(), value.into());
                    save = Some("theme.wallpaperProfiles");
                }
            }
            Action::ProfilePinned(i, value) => {
                if let Some(profile) = session.profiles.get_mut(i).and_then(Value::as_object_mut) {
                    profile.insert("settingsPinned".into(), value.into());
                    if let Some(settings) = pinned_settings {
                        profile.insert("settings".into(), settings);
                    } else {
                        profile.remove("settings");
                    }
                    save = Some("theme.wallpaperProfiles");
                }
            }
            Action::DeleteProfile(i) => {
                if i < session.profiles.len() {
                    session.profiles.remove(i);
                    save = Some("theme.wallpaperProfiles");
                }
            }
            Action::SaveProfiles => save = Some("theme.wallpaperProfiles"),
            Action::SaveWallpaperProfile => {
                capture_profile(app)?;
                save = Some("theme.wallpaperProfiles");
            }
        }
        if let Some(path) = save {
            if path == "schedule.rules" {
                app.settings_workbench.pending_forms.rules_dirty = true;
            }
            if path == "theme.wallpaperProfiles" {
                app.settings_workbench.pending_forms.profiles_dirty = true;
            }
            let result = publish(app, path);
            if structural {
                super::controller::rotate(app);
            }
            result?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        app.settings_workbench.pending_forms.message = error;
    }
}
fn profile_settings(app: &GpuCandidate) -> Value {
    use super::{
        bridge::Source,
        composed::{schema, SettingsSource},
    };
    let source = Source { app };
    let mut settings = serde_json::Map::new();
    for key in [
        "theme.policy",
        "theme.authority",
        "theme.engine",
        "theme.mode",
        "theme.staticTheme",
        "theme.style",
        "theme.scheme",
        "theme.wallustPalette",
        "theme.wallustColorspace",
        "theme.pywalSaturate",
        "theme.noctaliaScheme",
        "theme.noctaliaPureBlack",
        "matugen.schemeType",
        "matugen.colorIndex",
    ] {
        let value = match schema::value_kind(key) {
            Some(schema::ValueKind::Boolean) => source.flag(key).into(),
            Some(schema::ValueKind::Number) => source.number(key).into(),
            _ => source.text(key).into(),
        };
        settings.insert(key.into(), value);
    }
    Value::Object(settings)
}
fn split(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect()
}
fn capture_profile(app: &mut GpuCandidate) -> Result<(), String> {
    if app.theme_session.editor_pending() {
        return Err(
            "Set or reset pending Theme colour text before saving a wallpaper profile.".into(),
        );
    }
    let item = app
        .selected
        .and_then(|i| app.wallpapers.get(i))
        .ok_or("Select a wallpaper identity first.")?;
    let key = item.id.clone();
    let name = item.title.clone();
    let document = app
        .theme_session
        .editor_document()
        .ok_or("Open the visual Theme draft before saving a wallpaper profile.")?
        .clone();
    let session = &mut app.settings_workbench.pending_forms;
    if let Some(error) = session.errors.get("theme.wallpaperProfiles") {
        return Err(error.clone());
    }
    let index = session
        .profiles
        .iter()
        .position(|p| p.get("key").and_then(Value::as_str) == Some(key.as_str()))
        .unwrap_or_else(|| {
            session
                .profiles
                .push(serde_json::json!({"key":key,"name":name}));
            session.profiles.len() - 1
        });
    let profile = session.profiles[index]
        .as_object_mut()
        .ok_or("Stored profile is incompatible; original data retained.")?;
    for (variant, dark) in [("dark", true), ("light", false)] {
        let mut scheme = document.to_value();
        scheme["mode"] = if dark { "dark" } else { "light" }.into();
        scheme["is_dark_mode"] = dark.into();
        if let Some(colors) = scheme.get_mut("colors").and_then(Value::as_object_mut) {
            for node in colors.values_mut() {
                if let Some(value) = node.get(variant).cloned() {
                    node["default"] = value;
                }
            }
        }
        profile.insert(
            variant.into(),
            serde_json::json!({"_schemeVersion":1,"_scheme":scheme}),
        );
    }
    profile.insert("enabled".into(), true.into());
    Ok(())
}

fn form_button<'a>(
    app: &GpuCandidate,
    content: impl Into<Element<'a, Message>>,
) -> iced::widget::Button<'a, Message> {
    let palette = crate::candidate_app::view::donor_palette(app);
    button(content)
        .style(move |_, status| crate::donor_ui::action_style(palette, false, false, status))
}

pub(in crate::candidate_app) fn rules_view(app: &GpuCandidate) -> Element<'_, Message> {
    let session = &app.settings_workbench.pending_forms;
    let mut body=column![text("Advanced schedule rules - pending").size(20),
        text("Rules can be edited and saved locally. Advanced scheduling is unavailable; these never enter active schedules or start a timer.").size(12),
        text(&session.message).size(12)].spacing(12);
    if let Some(error) = session.errors.get("schedule.rules") {
        return body.push(text(error)).into();
    }
    body = body.push(
        row![
            form_button(app, "Add rule").on_press(message(app, Action::AddRule)),
            form_button(app, "Save pending rules").on_press(message(app, Action::SaveRules)),
            pick_list(
                rules::KINDS
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>(),
                Some(session.kind.clone()),
                move |kind| message(app, Action::NewKind(kind))
            ),
            form_button(app, "Run rules - unavailable"),
        ]
        .spacing(8),
    );
    for (index, rule) in session.rules.iter().enumerate() {
        let mut card = column![
            row![
                text(format!("Rule {}", index + 1)).size(16),
                Space::new().width(Fill),
                checkbox(rule.enabled)
                    .label("Enabled in pending configuration")
                    .on_toggle(move |value| message(app, Action::RuleEnabled(index, value))),
                form_button(app, "Up")
                    .on_press_maybe((index > 0).then(|| message(app, Action::MoveRule(index, -1)))),
                form_button(app, "Down").on_press_maybe(
                    (index + 1 < session.rules.len())
                        .then(|| message(app, Action::MoveRule(index, 1)))
                ),
                form_button(app, "Delete").on_press(message(app, Action::RemoveRule(index))),
            ]
            .spacing(8),
            text_input("Rule name", &rule.name)
                .on_input(move |text| message(app, Action::RuleText(index, "name".into(), text)))
                .on_submit(message(app, Action::SaveRules))
                .padding(7),
            row![
                text_input("Wallpaper identity / set", &rule.set)
                    .on_input(move |text| message(app, Action::RuleText(index, "set".into(), text)))
                    .on_submit(message(app, Action::SaveRules))
                    .padding(7),
                form_button(app, "Random").on_press(message(
                    app,
                    Action::RuleText(index, "set".into(), "random".into())
                )),
                pick_list(
                    vec![
                        Pick::new("", "Keep Theme mode"),
                        Pick::new("light", "Light"),
                        Pick::new("dark", "Dark")
                    ],
                    Some(Pick::new(
                        &rule.mode,
                        match rule.mode.as_str() {
                            "light" => "Light",
                            "dark" => "Dark",
                            _ => "Keep Theme mode",
                        }
                    )),
                    move |value| message(app, Action::RuleMode(index, value.value))
                ),
            ]
            .spacing(8),
        ]
        .spacing(9);
        if let Err(error) = rule.validate() {
            card = card.push(text(error)).push(
                form_button(app, "Reset invalid condition")
                    .on_press(message(app, Action::ResetNode(index, Vec::new()))),
            );
        }
        card = card.push(node_view(app, index, &rule.condition.root, Vec::new()));
        body = body.push(iced::widget::container(card).padding(12));
    }
    body.into()
}
fn node_view<'a>(
    app: &'a GpuCandidate,
    index: usize,
    node: &'a Node,
    path: Vec<usize>,
) -> Element<'a, Message> {
    let session = &app.settings_workbench.pending_forms;
    let negated_path = path.clone();
    let reset_path = path.clone();
    let mut head = row![checkbox(node.negated)
        .label("Not")
        .on_toggle(move |value| message(app, Action::Negate(index, negated_path.clone(), value)))]
    .spacing(7);
    if !path.is_empty() {
        head = head.push(
            form_button(app, "Remove condition")
                .on_press(message(app, Action::RemoveNode(index, path.clone()))),
        );
    }
    let mut body = column![head].spacing(8);
    if node.kind == "group" {
        let group_path = path.clone();
        body = body.push(
            row![
                text("Group"),
                pick_list(
                    vec![Pick::new("all", "All"), Pick::new("any", "Any")],
                    Some(Pick::new(
                        node.operator.as_deref().unwrap_or("all"),
                        if node.operator.as_deref() == Some("any") {
                            "Any"
                        } else {
                            "All"
                        }
                    )),
                    move |value| message(
                        app,
                        Action::GroupMode(index, group_path.clone(), value.value)
                    )
                ),
                form_button(app, "Add condition")
                    .on_press(message(app, Action::AddNode(index, path.clone(), None))),
                form_button(app, "+ All group").on_press(message(
                    app,
                    Action::AddNode(index, path.clone(), Some(false))
                )),
                form_button(app, "+ Any group").on_press(message(
                    app,
                    Action::AddNode(index, path.clone(), Some(true))
                )),
            ]
            .spacing(7),
        );
        for (i, child) in node.children.as_deref().unwrap_or(&[]).iter().enumerate() {
            let mut child_path = path.clone();
            child_path.push(i);
            body = body.push(node_view(app, index, child, child_path));
        }
    } else if let Some(value) = node.value.as_deref() {
        if !rules::valid_predicate(value) {
            body = body
                .push(text(format!("Unsupported stored clause retained: {value}")))
                .push(
                    form_button(app, "Reset condition")
                        .on_press(message(app, Action::ResetNode(index, reset_path))),
                );
        } else if let Some((kind, payload)) = value.split_once(':') {
            body = body.push(
                text(match kind {
                    "weekday" => "Weekdays",
                    "weather" => "Weather",
                    "time" => "Time",
                    "date" => "Date / range",
                    "year" => "Year",
                    "power" => "Power",
                    "battery" => "Battery percentage",
                    "output" => "Output identity (pending; no observed display is implied)",
                    "outputs" => "Output count",
                    _ => "Condition",
                })
                .size(13),
            );
            if ["weekday", "weather"].contains(&kind) {
                let choices = if kind == "weekday" {
                    rules::WEEKDAYS.as_slice()
                } else {
                    rules::WEATHER.as_slice()
                };
                let mut chips = row![].spacing(6);
                for choice in choices {
                    let selected = payload.split(',').any(|v| v == *choice);
                    chips = chips.push(
                        form_button(
                            app,
                            text(format!("{} {choice}", if selected { "[x]" } else { "[ ]" }))
                                .size(12),
                        )
                        .on_press(message(
                            app,
                            Action::NodeChip(index, path.clone(), choice.to_string()),
                        )),
                    );
                }
                body = body.push(chips);
            } else if kind == "power" {
                let choice_path = path.clone();
                body = body.push(pick_list(
                    vec!["battery".to_string(), "external".to_string()],
                    Some(session.node_text(index, &path, "a", payload)),
                    move |choice| {
                        message(
                            app,
                            Action::NodeChoice(index, choice_path.clone(), "a".into(), choice),
                        )
                    },
                ));
            } else {
                let ((op, a), b) = parts(kind, payload);
                let window = kind == "time" && payload.contains("..");
                if ["year", "battery", "outputs"].contains(&kind) || kind == "time" && !window {
                    let choices = if kind == "time" {
                        vec![">=", "<=", ">", "<"]
                    } else {
                        vec!["", ">=", "<=", ">", "<"]
                    };
                    let current = session.node_text(index, &path, "op", op);
                    let choice_path = path.clone();
                    body = body.push(pick_list(
                        choices
                            .into_iter()
                            .map(|v| Pick::new(v, if v.is_empty() { "=" } else { v }))
                            .collect::<Vec<_>>(),
                        Some(Pick::new(
                            &current,
                            if current.is_empty() { "=" } else { &current },
                        )),
                        move |choice| {
                            message(
                                app,
                                Action::NodeChoice(
                                    index,
                                    choice_path.clone(),
                                    "op".into(),
                                    choice.value,
                                ),
                            )
                        },
                    ));
                }
                for (slot, old) in if window {
                    vec![("a", a), ("b", b)]
                } else {
                    vec![("a", a)]
                } {
                    let raw = session.node_text(index, &path, slot, old);
                    let input_path = path.clone();
                    body = body.push(
                        text_input(
                            if kind == "time" {
                                "HH:MM or sunrise+30 / sunset-30"
                            } else {
                                "Value"
                            },
                            &raw,
                        )
                        .on_input(move |value| {
                            message(
                                app,
                                Action::NodeText(index, input_path.clone(), slot.into(), value),
                            )
                        })
                        .on_submit(message(app, Action::NodeCommit(index, path.clone())))
                        .padding(6),
                    );
                    if kind == "time" {
                        body = body.push(
                            row![
                                form_button(app, "Sunrise").on_press(message(
                                    app,
                                    Action::NodeChoice(
                                        index,
                                        path.clone(),
                                        slot.into(),
                                        "sunrise".into()
                                    )
                                )),
                                form_button(app, "Sunset").on_press(message(
                                    app,
                                    Action::NodeChoice(
                                        index,
                                        path.clone(),
                                        slot.into(),
                                        "sunset".into()
                                    )
                                )),
                            ]
                            .spacing(6),
                        );
                    }
                }
                body = body.push(
                    form_button(app, "Set condition")
                        .on_press(message(app, Action::NodeCommit(index, path.clone()))),
                );
            }
        }
    } else {
        body = body.push(text("Unsupported condition retained.")).push(
            form_button(app, "Reset condition")
                .on_press(message(app, Action::ResetNode(index, path.clone()))),
        );
    }
    iced::widget::container(body).padding(10).width(Fill).into()
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Pick {
    value: String,
    label: String,
}
impl Pick {
    fn new(value: &str, label: &str) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}
impl std::fmt::Display for Pick {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
fn browser_pick<'a>(
    app: &'a GpuCandidate,
    label: &str,
    field: &'static str,
    current: &str,
    options: &[&str],
) -> Element<'a, Message> {
    let choices: Vec<_> = options
        .iter()
        .map(|v| {
            Pick::new(
                v,
                if v.is_empty() || field == "duration" && *v == "0" {
                    "Any"
                } else {
                    v
                },
            )
        })
        .collect();
    let selected = choices.iter().find(|p| p.value == current).cloned();
    column![
        text(label.to_string()).size(12),
        pick_list(choices, selected, move |value| message(
            app,
            Action::BrowserChoice(field.into(), value.value)
        ))
        .width(Fill)
    ]
    .spacing(5)
    .into()
}
pub(in crate::candidate_app) fn browser_view(app: &GpuCandidate) -> Element<'_, Message> {
    let session = &app.settings_workbench.pending_forms;
    let mut body=column![text("Online source filters - pending").size(20),
        text("Edit local search/filter intent. Online search, authentication and downloads are unavailable; no results or jobs are produced.").size(12),text(&session.message).size(12)].spacing(10);
    let mut tabs = row![].spacing(7);
    for source in forms::SOURCES {
        tabs = tabs.push(
            form_button(app, source).on_press(message(app, Action::BrowserSource(source.into()))),
        );
    }
    body = body.push(tabs);
    if let Some(error) = session.errors.get("browser.forms") {
        return body.push(text(error)).into();
    }
    let Some(form) = session.browser() else {
        return body.push(text("Choose a source form.")).into();
    };
    body = body.push(
        text_input("Search query (saved pending only)", &form.query)
            .on_input(move |value| message(app, Action::BrowserText("query".into(), value)))
            .on_submit(message(app, Action::SaveBrowser))
            .padding(8),
    );
    match form.source.as_str() {
        "wallhaven" => {
            body = body.push(browser_pick(
                app,
                "Order",
                "sort",
                &form.sort,
                &[
                    "toplist",
                    "hot",
                    "date_added",
                    "relevance",
                    "views",
                    "favorites",
                    "random",
                ],
            ));
            if form.sort == "toplist" {
                body = body.push(browser_pick(
                    app,
                    "Top range",
                    "range",
                    &form.range,
                    &["1d", "3d", "1w", "1M", "3M", "6M", "1y"],
                ));
            }
            for (field, choices, current) in [
                (
                    "categories",
                    ["general", "anime", "people"].as_slice(),
                    &form.categories,
                ),
                (
                    "purity",
                    ["sfw", "sketchy", "nsfw"].as_slice(),
                    &form.purity,
                ),
                (
                    "ratios",
                    ["16x9", "16x10", "21x9", "32x9", "4x3"].as_slice(),
                    &form.ratios,
                ),
            ] {
                let mut chips = row![text(field.to_string()).size(12)].spacing(6);
                for choice in choices {
                    chips = chips.push(
                        form_button(
                            app,
                            text(format!(
                                "{} {choice}",
                                if current.iter().any(|v| v == choice) {
                                    "[x]"
                                } else {
                                    "[ ]"
                                }
                            ))
                            .size(12),
                        )
                        .on_press(message(
                            app,
                            Action::BrowserChip(field.into(), choice.to_string()),
                        )),
                    );
                }
                body = body.push(chips);
            }
            body = body.push(
                checkbox(form.exact)
                    .label("Exact resolutions (multiple)")
                    .on_toggle(move |value| {
                        message(app, Action::BrowserToggle("exact".into(), value))
                    }),
            );
            for (field, label, value) in if form.exact {
                vec![(
                    "resolutions",
                    "Exact resolutions, separated by commas",
                    form.resolutions.join(","),
                )]
            } else {
                vec![
                    ("minimum", "Minimum resolution", form.minimum.clone()),
                    ("maximum", "Maximum resolution", form.maximum.clone()),
                ]
            } {
                body = body.push(
                    text_input(label, &value)
                        .on_input(move |value| {
                            message(app, Action::BrowserText(field.into(), value))
                        })
                        .on_submit(message(app, Action::SaveBrowser))
                        .padding(7),
                );
            }
            body = body.push(browser_pick(
                app,
                "Colour bucket (-1=Any; source buckets0-12)",
                "color",
                &form.color,
                &[
                    "-1", "0", "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12",
                ],
            ));
        }
        "steam" => {
            body = body.push(browser_pick(
                app,
                "Order (3=trend,0=top,1=new,21=updated,9=subscribers)",
                "sort",
                &form.sort,
                &["3", "0", "1", "21", "9"],
            ));
            if form.sort == "3" {
                body = body.push(browser_pick(
                    app,
                    "Trend days",
                    "range",
                    &form.range,
                    &["1", "3", "7"],
                ));
            }
            body = body
                .push(browser_pick(
                    app,
                    "Kind",
                    "steam_kind",
                    &form.steam_kind,
                    &["", "Scene", "Video"],
                ))
                .push(browser_pick(
                    app,
                    "Resolution",
                    "steam_resolution",
                    &form.steam_resolution,
                    &forms::STEAM_RESOLUTIONS,
                ))
                .push(browser_pick(
                    app,
                    "Category",
                    "steam_category",
                    &form.steam_category,
                    &forms::STEAM_CATEGORIES,
                ));
        }
        "unsplash" => {
            body = body
                .push(browser_pick(
                    app,
                    "Order",
                    "sort",
                    &form.sort,
                    &["relevant", "latest"],
                ))
                .push(browser_pick(
                    app,
                    "Content safety",
                    "safety",
                    &form.safety,
                    &["low", "high"],
                ))
                .push(browser_pick(
                    app,
                    "Orientation",
                    "orientation",
                    &form.orientation,
                    &["", "landscape", "portrait", "square"],
                ))
                .push(browser_pick(
                    app,
                    "Colour",
                    "color",
                    &form.color,
                    &[
                        "",
                        "black_and_white",
                        "black",
                        "white",
                        "yellow",
                        "orange",
                        "red",
                        "purple",
                        "magenta",
                        "green",
                        "teal",
                        "blue",
                    ],
                ));
        }
        "pexels" => {
            body = body
                .push(browser_pick(
                    app,
                    "Size",
                    "size",
                    &form.size,
                    &["", "small", "medium", "large"],
                ))
                .push(browser_pick(
                    app,
                    "Orientation",
                    "orientation",
                    &form.orientation,
                    &["", "landscape", "portrait", "square"],
                ))
                .push(browser_pick(
                    app,
                    "Colour",
                    "color",
                    &form.color,
                    &[
                        "",
                        "red",
                        "orange",
                        "yellow",
                        "green",
                        "turquoise",
                        "blue",
                        "violet",
                        "pink",
                        "brown",
                        "black",
                        "gray",
                        "white",
                        "#808080",
                    ],
                ));
        }
        "youtube" => {
            body = body.push(browser_pick(
                app,
                "Maximum duration (seconds)",
                "duration",
                &form.duration.to_string(),
                &["0", "300", "600", "1800", "3600"],
            ))
        }
        _ => {}
    }
    body.push(
        row![
            form_button(app, "Save pending filters").on_press(message(app, Action::SaveBrowser)),
            form_button(app, "Search - unavailable"),
            form_button(app, "Download - unavailable")
        ]
        .spacing(8),
    )
    .into()
}
pub(in crate::candidate_app) fn profiles_view(app: &GpuCandidate) -> Element<'_, Message> {
    let session = &app.settings_workbench.pending_forms;
    let mut body=column![text("Wallpaper-bound Theme profiles - pending").size(20),
        text("Save this visual draft for the selected wallpaper identity. Profiles stay pending and never change current appearance or activate on wallpaper/capability changes.").size(12),text(&session.message).size(12)].spacing(10);
    if let Some(error) = session.errors.get("theme.wallpaperProfiles") {
        return body.push(text(error)).into();
    }
    body = body.push(
        row![
            form_button(app, "Save current draft as pending wallpaper profile").on_press_maybe(
                (app.selected.is_some() && app.theme_session.editor_document().is_some())
                    .then(|| message(app, Action::SaveWallpaperProfile))
            ),
            form_button(app, "Save pending names").on_press(message(app, Action::SaveProfiles))
        ]
        .spacing(8),
    );
    for (i, profile) in session.profiles.iter().enumerate() {
        body = body.push(
            column![
                text(
                    profile
                        .get("key")
                        .and_then(Value::as_str)
                        .unwrap_or("Unfamiliar source identity")
                        .to_string()
                )
                .size(11),
                text_input(
                    "Profile name",
                    profile.get("name").and_then(Value::as_str).unwrap_or("")
                )
                .on_input(move |name| message(app, Action::ProfileText(i, name)))
                .on_submit(message(app, Action::SaveProfiles))
                .padding(7),
                row![
                    checkbox(
                        profile
                            .get("enabled")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    )
                    .label("Enabled (pending)")
                    .on_toggle(move |enabled| message(app, Action::ProfileEnabled(i, enabled))),
                    checkbox(
                        profile
                            .get("settingsPinned")
                            .and_then(Value::as_bool)
                            .unwrap_or(false)
                    )
                    .label("Pin settings (pending)")
                    .on_toggle(move |pinned| message(app, Action::ProfilePinned(i, pinned))),
                    form_button(app, "Delete pending profile")
                        .on_press(message(app, Action::DeleteProfile(i)))
                ]
                .spacing(9),
            ]
            .spacing(7),
        );
    }
    body.into()
}

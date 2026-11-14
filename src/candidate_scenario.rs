use std::path::{Path, PathBuf};

use serde::Serialize;

const SCENARIO_FLAG: &str = "--scenario";
const ITEM_FLAG: &str = "--scenario-item";
const TARGET_FLAG: &str = "--scenario-target";
const OUTPUT_FLAG: &str = "--scenario-output";
const TIMEOUT_FLAG: &str = "--scenario-timeout-ms";
const UI_SMOKE: &str = "ui-smoke";
const DEFAULT_TIMEOUT_MS: u64 = 15_000;
const MIN_TIMEOUT_MS: u64 = 1_000;
const MAX_TIMEOUT_MS: u64 = 120_000;
pub const WINDOW_SETTLEMENT_TIMEOUT_MS: u64 = 1_000;
pub const WINDOW_SETTLEMENT_POLL_MS: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSettlementMode {
    Hidden,
    Fullscreen,
    Windowed,
    Unknown,
}

impl WindowSettlementMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hidden => "Hidden",
            Self::Fullscreen => "Fullscreen",
            Self::Windowed => "Windowed",
            Self::Unknown => "Unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowSettlementDecision {
    Pending,
    Settled,
    DeadlineExpired,
    Stale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WindowSettlementTracker {
    step: usize,
    requested: WindowSettlementMode,
    observed: Vec<WindowSettlementMode>,
    deadline_ms: u64,
}

impl WindowSettlementTracker {
    pub fn new(step: usize, requested: WindowSettlementMode, deadline_ms: u64) -> Self {
        Self {
            step,
            requested,
            observed: Vec::new(),
            deadline_ms,
        }
    }

    pub fn step(&self) -> usize {
        self.step
    }

    pub fn requested(&self) -> WindowSettlementMode {
        self.requested
    }

    pub fn observe(
        &mut self,
        step: usize,
        observed: WindowSettlementMode,
        elapsed_ms: u64,
    ) -> WindowSettlementDecision {
        if step != self.step {
            return WindowSettlementDecision::Stale;
        }
        self.observed.push(observed);
        if elapsed_ms >= self.deadline_ms {
            WindowSettlementDecision::DeadlineExpired
        } else if observed == self.requested {
            WindowSettlementDecision::Settled
        } else {
            WindowSettlementDecision::Pending
        }
    }

    pub fn detail(&self, elapsed_ms: u128) -> String {
        let observed = self
            .observed
            .iter()
            .map(|mode| mode.label())
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "requested_mode={} observed_modes=[{}] elapsed_ms={elapsed_ms}",
            self.requested.label(),
            observed
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioRequest {
    pub name: String,
    pub item: PathBuf,
    pub target: String,
    pub output: PathBuf,
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScenarioInvocation {
    NotRequested,
    Invalid(ScenarioReport),
    Ready(ScenarioRequest),
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ScenarioStage {
    pub name: String,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ScenarioReport {
    pub scenario: String,
    pub status: String,
    pub item_path: Option<String>,
    pub resolved_item_path: Option<String>,
    pub target: Option<String>,
    pub available_monitor_ids: Vec<String>,
    pub timeout_ms: Option<u64>,
    pub elapsed_ms: Option<u128>,
    pub stages: Vec<ScenarioStage>,
    pub error: Option<String>,
}

impl ScenarioReport {
    pub fn invalid(error: impl Into<String>) -> Self {
        Self {
            scenario: UI_SMOKE.to_string(),
            status: "invalid".to_string(),
            item_path: None,
            resolved_item_path: None,
            target: None,
            available_monitor_ids: Vec::new(),
            timeout_ms: None,
            elapsed_ms: None,
            stages: Vec::new(),
            error: Some(error.into()),
        }
    }

    pub fn for_request(request: &ScenarioRequest) -> Self {
        Self {
            scenario: request.name.clone(),
            status: "running".to_string(),
            item_path: Some(request.item.to_string_lossy().into_owned()),
            resolved_item_path: None,
            target: Some(request.target.clone()),
            available_monitor_ids: Vec::new(),
            timeout_ms: Some(request.timeout_ms),
            elapsed_ms: None,
            stages: Vec::new(),
            error: None,
        }
    }

    pub fn record(
        &mut self,
        name: impl Into<String>,
        status: impl Into<String>,
        detail: Option<String>,
    ) {
        self.stages.push(ScenarioStage {
            name: name.into(),
            status: status.into(),
            detail,
        });
    }

    pub fn finish_success(&mut self) {
        self.status = "passed".to_string();
    }

    pub fn finish_failure(&mut self, error: impl Into<String>) {
        self.status = "failed".to_string();
        self.error = Some(error.into());
    }

    pub fn mark_exit_requested(&mut self) {
        self.status = "exit-requested".to_string();
    }

    pub fn set_elapsed_ms(&mut self, elapsed_ms: u128) {
        self.elapsed_ms = Some(elapsed_ms);
    }

    pub fn set_monitor_ids(&mut self, monitor_ids: Vec<String>) {
        self.available_monitor_ids = monitor_ids;
    }

    pub fn set_resolved_item_path(&mut self, path: String) {
        self.resolved_item_path = Some(path);
    }

    pub fn write(&self, path: &Path) -> Result<(), String> {
        let json = serde_json::to_vec_pretty(self)
            .map_err(|error| format!("could not serialize scenario report: {error}"))?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("could not create scenario output directory: {error}"))?;
        }
        std::fs::write(path, json).map_err(|error| {
            format!(
                "could not write scenario report {}: {error}",
                path.display()
            )
        })
    }

    pub fn emit_json(&self) {
        match serde_json::to_string(self) {
            Ok(json) => println!("{json}"),
            Err(error) => println!(
                "{{\"scenario\":\"{UI_SMOKE}\",\"status\":\"invalid\",\"error\":{:?}}}",
                format!("could not serialize scenario report: {error}")
            ),
        }
    }
}

pub fn parse_arguments(arguments: &[String]) -> ScenarioInvocation {
    parse_arguments_with(arguments, Path::is_file)
}

fn parse_arguments_with<F>(arguments: &[String], is_file: F) -> ScenarioInvocation
where
    F: Fn(&Path) -> bool,
{
    let scenario_requested = arguments.iter().skip(1).any(|argument| {
        argument.eq_ignore_ascii_case(SCENARIO_FLAG)
            || argument.to_ascii_lowercase().starts_with("--scenario-")
    });
    if !scenario_requested {
        return ScenarioInvocation::NotRequested;
    }

    let mut scenario = None;
    let mut item = None;
    let mut target = None;
    let mut output = None;
    let mut timeout_ms = DEFAULT_TIMEOUT_MS;
    let mut index = 1;
    while index < arguments.len() {
        let argument = &arguments[index];
        let slot = if argument.eq_ignore_ascii_case(SCENARIO_FLAG) {
            Some(&mut scenario)
        } else if argument.eq_ignore_ascii_case(ITEM_FLAG) {
            Some(&mut item)
        } else if argument.eq_ignore_ascii_case(TARGET_FLAG) {
            Some(&mut target)
        } else if argument.eq_ignore_ascii_case(OUTPUT_FLAG) {
            Some(&mut output)
        } else {
            None
        };
        if let Some(slot) = slot {
            if index + 1 >= arguments.len() || arguments[index + 1].starts_with("--") {
                return ScenarioInvocation::Invalid(ScenarioReport::invalid(format!(
                    "{argument} requires an explicit value"
                )));
            }
            if slot.replace(arguments[index + 1].clone()).is_some() {
                return ScenarioInvocation::Invalid(ScenarioReport::invalid(format!(
                    "{argument} may be supplied only once"
                )));
            }
            index += 2;
            continue;
        }
        if argument.eq_ignore_ascii_case(TIMEOUT_FLAG) {
            if index + 1 >= arguments.len() || arguments[index + 1].starts_with("--") {
                return ScenarioInvocation::Invalid(ScenarioReport::invalid(format!(
                    "{argument} requires an explicit value"
                )));
            }
            let parsed = match arguments[index + 1].parse::<u64>() {
                Ok(value) if (MIN_TIMEOUT_MS..=MAX_TIMEOUT_MS).contains(&value) => value,
                _ => {
                    return ScenarioInvocation::Invalid(ScenarioReport::invalid(format!(
                        "{argument} must be between {MIN_TIMEOUT_MS} and {MAX_TIMEOUT_MS}"
                    )))
                }
            };
            timeout_ms = parsed;
            index += 2;
            continue;
        }
        if argument.to_ascii_lowercase().starts_with("--scenario-") {
            return ScenarioInvocation::Invalid(ScenarioReport::invalid(format!(
                "Unknown scenario argument: {argument}"
            )));
        }
        index += 1;
    }

    let Some(scenario) = scenario.filter(|value| !value.trim().is_empty()) else {
        return ScenarioInvocation::Invalid(ScenarioReport::invalid(
            "Scenario mode requires --scenario ui-smoke",
        ));
    };
    if !scenario.eq_ignore_ascii_case(UI_SMOKE) {
        return ScenarioInvocation::Invalid(ScenarioReport::invalid(
            "Only the explicitly enabled ui-smoke scenario is supported",
        ));
    }
    if !has_flag(arguments, "--show") {
        return ScenarioInvocation::Invalid(ScenarioReport::invalid(
            "Scenario mode requires --show",
        ));
    }
    if has_flag(arguments, "--allow-apply") {
        return ScenarioInvocation::Invalid(ScenarioReport::invalid(
            "ui-smoke never permits --allow-apply",
        ));
    }
    if has_flag(arguments, "--shell-hooks") {
        return ScenarioInvocation::Invalid(ScenarioReport::invalid(
            "ui-smoke does not register tray or global-hotkey hooks",
        ));
    }
    if arguments
        .iter()
        .any(|argument| argument.to_ascii_lowercase().starts_with("--diagnostic-"))
    {
        return ScenarioInvocation::Invalid(ScenarioReport::invalid(
            "ui-smoke cannot be combined with diagnostic Apply arguments",
        ));
    }

    let Some(item) = item.filter(|value| !value.trim().is_empty()) else {
        return ScenarioInvocation::Invalid(ScenarioReport::invalid(
            "Scenario mode requires --scenario-item <existing-image>",
        ));
    };
    let item = PathBuf::from(item);
    if !item.is_absolute() {
        return ScenarioInvocation::Invalid(ScenarioReport::invalid(
            "--scenario-item must be an absolute path",
        ));
    }
    if !is_file(&item) {
        return ScenarioInvocation::Invalid(ScenarioReport::invalid(
            "--scenario-item must name an existing file",
        ));
    }

    let Some(target) = target.filter(|value| !value.trim().is_empty()) else {
        return ScenarioInvocation::Invalid(ScenarioReport::invalid(
            "Scenario mode requires --scenario-target <device-id|all>",
        ));
    };

    let Some(output) = output.filter(|value| !value.trim().is_empty()) else {
        return ScenarioInvocation::Invalid(ScenarioReport::invalid(
            "Scenario mode requires --scenario-output <absolute-json-path>",
        ));
    };
    let output = PathBuf::from(output);
    if !output.is_absolute() {
        return ScenarioInvocation::Invalid(ScenarioReport::invalid(
            "--scenario-output must be an absolute path",
        ));
    }

    ScenarioInvocation::Ready(ScenarioRequest {
        name: UI_SMOKE.to_string(),
        item,
        target,
        output,
        timeout_ms,
    })
}

pub fn validate_isolated_environment() -> Result<PathBuf, String> {
    let Some(value) = std::env::var_os("WALLPAPER_SWITCHER_CONFIG_DIR") else {
        return Err(
            "ui-smoke requires WALLPAPER_SWITCHER_CONFIG_DIR; it never uses the default profile"
                .to_string(),
        );
    };
    let directory = PathBuf::from(value);
    if !directory.is_absolute() {
        return Err("WALLPAPER_SWITCHER_CONFIG_DIR must be absolute".to_string());
    }
    if !directory.is_dir() {
        return Err(format!(
            "scenario profile directory does not exist: {}",
            directory.display()
        ));
    }
    let config = directory.join("config.toml");
    if !config.is_file() {
        return Err(format!(
            "scenario profile is missing config.toml: {}",
            config.display()
        ));
    }
    let config_value =
        toml::from_str::<toml::Value>(&std::fs::read_to_string(&config).map_err(|error| {
            format!(
                "could not read scenario config {}: {error}",
                config.display()
            )
        })?)
        .map_err(|error| format!("scenario config is not valid TOML: {error}"))?;
    let state = directory.join("candidate-state.toml");
    let state_value = if state.is_file() {
        Some(
            toml::from_str::<toml::Value>(&std::fs::read_to_string(&state).map_err(|error| {
                format!("could not read scenario state {}: {error}", state.display())
            })?)
            .map_err(|error| format!("scenario state is not valid TOML: {error}"))?,
        )
    } else {
        None
    };
    validate_profile_values(&config_value, state_value.as_ref())?;
    Ok(directory)
}

fn validate_profile_values(
    config_value: &toml::Value,
    state_value: Option<&toml::Value>,
) -> Result<(), String> {
    if config_value
        .get("slideshow")
        .and_then(|value| value.get("enabled"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false)
    {
        return Err("scenario profile must disable [slideshow].enabled".to_string());
    }
    if state_value
        .and_then(|value| value.get("slideshow_enabled"))
        .and_then(toml::Value::as_bool)
        .unwrap_or(false)
    {
        return Err("scenario profile must disable candidate slideshow".to_string());
    }
    if state_value
        .and_then(|value| value.get("schedules"))
        .and_then(toml::Value::as_array)
        .is_some_and(|schedules| {
            schedules.iter().any(|schedule| {
                schedule
                    .get("enabled")
                    .and_then(toml::Value::as_bool)
                    .unwrap_or(false)
            })
        })
    {
        return Err("scenario profile must disable all schedules".to_string());
    }
    Ok(())
}

pub fn step_names() -> &'static [&'static str] {
    &[
        "pictures-filter",
        "no-match-filter",
        "clear-filter",
        "search-open",
        "search-close",
        "settings-open",
        "settings-close",
        "select-item",
        "detail-open",
        "hide-gallery",
        "tray-open-hidden",
        "tray-open-visible",
        "hide-before-exit",
        "tray-exit",
    ]
}

pub fn requires_window_settlement(step: &str) -> bool {
    window_settlement_mode(step).is_some()
}

pub fn settlement_observation_is_current(
    observed_step: usize,
    observed_generation: u64,
    active_step: usize,
    active_generation: u64,
) -> bool {
    observed_step == active_step && observed_generation == active_generation
}

pub fn settlement_visibility_generation_is_current(
    captured_generation: u64,
    current_generation: u64,
) -> bool {
    captured_generation == current_generation
}

pub fn window_settlement_mode(step: &str) -> Option<WindowSettlementMode> {
    match step {
        "hide-gallery" | "hide-before-exit" => Some(WindowSettlementMode::Hidden),
        "tray-open-hidden" | "tray-open-visible" => Some(WindowSettlementMode::Fullscreen),
        _ => None,
    }
}

fn has_flag(arguments: &[String], flag: &str) -> bool {
    arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case(flag))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn valid_args() -> Vec<String> {
        args(&[
            "gpu-candidate.exe",
            "--show",
            "--scenario",
            "ui-smoke",
            "--scenario-item",
            r"F:\PICTURE\target.png",
            "--scenario-target",
            "display-a",
            "--scenario-output",
            r"C:\evidence\scenario.json",
        ])
    }

    fn parse(arguments: &[String]) -> ScenarioInvocation {
        parse_arguments_with(arguments, |_| true)
    }

    #[test]
    fn normal_arguments_do_not_request_scenario_mode() {
        assert_eq!(
            parse(&args(&["gpu-candidate.exe", "--show"])),
            ScenarioInvocation::NotRequested
        );
    }

    #[test]
    fn scenario_requires_isolated_visible_non_mutating_invocation() {
        let mut arguments = valid_args();
        arguments.retain(|argument| argument != "--show");
        let ScenarioInvocation::Invalid(report) = parse(&arguments) else {
            panic!("expected --show rejection");
        };
        assert!(report.error.unwrap().contains("--show"));

        let mut arguments = valid_args();
        arguments.push("--allow-apply".into());
        let ScenarioInvocation::Invalid(report) = parse(&arguments) else {
            panic!("expected Apply rejection");
        };
        assert!(report.error.unwrap().contains("--allow-apply"));
    }

    #[test]
    fn scenario_requires_explicit_identity_paths() {
        let mut arguments = valid_args();
        arguments.retain(|argument| argument != "--scenario-target");
        arguments.retain(|argument| argument != "display-a");
        let ScenarioInvocation::Invalid(report) = parse(&arguments) else {
            panic!("expected target rejection");
        };
        assert!(report.error.unwrap().contains("--scenario-target"));

        let mut arguments = valid_args();
        let item_index = arguments
            .iter()
            .position(|argument| argument == r"F:\PICTURE\target.png")
            .unwrap();
        arguments[item_index] = r"relative.png".into();
        let ScenarioInvocation::Invalid(report) = parse(&arguments) else {
            panic!("expected absolute item rejection");
        };
        assert!(report.error.unwrap().contains("absolute"));
    }

    #[test]
    fn scenario_accepts_bounded_timeout_and_reports_steps() {
        let mut arguments = valid_args();
        arguments.extend(["--scenario-timeout-ms".into(), "2500".into()]);
        let ScenarioInvocation::Ready(request) = parse(&arguments) else {
            panic!("expected accepted scenario");
        };
        assert_eq!(request.timeout_ms, 2500);
        assert_eq!(step_names().last().copied(), Some("tray-exit"));
    }

    #[test]
    fn report_records_structured_stage_status() {
        let ScenarioInvocation::Ready(request) = parse(&valid_args()) else {
            panic!("expected accepted scenario");
        };
        let mut report = ScenarioReport::for_request(&request);
        report.record("catalog-ready", "passed", Some("items=1".into()));
        report.finish_success();
        let value = serde_json::to_value(report).unwrap();
        assert_eq!(value["status"], "passed");
        assert_eq!(value["stages"][0]["name"], "catalog-ready");
        assert_eq!(value["stages"][0]["status"], "passed");
    }

    #[test]
    fn window_actions_require_runtime_settlement() {
        assert!(requires_window_settlement("hide-gallery"));
        assert!(requires_window_settlement("tray-open-hidden"));
        assert!(requires_window_settlement("tray-open-visible"));
        assert!(requires_window_settlement("hide-before-exit"));
        assert!(!requires_window_settlement("pictures-filter"));
        assert!(!requires_window_settlement("detail-open"));
    }

    #[test]
    fn delayed_window_settlement_records_observations_before_success() {
        let mut tracker = WindowSettlementTracker::new(
            3,
            WindowSettlementMode::Hidden,
            WINDOW_SETTLEMENT_TIMEOUT_MS,
        );
        assert_eq!(
            tracker.observe(3, WindowSettlementMode::Windowed, 0),
            WindowSettlementDecision::Pending
        );
        assert_eq!(
            tracker.observe(3, WindowSettlementMode::Hidden, 32),
            WindowSettlementDecision::Settled
        );
        assert_eq!(
            tracker.detail(32),
            "requested_mode=Hidden observed_modes=[Windowed,Hidden] elapsed_ms=32"
        );
    }

    #[test]
    fn window_settlement_expires_at_its_explicit_deadline() {
        let mut tracker = WindowSettlementTracker::new(
            4,
            WindowSettlementMode::Fullscreen,
            WINDOW_SETTLEMENT_TIMEOUT_MS,
        );
        assert_eq!(
            tracker.observe(
                4,
                WindowSettlementMode::Windowed,
                WINDOW_SETTLEMENT_TIMEOUT_MS
            ),
            WindowSettlementDecision::DeadlineExpired
        );
        assert_eq!(tracker.observed.len(), 1);
    }

    #[test]
    fn matching_mode_after_deadline_does_not_pass_settlement() {
        let mut tracker = WindowSettlementTracker::new(
            4,
            WindowSettlementMode::Hidden,
            WINDOW_SETTLEMENT_TIMEOUT_MS,
        );
        assert_eq!(
            tracker.observe(
                4,
                WindowSettlementMode::Hidden,
                WINDOW_SETTLEMENT_TIMEOUT_MS + 1,
            ),
            WindowSettlementDecision::DeadlineExpired
        );
        assert_eq!(tracker.observed, [WindowSettlementMode::Hidden]);
    }

    #[test]
    fn stale_window_settlement_observation_is_rejected_without_recording() {
        let mut tracker = WindowSettlementTracker::new(
            5,
            WindowSettlementMode::Hidden,
            WINDOW_SETTLEMENT_TIMEOUT_MS,
        );
        assert_eq!(
            tracker.observe(4, WindowSettlementMode::Hidden, 12),
            WindowSettlementDecision::Stale
        );
        assert!(tracker.observed.is_empty());
        assert!(!settlement_observation_is_current(4, 9, 5, 9));
        assert!(!settlement_observation_is_current(5, 8, 5, 9));
        assert!(settlement_observation_is_current(5, 9, 5, 9));
        assert!(!settlement_visibility_generation_is_current(8, 9));
        assert!(settlement_visibility_generation_is_current(9, 9));
        assert_eq!(
            tracker.observe(5, WindowSettlementMode::Hidden, 20),
            WindowSettlementDecision::Settled
        );
    }

    #[test]
    fn exit_request_is_not_reported_as_completed() {
        let ScenarioInvocation::Ready(request) = parse(&valid_args()) else {
            panic!("expected accepted scenario");
        };
        let mut report = ScenarioReport::for_request(&request);
        report.record(
            "tray-exit",
            "requested",
            Some("external supervisor must verify process termination".into()),
        );
        report.mark_exit_requested();
        assert_eq!(report.status, "exit-requested");
        assert_eq!(report.stages[0].status, "requested");
        assert_ne!(report.status, "passed");
    }

    #[test]
    fn profile_guard_rejects_automation_and_slideshow() {
        let config: toml::Value = toml::from_str("[slideshow]\nenabled = true\n").unwrap();
        assert!(validate_profile_values(&config, None)
            .unwrap_err()
            .contains("slideshow"));

        let config = toml::Value::Table(Default::default());
        let state: toml::Value =
            toml::from_str("slideshow_enabled = false\n[[schedules]]\nenabled = true\n").unwrap();
        assert!(validate_profile_values(&config, Some(&state))
            .unwrap_err()
            .contains("schedules"));
    }
}

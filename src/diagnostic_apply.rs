use std::path::{Path, PathBuf};
use std::sync::mpsc;

use serde::Serialize;

use crate::coordinator::{
    resolve_explicit_apply_target, ApplyOperationState, ApplyResult, CoordinatorResult,
    MonitorApplyOutcome, WallpaperCoordinator,
};
use crate::scanner::{WallpaperItem, WallpaperSource};
use crate::transition::{TransitionConfig, TransitionEffect};

const APPLY_FLAG: &str = "--diagnostic-apply";
// Settled evidence for the live matrix: how long after playback the desktop is
// fingerprinted again, to compare with the captured incoming frame.
const SETTLED_CAPTURE_DELAY: std::time::Duration = std::time::Duration::from_secs(1);
const TARGET_FLAG: &str = "--diagnostic-target";
const TRANSITION_FLAG: &str = "--diagnostic-transition";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticTarget {
    All,
    DeviceId(String),
}

impl DiagnosticTarget {
    fn requested_name(&self) -> String {
        match self {
            Self::All => "all".to_string(),
            Self::DeviceId(device_id) => device_id.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticRequest {
    pub wallpaper_path: PathBuf,
    pub target: DiagnosticTarget,
    pub transition: TransitionEffect,
    pub wallpaper_id: Option<String>,
    pub wallpaper_source: Option<String>,
    pub wallpaper_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticInvocation {
    NotRequested,
    Invalid(DiagnosticApplyOutput),
    Ready(DiagnosticRequest),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticMonitorOutcome {
    pub target_id: String,
    pub confirmation_state: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagnosticApplyOutput {
    pub operation_id: Option<u64>,
    pub target: Option<String>,
    pub wallpaper_path: Option<String>,
    pub wallpaper_id: Option<String>,
    pub wallpaper_source: Option<String>,
    pub wallpaper_type: Option<String>,
    pub transition: Option<String>,
    pub transition_outcome: Option<String>,
    pub transition_warning: Option<String>,
    pub per_monitor_outcomes: Vec<DiagnosticMonitorOutcome>,
    pub confirmation_state: String,
    pub error: Option<String>,
}

impl DiagnosticApplyOutput {
    fn invalid(
        target: Option<String>,
        wallpaper_path: Option<String>,
        transition: Option<String>,
        error: impl Into<String>,
    ) -> Self {
        Self {
            operation_id: None,
            target,
            wallpaper_path,
            wallpaper_id: None,
            wallpaper_source: None,
            wallpaper_type: None,
            transition,
            transition_outcome: None,
            transition_warning: None,
            per_monitor_outcomes: Vec::new(),
            confirmation_state: "not-submitted".to_string(),
            error: Some(error.into()),
        }
    }

    pub fn before_submission(request: &DiagnosticRequest, error: impl Into<String>) -> Self {
        let mut output = Self::invalid(
            Some(request.target.requested_name()),
            Some(request.wallpaper_path.to_string_lossy().into_owned()),
            Some(request.transition.key().to_string()),
            error,
        );
        output.wallpaper_id = request.wallpaper_id.clone();
        output.wallpaper_source = request.wallpaper_source.clone();
        output.wallpaper_type = request.wallpaper_type.clone();
        output
    }

    fn after_submission(
        request: &DiagnosticRequest,
        operation_id: u64,
        error: impl Into<String>,
    ) -> Self {
        let mut output = Self::before_submission(request, error);
        output.operation_id = Some(operation_id);
        output.confirmation_state = "unknown".to_string();
        output
    }

    fn from_apply_result(request: &DiagnosticRequest, result: ApplyResult) -> Self {
        let error = result.error_message();
        let confirmation_state = aggregate_confirmation_state(&result.outcomes).to_string();
        let per_monitor_outcomes = result
            .outcomes
            .into_iter()
            .map(|outcome| DiagnosticMonitorOutcome {
                target_id: outcome.target_id,
                confirmation_state: state_name(outcome.state).to_string(),
                error: outcome.error,
            })
            .collect();
        Self {
            operation_id: Some(result.operation_id.get()),
            target: Some(request.target.requested_name()),
            wallpaper_path: Some(request.wallpaper_path.to_string_lossy().into_owned()),
            wallpaper_id: request.wallpaper_id.clone(),
            wallpaper_source: request.wallpaper_source.clone(),
            wallpaper_type: request.wallpaper_type.clone(),
            transition: Some(request.transition.key().to_string()),
            transition_outcome: Some(result.transition.status_name().to_string()),
            transition_warning: result.transition.warning_message(),
            per_monitor_outcomes,
            confirmation_state,
            error,
        }
    }

    pub fn emit_json(&self) {
        match serde_json::to_string(self) {
            Ok(json) => println!("{json}"),
            Err(error) => println!(
                "{{\"operation_id\":null,\"confirmation_state\":\"unknown\",\"error\":{:?}}}",
                format!("Could not serialize diagnostic result: {error}")
            ),
        }
    }
}

pub fn parse_arguments(arguments: &[String]) -> DiagnosticInvocation {
    parse_arguments_with(arguments, Path::is_file)
}

fn parse_arguments_with<F>(arguments: &[String], is_file: F) -> DiagnosticInvocation
where
    F: Fn(&Path) -> bool,
{
    let diagnostic_requested = arguments
        .iter()
        .skip(1)
        .any(|argument| argument.to_ascii_lowercase().starts_with("--diagnostic-"));
    if !diagnostic_requested {
        return DiagnosticInvocation::NotRequested;
    }

    let mut wallpaper_path = None;
    let mut target = None;
    let mut transition = None;
    let mut index = 1;
    while index < arguments.len() {
        let argument = &arguments[index];
        if argument.eq_ignore_ascii_case(APPLY_FLAG)
            || argument.eq_ignore_ascii_case(TARGET_FLAG)
            || argument.eq_ignore_ascii_case(TRANSITION_FLAG)
        {
            if index + 1 >= arguments.len() || arguments[index + 1].starts_with("--") {
                return DiagnosticInvocation::Invalid(DiagnosticApplyOutput::invalid(
                    target.clone(),
                    wallpaper_path.clone(),
                    transition.clone(),
                    format!("{argument} requires an explicit value"),
                ));
            }
            let slot = if argument.eq_ignore_ascii_case(APPLY_FLAG) {
                &mut wallpaper_path
            } else if argument.eq_ignore_ascii_case(TARGET_FLAG) {
                &mut target
            } else {
                &mut transition
            };
            if slot.replace(arguments[index + 1].clone()).is_some() {
                return DiagnosticInvocation::Invalid(DiagnosticApplyOutput::invalid(
                    target.clone(),
                    wallpaper_path.clone(),
                    transition.clone(),
                    format!("{argument} may be supplied only once"),
                ));
            }
            index += 2;
            continue;
        }
        if argument.to_ascii_lowercase().starts_with("--diagnostic-") {
            return DiagnosticInvocation::Invalid(DiagnosticApplyOutput::invalid(
                target.clone(),
                wallpaper_path.clone(),
                transition.clone(),
                format!("Unknown diagnostic argument: {argument}"),
            ));
        }
        index += 1;
    }

    if !has_flag(arguments, "--allow-apply") {
        return DiagnosticInvocation::Invalid(DiagnosticApplyOutput::invalid(
            target,
            wallpaper_path,
            transition,
            "Diagnostic Apply requires --allow-apply",
        ));
    }
    if !has_flag(arguments, crate::apply_trace::FLAG) {
        return DiagnosticInvocation::Invalid(DiagnosticApplyOutput::invalid(
            target,
            wallpaper_path,
            transition,
            "Diagnostic Apply requires --apply-trace",
        ));
    }

    let Some(wallpaper_path) = wallpaper_path else {
        return DiagnosticInvocation::Invalid(DiagnosticApplyOutput::invalid(
            target,
            None,
            transition,
            "Diagnostic Apply requires --diagnostic-apply <wallpaper-path>",
        ));
    };
    let wallpaper_path = PathBuf::from(wallpaper_path);
    if !is_file(&wallpaper_path) && !is_plain_engine_identity(&wallpaper_path) {
        return DiagnosticInvocation::Invalid(DiagnosticApplyOutput::invalid(
            target,
            Some(wallpaper_path.to_string_lossy().into_owned()),
            transition,
            "Diagnostic wallpaper path does not name an existing file or installed Engine catalog identity",
        ));
    }

    let Some(target) = target.filter(|value| !value.trim().is_empty()) else {
        return DiagnosticInvocation::Invalid(DiagnosticApplyOutput::invalid(
            None,
            Some(wallpaper_path.to_string_lossy().into_owned()),
            transition,
            "Diagnostic Apply requires --diagnostic-target <device-id|all>",
        ));
    };
    let target = if target.eq_ignore_ascii_case("all") {
        DiagnosticTarget::All
    } else {
        DiagnosticTarget::DeviceId(target)
    };

    let Some(transition) = transition.filter(|value| !value.trim().is_empty()) else {
        return DiagnosticInvocation::Invalid(DiagnosticApplyOutput::invalid(
            Some(target.requested_name()),
            Some(wallpaper_path.to_string_lossy().into_owned()),
            None,
            "Diagnostic Apply requires --diagnostic-transition <off|random|catalog effect>",
        ));
    };
    // Every catalog effect is reachable so the live matrix can use this harness.
    // Only exact keys are accepted, so the recorded name is the played effect;
    // display names and legacy aliases stay out.
    let transition = match std::iter::once(TransitionEffect::Off)
        .chain(std::iter::once(TransitionEffect::Random))
        .chain(crate::transition::TRANSITION_CATALOG)
        .find(|effect| effect.key() == transition)
    {
        Some(effect) => effect,
        None => {
            return DiagnosticInvocation::Invalid(DiagnosticApplyOutput::invalid(
                Some(target.requested_name()),
                Some(wallpaper_path.to_string_lossy().into_owned()),
                Some(transition),
                "Diagnostic transition must be off, random, or a catalog effect key",
            ));
        }
    };

    DiagnosticInvocation::Ready(DiagnosticRequest {
        wallpaper_path,
        target,
        transition,
        wallpaper_id: None,
        wallpaper_source: None,
        wallpaper_type: None,
    })
}

fn is_plain_engine_identity(path: &Path) -> bool {
    let value = path.to_string_lossy();
    !value.trim().is_empty()
        && !value.contains('\\')
        && !value.contains('/')
        && !value.contains(':')
}

fn has_flag(arguments: &[String], flag: &str) -> bool {
    arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case(flag))
}

type SingleSubmissionError = (Option<u64>, String);

fn execute_single_submission<T, S, W>(submit: S, wait: W) -> Result<(u64, T), SingleSubmissionError>
where
    S: FnOnce() -> Result<u64, String>,
    W: FnOnce(u64) -> Result<T, String>,
{
    let operation_id = submit().map_err(|error| (None, error))?;
    let result = wait(operation_id).map_err(|error| (Some(operation_id), error))?;
    Ok((operation_id, result))
}

pub fn run(request: DiagnosticRequest) -> DiagnosticApplyOutput {
    let wallpaper = match resolve_diagnostic_wallpaper(&request.wallpaper_path) {
        Ok(wallpaper) => wallpaper,
        Err(error) => return DiagnosticApplyOutput::before_submission(&request, error),
    };
    let request = DiagnosticRequest {
        wallpaper_path: wallpaper.engine_target().to_path_buf(),
        wallpaper_id: Some(wallpaper.id.clone()),
        wallpaper_source: Some(source_name(wallpaper.source).to_string()),
        wallpaper_type: Some(wallpaper.wallpaper_type.clone()),
        ..request
    };
    let requested_target = request.target.requested_name();
    let target = match resolve_explicit_apply_target(&requested_target) {
        Ok(target) => target,
        Err(error) => return DiagnosticApplyOutput::before_submission(&request, error),
    };

    let transition = TransitionConfig {
        effect: request.transition,
        duration_ms: TransitionConfig::default().duration_ms,
    };
    let engine_rendered = matches!(
        wallpaper.source,
        WallpaperSource::WallpaperEngine | WallpaperSource::LocalVideo
    );
    let (finished_tx, finished_rx) = mpsc::sync_channel(1);
    let coordinator = WallpaperCoordinator::spawn(move |result| {
        let _ = finished_tx.send(result);
    });
    crate::log_debug(&format!(
        "[DIAGNOSTIC-APPLY] stage=before-submit target={requested_target:?} wallpaper_id={:?} source={} path={:?} transition={}",
        request.wallpaper_id,
        request.wallpaper_source.as_deref().unwrap_or("unknown"),
        request.wallpaper_path,
        request.transition.key()
    ));

    let execution = execute_single_submission(
        || {
            let submission = coordinator.submit(wallpaper, target, transition)?;
            crate::log_debug(&format!(
                "[DIAGNOSTIC-APPLY] stage=after-submit operation_id={} state=command-accepted",
                submission.operation_id.get()
            ));
            Ok(submission.operation_id.get())
        },
        |expected_operation_id| {
            let result = finished_rx
                .recv()
                .map_err(|_| "Wallpaper coordinator ended before returning a result".to_string())?;
            match result {
                CoordinatorResult::Apply(result)
                    if result.operation_id.get() == expected_operation_id => Ok(result),
                CoordinatorResult::Apply(result) => Err(format!(
                    "Wallpaper coordinator returned operation {} while waiting for {expected_operation_id}",
                    result.operation_id.get()
                )),
                CoordinatorResult::Engine(_) => {
                    Err("Wallpaper coordinator returned an unexpected Engine result".to_string())
                }
            }
        },
    );

    let output = match execution {
        Ok((_, result)) => {
            // Trace-only and Diagnostic-Apply-only; normal Apply never waits here.
            if crate::apply_trace::enabled() && engine_rendered {
                let operation = result.operation_id.get();
                record_settled_frames(operation, &result.outcomes, std::thread::sleep, |display| {
                    settled_capture(operation, display)
                });
            }
            DiagnosticApplyOutput::from_apply_result(&request, result)
        }
        Err((Some(operation_id), error)) => {
            DiagnosticApplyOutput::after_submission(&request, operation_id, error)
        }
        Err((None, error)) => DiagnosticApplyOutput::before_submission(&request, error),
    };
    crate::log_debug(&format!(
        "[DIAGNOSTIC-APPLY] stage=finished operation_id={} confirmation_state={} error={:?}",
        output
            .operation_id
            .map_or_else(|| "none".to_string(), |id| id.to_string()),
        output.confirmation_state,
        output.error
    ));
    output
}

/// Fingerprints each confirmed display once the new wallpaper has settled and
/// records it in the apply trace. Returns the recorded trace details.
fn record_settled_frames(
    operation: u64,
    outcomes: &[MonitorApplyOutcome],
    sleep: impl FnOnce(std::time::Duration),
    mut capture: impl FnMut(&str) -> Result<crate::transition::FrozenFrame, String>,
) -> Vec<String> {
    let confirmed: Vec<_> = outcomes
        .iter()
        .filter(|outcome| outcome.state == ApplyOperationState::ProviderConfirmed)
        .collect();
    if confirmed.is_empty() {
        return Vec::new();
    }
    sleep(SETTLED_CAPTURE_DELAY);
    confirmed
        .into_iter()
        .map(|outcome| {
            let captured = capture(&outcome.target_id)
                .map(|frame| crate::engine_capture::FrameFingerprint::of(&frame));
            let details = format!(
                "display={:?} delay_ms={} {}",
                outcome.target_id,
                SETTLED_CAPTURE_DELAY.as_millis(),
                match captured {
                    Ok(fingerprint) => format!("fingerprint={fingerprint}"),
                    Err(error) => format!("error={error:?}"),
                }
            );
            crate::apply_trace::event_for(operation, "transition-settled-capture", &details);
            details
        })
        .collect()
}

/// Read-only capture of a display through the existing capture worker.
fn settled_capture(
    operation: u64,
    display: &str,
) -> Result<crate::transition::FrozenFrame, String> {
    let monitor = crate::native_wallpaper::monitors()?
        .into_iter()
        .find(|monitor| monitor.device_id.eq_ignore_ascii_case(display))
        .ok_or("the display is no longer available")?;
    let endpoint = crate::engine_ipc::control_window_identity()?;
    crate::engine_capture::capture_for_apply(
        monitor.rect,
        operation,
        display,
        "settled",
        endpoint,
        &[],
        None,
    )
}

fn resolve_diagnostic_wallpaper(reference: &Path) -> Result<WallpaperItem, String> {
    let identity = reference.to_string_lossy();
    match crate::scanner::resolve_installed_engine_item(&identity) {
        Ok(item) => return Ok(item),
        Err(error) if is_plain_engine_identity(reference) => return Err(error),
        Err(_) => {}
    }

    let canonical_path = std::fs::canonicalize(reference).map_err(|error| {
        format!(
            "Could not resolve diagnostic wallpaper path or installed Engine identity {}: {error}",
            reference.display()
        )
    })?;
    if !canonical_path.is_file() {
        return Err("Diagnostic wallpaper path is no longer a file".to_string());
    }
    crate::scanner::local_image_item(&canonical_path)
}

fn source_name(source: WallpaperSource) -> &'static str {
    match source {
        WallpaperSource::LocalImage => "local-image",
        WallpaperSource::LocalVideo => "local-video",
        WallpaperSource::WallpaperEngine => "wallpaper-engine",
    }
}

fn state_name(state: ApplyOperationState) -> &'static str {
    match state {
        ApplyOperationState::CommandAccepted => "command-accepted",
        ApplyOperationState::ProviderConfirmed => "provider-confirmed",
        ApplyOperationState::Failed => "failed",
        ApplyOperationState::Cancelled => "cancelled",
        ApplyOperationState::Unknown => "unknown",
    }
}

fn aggregate_confirmation_state(outcomes: &[MonitorApplyOutcome]) -> &'static str {
    if outcomes.is_empty() {
        return "failed";
    }
    for state in [
        ApplyOperationState::Unknown,
        ApplyOperationState::Failed,
        ApplyOperationState::Cancelled,
        ApplyOperationState::CommandAccepted,
    ] {
        if outcomes.iter().any(|outcome| outcome.state == state) {
            return state_name(state);
        }
    }
    "provider-confirmed"
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn valid_args(transition: &str) -> Vec<String> {
        args(&[
            "gpu-candidate.exe",
            "--allow-apply",
            "--apply-trace",
            "--diagnostic-apply",
            r"C:\wallpapers\incoming.png",
            "--diagnostic-target",
            "display-a",
            "--diagnostic-transition",
            transition,
        ])
    }

    fn parse_with_existing_path(arguments: &[String]) -> DiagnosticInvocation {
        parse_arguments_with(arguments, |_| true)
    }

    #[test]
    fn normal_ui_arguments_do_not_request_diagnostic_mode() {
        assert_eq!(
            parse_with_existing_path(&args(&["gpu-candidate.exe", "--show"])),
            DiagnosticInvocation::NotRequested
        );
    }

    #[test]
    fn diagnostic_mode_requires_allow_apply() {
        let mut arguments = valid_args("off");
        arguments.retain(|argument| argument != "--allow-apply");

        let DiagnosticInvocation::Invalid(output) = parse_with_existing_path(&arguments) else {
            panic!("expected rejected diagnostic invocation");
        };
        assert!(output.error.unwrap().contains("--allow-apply"));
    }

    #[test]
    fn diagnostic_mode_requires_apply_trace() {
        let mut arguments = valid_args("off");
        arguments.retain(|argument| argument != "--apply-trace");

        let DiagnosticInvocation::Invalid(output) = parse_with_existing_path(&arguments) else {
            panic!("expected rejected diagnostic invocation");
        };
        assert!(output.error.unwrap().contains("--apply-trace"));
    }

    #[test]
    fn diagnostic_mode_requires_explicit_path_and_existing_file() {
        let missing_value = args(&[
            "gpu-candidate.exe",
            "--allow-apply",
            "--apply-trace",
            "--diagnostic-target",
            "all",
            "--diagnostic-transition",
            "off",
        ]);
        let DiagnosticInvocation::Invalid(output) = parse_with_existing_path(&missing_value) else {
            panic!("expected missing path rejection");
        };
        assert!(output.error.unwrap().contains(APPLY_FLAG));

        let DiagnosticInvocation::Invalid(output) =
            parse_arguments_with(&valid_args("off"), |_| false)
        else {
            panic!("expected missing file rejection");
        };
        assert!(output.error.unwrap().contains("existing file"));
    }

    #[test]
    fn diagnostic_mode_requires_explicit_target() {
        let arguments = args(&[
            "gpu-candidate.exe",
            "--allow-apply",
            "--apply-trace",
            "--diagnostic-apply",
            r"C:\wallpapers\incoming.png",
            "--diagnostic-transition",
            "off",
        ]);
        let DiagnosticInvocation::Invalid(output) = parse_with_existing_path(&arguments) else {
            panic!("expected missing target rejection");
        };
        assert!(output.error.unwrap().contains(TARGET_FLAG));
    }

    #[test]
    fn diagnostic_transition_accepts_every_catalog_effect_off_and_random() {
        for expected in [TransitionEffect::Off, TransitionEffect::Random]
            .into_iter()
            .chain(crate::transition::TRANSITION_CATALOG)
        {
            let value = expected.key();
            let DiagnosticInvocation::Ready(request) = parse_with_existing_path(&valid_args(value))
            else {
                panic!("expected accepted transition {value}");
            };
            assert_eq!(request.transition, expected);
        }

        for rejected in [
            "sand",
            "definitely-not-an-effect",
            "none",
            "wipe",
            "ripple",
            TransitionEffect::WipeLeft.display_name(),
        ] {
            let DiagnosticInvocation::Invalid(output) =
                parse_with_existing_path(&valid_args(rejected))
            else {
                panic!("expected unsupported transition rejection for {rejected}");
            };
            assert!(output.error.unwrap().contains("catalog effect key"));
        }
    }

    #[test]
    fn device_target_must_resolve_once_before_submission() {
        assert!(
            crate::coordinator::resolve_explicit_apply_target_from_device_ids(
                "display-a",
                &["DISPLAY-A".to_string()]
            )
            .is_ok()
        );
        assert!(
            crate::coordinator::resolve_explicit_apply_target_from_device_ids(
                "display-a",
                &["display-b".to_string()]
            )
            .unwrap_err()
            .contains("does not match")
        );
        assert!(
            crate::coordinator::resolve_explicit_apply_target_from_device_ids(
                "display-a",
                &["display-a".to_string(), "DISPLAY-A".to_string()]
            )
            .unwrap_err()
            .contains("ambiguous")
        );
    }

    #[test]
    fn diagnostic_result_serializes_required_fields() {
        let output = DiagnosticApplyOutput {
            operation_id: Some(41),
            target: Some("display-a".into()),
            wallpaper_path: Some(r"C:\wallpapers\incoming.png".into()),
            wallpaper_id: None,
            wallpaper_source: None,
            wallpaper_type: None,
            transition: Some("off".into()),
            transition_outcome: Some("not-requested".into()),
            transition_warning: None,
            per_monitor_outcomes: vec![DiagnosticMonitorOutcome {
                target_id: "display-a".into(),
                confirmation_state: "provider-confirmed".into(),
                error: None,
            }],
            confirmation_state: "provider-confirmed".into(),
            error: None,
        };
        let value = serde_json::to_value(output).unwrap();

        assert_eq!(value["operation_id"], 41);
        assert_eq!(value["target"], "display-a");
        assert_eq!(value["wallpaper_path"], r"C:\wallpapers\incoming.png");
        assert!(value["wallpaper_id"].is_null());
        assert_eq!(value["transition"], "off");
        assert_eq!(value["transition_outcome"], "not-requested");
        assert!(value["transition_warning"].is_null());
        assert_eq!(
            value["per_monitor_outcomes"][0]["confirmation_state"],
            "provider-confirmed"
        );
        assert_eq!(value["confirmation_state"], "provider-confirmed");
        assert!(value["error"].is_null());
    }

    #[test]
    fn settled_capture_records_a_fingerprint_for_each_confirmed_display() {
        let outcomes = vec![
            MonitorApplyOutcome {
                target_id: "display-a".into(),
                state: ApplyOperationState::ProviderConfirmed,
                error: None,
            },
            MonitorApplyOutcome {
                target_id: "display-b".into(),
                state: ApplyOperationState::Unknown,
                error: Some("no independent confirmation".into()),
            },
            MonitorApplyOutcome {
                target_id: "display-c".into(),
                state: ApplyOperationState::ProviderConfirmed,
                error: None,
            },
        ];
        let frame = crate::transition::FrozenFrame {
            width: 8,
            height: 4,
            rgba: (0..32u8)
                .flat_map(|value| [value * 8, value * 8, value * 8, 255])
                .collect(),
        };
        let expected = crate::engine_capture::FrameFingerprint::of(&frame).to_string();
        let mut slept = None;
        let mut captured = Vec::new();
        let details = record_settled_frames(
            51,
            &outcomes,
            |delay| slept = Some(delay),
            |display| {
                captured.push(display.to_string());
                if display == "display-c" {
                    Err("presenter changed".into())
                } else {
                    Ok(frame.clone())
                }
            },
        );
        assert_eq!(slept, Some(SETTLED_CAPTURE_DELAY));
        assert_eq!(captured, ["display-a", "display-c"]);
        assert_eq!(
            details,
            [
                format!("display=\"display-a\" delay_ms=1000 fingerprint={expected}"),
                "display=\"display-c\" delay_ms=1000 error=\"presenter changed\"".to_string(),
            ]
        );

        assert!(record_settled_frames(
            51,
            &outcomes[1..2],
            |_| panic!("nothing to settle"),
            |_| panic!("nothing to capture")
        )
        .is_empty());
    }

    #[test]
    fn orchestration_submits_exactly_once() {
        let submissions = AtomicUsize::new(0);
        let result = execute_single_submission(
            || {
                submissions.fetch_add(1, Ordering::SeqCst);
                Ok(73)
            },
            |operation_id| Ok(operation_id + 1),
        )
        .unwrap();

        assert_eq!(submissions.load(Ordering::SeqCst), 1);
        assert_eq!(result, (73, 74));
    }

    #[test]
    fn diagnostic_mode_accepts_plain_engine_catalog_identity() {
        let mut arguments = valid_args("off");
        let apply_index = arguments
            .iter()
            .position(|argument| argument == "--diagnostic-apply")
            .unwrap();
        arguments[apply_index + 1] = "1661379435".into();

        let DiagnosticInvocation::Ready(request) = parse_arguments_with(&arguments, |_| false)
        else {
            panic!("expected an Engine catalog identity to defer to runtime resolution");
        };
        assert_eq!(request.wallpaper_path, PathBuf::from("1661379435"));
    }
}

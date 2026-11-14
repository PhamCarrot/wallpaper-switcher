use std::{
    cell::Cell,
    sync::atomic::{AtomicBool, AtomicU64, Ordering},
};

pub const FLAG: &str = "--apply-trace";

#[cfg(test)]
pub const REQUIRED_STAGE_NAMES: &[&str] = &[
    "submission",
    "worker-receipt",
    "worker-panic",
    "monitor-resolution-start",
    "monitor-resolution-complete",
    "monitor-resolution-failure",
    "rollback-snapshot-start",
    "rollback-snapshot-complete",
    "rollback-snapshot-failure",
    "outgoing-capture-start",
    "outgoing-capture-complete",
    "outgoing-capture-failure",
    "incoming-decode-start",
    "incoming-decode-complete",
    "incoming-decode-failure",
    "transition-window-start",
    "transition-window-complete",
    "transition-window-failure",
    "d3d-create-start",
    "d3d-create-complete",
    "d3d-create-fallback",
    "native-conversion-start",
    "native-conversion-complete",
    "native-conversion-failure",
    "set-wallpaper-start",
    "set-wallpaper-complete",
    "set-wallpaper-failure",
    "readback-start",
    "readback-complete",
    "readback-failure",
    "completion",
    "failure",
    "automation-timer-generation",
    "automation-timer-start",
    "automation-timer-failure",
    "automation-timer-stopped",
    "automation-tick-emitted",
    "automation-tick-send-failure",
    "automation-tick-received",
    "automation-decision",
    "automation-playlist-catalog-snapshot",
    "automation-submission",
    "ui-item-selected",
    "ui-detail-transition",
    "ui-apply-intent",
    "ui-apply-decision",
    "ui-apply-submission",
    "ui-apply-result",
    "ui-visibility-transition",
    "iced-widget-mouse-receipt",
    "iced-widget-mouse-publication",
    "iced-widget-mouse-ignored",
    "gallery-mouse-consumption",
];

static ENABLED: AtomicBool = AtomicBool::new(false);
static WIDGET_MOUSE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

thread_local! {
    static OPERATION_ID: Cell<Option<u64>> = const { Cell::new(None) };
}

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Release);
}

pub fn enabled() -> bool {
    ENABLED.load(Ordering::Acquire)
}

/// Bounded opt-in CPU-stage diagnostics; elapsed time is not presented-frame time.
pub(crate) fn cpu_span(stage: &'static str, identity: impl FnOnce() -> String) -> Option<CpuSpan> {
    static SAMPLES: AtomicU64 = AtomicU64::new(0);
    if !enabled() || SAMPLES.fetch_add(1, Ordering::Relaxed) >= 20_000 {
        return None;
    }
    Some(CpuSpan {
        stage,
        identity: identity(),
        started: std::time::Instant::now(),
    })
}

pub(crate) struct CpuSpan {
    stage: &'static str,
    identity: String,
    started: std::time::Instant,
}

impl Drop for CpuSpan {
    fn drop(&mut self) {
        let elapsed_us = self.started.elapsed().as_micros();
        event_lazy(self.stage, || {
            format!("elapsed_us={elapsed_us} {}", self.identity)
        });
    }
}

/// Returns the next process-local sequence for an opt-in gallery mouse trace.
/// Callers must only request it after confirming that tracing is enabled.
pub fn next_widget_mouse_sequence() -> u64 {
    WIDGET_MOUSE_SEQUENCE
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("widget mouse trace sequence exhausted")
        + 1
}

pub fn with_operation<T>(operation_id: u64, operation: impl FnOnce() -> T) -> T {
    struct RestoreOperation(Option<u64>);

    impl Drop for RestoreOperation {
        fn drop(&mut self) {
            OPERATION_ID.with(|current| current.set(self.0));
        }
    }

    let previous = OPERATION_ID.with(|current| current.replace(Some(operation_id)));
    let _restore = RestoreOperation(previous);
    operation()
}

pub fn event(stage: &str, details: impl AsRef<str>) {
    let operation_id = OPERATION_ID.with(Cell::get);
    event_with_operation(operation_id, stage, details);
}

pub fn event_for(operation_id: u64, stage: &str, details: impl AsRef<str>) {
    event_with_operation(Some(operation_id), stage, details);
}

pub fn event_lazy(stage: &str, details: impl FnOnce() -> String) {
    event_with_operation_lazy(OPERATION_ID.with(Cell::get), stage, details);
}

pub fn event_for_lazy(operation_id: u64, stage: &str, details: impl FnOnce() -> String) {
    event_with_operation_lazy(Some(operation_id), stage, details);
}

fn event_with_operation_lazy(
    operation_id: Option<u64>,
    stage: &str,
    details: impl FnOnce() -> String,
) {
    let Some(details) = build_if_enabled(enabled(), details) else {
        return;
    };
    event_with_operation(operation_id, stage, details);
}

fn build_if_enabled<T>(is_enabled: bool, build: impl FnOnce() -> T) -> Option<T> {
    if is_enabled {
        Some(build())
    } else {
        None
    }
}

fn event_with_operation(operation_id: Option<u64>, stage: &str, details: impl AsRef<str>) {
    let Some(line) = trace_line(operation_id, stage, details.as_ref()) else {
        return;
    };
    crate::log_debug(&line);
}

fn trace_line(operation_id: Option<u64>, stage: &str, details: &str) -> Option<String> {
    if !enabled() {
        return None;
    }
    Some(format_trace_line(operation_id, stage, details))
}

fn format_trace_line(operation_id: Option<u64>, stage: &str, details: &str) -> String {
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    let details = details.replace(['\r', '\n'], " ");
    format!(
        "[APPLY-TRACE] timestamp_unix_ms={timestamp_ms} pid={} op={} stage={stage} {details}",
        std::process::id(),
        operation_id.map_or_else(|| "none".to_string(), |id| id.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracing_is_disabled_by_default() {
        assert!(!enabled());
        assert!(trace_line(Some(1), "submission", "path=test").is_none());
    }

    #[test]
    fn required_stage_names_are_stable_and_unique() {
        let mut names = REQUIRED_STAGE_NAMES.to_vec();
        names.sort_unstable();
        names.dedup();

        assert_eq!(names.len(), REQUIRED_STAGE_NAMES.len());
        for required in [
            "submission",
            "worker-receipt",
            "worker-panic",
            "monitor-resolution-start",
            "rollback-snapshot-start",
            "outgoing-capture-start",
            "incoming-decode-start",
            "transition-window-start",
            "d3d-create-start",
            "native-conversion-start",
            "set-wallpaper-start",
            "readback-start",
            "completion",
            "failure",
            "automation-timer-generation",
            "automation-timer-start",
            "automation-tick-emitted",
            "automation-tick-received",
            "automation-decision",
            "automation-playlist-catalog-snapshot",
            "automation-submission",
            "ui-item-selected",
            "ui-detail-transition",
            "ui-apply-intent",
            "ui-apply-submission",
            "ui-apply-result",
            "ui-visibility-transition",
        ] {
            assert!(
                REQUIRED_STAGE_NAMES.contains(&required),
                "missing {required}"
            );
        }
    }

    #[test]
    fn disabled_lazy_trace_does_not_build_details() {
        let mut built = false;

        let details = build_if_enabled(false, || {
            built = true;
            "should not be built".to_string()
        });

        assert!(!built);
        assert_eq!(details, None);
    }

    #[test]
    fn trace_line_records_time_process_and_operation_identity() {
        let line = format_trace_line(Some(42), "automation-submission", "item_id=fixture");

        assert!(line.contains("timestamp_unix_ms="));
        assert!(line.contains(&format!("pid={}", std::process::id())));
        assert!(line.contains("op=42 stage=automation-submission item_id=fixture"));
    }
}

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, OnceLock};
use std::{
    collections::{HashMap, HashSet},
    panic::{catch_unwind, AssertUnwindSafe},
    path::PathBuf,
};

use crate::desktop_transition::DesktopTransitionSession;
use crate::engine_capture;
use crate::engine_ipc;
use crate::engine_ipc::{EngineObservation, EngineObservationState, EngineTarget};
use crate::monitor::{best_rect_match, DesktopRect};
use crate::native_wallpaper;
use crate::scanner::{WallpaperItem, WallpaperSource};
use crate::transition::{FrozenFrame, TransitionConfig, TransitionEffect};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyTarget {
    Monitor(DesktopRect),
    DeviceId(String),
    DeviceIds(Vec<String>),
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OperationId(u64);

impl OperationId {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
    Native,
    WallpaperEngine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProviderPostcondition {
    EngineControlAvailable,
    EngineProject {
        location: String,
        path: PathBuf,
    },
    EngineAbsent {
        location: String,
    },
    NativeWallpaper {
        device_id: String,
        path: PathBuf,
        position: i32,
    },
    ProviderStateReadable {
        provider: ProviderKind,
        target_id: String,
        location: Option<String>,
    },
    ReadbackUnavailable {
        capability: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderRecovery {
    pub operation_id: u64,
    pub wallpaper_id: Option<String>,
    pub target_id: String,
    pub provider: ProviderKind,
    pub identity: String,
    pub expected: ProviderPostcondition,
    pub reason: String,
    pub phase: RecoveryPhase,
    pub(crate) observation: Option<EngineObservation>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryPhase {
    Preflight,
    DispatchedOrUnknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecoveryCapabilities {
    pub retry_observation: bool,
    pub cancel_preflight: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ShutdownAction {
    Ready,
    WaitForActive,
    CancelPreflight(ProviderRecovery),
    Blocked(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyOperationState {
    CommandAccepted,
    ProviderConfirmed,
    Failed,
    Cancelled,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplySubmission {
    pub operation_id: OperationId,
    pub wallpaper_id: String,
    pub target_ids: Vec<String>,
    pub state: ApplyOperationState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorApplyOutcome {
    pub target_id: String,
    pub state: ApplyOperationState,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyResult {
    pub operation_id: OperationId,
    pub wallpaper_id: String,
    pub outcomes: Vec<MonitorApplyOutcome>,
    pub recoveries: Vec<ProviderRecovery>,
    pub transition: TransitionResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionStatus {
    NotRequested,
    /// A transition was requested, but the display already shows the requested
    /// wallpaper, so there is nothing to animate.
    Unchanged,
    NotAttempted,
    Completed,
    DirectFallback,
    IncomingCaptureUnavailable,
    PartialIncomingCapture,
    PlaybackFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionResult {
    pub status: TransitionStatus,
    pub warnings: Vec<String>,
    pub displays: Vec<DisplayTransitionOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayTransitionOutcome {
    pub display_id: String,
    pub status: TransitionStatus,
    pub intermediate_submitted: bool,
    pub error: Option<String>,
}

impl TransitionResult {
    pub const fn not_requested() -> Self {
        Self {
            status: TransitionStatus::NotRequested,
            warnings: Vec::new(),
            displays: Vec::new(),
        }
    }

    const fn unchanged() -> Self {
        Self {
            status: TransitionStatus::Unchanged,
            warnings: Vec::new(),
            displays: Vec::new(),
        }
    }

    fn not_attempted(reason: impl Into<String>) -> Self {
        Self {
            status: TransitionStatus::NotAttempted,
            warnings: vec![reason.into()],
            displays: Vec::new(),
        }
    }

    fn direct_fallback(reason: impl Into<String>) -> Self {
        Self {
            status: TransitionStatus::DirectFallback,
            warnings: vec![reason.into()],
            displays: Vec::new(),
        }
    }

    fn incoming_capture_unavailable(warnings: Vec<String>) -> Self {
        Self {
            status: TransitionStatus::IncomingCaptureUnavailable,
            warnings,
            displays: Vec::new(),
        }
    }

    fn partial_incoming_capture(warnings: Vec<String>) -> Self {
        Self {
            status: TransitionStatus::PartialIncomingCapture,
            warnings,
            displays: Vec::new(),
        }
    }

    fn playback_failed(warnings: Vec<String>) -> Self {
        Self {
            status: TransitionStatus::PlaybackFailed,
            warnings,
            displays: Vec::new(),
        }
    }

    pub fn status_name(&self) -> &'static str {
        match self.status {
            TransitionStatus::NotRequested => "not-requested",
            TransitionStatus::Unchanged => "unchanged",
            TransitionStatus::NotAttempted => "not-attempted",
            TransitionStatus::Completed => "completed",
            TransitionStatus::DirectFallback => "direct-fallback",
            TransitionStatus::IncomingCaptureUnavailable => "incoming-capture-unavailable",
            TransitionStatus::PartialIncomingCapture => "partial-incoming-capture",
            TransitionStatus::PlaybackFailed => "playback-failed",
        }
    }

    pub fn warning_message(&self) -> Option<String> {
        if self.warnings.is_empty() {
            return None;
        }
        let prefix = match self.status {
            TransitionStatus::NotAttempted => "Transition was not attempted",
            TransitionStatus::DirectFallback => {
                "Transition preparation failed; wallpaper changed by direct switch"
            }
            TransitionStatus::IncomingCaptureUnavailable => {
                "Wallpaper changed, but animation was skipped because an incoming frame was unavailable"
            }
            TransitionStatus::PartialIncomingCapture => {
                "Animation ran for available targets, but was skipped where an incoming frame was unavailable"
            }
            TransitionStatus::PlaybackFailed => {
                "Wallpaper changed, but its animation did not complete"
            }
            TransitionStatus::NotRequested
            | TransitionStatus::Unchanged
            | TransitionStatus::Completed => return None,
        };
        Some(format!("{prefix}: {}", self.warnings.join("; ")))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum EngineCommand {
    SetPaused(bool),
    SetMuted(bool),
    SetVolume(u8),
    ApplyProperties {
        wallpaper_id: String,
        project_json_path: PathBuf,
        properties: serde_json::Map<String, serde_json::Value>,
        target: ApplyTarget,
    },
    ReconcileProvider {
        recovery: ProviderRecovery,
    },
    CancelPreflight {
        recovery: ProviderRecovery,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineControlResult {
    pub operation_id: OperationId,
    pub command: EngineCommand,
    pub outcomes: Vec<MonitorApplyOutcome>,
    pub recoveries: Vec<ProviderRecovery>,
}

impl EngineControlResult {
    pub(crate) fn recovery_resolved(&self) -> bool {
        matches!(
            self.command,
            EngineCommand::ReconcileProvider { .. } | EngineCommand::CancelPreflight { .. }
        ) && !self.outcomes.is_empty()
            && self.outcomes.iter().all(|outcome| {
                matches!(
                    outcome.state,
                    ApplyOperationState::Cancelled | ApplyOperationState::ProviderConfirmed
                )
            })
    }
    pub fn is_success(&self) -> bool {
        !self.outcomes.is_empty()
            && self.outcomes.iter().all(|outcome| {
                matches!(
                    outcome.state,
                    ApplyOperationState::CommandAccepted | ApplyOperationState::ProviderConfirmed
                )
            })
    }

    pub fn error_message(&self) -> Option<String> {
        let failures: Vec<_> = self
            .outcomes
            .iter()
            .filter(|outcome| {
                !matches!(
                    outcome.state,
                    ApplyOperationState::CommandAccepted | ApplyOperationState::ProviderConfirmed
                )
            })
            .map(|outcome| {
                format!(
                    "{}: {}",
                    outcome.target_id,
                    outcome.error.as_deref().unwrap_or(match outcome.state {
                        ApplyOperationState::Cancelled => "operation was cancelled",
                        ApplyOperationState::Unknown => "provider state is unknown",
                        ApplyOperationState::Failed => "provider rejected the command",
                        ApplyOperationState::CommandAccepted
                        | ApplyOperationState::ProviderConfirmed => unreachable!(),
                    })
                )
            })
            .collect();
        (!failures.is_empty()).then(|| failures.join("; "))
    }

    pub fn user_message(&self) -> Option<&'static str> {
        user_facing_outcome_message(&self.outcomes)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CoordinatorResult {
    Apply(ApplyResult),
    Engine(EngineControlResult),
}

impl ApplyResult {
    pub fn is_success(&self) -> bool {
        !self.outcomes.is_empty()
            && self
                .outcomes
                .iter()
                .all(|outcome| outcome.state == ApplyOperationState::ProviderConfirmed)
    }

    pub fn error_message(&self) -> Option<String> {
        let failures: Vec<_> = self
            .outcomes
            .iter()
            .filter(|outcome| outcome.state != ApplyOperationState::ProviderConfirmed)
            .map(|outcome| {
                let detail = outcome.error.as_deref().unwrap_or(match outcome.state {
                    ApplyOperationState::Cancelled => "operation was cancelled",
                    ApplyOperationState::Unknown => "provider state is unknown",
                    ApplyOperationState::CommandAccepted => "command is still pending",
                    ApplyOperationState::Failed => "provider rejected the wallpaper",
                    ApplyOperationState::ProviderConfirmed => unreachable!(),
                });
                format!("{}: {detail}", outcome.target_id)
            })
            .collect();
        (!failures.is_empty()).then(|| failures.join("; "))
    }

    pub fn user_message(&self) -> Option<&'static str> {
        user_facing_outcome_message(&self.outcomes)
    }
}

fn user_facing_outcome_message(outcomes: &[MonitorApplyOutcome]) -> Option<&'static str> {
    if outcomes
        .iter()
        .any(|outcome| outcome.state == ApplyOperationState::Unknown)
    {
        Some("Wallpaper provider state could not be confirmed.")
    } else if outcomes
        .iter()
        .any(|outcome| outcome.state == ApplyOperationState::Failed)
    {
        Some("Wallpaper provider rejected the request.")
    } else if outcomes
        .iter()
        .any(|outcome| outcome.state == ApplyOperationState::Cancelled)
    {
        Some("Wallpaper request was cancelled.")
    } else if outcomes
        .iter()
        .any(|outcome| outcome.state == ApplyOperationState::CommandAccepted)
    {
        Some("Wallpaper request is still pending.")
    } else {
        None
    }
}

#[derive(Debug, Clone)]
struct ApplyRequest {
    operation_id: OperationId,
    wallpaper: WallpaperItem,
    target: ApplyTarget,
    transition: TransitionConfig,
}

#[derive(Debug, Clone)]
struct EngineRequest {
    operation_id: OperationId,
    command: EngineCommand,
}

#[derive(Debug, Clone)]
enum CoordinatorRequest {
    Apply(ApplyRequest),
    Engine(EngineRequest),
}

pub struct WallpaperCoordinator {
    sender: SyncSender<CoordinatorRequest>,
    pending_requests: Arc<AtomicUsize>,
}

struct PendingRequest(Arc<AtomicUsize>);

impl Drop for PendingRequest {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

fn next_operation_id() -> OperationId {
    static NEXT_OPERATION_ID: AtomicU64 = AtomicU64::new(1);
    OperationId(NEXT_OPERATION_ID.fetch_add(1, Ordering::Relaxed))
}

#[cfg(test)]
pub(crate) fn fixture_operation_id() -> OperationId {
    next_operation_id()
}

static UNRESOLVED_PROVIDER_OPERATIONS: OnceLock<Mutex<Vec<ProviderRecovery>>> = OnceLock::new();
// Keep input paths and rollback snapshots alive through Apply and recovery recording.
static NATIVE_CACHE_USE: Mutex<()> = Mutex::new(());

fn unresolved_provider_operations() -> &'static Mutex<Vec<ProviderRecovery>> {
    UNRESOLVED_PROVIDER_OPERATIONS.get_or_init(|| Mutex::new(Vec::new()))
}

pub(crate) fn with_native_cache_eviction<T>(
    evict: impl FnOnce(&HashSet<String>) -> T,
) -> Result<T, String> {
    let _gate = NATIVE_CACHE_USE
        .try_lock()
        .map_err(|_| "native cache is in use or Apply state is unavailable".to_string())?;
    let pending = unresolved_provider_operations()
        .lock()
        .map_err(|_| "provider recovery state is unavailable".to_string())?;
    if pending
        .iter()
        .any(|record| record.provider == ProviderKind::Native)
    {
        return Err("native provider recovery is pending".into());
    }
    drop(pending);

    let monitors = native_wallpaper::monitors()?;
    if monitors.is_empty() {
        return Err("no monitor wallpaper paths could be confirmed".into());
    }
    let mut protected = HashSet::with_capacity(monitors.len());
    for monitor in monitors {
        let path = native_wallpaper::get_wallpaper_for_device(&monitor.device_id)?;
        protected.insert(native_wallpaper::normalized_wallpaper_path(&path));
    }
    Ok(evict(&protected))
}

pub(crate) fn same_recovery(left: &ProviderRecovery, right: &ProviderRecovery) -> bool {
    left.operation_id == right.operation_id
        && left.target_id.eq_ignore_ascii_case(&right.target_id)
        && left.provider == right.provider
        && left.identity.eq_ignore_ascii_case(&right.identity)
        && left.expected == right.expected
        && left.phase == right.phase
}

fn retain_provider_recovery(recovery: ProviderRecovery) {
    if let Ok(mut pending) = unresolved_provider_operations().lock() {
        if let Some(existing) = pending
            .iter_mut()
            .find(|existing| same_recovery(existing, &recovery))
        {
            existing.reason = recovery.reason;
            existing.observation = recovery.observation;
        } else {
            pending.push(recovery);
        }
    }
}

pub(crate) fn settle_matching_recovery(
    pending: &mut Vec<ProviderRecovery>,
    recovery: &ProviderRecovery,
) -> bool {
    let before = pending.len();
    pending.retain(|existing| !same_recovery(existing, recovery));
    pending.len() != before
}

fn retained_provider_recoveries() -> Vec<ProviderRecovery> {
    unresolved_provider_operations()
        .lock()
        .map(|pending| pending.clone())
        .unwrap_or_default()
}

pub(crate) fn recovery_capabilities_for(
    recovery: &ProviderRecovery,
    pending: &[ProviderRecovery],
    children_settled: bool,
) -> RecoveryCapabilities {
    let blocked = |reason: &str| RecoveryCapabilities {
        retry_observation: false,
        cancel_preflight: false,
        reason: reason.into(),
    };
    let Some(retained) = pending
        .iter()
        .find(|record| same_recovery(record, recovery))
    else {
        return blocked("Stale request: no exact retained recovery matches");
    };
    if !children_settled {
        return blocked("An owned child remains unsettled; observe child settlement first");
    }
    let whole_request_preflight = recovery.phase == RecoveryPhase::Preflight
        && pending
            .iter()
            .filter(|record| record.operation_id == recovery.operation_id)
            .all(|record| record.phase == RecoveryPhase::Preflight);
    if recovery.phase == RecoveryPhase::Preflight && !whole_request_preflight {
        return blocked("Another target in this request has dispatched or unknown work");
    }
    if recovery.phase == RecoveryPhase::DispatchedOrUnknown
        && matches!(
            recovery.expected,
            ProviderPostcondition::ProviderStateReadable {
                provider: ProviderKind::WallpaperEngine,
                location: None,
                ..
            }
        )
    {
        return blocked(
            "Dispatched recovery has no original Engine binding; fresh rebinding is forbidden",
        );
    }
    let unsupported = matches!(
        recovery.expected,
        ProviderPostcondition::ReadbackUnavailable { .. }
            | ProviderPostcondition::EngineAbsent { .. }
    ) || retained
        .observation
        .as_ref()
        .is_some_and(|observation| observation.state == EngineObservationState::Unsupported);
    RecoveryCapabilities {
        retry_observation: !unsupported,
        cancel_preflight: whole_request_preflight,
        reason: if matches!(
            recovery.expected,
            ProviderPostcondition::EngineAbsent { .. }
        ) {
            "Engine absence has no established provider schema; dispatched close recovery remains blocked".into()
        } else if unsupported {
            "Independent observation is unsupported; the retained request remains unresolved".into()
        } else if whole_request_preflight
            && recovery.expected == ProviderPostcondition::EngineControlAvailable
        {
            "Engine control availability is unobserved; the unstarted control request is retained"
                .into()
        } else if whole_request_preflight {
            "Provider state is Unknown for the same stable display; the unstarted request is retained".into()
        } else {
            "The provider could not confirm the requested change. Restore the requested state on the original display, then use Recheck provider in display settings.".into()
        },
    }
}

pub(crate) fn provider_recheck_available(recovery: &ProviderRecovery) -> bool {
    unresolved_provider_operations()
        .lock()
        .is_ok_and(|pending| {
            pending.contains(recovery)
                && recovery_capabilities_for(
                    recovery,
                    &pending,
                    engine_ipc::ensure_mutation_allowed().is_ok(),
                )
                .retry_observation
        })
}

pub(crate) fn recovery_capabilities(recovery: &ProviderRecovery) -> RecoveryCapabilities {
    match unresolved_provider_operations().lock() {
        Ok(pending) => recovery_capabilities_for(
            recovery,
            &pending,
            engine_ipc::ensure_mutation_allowed().is_ok(),
        ),
        Err(_) => RecoveryCapabilities {
            retry_observation: false,
            cancel_preflight: false,
            reason: "Provider recovery gate was poisoned".into(),
        },
    }
}

pub(crate) fn shutdown_action_for(
    pending: &[ProviderRecovery],
    active: bool,
    children_settled: bool,
) -> ShutdownAction {
    if active {
        return ShutdownAction::WaitForActive;
    }
    if !children_settled {
        return ShutdownAction::Blocked("An owned child remains unsettled".into());
    }
    if pending.is_empty() {
        return ShutdownAction::Ready;
    }
    if let Some(recovery) = pending.iter().find(|recovery| {
        recovery_capabilities_for(recovery, pending, children_settled).cancel_preflight
    }) {
        return ShutdownAction::CancelPreflight(recovery.clone());
    }
    ShutdownAction::Blocked(
        pending
            .iter()
            .map(|recovery| recovery_capabilities_for(recovery, pending, children_settled).reason)
            .collect::<Vec<_>>()
            .join("; "),
    )
}

fn cancel_preflight_record(
    pending: &mut Vec<ProviderRecovery>,
    recovery: &ProviderRecovery,
    children_settled: bool,
) -> Result<(), String> {
    let capability = recovery_capabilities_for(recovery, pending, children_settled);
    if !capability.cancel_preflight {
        return Err(capability.reason);
    }
    // Preserve the last Unreadable/Unsupported observation in the returned command.
    // This abandons only this exact unstarted work; it confirms no provider state.
    if settle_matching_recovery(pending, recovery) {
        Ok(())
    } else {
        Err("Retained request changed before cancellation".into())
    }
}

fn cancel_preflight(recovery: &ProviderRecovery) -> Vec<MonitorApplyOutcome> {
    let result = unresolved_provider_operations()
        .lock()
        .map_err(|_| "Provider recovery gate was poisoned".to_string())
        .and_then(|mut pending| {
            cancel_preflight_record(
                &mut pending,
                recovery,
                engine_ipc::ensure_mutation_allowed().is_ok(),
            )
        });
    match result {
        Ok(()) => vec![not_attempted_outcome(
            recovery.target_id.clone(),
            "Unstarted request cancelled; provider observation remains unconfirmed".into(),
        )],
        Err(error) => vec![unknown_outcome(recovery.target_id.clone(), error)],
    }
}

fn ensure_provider_recovery_gate() -> Result<(), String> {
    let pending = unresolved_provider_operations()
        .lock()
        .map_err(|_| "provider-state-unknown: provider recovery gate was poisoned".to_string())?;
    pending.first().map_or(Ok(()), |recovery| {
        Err(format!(
            "provider-state-unknown: {} state for target {} remains unresolved; further provider changes remain blocked (operation {}, identity {})",
            match recovery.provider {
                ProviderKind::Native => "native wallpaper",
                ProviderKind::WallpaperEngine => "Wallpaper Engine",
            },
            recovery.target_id,
            recovery.operation_id,
            recovery.identity
        ))
    })
}

fn recovery_for_outcomes(outcomes: &[MonitorApplyOutcome]) -> Vec<ProviderRecovery> {
    if !outcomes
        .iter()
        .any(|outcome| outcome.state == ApplyOperationState::Unknown)
    {
        return Vec::new();
    }
    let pending = retained_provider_recoveries();
    if pending.is_empty() {
        return pending;
    }
    let target_ids: Vec<_> = outcomes
        .iter()
        .map(|outcome| outcome.target_id.as_str())
        .collect();
    let matching: Vec<_> = pending
        .iter()
        .filter(|recovery| {
            target_ids
                .iter()
                .any(|target_id| target_id.eq_ignore_ascii_case(&recovery.target_id))
                || recovery.target_id == "wallpaper-engine-global"
        })
        .cloned()
        .collect();
    if matching.is_empty() {
        pending
    } else {
        matching
    }
}

fn engine_apply_recovery(
    operation_id: OperationId,
    wallpaper: &WallpaperItem,
    target_id: &str,
    location: &str,
    reason: String,
) -> ProviderRecovery {
    let path = wallpaper.engine_target().to_path_buf();
    ProviderRecovery {
        operation_id: operation_id.get(),
        wallpaper_id: Some(wallpaper.id.clone()),
        target_id: target_id.to_string(),
        provider: ProviderKind::WallpaperEngine,
        identity: path.display().to_string(),
        expected: ProviderPostcondition::EngineProject {
            location: location.to_string(),
            path,
        },
        reason,
        phase: RecoveryPhase::DispatchedOrUnknown,
        observation: None,
    }
}

fn close_recovery(
    operation_id: OperationId,
    wallpaper_id: String,
    target_id: &str,
    location: &str,
    reason: String,
) -> ProviderRecovery {
    ProviderRecovery {
        operation_id: operation_id.get(),
        wallpaper_id: Some(wallpaper_id),
        target_id: target_id.to_string(),
        provider: ProviderKind::WallpaperEngine,
        identity: location.to_string(),
        expected: ProviderPostcondition::EngineAbsent {
            location: location.to_string(),
        },
        reason,
        phase: RecoveryPhase::DispatchedOrUnknown,
        observation: None,
    }
}

fn native_recovery(
    operation_id: OperationId,
    wallpaper_id: Option<String>,
    device_id: &str,
    path: PathBuf,
    position: i32,
    reason: String,
) -> ProviderRecovery {
    ProviderRecovery {
        operation_id: operation_id.get(),
        wallpaper_id,
        target_id: device_id.to_string(),
        provider: ProviderKind::Native,
        identity: path.display().to_string(),
        expected: ProviderPostcondition::NativeWallpaper {
            device_id: device_id.to_string(),
            path,
            position,
        },
        reason,
        phase: RecoveryPhase::DispatchedOrUnknown,
        observation: None,
    }
}

fn engine_control_recovery(
    operation_id: OperationId,
    identity: String,
    target_id: &str,
    reason: String,
) -> ProviderRecovery {
    ProviderRecovery {
        operation_id: operation_id.get(),
        wallpaper_id: None,
        target_id: target_id.to_string(),
        provider: ProviderKind::WallpaperEngine,
        identity,
        expected: ProviderPostcondition::ReadbackUnavailable {
            capability: reason.clone(),
        },
        reason,
        phase: RecoveryPhase::DispatchedOrUnknown,
        observation: None,
    }
}

fn engine_state_unavailable_recovery(
    operation_id: OperationId,
    wallpaper_id: Option<String>,
    target_id: &str,
    reason: String,
) -> ProviderRecovery {
    ProviderRecovery {
        operation_id: operation_id.get(),
        wallpaper_id,
        target_id: target_id.to_string(),
        provider: ProviderKind::WallpaperEngine,
        identity: format!("engine-state:{target_id}"),
        expected: ProviderPostcondition::ReadbackUnavailable {
            capability: reason.clone(),
        },
        reason,
        phase: RecoveryPhase::DispatchedOrUnknown,
        observation: None,
    }
}

fn provider_state_readback_recovery(
    operation_id: OperationId,
    wallpaper: &WallpaperItem,
    target_id: &str,
    provider: ProviderKind,
    reason: String,
) -> ProviderRecovery {
    provider_state_readback_recovery_at(operation_id, wallpaper, target_id, provider, None, reason)
}

fn provider_state_readback_recovery_at(
    operation_id: OperationId,
    wallpaper: &WallpaperItem,
    target_id: &str,
    provider: ProviderKind,
    location: Option<&str>,
    reason: String,
) -> ProviderRecovery {
    ProviderRecovery {
        operation_id: operation_id.get(),
        wallpaper_id: Some(wallpaper.id.clone()),
        target_id: target_id.to_string(),
        provider,
        identity: format!("state-readback:{target_id}"),
        expected: ProviderPostcondition::ProviderStateReadable {
            provider,
            target_id: target_id.to_string(),
            location: location.map(str::to_string),
        },
        reason,
        phase: RecoveryPhase::DispatchedOrUnknown,
        observation: None,
    }
}

fn preflight_readback_recovery(
    operation_id: OperationId,
    wallpaper: &WallpaperItem,
    target_id: &str,
    provider: ProviderKind,
    location: Option<&str>,
    observation: Option<EngineObservation>,
    context: &ApplyContext,
    children_settled: bool,
    reason: String,
) -> ProviderRecovery {
    let mut recovery = provider_state_readback_recovery_at(
        operation_id,
        wallpaper,
        target_id,
        provider,
        location,
        reason,
    );
    recovery.phase = context.recovery_phase(children_settled);
    recovery.observation = observation;
    recovery
}

impl WallpaperCoordinator {
    pub fn spawn<F>(on_finished: F) -> Self
    where
        F: Fn(CoordinatorResult) + Send + 'static,
    {
        let (sender, receiver) = mpsc::sync_channel::<CoordinatorRequest>(1);
        let pending_requests = Arc::new(AtomicUsize::new(0));
        let worker_pending = pending_requests.clone();
        std::thread::Builder::new()
            .name("wallpaper-change-coordinator".to_string())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    let owned_request = PendingRequest(worker_pending.clone());
                    let result = match request {
                        CoordinatorRequest::Apply(request) => {
                            let operation_id = request.operation_id.get();
                            crate::apply_trace::event_for(
                                operation_id,
                                "worker-receipt",
                                "request=apply dequeued=true",
                            );
                            match catch_unwind(AssertUnwindSafe(|| {
                                crate::apply_trace::with_operation(operation_id, || {
                                    run_operation(request)
                                })
                            })) {
                                Ok(result) => CoordinatorResult::Apply(result),
                                Err(panic) => {
                                    crate::apply_trace::event_for(
                                        operation_id,
                                        "worker-panic",
                                        "request=apply action=resume-unwind",
                                    );
                                    std::panic::resume_unwind(panic);
                                }
                            }
                        }
                        CoordinatorRequest::Engine(request) => {
                            CoordinatorResult::Engine(run_engine_operation(request))
                        }
                    };
                    drop(owned_request);
                    on_finished(result);
                }
            })
            .expect("wallpaper coordinator thread");
        Self {
            sender,
            pending_requests,
        }
    }

    pub fn submit(
        &self,
        wallpaper: WallpaperItem,
        target: ApplyTarget,
        transition: TransitionConfig,
    ) -> Result<ApplySubmission, String> {
        let operation_id = next_operation_id();
        let wallpaper_id = wallpaper.id.clone();
        let target_ids = requested_target_ids(&target);
        let trace_details = format!(
            "queue_attempt=true wallpaper_id={:?} path={:?} targets={:?} effect={} duration_ms={} off={}",
            wallpaper_id,
            wallpaper.file_target,
            target_ids,
            transition.effect.key(),
            transition.duration_ms,
            transition.effect == TransitionEffect::Off,
        );
        crate::apply_trace::event_for(operation_id.get(), "submission", trace_details);
        self.pending_requests.fetch_add(1, Ordering::AcqRel);
        match self
            .sender
            .try_send(CoordinatorRequest::Apply(ApplyRequest {
                operation_id,
                wallpaper,
                target,
                transition,
            })) {
            Ok(()) => Ok(ApplySubmission {
                operation_id,
                wallpaper_id,
                target_ids,
                state: ApplyOperationState::CommandAccepted,
            }),
            Err(TrySendError::Full(_)) => {
                self.pending_requests.fetch_sub(1, Ordering::AcqRel);
                crate::apply_trace::event_for(
                    operation_id.get(),
                    "failure",
                    "stage=submission reason=queue-full",
                );
                Err("Another wallpaper change is already in progress".to_string())
            }
            Err(TrySendError::Disconnected(_)) => {
                self.pending_requests.fetch_sub(1, Ordering::AcqRel);
                crate::apply_trace::event_for(
                    operation_id.get(),
                    "failure",
                    "stage=submission reason=coordinator-disconnected",
                );
                Err("The wallpaper coordinator is unavailable".to_string())
            }
        }
    }

    pub fn submit_engine_command(&self, command: EngineCommand) -> Result<OperationId, String> {
        let operation_id = next_operation_id();
        self.pending_requests.fetch_add(1, Ordering::AcqRel);
        match self
            .sender
            .try_send(CoordinatorRequest::Engine(EngineRequest {
                operation_id,
                command,
            })) {
            Ok(()) => Ok(operation_id),
            Err(TrySendError::Full(_)) => {
                self.pending_requests.fetch_sub(1, Ordering::AcqRel);
                Err("Another wallpaper operation is already in progress".to_string())
            }
            Err(TrySendError::Disconnected(_)) => {
                self.pending_requests.fetch_sub(1, Ordering::AcqRel);
                Err("The wallpaper coordinator is unavailable".to_string())
            }
        }
    }

    pub(crate) fn shutdown_action(&self) -> ShutdownAction {
        if self.pending_requests.load(Ordering::Acquire) != 0 {
            return ShutdownAction::WaitForActive;
        }
        match unresolved_provider_operations().lock() {
            Ok(pending) => shutdown_action_for(
                &pending,
                false,
                engine_ipc::ensure_mutation_allowed().is_ok(),
            ),
            Err(_) => ShutdownAction::Blocked("Provider recovery gate was poisoned".into()),
        }
    }
}

fn run_engine_operation(request: EngineRequest) -> EngineControlResult {
    let outcomes = match &request.command {
        EngineCommand::ReconcileProvider { recovery } => reconcile_provider(recovery),
        EngineCommand::CancelPreflight { recovery } => cancel_preflight(recovery),
        _ => match ensure_mutation_allowed() {
            Ok(()) => match &request.command {
                command @ (EngineCommand::SetPaused(_)
                | EngineCommand::SetMuted(_)
                | EngineCommand::SetVolume(_)) => global_control_with(
                    request.operation_id,
                    command,
                    engine_ipc::availability(),
                    || engine_ipc::ensure_mutation_allowed().is_ok(),
                    || match command {
                        EngineCommand::SetPaused(value) => engine_ipc::set_paused(*value),
                        EngineCommand::SetMuted(value) => engine_ipc::set_muted(*value),
                        EngineCommand::SetVolume(value) => engine_ipc::set_volume(*value),
                        _ => unreachable!(),
                    },
                    retain_provider_recovery,
                ),
                EngineCommand::ApplyProperties {
                    wallpaper_id,
                    project_json_path,
                    properties,
                    target,
                } => apply_engine_properties(
                    request.operation_id,
                    wallpaper_id,
                    project_json_path,
                    properties,
                    target,
                ),
                EngineCommand::ReconcileProvider { .. } | EngineCommand::CancelPreflight { .. } => {
                    unreachable!()
                }
            },
            Err(error) => blocked_engine_outcomes(&request.command, error),
        },
    };
    retain_engine_recoveries(request.operation_id, &request.command, &outcomes);
    let recoveries = recovery_for_outcomes(&outcomes);
    EngineControlResult {
        operation_id: request.operation_id,
        command: request.command,
        outcomes,
        recoveries,
    }
}

fn ensure_mutation_allowed() -> Result<(), String> {
    engine_ipc::ensure_mutation_allowed()?;
    ensure_provider_recovery_gate()
}

fn global_engine_outcome(result: Result<(), String>) -> Vec<MonitorApplyOutcome> {
    vec![match result {
        Ok(()) => accepted_outcome("wallpaper-engine-global".to_string()),
        Err(error) if provider_state_is_unknown(&error) => {
            unknown_outcome("wallpaper-engine-global".to_string(), error)
        }
        Err(error) => failed_outcome("wallpaper-engine-global".to_string(), error),
    }]
}

fn global_control_with(
    operation_id: OperationId,
    command: &EngineCommand,
    availability: engine_ipc::EngineAvailability,
    children_settled: impl Fn() -> bool,
    mutate: impl FnOnce() -> Result<(), engine_ipc::EngineControlFailure>,
    mut retain: impl FnMut(ProviderRecovery),
) -> Vec<MonitorApplyOutcome> {
    let mut retain_preflight = |reason: String| {
        let mut recovery = engine_control_recovery(
            operation_id,
            format!("{command:?}"),
            "wallpaper-engine-global",
            reason,
        );
        if children_settled() {
            recovery.phase = RecoveryPhase::Preflight;
            recovery.expected = ProviderPostcondition::EngineControlAvailable;
        }
        retain(recovery);
    };
    match availability {
        engine_ipc::EngineAvailability::ControlAvailable if children_settled() => match mutate() {
            Ok(()) => global_engine_outcome(Ok(())),
            Err(engine_ipc::EngineControlFailure::PreflightObservationFailed(error)) => {
                retain_preflight(error.clone());
                vec![unknown_outcome("wallpaper-engine-global".into(), error)]
            }
            Err(engine_ipc::EngineControlFailure::BeforeDispatch(error)) => {
                vec![failed_outcome("wallpaper-engine-global".into(), error)]
            }
            Err(engine_ipc::EngineControlFailure::DispatchedOrUnknown(error)) => {
                vec![unknown_outcome("wallpaper-engine-global".into(), error)]
            }
        },
        engine_ipc::EngineAvailability::Unavailable if children_settled() => vec![failed_outcome(
            "wallpaper-engine-global".into(),
            "Wallpaper Engine is not running".into(),
        )],
        _ => {
            retain_preflight("Engine control unavailable before dispatch".into());
            vec![unknown_outcome(
                "wallpaper-engine-global".into(),
                "Engine control preflight failed; no mutation was dispatched".into(),
            )]
        }
    }
}

fn control_preflight_readback(
    phase: RecoveryPhase,
    children_settled: bool,
    availability: engine_ipc::EngineAvailability,
) -> Result<(), String> {
    if phase != RecoveryPhase::Preflight || !children_settled {
        return Err(
            "Only settled zero-dispatch work can use control availability as preflight observation"
                .into(),
        );
    }
    if availability == engine_ipc::EngineAvailability::ControlAvailable {
        Ok(())
    } else {
        Err("Engine control remains unavailable; the unstarted request is retained".into())
    }
}

fn blocked_engine_outcomes(command: &EngineCommand, error: String) -> Vec<MonitorApplyOutcome> {
    let target_ids = match command {
        EngineCommand::ApplyProperties { target, .. } => requested_target_ids(target),
        EngineCommand::ReconcileProvider { recovery }
        | EngineCommand::CancelPreflight { recovery } => vec![recovery.target_id.clone()],
        EngineCommand::SetPaused(_) | EngineCommand::SetMuted(_) | EngineCommand::SetVolume(_) => {
            vec!["wallpaper-engine-global".to_string()]
        }
    };
    if target_ids.is_empty() {
        return vec![unknown_outcome(
            "wallpaper-engine-operation".to_string(),
            error,
        )];
    }
    unsettled_outcomes(&target_ids, 0, error)
}

fn has_retained_recovery(operation_id: OperationId, target_id: &str) -> bool {
    retained_provider_recoveries().iter().any(|recovery| {
        recovery.operation_id == operation_id.get()
            && recovery.target_id.eq_ignore_ascii_case(target_id)
    })
}

fn retain_apply_recoveries(
    operation_id: OperationId,
    request: &ApplyRequest,
    outcomes: &[MonitorApplyOutcome],
) {
    if !retained_provider_recoveries().is_empty() {
        return;
    }
    for outcome in outcomes {
        if outcome.state != ApplyOperationState::Unknown
            || has_retained_recovery(operation_id, &outcome.target_id)
        {
            continue;
        }
        let provider = match request.wallpaper.source {
            WallpaperSource::LocalImage => ProviderKind::Native,
            WallpaperSource::LocalVideo | WallpaperSource::WallpaperEngine => {
                ProviderKind::WallpaperEngine
            }
        };
        retain_provider_recovery(provider_state_readback_recovery(
            operation_id,
            &request.wallpaper,
            &outcome.target_id,
            provider,
            outcome
                .error
                .clone()
                .unwrap_or_else(|| "provider state did not settle".to_string()),
        ));
    }
}

fn retain_engine_recoveries(
    operation_id: OperationId,
    command: &EngineCommand,
    outcomes: &[MonitorApplyOutcome],
) {
    if matches!(
        command,
        EngineCommand::ReconcileProvider { .. } | EngineCommand::CancelPreflight { .. }
    ) {
        return;
    }
    if !retained_provider_recoveries().is_empty() {
        return;
    }
    for outcome in outcomes {
        if outcome.state != ApplyOperationState::Unknown
            || has_retained_recovery(operation_id, &outcome.target_id)
        {
            continue;
        }
        let (identity, reason) = match command {
            EngineCommand::SetPaused(value) => (
                format!("pause:{value}"),
                "Wallpaper Engine pause/play value readback is unavailable".to_string(),
            ),
            EngineCommand::SetMuted(value) => (
                format!("mute:{value}"),
                "Wallpaper Engine mute value readback is unavailable".to_string(),
            ),
            EngineCommand::SetVolume(value) => (
                format!("volume:{}", (*value).min(100)),
                "Wallpaper Engine volume readback is unavailable".to_string(),
            ),
            EngineCommand::ApplyProperties {
                wallpaper_id,
                project_json_path,
                ..
            } => (
                format!("{wallpaper_id}:{}", project_json_path.display()),
                "Wallpaper Engine property-value readback is unavailable".to_string(),
            ),
            EngineCommand::ReconcileProvider { .. } | EngineCommand::CancelPreflight { .. } => {
                unreachable!()
            }
        };
        retain_provider_recovery(engine_control_recovery(
            operation_id,
            identity,
            &outcome.target_id,
            outcome.error.clone().unwrap_or(reason),
        ));
    }
}

fn apply_engine_properties(
    operation_id: OperationId,
    wallpaper_id: &str,
    project_json_path: &std::path::Path,
    properties: &serde_json::Map<String, serde_json::Value>,
    target: &ApplyTarget,
) -> Vec<MonitorApplyOutcome> {
    let resolutions = match resolve_target_monitors(target) {
        Ok(resolutions) => resolutions,
        Err(error) => {
            return requested_target_ids(target)
                .into_iter()
                .map(|target_id| failed_outcome(target_id, error.clone()))
                .collect();
        }
    };
    let mut outcomes = Vec::with_capacity(resolutions.len());
    let context = ApplyContext::default();
    let mut stopped_after_unknown = false;
    for resolution in resolutions {
        if stopped_after_unknown {
            outcomes.push(not_attempted_outcome(
                resolution.target_id,
                "not attempted after an unknown Wallpaper Engine provider state".to_string(),
            ));
            continue;
        }
        let outcome = if let Some(monitor) = resolution.monitor {
            let target_id = resolution.target_id;
            let retain_observation_failure =
                |location: Option<&str>, observation: Option<EngineObservation>, reason: String| {
                    let mut recovery = engine_control_recovery(
                        operation_id,
                        format!("properties:{wallpaper_id}"),
                        &target_id,
                        reason,
                    );
                    recovery.wallpaper_id = Some(wallpaper_id.into());
                    recovery.phase =
                        context.recovery_phase(engine_ipc::ensure_mutation_allowed().is_ok());
                    if recovery.phase == RecoveryPhase::Preflight {
                        recovery.expected = ProviderPostcondition::ProviderStateReadable {
                            provider: ProviderKind::WallpaperEngine,
                            target_id: target_id.clone(),
                            location: location.map(str::to_string),
                        };
                    }
                    recovery.observation = observation;
                    retain_provider_recovery(recovery);
                };
            let result = (|| {
                match engine_ipc::availability() {
                    engine_ipc::EngineAvailability::ControlAvailable => {}
                    engine_ipc::EngineAvailability::Unavailable => {
                        return Err("Wallpaper Engine is not running".to_string());
                    }
                    engine_ipc::EngineAvailability::ProcessOnly
                    | engine_ipc::EngineAvailability::Unreadable => {
                        retain_observation_failure(
                            None,
                            None,
                            "Engine control unavailable before property dispatch".into(),
                        );
                        return Err(
                            "provider-state-unknown: Wallpaper Engine control is unavailable"
                                .to_string(),
                        );
                    }
                }
                let location = engine_location_for_device(&monitor.device_id).map_err(|error| {
                    retain_observation_failure(None, None, error.clone());
                    format!("provider-state-unknown: {error}")
                })?;
                let observation = engine_ipc::observe_wallpaper(&EngineTarget::bound(
                    &monitor.device_id,
                    &location,
                ));
                let active = match &observation.state {
                    EngineObservationState::Present(active)
                        if observation
                            .adequate_for(&EngineTarget::bound(&monitor.device_id, &location))
                            .is_ok() =>
                    {
                        active
                    }
                    EngineObservationState::ConfirmedAbsent
                        if observation
                            .adequate_for(&EngineTarget::bound(&monitor.device_id, &location))
                            .is_ok() =>
                    {
                        return Err(format!(
                            "Wallpaper Engine target {location} has no active wallpaper"
                        ));
                    }
                    _ => {
                        retain_observation_failure(
                            Some(&location),
                            Some(observation.clone()),
                            observation.description(),
                        );
                        return Err(format!(
                            "provider-state-unknown: {}",
                            observation.description()
                        ));
                    }
                };
                if !engine_ipc::same_engine_project(project_json_path, active) {
                    return Err(format!(
                        "Wallpaper Engine target {location} no longer has selected project {wallpaper_id:?} active (reported {})",
                        active.display()
                    ));
                }
                context.mark_mutation_dispatched();
                engine_ipc::apply_properties(properties, &location)
            })();
            match result {
                Ok(()) => accepted_outcome(target_id),
                Err(error) if provider_state_is_unknown(&error) => {
                    unknown_outcome(target_id, error)
                }
                Err(error) => failed_outcome(target_id, error),
            }
        } else {
            failed_outcome(
                resolution.target_id,
                resolution
                    .error
                    .unwrap_or_else(|| "The target monitor is unavailable".to_string()),
            )
        };
        stopped_after_unknown = outcome.state == ApplyOperationState::Unknown;
        outcomes.push(outcome);
    }
    outcomes
}

fn reconcile_provider(recovery: &ProviderRecovery) -> Vec<MonitorApplyOutcome> {
    let capability = recovery_capabilities(recovery);
    if !capability.retry_observation {
        return vec![unknown_outcome(
            recovery.target_id.clone(),
            capability.reason,
        )];
    }
    let readback = reconcile_provider_readback(recovery);
    match unresolved_provider_operations().lock() {
        Ok(mut pending) => finish_reconciliation(&mut pending, recovery, readback),
        Err(_) => vec![unknown_outcome(
            recovery.target_id.clone(),
            "Provider recovery gate was poisoned".into(),
        )],
    }
}

fn finish_reconciliation(
    pending: &mut Vec<ProviderRecovery>,
    recovery: &ProviderRecovery,
    readback: Result<(), String>,
) -> Vec<MonitorApplyOutcome> {
    match readback {
        Ok(()) if settle_matching_recovery(pending, recovery) => {
            if recovery.phase == RecoveryPhase::Preflight {
                vec![not_attempted_outcome(recovery.target_id.clone(), "Preflight observation recovered; failed request remains unstarted. A new Apply requires fresh preflight".into())]
            } else {
                vec![confirmed_outcome(recovery.target_id.clone())]
            }
        }
        Ok(()) => vec![unknown_outcome(
            recovery.target_id.clone(),
            "Stale reconciliation cannot settle another request".into(),
        )],
        Err(error) => vec![unknown_outcome(recovery.target_id.clone(), error)],
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ProviderReadback {
    Engine(EngineObservation),
    Native { path: PathBuf, position: i32 },
    Unreadable(String),
}

fn readback_satisfies(
    expected: &ProviderPostcondition,
    observed: &ProviderReadback,
    captured_target: Option<&EngineTarget>,
) -> Result<(), String> {
    if let ProviderReadback::Engine(observation) = observed {
        let target = captured_target.ok_or("Engine readback has no captured target authority")?;
        observation
            .adequate_for(target)
            .map_err(|error| format!("{error}: {}", observation.description()))?;
    }
    match (expected, observed) {
        (
            ProviderPostcondition::EngineProject { path, location },
            ProviderReadback::Engine(EngineObservation { target, state: EngineObservationState::Present(actual), .. }),
        ) if target.location == *location && engine_ipc::same_engine_project(path, actual) => Ok(()),
        (ProviderPostcondition::EngineAbsent { location },
            ProviderReadback::Engine(EngineObservation { target, state: EngineObservationState::ConfirmedAbsent, .. }))
            if target.location == *location => Ok(()),
        (
            ProviderPostcondition::NativeWallpaper {
                path,
                position,
                ..
            },
            ProviderReadback::Native {
                path: actual_path,
                position: actual_position,
            },
        ) if native_wallpaper::normalized_wallpaper_path(path)
            == native_wallpaper::normalized_wallpaper_path(actual_path)
            && position == actual_position =>
        {
            Ok(())
        }
        (
            ProviderPostcondition::ProviderStateReadable { provider, target_id, location: Some(location) },
            ProviderReadback::Engine(EngineObservation { target, state: EngineObservationState::Present(_) | EngineObservationState::ConfirmedAbsent, .. }),
        ) if *provider == ProviderKind::WallpaperEngine && target.location == *location
            && target.device_id.as_deref().is_some_and(|id| id.eq_ignore_ascii_case(target_id)) => Ok(()),
        (
            ProviderPostcondition::ProviderStateReadable { provider, .. },
            ProviderReadback::Native { .. },
        ) if *provider == ProviderKind::Native => Ok(()),
        (ProviderPostcondition::ReadbackUnavailable { capability }, _) => Err(format!(
            "independent readback unavailable: {capability}"
        )),
        (_, ProviderReadback::Unreadable(error)) => Err(error.clone()),
        (expected, observed) => Err(format!(
            "provider readback did not match expected postcondition {expected:?}: observed {observed:?}"
        )),
    }
}

fn reconcile_provider_readback(recovery: &ProviderRecovery) -> Result<(), String> {
    engine_ipc::ensure_mutation_allowed()?;
    if recovery.expected == ProviderPostcondition::EngineControlAvailable {
        return control_preflight_readback(recovery.phase, true, engine_ipc::availability());
    }
    reconcile_provider_readback_with(
        recovery,
        true,
        || {
            native_wallpaper::monitors().map(|monitors| {
                monitors
                    .into_iter()
                    .map(|monitor| monitor.device_id)
                    .collect()
            })
        },
        engine_ipc::load_monitor_map,
        engine_ipc::observe_wallpaper,
        |device_id| {
            let path = native_wallpaper::get_wallpaper_for_device(device_id)?;
            let position = native_wallpaper::get_position()?;
            Ok(ProviderReadback::Native {
                path,
                position: position.0,
            })
        },
    )
}

fn recovery_location_from_map(
    recovery: &ProviderRecovery,
    device_ids: &[String],
    map: &engine_ipc::EngineMonitorMap,
) -> Result<String, String> {
    if device_ids
        .iter()
        .filter(|id| id.eq_ignore_ascii_case(&recovery.target_id))
        .count()
        != 1
    {
        return Err(format!(
            "Stable display {} is missing or ambiguous; the preflight request is retained",
            recovery.target_id
        ));
    }
    let location = map
        .location_for_device(&recovery.target_id)
        .ok_or_else(|| {
            format!(
                "No validated Engine mapping for stable display {}",
                recovery.target_id
            )
        })?;
    if recovery.phase == RecoveryPhase::DispatchedOrUnknown {
        let original = match &recovery.expected {
            ProviderPostcondition::EngineProject { location, .. }
            | ProviderPostcondition::EngineAbsent { location } => Some(location),
            ProviderPostcondition::ProviderStateReadable { location, .. } => location.as_ref(),
            _ => None,
        }
        .ok_or("Dispatched recovery has no original Engine binding")?;
        if location != *original {
            return Err(
                "Dispatched recovery mapping changed; original binding cannot be replaced".into(),
            );
        }
    }
    Ok(location)
}

fn reconcile_provider_readback_with(
    recovery: &ProviderRecovery,
    children_settled: bool,
    displays: impl FnOnce() -> Result<Vec<String>, String>,
    mapping: impl FnOnce() -> Result<engine_ipc::EngineMonitorMap, String>,
    mut observe: impl FnMut(&EngineTarget) -> EngineObservation,
    mut native: impl FnMut(&str) -> Result<ProviderReadback, String>,
) -> Result<(), String> {
    if !children_settled {
        return Err("Owned child remains unsettled".into());
    }
    if let ProviderPostcondition::ReadbackUnavailable { capability } = &recovery.expected {
        return Err(format!("Independent readback unavailable: {capability}"));
    }
    let device_ids = displays()?;
    if device_ids
        .iter()
        .filter(|id| id.eq_ignore_ascii_case(&recovery.target_id))
        .count()
        != 1
    {
        return Err(format!(
            "Stable display {} is missing or ambiguous",
            recovery.target_id
        ));
    }
    if recovery.provider == ProviderKind::WallpaperEngine {
        let location = recovery_location_from_map(recovery, &device_ids, &mapping()?)?;
        let target = EngineTarget::bound(&recovery.target_id, &location);
        let observed = observe(&target);
        crate::apply_trace::event("recovery-observation", observed.description());
        let expected = if recovery.phase == RecoveryPhase::Preflight {
            ProviderPostcondition::ProviderStateReadable {
                provider: recovery.provider,
                target_id: recovery.target_id.clone(),
                location: Some(location),
            }
        } else {
            recovery.expected.clone()
        };
        readback_satisfies(
            &expected,
            &ProviderReadback::Engine(observed),
            Some(&target),
        )
    } else {
        readback_satisfies(&recovery.expected, &native(&recovery.target_id)?, None)
    }
}

fn provider_state_is_unknown(error: &str) -> bool {
    error.starts_with("provider-state-unknown:")
}

fn requested_target_ids(target: &ApplyTarget) -> Vec<String> {
    match target {
        ApplyTarget::DeviceId(device_id) => vec![device_id.clone()],
        ApplyTarget::DeviceIds(device_ids) => device_ids.clone(),
        ApplyTarget::Monitor(rect) => vec![format!(
            "rect:{},{},{},{}",
            rect.left, rect.top, rect.right, rect.bottom
        )],
        ApplyTarget::All => vec!["all-monitors".to_string()],
    }
}

#[allow(dead_code)]
pub fn resolve_explicit_apply_target(requested: &str) -> Result<ApplyTarget, String> {
    let device_ids = native_wallpaper::monitors()?
        .into_iter()
        .map(|monitor| monitor.device_id)
        .collect::<Vec<_>>();
    resolve_explicit_apply_target_from_device_ids(requested, &device_ids)
}

pub(crate) fn resolve_explicit_apply_target_from_device_ids(
    requested: &str,
    device_ids: &[String],
) -> Result<ApplyTarget, String> {
    if device_ids.is_empty() {
        return Err("No Windows wallpaper monitors are available".to_string());
    }
    if requested.eq_ignore_ascii_case("all") {
        return Ok(ApplyTarget::All);
    }

    let matches = device_ids
        .iter()
        .filter(|device_id| device_id.eq_ignore_ascii_case(requested))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [device_id] => Ok(ApplyTarget::DeviceId((*device_id).clone())),
        [] => Err(format!(
            "Diagnostic target does not match an available device ID: {requested}"
        )),
        _ => Err(format!(
            "Diagnostic target is ambiguous across available device IDs: {requested}"
        )),
    }
}

fn run_operation(request: ApplyRequest) -> ApplyResult {
    let _native_cache_use = NATIVE_CACHE_USE
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let operation_id = request.operation_id;
    let wallpaper_id = request.wallpaper.id.clone();
    let context = ApplyContext::default();
    let (outcomes, mut transition) = match ensure_mutation_allowed() {
        Ok(()) => {
            crate::apply_trace::event("monitor-resolution-start", "enumerating=true");
            match resolve_target_monitors(&request.target) {
                Ok(resolutions) => {
                    let resolved = resolutions
                        .iter()
                        .filter(|resolution| resolution.monitor.is_some())
                        .count();
                    crate::apply_trace::event(
                        "monitor-resolution-complete",
                        format!("requested={} resolved={resolved}", resolutions.len()),
                    );
                    apply_with_outcomes(&request, resolutions, &context)
                }
                Err(error) => {
                    crate::apply_trace::event(
                        "monitor-resolution-failure",
                        format!("error={error:?}"),
                    );
                    (
                        requested_target_ids(&request.target)
                            .into_iter()
                            .map(|target_id| failed_outcome(target_id, error.clone()))
                            .collect(),
                        TransitionResult::not_attempted(
                            "target monitor resolution failed before transition preparation",
                        ),
                    )
                }
            }
        }
        Err(error) => {
            crate::apply_trace::event("failure", format!("stage=operation-gate error={error:?}"));
            (
                blocked_apply_outcomes(&request.target, error),
                TransitionResult::not_attempted("the mutation gate blocked transition preparation"),
            )
        }
    };
    for outcome in &outcomes {
        if !transition
            .displays
            .iter()
            .any(|display| display.display_id == outcome.target_id)
        {
            transition.displays.push(DisplayTransitionOutcome {
                display_id: outcome.target_id.clone(),
                status: transition.status,
                intermediate_submitted: false,
                error: transition.warnings.first().cloned(),
            });
        }
        let animation = transition
            .displays
            .iter()
            .find(|display| display.display_id == outcome.target_id);
        crate::apply_trace::event_lazy("transition-display-result", || {
            format!(
                "display={:?} provider={:?} provider_error={:?} animation={animation:?}",
                outcome.target_id, outcome.state, outcome.error
            )
        });
    }
    transition.displays.sort_by_key(|display| {
        outcomes
            .iter()
            .position(|outcome| outcome.target_id == display.display_id)
            .unwrap_or(usize::MAX)
    });
    retain_apply_recoveries(operation_id, &request, &outcomes);
    let recoveries = recovery_for_outcomes(&outcomes);
    let result = ApplyResult {
        operation_id,
        wallpaper_id,
        outcomes,
        recoveries,
        transition,
    };
    crate::apply_trace::event(
        "transition-result",
        format!(
            "status={} warnings={:?}",
            result.transition.status_name(),
            result.transition.warnings
        ),
    );
    if result.is_success() {
        crate::apply_trace::event(
            "completion",
            format!("provider_confirmed=true outcomes={:?}", result.outcomes),
        );
    } else {
        crate::apply_trace::event("failure", format!("outcomes={:?}", result.outcomes));
    }
    result
}

#[derive(Debug, Clone)]
struct TargetResolution {
    target_id: String,
    monitor: Option<native_wallpaper::NativeMonitor>,
    error: Option<String>,
}

fn failed_outcome(target_id: String, error: String) -> MonitorApplyOutcome {
    MonitorApplyOutcome {
        target_id,
        state: ApplyOperationState::Failed,
        error: Some(error),
    }
}

fn confirmed_outcome(target_id: String) -> MonitorApplyOutcome {
    MonitorApplyOutcome {
        target_id,
        state: ApplyOperationState::ProviderConfirmed,
        error: None,
    }
}

fn unknown_outcome(target_id: String, error: String) -> MonitorApplyOutcome {
    MonitorApplyOutcome {
        target_id,
        state: ApplyOperationState::Unknown,
        error: Some(error),
    }
}

fn accepted_outcome(target_id: String) -> MonitorApplyOutcome {
    MonitorApplyOutcome {
        target_id,
        state: ApplyOperationState::CommandAccepted,
        error: None,
    }
}

fn not_attempted_outcome(target_id: String, reason: String) -> MonitorApplyOutcome {
    MonitorApplyOutcome {
        target_id,
        state: ApplyOperationState::Cancelled,
        error: Some(reason),
    }
}

fn resolve_target_monitors(target: &ApplyTarget) -> Result<Vec<TargetResolution>, String> {
    let native_monitors = native_wallpaper::monitors()?;
    let missing = |target_id: String| TargetResolution {
        error: Some(format!(
            "The target monitor is no longer available: {target_id}"
        )),
        target_id,
        monitor: None,
    };
    let found = |monitor: native_wallpaper::NativeMonitor| TargetResolution {
        target_id: monitor.device_id.clone(),
        monitor: Some(monitor),
        error: None,
    };

    Ok(match target {
        ApplyTarget::All => native_monitors.into_iter().map(found).collect(),
        ApplyTarget::DeviceId(device_id) => vec![native_monitors
            .into_iter()
            .find(|monitor| monitor.device_id.eq_ignore_ascii_case(device_id))
            .map(found)
            .unwrap_or_else(|| missing(device_id.clone()))],
        ApplyTarget::DeviceIds(device_ids) => device_ids
            .iter()
            .map(|device_id| {
                native_monitors
                    .iter()
                    .find(|monitor| monitor.device_id.eq_ignore_ascii_case(device_id))
                    .cloned()
                    .map(found)
                    .unwrap_or_else(|| missing(device_id.clone()))
            })
            .collect(),
        ApplyTarget::Monitor(rect) => {
            let rects: Vec<_> = native_monitors.iter().map(|monitor| monitor.rect).collect();
            let target_id = format!(
                "rect:{},{},{},{}",
                rect.left, rect.top, rect.right, rect.bottom
            );
            vec![best_rect_match(*rect, &rects)
                .and_then(|index| native_monitors.get(index).cloned())
                .map(found)
                .unwrap_or_else(|| missing(target_id))]
        }
    })
}

fn engine_location_for_device(device_id: &str) -> Result<String, String> {
    engine_ipc::load_monitor_map()?
        .location_for_device(device_id)
        .ok_or_else(|| format!("Wallpaper Engine has no location mapping for {device_id}"))
}

#[derive(Default)]
struct ApplyContext {
    engine_monitor_map: OnceLock<Result<engine_ipc::EngineMonitorMap, String>>,
    mutation_dispatched: std::cell::Cell<bool>,
    engine_capabilities: engine_ipc::EngineCapabilities,
    confirmed_capture_owners: std::cell::RefCell<HashMap<String, (usize, u32)>>,
}

impl ApplyContext {
    fn recovery_phase(&self, children_settled: bool) -> RecoveryPhase {
        if !self.mutation_dispatched.get() && children_settled {
            RecoveryPhase::Preflight
        } else {
            RecoveryPhase::DispatchedOrUnknown
        }
    }

    fn mark_mutation_dispatched(&self) {
        // Set before invoking a mutating adapter, even if its dispatch outcome is unknown.
        self.mutation_dispatched.set(true);
    }
    fn engine_monitor_map(&self) -> Result<&engine_ipc::EngineMonitorMap, String> {
        self.engine_monitor_map_with(engine_ipc::load_monitor_map)
    }

    fn engine_monitor_map_with<F>(&self, load: F) -> Result<&engine_ipc::EngineMonitorMap, String>
    where
        F: FnOnce() -> Result<engine_ipc::EngineMonitorMap, String>,
    {
        self.engine_monitor_map
            .get_or_init(load)
            .as_ref()
            .map_err(Clone::clone)
    }

    fn engine_location_for_device(&self, device_id: &str) -> Result<String, String> {
        self.engine_monitor_map()?
            .location_for_device(device_id)
            .ok_or_else(|| format!("Wallpaper Engine has no location mapping for {device_id}"))
    }
}

fn apply_with_outcomes(
    request: &ApplyRequest,
    resolutions: Vec<TargetResolution>,
    context: &ApplyContext,
) -> (Vec<MonitorApplyOutcome>, TransitionResult) {
    let target_order: Vec<_> = resolutions
        .iter()
        .map(|resolution| resolution.target_id.clone())
        .collect();
    let mut outcomes = Vec::with_capacity(resolutions.len());
    let mut monitors = Vec::new();
    for resolution in resolutions {
        match resolution.monitor {
            Some(monitor) => monitors.push(monitor),
            None => outcomes.push(failed_outcome(
                resolution.target_id,
                resolution
                    .error
                    .unwrap_or_else(|| "The target monitor is unavailable".to_string()),
            )),
        }
    }
    if monitors.is_empty() {
        if outcomes.is_empty() {
            outcomes.push(failed_outcome(
                "all-monitors".to_string(),
                "No desktop monitors are available".to_string(),
            ));
        }
        return (
            outcomes,
            TransitionResult::not_attempted("no target monitor was available"),
        );
    }

    let order_outcomes = |mut outcomes: Vec<MonitorApplyOutcome>| {
        outcomes.sort_by_key(|outcome| {
            target_order
                .iter()
                .position(|target_id| target_id.eq_ignore_ascii_case(&outcome.target_id))
                .unwrap_or(usize::MAX)
        });
        outcomes
    };
    if matches!(
        request.wallpaper.source,
        WallpaperSource::WallpaperEngine | WallpaperSource::LocalVideo
    ) {
        if let Err(error) = engine_ipc::validate_project_file(request.wallpaper.engine_target()) {
            outcomes.extend(
                monitors
                    .iter()
                    .map(|monitor| failed_outcome(monitor.device_id.clone(), error.clone())),
            );
            return (
                order_outcomes(outcomes),
                TransitionResult::not_attempted("the provider target file was unavailable"),
            );
        }
    }
    let effect = request.transition.effect.resolve(random_sample());
    crate::apply_trace::event_lazy("transition-effect", || {
        format!(
            "configured={} effect={} duration_ms={} targets={target_order:?}",
            request.transition.effect.key(),
            effect.key(),
            request.transition.duration_ms,
        )
    });
    if effect == TransitionEffect::Off {
        outcomes.extend(apply_direct_to_monitors(request, monitors, context));
        return (order_outcomes(outcomes), TransitionResult::not_requested());
    }

    // A missing display has no outgoing frame and cannot confirm a target.
    if !outcomes.is_empty() {
        outcomes.extend(apply_direct_to_monitors(request, monitors, context));
        return (
            order_outcomes(outcomes),
            TransitionResult::not_attempted("an outgoing target display was unavailable"),
        );
    }

    match prepare_transition(request, monitors.clone(), context) {
        Ok(Some(prepared)) => {
            #[cfg(test)]
            eprintln!("trace: transition prepared");
            let duration = std::time::Duration::from_millis(request.transition.duration_ms);
            let overlay_windows: Vec<_> = prepared
                .outputs
                .iter()
                .flat_map(|(_, output)| output.as_ref())
                .map(|output| output.session.window_identity())
                .collect();
            // Every target receives the provider command, in target order.
            let (applied, transition) = settle_prepared_transition(
                prepared.outputs,
                || apply_direct_to_monitors(request, monitors, context),
                |mut output| {
                    let display = &output.monitor.device_id;
                    if output.incoming.is_none() {
                        output.session.ensure_wallpaper_layer()?;
                        let endpoint = context
                            .confirmed_capture_owners
                            .borrow()
                            .get(display)
                            .copied()
                            .ok_or(
                                "Wallpaper Engine did not confirm a capture owner for this display",
                            )?;
                        output.incoming = Some(engine_capture::capture_for_apply(
                            output.monitor.rect,
                            request.operation_id.get(),
                            display,
                            "incoming",
                            endpoint,
                            &overlay_windows,
                            Some(output.outgoing_fingerprint),
                        )?);
                    }
                    Ok(output)
                },
                |playable| play_synchronized(playable, effect, duration),
            );
            outcomes.extend(applied);
            return (order_outcomes(outcomes), transition);
        }
        Ok(None) => {
            outcomes.extend(apply_direct_to_monitors(request, monitors, context));
            return (
                order_outcomes(outcomes),
                TransitionResult::direct_fallback(
                    "the transition surface was unavailable; no animation was attempted",
                ),
            );
        }
        Err(error) => {
            crate::log_debug(&format!(
                "Transition unavailable; using direct switch fallback: {error}"
            ));
            outcomes.extend(apply_direct_to_monitors(request, monitors, context));
            return (
                order_outcomes(outcomes),
                TransitionResult::direct_fallback(error),
            );
        }
    }
}

#[cfg(test)]
fn apply(request: ApplyRequest) -> Result<(), String> {
    let result = run_operation(request);
    result.error_message().map_or(Ok(()), Err)
}

/// Dispatches every target, then animates the displays whose wallpaper changes.
/// `None` marks a display that already shows the requested wallpaper; it has
/// nothing to animate and reports `Unchanged` without a warning.
fn settle_prepared_transition<T>(
    targets: Vec<(String, Option<T>)>,
    dispatch: impl FnOnce() -> Vec<MonitorApplyOutcome>,
    capture: impl FnMut(T) -> Result<T, String>,
    play: impl FnOnce(&mut [T]) -> Vec<PlaybackProgress>,
) -> (Vec<MonitorApplyOutcome>, TransitionResult) {
    let providers = dispatch();
    let (changed, unchanged): (Vec<_>, Vec<_>) = targets
        .into_iter()
        .partition(|(_, output)| output.is_some());
    if changed.is_empty() {
        return (providers, TransitionResult::unchanged());
    }
    let mut transition = settle_incoming_transitions(
        &providers,
        changed
            .into_iter()
            .filter_map(|(display, output)| Some((display, output?))),
        capture,
        play,
    );
    transition
        .displays
        .extend(
            unchanged
                .into_iter()
                .map(|(display_id, _)| DisplayTransitionOutcome {
                    display_id,
                    status: TransitionStatus::Unchanged,
                    intermediate_submitted: false,
                    error: None,
                }),
        );
    (providers, transition)
}

fn settle_incoming_transitions<T>(
    providers: &[MonitorApplyOutcome],
    outputs: impl IntoIterator<Item = (String, T)>,
    mut capture: impl FnMut(T) -> Result<T, String>,
    play: impl FnOnce(&mut [T]) -> Vec<PlaybackProgress>,
) -> TransitionResult {
    if providers.is_empty()
        || providers.iter().any(|outcome| {
            outcome.state != ApplyOperationState::ProviderConfirmed || outcome.error.is_some()
        })
    {
        return TransitionResult::not_attempted(
            "transition playback was not started because the provider did not confirm every target",
        );
    }
    let mut playable = Vec::new();
    let mut targets = Vec::new();
    let mut displays = Vec::new();
    let mut warnings = Vec::new();
    for (display_id, output) in outputs {
        match capture(output) {
            Ok(output) => {
                targets.push(display_id);
                playable.push(output);
            }
            Err(error) => {
                let text = format!("{display_id}: incoming frame unavailable ({error})");
                crate::log_debug(&text);
                warnings.push(text);
                displays.push(DisplayTransitionOutcome {
                    display_id,
                    status: TransitionStatus::IncomingCaptureUnavailable,
                    intermediate_submitted: false,
                    error: Some(error),
                });
            }
        }
    }

    let incoming_missing = warnings.len();
    let playback = if playable.is_empty() {
        Vec::new()
    } else {
        play(&mut playable)
    };
    assert_eq!(playback.len(), targets.len());
    let mut playback_failed = false;
    for (display_id, progress) in targets.into_iter().zip(playback) {
        let status = if let Some(error) = &progress.error {
            playback_failed = true;
            warnings.push(format!("{display_id}: {error}"));
            TransitionStatus::PlaybackFailed
        } else {
            TransitionStatus::Completed
        };
        displays.push(DisplayTransitionOutcome {
            display_id,
            status,
            intermediate_submitted: progress.intermediate_submitted,
            error: progress.error,
        });
    }

    let mut result = if playable.is_empty() {
        TransitionResult::incoming_capture_unavailable(warnings)
    } else if playback_failed {
        TransitionResult::playback_failed(warnings)
    } else if incoming_missing > 0 {
        TransitionResult::partial_incoming_capture(warnings)
    } else {
        TransitionResult {
            status: TransitionStatus::Completed,
            warnings,
            displays: Vec::new(),
        }
    };
    result.displays = displays;
    result
}

#[derive(Debug, Default)]
struct PlaybackProgress {
    intermediate_submitted: bool,
    final_submitted: bool,
    error: Option<String>,
}

fn play_synchronized(
    outputs: &mut [PreparedOutput],
    effect: TransitionEffect,
    duration: std::time::Duration,
) -> Vec<PlaybackProgress> {
    let frame_interval = std::time::Duration::from_millis(16);
    let mut results: Vec<_> = outputs
        .iter()
        .map(|_| PlaybackProgress::default())
        .collect();
    for output in outputs.iter() {
        crate::apply_trace::event_lazy("transition-playback-start", || {
            format!(
                "display={:?} effect={} duration_ms={} {}",
                output.monitor.device_id,
                effect.key(),
                duration.as_millis(),
                output.session.placement_trace()
            )
        });
    }
    let started = std::time::Instant::now();
    // Frame count, last present and largest present gap per display (ms).
    let mut pacing = vec![(0u32, 0u128, 0u128); outputs.len()];
    let playback = (|| -> Result<(), (usize, String)> {
        loop {
            let frame_started = std::time::Instant::now();
            let elapsed = started.elapsed();
            let progress = if duration.is_zero() {
                1.0
            } else {
                (elapsed.as_secs_f32() / duration.as_secs_f32()).min(1.0)
            };
            for (index, output) in outputs.iter_mut().enumerate() {
                let incoming = output.incoming.as_ref().ok_or_else(|| {
                    (
                        index,
                        "A prepared transition output has no incoming frame".into(),
                    )
                })?;
                output
                    .session
                    .render(incoming, effect, progress)
                    .map_err(|error| (index, error))?;
            }
            for (index, output) in outputs.iter().enumerate() {
                output.session.present().map_err(|error| (index, error))?;
                let presented = started.elapsed().as_millis();
                let (frames, last, gap) = &mut pacing[index];
                if *frames > 0 {
                    *gap = (*gap).max(presented - *last);
                }
                *frames += 1;
                *last = presented;
                let result = &mut results[index];
                result.final_submitted = progress >= 1.0;
                if !result.intermediate_submitted && progress > 0.0 && progress < 1.0 {
                    result.intermediate_submitted = true;
                    crate::apply_trace::event_lazy("transition-intermediate-submitted", || {
                        format!(
                            "display={:?} progress={progress:.3} elapsed_ms={} {}",
                            output.monitor.device_id,
                            elapsed.as_millis(),
                            output.session.placement_trace()
                        )
                    });
                }
            }
            if progress >= 1.0 {
                return Ok(());
            }
            std::thread::sleep(frame_interval.saturating_sub(frame_started.elapsed()));
        }
    })();
    if let Err((failed, error)) = playback {
        let failed_display = &outputs[failed].monitor.device_id;
        crate::log_debug(&format!(
            "Transition playback failed for {failed_display}: {error}"
        ));
        for (index, result) in results.iter_mut().enumerate() {
            if index == failed {
                result.error = Some(error.clone());
            } else if !result.final_submitted {
                result.error = Some(format!(
                    "Playback stopped before the final frame after display {failed_display} failed"
                ));
            }
        }
    }
    for result in &mut results {
        if result.error.is_none() && !duration.is_zero() && !result.intermediate_submitted {
            result.error = Some(
                "The transition reached the final frame before submitting an intermediate frame"
                    .into(),
            );
        }
    }
    for (output, ((frames, last, gap), result)) in outputs.iter().zip(pacing.iter().zip(&results)) {
        crate::apply_trace::event_lazy("transition-playback-complete", || {
            format!(
                "display={:?} effect={} frames={frames} last_present_ms={last} max_gap_ms={gap} final_submitted={} error={:?}",
                output.monitor.device_id,
                effect.key(),
                result.final_submitted,
                result.error
            )
        });
    }
    results
}

fn random_sample() -> u64 {
    use std::sync::atomic::AtomicU64;

    static STATE: AtomicU64 = AtomicU64::new(0);
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0x9e37_79b9_7f4a_7c15, |duration| duration.as_nanos() as u64);
    next_random_sample(&STATE, seed)
}

fn next_random_sample(state: &std::sync::atomic::AtomicU64, seed: u64) -> u64 {
    use std::sync::atomic::Ordering;

    let mut current = state.load(Ordering::Relaxed);
    loop {
        let mut next = if current == 0 { seed.max(1) } else { current };
        next ^= next << 13;
        next ^= next >> 7;
        next ^= next << 17;
        if next == 0 {
            next = 0x9e37_79b9_7f4a_7c15;
        }
        match state.compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(actual) => current = actual,
        }
    }
}

struct PreparedTransition {
    // One slot per target in target order; `None` when the display already
    // shows the requested wallpaper.
    outputs: Vec<(String, Option<PreparedOutput>)>,
}

struct PreparedOutput {
    monitor: native_wallpaper::NativeMonitor,
    session: DesktopTransitionSession,
    incoming: Option<FrozenFrame>,
    outgoing_fingerprint: engine_capture::FrameFingerprint,
}

fn outgoing_frame(
    operation_id: OperationId,
    monitor: &native_wallpaper::NativeMonitor,
    context: &ApplyContext,
    overlay_windows: &[usize],
    requested: &std::path::Path,
) -> Result<Option<FrozenFrame>, String> {
    crate::apply_trace::event(
        "outgoing-capture-start",
        format!(
            "target={:?} width={} height={}",
            monitor.device_id,
            monitor.rect.width(),
            monitor.rect.height()
        ),
    );
    let result = outgoing_frame_inner(operation_id, monitor, context, overlay_windows, requested);
    match &result {
        Ok(Some(frame)) => crate::apply_trace::event(
            "outgoing-capture-complete",
            format!(
                "display={:?} width={} height={}",
                monitor.device_id, frame.width, frame.height
            ),
        ),
        Ok(None) => crate::apply_trace::event(
            "outgoing-capture-skipped",
            format!(
                "display={:?} reason=\"requested wallpaper already shown\"",
                monitor.device_id
            ),
        ),
        Err(error) => crate::apply_trace::event(
            "outgoing-capture-failure",
            format!("display={:?} error={error:?}", monitor.device_id),
        ),
    }
    result
}

fn outgoing_frame_inner(
    operation_id: OperationId,
    monitor: &native_wallpaper::NativeMonitor,
    context: &ApplyContext,
    overlay_windows: &[usize],
    requested: &std::path::Path,
) -> Result<Option<FrozenFrame>, String> {
    match engine_ipc::availability() {
        engine_ipc::EngineAvailability::ControlAvailable => {
            let location = context.engine_location_for_device(&monitor.device_id)?;
            let (observation, endpoint) = engine_ipc::observe_wallpaper_with_identity(
                &EngineTarget::bound(&monitor.device_id, &location),
            );
            match observation.state {
                EngineObservationState::Present(ref current)
                    if observation
                        .adequate_for(&EngineTarget::bound(&monitor.device_id, &location))
                        .is_ok() =>
                {
                    // Re-applying the wallpaper already on screen has no
                    // visible change to animate.
                    if engine_ipc::same_engine_project(requested, current) {
                        return Ok(None);
                    }
                    let endpoint = endpoint
                        .ok_or("Wallpaper Engine did not confirm an outgoing capture owner")?;
                    return engine_capture::capture_for_apply(
                        monitor.rect,
                        operation_id.get(),
                        &monitor.device_id,
                        "outgoing",
                        endpoint,
                        overlay_windows,
                        None,
                    )
                    .map(Some);
                }
                EngineObservationState::ConfirmedAbsent
                    if observation
                        .adequate_for(&EngineTarget::bound(&monitor.device_id, &location))
                        .is_ok() => {}
                _ => {
                    return Err(format!(
                        "provider-state-unknown: {}",
                        observation.description()
                    ));
                }
            }
        }
        engine_ipc::EngineAvailability::Unavailable => {}
        engine_ipc::EngineAvailability::ProcessOnly
        | engine_ipc::EngineAvailability::Unreadable => {
            return Err(
                "provider-state-unknown: Engine target state is unreadable while control is unavailable"
                    .to_string(),
            );
        }
    }
    let outgoing_path = native_wallpaper::get_wallpaper_for_device(&monitor.device_id)?;
    FrozenFrame::from_path_cover(&outgoing_path, monitor.rect.width(), monitor.rect.height())
        .map(Some)
}

fn prepare_transition(
    request: &ApplyRequest,
    targets: Vec<native_wallpaper::NativeMonitor>,
    context: &ApplyContext,
) -> Result<Option<PreparedTransition>, String> {
    let mut outputs = Vec::with_capacity(targets.len());
    let mut overlay_windows = Vec::with_capacity(targets.len());
    for monitor in targets {
        #[cfg(test)]
        eprintln!("trace: capturing outgoing frame");
        let Some(outgoing) = outgoing_frame(
            request.operation_id,
            &monitor,
            context,
            &overlay_windows,
            request.wallpaper.engine_target(),
        )
        .map_err(|error| {
            format!(
                "{}: outgoing frame unavailable ({error})",
                monitor.device_id
            )
        })?
        else {
            outputs.push((monitor.device_id, None));
            continue;
        };
        let incoming = match request.wallpaper.source {
            WallpaperSource::LocalImage => {
                crate::apply_trace::event(
                    "incoming-decode-start",
                    format!(
                        "display={:?} path={:?} bytes={} width={} height={}",
                        monitor.device_id,
                        request.wallpaper.file_target,
                        request
                            .wallpaper
                            .file_target
                            .metadata()
                            .map_or(0, |metadata| metadata.len()),
                        monitor.rect.width(),
                        monitor.rect.height()
                    ),
                );
                match FrozenFrame::from_path_cover(
                    &request.wallpaper.file_target,
                    monitor.rect.width(),
                    monitor.rect.height(),
                ) {
                    Ok(frame) => {
                        crate::apply_trace::event(
                            "incoming-decode-complete",
                            format!(
                                "display={:?} width={} height={}",
                                monitor.device_id, frame.width, frame.height
                            ),
                        );
                        Some(frame)
                    }
                    Err(error) => {
                        crate::apply_trace::event(
                            "incoming-decode-failure",
                            format!("display={:?} error={error:?}", monitor.device_id),
                        );
                        return Err(format!(
                            "{}: incoming image unavailable ({error})",
                            monitor.device_id
                        ));
                    }
                }
            }
            WallpaperSource::LocalVideo | WallpaperSource::WallpaperEngine => None,
        };
        #[cfg(test)]
        eprintln!("trace: showing desktop transition surface");
        let outgoing_fingerprint = engine_capture::FrameFingerprint::of(&outgoing);
        let session = DesktopTransitionSession::show(outgoing, monitor.rect)
            .map_err(|error| format!("{}: {error}", monitor.device_id))?;
        crate::apply_trace::event_lazy("transition-display-host", || {
            format!(
                "display={:?} target={:?} {}",
                monitor.device_id,
                monitor.rect,
                session.placement_trace()
            )
        });
        overlay_windows.push(session.window_identity());
        outputs.push((
            monitor.device_id.clone(),
            Some(PreparedOutput {
                monitor,
                session,
                incoming,
                outgoing_fingerprint,
            }),
        ));
    }
    Ok(Some(PreparedTransition { outputs }))
}

fn apply_direct_to_monitors(
    request: &ApplyRequest,
    monitors: Vec<native_wallpaper::NativeMonitor>,
    context: &ApplyContext,
) -> Vec<MonitorApplyOutcome> {
    apply_direct_to_monitors_with(
        request,
        monitors,
        context,
        monitor_provider_state,
        apply_direct_to_monitor,
        restore_monitor_provider_state,
        retain_provider_recovery,
        || engine_ipc::ensure_mutation_allowed().is_ok(),
    )
}

fn apply_direct_to_monitors_with(
    request: &ApplyRequest,
    monitors: Vec<native_wallpaper::NativeMonitor>,
    context: &ApplyContext,
    mut baseline: impl FnMut(
        &native_wallpaper::NativeMonitor,
        &ApplyContext,
    ) -> Result<MonitorProviderState, BaselineFailure>,
    mut apply_target: impl FnMut(
        &WallpaperItem,
        OperationId,
        &native_wallpaper::NativeMonitor,
        &ApplyContext,
        &[PathBuf],
    ) -> Result<(), ProviderFailure>,
    mut restore: impl FnMut(
        &native_wallpaper::NativeMonitor,
        &MonitorProviderState,
        &ApplyContext,
    ) -> Result<(), RestorationFailure>,
    mut retain: impl FnMut(ProviderRecovery),
    children_settled: impl Fn() -> bool,
) -> Vec<MonitorApplyOutcome> {
    let target_ids: Vec<_> = monitors
        .iter()
        .map(|monitor| monitor.device_id.clone())
        .collect();
    let mut previous = Vec::with_capacity(monitors.len());
    crate::apply_trace::event(
        "rollback-snapshot-start",
        format!("targets={}", monitors.len()),
    );
    for (index, monitor) in monitors.iter().enumerate() {
        match baseline(monitor, context) {
            Ok(state) => previous.push(state),
            Err(error) => {
                crate::apply_trace::event(
                    "rollback-snapshot-failure",
                    format!("target={:?} error={:?}", monitor.device_id, error.message),
                );
                let recovery = preflight_readback_recovery(
                    request.operation_id,
                    &request.wallpaper,
                    &monitor.device_id,
                    error.provider,
                    error.location.as_deref(),
                    error.observation,
                    context,
                    children_settled(),
                    error.message.clone(),
                );
                retain(recovery);
                return target_ids
                    .into_iter()
                    .enumerate()
                    .map(|(target_index, target_id)| {
                        if target_index == index {
                            unknown_outcome(target_id, format!("Could not record the current wallpaper for rollback: {}", error.message))
                        } else {
                            MonitorApplyOutcome {
                                target_id,
                                state: ApplyOperationState::Cancelled,
                                error: Some(
                                    "Wallpaper change was not started because rollback state was unavailable"
                                        .to_string(),
                                ),
                            }
                        }
                    })
                    .collect();
            }
        }
    }
    crate::apply_trace::event(
        "rollback-snapshot-complete",
        format!("targets={}", previous.len()),
    );
    if let Err(error) = validate_operation_capabilities(
        request.wallpaper.source,
        &target_ids,
        &previous,
        context.engine_capabilities,
    ) {
        crate::apply_trace::event("capability-preflight-failure", &error);
        return target_ids
            .into_iter()
            .map(|target_id| failed_outcome(target_id, error.clone()))
            .collect();
    }
    let protected_native_paths = protected_native_paths(&previous);

    for (index, monitor) in monitors.iter().enumerate() {
        if let Err(error) = apply_target(
            &request.wallpaper,
            request.operation_id,
            monitor,
            context,
            &protected_native_paths,
        ) {
            if error.unsupported_capability
                && index == 0
                && !context.mutation_dispatched.get()
                && children_settled()
            {
                return target_ids
                    .into_iter()
                    .map(|target_id| failed_outcome(target_id, error.message.clone()))
                    .collect();
            }
            if let Some(recovery) = error.recovery.clone() {
                retain(recovery);
            }
            if error.recovery.is_some() {
                return unsettled_outcomes(&target_ids, index, error.message);
            }
            let mut attempted_restore_indices = HashSet::new();
            let touched = index + usize::from(!error.before_dispatch);
            let rollback_errors = touched.checked_sub(1).map_or_else(HashMap::new, |last| {
                restore_touched_monitors(&target_ids, last, |rollback_index| {
                    attempted_restore_indices.insert(rollback_index);
                    match restore(
                        &monitors[rollback_index],
                        &previous[rollback_index],
                        context,
                    ) {
                        Ok(()) => Ok(()),
                        Err(RestorationFailure::EngineClose { location, message }) => {
                            retain(close_recovery(
                                request.operation_id,
                                request.wallpaper.id.clone(),
                                &monitors[rollback_index].device_id,
                                &location,
                                message.clone(),
                            ));
                            Err(message)
                        }
                        Err(RestorationFailure::EngineState { message }) => {
                            retain(engine_state_unavailable_recovery(
                                request.operation_id,
                                Some(request.wallpaper.id.clone()),
                                &monitors[rollback_index].device_id,
                                message.clone(),
                            ));
                            Err(message)
                        }
                        Err(RestorationFailure::Other(message)) => {
                            retain(recovery_for_provider_state(
                                request.operation_id,
                                Some(request.wallpaper.id.clone()),
                                &monitors[rollback_index],
                                &previous[rollback_index],
                                message.clone(),
                            ));
                            Err(message)
                        }
                    }
                })
            });
            for rollback_index in 0..touched {
                if !attempted_restore_indices.contains(&rollback_index) {
                    let reason = rollback_errors
                        .get(&rollback_index)
                        .cloned()
                        .unwrap_or_else(|| {
                            "rollback was not attempted after an earlier restoration failure"
                                .to_string()
                        });
                    retain(recovery_for_provider_state(
                        request.operation_id,
                        Some(request.wallpaper.id.clone()),
                        &monitors[rollback_index],
                        &previous[rollback_index],
                        reason,
                    ));
                }
            }
            return rollback_outcomes(&target_ids, index, error.message, &rollback_errors);
        }
    }

    target_ids.into_iter().map(confirmed_outcome).collect()
}

fn unsettled_outcomes(
    target_ids: &[String],
    failed_index: usize,
    apply_error: String,
) -> Vec<MonitorApplyOutcome> {
    target_ids
        .iter()
        .enumerate()
        .map(|(index, target_id)| {
            if index < failed_index {
                confirmed_outcome(target_id.clone())
            } else if index == failed_index {
                unknown_outcome(target_id.clone(), apply_error.clone())
            } else {
                MonitorApplyOutcome {
                    target_id: target_id.clone(),
                    state: ApplyOperationState::Cancelled,
                    error: Some(format!(
                        "Wallpaper change was not attempted because provider state for {} is unknown",
                        target_ids[failed_index]
                    )),
                }
            }
        })
        .collect()
}

fn blocked_apply_outcomes(target: &ApplyTarget, error: String) -> Vec<MonitorApplyOutcome> {
    let target_ids = requested_target_ids(target);
    if target_ids.is_empty() {
        return vec![unknown_outcome("wallpaper-operation".to_string(), error)];
    }
    unsettled_outcomes(&target_ids, 0, error)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum MonitorProviderState {
    Native {
        wallpaper: PathBuf,
        position: native_wallpaper::WallpaperPosition,
    },
    Engine {
        wallpaper: PathBuf,
        location: String,
    },
}

fn protected_native_paths(states: &[MonitorProviderState]) -> Vec<PathBuf> {
    states
        .iter()
        .filter_map(|state| match state {
            MonitorProviderState::Native { wallpaper, .. } => Some(wallpaper.clone()),
            MonitorProviderState::Engine { .. } => None,
        })
        .collect()
}

fn validate_operation_capabilities(
    source: WallpaperSource,
    target_ids: &[String],
    baselines: &[MonitorProviderState],
    capabilities: engine_ipc::EngineCapabilities,
) -> Result<(), String> {
    for (target_id, baseline) in target_ids.iter().zip(baselines) {
        let requirement = match (source, baseline) {
            (WallpaperSource::LocalImage, MonitorProviderState::Engine { .. }) => {
                Some("forward Engine-to-native handoff")
            }
            (
                WallpaperSource::LocalVideo | WallpaperSource::WallpaperEngine,
                MonitorProviderState::Native { .. },
            ) => Some("required rollback to the native baseline after Engine Apply"),
            _ => None,
        };
        if let Some(requirement) = requirement {
            capabilities
                .require_absence_confirmation()
                .map_err(|error| {
                    format!(
                        "Request not started: target {target_id} requires {requirement}; {error}"
                    )
                })?;
        }
    }
    Ok(())
}

fn unsupported_provider_failure(message: String) -> ProviderFailure {
    ProviderFailure {
        message,
        recovery: None,
        unsupported_capability: true,
        before_dispatch: true,
    }
}

fn provider_state_from_observation(
    engine_running: bool,
    target: &EngineTarget,
    engine_readback: EngineObservation,
    native_state: Result<MonitorProviderState, String>,
) -> Result<MonitorProviderState, String> {
    if !engine_running {
        return native_state;
    }

    engine_readback.adequate_for(target).map_err(|error| {
        format!(
            "provider-state-unknown: {error}: {}",
            engine_readback.description()
        )
    })?;
    match &engine_readback.state {
        EngineObservationState::Present(wallpaper) => Ok(MonitorProviderState::Engine {
            wallpaper: wallpaper.clone(),
            location: target.location.clone(),
        }),
        EngineObservationState::ConfirmedAbsent => native_state,
        EngineObservationState::Unreadable | EngineObservationState::Unsupported => Err(format!(
            "provider-state-unknown: {}",
            engine_readback.description()
        )),
    }
}

struct BaselineFailure {
    provider: ProviderKind,
    location: Option<String>,
    observation: Option<EngineObservation>,
    message: String,
}

fn monitor_provider_state(
    monitor: &native_wallpaper::NativeMonitor,
    context: &ApplyContext,
) -> Result<MonitorProviderState, BaselineFailure> {
    let failure = |provider, location, observation, message| BaselineFailure {
        provider,
        location,
        observation,
        message,
    };
    let native_state = || {
        let wallpaper = native_wallpaper::get_wallpaper_for_device(&monitor.device_id)?;
        let position = native_wallpaper::get_position()?;
        Ok(MonitorProviderState::Native {
            wallpaper,
            position,
        })
    };
    match engine_ipc::availability() {
        engine_ipc::EngineAvailability::Unavailable => {
            native_state().map_err(|message| failure(ProviderKind::Native, None, None, message))
        }
        engine_ipc::EngineAvailability::ProcessOnly
        | engine_ipc::EngineAvailability::Unreadable => {
            let observation = EngineObservation {
                target: EngineTarget::bound(&monitor.device_id, ""),
                source: engine_ipc::EngineObservationSource::Availability,
                state: EngineObservationState::Unreadable,
                diagnostic: Some("Control unavailable; Engine location has not been bound".into()),
            };
            Err(failure(
                ProviderKind::WallpaperEngine,
                None,
                Some(observation.clone()),
                observation.description(),
            ))
        }
        engine_ipc::EngineAvailability::ControlAvailable => {
            let location = context
                .engine_location_for_device(&monitor.device_id)
                .map_err(|message| failure(ProviderKind::WallpaperEngine, None, None, message))?;
            let observation =
                engine_ipc::observe_wallpaper(&EngineTarget::bound(&monitor.device_id, &location));
            crate::apply_trace::event("baseline-observation", observation.description());
            provider_state_from_observation(
                true,
                &EngineTarget::bound(&monitor.device_id, &location),
                observation.clone(),
                native_state(),
            )
            .map_err(|message| {
                failure(
                    ProviderKind::WallpaperEngine,
                    Some(location),
                    Some(observation),
                    message,
                )
            })
        }
    }
}

#[derive(Debug, Clone)]
enum RestorationFailure {
    EngineClose { location: String, message: String },
    EngineState { message: String },
    Other(String),
}

fn restore_monitor_provider_state(
    monitor: &native_wallpaper::NativeMonitor,
    state: &MonitorProviderState,
    context: &ApplyContext,
) -> Result<(), RestorationFailure> {
    match state {
        MonitorProviderState::Native {
            wallpaper,
            position,
        } => {
            match engine_ipc::availability() {
                engine_ipc::EngineAvailability::ControlAvailable => {
                    let location = context
                        .engine_location_for_device(&monitor.device_id)
                        .map_err(|message| RestorationFailure::EngineState {
                            message: format!(
                                "provider-state-unknown: Engine target mapping unreadable: {message}"
                            ),
                        })?;
                    let observation = engine_ipc::observe_wallpaper(&EngineTarget::bound(
                        &monitor.device_id,
                        &location,
                    ));
                    match observation.state {
                        EngineObservationState::Present(_)
                            if observation
                                .adequate_for(&EngineTarget::bound(&monitor.device_id, &location))
                                .is_ok() =>
                        {
                            engine_ipc::close_wallpaper(&location).map_err(|error| {
                                RestorationFailure::EngineClose {
                                    location: location.clone(),
                                    message: error.description(),
                                }
                            })?
                        }
                        EngineObservationState::ConfirmedAbsent
                            if observation
                                .adequate_for(&EngineTarget::bound(&monitor.device_id, &location))
                                .is_ok() => {}
                        _ => {
                            return Err(RestorationFailure::EngineState {
                                message: observation.description(),
                            });
                        }
                    }
                }
                engine_ipc::EngineAvailability::Unavailable => {}
                engine_ipc::EngineAvailability::ProcessOnly
                | engine_ipc::EngineAvailability::Unreadable => {
                    return Err(RestorationFailure::EngineState {
                        message:
                            "provider-state-unknown: Engine target state is unreadable while control is unavailable"
                                .to_string(),
                    });
                }
            }
            native_wallpaper::restore_wallpaper_state_for_device(
                wallpaper,
                &monitor.device_id,
                *position,
            )
            .map_err(RestorationFailure::Other)?;
            native_wallpaper::verify_wallpaper_state_for_device(
                wallpaper,
                &monitor.device_id,
                *position,
            )
            .map_err(RestorationFailure::Other)
        }
        MonitorProviderState::Engine {
            wallpaper,
            location,
        } => {
            let (observed, _) = engine_ipc::open_wallpaper(
                wallpaper,
                None, // A rollback snapshot has no independently captured project manifest.
                &EngineTarget::bound(&monitor.device_id, location),
                || context.mark_mutation_dispatched(),
            )
            .map_err(|error| RestorationFailure::Other(error.into_message()))?;
            readback_satisfies(
                &ProviderPostcondition::EngineProject {
                    location: location.clone(),
                    path: wallpaper.clone(),
                },
                &ProviderReadback::Engine(observed),
                Some(&EngineTarget::bound(&monitor.device_id, location)),
            )
            .map_err(RestorationFailure::Other)
        }
    }
}

fn rollback_outcomes(
    target_ids: &[String],
    failed_index: usize,
    apply_error: String,
    rollback_errors: &HashMap<usize, String>,
) -> Vec<MonitorApplyOutcome> {
    target_ids
        .iter()
        .enumerate()
        .map(|(index, target_id)| {
            if let Some(rollback_error) = rollback_errors.get(&index) {
                return MonitorApplyOutcome {
                    target_id: target_id.clone(),
                    state: ApplyOperationState::Unknown,
                    error: Some(format!(
                        "Wallpaper change failed and rollback could not restore the prior state: {rollback_error}"
                    )),
                };
            }
            if index < failed_index {
                return MonitorApplyOutcome {
                    target_id: target_id.clone(),
                    state: ApplyOperationState::Cancelled,
                    error: Some(format!(
                        "Wallpaper change was rolled back because {} failed: {apply_error}",
                        target_ids[failed_index]
                    )),
                };
            }
            if index == failed_index {
                return failed_outcome(
                    target_id.clone(),
                    format!("{apply_error}; the prior wallpaper was restored"),
                );
            }
            MonitorApplyOutcome {
                target_id: target_id.clone(),
                state: ApplyOperationState::Cancelled,
                error: Some(format!(
                    "Wallpaper change was not attempted because {} failed",
                    target_ids[failed_index]
                )),
            }
        })
        .collect()
}

fn restore_touched_monitors<F>(
    target_ids: &[String],
    failed_index: usize,
    mut restore: F,
) -> HashMap<usize, String>
where
    F: FnMut(usize) -> Result<(), String>,
{
    let mut rollback_errors = HashMap::new();
    for rollback_index in (0..=failed_index).rev() {
        if let Err(error) = restore(rollback_index) {
            let not_attempted = format!(
                "rollback was not attempted after restoration of {} failed: {error}",
                target_ids[rollback_index]
            );
            rollback_errors.insert(rollback_index, error);
            for skipped_index in (0..rollback_index).rev() {
                rollback_errors.insert(skipped_index, not_attempted.clone());
            }
            break;
        }
    }
    rollback_errors
}

#[derive(Debug, Clone)]
struct ProviderFailure {
    message: String,
    recovery: Option<ProviderRecovery>,
    unsupported_capability: bool,
    before_dispatch: bool,
}

impl From<String> for ProviderFailure {
    fn from(message: String) -> Self {
        Self {
            message,
            recovery: None,
            unsupported_capability: false,
            before_dispatch: false,
        }
    }
}

fn unknown_provider_failure(message: String, recovery: ProviderRecovery) -> ProviderFailure {
    ProviderFailure {
        message: if provider_state_is_unknown(&message) {
            message
        } else {
            format!("provider-state-unknown: {message}")
        },
        recovery: Some(recovery),
        unsupported_capability: false,
        before_dispatch: false,
    }
}

fn recovery_for_provider_state(
    operation_id: OperationId,
    wallpaper_id: Option<String>,
    monitor: &native_wallpaper::NativeMonitor,
    state: &MonitorProviderState,
    reason: String,
) -> ProviderRecovery {
    match state {
        MonitorProviderState::Native {
            wallpaper,
            position,
        } => native_recovery(
            operation_id,
            wallpaper_id,
            &monitor.device_id,
            wallpaper.clone(),
            position.0,
            reason,
        ),
        MonitorProviderState::Engine {
            wallpaper,
            location,
        } => ProviderRecovery {
            operation_id: operation_id.get(),
            wallpaper_id,
            target_id: monitor.device_id.clone(),
            provider: ProviderKind::WallpaperEngine,
            identity: wallpaper.display().to_string(),
            expected: ProviderPostcondition::EngineProject {
                location: location.clone(),
                path: wallpaper.clone(),
            },
            reason,
            phase: RecoveryPhase::DispatchedOrUnknown,
            observation: None,
        },
    }
}

fn apply_direct_to_monitor(
    wallpaper: &WallpaperItem,
    operation_id: OperationId,
    monitor: &native_wallpaper::NativeMonitor,
    context: &ApplyContext,
    protected_native_paths: &[PathBuf],
) -> Result<(), ProviderFailure> {
    match wallpaper.source {
        WallpaperSource::LocalImage => {
            match engine_ipc::availability() {
                engine_ipc::EngineAvailability::ControlAvailable => {
                    let location = apply_engine_location(
                        operation_id,
                        wallpaper,
                        &monitor.device_id,
                        context,
                        engine_ipc::ensure_mutation_allowed().is_ok(),
                    )?;
                    let observation = engine_ipc::observe_wallpaper(&EngineTarget::bound(
                        &monitor.device_id,
                        &location,
                    ));
                    match observation.state {
                        EngineObservationState::Present(_)
                            if observation
                                .adequate_for(&EngineTarget::bound(&monitor.device_id, &location))
                                .is_ok() =>
                        {
                            context
                                .engine_capabilities
                                .require_absence_confirmation()
                                .map_err(unsupported_provider_failure)?;
                            let recovery = close_recovery(
                                operation_id,
                                wallpaper.id.clone(),
                                &monitor.device_id,
                                &location,
                                "close command did not reach confirmed Engine absence".into(),
                            );
                            context.mark_mutation_dispatched();
                            engine_ipc::close_wallpaper(&location).map_err(
                                |error| match error {
                                    engine_ipc::EngineCloseFailure::UnsupportedBeforeDispatch(
                                        message,
                                    ) => unsupported_provider_failure(message),
                                    _ => unknown_provider_failure(error.description(), recovery),
                                },
                            )?;
                        }
                        EngineObservationState::ConfirmedAbsent
                            if observation
                                .adequate_for(&EngineTarget::bound(&monitor.device_id, &location))
                                .is_ok() => {}
                        _ => {
                            return Err(unknown_provider_failure(
                                observation.description(),
                                preflight_readback_recovery(
                                    operation_id,
                                    wallpaper,
                                    &monitor.device_id,
                                    ProviderKind::WallpaperEngine,
                                    Some(&location),
                                    Some(observation),
                                    context,
                                    engine_ipc::ensure_mutation_allowed().is_ok(),
                                    "Engine target readback was unavailable before native Apply"
                                        .into(),
                                ),
                            ));
                        }
                    }
                }
                engine_ipc::EngineAvailability::Unavailable => {}
                engine_ipc::EngineAvailability::ProcessOnly
                | engine_ipc::EngineAvailability::Unreadable => {
                    return Err(unknown_provider_failure(
                        "Wallpaper Engine target state is unreadable while control is unavailable"
                            .into(),
                        preflight_readback_recovery(
                            operation_id,
                            wallpaper,
                            &monitor.device_id,
                            ProviderKind::WallpaperEngine,
                            None,
                            None,
                            context,
                            engine_ipc::ensure_mutation_allowed().is_ok(),
                            "Engine target readback was unavailable before native Apply".into(),
                        ),
                    ));
                }
            }
            context.mark_mutation_dispatched();
            let expected = native_wallpaper::set_wallpaper_for_device_with_protected_paths(
                &wallpaper.file_target,
                &monitor.device_id,
                protected_native_paths,
            )?;
            crate::apply_trace::event("readback-start", format!("target={:?}", monitor.device_id));
            let actual = match native_wallpaper::get_wallpaper_for_device(&monitor.device_id) {
                Ok(actual) => {
                    crate::apply_trace::event(
                        "readback-complete",
                        format!("target={:?} actual_path={actual:?}", monitor.device_id),
                    );
                    actual
                }
                Err(error) => {
                    crate::apply_trace::event(
                        "readback-failure",
                        format!("target={:?} error={error:?}", monitor.device_id),
                    );
                    return Err(unknown_provider_failure(
                        error,
                        native_recovery(
                            operation_id,
                            Some(wallpaper.id.clone()),
                            &monitor.device_id,
                            expected.clone(),
                            native_wallpaper::FILL_POSITION.0,
                            "native wallpaper path readback was unavailable".into(),
                        ),
                    ));
                }
            };
            if native_wallpaper::normalized_wallpaper_path(&actual)
                != native_wallpaper::normalized_wallpaper_path(&expected)
            {
                return Err(unknown_provider_failure(
                    format!(
                        "Windows accepted the command, but reported {} instead of {}",
                        actual.display(),
                        expected.display()
                    ),
                    native_recovery(
                        operation_id,
                        Some(wallpaper.id.clone()),
                        &monitor.device_id,
                        expected.clone(),
                        native_wallpaper::FILL_POSITION.0,
                        "native wallpaper path readback did not match".into(),
                    ),
                ));
            }
            let actual_position = native_wallpaper::get_position().map_err(|error| {
                unknown_provider_failure(
                    error,
                    native_recovery(
                        operation_id,
                        Some(wallpaper.id.clone()),
                        &monitor.device_id,
                        expected.clone(),
                        native_wallpaper::FILL_POSITION.0,
                        "native wallpaper position readback was unavailable".into(),
                    ),
                )
            })?;
            if actual_position != native_wallpaper::FILL_POSITION {
                return Err(unknown_provider_failure(
                    format!(
                        "Windows accepted the wallpaper command but reported position {} instead of Fill",
                        actual_position.0
                    ),
                    native_recovery(
                        operation_id,
                        Some(wallpaper.id.clone()),
                        &monitor.device_id,
                        expected,
                        native_wallpaper::FILL_POSITION.0,
                        "native wallpaper position readback did not match".into(),
                    ),
                ));
            }
            Ok(())
        }
        WallpaperSource::LocalVideo | WallpaperSource::WallpaperEngine => {
            let location = apply_engine_location(
                operation_id,
                wallpaper,
                &monitor.device_id,
                context,
                engine_ipc::ensure_mutation_allowed().is_ok(),
            )?;
            let recovery = engine_apply_recovery(
                operation_id,
                wallpaper,
                &monitor.device_id,
                &location,
                "Engine project/media target did not reach independent confirmation".into(),
            );
            let (observed, endpoint) = engine_ipc::open_wallpaper(
                wallpaper.engine_target(),
                (wallpaper.source == WallpaperSource::WallpaperEngine)
                    .then_some(wallpaper.project_json_path.as_path()),
                &EngineTarget::bound(&monitor.device_id, &location),
                || context.mark_mutation_dispatched(),
            )
            .map_err(|error| match error {
                engine_ipc::EngineControlFailure::BeforeDispatch(message) => ProviderFailure {
                    before_dispatch: true,
                    ..ProviderFailure::from(message)
                },
                other => unknown_provider_failure(other.into_message(), recovery.clone()),
            })?;
            readback_satisfies(
                &recovery.expected,
                &ProviderReadback::Engine(observed),
                Some(&EngineTarget::bound(&monitor.device_id, &location)),
            )
            .map_err(|error| unknown_provider_failure(error, recovery))?;
            context
                .confirmed_capture_owners
                .borrow_mut()
                .insert(monitor.device_id.clone(), endpoint);
            Ok(())
        }
    }
}

fn apply_engine_location(
    operation_id: OperationId,
    wallpaper: &WallpaperItem,
    target_id: &str,
    context: &ApplyContext,
    children_settled: bool,
) -> Result<String, ProviderFailure> {
    context
        .engine_location_for_device(target_id)
        .map_err(|error| {
            unknown_provider_failure(
                error.clone(),
                preflight_readback_recovery(
                    operation_id,
                    wallpaper,
                    target_id,
                    ProviderKind::WallpaperEngine,
                    None,
                    None,
                    context,
                    children_settled,
                    error,
                ),
            )
        })
}

#[cfg(test)]
pub(crate) use contract_tests::RecheckFixture;

#[cfg(test)]
mod contract_tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    pub(crate) struct RecheckFixture {
        pub(crate) record: ProviderRecovery,
        receiver: mpsc::Receiver<CoordinatorRequest>,
        pending_requests: Arc<AtomicUsize>,
    }

    impl RecheckFixture {
        pub(crate) fn new(phase: RecoveryPhase) -> (Self, WallpaperCoordinator) {
            let mut record = r12_preflight();
            record
                .reason
                .push_str(" C:/private-fixture/provider-output.txt");
            record.operation_id = fixture_operation_id().get();
            record.phase = phase;
            if phase == RecoveryPhase::DispatchedOrUnknown {
                record.expected = ProviderPostcondition::EngineProject {
                    location: "Monitor0".into(),
                    path: "C:/fixture/project.json".into(),
                };
            }

            let (sender, receiver) = mpsc::sync_channel(1);
            let pending_requests = Arc::new(AtomicUsize::new(0));
            let owner = WallpaperCoordinator {
                sender,
                pending_requests: pending_requests.clone(),
            };
            retain_provider_recovery(record.clone());
            (
                Self {
                    record,
                    receiver,
                    pending_requests,
                },
                owner,
            )
        }

        pub(crate) fn take(&self) -> Option<(OperationId, EngineCommand)> {
            let request = match self.receiver.try_recv() {
                Ok(request) => request,
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => panic!("fixture coordinator disconnected"),
            };
            let _owned = PendingRequest(self.pending_requests.clone());
            match request {
                CoordinatorRequest::Engine(request) => {
                    Some((request.operation_id, request.command))
                }
                CoordinatorRequest::Apply(_) => panic!("Recheck provider queued an Apply"),
            }
        }

        pub(crate) fn finish(
            &self,
            queued: (OperationId, EngineCommand),
            saved_only: bool,
        ) -> EngineControlResult {
            let (operation_id, command) = queued;
            assert_eq!(
                command,
                EngineCommand::ReconcileProvider {
                    recovery: self.record.clone(),
                }
            );

            let readback = reconcile_provider_readback_with(
                &self.record,
                true,
                || Ok(vec!["display-a".into(), "display-b".into()]),
                || Ok(r12_mapping(0, 1)),
                |target| {
                    assert_eq!(target, &EngineTarget::bound("display-a", "Monitor0"));
                    if saved_only {
                        s1_saved_selection(target)
                    } else {
                        engine_ipc::control_query_observation(target, "C:/fixture/project.json")
                    }
                },
                |_| panic!("Engine recheck attempted native wallpaper readback"),
            );
            let outcomes = {
                let mut pending = unresolved_provider_operations().lock().unwrap();
                finish_reconciliation(&mut pending, &self.record, readback)
            };
            let recoveries = recovery_for_outcomes(&outcomes);
            EngineControlResult {
                operation_id,
                command,
                outcomes,
                recoveries,
            }
        }
    }

    impl Drop for RecheckFixture {
        fn drop(&mut self) {
            let mut pending = unresolved_provider_operations()
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            settle_matching_recovery(&mut pending, &self.record);
        }
    }

    fn s1_saved_selection(target: &EngineTarget) -> EngineObservation {
        let json = serde_json::json!({"account":{"general":{
            "user":{"monitormap":{"display-a":{"location":0}}},
            "wallpaperconfig":{"layout":0,"profile":null,
                "selectedwallpapers":{"Monitor0":{"file":"C:/fixture/project.json"}}}
        }}})
        .to_string();
        engine_ipc::selected_wallpaper_from_json(&json, target, std::path::Path::new("saved.json"))
    }

    #[test]
    fn s1_saved_parser_output_fails_confirmation_readability_and_baseline() {
        let target = EngineTarget::bound("display-a", "Monitor0");
        let observed = s1_saved_selection(&target);
        assert_eq!(
            observed.state,
            EngineObservationState::Present("C:/fixture/project.json".into())
        );
        assert!(observed.diagnostic.is_some());
        assert!(observed.adequate_for(&target).is_err());
        for expected in [
            ProviderPostcondition::EngineProject {
                path: "C:/fixture/project.json".into(),
                location: "Monitor0".into(),
            },
            ProviderPostcondition::ProviderStateReadable {
                provider: ProviderKind::WallpaperEngine,
                target_id: "display-a".into(),
                location: Some("Monitor0".into()),
            },
        ] {
            assert!(readback_satisfies(
                &expected,
                &ProviderReadback::Engine(observed.clone()),
                Some(&target)
            )
            .is_err());
        }
        assert!(
            provider_state_from_observation(true, &target, observed, Ok(r12_native_state()))
                .is_err()
        );
    }

    #[test]
    fn s1_current_control_query_requires_captured_target_and_supported_state() {
        let target = EngineTarget::bound("display-a", "Monitor0");
        let query = engine_ipc::control_query_observation(
            &EngineTarget::bound("DISPLAY-A", "Monitor0"),
            "C:/fixture/project.json",
        );
        let expected = ProviderPostcondition::EngineProject {
            path: "C:/fixture/project.json".into(),
            location: "Monitor0".into(),
        };
        assert!(readback_satisfies(
            &expected,
            &ProviderReadback::Engine(query.clone()),
            Some(&target)
        )
        .is_ok());
        assert!(provider_state_from_observation(
            true,
            &target,
            query.clone(),
            Err("Native unused".into())
        )
        .is_ok());
        assert!(
            readback_satisfies(&expected, &ProviderReadback::Engine(query.clone()), None).is_err()
        );
        for bad in [
            EngineObservation {
                source: engine_ipc::EngineObservationSource::Availability,
                ..query.clone()
            },
            EngineObservation {
                source: engine_ipc::EngineObservationSource::PersistedConfiguration {
                    path: "saved.json".into(),
                },
                ..query.clone()
            },
            EngineObservation {
                diagnostic: Some("query failed".into()),
                ..query.clone()
            },
            EngineObservation {
                target: EngineTarget::bound("display-b", "Monitor0"),
                ..query.clone()
            },
            EngineObservation {
                target: EngineTarget::bound("display-a", "Monitor1"),
                ..query.clone()
            },
            EngineObservation {
                target: EngineTarget {
                    device_id: None,
                    location: "Monitor0".into(),
                },
                ..query.clone()
            },
            EngineObservation {
                state: EngineObservationState::ConfirmedAbsent,
                ..query.clone()
            },
            EngineObservation {
                state: EngineObservationState::Present("relative.pkg".into()),
                ..query.clone()
            },
            EngineObservation {
                state: EngineObservationState::Unreadable,
                ..query.clone()
            },
            EngineObservation {
                state: EngineObservationState::Unsupported,
                ..query.clone()
            },
        ] {
            assert!(bad.adequate_for(&target).is_err(), "{bad:?}");
            assert!(readback_satisfies(
                &expected,
                &ProviderReadback::Engine(bad.clone()),
                Some(&target)
            )
            .is_err());
            assert!(
                provider_state_from_observation(true, &target, bad, Ok(r12_native_state()))
                    .is_err()
            );
        }
    }

    #[test]
    fn s1_saved_selection_keeps_dispatched_reconciliation_and_operation_ownership() {
        let mut recovery = r12_preflight();
        recovery.phase = RecoveryPhase::DispatchedOrUnknown;
        recovery.expected = ProviderPostcondition::EngineProject {
            path: "C:/fixture/project.json".into(),
            location: "Monitor0".into(),
        };
        let mut pending = vec![recovery.clone()];
        for settled in [false, true] {
            let result = reconcile_provider_readback_with(
                &recovery,
                settled,
                || Ok(vec!["display-a".into()]),
                || Ok(r12_mapping(0, 1)),
                s1_saved_selection,
                |_| panic!("Engine recovery cannot use native readback"),
            );
            assert_eq!(
                finish_reconciliation(&mut pending, &recovery, result)[0].state,
                ApplyOperationState::Unknown
            );
            assert_eq!(pending, vec![recovery.clone()]);
            assert!(cancel_preflight_record(&mut pending, &recovery, settled).is_err());
        }
        let mut stale = recovery.clone();
        stale.operation_id += 1;
        assert_eq!(
            finish_reconciliation(&mut pending, &stale, Ok(()))[0].state,
            ApplyOperationState::Unknown
        );
        assert_eq!(pending, vec![recovery]);
    }

    #[test]
    fn s1_forward_and_restore_consumer_failures_retain_unknown_recovery() {
        let mut request = r12_request();
        request.wallpaper.source = WallpaperSource::WallpaperEngine;
        let target = EngineTarget::bound("display-a", "Monitor0");
        let expected = ProviderPostcondition::EngineProject {
            path: "C:/fixture/project.json".into(),
            location: "Monitor0".into(),
        };
        for restoring in [false, true] {
            let context = ApplyContext::default();
            let mut pending = Vec::new();
            let outcomes = apply_direct_to_monitors_with(
                &request,
                vec![r12_monitor("display-a")],
                &context,
                |_, _| {
                    Ok(MonitorProviderState::Engine {
                        wallpaper: "C:/fixture/project.json".into(),
                        location: "Monitor0".into(),
                    })
                },
                |wallpaper, operation, monitor, context, _| {
                    context.mark_mutation_dispatched();
                    if restoring {
                        return Err("Dispatch failed; restore required".to_string().into());
                    }
                    readback_satisfies(
                        &expected,
                        &ProviderReadback::Engine(s1_saved_selection(&target)),
                        Some(&target),
                    )
                    .map_err(|error| {
                        unknown_provider_failure(
                            error,
                            engine_apply_recovery(
                                operation,
                                wallpaper,
                                &monitor.device_id,
                                "Monitor0",
                                "Unconfirmed forward".into(),
                            ),
                        )
                    })
                },
                |_, _, _| {
                    readback_satisfies(
                        &expected,
                        &ProviderReadback::Engine(s1_saved_selection(&target)),
                        Some(&target),
                    )
                    .map_err(RestorationFailure::Other)
                },
                |record| pending.push(record),
                || true,
            );
            assert_eq!(outcomes[0].state, ApplyOperationState::Unknown);
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].phase, RecoveryPhase::DispatchedOrUnknown);
            assert!(!recovery_capabilities_for(&pending[0], &pending, true).cancel_preflight);
        }
    }

    fn test_engine_observation(state: EngineObservationState) -> EngineObservation {
        EngineObservation {
            target: EngineTarget::bound("Monitor1", "Monitor1"),
            source: engine_ipc::EngineObservationSource::SyntheticProvider,
            state,
            diagnostic: None,
        }
    }

    fn absence_guard_context_supported() -> ApplyContext {
        ApplyContext {
            engine_capabilities: engine_ipc::EngineCapabilities {
                absence_confirmation: engine_ipc::AbsenceConfirmationCapability::SyntheticSupported,
            },
            ..ApplyContext::default()
        }
    }

    #[test]
    fn absence_guard_checks_forward_and_restoration_for_all_targets_before_dispatch() {
        for (source, baselines) in [
            (
                WallpaperSource::LocalImage,
                vec![
                    r12_native_state(),
                    MonitorProviderState::Engine {
                        wallpaper: "C:/baseline.pkg".into(),
                        location: "Monitor1".into(),
                    },
                ],
            ),
            (
                WallpaperSource::LocalVideo,
                vec![
                    MonitorProviderState::Engine {
                        wallpaper: "C:/baseline.pkg".into(),
                        location: "Monitor0".into(),
                    },
                    r12_native_state(),
                ],
            ),
            (WallpaperSource::WallpaperEngine, vec![r12_native_state()]),
        ] {
            let mut request = r12_request();
            request.wallpaper.source = source;
            let monitors: Vec<_> = ["display-a", "display-b"]
                .iter()
                .take(baselines.len())
                .map(|id| r12_monitor(id))
                .collect();
            let mut captured = 0;
            let dispatches = AtomicUsize::new(0);
            let outcomes = apply_direct_to_monitors_with(
                &request,
                monitors,
                &ApplyContext::default(),
                |_, _| {
                    let state = baselines[captured].clone();
                    captured += 1;
                    Ok(state)
                },
                |_, _, _, _, _| {
                    dispatches.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                },
                |_, _, _| panic!("Unsupported feasibility is separate from restoration failure"),
                |_| panic!("Wholly unstarted unsupported request needs no recovery"),
                || true,
            );
            assert_eq!(captured, baselines.len());
            assert_eq!(dispatches.load(Ordering::Relaxed), 0);
            assert!(outcomes
                .iter()
                .all(|outcome| outcome.state == ApplyOperationState::Failed));
            let result = ApplyResult {
                operation_id: request.operation_id,
                wallpaper_id: request.wallpaper.id,
                outcomes,
                recoveries: Vec::new(),
                transition: TransitionResult::not_requested(),
            };
            assert!(!result.is_success());
            let error = result.error_message().unwrap();
            assert!(error.contains("unsupported Engine absence confirmation"));
            assert!(error.contains(if source == WallpaperSource::LocalImage {
                "forward"
            } else {
                "required rollback"
            }));
            assert_eq!(
                shutdown_action_for(&result.recoveries, false, true),
                ShutdownAction::Ready
            );
        }
    }

    #[test]
    fn absence_guard_preserves_unaffected_native_and_engine_requests() {
        for (source, baseline) in [
            (WallpaperSource::LocalImage, r12_native_state()),
            (
                WallpaperSource::WallpaperEngine,
                MonitorProviderState::Engine {
                    wallpaper: "C:/baseline.pkg".into(),
                    location: "Monitor0".into(),
                },
            ),
            (
                WallpaperSource::LocalVideo,
                MonitorProviderState::Engine {
                    wallpaper: "C:/baseline.mp4".into(),
                    location: "Monitor0".into(),
                },
            ),
        ] {
            let mut request = r12_request();
            request.wallpaper.source = source;
            let dispatches = AtomicUsize::new(0);
            let outcomes = apply_direct_to_monitors_with(
                &request,
                vec![r12_monitor("display-a")],
                &ApplyContext::default(),
                |_, _| Ok(baseline.clone()),
                |_, _, _, context, _| {
                    context.mark_mutation_dispatched();
                    dispatches.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                },
                |_, _, _| panic!("Successful unchanged path needs no rollback"),
                |_| panic!("Successful path needs no recovery"),
                || true,
            );
            assert_eq!(dispatches.load(Ordering::Relaxed), 1);
            assert_eq!(outcomes[0].state, ApplyOperationState::ProviderConfirmed);
        }
    }

    #[test]
    fn absence_guard_synthetic_supported_provider_allows_required_close_and_restore() {
        let context = absence_guard_context_supported();
        let closed = AtomicUsize::new(0);
        let native = AtomicUsize::new(0);
        let request = r12_request();
        let outcomes = apply_direct_to_monitors_with(
            &request,
            vec![r12_monitor("display-a")],
            &context,
            |_, _| {
                Ok(MonitorProviderState::Engine {
                    wallpaper: "C:/baseline.pkg".into(),
                    location: "Monitor0".into(),
                })
            },
            |_, _, monitor, context, _| {
                context.mark_mutation_dispatched();
                let target = EngineTarget::bound(&monitor.device_id, "Monitor0");
                engine_ipc::close_wallpaper_with(
                    context.engine_capabilities,
                    &target,
                    || Ok(()),
                    || {
                        closed.fetch_add(1, Ordering::Relaxed);
                        Ok(())
                    },
                    || true,
                    |target| r12_observe(target, EngineObservationState::ConfirmedAbsent),
                    std::time::Duration::ZERO,
                )
                .map_err(|error| ProviderFailure::from(error.description()))?;
                native.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
            |_, _, _| panic!("No restoration on successful forward path"),
            |_| panic!("No uncertainty"),
            || true,
        );
        assert_eq!(closed.load(Ordering::Relaxed), 1);
        assert_eq!(native.load(Ordering::Relaxed), 1);
        assert_eq!(outcomes[0].state, ApplyOperationState::ProviderConfirmed);

        let mut engine_request = r12_request();
        engine_request.wallpaper.source = WallpaperSource::LocalVideo;
        let mut retained = Vec::new();
        let restores = AtomicUsize::new(0);
        let context = absence_guard_context_supported();
        let outcomes = apply_direct_to_monitors_with(
            &engine_request,
            vec![r12_monitor("display-a")],
            &context,
            |_, _| Ok(r12_native_state()),
            |_, _, _, context, _| {
                context.mark_mutation_dispatched();
                Err("Actual forward adapter failure after dispatch"
                    .to_string()
                    .into())
            },
            |_, _, _| {
                restores.fetch_add(1, Ordering::Relaxed);
                Err(RestorationFailure::Other(
                    "Actual restoration failure".into(),
                ))
            },
            |record| retained.push(record),
            || true,
        );
        assert_eq!(
            restores.load(Ordering::Relaxed),
            1,
            "Supported feasibility does not promise restoration cannot fail"
        );
        assert_eq!(outcomes[0].state, ApplyOperationState::Unknown);
        assert_eq!(retained[0].phase, RecoveryPhase::DispatchedOrUnknown);
        assert!(!recovery_capabilities_for(&retained[0], &retained, true).cancel_preflight);
    }

    #[test]
    fn absence_guard_defensive_rejection_never_relabels_prior_dispatch() {
        let context = ApplyContext::default();
        let unsupported = || {
            unsupported_provider_failure(
                "unsupported Engine absence confirmation at defensive boundary".into(),
            )
        };
        let outcomes = apply_direct_to_monitors_with(
            &r12_request(),
            vec![r12_monitor("display-a")],
            &context,
            |_, _| Ok(r12_native_state()),
            |_, _, _, _, _| Err(unsupported()),
            |_, _, _| panic!("No rollback for wholly unstarted unsupported request"),
            |_| panic!("No fabricated recovery"),
            || true,
        );
        assert_eq!(outcomes[0].state, ApplyOperationState::Failed);
        assert!(!context.mutation_dispatched.get());

        let mut pending = Vec::new();
        let dispatches = AtomicUsize::new(0);
        let restored = AtomicUsize::new(0);
        let context = ApplyContext::default();
        let outcomes = apply_direct_to_monitors_with(
            &r12_request(),
            vec![r12_monitor("display-a"), r12_monitor("display-b")],
            &context,
            |_, _| Ok(r12_native_state()),
            |_, _, monitor, context, _| {
                if monitor.device_id == "display-a" {
                    context.mark_mutation_dispatched();
                    dispatches.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                } else {
                    Err(unsupported())
                }
            },
            |_, _, _| {
                restored.fetch_add(1, Ordering::Relaxed);
                Err(RestorationFailure::Other(
                    "Existing dispatched restoration uncertainty".into(),
                ))
            },
            |record| pending.push(record),
            || true,
        );
        assert_eq!(dispatches.load(Ordering::Relaxed), 1);
        assert_eq!(restored.load(Ordering::Relaxed), 1);
        assert!(outcomes
            .iter()
            .any(|outcome| outcome.state == ApplyOperationState::Unknown));
        assert!(pending
            .iter()
            .all(|record| record.phase == RecoveryPhase::DispatchedOrUnknown));
        assert!(matches!(
            shutdown_action_for(&pending, false, true),
            ShutdownAction::Blocked(_)
        ));
    }

    #[test]
    fn absence_guard_rejection_does_not_release_unrelated_or_stale_ownership() {
        let dispatched = engine_apply_recovery(
            OperationId(1250),
            &r12_request().wallpaper,
            "display-b",
            "Monitor1",
            "Actual uncertainty".into(),
        );
        let mut pending = vec![dispatched.clone()];
        let stale = r12_preflight();
        assert!(cancel_preflight_record(&mut pending, &stale, true).is_err());
        let wrong = reconcile_provider_readback_with(
            &dispatched,
            false,
            || panic!("Child settlement must precede readback"),
            || panic!("No map"),
            |_| panic!("No observation"),
            |_| panic!("No native read"),
        );
        assert_eq!(
            finish_reconciliation(&mut pending, &dispatched, wrong)[0].state,
            ApplyOperationState::Unknown
        );
        assert_eq!(pending, vec![dispatched]);
        assert!(matches!(
            shutdown_action_for(&pending, false, true),
            ShutdownAction::Blocked(_)
        ));
    }

    fn r12_request() -> ApplyRequest {
        ApplyRequest {
            operation_id: OperationId(1201),
            wallpaper: WallpaperItem {
                scan_metadata: None,
                id: "item-a".into(),
                title: "Fixture".into(),
                folder_path: "C:/fixture".into(),
                project_json_path: "C:/fixture/project.json".into(),
                file_target: "C:/fixture/image.jpg".into(),
                preview_path: None,
                wallpaper_type: "image".into(),
                source: WallpaperSource::LocalImage,
            },
            target: ApplyTarget::DeviceId("display-a".into()),
            transition: TransitionConfig::default(),
        }
    }

    #[test]
    fn absence_guard_rejects_engine_to_native_before_any_provider_mutation() {
        let dispatches = AtomicUsize::new(0);
        let outcomes = apply_direct_to_monitors_with(
            &r12_request(),
            vec![r12_monitor("display-a")],
            &ApplyContext::default(),
            |_, _| {
                Ok(MonitorProviderState::Engine {
                    wallpaper: "C:/baseline.pkg".into(),
                    location: "Monitor0".into(),
                })
            },
            |_, _, _, _, _| {
                dispatches.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
            |_, _, _| panic!("Known unsupported work must not start rollback"),
            |_| panic!("Known unsupported work must not create recovery"),
            || true,
        );
        assert_eq!(
            dispatches.load(Ordering::Relaxed),
            0,
            "Complete operation must be rejected before Apply adapter"
        );
        assert_eq!(outcomes[0].state, ApplyOperationState::Failed);
        assert!(outcomes[0]
            .error
            .as_deref()
            .unwrap()
            .contains("unsupported"));
    }

    fn r12_monitor(id: &str) -> native_wallpaper::NativeMonitor {
        native_wallpaper::NativeMonitor {
            device_id: id.into(),
            rect: DesktopRect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
        }
    }

    fn r12_native_state() -> MonitorProviderState {
        MonitorProviderState::Native {
            wallpaper: "C:/baseline.bmp".into(),
            position: native_wallpaper::FILL_POSITION,
        }
    }

    fn r12_mapping(a: u64, b: u64) -> engine_ipc::EngineMonitorMap {
        engine_ipc::parse_monitor_map(
            &serde_json::json!({"account":{"general":{"user":{"monitormap":{
                "display-a":{"location":a},"display-b":{"location":b}
            }}}}})
            .to_string(),
        )
        .unwrap()
    }

    fn r12_observe(target: &EngineTarget, state: EngineObservationState) -> EngineObservation {
        EngineObservation {
            target: target.clone(),
            source: engine_ipc::EngineObservationSource::SyntheticProvider,
            state,
            diagnostic: None,
        }
    }

    fn r12_preflight() -> ProviderRecovery {
        preflight_readback_recovery(
            OperationId(1201),
            &r12_request().wallpaper,
            "display-a",
            ProviderKind::WallpaperEngine,
            None,
            Some(r12_observe(
                &EngineTarget::bound("display-a", ""),
                EngineObservationState::Unreadable,
            )),
            &ApplyContext::default(),
            true,
            "ProcessOnly/Unreadable before dispatch".into(),
        )
    }

    #[test]
    fn r12_preflight_outage_recovers_same_stable_display_without_apply_retry() {
        for state in [
            EngineObservationState::Unreadable,
            EngineObservationState::Unsupported,
        ] {
            let context = ApplyContext::default();
            let dispatches = AtomicUsize::new(0);
            let mut pending = Vec::new();
            let outcomes = apply_direct_to_monitors_with(
                &r12_request(),
                vec![r12_monitor("display-a")],
                &context,
                |monitor, _| {
                    Err(BaselineFailure {
                        provider: ProviderKind::WallpaperEngine,
                        location: None,
                        observation: Some(r12_observe(
                            &EngineTarget::bound(&monitor.device_id, ""),
                            state.clone(),
                        )),
                        message: "Preflight observation failed".into(),
                    })
                },
                |_, _, _, _, _| {
                    dispatches.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                },
                |_, _, _| panic!("No rollback before dispatch"),
                |record| pending.push(record),
                || true,
            );
            assert_eq!(dispatches.load(Ordering::Relaxed), 0);
            assert_eq!(outcomes[0].state, ApplyOperationState::Unknown);
            let recovery = pending[0].clone();
            assert_eq!(recovery.phase, RecoveryPhase::Preflight);
            assert!(recovery_capabilities_for(&recovery, &pending, true).cancel_preflight);
            if state == EngineObservationState::Unsupported {
                assert!(!recovery_capabilities_for(&recovery, &pending, true).retry_observation);
                cancel_preflight_record(&mut pending, &recovery, true).unwrap();
                assert_eq!(recovery.observation.as_ref().unwrap().state, state);
            } else {
                assert!(recovery_capabilities_for(&recovery, &pending, true).retry_observation);
                let readback = reconcile_provider_readback_with(
                    &recovery,
                    true,
                    || Ok(vec!["display-b".into(), "display-a".into()]),
                    || Ok(r12_mapping(1, 0)),
                    |target| {
                        assert_eq!(target, &EngineTarget::bound("display-a", "Monitor1"));
                        r12_observe(
                            target,
                            EngineObservationState::Present("C:/healthy.pkg".into()),
                        )
                    },
                    |_| panic!("Engine recovery cannot read another native target"),
                );
                assert!(readback.is_ok());
                let settled = finish_reconciliation(&mut pending, &recovery, readback);
                assert_eq!(settled[0].state, ApplyOperationState::Cancelled);
            }
            assert!(pending.is_empty());
            assert_eq!(
                dispatches.load(Ordering::Relaxed),
                0,
                "Recovery must not retry Apply"
            );
            let fresh_baselines = AtomicUsize::new(0);
            let later = apply_direct_to_monitors_with(
                &r12_request(),
                vec![r12_monitor("display-a")],
                &ApplyContext::default(),
                |_, _| {
                    fresh_baselines.fetch_add(1, Ordering::Relaxed);
                    Ok(r12_native_state())
                },
                |_, _, _, context, _| {
                    context.mark_mutation_dispatched();
                    dispatches.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                },
                |_, _, _| panic!("Healthy Apply needs no rollback"),
                |_| panic!("No uncertainty"),
                || true,
            );
            assert_eq!(fresh_baselines.load(Ordering::Relaxed), 1);
            assert_eq!(dispatches.load(Ordering::Relaxed), 1);
            assert_eq!(later[0].state, ApplyOperationState::ProviderConfirmed);
        }
    }

    #[test]
    fn r12_removed_ambiguous_and_wrong_targets_never_rebind_recovery() {
        let recovery = r12_preflight();
        for targets in [
            vec![],
            vec!["display-b".into()],
            vec!["display-a".into(), "DISPLAY-A".into()],
        ] {
            assert!(reconcile_provider_readback_with(
                &recovery,
                true,
                || Ok(targets),
                || Ok(r12_mapping(1, 0)),
                |_| panic!("Missing/ambiguous display must stop before observation"),
                |_| panic!("Wrong native read")
            )
            .is_err());
        }
        assert!(reconcile_provider_readback_with(
            &recovery,
            true,
            || Ok(vec!["display-a".into()]),
            || Ok(r12_mapping(1, 0)),
            |_| r12_observe(
                &EngineTarget::bound("display-b", "Monitor1"),
                EngineObservationState::Present("C:/healthy.pkg".into())
            ),
            |_| panic!("Wrong native read")
        )
        .is_err());
        let mut pending = vec![recovery.clone()];
        cancel_preflight_record(&mut pending, &recovery, true).unwrap();
        assert!(
            pending.is_empty(),
            "Removed preflight target must remain cancellable without readback"
        );
    }

    #[test]
    fn r12_dispatched_child_settlement_and_original_binding_are_required() {
        let scene = WallpaperItem {
            source: WallpaperSource::WallpaperEngine,
            wallpaper_type: "scene".into(),
            ..r12_request().wallpaper
        };
        let recovery = engine_apply_recovery(
            OperationId(1202),
            &scene,
            "display-a",
            "Monitor0",
            "Dispatch uncertain".into(),
        );
        let mut pending = vec![recovery.clone(), r12_preflight()];
        assert!(!recovery_capabilities_for(&recovery, &pending, true).cancel_preflight);
        for (children, path, map_index) in [
            (false, "C:/fixture/project.json", 0),
            (true, "C:/wrong.pkg", 0),
            (true, "C:/fixture/project.json", 1),
        ] {
            let result = reconcile_provider_readback_with(
                &recovery,
                children,
                || Ok(vec!["display-a".into()]),
                || Ok(r12_mapping(map_index, 1 - map_index)),
                |target| r12_observe(target, EngineObservationState::Present(path.into())),
                |_| panic!("Wrong native read"),
            );
            assert!(result.is_err());
            assert_eq!(
                finish_reconciliation(&mut pending, &recovery, result)[0].state,
                ApplyOperationState::Unknown
            );
            assert_eq!(pending.len(), 2);
        }
        let result = reconcile_provider_readback_with(
            &recovery,
            true,
            || Ok(vec!["display-a".into()]),
            || Ok(r12_mapping(0, 1)),
            |target| {
                r12_observe(
                    target,
                    EngineObservationState::Present("C:/fixture/project.json".into()),
                )
            },
            |_| panic!("Wrong native read"),
        );
        assert_eq!(
            finish_reconciliation(&mut pending, &recovery, result)[0].state,
            ApplyOperationState::ProviderConfirmed
        );
        assert_eq!(pending, vec![r12_preflight()]);
        assert_eq!(
            finish_reconciliation(&mut pending, &recovery, Ok(()))[0].state,
            ApplyOperationState::Unknown
        );
        assert_eq!(pending.len(), 1, "Stale completion cannot clear newer work");
    }

    #[test]
    fn r12_cancellation_and_exit_preserve_all_other_ownership() {
        let preflight = r12_preflight();
        let dispatched = engine_apply_recovery(
            OperationId(1202),
            &r12_request().wallpaper,
            "display-b",
            "Monitor1",
            "Dispatch uncertain".into(),
        );
        let mut pending = vec![preflight.clone(), dispatched.clone()];
        assert!(cancel_preflight_record(&mut pending, &preflight, false).is_err());
        assert_eq!(pending.len(), 2);
        assert!(matches!(
            shutdown_action_for(&pending, true, true),
            ShutdownAction::WaitForActive
        ));
        assert!(matches!(
            shutdown_action_for(&pending, false, false),
            ShutdownAction::Blocked(_)
        ));
        cancel_preflight_record(&mut pending, &preflight, true).unwrap();
        assert_eq!(pending, vec![dispatched.clone()]);
        assert!(matches!(
            shutdown_action_for(&pending, false, true),
            ShutdownAction::Blocked(_)
        ));
        assert!(cancel_preflight_record(&mut pending, &dispatched, true).is_err());
        assert!(cancel_preflight_record(&mut pending, &preflight, true).is_err());
        let mut same_request_dispatch = dispatched;
        same_request_dispatch.operation_id = preflight.operation_id;
        let mut peers = vec![preflight.clone(), same_request_dispatch];
        assert!(
            cancel_preflight_record(&mut peers, &preflight, true).is_err(),
            "Earlier target dispatch forbids whole-request cancellation"
        );
        assert!(!recovery_capabilities_for(&preflight, &peers, true).retry_observation);
        let context = ApplyContext::default();
        assert_eq!(
            context.recovery_phase(false),
            RecoveryPhase::DispatchedOrUnknown
        );
        context.mark_mutation_dispatched();
        assert_eq!(
            context.recovery_phase(true),
            RecoveryPhase::DispatchedOrUnknown
        );
        let mut only_preflight = vec![preflight.clone()];
        assert!(matches!(
            shutdown_action_for(&only_preflight, false, true),
            ShutdownAction::CancelPreflight(_)
        ));
        cancel_preflight_record(&mut only_preflight, &preflight, true).unwrap();
        assert_eq!(
            shutdown_action_for(&only_preflight, false, true),
            ShutdownAction::Ready
        );
    }

    #[test]
    fn r12_unsupported_absence_and_property_readback_stay_unconfirmed() {
        let close = close_recovery(
            OperationId(1203),
            "item-a".into(),
            "display-a",
            "Monitor0",
            "Close uncertain".into(),
        );
        for state in [
            EngineObservationState::Unreadable,
            EngineObservationState::Unsupported,
            EngineObservationState::Present("C:/other.pkg".into()),
        ] {
            assert!(reconcile_provider_readback_with(
                &close,
                true,
                || Ok(vec!["display-a".into()]),
                || Ok(r12_mapping(0, 1)),
                |target| r12_observe(target, state.clone()),
                |_| panic!("Wrong native read")
            )
            .is_err());
        }
        let control = engine_control_recovery(
            OperationId(1204),
            "properties".into(),
            "display-a",
            "Property value readback unavailable".into(),
        );
        assert!(reconcile_provider_readback_with(
            &control,
            true,
            || panic!("Unsupported value readback must stop before probing"),
            || panic!("No mapping substitutes property values"),
            |_| panic!("No path substitutes property values"),
            |_| panic!("No native value")
        )
        .is_err());
        assert!(
            !recovery_capabilities_for(&control, std::slice::from_ref(&control), true)
                .retry_observation
        );
        assert!(
            !recovery_capabilities_for(&control, std::slice::from_ref(&control), true)
                .cancel_preflight
        );
    }

    #[test]
    fn r12_earlier_target_mutation_prevents_preflight_cancellation_of_whole_request() {
        let context = ApplyContext::default();
        let dispatches = AtomicUsize::new(0);
        let mut pending = Vec::new();
        let outcomes = apply_direct_to_monitors_with(
            &r12_request(),
            vec![r12_monitor("display-a"), r12_monitor("display-b")],
            &context,
            |_, _| Ok(r12_native_state()),
            |wallpaper, operation, monitor, context, _| {
                if monitor.device_id == "display-a" {
                    context.mark_mutation_dispatched();
                    dispatches.fetch_add(1, Ordering::Relaxed);
                    Ok(())
                } else {
                    Err(unknown_provider_failure(
                        "Second target observation failed before its dispatch".into(),
                        preflight_readback_recovery(
                            operation,
                            wallpaper,
                            &monitor.device_id,
                            ProviderKind::WallpaperEngine,
                            Some("Monitor1"),
                            None,
                            context,
                            true,
                            "Unresolved earlier target".into(),
                        ),
                    ))
                }
            },
            |_, _, _| panic!("Unknown must not start rollback"),
            |record| pending.push(record),
            || true,
        );
        assert_eq!(dispatches.load(Ordering::Relaxed), 1);
        assert_eq!(pending[0].phase, RecoveryPhase::DispatchedOrUnknown);
        assert_eq!(outcomes[0].state, ApplyOperationState::ProviderConfirmed);
        assert_eq!(outcomes[1].state, ApplyOperationState::Unknown);
        let retained = pending[0].clone();
        assert!(cancel_preflight_record(&mut pending, &retained, true).is_err());
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn r12_unknown_and_misbound_observations_cannot_select_native_baseline() {
        for state in [
            EngineObservationState::Unreadable,
            EngineObservationState::Unsupported,
        ] {
            assert!(provider_state_from_observation(
                true,
                &EngineTarget::bound("Monitor1", "Monitor1"),
                test_engine_observation(state),
                Ok(r12_native_state())
            )
            .is_err());
        }
        let mut wrong_target = test_engine_observation(EngineObservationState::ConfirmedAbsent);
        wrong_target.target.location = "Monitor0".into();
        assert!(provider_state_from_observation(
            true,
            &EngineTarget::bound("Monitor1", "Monitor1"),
            wrong_target,
            Ok(r12_native_state())
        )
        .is_err());
    }

    #[test]
    fn r12_global_control_outages_and_adapter_preflight_remain_cancellable() {
        for command in [
            EngineCommand::SetPaused(true),
            EngineCommand::SetMuted(true),
            EngineCommand::SetVolume(23),
        ] {
            for availability in [
                engine_ipc::EngineAvailability::ProcessOnly,
                engine_ipc::EngineAvailability::Unreadable,
                engine_ipc::EngineAvailability::ControlAvailable,
            ] {
                let dispatches = AtomicUsize::new(0);
                let mut pending = Vec::new();
                let outcomes = global_control_with(
                    OperationId(1220),
                    &command,
                    availability,
                    || true,
                    || {
                        assert_eq!(
                            availability,
                            engine_ipc::EngineAvailability::ControlAvailable
                        );
                        Err(
                            engine_ipc::EngineControlFailure::PreflightObservationFailed(
                                "Control disappeared before actual mutation".into(),
                            ),
                        )
                    },
                    |record| pending.push(record),
                );
                assert_eq!(dispatches.load(Ordering::Relaxed), 0);
                assert_eq!(outcomes[0].state, ApplyOperationState::Unknown);
                let recovery = pending[0].clone();
                assert_eq!(recovery.phase, RecoveryPhase::Preflight);
                assert!(recovery_capabilities_for(&recovery, &pending, true).cancel_preflight);
                assert!(control_preflight_readback(
                    recovery.phase,
                    true,
                    engine_ipc::EngineAvailability::ControlAvailable
                )
                .is_ok());
                assert_eq!(
                    finish_reconciliation(&mut pending, &recovery, Ok(()))[0].state,
                    ApplyOperationState::Cancelled
                );
                assert!(pending.is_empty());
                assert_eq!(dispatches.load(Ordering::Relaxed), 0);
                let accepted = global_control_with(
                    OperationId(1221),
                    &command,
                    engine_ipc::EngineAvailability::ControlAvailable,
                    || true,
                    || {
                        dispatches.fetch_add(1, Ordering::Relaxed);
                        Ok(())
                    },
                    |_| panic!("Accepted controls need no recovery"),
                );
                assert_eq!(accepted[0].state, ApplyOperationState::CommandAccepted);
                assert_eq!(dispatches.load(Ordering::Relaxed), 1);
            }
        }
        assert!(control_preflight_readback(
            RecoveryPhase::DispatchedOrUnknown,
            true,
            engine_ipc::EngineAvailability::ControlAvailable
        )
        .is_err());
    }

    #[test]
    fn r12_mapping_failure_at_adapter_entry_never_starts_rollback_or_mutation() {
        let request = r12_request();
        let context = ApplyContext::default();
        let _ = context.engine_monitor_map_with(|| Err("Missing or ambiguous mapping".into()));
        let mut pending = Vec::new();
        let dispatches = AtomicUsize::new(0);
        let outcomes = apply_direct_to_monitors_with(
            &request,
            vec![r12_monitor("display-a")],
            &context,
            |_, _| Ok(r12_native_state()),
            |wallpaper, operation_id, monitor, context, _| {
                apply_engine_location(operation_id, wallpaper, &monitor.device_id, context, true)?;
                dispatches.fetch_add(1, Ordering::Relaxed);
                Ok(())
            },
            |_, _, _| panic!("Mapping observation failure must not begin rollback"),
            |record| pending.push(record),
            || true,
        );
        assert_eq!(outcomes[0].state, ApplyOperationState::Unknown);
        assert_eq!(dispatches.load(Ordering::Relaxed), 0);
        let recovery = pending[0].clone();
        assert_eq!(recovery.phase, RecoveryPhase::Preflight);
        cancel_preflight_record(&mut pending, &recovery, true).unwrap();
        assert!(pending.is_empty());
    }

    #[test]
    fn r12_known_control_spawn_failure_stays_failed_and_after_start_stays_unknown() {
        let command = EngineCommand::SetPaused(true);
        let failed = global_control_with(
            OperationId(1222),
            &command,
            engine_ipc::EngineAvailability::ControlAvailable,
            || true,
            || {
                Err(engine_ipc::EngineControlFailure::BeforeDispatch(
                    "Missing executable".into(),
                ))
            },
            |_| panic!("Known zero dispatch failure must not create an uncertainty gate"),
        );
        assert_eq!(failed[0].state, ApplyOperationState::Failed);
        let unknown = global_control_with(
            OperationId(1223),
            &command,
            engine_ipc::EngineAvailability::ControlAvailable,
            || true,
            || {
                Err(engine_ipc::EngineControlFailure::DispatchedOrUnknown(
                    "Child started but result unknown".into(),
                ))
            },
            |_| panic!("Post-dispatch recovery is retained by the worker result path"),
        );
        assert_eq!(unknown[0].state, ApplyOperationState::Unknown);
    }

    #[test]
    fn random_sampler_initializes_and_advances_from_zero_state() {
        let state = AtomicU64::new(0);

        let first = next_random_sample(&state, 0x1234_5678_9abc_def0);
        let second = next_random_sample(&state, 0xfeed_face_cafe_beef);

        assert_ne!(first, 0);
        assert_eq!(state.load(Ordering::Relaxed), second);
        assert_ne!(second, first);
    }

    #[test]
    fn apply_context_loads_engine_monitor_map_at_most_once() {
        let context = ApplyContext::default();
        let loads = AtomicUsize::new(0);
        let load = || {
            loads.fetch_add(1, Ordering::Relaxed);
            Ok(engine_ipc::EngineMonitorMap::default())
        };

        assert!(context.engine_monitor_map_with(load).is_ok());
        assert!(context.engine_monitor_map_with(load).is_ok());
        assert_eq!(loads.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn native_rollback_paths_are_all_protected_but_engine_paths_are_not() {
        let native_a = PathBuf::from(r"C:\cache\native-a.bmp");
        let native_b = PathBuf::from(r"C:\cache\native-b.bmp");
        let engine_path = PathBuf::from(r"C:\engine\project.json");
        let states = vec![
            MonitorProviderState::Native {
                wallpaper: native_a.clone(),
                position: native_wallpaper::FILL_POSITION,
            },
            MonitorProviderState::Engine {
                wallpaper: engine_path,
                location: "display-b".into(),
            },
            MonitorProviderState::Native {
                wallpaper: native_b.clone(),
                position: native_wallpaper::FILL_POSITION,
            },
        ];

        assert_eq!(protected_native_paths(&states), vec![native_a, native_b]);
    }

    #[test]
    fn submission_targets_preserve_explicit_device_order() {
        assert_eq!(
            requested_target_ids(&ApplyTarget::DeviceIds(vec![
                "display-b".into(),
                "display-a".into(),
            ])),
            vec!["display-b", "display-a"]
        );
    }

    #[test]
    fn completion_is_successful_only_when_every_monitor_is_confirmed() {
        let complete = ApplyResult {
            operation_id: OperationId(17),
            wallpaper_id: "wallpaper-1".into(),
            outcomes: vec![
                MonitorApplyOutcome {
                    target_id: "display-a".into(),
                    state: ApplyOperationState::ProviderConfirmed,
                    error: None,
                },
                MonitorApplyOutcome {
                    target_id: "display-b".into(),
                    state: ApplyOperationState::ProviderConfirmed,
                    error: None,
                },
            ],
            recoveries: Vec::new(),
            transition: TransitionResult::not_requested(),
        };
        assert!(complete.is_success());
        assert_eq!(complete.error_message(), None);

        let partial = ApplyResult {
            operation_id: OperationId(18),
            wallpaper_id: "wallpaper-1".into(),
            outcomes: vec![
                complete.outcomes[0].clone(),
                MonitorApplyOutcome {
                    target_id: "display-b".into(),
                    state: ApplyOperationState::Failed,
                    error: Some("provider rejected command".into()),
                },
            ],
            recoveries: Vec::new(),
            transition: TransitionResult::not_requested(),
        };
        assert!(!partial.is_success());
        assert_eq!(
            partial.error_message().as_deref(),
            Some("display-b: provider rejected command")
        );
    }

    #[test]
    fn cancellation_and_unknown_states_have_stable_messages() {
        let result = ApplyResult {
            operation_id: OperationId(19),
            wallpaper_id: "wallpaper-2".into(),
            outcomes: vec![
                MonitorApplyOutcome {
                    target_id: "display-a".into(),
                    state: ApplyOperationState::Cancelled,
                    error: None,
                },
                MonitorApplyOutcome {
                    target_id: "display-b".into(),
                    state: ApplyOperationState::Unknown,
                    error: None,
                },
            ],
            recoveries: Vec::new(),
            transition: TransitionResult::not_requested(),
        };
        assert_eq!(
            result.error_message().as_deref(),
            Some("display-a: operation was cancelled; display-b: provider state is unknown")
        );
        assert_eq!(
            result.user_message(),
            Some("Wallpaper provider state could not be confirmed.")
        );
    }

    #[test]
    fn user_message_hides_provider_diagnostics_but_error_message_keeps_them() {
        let result = ApplyResult {
            operation_id: OperationId(22),
            wallpaper_id: "wallpaper-3".into(),
            outcomes: vec![MonitorApplyOutcome {
                target_id: "DISPLAY-A".into(),
                state: ApplyOperationState::Unknown,
                error: Some(
                    "provider-state-unknown: persisted path C:/private/project.json was not a live readback"
                        .into(),
                ),
            }],
            recoveries: Vec::new(),
            transition: TransitionResult::not_requested(),
        };
        assert!(result
            .error_message()
            .is_some_and(|message| message.contains("C:/private/project.json")));
        assert_eq!(
            result.user_message(),
            Some("Wallpaper provider state could not be confirmed.")
        );
    }

    #[test]
    fn transition_warning_does_not_turn_provider_confirmation_into_failure() {
        let result = ApplyResult {
            operation_id: OperationId(21),
            wallpaper_id: "wallpaper-transition-fallback".into(),
            outcomes: vec![MonitorApplyOutcome {
                target_id: "display-a".into(),
                state: ApplyOperationState::ProviderConfirmed,
                error: None,
            }],
            recoveries: Vec::new(),
            transition: TransitionResult::direct_fallback("outgoing capture failed"),
        };

        assert!(result.is_success());
        assert_eq!(result.error_message(), None);
        assert_eq!(result.transition.status, TransitionStatus::DirectFallback);
        assert!(result
            .transition
            .warning_message()
            .is_some_and(|warning| warning.contains("wallpaper changed by direct switch")));
    }

    #[test]
    fn mixed_incoming_capture_keeps_both_provider_confirmations_and_one_playback() {
        let context = ApplyContext::default();
        let mut dispatches = Vec::new();
        let providers = apply_direct_to_monitors_with(
            &r12_request(),
            vec![r12_monitor("display-a"), r12_monitor("display-b")],
            &context,
            |_, _| {
                Ok(MonitorProviderState::Native {
                    wallpaper: "C:/owned-baseline.jpg".into(),
                    position: native_wallpaper::FILL_POSITION,
                })
            },
            |_, _, display, _, _| {
                dispatches.push(display.device_id.clone());
                Ok(())
            },
            |_, _, _| panic!("both distinct mock providers confirmed"),
            |_| panic!("no mock recovery necessary"),
            || true,
        );
        assert_eq!(dispatches, ["display-a", "display-b"]);
        let slots = providers
            .iter()
            .enumerate()
            .map(|(index, outcome)| (outcome.target_id.clone(), index));
        let mut presented = Vec::new();
        let receipt = settle_incoming_transitions(
            &providers,
            slots,
            |slot| {
                if slot == 1 {
                    Err("rendered frame unavailable".into())
                } else {
                    Ok(slot)
                }
            },
            |outputs| {
                presented.extend_from_slice(outputs);
                vec![PlaybackProgress {
                    intermediate_submitted: true,
                    final_submitted: true,
                    error: None,
                }]
            },
        );
        assert_eq!(receipt.status, TransitionStatus::PartialIncomingCapture);
        assert_eq!(presented, [0]);
        for outcome in providers {
            assert_eq!(outcome.state, ApplyOperationState::ProviderConfirmed);
            let animation = receipt
                .displays
                .iter()
                .find(|item| item.display_id == outcome.target_id)
                .unwrap();
            if outcome.target_id == "display-a" {
                assert_eq!(animation.status, TransitionStatus::Completed);
                assert!(animation.intermediate_submitted);
                assert!(animation.error.is_none());
            } else {
                assert_eq!(
                    animation.status,
                    TransitionStatus::IncomingCaptureUnavailable
                );
                assert!(!animation.intermediate_submitted);
                assert_eq!(
                    animation.error.as_deref(),
                    Some("rendered frame unavailable")
                );
            }
        }
    }

    #[test]
    fn mixed_incoming_capture_requires_confirmation_on_every_display() {
        let providers = [
            confirmed_outcome("display-a".into()),
            unknown_outcome("display-b".into(), "no independent confirmation".into()),
        ];
        let receipt = settle_incoming_transitions(
            &providers,
            [("display-a".into(), 0), ("display-b".into(), 1)],
            |_| panic!("unconfirmed provider must stop incoming capture"),
            |_| panic!("unconfirmed provider must stop playback"),
        );
        assert_eq!(receipt.status, TransitionStatus::NotAttempted);
        assert_eq!(providers[1].state, ApplyOperationState::Unknown);
    }

    #[test]
    fn same_wallpaper_skips_animation_only_when_every_target_already_shows_it() {
        // `None` marks a display that already shows the requested wallpaper.
        for (case, slots, animated) in [
            ("none-same", [Some(0), Some(1)], vec![0, 1]),
            ("mixed", [Some(0), None], vec![0]),
            ("all-same", [None, None], vec![]),
        ] {
            let targets: Vec<(String, Option<usize>)> = ["display-a", "display-b"]
                .into_iter()
                .map(String::from)
                .zip(slots)
                .collect();
            let mut dispatched = false;
            let mut presented = Vec::new();
            let (providers, transition) = settle_prepared_transition(
                targets,
                || {
                    dispatched = true;
                    vec![
                        confirmed_outcome("display-a".into()),
                        confirmed_outcome("display-b".into()),
                    ]
                },
                Ok,
                |outputs| {
                    presented.extend_from_slice(outputs);
                    outputs
                        .iter()
                        .map(|_| PlaybackProgress {
                            intermediate_submitted: true,
                            final_submitted: true,
                            error: None,
                        })
                        .collect()
                },
            );
            assert!(
                dispatched,
                "{case}: every target receives the provider command"
            );
            assert!(
                providers
                    .iter()
                    .all(|outcome| outcome.state == ApplyOperationState::ProviderConfirmed),
                "{case}: provider confirmation is untouched"
            );
            assert_eq!(presented, animated, "{case}: animated displays");
            assert_eq!(transition.warning_message(), None, "{case}: no warning");
            if animated.is_empty() {
                assert_eq!(transition.status, TransitionStatus::Unchanged, "{case}");
                assert_eq!(transition.status_name(), "unchanged");
                continue;
            }
            assert_eq!(transition.status, TransitionStatus::Completed, "{case}");
            for (display, slot) in ["display-a", "display-b"].into_iter().zip(slots) {
                let animation = transition
                    .displays
                    .iter()
                    .find(|item| item.display_id == display)
                    .unwrap_or_else(|| panic!("{case}: {display} has a transition outcome"));
                let expected =
                    slot.map_or(TransitionStatus::Unchanged, |_| TransitionStatus::Completed);
                assert_eq!(animation.status, expected, "{case}: {display}");
                assert_eq!(animation.error, None, "{case}: {display}");
            }
        }
        assert_ne!(
            TransitionResult::unchanged().status_name(),
            TransitionResult::not_requested().status_name()
        );
    }

    #[test]
    fn transition_result_keeps_capture_playback_and_success_paths_distinct() {
        let missing = TransitionResult::incoming_capture_unavailable(vec![
            "display-a: incoming frame unavailable".into(),
        ]);
        assert_eq!(missing.status_name(), "incoming-capture-unavailable");
        assert!(missing
            .warning_message()
            .is_some_and(|warning| warning.contains("display-a")));

        let partial = TransitionResult::partial_incoming_capture(vec![
            "display-b: incoming frame unavailable".into(),
        ]);
        assert_eq!(partial.status_name(), "partial-incoming-capture");
        assert!(partial
            .warning_message()
            .is_some_and(|warning| warning.contains("available targets")));

        let playback = TransitionResult::playback_failed(vec!["surface was destroyed".into()]);
        assert_eq!(playback.status_name(), "playback-failed");
        assert!(playback
            .warning_message()
            .is_some_and(|warning| warning.contains("did not complete")));

        let completed = TransitionResult {
            status: TransitionStatus::Completed,
            warnings: Vec::new(),
            displays: Vec::new(),
        };
        assert_eq!(completed.status_name(), "completed");
        assert_eq!(completed.warning_message(), None);
    }

    #[test]
    fn engine_results_distinguish_acceptance_from_provider_confirmation() {
        let result = EngineControlResult {
            operation_id: OperationId(20),
            command: EngineCommand::SetPaused(true),
            outcomes: vec![accepted_outcome("wallpaper-engine-global".into())],
            recoveries: Vec::new(),
        };

        assert!(result.is_success());
        assert_eq!(result.error_message(), None);
        assert_eq!(
            result.outcomes[0].state,
            ApplyOperationState::CommandAccepted
        );
    }

    #[test]
    fn successful_engine_setter_is_accepted_without_value_readback() {
        let outcomes = global_engine_outcome(Ok(()));

        assert_eq!(outcomes.len(), 1);
        assert_eq!(
            outcomes[0].state,
            ApplyOperationState::CommandAccepted,
            "a successful setter does not independently observe the Engine value"
        );
        assert_ne!(outcomes[0].state, ApplyOperationState::ProviderConfirmed);
    }

    #[test]
    fn matching_provider_readback_keeps_confirmation_semantics() {
        let expected = ProviderPostcondition::EngineProject {
            location: "Monitor1".into(),
            path: std::path::PathBuf::from(r"C:\fixture\scene\project.json"),
        };
        let observed =
            ProviderReadback::Engine(test_engine_observation(EngineObservationState::Present(
                std::path::PathBuf::from(r"C:\fixture\scene\project.json"),
            )));

        assert!(readback_satisfies(
            &expected,
            &observed,
            Some(&EngineTarget::bound("Monitor1", "Monitor1"))
        )
        .is_ok());
        assert_eq!(
            confirmed_outcome("Monitor1".into()).state,
            ApplyOperationState::ProviderConfirmed
        );
    }

    #[test]
    fn partial_apply_rolls_back_only_touched_monitors() {
        let targets = vec!["display-a".into(), "display-b".into(), "display-c".into()];
        let outcomes = rollback_outcomes(
            &targets,
            1,
            "provider rejected command".into(),
            &HashMap::new(),
        );

        assert_eq!(outcomes[0].state, ApplyOperationState::Cancelled);
        assert!(outcomes[0]
            .error
            .as_deref()
            .unwrap()
            .contains("rolled back"));
        assert_eq!(outcomes[1].state, ApplyOperationState::Failed);
        assert!(outcomes[1]
            .error
            .as_deref()
            .unwrap()
            .contains("prior wallpaper was restored"));
        assert_eq!(outcomes[2].state, ApplyOperationState::Cancelled);
        assert!(outcomes[2]
            .error
            .as_deref()
            .unwrap()
            .contains("not attempted"));
    }

    #[test]
    fn rollback_stops_after_the_first_failed_restoration() {
        let targets = vec!["display-a".into(), "display-b".into()];
        let mut attempted = Vec::new();
        let rollback_errors = restore_touched_monitors(&targets, 1, |index| {
            attempted.push(index);
            Err("provider-state-unknown: restoration did not settle".to_string())
        });

        assert_eq!(attempted, vec![1]);
        assert!(rollback_errors.contains_key(&1));
        assert!(rollback_errors.contains_key(&0));

        let outcomes = rollback_outcomes(
            &targets,
            1,
            "provider rejected command".into(),
            &rollback_errors,
        );
        assert_eq!(outcomes[0].state, ApplyOperationState::Unknown);
        assert_eq!(outcomes[1].state, ApplyOperationState::Unknown);
    }

    #[test]
    fn unsettled_provider_failure_is_unknown_and_does_not_claim_rollback() {
        let targets = vec!["display-a".into(), "display-b".into(), "display-c".into()];
        let outcomes = unsettled_outcomes(
            &targets,
            1,
            "provider-state-unknown: operation did not settle".into(),
        );

        assert_eq!(outcomes[0].state, ApplyOperationState::ProviderConfirmed);
        assert_eq!(outcomes[1].state, ApplyOperationState::Unknown);
        assert!(outcomes[1]
            .error
            .as_deref()
            .unwrap()
            .contains("provider-state-unknown"));
        assert_eq!(outcomes[2].state, ApplyOperationState::Cancelled);
        assert!(outcomes[2]
            .error
            .as_deref()
            .unwrap()
            .contains("not attempted"));
    }

    #[test]
    fn only_explicit_unsettled_provider_errors_bypass_rollback() {
        assert!(provider_state_is_unknown(
            "provider-state-unknown: readback did not settle"
        ));
        assert!(!provider_state_is_unknown("provider rejected the command"));
    }

    #[test]
    fn failed_rollback_reports_unknown_provider_state() {
        let targets = vec!["display-a".into(), "display-b".into()];
        let outcomes = rollback_outcomes(
            &targets,
            1,
            "provider rejected command".into(),
            &HashMap::from([(0, "restore failed".into())]),
        );

        assert_eq!(outcomes[0].state, ApplyOperationState::Unknown);
        assert!(outcomes[0]
            .error
            .as_deref()
            .unwrap()
            .contains("restore failed"));
        assert_eq!(outcomes[1].state, ApplyOperationState::Failed);
    }

    #[test]
    fn engine_property_targets_after_unknown_are_explicitly_not_attempted() {
        let outcome = not_attempted_outcome(
            "display-b".into(),
            "not attempted after an unknown Wallpaper Engine provider state".into(),
        );

        assert_eq!(outcome.state, ApplyOperationState::Cancelled);
        assert_eq!(outcome.target_id, "display-b");
        assert!(outcome
            .error
            .as_deref()
            .is_some_and(|error| error.contains("not attempted after an unknown")));
    }

    #[test]
    fn resident_engine_with_confirmed_empty_target_uses_native_baseline() {
        let native = MonitorProviderState::Native {
            wallpaper: std::path::PathBuf::from(r"C:\wallpapers\baseline.bmp"),
            position: native_wallpaper::FILL_POSITION,
        };

        let state = provider_state_from_observation(
            true,
            &EngineTarget::bound("Monitor1", "Monitor1"),
            test_engine_observation(EngineObservationState::ConfirmedAbsent),
            Ok(native.clone()),
        )
        .expect("confirmed Engine absence should select the native baseline");

        assert_eq!(state, native);
    }

    #[test]
    fn unreadable_engine_target_blocks_unsafe_mutation() {
        let error = provider_state_from_observation(
            true,
            &EngineTarget::bound("Monitor1", "Monitor1"),
            test_engine_observation(EngineObservationState::Unreadable),
            Ok(MonitorProviderState::Native {
                wallpaper: std::path::PathBuf::from(r"C:\wallpapers\baseline.bmp"),
                position: native_wallpaper::FILL_POSITION,
            }),
        )
        .expect_err("unreadable Engine state must block the operation");

        assert!(error.starts_with("provider-state-unknown:"));
    }

    #[test]
    fn engine_recovery_uses_the_actual_scene_media_or_local_video_identity() {
        let scene = WallpaperItem {
            scan_metadata: None,
            id: "scene".into(),
            title: "Scene".into(),
            folder_path: std::path::PathBuf::from(r"C:\scene"),
            project_json_path: std::path::PathBuf::from(r"C:\scene\project.json"),
            file_target: std::path::PathBuf::from(r"C:\scene\scene.pkg"),
            preview_path: None,
            wallpaper_type: "scene".into(),
            source: WallpaperSource::WallpaperEngine,
        };
        let installed_media = WallpaperItem {
            wallpaper_type: "video".into(),
            file_target: std::path::PathBuf::from(r"C:\media\video.webm"),
            project_json_path: std::path::PathBuf::from(r"C:\media\project.json"),
            id: "installed-media".into(),
            ..scene.clone()
        };
        let local_video = WallpaperItem {
            id: "local-video".into(),
            source: WallpaperSource::LocalVideo,
            wallpaper_type: "video".into(),
            project_json_path: std::path::PathBuf::from(r"C:\local\loop.mp4"),
            file_target: std::path::PathBuf::from(r"C:\local\loop.mp4"),
            ..scene.clone()
        };

        for wallpaper in [scene, installed_media, local_video] {
            let recovery = engine_apply_recovery(
                OperationId(71),
                &wallpaper,
                "Monitor1",
                "Monitor1",
                "provider-state-unknown: operation did not settle".into(),
            );
            assert_eq!(
                recovery.identity,
                wallpaper.engine_target().display().to_string()
            );
            assert!(matches!(
                recovery.expected,
                ProviderPostcondition::EngineProject { .. }
            ));
        }
    }

    #[test]
    fn close_native_and_control_unknowns_retain_distinct_conditions() {
        let close = close_recovery(
            OperationId(72),
            "wallpaper-id".into(),
            "Monitor1",
            "Monitor1",
            "close did not settle".into(),
        );
        let native = native_recovery(
            OperationId(73),
            Some("wallpaper-id".into()),
            "Monitor1",
            std::path::PathBuf::from(r"C:\wallpapers\baseline.bmp"),
            native_wallpaper::FILL_POSITION.0,
            "restore did not settle".into(),
        );
        let control = engine_control_recovery(
            OperationId(74),
            "pause:true".into(),
            "wallpaper-engine-global",
            "pause/play value readback is unavailable".into(),
        );

        assert!(matches!(
            close.expected,
            ProviderPostcondition::EngineAbsent { .. }
        ));
        assert!(matches!(
            native.expected,
            ProviderPostcondition::NativeWallpaper { .. }
        ));
        assert!(matches!(
            control.expected,
            ProviderPostcondition::ReadbackUnavailable { .. }
        ));
    }

    #[test]
    fn matching_readback_settles_only_the_intended_retained_condition() {
        let older = close_recovery(
            OperationId(75),
            "wallpaper-id".into(),
            "Monitor1",
            "Monitor1",
            "older".into(),
        );
        let newer = close_recovery(
            OperationId(76),
            "wallpaper-id".into(),
            "Monitor1",
            "Monitor1",
            "newer".into(),
        );
        let mut pending = vec![older.clone(), newer.clone()];

        assert!(settle_matching_recovery(&mut pending, &older));
        assert_eq!(pending, vec![newer.clone()]);
        assert!(!settle_matching_recovery(&mut pending, &older));
        assert!(settle_matching_recovery(&mut pending, &newer));
        assert!(pending.is_empty());
    }

    #[test]
    fn failed_or_mismatched_readback_cannot_settle_a_recovery() {
        let expected = ProviderPostcondition::EngineProject {
            location: "Monitor1".into(),
            path: std::path::PathBuf::from(r"C:\fixture\scene\project.json"),
        };

        assert!(readback_satisfies(
            &expected,
            &ProviderReadback::Unreadable("transport failed".into()),
            Some(&EngineTarget::bound("Monitor1", "Monitor1")),
        )
        .is_err());
        assert!(readback_satisfies(
            &expected,
            &ProviderReadback::Engine(test_engine_observation(EngineObservationState::Present(
                std::path::PathBuf::from(r"C:\fixture\other\project.json")
            ))),
            Some(&EngineTarget::bound("Monitor1", "Monitor1")),
        )
        .is_err());
        assert!(readback_satisfies(
            &ProviderPostcondition::EngineAbsent {
                location: "Monitor1".into()
            },
            &ProviderReadback::Engine(test_engine_observation(
                EngineObservationState::ConfirmedAbsent
            )),
            Some(&EngineTarget::bound("Monitor1", "Monitor1")),
        )
        .is_ok());
    }
}

#[cfg(test)]
mod runtime_tests {
    use super::*;
    use std::path::{Path, PathBuf};

    struct RestoreState {
        device_id: String,
        location: Option<String>,
        original: PathBuf,
        test_dir: PathBuf,
    }

    impl Drop for RestoreState {
        fn drop(&mut self) {
            if let Some(location) = &self.location {
                let _ = engine_ipc::close_wallpaper(location);
            }
            let _ = native_wallpaper::restore_wallpaper_for_device(&self.original, &self.device_id);
            let _ = std::fs::remove_dir_all(&self.test_dir);
        }
    }

    fn local_item(path: &Path) -> WallpaperItem {
        WallpaperItem {
            scan_metadata: None,
            id: format!("runtime-local:{}", path.display()),
            title: "Runtime local image".to_string(),
            folder_path: path.parent().unwrap_or(Path::new("")).to_path_buf(),
            project_json_path: PathBuf::new(),
            file_target: path.to_path_buf(),
            preview_path: Some(path.to_path_buf()),
            wallpaper_type: "image".to_string(),
            source: WallpaperSource::LocalImage,
        }
    }

    fn engine_item(path: &Path) -> WallpaperItem {
        let project_path = path.join("project.json");
        let value: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(&project_path).expect("engine project.json"),
        )
        .expect("valid engine project metadata");
        let wallpaper_type = value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("scene")
            .to_string();
        let file = value
            .get("file")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("scene.pkg");
        WallpaperItem {
            scan_metadata: None,
            id: format!("runtime-engine:{}", path.display()),
            title: value
                .get("title")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("Runtime Engine project")
                .to_string(),
            folder_path: path.to_path_buf(),
            project_json_path: project_path,
            file_target: path.join(file),
            preview_path: None,
            wallpaper_type,
            source: WallpaperSource::WallpaperEngine,
        }
    }

    #[test]
    #[ignore = "changes and restores wallpaper while exercising every effect and provider handoff"]
    fn runtime_all_effects_and_provider_combinations() {
        assert!(
            !engine_ipc::is_running(),
            "the restoration test must start with Wallpaper Engine stopped"
        );
        let project_a = std::env::var_os("WSL_ENGINE_PROJECT_A")
            .map(PathBuf::from)
            .expect("WSL_ENGINE_PROJECT_A");
        let project_b = std::env::var_os("WSL_ENGINE_PROJECT_B")
            .map(PathBuf::from)
            .expect("WSL_ENGINE_PROJECT_B");
        let monitor = native_wallpaper::monitors()
            .unwrap()
            .into_iter()
            .next()
            .expect("desktop monitor");
        let original = native_wallpaper::get_wallpaper_for_device(&monitor.device_id).unwrap();
        let location = engine_location_for_device(&monitor.device_id).ok();
        eprintln!(
            "recorded state: device={} native={} engine_location={location:?}",
            monitor.device_id,
            original.display()
        );
        let test_dir = crate::config::Config::cache_dir().join("runtime-provider-test");
        std::fs::create_dir_all(&test_dir).unwrap();
        let local_a = test_dir.join("incoming-a.bmp");
        let local_b = test_dir.join("incoming-b.bmp");
        image::RgbaImage::from_pixel(64, 64, image::Rgba([49, 116, 143, 255]))
            .save(&local_a)
            .unwrap();
        image::RgbaImage::from_pixel(64, 64, image::Rgba([203, 105, 140, 255]))
            .save(&local_b)
            .unwrap();
        let restore = RestoreState {
            device_id: monitor.device_id.clone(),
            location,
            original: original.clone(),
            test_dir,
        };
        let target = ApplyTarget::DeviceId(monitor.device_id.clone());

        for effect in crate::transition::TRANSITION_CATALOG {
            let transition = TransitionConfig {
                effect,
                duration_ms: 120,
            };
            eprintln!("{effect:?}: local -> local");
            apply(ApplyRequest {
                operation_id: OperationId(0),
                wallpaper: local_item(&local_a),
                target: target.clone(),
                transition: transition.clone(),
            })
            .unwrap();
            eprintln!("{effect:?}: local -> Wallpaper Engine");
            apply(ApplyRequest {
                operation_id: OperationId(0),
                wallpaper: engine_item(&project_a),
                target: target.clone(),
                transition: transition.clone(),
            })
            .unwrap();
            eprintln!("{effect:?}: Wallpaper Engine -> Wallpaper Engine");
            apply(ApplyRequest {
                operation_id: OperationId(0),
                wallpaper: engine_item(&project_b),
                target: target.clone(),
                transition: transition.clone(),
            })
            .unwrap();
            eprintln!("{effect:?}: Wallpaper Engine -> local");
            apply(ApplyRequest {
                operation_id: OperationId(0),
                wallpaper: local_item(&local_b),
                target: target.clone(),
                transition,
            })
            .unwrap();
        }

        drop(restore);
        let restored = native_wallpaper::get_wallpaper_for_device(&monitor.device_id).unwrap();
        assert_eq!(
            std::fs::canonicalize(restored).unwrap(),
            std::fs::canonicalize(original).unwrap()
        );
    }

    #[test]
    #[ignore = "changes and restores the actual wallpaper while validating failed-apply rollback"]
    fn runtime_failed_apply_restores_the_targeted_monitor() {
        assert!(
            !engine_ipc::is_running(),
            "the rollback test must start with Wallpaper Engine stopped"
        );
        let monitor = native_wallpaper::monitors()
            .unwrap()
            .into_iter()
            .next()
            .expect("desktop monitor");
        let original = native_wallpaper::get_wallpaper_for_device(&monitor.device_id).unwrap();
        let missing = crate::config::Config::cache_dir()
            .join("runtime-provider-test")
            .join("intentionally-missing.png");
        let result = run_operation(ApplyRequest {
            operation_id: OperationId(0),
            wallpaper: local_item(&missing),
            target: ApplyTarget::DeviceId(monitor.device_id.clone()),
            transition: TransitionConfig {
                effect: TransitionEffect::Off,
                duration_ms: 0,
            },
        });

        assert!(!result.is_success());
        let restored = native_wallpaper::get_wallpaper_for_device(&monitor.device_id).unwrap();
        assert_eq!(
            std::fs::canonicalize(restored).unwrap(),
            std::fs::canonicalize(original).unwrap()
        );
    }
}

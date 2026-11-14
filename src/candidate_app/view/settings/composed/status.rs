//! Narrow source presentation data; no config IO, daemon, mutation, or worker.
use super::media::MediaKind;
#[derive(Debug, Clone, Default)]
pub struct AppThemeStatus {
    pub id: String,
    pub name: String,
    pub installed: bool,
    pub config_found: bool,
    pub config_path: String,
    pub output_path: String,
    pub enabled: bool,
    pub state: String,
    pub detail: String,
    pub can_enable: bool,
    pub can_disable: bool,
    pub can_adopt: bool,
    pub template_path: String,
    pub customized: bool,
    pub can_disconnect: bool,
    pub can_reconnect: bool,
}
#[derive(Debug, Clone, Default)]
pub struct AppThemesResult {
    pub apps: Vec<AppThemeStatus>,
}
#[derive(Debug, Clone, Default)]
pub struct OutputStatus {
    pub name: String,
    pub target: String,
    pub connected: bool,
    pub width: i32,
    pub height: i32,
    pub logical_width: i32,
    pub logical_height: i32,
    pub current: String,
    pub kind: MediaKind,
    pub path: String,
    pub we_id: String,
    pub mute: bool,
    pub volume: u32,
    pub fill: String,
    pub audio_shared: bool,
    pub paused: bool,
    pub manual_paused: bool,
}
#[derive(Debug, Clone, Default)]
pub struct LibraryWatchRootStatus {
    pub path: String,
    pub mode: String,
    pub native_error: Option<String>,
    pub last_completed_sweep_unix_ms: Option<u64>,
    pub last_scan_requested_unix_ms: Option<u64>,
    pub last_successful_convergence_unix_ms: Option<u64>,
    pub pending_scans: usize,
    pub last_poll_error: Option<String>,
}
#[derive(Debug, Clone, Default)]
pub struct LibraryWatchStatus {
    pub ok: bool,
    pub degraded: bool,
    pub mode: String,
    pub detail: String,
    pub interval_seconds: Option<u64>,
    pub entry_budget_per_root: Option<usize>,
    pub last_successful_convergence_unix_ms: Option<u64>,
    pub roots: Vec<LibraryWatchRootStatus>,
}
#[derive(Debug, Clone, Default)]
pub struct LibraryWatchMode;
#[derive(Debug, Clone, Default)]
pub struct PlaybackStatus {
    pub fullscreen_supported: bool,
    pub maximized_supported: bool,
    pub maximized_paused: bool,
    pub full_width_supported: bool,
    pub full_width_paused: bool,
    pub overview_paused: bool,
    pub processes: Vec<String>,
    pub outputs: Vec<String>,
    pub all_displays: bool,
    pub automatic_paused: bool,
    pub resume_pending: bool,
    pub other_audio_supported: bool,
    pub audio_ducked: bool,
    pub available_processes: Vec<String>,
}
impl OutputStatus {
    pub fn target(&self) -> &str {
        &self.target
    }
    pub fn is_connected(&self) -> bool {
        self.connected && !self.target.is_empty()
    }
    pub fn logical_size(&self) -> (i32, i32) {
        if self.logical_width > 0 && self.logical_height > 0 {
            (self.logical_width, self.logical_height)
        } else {
            (self.width, self.height)
        }
    }
}
impl LibraryWatchMode {
    pub const NATIVE: &str = "native";
    pub const POLLING: &str = "polling";
    pub const RECOVERING: &str = "recovering";
    pub const UNAVAILABLE: &str = "unavailable";
}

#[derive(Debug, Clone)]
pub enum TaskState {
    Running,
    Paused,
    Completed,
    Failed,
    Cancelled,
    Other(String),
}
impl TaskState {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Paused | Self::Other(_))
    }
}
#[derive(Debug, Clone)]
pub struct TaskStatus {
    pub state: TaskState,
    pub progress: u64,
    pub total: u64,
    pub detail: String,
}

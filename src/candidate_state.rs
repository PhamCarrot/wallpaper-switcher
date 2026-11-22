use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fmt;
use std::fs;
use std::io::{self, ErrorKind, Write};
use std::path::{Path, PathBuf};

use crate::file_publication;

#[cfg(test)]
#[path = "candidate_state/commit_test.rs"]
pub(crate) mod commit_test;

#[path = "candidate_state/legacy_depth.rs"]
pub mod legacy_depth;
#[path = "candidate_state/mode_profiles.rs"]
pub mod mode_profiles;
pub use legacy_depth::LegacyDepthSettings;

pub const CURRENT_STATE_VERSION: u32 = 4;
#[path = "candidate_state/local_library.rs"]
pub mod local_library;
#[cfg(test)]
#[path = "candidate_state/v4_tests.rs"]
mod v4_tests;
const DEPTH_SCHEMA_INTRODUCED_VERSION: i64 = 2;

#[path = "candidate_state/binding_codec.rs"]
pub mod binding_codec;
#[path = "candidate_state/frontend.rs"]
pub mod frontend;
#[path = "candidate_state/image_effects.rs"]
pub mod image_effects;
#[path = "candidate_state/pending.rs"]
pub mod pending;
#[path = "candidate_state/preferences.rs"]
pub mod preferences;
use frontend::{AppliedThemeSnapshot, PendingFrontendState, ThemeLibrary, ThemeOrigin};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GalleryMotionSpeed {
    Fast,
    Standard,
    #[default]
    Slow,
}

impl GalleryMotionSpeed {
    pub const fn duration_ms(self) -> u64 {
        match self {
            Self::Fast => 180,
            Self::Standard => 250,
            Self::Slow => 450,
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Fast => Self::Standard,
            Self::Standard => Self::Slow,
            Self::Slow => Self::Fast,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Fast => "Fast",
            Self::Standard => "Standard",
            Self::Slow => "Slow",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GalleryKind {
    #[default]
    Slices,
    Geometric,
    Wall,
    Depth,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SliceSettings {
    pub offset_x: f32,
    pub offset_y: f32,
    pub width: f32,
    pub expanded_width: f32,
    pub height: f32,
    pub spacing: f32,
    pub skew_distance: f32,
    pub edge_tilt_distance: f32,
    pub visible_count: usize,
    pub corner_radius: f32,
    pub wobble: bool,
    pub wobble_strength: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shadows: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallax: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round_corners: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configured_corners: Option<[f32; 4]>,
}

impl Default for SliceSettings {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            width: 135.0,
            expanded_width: 924.0,
            height: 520.0,
            spacing: -30.0,
            skew_distance: 35.0,
            edge_tilt_distance: 0.0,
            visible_count: 12,
            corner_radius: 16.0,
            wobble: false,
            wobble_strength: 1.0,
            shadows: None,
            parallax: None,
            round_corners: None,
            configured_corners: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GeometricShape {
    #[default]
    Hexagon,
    Circle,
    Triangle,
    Diamond,
    Rhombus,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GeometricCurve {
    Flat,
    #[default]
    Arc,
    Wave,
    S,
    Cylinder,
}

/// This one source-hidden setting accepts raw fractional edits, while a
/// profile Apply copies an exact integer. Keep both ordinary TOML number forms
/// in the same field; do not narrow untouched integers through f32/f64 storage.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum HexScrollStep {
    Integer(i64),
    Number(f64),
}
impl HexScrollStep {
    pub fn from_input(value: f64) -> Result<Self, String> {
        if !value.is_finite() {
            return Err("Enter a finite scroll step.".into());
        }
        // Match the source scalar normalizer's integral JSON encoding.
        Ok(
            if value.fract() == 0.0 && value >= i64::MIN as f64 && value <= i64::MAX as f64 {
                Self::Integer(value as i64)
            } else {
                Self::Number(value)
            },
        )
    }
    pub fn from_profile(value: u64) -> Result<Self, String> {
        i64::try_from(value)
            .map(Self::Integer)
            .map_err(|_| "Hex scroll step exceeds the supported TOML integer range.".into())
    }
    pub fn configured_number(self) -> f64 {
        match self {
            Self::Integer(value) => value as f64,
            Self::Number(value) => value,
        }
    }
    pub fn effective(self) -> u64 {
        // Reference getter: raw numeric value -> truncating/saturating usize,
        // then minimum one. Profile capture does not rewrite the raw number.
        (self.configured_number() as usize).max(1) as u64
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeometricSettings {
    pub radius: f32,
    pub rows: usize,
    pub columns: usize,
    pub shape: GeometricShape,
    pub curve: GeometricCurve,
    pub curve_strength: f32,
    pub curve_frequency: f32,
    pub gap_x: f32,
    pub gap_y: f32,
    pub aspect: f32,
    pub stagger: f32,
    pub lens: f32,
    pub orbit: f32,
    /// Cylinder orbit radius in pixels. Zero preserves the viewport-derived default.
    #[serde(default)]
    pub orbit_radius: f32,
    pub twist: f32,
    pub scatter: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parallax: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scroll_step: Option<HexScrollStep>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arc: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lens_radius: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<mode_profiles::StageTransform>,
}

impl Default for GeometricSettings {
    fn default() -> Self {
        Self {
            radius: 140.0,
            rows: 3,
            columns: 7,
            shape: GeometricShape::Hexagon,
            curve: GeometricCurve::Arc,
            curve_strength: 1.2,
            curve_frequency: 1.0,
            gap_x: 6.0,
            gap_y: 6.0,
            aspect: 1.0,
            stagger: 0.5,
            lens: 0.0,
            orbit: 0.0,
            orbit_radius: 0.0,
            twist: 0.0,
            scatter: 0.0,
            parallax: None,
            scroll_step: None,
            arc: None,
            lens_radius: None,
            stage: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WallLayout {
    Uniform,
    #[default]
    Brick,
    Masonry,
    Justified,
    Editorial,
    Cylinder,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WallSettings {
    pub columns: usize,
    pub rows: usize,
    pub card_width: f32,
    pub card_height: f32,
    pub gap_x: f32,
    pub gap_y: f32,
    pub corner_radius: f32,
    pub layout: WallLayout,
    pub stagger: f32,
    pub selected_scale: f32,
    pub flow_wave: f32,
    pub flow_frequency: f32,
    pub scatter: f32,
    pub scale_variance: f32,
    pub cylinder_bend: f32,
    pub cylinder_radius: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub round_corners: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub border_width: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<mode_profiles::StageTransform>,
}

impl Default for WallSettings {
    fn default() -> Self {
        Self {
            columns: 6,
            rows: 3,
            card_width: 300.0,
            card_height: 169.0,
            gap_x: 8.0,
            gap_y: 8.0,
            corner_radius: 12.0,
            layout: WallLayout::Uniform,
            stagger: 0.5,
            selected_scale: 1.0,
            flow_wave: 0.0,
            flow_frequency: 1.0,
            scatter: 0.0,
            scale_variance: 0.0,
            cylinder_bend: 0.65,
            cylinder_radius: 720.0,
            round_corners: None,
            border_width: None,
            stage: None,
        }
    }
}

/// Stable persisted names; shader IDs are explicit, never enum ordinals.
pub const DEPTH_EFFECTS: [&str; 12] = [
    "Ignite",
    "Edge Fracture",
    "Tonal Wipe",
    "Ash",
    "Depth Parallax",
    "Bokeh Bloom",
    "Light Streaks",
    "Voxel Extrude",
    "Pixel Sort",
    "Rack Focus",
    "Topographic",
    "Tonal Layers",
];
pub fn depth_effect_id(name: &str) -> Option<u32> {
    match name {
        "Ignite" => Some(0),
        "Edge Fracture" => Some(1),
        "Tonal Wipe" => Some(2),
        "Ash" => Some(3),
        "Depth Parallax" => Some(4),
        "Bokeh Bloom" => Some(5),
        "Light Streaks" => Some(6),
        "Voxel Extrude" => Some(7),
        "Pixel Sort" => Some(8),
        "Rack Focus" => Some(9),
        "Topographic" => Some(10),
        "Tonal Layers" => Some(11),
        _ => None,
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DepthDetailSettings {
    pub effect: String,
    pub animate_shader: bool,
    pub animate_back: bool,
    pub duration_ms: u64,
}
impl Default for DepthDetailSettings {
    fn default() -> Self {
        Self {
            effect: "Ignite".into(),
            animate_shader: true,
            animate_back: true,
            duration_ms: 1500,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DepthSettings {
    pub offset_x: f32,
    pub offset_y: f32,
    pub height: f32,
    pub width: f32,
    pub spacing: f32,
    pub visible_count: f32,
    pub falloff: f32,
    pub corner_radius: f32,
    pub skew_distance: f32,
    pub navigation_ms: f32,
    pub selection_frame: bool,
    pub shadows: bool,
    pub detail: DepthDetailSettings,
}
impl Default for DepthSettings {
    fn default() -> Self {
        LegacyDepthSettings::default().current_projection()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GalleryPreset {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depth: Option<LegacyDepthSettings>,
    pub name: String,
    pub mode: GalleryKind,
    /// A curated layout index, or `None` when this preset contains custom geometry.
    #[serde(default)]
    pub variant: Option<u8>,
    #[serde(default)]
    pub slices: SliceSettings,
    #[serde(default)]
    pub geometric: GeometricSettings,
    #[serde(default)]
    pub wall: WallSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GallerySettings {
    pub depth: DepthSettings,
    pub mode: GalleryKind,
    /// The active curated layout for `mode`; `None` means user-edited geometry.
    #[serde(default)]
    pub variant: Option<u8>,
    pub slices: SliceSettings,
    pub geometric: GeometricSettings,
    pub wall: WallSettings,
    pub presets: Vec<GalleryPreset>,
}

impl Default for GallerySettings {
    fn default() -> Self {
        Self {
            mode: GalleryKind::Slices,
            depth: DepthSettings::default(),
            variant: default_curated_variant(),
            slices: SliceSettings::default(),
            geometric: GeometricSettings::default(),
            wall: WallSettings::default(),
            presets: Vec::new(),
        }
    }
}

fn default_curated_variant() -> Option<u8> {
    Some(0)
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BackgroundPresentation {
    LiveTransparentOverlay,
    #[default]
    #[serde(other)]
    WallpaperBackdrop,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearanceSettings {
    pub presentation: BackgroundPresentation,
    pub preset: String,
    pub background: String,
    pub surface: String,
    pub text: String,
    pub muted: String,
    pub accent: String,
    pub overlay_opacity: f32,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            presentation: BackgroundPresentation::default(),
            preset: "nord".into(),
            background: "#2e3440".into(),
            surface: "#3b4252".into(),
            text: "#eceff4".into(),
            muted: "#4c566a".into(),
            accent: "#88c0d0".into(),
            overlay_opacity: 0.82,
        }
    }
}

// The RGB byte decoder is owned once by `crate::rgb_color` (extracted
// from this module without semantic change). This compatibility re-export
// keeps the established `candidate_state::parse_hex_rgb_bytes` entry point
// available to existing crate callers at its original visibility.
pub(crate) use crate::rgb_color::parse_hex_rgb_bytes;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlaybackOrder {
    #[default]
    Sequential,
    Shuffle,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OrientationFilter {
    #[default]
    Any,
    Landscape,
    Portrait,
    Square,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SmartRule {
    pub text: String,
    pub tags: BTreeSet<String>,
    pub excluded_tags: BTreeSet<String>,
    pub tags_match_any: bool,
    pub kinds: BTreeSet<String>,
    pub folders: BTreeSet<PathBuf>,
    pub minimum_width: u32,
    pub minimum_height: u32,
    /// Browse selection is transient; explicit smart-playlist capture persists the snapshot.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution_preset: Option<preferences::ResolutionPreset>,
    #[serde(flatten)]
    pub retained: local_library::Retained,
    pub orientation: OrientationFilter,
    pub favorites_only: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "membership", rename_all = "kebab-case")]
pub enum PlaylistMembership {
    Explicit { wallpaper_ids: Vec<String> },
    Smart { rule: SmartRule },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub order: PlaybackOrder,
    pub membership: PlaylistMembership,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Schedule {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    /// Sunday is bit 0; Saturday is bit 6.
    pub days: u8,
    /// Local minute from midnight, clamped to 0..1439 while evaluating.
    pub minute_of_day: u16,
    pub priority: u32,
    pub creation_order: u64,
    pub playlist_id: String,
    pub monitor_ids: Vec<String>,
}

impl Default for Schedule {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            enabled: false,
            days: 0,
            minute_of_day: 0,
            priority: 100,
            creation_order: 0,
            playlist_id: String::new(),
            monitor_ids: Vec::new(),
        }
    }
}

/// Independent interval-owner intent; absent records retain the exact legacy owner.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutomationIntent {
    pub random_enabled: bool,
    pub playlist_playing: bool,
    pub safely_stopped: bool,
    #[serde(flatten)]
    pub retained: local_library::Retained,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RandomRotationState {
    Off,
    On,
    PausedForPlaylist,
    SafelyStopped,
}

impl RandomRotationState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "Random rotation: Off",
            Self::On => "Random rotation: On",
            Self::PausedForPlaylist => "Random rotation: Paused for playlist",
            Self::SafelyStopped => "Random rotation: Safely stopped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct CandidateState {
    pub version: u32,
    /// Session-only protection survives compatibility loaders, clones and import previews.
    #[serde(skip)]
    pub write_restriction: Option<StateWriteRestriction>,
    #[serde(skip)]
    pub pending_migration: Option<MigrationProvenance>,
    #[serde(skip)]
    pub depth_diagnostics: Vec<String>,
    pub gallery: GallerySettings,
    #[serde(default)]
    pub gallery_motion_speed: GalleryMotionSpeed,
    pub appearance: AppearanceSettings,
    pub theme_library: ThemeLibrary,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub applied_theme: Option<AppliedThemeSnapshot>,
    pub mode_profiles: mode_profiles::ModeProfiles,
    pub pending_frontend: PendingFrontendState,
    pub frontend_preferences: preferences::FrontendPreferences,
    pub local_library: local_library::LocalLibraryState,
    #[serde(flatten)]
    pub retained: local_library::Retained,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card_flip: Option<DepthDetailSettings>,
    #[serde(skip)]
    pub reconciliation_required: Option<SaveError>,
    pub favorites: BTreeSet<String>,
    pub playlists: Vec<Playlist>,
    pub active_playlist_id: Option<String>,
    pub schedules: Vec<Schedule>,
    pub locked_monitors: BTreeSet<String>,
    pub manual_holdoff_until_unix: u64,
    /// Legacy shared bit; the optional independent record is authoritative when present.
    pub slideshow_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automation_intent: Option<AutomationIntent>,
    pub slideshow_interval_seconds: u64,
    pub global_paused: bool,
    pub global_muted: bool,
    pub global_volume: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateLoadSource {
    Primary,
    BackupRecovery,
    FirstUseDefaults,
    FailureDefaults,
    UnsupportedDefaults,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateWriteRestriction {
    pub path: PathBuf,
    pub version: i64,
}

impl fmt::Display for StateWriteRestriction {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "Unsupported candidate-state version {} at {}; this app supports version {}. Candidate-state writes are refused for this session to preserve the primary and backup. Use a compatible newer app or deliberately resolve the files outside this session.", self.version, self.path.display(), CURRENT_STATE_VERSION)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StateLoadOutcome {
    pub state: CandidateState,
    pub source: StateLoadSource,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackupStatus {
    Published,
    Preserved,
    Unavailable,
    ProtectedUnsupported,
}

impl BackupStatus {
    pub const fn is_available(self) -> bool {
        matches!(self, Self::Published | Self::Preserved)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveStage {
    CheckCompatibility,
    PrepareMigration,
    CreateDirectory,
    InspectExisting,
    PreparePrimary,
    PrepareBackup,
    PublishBackup,
    PublishPrimary,
}

impl fmt::Display for SaveStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::CheckCompatibility => "check state version compatibility",
            Self::PrepareMigration => "preserve raw migration snapshots",
            Self::CreateDirectory => "create state directory",
            Self::InspectExisting => "inspect existing state files",
            Self::PreparePrimary => "prepare primary temporary state",
            Self::PrepareBackup => "prepare backup temporary state",
            Self::PublishBackup => "publish backup",
            Self::PublishPrimary => "publish primary",
        };
        formatter.write_str(label)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveFileStatus {
    Valid,
    Missing,
    Invalid,
    Unreadable,
    UnsupportedVersion,
}

impl fmt::Display for SaveFileStatus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Valid => "valid",
            Self::Missing => "missing",
            Self::Invalid => "invalid",
            Self::Unreadable => "unreadable",
            Self::UnsupportedVersion => "protected unsupported version",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveOutcome {
    pub migration: Vec<MigrationSnapshot>,
    pub backup: BackupStatus,
}

impl SaveOutcome {
    pub fn backup_description(&self, backup_path: &Path) -> String {
        match self.backup {
            BackupStatus::Published => {
                format!("a valid backup was published at {}", backup_path.display())
            }
            BackupStatus::Preserved => {
                format!(
                    "the existing valid backup remains at {}",
                    backup_path.display()
                )
            }
            BackupStatus::Unavailable => "no valid backup is available".to_string(),
            BackupStatus::ProtectedUnsupported => format!(
                "the unsupported newer backup is preserved at {}; it is not current-format recovery data",
                backup_path.display()
            ),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveError {
    pub migration: Vec<MigrationSnapshot>,
    pub stage: SaveStage,
    pub primary: Option<SaveFileStatus>,
    pub backup: BackupStatus,
    pub write_restriction: Option<StateWriteRestriction>,
    pub(crate) os_code: Option<i32>,
    pub(crate) primary_committed: bool,
    recovery_paths: Vec<PathBuf>,
    message: String,
}

impl SaveError {
    fn new(stage: SaveStage, backup: BackupStatus, error: impl ToString) -> Self {
        Self {
            stage,
            primary: None,
            backup,
            write_restriction: None,
            migration: Vec::new(),
            os_code: None,
            primary_committed: false,
            recovery_paths: Vec::new(),
            message: error.to_string(),
        }
    }

    fn io(stage: SaveStage, backup: BackupStatus, error: io::Error) -> Self {
        let os_code = error.raw_os_error();
        Self {
            os_code,
            ..Self::new(stage, backup, error)
        }
    }

    fn observed(
        stage: SaveStage,
        primary: SaveFileStatus,
        backup: BackupStatus,
        recovery_paths: Vec<PathBuf>,
        error: impl ToString,
    ) -> Self {
        Self {
            stage,
            primary: Some(primary),
            backup,
            write_restriction: None,
            recovery_paths,
            migration: Vec::new(),
            os_code: None,
            primary_committed: false,
            message: error.to_string(),
        }
    }

    pub fn backup_description(&self, backup_path: &Path) -> String {
        SaveOutcome {
            migration: Vec::new(),
            backup: self.backup,
        }
        .backup_description(backup_path)
    }

    pub fn recovery_paths(&self) -> &[PathBuf] {
        &self.recovery_paths
    }

    pub fn recovery_description(&self) -> String {
        if self.recovery_paths.is_empty() {
            if self.backup.is_available() {
                "the canonical backup is known valid; no additional validated recovery path was established".to_string()
            } else {
                "no validated recovery copy was established".to_string()
            }
        } else {
            let paths = self
                .recovery_paths
                .iter()
                .map(|path| path.display().to_string())
                .collect::<Vec<_>>();
            format!("validated recovery path(s): {}", paths.join(", "))
        }
    }
}

impl fmt::Display for SaveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} did not complete: {}",
            self.stage, self.message
        )?;
        if let Some(primary) = self.primary {
            write!(formatter, "; primary after failure: {primary}")?;
        }
        write!(formatter, "; {}", self.recovery_description())?;
        if !self.migration.is_empty() {
            write!(
                formatter,
                "; raw migration snapshot records: {}",
                self.migration
                    .iter()
                    .map(|record| format!(
                        "{} ({:?}, original {})",
                        record.snapshot.display(),
                        record.status,
                        if record.original_absent {
                            "absent"
                        } else {
                            "present"
                        }
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for SaveError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveFailurePoint {
    CreateSnapshot,
    WriteSnapshot,
    FlushSnapshot,
    ReadSnapshot,
    PreparePrimary,
    FlushPrimary,
    PrepareBackup,
    FlushBackup,
    PublishBackup,
    PublishPrimary,
}

#[derive(Debug, Clone, Copy)]
struct SaveFault {
    point: SaveFailurePoint,
    kind: ErrorKind,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PublicationEffect {
    DestinationUnchanged,
    DestinationAbsent,
    OriginalMovedToKnownRecovery,
}

impl PublicationEffect {
    const fn error_code(self) -> i32 {
        match self {
            Self::DestinationUnchanged => 1175,
            Self::DestinationAbsent => 1176,
            Self::OriginalMovedToKnownRecovery => 1177,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PublicationFault {
    point: SaveFailurePoint,
    effect: PublicationEffect,
}

/// Exact session provenance is tied to a profile path and both inspected originals.
/// None records absence; these bytes are never serialized into the v2 state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationProvenance {
    pub path: PathBuf,
    primary: Option<Vec<u8>>,
    backup: Option<Vec<u8>>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MigrationSnapshotStatus {
    Created,
    Reused,
    Incomplete,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationSnapshot {
    pub original: PathBuf,
    pub snapshot: PathBuf,
    pub sha256: String,
    pub original_absent: bool,
    pub status: MigrationSnapshotStatus,
}
fn raw_original(path: &Path) -> io::Result<Option<Vec<u8>>> {
    match fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn raw_pair(path: &Path) -> io::Result<MigrationProvenance> {
    Ok(MigrationProvenance {
        path: path.to_path_buf(),
        primary: raw_original(path)?,
        backup: raw_original(&path.with_extension("toml.bak"))?,
    })
}
fn legacy_original(bytes: &Option<Vec<u8>>) -> bool {
    bytes.as_ref().is_some_and(|bytes| {
        let Ok(text) = std::str::from_utf8(bytes) else {
            return false;
        };
        if parse_state_contents(text).is_err() {
            return false;
        }
        let Ok(raw) = toml::from_str::<toml::Value>(text) else {
            return false;
        };
        raw.get("version")
            .and_then(toml::Value::as_integer)
            .unwrap_or(0)
            < i64::from(CURRENT_STATE_VERSION)
    })
}
impl MigrationProvenance {
    fn needs_migration(&self) -> bool {
        legacy_original(&self.primary) || legacy_original(&self.backup)
    }
    fn recheck(&self, stage: SaveStage) -> Result<(), SaveError> {
        let actual = raw_pair(&self.path).map_err(|e| {
            observed_save_error(
                stage,
                &self.path,
                &self.path.with_extension("toml.bak"),
                e.to_string(),
            )
        })?;
        if actual != *self {
            return Err(observed_save_error(
                stage,
                &self.path,
                &self.path.with_extension("toml.bak"),
                "migration originals changed after inspection; reload deliberately before saving",
            ));
        }
        Ok(())
    }
}
fn raw_version_label(bytes: &Option<Vec<u8>>) -> String {
    let Some(bytes) = bytes else {
        return "absent".into();
    };
    let Ok(text) = std::str::from_utf8(bytes) else {
        return "malformed".into();
    };
    if matches!(
        parse_state_contents(text),
        Err(StateFileError::Malformed(_))
    ) {
        return "malformed".into();
    }
    let Ok(raw) = toml::from_str::<toml::Value>(text) else {
        return "malformed".into();
    };
    match raw.get("version") {
        None => "v0".into(),
        Some(value) => match value.as_integer() {
            Some(version) if version >= 0 => format!("v{version}"),
            _ => "malformed".into(),
        },
    }
}

fn snapshot_path(profile: &Path, role: &str, bytes: &Option<Vec<u8>>) -> PathBuf {
    let hash = format!(
        "{:x}",
        Sha256::digest(bytes.as_deref().unwrap_or(b"absent\n"))
    );
    let mut name = profile.file_name().unwrap().to_os_string();
    let from = raw_version_label(bytes);
    name.push(format!(
        ".migration-{from}-to-v{CURRENT_STATE_VERSION}-{role}-{hash}.{}",
        if bytes.is_some() { "raw" } else { "absent" }
    ));
    profile.with_file_name(name)
}
fn snapshot_originals(
    plan: &MigrationProvenance,
    fault: Option<SaveFault>,
    records: &mut Vec<MigrationSnapshot>,
) -> Result<(), SaveError> {
    let backup_path = plan.path.with_extension("toml.bak");
    for (role, original, bytes) in [
        ("primary", &plan.path, &plan.primary),
        ("backup", &backup_path, &plan.backup),
    ] {
        let snapshot = snapshot_path(&plan.path, role, bytes);
        let content = bytes.as_deref().unwrap_or(b"absent\n");
        let hash = format!("{:x}", Sha256::digest(content));
        let marker = snapshot.with_extension(format!(
            "{}.complete",
            snapshot.extension().unwrap().to_string_lossy()
        ));
        let completion = format!("sha256={hash}\nbytes={}\n", content.len());
        let mut record = MigrationSnapshot {
            original: original.clone(),
            snapshot: snapshot.clone(),
            sha256: hash.clone(),
            original_absent: bytes.is_none(),
            status: MigrationSnapshotStatus::Incomplete,
        };
        let result = (|| -> io::Result<()> {
            if snapshot.exists() || marker.exists() {
                if fs::read(&snapshot)? != content || fs::read(&marker)? != completion.as_bytes() {
                    return Err(io::Error::new(ErrorKind::InvalidData, "existing migration snapshot or completion record mismatches; preserved for deliberate recovery"));
                }
                record.status = MigrationSnapshotStatus::Reused;
                return Ok(());
            }
            maybe_fail(fault, SaveFailurePoint::CreateSnapshot)?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&snapshot)?;
            maybe_fail(fault, SaveFailurePoint::WriteSnapshot)?;
            file.write_all(content)?;
            maybe_fail(fault, SaveFailurePoint::FlushSnapshot)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            maybe_fail(fault, SaveFailurePoint::ReadSnapshot)?;
            let readback = fs::read(&snapshot)?;
            if readback != content || format!("{:x}", Sha256::digest(&readback)) != hash {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "raw snapshot readback/hash mismatch",
                ));
            }
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&marker)?;
            file.write_all(completion.as_bytes())?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            if fs::read(&marker)? != completion.as_bytes() {
                return Err(io::Error::new(
                    ErrorKind::InvalidData,
                    "completion record readback mismatch",
                ));
            }
            record.status = MigrationSnapshotStatus::Created;
            Ok(())
        })();
        records.push(record);
        result.map_err(|e| {
            observed_save_error(
                SaveStage::PrepareMigration,
                &plan.path,
                &backup_path,
                e.to_string(),
            )
        })?;
    }
    Ok(())
}
fn sanitize_depth_geometry(settings: &mut DepthSettings) {
    let defaults = DepthSettings::default();
    settings.offset_x = finite_or(settings.offset_x, 0.0).clamp(-1.0, 1.0);
    settings.offset_y = finite_or(settings.offset_y, 0.0).clamp(-1.0, 1.0);
    settings.height = finite_or(settings.height, defaults.height).clamp(100.0, 1600.0);
    settings.width = finite_or(settings.width, defaults.width).clamp(40.0, 2400.0);
    settings.spacing = finite_or(settings.spacing, defaults.spacing).clamp(0.0, 4000.0);
    settings.visible_count =
        finite_or(settings.visible_count, defaults.visible_count).clamp(3.0, 21.0);
    settings.falloff = finite_or(settings.falloff, defaults.falloff).clamp(0.0, 1.0);
    settings.corner_radius =
        finite_or(settings.corner_radius, defaults.corner_radius).clamp(0.0, 100.0);
    settings.skew_distance =
        finite_or(settings.skew_distance, defaults.skew_distance).clamp(-200.0, 200.0);
    settings.navigation_ms =
        finite_or(settings.navigation_ms, defaults.navigation_ms).clamp(35.0, 8000.0);
}

fn sanitize_depth_detail(detail: &mut DepthDetailSettings, diagnostics: &mut Vec<String>) {
    detail.duration_ms = detail.duration_ms.clamp(100, 5000);
    if depth_effect_id(&detail.effect).is_none() {
        diagnostics.push(format!(
            "Unknown Depth detail effect {:?}; effective detail normalized to Ignite.",
            detail.effect
        ));
        detail.effect = "Ignite".into();
    }
}

/// Effective-copy sanitation for legacy projection and existing bounded consumers.
fn sanitize_depth(settings: &mut DepthSettings, diagnostics: &mut Vec<String>) {
    sanitize_depth_geometry(settings);
    sanitize_depth_detail(&mut settings.detail, diagnostics);
}

impl Default for CandidateState {
    fn default() -> Self {
        Self {
            version: CURRENT_STATE_VERSION,
            write_restriction: None,
            pending_migration: None,
            depth_diagnostics: Vec::new(),
            gallery: GallerySettings::default(),
            gallery_motion_speed: GalleryMotionSpeed::default(),
            appearance: AppearanceSettings::default(),
            theme_library: ThemeLibrary::default(),
            applied_theme: Some(AppliedThemeSnapshot::default()),
            mode_profiles: mode_profiles::ModeProfiles::default(),
            pending_frontend: PendingFrontendState::default(),
            frontend_preferences: preferences::FrontendPreferences::default(),
            local_library: Default::default(),
            retained: Default::default(),
            card_flip: Some(DepthDetailSettings::default()),
            reconciliation_required: None,
            favorites: BTreeSet::new(),
            playlists: Vec::new(),
            active_playlist_id: None,
            schedules: Vec::new(),
            locked_monitors: BTreeSet::new(),
            manual_holdoff_until_unix: 0,
            slideshow_enabled: false,
            automation_intent: None,
            slideshow_interval_seconds: 300,
            global_paused: false,
            global_muted: false,
            global_volume: 100,
        }
    }
}

pub(crate) fn commit_saved_state(
    current: &mut CandidateState,
    proposed: CandidateState,
    path: &std::path::Path,
) -> Result<SaveOutcome, SaveError> {
    commit_saved_state_using(current, proposed, path, |state| {
        #[cfg(test)]
        if let Some(fault) = commit_test::take() {
            return fault.save(state, path);
        }
        state.save(path)
    })
}

fn commit_saved_state_using(
    current: &mut CandidateState,
    mut proposed: CandidateState,
    path: &std::path::Path,
    save: impl FnOnce(&CandidateState) -> Result<SaveOutcome, SaveError>,
) -> Result<SaveOutcome, SaveError> {
    // A fresh/default proposed state must not bypass the loaded session restriction.
    proposed.pending_migration = current
        .pending_migration
        .clone()
        .or(proposed.pending_migration);
    proposed.reconciliation_required = current
        .reconciliation_required
        .clone()
        .or(proposed.reconciliation_required);
    proposed.write_restriction = current
        .write_restriction
        .clone()
        .or(proposed.write_restriction);
    let result = current.ensure_writable(path).and_then(|()| save(&proposed));
    match result {
        Ok(outcome) => {
            proposed.acknowledge_save(&outcome);
            *current = proposed;
            Ok(outcome)
        }
        Err(error) => {
            retain_state_write_restriction(current, &error);
            Err(error)
        }
    }
}

fn retain_state_write_restriction(state: &mut CandidateState, error: &SaveError) {
    if state.reconciliation_required.is_none()
        && matches!(
            error.stage,
            SaveStage::PublishPrimary | SaveStage::PublishBackup
        )
        && error.primary.is_some()
    {
        state.reconciliation_required = Some(error.clone());
    }
    if let Some(restriction) = &error.write_restriction {
        state.write_restriction = Some(restriction.clone());
    }
}

pub(crate) fn persist_candidate_state(
    state: &mut CandidateState,
    path: &std::path::Path,
) -> Result<SaveOutcome, SaveError> {
    persist_candidate_state_using(state, |state| {
        #[cfg(test)]
        if let Some(fault) = commit_test::take() {
            return fault.save(state, path);
        }
        state.save(path)
    })
}

fn persist_candidate_state_using(
    state: &mut CandidateState,
    save: impl FnOnce(&CandidateState) -> Result<SaveOutcome, SaveError>,
) -> Result<SaveOutcome, SaveError> {
    let result = save(state);
    match &result {
        Ok(outcome) => state.acknowledge_save(outcome),
        Err(error) => retain_state_write_restriction(state, error),
    }
    result
}

impl CandidateState {
    pub fn card_flip(&self) -> &DepthDetailSettings {
        self.card_flip
            .as_ref()
            .unwrap_or(&self.gallery.depth.detail)
    }
    pub fn card_flip_mut(&mut self) -> &mut DepthDetailSettings {
        self.ensure_shared_flip();
        self.card_flip.as_mut().expect("shared flip was seeded")
    }
    fn ensure_shared_flip(&mut self) {
        if self.card_flip.is_none() {
            let mut effective = self.gallery.depth.detail.clone();
            sanitize_depth_detail(&mut effective, &mut self.depth_diagnostics);
            self.card_flip = Some(effective);
        }
    }
    fn sanitize_current_depth(&mut self) {
        // Current geometry remains bounded. The old detail field is now a
        // preservation-only shadow; only a clone can seed missing shared data.
        sanitize_depth_geometry(&mut self.gallery.depth);
        self.ensure_shared_flip();
    }
    fn validate_frontend(&self) -> Result<(), String> {
        local_library::validate_retained(&self.retained)?;
        self.local_library.validate()?;
        if let Some(shared) = &self.card_flip {
            if depth_effect_id(&shared.effect).is_none()
                || !(100..=5000).contains(&shared.duration_ms)
            {
                return Err(
                    "Shared card-flip configuration is outside supported effect/duration bounds."
                        .into(),
                );
            }
        }
        let slices = &self.gallery.slices;
        let hex = &self.gallery.geometric;
        let wall = &self.gallery.wall;
        if hex
            .scroll_step
            .is_some_and(|value| !value.configured_number().is_finite())
        {
            return Err("Non-finite current Hex scroll step.".into());
        }
        if slices
            .configured_corners
            .iter()
            .flatten()
            .copied()
            .chain(hex.lens_radius)
            .chain(wall.border_width)
            .any(|value| !value.is_finite())
        {
            return Err("Non-finite current mode-local geometry.".into());
        }
        for stage in [hex.stage.as_ref(), wall.stage.as_ref()]
            .into_iter()
            .flatten()
        {
            if [
                stage.x,
                stage.y,
                stage.scale,
                stage.rotation_degrees,
                stage.perspective,
                stage.shear_x,
                stage.shear_y,
                stage.depth_angle_degrees,
            ]
            .into_iter()
            .any(|value| !value.is_finite())
            {
                return Err("Non-finite current stage geometry.".into());
            }
        }
        for playlist in &self.playlists {
            if let PlaylistMembership::Smart { rule } = &playlist.membership {
                local_library::validate_retained(&rule.retained)?;
                if let Some(snapshot) = &rule.resolution_preset {
                    snapshot.validate()?;
                }
            }
        }
        if let Some(intent) = &self.automation_intent {
            local_library::validate_retained(&intent.retained)?;
        }
        self.frontend_preferences.validate()?;
        self.theme_library.validate()?;
        self.pending_frontend.validate()?;
        self.mode_profiles.validate()?;
        if let Some(AppliedThemeSnapshot {
            origin: ThemeOrigin::Custom { record_id },
            ..
        }) = &self.applied_theme
        {
            if !self
                .theme_library
                .records
                .iter()
                .any(|r| r.id == *record_id)
            {
                return Err("Applied Theme origin has no library identity.".into());
            }
        }
        Ok(())
    }
    pub(crate) fn propose_theme_delete(&self, id: u64) -> Option<Self> {
        let mut proposed = self.clone();
        let index = proposed
            .theme_library
            .records
            .iter()
            .position(|record| record.id == id)?;
        proposed.theme_library.records.remove(index);
        if matches!(proposed.applied_theme.as_ref().map(|s| &s.origin), Some(ThemeOrigin::Custom { record_id }) if *record_id == id)
        {
            proposed.applied_theme = Some(AppliedThemeSnapshot::default());
        }
        Some(proposed)
    }
    pub fn load(path: &Path) -> Self {
        Self::load_with_diagnostics(path).state
    }

    pub fn load_with_diagnostics(path: &Path) -> StateLoadOutcome {
        let mut outcome = Self::load_diagnosed_inner(path);
        let mut diagnostics = outcome.diagnostic.take().into_iter().collect::<Vec<_>>();
        if outcome.state.pending_migration.is_some() {
            let plan = outcome.state.pending_migration.as_ref().unwrap();
            diagnostics.push(format!("Legacy state loaded read-only (primary {}, backup {}); first successful v{CURRENT_STATE_VERSION} save requires exact raw primary/backup migration snapshots.", raw_version_label(&plan.primary), raw_version_label(&plan.backup)));
        }
        diagnostics.extend(outcome.state.depth_diagnostics.iter().cloned());
        if !outcome.state.retained.is_empty()
            || !outcome.state.frontend_preferences.retained.is_empty()
            || outcome.state.local_library.has_retained()
        {
            diagnostics.push("Unknown local records retained unchanged and inactive.".into());
        }
        for entry in outcome.state.pending_frontend.settings.iter().chain(
            outcome
                .state
                .pending_frontend
                .presets
                .iter()
                .flat_map(|preset| &preset.settings),
        ) {
            if let Some(message) =
                pending::diagnostic(entry.capability, &entry.source_path, &entry.value_json)
            {
                diagnostics.push(message);
            }
        }
        if !outcome.state.pending_frontend.settings.is_empty()
            || !outcome.state.pending_frontend.presets.is_empty()
        {
            diagnostics
                .push("Saved pending configuration is inert; no execution was activated.".into());
        }
        outcome.diagnostic = (!diagnostics.is_empty()).then(|| diagnostics.join(" "));
        outcome
    }
    fn load_diagnosed_inner(path: &Path) -> StateLoadOutcome {
        let backup = path.with_extension("toml.bak");
        let primary_raw = raw_original(path);
        let backup_raw = raw_original(&backup);
        let decode = |raw: &io::Result<Option<Vec<u8>>>| match raw {
            Ok(Some(bytes)) => std::str::from_utf8(bytes)
                .map_err(|e| StateFileError::Malformed(e.to_string()))
                .and_then(parse_state_contents),
            Ok(None) => Err(StateFileError::Missing),
            Err(e) => Err(StateFileError::Read(e.to_string())),
        };
        let mut primary_result = decode(&primary_raw);
        let mut backup_result = decode(&backup_raw);
        if let (Ok(primary), Ok(backup_bytes)) = (&primary_raw, &backup_raw) {
            let plan = MigrationProvenance {
                path: path.to_path_buf(),
                primary: primary.clone(),
                backup: backup_bytes.clone(),
            };
            if plan.needs_migration() {
                for state in [&mut primary_result, &mut backup_result]
                    .into_iter()
                    .filter_map(|result| result.as_mut().ok())
                {
                    state.pending_migration = Some(plan.clone());
                }
            }
        }
        let restriction = match (&primary_result, &backup_result) {
            (Err(StateFileError::Unsupported(version)), _) => Some(StateWriteRestriction {
                path: path.to_path_buf(),
                version: *version,
            }),
            (_, Err(StateFileError::Unsupported(version))) => Some(StateWriteRestriction {
                path: backup.clone(),
                version: *version,
            }),
            _ => None,
        };
        if let Some(restriction) = restriction {
            // Never activate settings or automation from a future profile or its fallback.
            let (mut state, source, presentation) = match primary_result {
                Ok(state) => (state, StateLoadSource::Primary, "Loaded the supported primary for this session with persistence disabled."),
                Err(_) => (Self::default(), StateLoadSource::UnsupportedDefaults, "Showing nonpersistent defaults; no profile settings or backup automation were loaded."),
            };
            let diagnostic = Some(format!("{restriction} {presentation}"));
            state.write_restriction = Some(restriction);
            return StateLoadOutcome {
                state,
                source,
                diagnostic,
            };
        }
        match primary_result {
            Ok(state) => StateLoadOutcome {
                state,
                source: StateLoadSource::Primary,
                diagnostic: None,
            },
            Err(primary_error) => match backup_result {
                Ok(state) => StateLoadOutcome {
                    state,
                    source: StateLoadSource::BackupRecovery,
                    diagnostic: Some(format!(
                        "Recovered candidate state from the backup because the primary could not be loaded ({}).",
                        primary_error
                    )),
                },
                Err(backup_error) => {
                    let first_use = primary_error.is_missing() && backup_error.is_missing();
                    StateLoadOutcome {
                        state: Self::default(),
                        source: if first_use {
                            StateLoadSource::FirstUseDefaults
                        } else {
                            StateLoadSource::FailureDefaults
                        },
                        diagnostic: Some(if first_use {
                            "Using default candidate state for first use; no primary or backup state exists."
                                .to_string()
                        } else {
                            format!(
                                "Using default candidate state because neither the primary nor backup could be loaded (primary: {}; backup: {}).",
                                primary_error, backup_error
                            )
                        }),
                    }
                }
            },
        }
    }

    pub fn automation_intent(&self) -> AutomationIntent {
        self.automation_intent
            .clone()
            .unwrap_or_else(|| AutomationIntent {
                random_enabled: self.slideshow_enabled && self.active_playlist_id.is_none(),
                playlist_playing: self.slideshow_enabled && self.active_playlist_id.is_some(),
                ..Default::default()
            })
    }

    pub fn random_rotation_state(&self) -> RandomRotationState {
        let intent = self.automation_intent();
        if !intent.random_enabled {
            RandomRotationState::Off
        } else if intent.safely_stopped {
            RandomRotationState::SafelyStopped
        } else if intent.playlist_playing {
            RandomRotationState::PausedForPlaylist
        } else {
            RandomRotationState::On
        }
    }

    fn sanitize(&mut self) {
        self.version = CURRENT_STATE_VERSION;
        self.global_volume = self.global_volume.min(100);
        self.slideshow_interval_seconds = self.slideshow_interval_seconds.max(10);
        sanitize_gallery(&mut self.gallery);
        self.sanitize_current_depth();
        if self
            .active_playlist_id
            .as_ref()
            .is_some_and(|id| !self.playlists.iter().any(|playlist| &playlist.id == id))
        {
            // Capture the lost required owner before clearing its legacy selected ID.
            let mut intent = self.automation_intent();
            if intent.playlist_playing {
                intent.playlist_playing = false;
                intent.safely_stopped = true;
                self.automation_intent = Some(intent);
            }
            self.active_playlist_id = None;
        }
        if self.automation_intent().playlist_playing && self.active_playlist_id.is_none() {
            let intent = self.automation_intent.get_or_insert_with(Default::default);
            intent.playlist_playing = false;
            intent.safely_stopped = true;
        }
        sanitize_appearance(&mut self.appearance);
        self.appearance.overlay_opacity =
            finite_or(self.appearance.overlay_opacity, 0.82).clamp(0.0, 1.0);
    }

    pub fn save(&self, path: &Path) -> Result<SaveOutcome, SaveError> {
        self.save_impl(path, None, None, |_| {})
    }

    /// Only the common successful publication boundary acknowledges migration.
    pub fn acknowledge_save(&mut self, _outcome: &SaveOutcome) {
        self.version = CURRENT_STATE_VERSION;
        self.pending_migration = None;
    }
    pub fn ensure_writable(&self, path: &Path) -> Result<(), SaveError> {
        let restriction = self.write_restriction.clone().or_else(|| {
            (self.version > CURRENT_STATE_VERSION).then(|| StateWriteRestriction {
                path: path.to_path_buf(),
                version: i64::from(self.version),
            })
        });
        if let Some(restriction) = restriction {
            return Err(version_refusal(
                path,
                SaveStage::CheckCompatibility,
                restriction,
            ));
        }
        ensure_compatible_files(path, SaveStage::CheckCompatibility)?;
        if let Some(error) = &self.reconciliation_required {
            let mut error = error.clone();
            error
                .message
                .push_str("; disk may have changed; deliberately reload/reconcile before retrying");
            return Err(error);
        }
        Ok(())
    }

    #[cfg(test)]
    fn save_with_fault(&self, path: &Path, fault: SaveFault) -> Result<SaveOutcome, SaveError> {
        self.save_impl(path, Some(fault), None, |_| {})
    }

    #[cfg(test)]
    fn save_with_publication_effect(
        &self,
        path: &Path,
        point: SaveFailurePoint,
        effect: PublicationEffect,
    ) -> Result<SaveOutcome, SaveError> {
        self.save_impl(path, None, Some(PublicationFault { point, effect }), |_| {})
    }

    fn save_impl(
        &self,
        path: &Path,
        fault: Option<SaveFault>,
        publication_fault: Option<PublicationFault>,
        mut checkpoint: impl FnMut(SaveStage),
    ) -> Result<SaveOutcome, SaveError> {
        // Known incompatibility must refuse before directory creation or staging.
        self.ensure_writable(path)?;
        self.validate_frontend().map_err(|error| {
            SaveError::new(
                SaveStage::PreparePrimary,
                backup_status(&inspect_state_file(&path.with_extension("toml.bak"))),
                error,
            )
        })?;
        let parent = path.parent().ok_or_else(|| {
            SaveError::new(
                SaveStage::CreateDirectory,
                BackupStatus::Unavailable,
                "state path has no parent",
            )
        })?;
        if let Err(error) = fs::create_dir_all(parent) {
            return Err(SaveError::io(
                SaveStage::CreateDirectory,
                BackupStatus::Unavailable,
                error,
            ));
        }

        let temporary = unique_sibling(path, "state-tmp");
        let backup_temporary = unique_sibling(path, "backup-tmp");
        let backup = path.with_extension("toml.bak");
        let initial_backup_state = backup_status(&inspect_state_file(&backup));
        let mut publication_fault = publication_fault;
        let mut publish_file_for = |point: SaveFailurePoint,
                                    replacement: &Path,
                                    destination: &Path| {
            let mut replace_existing = |replacement: &Path, destination: &Path, recovery: &Path| {
                if publication_fault
                    .as_ref()
                    .is_some_and(|fault| fault.point == point)
                {
                    let fault = publication_fault
                        .take()
                        .expect("publication fault was checked above");
                    apply_publication_effect(replacement, destination, recovery, fault.effect)?;
                    return Err(io::Error::from_raw_os_error(fault.effect.error_code()));
                }
                file_publication::replace_existing_file(replacement, destination, recovery)
            };
            publish_file_with(replacement, destination, &mut replace_existing)
        };
        let mut migration = Vec::new();
        let result = (|| {
            let mut persisted = self.clone();
            persisted.version = CURRENT_STATE_VERSION;
            persisted.sanitize_current_depth();
            // Keep partial text in the live edit state, but only write safe colors.
            sanitize_appearance(&mut persisted.appearance);
            let contents = toml::to_string_pretty(&persisted).map_err(|error| {
                SaveError::new(SaveStage::PreparePrimary, initial_backup_state, error)
            })?;
            write_complete_file(
                &temporary,
                contents.as_bytes(),
                fault,
                SaveFailurePoint::PreparePrimary,
                SaveFailurePoint::FlushPrimary,
                SaveStage::PreparePrimary,
                initial_backup_state,
            )?;
            load_state_file(&temporary).map_err(|error| {
                SaveError::new(SaveStage::PreparePrimary, initial_backup_state, error)
            })?;

            checkpoint(SaveStage::InspectExisting);
            ensure_compatible_files(path, SaveStage::InspectExisting)?;

            let primary = inspect_state_file(path);
            let existing_backup = inspect_state_file(&backup);
            if let StateFileInspection::Unreadable(error) = &primary {
                return Err(SaveError::new(
                    SaveStage::InspectExisting,
                    backup_status(&existing_backup),
                    error,
                ));
            }
            if let StateFileInspection::Unreadable(error) = &existing_backup {
                return Err(SaveError::new(
                    SaveStage::InspectExisting,
                    BackupStatus::Unavailable,
                    error,
                ));
            }

            let raw = raw_pair(path).map_err(|error| {
                SaveError::new(SaveStage::InspectExisting, initial_backup_state, error)
            })?;
            if let StateFileInspection::Valid(bytes) = &primary {
                if raw.primary.as_ref() != Some(bytes) {
                    return Err(observed_save_error(
                        SaveStage::InspectExisting,
                        path,
                        &backup,
                        "primary changed during inspection; reload deliberately before saving",
                    ));
                }
            }
            if let StateFileInspection::Valid(bytes) = &existing_backup {
                if raw.backup.as_ref() != Some(bytes) {
                    return Err(observed_save_error(
                        SaveStage::InspectExisting,
                        path,
                        &backup,
                        "backup changed during inspection; reload deliberately before saving",
                    ));
                }
            }
            if let Some(loaded) = &self.pending_migration {
                if loaded != &raw {
                    return Err(observed_save_error(
                        SaveStage::InspectExisting,
                        path,
                        &backup,
                        "loaded migration provenance changed; reload deliberately before saving",
                    ));
                }
            }
            let mut expected = raw.needs_migration().then_some(raw);
            if let Some(plan) = &expected {
                snapshot_originals(plan, fault, &mut migration)?;
            }
            let mut backup_state = backup_status(&existing_backup);
            if let StateFileInspection::Valid(previous_primary) = primary {
                write_complete_file(
                    &backup_temporary,
                    &previous_primary,
                    fault,
                    SaveFailurePoint::PrepareBackup,
                    SaveFailurePoint::FlushBackup,
                    SaveStage::PrepareBackup,
                    backup_state,
                )?;
                load_state_file(&backup_temporary).map_err(|error| {
                    SaveError::new(SaveStage::PrepareBackup, backup_state, error)
                })?;
                maybe_fail(fault, SaveFailurePoint::PublishBackup).map_err(|error| {
                    SaveError::new(SaveStage::PublishBackup, backup_state, error)
                })?;
                checkpoint(SaveStage::PublishBackup);
                ensure_compatible_files(path, SaveStage::PublishBackup)?;
                if let Some(plan) = &expected {
                    plan.recheck(SaveStage::PublishBackup)?;
                }
                publish_file_for(SaveFailurePoint::PublishBackup, &backup_temporary, &backup)
                    .map_err(|error| {
                        publication_save_error(SaveStage::PublishBackup, path, &backup, error)
                    })?;
                let published_backup = inspect_state_file(&backup);
                if !matches!(published_backup, StateFileInspection::Valid(_)) {
                    return Err(observed_save_error(
                        SaveStage::PublishBackup,
                        path,
                        &backup,
                        "published backup could not be validated",
                    ));
                }
                if let Some(plan) = &mut expected {
                    plan.backup = Some(previous_primary);
                }
                backup_state = BackupStatus::Published;
            }

            maybe_fail(fault, SaveFailurePoint::PublishPrimary)
                .map_err(|error| SaveError::new(SaveStage::PublishPrimary, backup_state, error))?;
            checkpoint(SaveStage::PublishPrimary);
            ensure_compatible_files(path, SaveStage::PublishPrimary)?;
            if let Some(plan) = &expected {
                plan.recheck(SaveStage::PublishPrimary)?;
            }
            publish_file_for(SaveFailurePoint::PublishPrimary, &temporary, path).map_err(
                |error| publication_save_error(SaveStage::PublishPrimary, path, &backup, error),
            )?;
            if !matches!(inspect_state_file(path), StateFileInspection::Valid(_)) {
                return Err(observed_save_error(
                    SaveStage::PublishPrimary,
                    path,
                    &backup,
                    "primary publication did not leave a valid primary state",
                ));
            }
            Ok(SaveOutcome {
                migration: migration.clone(),
                backup: backup_state,
            })
        })();

        let result = result.map_err(|mut error| {
            error.migration = migration;
            error
        });
        let recovery_paths = result
            .as_ref()
            .err()
            .map(|error| error.recovery_paths().to_vec())
            .unwrap_or_default();
        if !recovery_paths.iter().any(|path| path == &temporary) {
            let _ = file_publication::remove_if_present(&temporary);
        }
        if !recovery_paths.iter().any(|path| path == &backup_temporary) {
            let _ = file_publication::remove_if_present(&backup_temporary);
        }
        result
    }

    pub fn toggle_favorite(&mut self, wallpaper_id: &str) -> bool {
        if self.favorites.remove(wallpaper_id) {
            false
        } else {
            self.favorites.insert(wallpaper_id.to_string());
            true
        }
    }

    pub fn begin_manual_holdoff(&mut self, now_unix: u64) {
        self.manual_holdoff_until_unix = now_unix.saturating_add(10 * 60);
    }

    pub fn automation_allowed(&self, now_unix: u64) -> bool {
        now_unix >= self.manual_holdoff_until_unix
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum StateFileError {
    Missing,
    Read(String),
    Malformed(String),
    Unsupported(i64),
}

impl StateFileError {
    const fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}

impl fmt::Display for StateFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing => formatter.write_str("file is missing"),
            Self::Read(error) => write!(formatter, "read failed: {error}"),
            Self::Malformed(error) => write!(formatter, "malformed state: {error}"),
            Self::Unsupported(version) => {
                write!(formatter, "unsupported candidate-state version {version}")
            }
        }
    }
}

#[derive(Debug)]
enum StateFileInspection {
    Missing,
    Valid(Vec<u8>),
    Invalid,
    Unreadable(String),
    Unsupported(i64),
}

fn load_state_file(path: &Path) -> Result<CandidateState, StateFileError> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == ErrorKind::NotFound => return Err(StateFileError::Missing),
        Err(error) => return Err(StateFileError::Read(error.to_string())),
    };
    parse_state_contents(&contents)
}

fn parse_state_contents(contents: &str) -> Result<CandidateState, StateFileError> {
    if contents.trim().is_empty() {
        return Err(StateFileError::Malformed("the file is empty".to_string()));
    }
    let value = toml::from_str::<toml::Value>(contents)
        .map_err(|error| StateFileError::Malformed(error.to_string()))?;
    let Some(table) = value.as_table() else {
        return Err(StateFileError::Malformed(
            "the TOML document has no state table".to_string(),
        ));
    };
    // Classify the raw signed TOML integer without narrowing to the model's u32.
    // Missing version and version 0 retain the established partial legacy format.
    if let Some(value) = table.get("version") {
        match value.as_integer() {
            Some(version) if version > i64::from(CURRENT_STATE_VERSION) => {
                return Err(StateFileError::Unsupported(version));
            }
            Some(version) if version >= 0 => {}
            _ => {
                return Err(StateFileError::Malformed(
                    "version must be a nonnegative TOML integer".to_string(),
                ))
            }
        }
    }
    const RECOGNIZED_FIELDS: &[&str] = &[
        "version",
        "gallery",
        "gallery_motion_speed",
        "appearance",
        "theme_library",
        "applied_theme",
        "mode_profiles",
        "pending_frontend",
        "frontend_preferences",
        "local_library",
        "card_flip",
        "favorites",
        "playlists",
        "active_playlist_id",
        "schedules",
        "locked_monitors",
        "manual_holdoff_until_unix",
        "slideshow_enabled",
        "automation_intent",
        "slideshow_interval_seconds",
        "global_paused",
        "global_muted",
        "global_volume",
    ];
    if !table
        .keys()
        .any(|key| RECOGNIZED_FIELDS.iter().any(|field| key == *field))
    {
        return Err(StateFileError::Malformed(
            "the TOML document has no recognized state fields".to_string(),
        ));
    }
    let mut state = toml::from_str::<CandidateState>(contents)
        .map_err(|error| StateFileError::Malformed(error.to_string()))?;
    let source_version = table
        .get("version")
        .and_then(toml::Value::as_integer)
        .unwrap_or(0);
    let legacy_presets = state.gallery.presets.clone();
    if source_version < DEPTH_SCHEMA_INTRODUCED_VERSION {
        state.gallery.depth = DepthSettings::default();
        for preset in &mut state.gallery.presets {
            preset.depth = None;
        }
    }
    if source_version < 3 {
        state.applied_theme = None;
    }
    // Keep complete decoded legacy payloads before any runtime sanitation.
    if source_version < 3 && !table.contains_key("mode_profiles") {
        state.mode_profiles =
            mode_profiles::ModeProfiles::from_legacy(&legacy_presets, source_version)
                .map_err(StateFileError::Malformed)?;
    }
    state.sanitize();
    state
        .validate_frontend()
        .map_err(StateFileError::Malformed)?;
    Ok(state)
}

fn inspect_state_file(path: &Path) -> StateFileInspection {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == ErrorKind::NotFound => return StateFileInspection::Missing,
        Err(error) => return StateFileInspection::Unreadable(error.to_string()),
    };
    match String::from_utf8(bytes.clone()) {
        Ok(contents) => match parse_state_contents(&contents) {
            Ok(_) => StateFileInspection::Valid(bytes),
            Err(StateFileError::Unsupported(version)) => StateFileInspection::Unsupported(version),
            Err(_) => StateFileInspection::Invalid,
        },
        Err(_) => StateFileInspection::Invalid,
    }
}

fn backup_status(inspection: &StateFileInspection) -> BackupStatus {
    match inspection {
        StateFileInspection::Valid(_) => BackupStatus::Preserved,
        StateFileInspection::Unsupported(_) => BackupStatus::ProtectedUnsupported,
        _ => BackupStatus::Unavailable,
    }
}

fn save_file_status(inspection: &StateFileInspection) -> SaveFileStatus {
    match inspection {
        StateFileInspection::Valid(_) => SaveFileStatus::Valid,
        StateFileInspection::Missing => SaveFileStatus::Missing,
        StateFileInspection::Invalid => SaveFileStatus::Invalid,
        StateFileInspection::Unreadable(_) => SaveFileStatus::Unreadable,
        StateFileInspection::Unsupported(_) => SaveFileStatus::UnsupportedVersion,
    }
}

fn version_refusal(path: &Path, stage: SaveStage, restriction: StateWriteRestriction) -> SaveError {
    let mut error = SaveError::observed(
        stage,
        save_file_status(&inspect_state_file(path)),
        backup_status(&inspect_state_file(&path.with_extension("toml.bak"))),
        Vec::new(),
        &restriction,
    );
    error.write_restriction = Some(restriction);
    error
}

fn ensure_compatible_files(path: &Path, stage: SaveStage) -> Result<(), SaveError> {
    for file in [path.to_path_buf(), path.with_extension("toml.bak")] {
        if let StateFileInspection::Unsupported(version) = inspect_state_file(&file) {
            return Err(version_refusal(
                path,
                stage,
                StateWriteRestriction {
                    path: file,
                    version,
                },
            ));
        }
    }
    Ok(())
}

fn unique_sibling(path: &Path, suffix: &str) -> PathBuf {
    let filename = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "candidate-state".to_string());
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    path.with_file_name(format!(
        "{filename}.{suffix}.{}.{}",
        std::process::id(),
        timestamp
    ))
}

fn write_complete_file(
    path: &Path,
    contents: &[u8],
    fault: Option<SaveFault>,
    prepare_point: SaveFailurePoint,
    flush_point: SaveFailurePoint,
    stage: SaveStage,
    backup: BackupStatus,
) -> Result<(), SaveError> {
    maybe_fail(fault, prepare_point).map_err(|error| SaveError::io(stage, backup, error))?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| SaveError::io(stage, backup, error))?;
    file.write_all(contents)
        .map_err(|error| SaveError::io(stage, backup, error))?;
    maybe_fail(fault, flush_point).map_err(|error| SaveError::io(stage, backup, error))?;
    file.flush()
        .and_then(|()| file.sync_all())
        .map_err(|error| SaveError::io(stage, backup, error))?;
    Ok(())
}

fn maybe_fail(fault: Option<SaveFault>, point: SaveFailurePoint) -> io::Result<()> {
    if let Some(fault) = fault.filter(|fault| fault.point == point) {
        Err(io::Error::new(
            fault.kind,
            format!("injected persistence failure at {point:?}"),
        ))
    } else {
        Ok(())
    }
}

#[derive(Debug)]
struct PublicationError {
    error: io::Error,
    replacement: PathBuf,
    destination: PathBuf,
    backup: Option<PathBuf>,
    replace_file_called: bool,
    publication_succeeded: bool,
}

impl From<file_publication::StagedPublicationError> for PublicationError {
    fn from(publication: file_publication::StagedPublicationError) -> Self {
        Self {
            error: publication.error,
            replacement: publication.replacement,
            destination: publication.destination,
            backup: publication.recovery,
            replace_file_called: publication.replace_file_called,
            publication_succeeded: publication.publication_succeeded,
        }
    }
}

impl fmt::Display for PublicationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.replace_file_called && self.publication_succeeded {
            write!(
                formatter,
                "ReplaceFileW completed, but cleanup of its tracked recovery copy failed: {}",
                self.error
            )?;
        } else if self.replace_file_called {
            write!(
                formatter,
                "ReplaceFileW returned {}: {}",
                file_publication::replace_file_error_name(&self.error),
                self.error
            )?;
        } else if !file_publication::replace_file_was_called(&self.error) {
            write!(formatter, "{}", self.error)?;
        } else {
            write!(formatter, "same-directory rename failed: {}", self.error)?;
        }
        write!(
            formatter,
            "; replacement was {}; destination was {}",
            self.replacement.display(),
            self.destination.display()
        )?;
        if let Some(backup) = &self.backup {
            write!(formatter, "; tracked backup was {}", backup.display())?;
        }
        Ok(())
    }
}

fn publish_file_with<F>(
    replacement: &Path,
    destination: &Path,
    replace_existing: &mut F,
) -> Result<(), PublicationError>
where
    F: FnMut(&Path, &Path, &Path) -> io::Result<()>,
{
    file_publication::publish_staged_file_with(
        replacement,
        destination,
        "candidate-state-replace-recovery",
        replace_existing,
    )
    .map_err(Into::into)
}

fn add_valid_recovery_path(
    paths: &mut Vec<PathBuf>,
    path: &Path,
    inspection: &StateFileInspection,
) {
    if matches!(inspection, StateFileInspection::Valid(_)) && !paths.iter().any(|item| item == path)
    {
        paths.push(path.to_path_buf());
    }
}

fn publication_save_error(
    stage: SaveStage,
    primary_path: &Path,
    backup_path: &Path,
    error: PublicationError,
) -> SaveError {
    let primary = inspect_state_file(primary_path);
    let backup = inspect_state_file(backup_path);
    let replacement = inspect_state_file(&error.replacement);
    let tracked_backup = error
        .backup
        .as_ref()
        .map(|path| (path, inspect_state_file(path)));
    let mut recovery_paths = Vec::new();
    add_valid_recovery_path(&mut recovery_paths, primary_path, &primary);
    if !file_publication::failed_before_replace_file(&error.error) {
        add_valid_recovery_path(&mut recovery_paths, &error.replacement, &replacement);
    }
    if let Some((path, inspection)) = &tracked_backup {
        add_valid_recovery_path(&mut recovery_paths, path, inspection);
    }
    add_valid_recovery_path(&mut recovery_paths, backup_path, &backup);
    SaveError {
        os_code: error.error.raw_os_error(),
        primary_committed: stage == SaveStage::PublishPrimary
            && error.publication_succeeded
            && matches!(primary, StateFileInspection::Valid(_)),
        ..SaveError::observed(
            stage,
            save_file_status(&primary),
            backup_status(&backup),
            recovery_paths,
            error,
        )
    }
}

fn observed_save_error(
    stage: SaveStage,
    primary_path: &Path,
    backup_path: &Path,
    message: impl ToString,
) -> SaveError {
    let primary = inspect_state_file(primary_path);
    let backup = inspect_state_file(backup_path);
    let mut recovery_paths = Vec::new();
    add_valid_recovery_path(&mut recovery_paths, primary_path, &primary);
    add_valid_recovery_path(&mut recovery_paths, backup_path, &backup);
    SaveError::observed(
        stage,
        save_file_status(&primary),
        backup_status(&backup),
        recovery_paths,
        message,
    )
}

fn apply_publication_effect(
    replacement: &Path,
    destination: &Path,
    backup: &Path,
    effect: PublicationEffect,
) -> io::Result<()> {
    let _ = replacement;
    match effect {
        PublicationEffect::DestinationUnchanged => Ok(()),
        PublicationEffect::DestinationAbsent => file_publication::remove_if_present(destination),
        PublicationEffect::OriginalMovedToKnownRecovery => fs::rename(destination, backup),
    }
}

fn sanitize_appearance(appearance: &mut AppearanceSettings) {
    let defaults = AppearanceSettings::default();
    sanitize_color(&mut appearance.background, &defaults.background);
    sanitize_color(&mut appearance.surface, &defaults.surface);
    sanitize_color(&mut appearance.text, &defaults.text);
    sanitize_color(&mut appearance.muted, &defaults.muted);
    sanitize_color(&mut appearance.accent, &defaults.accent);
}

fn sanitize_color(value: &mut String, fallback: &str) {
    if parse_hex_rgb_bytes(value).is_none() {
        *value = fallback.to_owned();
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn sanitize_gallery(gallery: &mut GallerySettings) {
    if gallery.mode == GalleryKind::Depth {
        gallery.variant = None;
    }
    sanitize_slices(&mut gallery.slices);
    sanitize_geometric(&mut gallery.geometric);
    sanitize_wall(&mut gallery.wall);
}

fn sanitize_slices(settings: &mut SliceSettings) {
    let defaults = SliceSettings::default();
    settings.offset_x = finite_or(settings.offset_x, defaults.offset_x).clamp(-2.0, 2.0);
    settings.offset_y = finite_or(settings.offset_y, defaults.offset_y).clamp(-2.0, 2.0);
    settings.width = finite_or(settings.width, defaults.width).clamp(1.0, 4096.0);
    settings.expanded_width =
        finite_or(settings.expanded_width, defaults.expanded_width).clamp(1.0, 4096.0);
    settings.height = finite_or(settings.height, defaults.height).clamp(1.0, 4096.0);
    settings.spacing = finite_or(settings.spacing, defaults.spacing).clamp(-4096.0, 4096.0);
    settings.skew_distance =
        finite_or(settings.skew_distance, defaults.skew_distance).clamp(-4096.0, 4096.0);
    settings.edge_tilt_distance =
        finite_or(settings.edge_tilt_distance, defaults.edge_tilt_distance).clamp(-4096.0, 4096.0);
    settings.visible_count = settings.visible_count.clamp(1, 256);
    settings.corner_radius =
        finite_or(settings.corner_radius, defaults.corner_radius).clamp(0.0, 2048.0);
    settings.wobble_strength =
        finite_or(settings.wobble_strength, defaults.wobble_strength).clamp(0.0, 10.0);
}

fn sanitize_geometric(settings: &mut GeometricSettings) {
    let defaults = GeometricSettings::default();
    settings.radius = finite_or(settings.radius, defaults.radius).clamp(1.0, 2048.0);
    settings.rows = settings.rows.clamp(1, 128);
    settings.columns = settings.columns.clamp(1, 128);
    settings.curve_strength =
        finite_or(settings.curve_strength, defaults.curve_strength).clamp(-10.0, 10.0);
    settings.curve_frequency =
        finite_or(settings.curve_frequency, defaults.curve_frequency).clamp(-20.0, 20.0);
    settings.gap_x = finite_or(settings.gap_x, defaults.gap_x).clamp(-2048.0, 2048.0);
    settings.gap_y = finite_or(settings.gap_y, defaults.gap_y).clamp(-2048.0, 2048.0);
    settings.aspect = finite_or(settings.aspect, defaults.aspect).clamp(0.05, 10.0);
    settings.stagger = finite_or(settings.stagger, defaults.stagger).clamp(-4.0, 4.0);
    settings.lens = finite_or(settings.lens, defaults.lens).clamp(-0.85, 8.0);
    settings.orbit = finite_or(settings.orbit, defaults.orbit).clamp(-1.0, 1.0);
    settings.orbit_radius =
        finite_or(settings.orbit_radius, defaults.orbit_radius).clamp(0.0, 8192.0);
    settings.twist = finite_or(settings.twist, defaults.twist).clamp(-720.0, 720.0);
    settings.scatter = finite_or(settings.scatter, defaults.scatter).clamp(0.0, 4096.0);
}

fn sanitize_wall(settings: &mut WallSettings) {
    let defaults = WallSettings::default();
    settings.columns = settings.columns.clamp(1, 128);
    settings.rows = settings.rows.clamp(1, 128);
    settings.card_width = finite_or(settings.card_width, defaults.card_width).clamp(1.0, 4096.0);
    settings.card_height = finite_or(settings.card_height, defaults.card_height).clamp(1.0, 4096.0);
    settings.gap_x = finite_or(settings.gap_x, defaults.gap_x).clamp(-2048.0, 2048.0);
    settings.gap_y = finite_or(settings.gap_y, defaults.gap_y).clamp(-2048.0, 2048.0);
    settings.corner_radius =
        finite_or(settings.corner_radius, defaults.corner_radius).clamp(0.0, 2048.0);
    settings.stagger = finite_or(settings.stagger, defaults.stagger).clamp(-4.0, 4.0);
    settings.selected_scale =
        finite_or(settings.selected_scale, defaults.selected_scale).clamp(0.1, 10.0);
    settings.flow_wave = finite_or(settings.flow_wave, defaults.flow_wave).clamp(-4096.0, 4096.0);
    settings.flow_frequency =
        finite_or(settings.flow_frequency, defaults.flow_frequency).clamp(-20.0, 20.0);
    settings.scatter = finite_or(settings.scatter, defaults.scatter).clamp(0.0, 4096.0);
    settings.scale_variance =
        finite_or(settings.scale_variance, defaults.scale_variance).clamp(0.0, 1.0);
    settings.cylinder_bend =
        finite_or(settings.cylinder_bend, defaults.cylinder_bend).clamp(-1.0, 1.0);
    settings.cylinder_radius =
        finite_or(settings.cylinder_radius, defaults.cylinder_radius).clamp(1.0, 8192.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn b2_state_cleanup_failure_keeps_success_facts_and_published_bytes() {
        let root = test_root("b2-cleanup-after-success");
        fs::create_dir_all(&root).unwrap();
        let destination = root.join("state.toml");
        let staged = root.join("staged.toml");
        let previous = toml::to_string_pretty(&state_with_favorite("previous")).unwrap();
        let next = toml::to_string_pretty(&state_with_favorite("next")).unwrap();
        fs::write(&destination, &previous).unwrap();
        fs::write(&staged, &next).unwrap();
        let publication = publish_file_with(
            &staged,
            &destination,
            &mut |replacement: &Path, destination: &Path, recovery: &Path| {
                file_publication::replace_existing_file(replacement, destination, recovery)?;
                // Fail recovery cleanup after a real successful replacement.
                let bytes = fs::read(recovery)?;
                fs::remove_file(recovery)?;
                fs::create_dir(recovery)?;
                fs::write(recovery.join("previous.toml"), bytes)
            },
        )
        .unwrap_err();
        assert!(publication.replace_file_called);
        assert!(publication.publication_succeeded);
        assert_eq!(publication.replacement, staged);
        assert_eq!(publication.destination, destination);
        let recovery = publication.backup.clone().unwrap();
        assert!(recovery
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("candidate-state-replace-recovery"));
        assert_eq!(
            fs::read(recovery.join("previous.toml")).unwrap(),
            previous.as_bytes()
        );
        assert_eq!(fs::read(&destination).unwrap(), next.as_bytes());
        assert!(!staged.exists());
        let error = publication_save_error(
            SaveStage::PublishPrimary,
            &destination,
            &destination.with_extension("toml.bak"),
            publication,
        );
        assert_eq!(error.stage, SaveStage::PublishPrimary);
        assert!(error
            .to_string()
            .contains("ReplaceFileW completed, but cleanup"));
        assert!(error.recovery_paths().contains(&destination));
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn r3_long_state_replacement_and_corrupt_primary_recovery_keep_exact_backups() {
        use std::os::windows::ffi::OsStrExt;
        let root = test_root("r3-long-state");
        let directory = root
            .join("valid state directory 日本語".repeat(4))
            .join("deep-valid-state-".repeat(6));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        assert!(path.as_os_str().encode_wide().count() > 260);
        state_with_favorite("first").save(&path).unwrap();
        let first = fs::read(&path).unwrap();
        state_with_favorite("second").save(&path).unwrap();
        assert_eq!(fs::read(&backup).unwrap(), first);
        assert!(CandidateState::load(&path).favorites.contains("second"));
        let preserved_backup = fs::read(&backup).unwrap();
        fs::write(&path, "invalid toml = [").unwrap();
        let mut recovered = CandidateState::load(&path);
        recovered.favorites.insert("recovered".into());
        recovered.save(&path).unwrap();
        assert_eq!(fs::read(&backup).unwrap(), preserved_backup);
        assert!(CandidateState::load(&path).favorites.contains("recovered"));
        eprintln!(
            "R3 state path={path:?}, UTF16 units={}",
            path.as_os_str().encode_wide().count()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn r3_before_call_validation_preserves_state_and_cleans_owned_stages() {
        let root = test_root("r3-component-rejection");
        fs::create_dir_all(&root).unwrap();
        // Primary/stage components fit NTFS; the generated ReplaceFile backup
        // name is longer than the volume component limit and is rejected first.
        let path = root.join(format!("{}.toml", "x".repeat(196)));
        state_with_favorite("original").save(&path).unwrap();
        let original = fs::read(&path).unwrap();
        let error = state_with_favorite("new").save(&path).unwrap_err();
        assert_eq!(error.stage, SaveStage::PublishPrimary);
        assert!(error.to_string().contains("before ReplaceFileW"));
        assert!(!error.to_string().contains("ReplaceFileW returned"));
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read(path.with_extension("toml.bak")).unwrap(), original);
        assert_eq!(
            fs::read_dir(&root).unwrap().count(),
            2,
            "Owned staged files must not be retained after a known pre-call rejection"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn r3_actual_rename_failure_retains_valid_state_stage() {
        let root = test_root("r3-rename-failure");
        fs::create_dir_all(&root).unwrap();
        let staged = root.join("staged.toml");
        fs::write(
            &staged,
            toml::to_string_pretty(&state_with_favorite("staged")).unwrap(),
        )
        .unwrap();
        let destination = root.join("missing-parent").join("state.toml");
        let publication = publish_file_with(&staged, &destination, &mut |_, _, _| {
            panic!("Missing destination uses rename")
        })
        .unwrap_err();
        assert!(!publication.replace_file_called);
        let error = publication_save_error(
            SaveStage::PublishPrimary,
            &destination,
            &destination.with_extension("toml.bak"),
            publication,
        );
        assert!(error.recovery_paths().contains(&staged));
        assert!(CandidateState::load(&staged).favorites.contains("staged"));
        fs::remove_dir_all(root).unwrap();
    }

    fn test_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "wsl-candidate-state-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn d2_raw_snapshot_enforced_for_default_proposal_without_load() {
        let root = test_root("d2-default-red");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("state.toml");
        let backup = path.with_extension("toml.bak");
        let primary =
            b"# exact primary comment\nversion = 1\nfavorites = ['old']\n[unknown]\nfuture = 7\n";
        let old_backup = b"# exact distinct backup\nfavorites = ['backup']\n";
        fs::write(&path, primary).unwrap();
        fs::write(&backup, old_backup).unwrap();
        CandidateState::default().save(&path).unwrap();
        let archived: Vec<_> = fs::read_dir(&root)
            .unwrap()
            .map(|x| x.unwrap().path())
            .filter(|x| {
                x.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .contains(&format!("-to-v{CURRENT_STATE_VERSION}-"))
            })
            .collect();
        assert!(
            archived.iter().any(|x| fs::read(x).unwrap() == primary),
            "shared default save lost raw primary snapshot"
        );
        assert!(
            archived.iter().any(|x| fs::read(x).unwrap() == old_backup),
            "shared default save lost raw backup snapshot"
        );
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn d2_loaded_legacy_refuses_intervening_source_change() {
        let root = test_root("d2-stale-red");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("state.toml");
        fs::write(&path, b"version = 1\nfavorites = ['old']\n").unwrap();
        let loaded = CandidateState::load(&path);
        let changed = b"version = 1\nfavorites = ['external']\n";
        fs::write(&path, changed).unwrap();
        assert!(
            loaded.save(&path).is_err(),
            "stale loaded migration overwrote external legacy state"
        );
        assert_eq!(fs::read(&path).unwrap(), changed);
        fs::remove_dir_all(root).unwrap();
    }

    fn d2_assert_snapshots(
        records: &[MigrationSnapshot],
        primary: Option<&[u8]>,
        backup: Option<&[u8]>,
    ) {
        assert_eq!(records.len(), 2);
        for (record, bytes) in records.iter().zip([primary, backup]) {
            assert_ne!(record.status, MigrationSnapshotStatus::Incomplete);
            assert_eq!(record.original_absent, bytes.is_none());
            assert_eq!(
                fs::read(&record.snapshot).unwrap(),
                bytes.unwrap_or(b"absent\n")
            );
            assert_eq!(
                record.sha256,
                format!("{:x}", Sha256::digest(bytes.unwrap_or(b"absent\n")))
            );
        }
    }
    #[test]
    fn d3_depth_mode_is_supported_without_a_second_migration() {
        let state = parse_state_contents("version=2\n[gallery]\nmode='depth'")
            .expect("D3 normal startup must support mode=depth");
        assert_eq!(state.version, CURRENT_STATE_VERSION);
        assert_eq!(state.gallery.depth, DepthSettings::default());
    }

    #[test]
    fn d2_schema_defaults_ids_custom_and_explicit_variants_roundtrip() {
        for header in ["", "version = 0\n", "version = 1\n", "version = 2\n"] {
            for variant in ["", "variant = 3\n"] {
                let input=format!("{header}favorites=['stable:id']\nactive_playlist_id='playlist'\nlocked_monitors=['monitor']\n[[playlists]]\nid='playlist'\nname='One'\norder='sequential'\nmembership={{membership='explicit',wallpaper_ids=['stable:id']}}\n[[schedules]]\nid='schedule'\nplaylist_id='playlist'\nminute_of_day=720\n[gallery]\nmode='wall'\n{variant}[gallery.slices]\nwidth=145.0\n[[gallery.presets]]\nname='old'\nmode='geometric'\nvariant=2\n");
                let state = parse_state_contents(&input).unwrap();
                assert_eq!(state.version, CURRENT_STATE_VERSION);
                assert_eq!(state.gallery.depth, DepthSettings::default());
                assert_eq!(state.gallery.presets[0].depth, None);
                assert_eq!(
                    state.gallery.variant,
                    if variant.is_empty() { None } else { Some(3) }
                );
                assert_eq!(state.gallery.slices.width, 145.0);
                assert_eq!(state.active_playlist_id.as_deref(), Some("playlist"));
                assert_eq!(state.schedules[0].id, "schedule");
                assert!(state.favorites.contains("stable:id"));
                let encoded = toml::to_string_pretty(&state).unwrap();
                assert_eq!(parse_state_contents(&encoded).unwrap(), state);
            }
        }
        assert_eq!(
            parse_state_contents("version=2\n[gallery]\nmode='depth'")
                .unwrap()
                .gallery
                .mode,
            GalleryKind::Depth
        );
        assert!(matches!(
            parse_state_contents("version=2\n[gallery]\nmode='unrecognized'"),
            Err(StateFileError::Malformed(_))
        ));
    }
    #[test]
    fn d2_all_fourteen_fields_twelve_effects_and_optional_preset_roundtrip() {
        for (id, effect) in DEPTH_EFFECTS.iter().enumerate() {
            let mut state = CandidateState::default();
            state.gallery.mode = GalleryKind::Wall;
            state.gallery.variant = None;
            state.gallery.depth = DepthSettings {
                offset_x: 0.0,
                offset_y: 0.0,
                height: 640.0,
                width: 380.0,
                spacing: 700.0,
                visible_count: 8.0,
                falloff: 0.4,
                corner_radius: 32.0,
                skew_distance: -42.0,
                navigation_ms: 750.0,
                selection_frame: true,
                shadows: true,
                detail: DepthDetailSettings {
                    effect: (*effect).into(),
                    animate_shader: false,
                    animate_back: false,
                    duration_ms: 2300,
                },
            };
            assert_eq!(depth_effect_id(effect), Some(id as u32));
            state.gallery.presets.push(GalleryPreset {
                name: "internal payload".into(),
                mode: GalleryKind::Wall,
                variant: None,
                slices: SliceSettings::default(),
                geometric: GeometricSettings::default(),
                wall: WallSettings::default(),
                depth: Some(LegacyDepthSettings::try_from(&state.gallery.depth).unwrap()),
            });
            assert_eq!(
                parse_state_contents(&toml::to_string_pretty(&state).unwrap()).unwrap(),
                state
            );
        }
        assert_eq!(depth_effect_id("no such effect"), None);
    }
    #[test]
    fn d2_depth_ranges_nonfinite_partial_defaults_and_unknown_effect_diagnostic() {
        let input="version=2\n[gallery.depth]\nheight=-1.0\nwidth=inf\nspacing=9999.0\nvisible_count=22\nfalloff=nan\ncorner_radius=999.0\nskew_distance=-999.0\nnavigation_ms=1\n[gallery.depth.detail]\neffect='unknown'\nduration_ms=99999";
        let state = parse_state_contents(input).unwrap();
        let d = &state.gallery.depth;
        assert_eq!(
            (
                d.height,
                d.width,
                d.spacing,
                d.visible_count,
                d.falloff,
                d.corner_radius,
                d.skew_distance,
                d.navigation_ms
            ),
            (100.0, 280.0, 4000.0, 21.0, 0.05, 100.0, -200.0, 35.0)
        );
        assert_eq!(d.detail.duration_ms, 99999);
        assert_eq!(d.detail.effect, "unknown");
        assert_eq!(state.card_flip().duration_ms, 5000);
        assert_eq!(state.card_flip().effect, "Ignite");
        assert!(d.detail.animate_shader && d.detail.animate_back);
        assert_eq!(state.depth_diagnostics.len(), 1);
        assert_eq!(
            parse_state_contents("version=2\n[gallery.depth]\nvisible_count=3.5")
                .unwrap()
                .gallery
                .depth
                .visible_count,
            3.5
        );
        let mut low = DepthSettings {
            offset_x: f32::NAN,
            offset_y: 2.0,
            height: f32::NAN,
            width: 0.0,
            spacing: f32::NEG_INFINITY,
            visible_count: 0.0,
            falloff: -3.0,
            corner_radius: f32::INFINITY,
            skew_distance: f32::NAN,
            navigation_ms: 99999.0,
            selection_frame: false,
            shadows: false,
            detail: DepthDetailSettings {
                duration_ms: 0,
                ..DepthDetailSettings::default()
            },
        };
        sanitize_depth(&mut low, &mut Vec::new());
        assert_eq!([low.offset_x, low.offset_y], [0.0, 1.0]);
        assert_eq!(
            (
                low.height,
                low.width,
                low.spacing,
                low.visible_count,
                low.falloff,
                low.corner_radius,
                low.skew_distance,
                low.navigation_ms,
                low.detail.duration_ms
            ),
            (520.0, 40.0, 280.0, 3.0, 0.0, 0.0, 0.0, 8000.0, 100)
        );
        assert_eq!(
            parse_state_contents("version=2\n[gallery.depth]\nwidth=450.0")
                .unwrap()
                .gallery
                .depth,
            DepthSettings {
                width: 450.0,
                ..DepthSettings::default()
            }
        );
    }
    #[test]
    fn d2_snapshot_raw_pair_matrix_and_readonly_load() {
        let legacy =
            b"# preserve legacy comments\nversion=1\nfavorites=['legacy']\n[unknown]\nx=8\n"
                .as_slice();
        let backup = b"# distinct backup\nfavorites=['backup']\n".as_slice();
        let current = b"# current pair\nversion=2\nfavorites=['current']\n".as_slice();
        let corrupt = b"# retain corrupt primary\nbroken=[".as_slice();
        for (primary, old_backup) in [
            (Some(legacy), Some(backup)),
            (Some(corrupt), Some(backup)),
            (None, Some(backup)),
            (Some(legacy), None),
            (Some(legacy), Some(current)),
            (Some(current), Some(backup)),
        ] {
            for fresh in [false, true] {
                let root = test_root("d2-pair-matrix");
                fs::create_dir_all(&root).unwrap();
                let path = root.join("state.toml");
                let bak = path.with_extension("toml.bak");
                if let Some(p) = primary {
                    fs::write(&path, p).unwrap();
                }
                if let Some(b) = old_backup {
                    fs::write(&bak, b).unwrap();
                }
                let loaded = CandidateState::load_with_diagnostics(&path);
                assert!(loaded.state.pending_migration.is_some());
                assert_eq!(
                    fs::read_dir(&root).unwrap().count(),
                    usize::from(primary.is_some()) + usize::from(old_backup.is_some())
                );
                let mut proposed = if fresh {
                    CandidateState::default()
                } else {
                    loaded.state
                };
                let result = proposed.save(&path).unwrap();
                d2_assert_snapshots(&result.migration, primary, old_backup);
                proposed.acknowledge_save(&result);
                let archives = result.migration.clone();
                for _ in 0..3 {
                    let out = proposed.save(&path).unwrap();
                    proposed.acknowledge_save(&out);
                }
                // A legacy rolling backup can remain after first publication; the next save may
                // snapshot that newly inspected pair, but acknowledged provenance must not block.
                for record in archives {
                    assert_eq!(
                        fs::read(&record.snapshot).unwrap(),
                        if record.original == path {
                            primary.unwrap_or(b"absent\n")
                        } else {
                            old_backup.unwrap_or(b"absent\n")
                        }
                    );
                }
                assert!(proposed.pending_migration.is_none());
                fs::remove_dir_all(root).unwrap();
            }
        }
    }
    #[test]
    fn d2_new_current_profile_needs_no_legacy_snapshot() {
        let root = test_root("d2-new");
        let path = root.join("state.toml");
        let mut state = CandidateState::default();
        for _ in 0..4 {
            let out = state.save(&path).unwrap();
            assert!(out.migration.is_empty());
            state.acknowledge_save(&out);
        }
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn d2_snapshot_failure_preserves_originals_and_failed_attempt() {
        for point in [
            SaveFailurePoint::CreateSnapshot,
            SaveFailurePoint::WriteSnapshot,
            SaveFailurePoint::FlushSnapshot,
            SaveFailurePoint::ReadSnapshot,
        ] {
            let root = test_root("d2-snapshot-fault");
            fs::create_dir_all(&root).unwrap();
            let path = root.join("state.toml");
            let bak = path.with_extension("toml.bak");
            let p = b"version=1\nfavorites=['primary']";
            let b = b"# backup raw\nversion=1\nfavorites=['backup']";
            fs::write(&path, p).unwrap();
            fs::write(&bak, b).unwrap();
            let loaded = CandidateState::load(&path);
            let out = loaded
                .save_with_fault(
                    &path,
                    SaveFault {
                        point,
                        kind: ErrorKind::PermissionDenied,
                    },
                )
                .unwrap_err();
            assert_eq!(out.stage, SaveStage::PrepareMigration);
            assert_eq!(fs::read(&path).unwrap(), p);
            assert_eq!(fs::read(&bak).unwrap(), b);
            assert!(loaded.pending_migration.is_some());
            assert_eq!(out.migration.len(), 1);
            assert_eq!(out.migration[0].status, MigrationSnapshotStatus::Incomplete);
            if point != SaveFailurePoint::CreateSnapshot {
                let attempt = out.migration[0].snapshot.clone();
                assert!(attempt.exists());
                let bytes = fs::read(&attempt).unwrap();
                loaded.save(&path).unwrap_err();
                assert_eq!(fs::read(attempt).unwrap(), bytes);
            } else {
                let good = loaded.save(&path).unwrap();
                d2_assert_snapshots(&good.migration, Some(p), Some(b));
            }
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn d2_completed_identical_snapshot_retry_and_mismatch_refusal() {
        let root = test_root("d2-snapshot-retry");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("state.toml");
        let bak = path.with_extension("toml.bak");
        let p = b"version=1\nfavorites=['primary']";
        let b = b"version=1\nfavorites=['backup']";
        fs::write(&path, p).unwrap();
        fs::write(&bak, b).unwrap();
        let state = CandidateState::load(&path);
        let out = state
            .save_with_fault(
                &path,
                SaveFault {
                    point: SaveFailurePoint::PublishBackup,
                    kind: ErrorKind::PermissionDenied,
                },
            )
            .unwrap_err();
        d2_assert_snapshots(&out.migration, Some(p), Some(b));
        assert_eq!(fs::read(&path).unwrap(), p);
        assert_eq!(fs::read(&bak).unwrap(), b);
        let first = out.migration[0].snapshot.clone();
        fs::write(&first, b"mismatched").unwrap();
        let error = state.save(&path).unwrap_err();
        assert_eq!(error.stage, SaveStage::PrepareMigration);
        assert_eq!(fs::read(&first).unwrap(), b"mismatched");
        assert_eq!(fs::read(&path).unwrap(), p);
        assert_eq!(fs::read(&bak).unwrap(), b);
        // Restore this owned corrupted fixture to its completed exact bytes for retry.
        fs::write(&first, p).unwrap();
        let saved = state.save(&path).unwrap();
        assert!(saved
            .migration
            .iter()
            .all(|r| r.status == MigrationSnapshotStatus::Reused));
        d2_assert_snapshots(&saved.migration, Some(p), Some(b));
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn d2_stale_identity_at_both_publication_boundaries_and_after_own_backup() {
        for stage in [SaveStage::PublishBackup, SaveStage::PublishPrimary] {
            for change_backup in [false, true] {
                let root = test_root("d2-publish-stale");
                fs::create_dir_all(&root).unwrap();
                let path = root.join("state.toml");
                let bak = path.with_extension("toml.bak");
                let p = b"version=1\nfavorites=['primary']";
                let b = b"version=1\nfavorites=['backup']";
                fs::write(&path, p).unwrap();
                fs::write(&bak, b).unwrap();
                let loaded = CandidateState::load(&path);
                let external = b"# supported external change\nversion=1\nfavorites=['external']";
                let error = loaded
                    .save_impl(&path, None, None, |checkpoint| {
                        if checkpoint == stage {
                            fs::write(if change_backup { &bak } else { &path }, external).unwrap()
                        }
                    })
                    .unwrap_err();
                assert_eq!(error.stage, stage);
                assert!(error.to_string().contains("changed"));
                d2_assert_snapshots(&error.migration, Some(p), Some(b));
                assert_eq!(
                    fs::read(if change_backup { &bak } else { &path }).unwrap(),
                    external
                );
                assert_eq!(
                    fs::read(if change_backup { &path } else { &bak }).unwrap(),
                    if change_backup || stage == SaveStage::PublishPrimary {
                        p.as_slice()
                    } else {
                        b.as_slice()
                    }
                );
                assert_eq!(error.primary, Some(SaveFileStatus::Valid));
                assert!(error.backup.is_available());
                assert!(loaded.pending_migration.is_some());
                fs::remove_dir_all(root).unwrap();
            }
        }
    }
    #[test]
    fn d2_partial_publication_retains_raw_and_staged_recovery_then_deliberate_reload() {
        for point in [
            SaveFailurePoint::PublishBackup,
            SaveFailurePoint::PublishPrimary,
        ] {
            for effect in [
                PublicationEffect::DestinationUnchanged,
                PublicationEffect::DestinationAbsent,
                PublicationEffect::OriginalMovedToKnownRecovery,
            ] {
                let root = test_root("d2-partial");
                fs::create_dir_all(&root).unwrap();
                let path = root.join("state.toml");
                let bak = path.with_extension("toml.bak");
                let p = b"# old primary\nversion=1\nfavorites=['primary']";
                let b = b"# old backup\nversion=1\nfavorites=['backup']";
                fs::write(&path, p).unwrap();
                fs::write(&bak, b).unwrap();
                let mut loaded = CandidateState::load(&path);
                loaded.favorites.insert("new".into());
                let error = loaded
                    .save_with_publication_effect(&path, point, effect)
                    .unwrap_err();
                d2_assert_snapshots(&error.migration, Some(p), Some(b));
                assert!(loaded.pending_migration.is_some());
                assert!(!error.recovery_paths().is_empty());
                for recovery in error.recovery_paths() {
                    assert!(load_state_file(recovery).is_ok());
                }
                assert!(error.to_string().contains("ReplaceFileW returned"));
                let mut reloaded = CandidateState::load(&path);
                let out = reloaded.save(&path).unwrap();
                reloaded.acknowledge_save(&out);
                assert!(reloaded.pending_migration.is_none());
                for record in &error.migration {
                    assert!(record.snapshot.exists());
                }
                fs::remove_dir_all(root).unwrap();
            }
        }
    }
    #[cfg(windows)]
    #[test]
    fn d2_long_local_snapshot_and_real_replace_publication() {
        use std::os::windows::ffi::OsStrExt;
        let root = test_root("d2-long");
        let dir = root
            .join("valid depth migration directory 日本語".repeat(3))
            .join("deep-valid-".repeat(8));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("state.toml");
        assert!(path.as_os_str().encode_wide().count() > 260);
        let p = b"# long original\nversion=1\nfavorites=['long']";
        fs::write(&path, p).unwrap();
        let mut state = CandidateState::load(&path);
        let out = state.save(&path).unwrap();
        d2_assert_snapshots(&out.migration, Some(p), None);
        state.acknowledge_save(&out);
        state.save(&path).unwrap();
        state.save(&path).unwrap();
        assert_eq!(fs::read(&out.migration[0].snapshot).unwrap(), p);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn d2_frozen_v1_reader_refuses_v2_isolated_raw_fixture() {
        for future_backup in [false, true] {
            let root = test_root("d2-frozen-v1");
            fs::create_dir_all(&root).unwrap();
            let path = root.join("state.toml");
            let bak = path.with_extension("toml.bak");
            let old = b"# v1 original\nversion=1\nfavorites=['older']";
            let current=b"# v2 raw refuses before incompatible typed decode\nversion=2\ngallery='newer semantics'\n[unknown]\nx=8";
            fs::write(
                &path,
                if future_backup {
                    old.as_slice()
                } else {
                    current.as_slice()
                },
            )
            .unwrap();
            fs::write(
                &bak,
                if future_backup {
                    current.as_slice()
                } else {
                    old.as_slice()
                },
            )
            .unwrap();
            let before = (fs::read(&path).unwrap(), fs::read(&bak).unwrap());
            assert!(matches!(
                frozen_v1_reader::parse_state_contents(std::str::from_utf8(current).unwrap()),
                Err(StateFileError::Unsupported(2))
            ));
            let refusal = frozen_v1_reader::preflight(&path).unwrap_err();
            assert_eq!(refusal.stage, SaveStage::CheckCompatibility);
            assert_eq!(refusal.write_restriction.as_ref().unwrap().version, 2);
            assert_eq!(
                refusal.write_restriction.as_ref().unwrap().path,
                if future_backup {
                    bak.clone()
                } else {
                    path.clone()
                }
            );
            assert_eq!((fs::read(&path).unwrap(), fs::read(&bak).unwrap()), before);
            assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
            fs::remove_dir_all(root).unwrap();
        }
        let current = toml::to_string_pretty(&CandidateState::default()).unwrap();
        assert!(parse_state_contents(&current).is_ok());
        assert!(matches!(
            frozen_v1_reader::parse_state_contents(&current),
            Err(StateFileError::Unsupported(version)) if version == i64::from(CURRENT_STATE_VERSION)
        ));
    }

    #[test]
    fn d2_legacy_late_future_refusal_keeps_archives_and_observed_pair() {
        for stage in [SaveStage::PublishBackup, SaveStage::PublishPrimary] {
            for to_backup in [false, true] {
                let root = test_root("d2-legacy-late-future");
                fs::create_dir_all(&root).unwrap();
                let path = root.join("state.toml");
                let bak = path.with_extension("toml.bak");
                let p = b"version=1\nfavorites=['primary']";
                let b = b"version=1\nfavorites=['backup']";
                fs::write(&path, p).unwrap();
                fs::write(&bak, b).unwrap();
                let loaded = CandidateState::load(&path);
                let mut observed = None;
                let error = loaded
                    .save_impl(&path, None, None, |checkpoint| {
                        if checkpoint == stage {
                            fs::write(
                                if to_backup { &bak } else { &path },
                                r5_future_bytes(i64::MAX),
                            )
                            .unwrap();
                            observed = Some((fs::read(&path).unwrap(), fs::read(&bak).unwrap()));
                        }
                    })
                    .unwrap_err();
                assert_eq!(error.stage, stage);
                assert_eq!(error.write_restriction.as_ref().unwrap().version, i64::MAX);
                assert_eq!(
                    (fs::read(&path).unwrap(), fs::read(&bak).unwrap()),
                    observed.unwrap()
                );
                d2_assert_snapshots(&error.migration, Some(p), Some(b));
                assert!(loaded.pending_migration.is_some());
                if stage == SaveStage::PublishPrimary && !to_backup {
                    assert_eq!(
                        fs::read(&bak).unwrap(),
                        p,
                        "our earlier backup publication remains observable"
                    );
                }
                fs::remove_dir_all(root).unwrap();
            }
        }
    }
    #[test]
    fn d2_unreadable_original_refuses_without_snapshot_or_publication() {
        for block_backup in [false, true] {
            let root = test_root("d2-unreadable");
            fs::create_dir_all(&root).unwrap();
            let path = root.join("state.toml");
            let bak = path.with_extension("toml.bak");
            let raw = b"version=1\nfavorites=['readable']";
            let (blocked, readable) = if block_backup {
                (&bak, &path)
            } else {
                (&path, &bak)
            };
            fs::create_dir(blocked).unwrap();
            fs::write(readable, raw).unwrap();
            let error = CandidateState::default().save(&path).unwrap_err();
            assert_eq!(error.stage, SaveStage::InspectExisting);
            assert!(error.migration.is_empty());
            assert_eq!(fs::read(readable).unwrap(), raw);
            assert!(blocked.is_dir());
            assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
            fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn d2_snapshot_completion_marker_mismatch_is_never_overwritten() {
        let root = test_root("d2-marker");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("state.toml");
        let raw = b"version=1\nfavorites=['original']";
        fs::write(&path, raw).unwrap();
        let loaded = CandidateState::load(&path);
        let error = loaded
            .save_with_fault(
                &path,
                SaveFault {
                    point: SaveFailurePoint::PublishBackup,
                    kind: ErrorKind::PermissionDenied,
                },
            )
            .unwrap_err();
        let snapshot = &error.migration[0].snapshot;
        let marker = snapshot.with_extension("raw.complete");
        fs::write(&marker, b"incomplete-or-wrong").unwrap();
        let refused = loaded.save(&path).unwrap_err();
        assert_eq!(refused.stage, SaveStage::PrepareMigration);
        assert_eq!(fs::read(marker).unwrap(), b"incomplete-or-wrong");
        assert_eq!(fs::read(snapshot).unwrap(), raw);
        assert_eq!(fs::read(&path).unwrap(), raw);
        fs::remove_dir_all(root).unwrap();
    }

    /// Frozen raw reader/write preflight from the accepted D1 v1 source.
    /// Original candidate_state.rs SHA-256 is in STARTING-IDENTITY and originals.
    /// Only raw refusal semantics are exercised, never an older application.
    mod frozen_v1_reader {
        use super::*;
        const CURRENT_STATE_VERSION: u32 = 1;
        fn load_state_file(path: &Path) -> Result<CandidateState, StateFileError> {
            let contents = match fs::read_to_string(path) {
                Ok(contents) => contents,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    return Err(StateFileError::Missing)
                }
                Err(error) => return Err(StateFileError::Read(error.to_string())),
            };
            parse_state_contents(&contents)
        }
        pub(super) fn parse_state_contents(
            contents: &str,
        ) -> Result<CandidateState, StateFileError> {
            if contents.trim().is_empty() {
                return Err(StateFileError::Malformed("the file is empty".to_string()));
            }
            let value = toml::from_str::<toml::Value>(contents)
                .map_err(|error| StateFileError::Malformed(error.to_string()))?;
            let Some(table) = value.as_table() else {
                return Err(StateFileError::Malformed(
                    "the TOML document has no state table".to_string(),
                ));
            };
            // Classify the raw signed TOML integer without narrowing to the model's u32.
            // Missing version and version 0 retain the established partial legacy format.
            if let Some(value) = table.get("version") {
                match value.as_integer() {
                    Some(version) if version > i64::from(CURRENT_STATE_VERSION) => {
                        return Err(StateFileError::Unsupported(version));
                    }
                    Some(version) if version >= 0 => {}
                    _ => {
                        return Err(StateFileError::Malformed(
                            "version must be a nonnegative TOML integer".to_string(),
                        ))
                    }
                }
            }
            const RECOGNIZED_FIELDS: &[&str] = &[
                "version",
                "gallery",
                "gallery_motion_speed",
                "appearance",
                "favorites",
                "playlists",
                "active_playlist_id",
                "schedules",
                "locked_monitors",
                "manual_holdoff_until_unix",
                "slideshow_enabled",
                "slideshow_interval_seconds",
                "global_paused",
                "global_muted",
                "global_volume",
            ];
            if !table
                .keys()
                .any(|key| RECOGNIZED_FIELDS.iter().any(|field| key == *field))
            {
                return Err(StateFileError::Malformed(
                    "the TOML document has no recognized state fields".to_string(),
                ));
            }
            let mut state = toml::from_str::<CandidateState>(contents)
                .map_err(|error| StateFileError::Malformed(error.to_string()))?;
            state.sanitize();
            Ok(state)
        }
        fn inspect_state_file(path: &Path) -> StateFileInspection {
            let bytes = match fs::read(path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == ErrorKind::NotFound => {
                    return StateFileInspection::Missing
                }
                Err(error) => return StateFileInspection::Unreadable(error.to_string()),
            };
            match String::from_utf8(bytes.clone()) {
                Ok(contents) => match parse_state_contents(&contents) {
                    Ok(_) => StateFileInspection::Valid(bytes),
                    Err(StateFileError::Unsupported(version)) => {
                        StateFileInspection::Unsupported(version)
                    }
                    Err(_) => StateFileInspection::Invalid,
                },
                Err(_) => StateFileInspection::Invalid,
            }
        }
        fn ensure_compatible_files(path: &Path, stage: SaveStage) -> Result<(), SaveError> {
            for file in [path.to_path_buf(), path.with_extension("toml.bak")] {
                if let StateFileInspection::Unsupported(version) = inspect_state_file(&file) {
                    return Err(version_refusal(
                        path,
                        stage,
                        StateWriteRestriction {
                            path: file,
                            version,
                        },
                    ));
                }
            }
            Ok(())
        }
        pub(super) fn preflight(path: &Path) -> Result<(), SaveError> {
            ensure_compatible_files(path, SaveStage::CheckCompatibility)
        }
    }

    fn state_with_favorite(id: &str) -> CandidateState {
        let mut state = CandidateState::default();
        state.favorites.insert(id.to_string());
        state
    }

    #[test]
    fn r5_future_primary_default_save_preserves_exact_pair() {
        let root = test_root("r5-future-primary");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        let primary_bytes = b"# newer profile; retain comments\nversion = 5\nfavorites = ['future']\n[new_feature.nested]\nunknown = 'preserve exactly'\n";
        let backup_bytes = b"# distinct older backup\nversion = 1\nfavorites = ['backup']\nslideshow_enabled = true\n";
        fs::write(&path, primary_bytes).unwrap();
        fs::write(&backup, backup_bytes).unwrap();
        let result = CandidateState::default().save(&path);
        assert!(
            result.is_err(),
            "future profile write must be refused: {result:?}"
        );
        assert_eq!(fs::read(&path).unwrap(), primary_bytes);
        assert_eq!(fs::read(&backup).unwrap(), backup_bytes);
        assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
        let loaded = CandidateState::load_with_diagnostics(&path);
        assert_eq!(loaded.source, StateLoadSource::UnsupportedDefaults);
        assert!(loaded
            .diagnostic
            .as_deref()
            .unwrap()
            .contains("Unsupported candidate-state version 5"));
        assert!(loaded.state.favorites.is_empty());
        assert!(loaded.state.schedules.is_empty());
        assert!(!loaded.state.slideshow_enabled);
        for mut state in [
            loaded.state.clone(),
            CandidateState::load(&path),
            CandidateState::default(),
        ] {
            state.favorites.insert("edited-or-imported".into());
            for _ in 0..3 {
                let error = state.save(&path).unwrap_err();
                assert_eq!(error.stage, SaveStage::CheckCompatibility);
                assert_eq!(error.primary, Some(SaveFileStatus::UnsupportedVersion));
                assert!(error.write_restriction.is_some());
                assert!(error.recovery_paths().is_empty());
                assert_eq!(fs::read(&path).unwrap(), primary_bytes);
                assert_eq!(fs::read(&backup).unwrap(), backup_bytes);
                assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
            }
        }
        fs::remove_dir_all(root).unwrap();
        // Session protection must survive even deliberate external file removal.
        loaded.state.save(&path).unwrap_err();
        assert!(!path.parent().unwrap().exists());
    }

    fn r5_future_bytes(version: i64) -> Vec<u8> {
        format!("# preserve newer comments/format\nversion = {version}\ngallery = 'newer semantics'\nslideshow_enabled = true\n[new_feature.nested]\nvalue = ['unknown', 'retain']\n").into_bytes()
    }

    #[test]
    fn r5_future_backup_protects_current_missing_and_corrupt_primary() {
        for primary_bytes in [Some(b"# supported primary\nversion = 1\nfavorites = ['current']\nslideshow_enabled = true\n".as_slice()), None, Some(b"broken = [".as_slice())] {
            let root = test_root("r5-future-backup");
            fs::create_dir_all(&root).unwrap();
            let path = root.join("candidate-state.toml");
            let backup = path.with_extension("toml.bak");
            if let Some(bytes) = primary_bytes { fs::write(&path, bytes).unwrap(); }
            let newer = r5_future_bytes(i64::from(CURRENT_STATE_VERSION) + 1);
            fs::write(&backup, &newer).unwrap();
            let loaded = CandidateState::load_with_diagnostics(&path);
            let current_primary = primary_bytes.is_some_and(|bytes| bytes.starts_with(b"# supported"));
            assert_eq!(loaded.source, if current_primary { StateLoadSource::Primary } else { StateLoadSource::UnsupportedDefaults });
            assert_eq!(loaded.state.favorites.contains("current"), current_primary);
            assert_eq!(loaded.state.slideshow_enabled, current_primary);
            assert!(loaded.diagnostic.as_deref().unwrap().contains("Unsupported candidate-state version 5"));
            assert_eq!(loaded.state.write_restriction.as_ref().unwrap().path, backup);
            for state in [loaded.state, CandidateState::default()] {
                let error = state.save(&path).unwrap_err();
                assert_eq!(error.backup, BackupStatus::ProtectedUnsupported);
                assert!(error.backup_description(&backup).contains("unsupported newer backup is preserved"));
                assert_eq!(fs::read(&path).ok().as_deref(), primary_bytes);
                assert_eq!(fs::read(&backup).unwrap(), newer);
                assert_eq!(fs::read_dir(&root).unwrap().count(), if primary_bytes.is_some() { 2 } else { 1 });
            }
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn r5_raw_version_classification_precedes_typed_decode_and_sanitize() {
        for version in [
            i64::from(CURRENT_STATE_VERSION) + 1,
            i64::from(u32::MAX) + 1,
            i64::MAX,
        ] {
            let bytes = r5_future_bytes(version);
            assert_eq!(
                parse_state_contents(std::str::from_utf8(&bytes).unwrap()),
                Err(StateFileError::Unsupported(version))
            );
        }
        for contents in [
            "version = -1",
            "version = 1.0",
            "version = true",
            "version = '2'",
            "version = [2]",
            "version = {value = 2}",
            "version = 3\nversion = 3",
            "version = 9223372036854775808",
            "version = 3\nbroken = [",
            "[",
        ] {
            assert!(
                matches!(
                    parse_state_contents(contents),
                    Err(StateFileError::Malformed(_))
                ),
                "{contents}"
            );
        }
        assert!(matches!(
            load_state_file(&test_root("r5-missing").join("absent.toml")),
            Err(StateFileError::Missing)
        ));
    }

    #[test]
    fn r5_oversized_future_file_and_in_memory_version_cannot_be_saved() {
        let root = test_root("r5-oversized");
        fs::create_dir_all(&root).unwrap();
        let path = root.join("candidate-state.toml");
        for version in [i64::from(u32::MAX) + 1, i64::MAX] {
            let bytes = r5_future_bytes(version);
            fs::write(&path, &bytes).unwrap();
            let loaded = CandidateState::load_with_diagnostics(&path);
            assert_eq!(loaded.source, StateLoadSource::UnsupportedDefaults);
            assert_eq!(loaded.state.write_restriction.unwrap().version, version);
            let error = CandidateState::default().save(&path).unwrap_err();
            assert_eq!(error.write_restriction.unwrap().version, version);
            assert_eq!(fs::read(&path).unwrap(), bytes);
            assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        }
        fs::remove_dir_all(&root).unwrap();
        let mut state = CandidateState::default();
        state.version = u32::MAX;
        let error = state.save(&path).unwrap_err();
        assert_eq!(error.stage, SaveStage::CheckCompatibility);
        assert_eq!(state.version, u32::MAX);
        assert!(!root.exists());
    }

    #[test]
    fn r5_future_files_appearing_after_load_are_refused() {
        for replace_backup in [false, true] {
            let root = test_root("r5-after-load");
            let path = root.join("candidate-state.toml");
            state_with_favorite("first").save(&path).unwrap();
            let backup = path.with_extension("toml.bak");
            fs::write(&backup, b"# older recovery\nfavorites = ['backup']\n").unwrap();
            let loaded = CandidateState::load(&path);
            let target = if replace_backup { &backup } else { &path };
            fs::write(
                target,
                r5_future_bytes(i64::from(CURRENT_STATE_VERSION) + 1),
            )
            .unwrap();
            let before = (fs::read(&path).unwrap(), fs::read(&backup).unwrap());
            assert!(loaded.write_restriction.is_none());
            loaded.save(&path).unwrap_err();
            assert_eq!(
                (fs::read(&path).unwrap(), fs::read(&backup).unwrap()),
                before
            );
            assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn r5_future_arrival_at_prepublication_checkpoints_preserves_observed_pair() {
        for stage in [
            SaveStage::InspectExisting,
            SaveStage::PublishBackup,
            SaveStage::PublishPrimary,
        ] {
            for replace_backup in [false, true] {
                let root = test_root("r5-prepublication");
                let path = root.join("candidate-state.toml");
                state_with_favorite("first").save(&path).unwrap();
                let backup = path.with_extension("toml.bak");
                state_with_favorite("second").save(&path).unwrap();
                let mut before = None;
                let error = state_with_favorite("third")
                    .save_impl(&path, None, None, |checkpoint| {
                        if checkpoint == stage {
                            let target = if replace_backup { &backup } else { &path };
                            fs::write(
                                target,
                                r5_future_bytes(i64::from(CURRENT_STATE_VERSION) + 1),
                            )
                            .unwrap();
                            before = Some((fs::read(&path).unwrap(), fs::read(&backup).unwrap()));
                        }
                    })
                    .unwrap_err();
                assert_eq!(error.stage, stage);
                assert!(error.write_restriction.is_some());
                assert_eq!(
                    (fs::read(&path).unwrap(), fs::read(&backup).unwrap()),
                    before.unwrap()
                );
                assert!(error.recovery_paths().is_empty());
                assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
                fs::remove_dir_all(root).unwrap();
            }
        }
    }

    #[test]
    fn r5_current_and_established_legacy_keep_ids_defaults_and_validation() {
        for header in ["", "version = 0\n", "version = 1\n"] {
            let root = test_root("r5-compatible");
            fs::create_dir_all(&root).unwrap();
            let path = root.join("candidate-state.toml");
            fs::write(&path, format!("{header}favorites = ['stable:favorite']\nglobal_volume = 150\nslideshow_interval_seconds = 1\nactive_playlist_id = 'stable:playlist'\n[[playlists]]\nid = 'stable:playlist'\nname = 'Legacy'\norder = 'sequential'\nmembership = {{ membership = 'explicit', wallpaper_ids = ['stable:favorite'] }}\n")).unwrap();
            let loaded = CandidateState::load_with_diagnostics(&path);
            assert_eq!(loaded.source, StateLoadSource::Primary);
            assert_eq!(loaded.state.version, CURRENT_STATE_VERSION);
            assert!(loaded.state.write_restriction.is_none());
            assert_eq!(loaded.state.global_volume, 100);
            assert_eq!(loaded.state.slideshow_interval_seconds, 10);
            assert_eq!(
                loaded.state.active_playlist_id.as_deref(),
                Some("stable:playlist")
            );
            assert_eq!(loaded.state.playlists[0].id, "stable:playlist");
            assert!(loaded.state.favorites.contains("stable:favorite"));
            assert!(!loaded.state.slideshow_enabled);
            let saved = loaded.state.save(&path).unwrap();
            assert_eq!(saved.migration.len(), 2);
            let mut expected = loaded.state;
            expected.acknowledge_save(&saved);
            let mut reloaded = CandidateState::load(&path);
            assert!(
                reloaded.pending_migration.is_some(),
                "rolling legacy backup still requires inspection on next save"
            );
            reloaded.pending_migration = None;
            assert_eq!(reloaded, expected);
            fs::remove_dir_all(root).unwrap();
        }
    }

    #[test]
    fn gallery_presets_round_trip_with_pixel_distance_units() {
        let mut state = CandidateState::default();
        state.gallery.slices.skew_distance = 73.5;
        state.gallery.presets.push(GalleryPreset {
            depth: None,
            name: "My slices".into(),
            mode: GalleryKind::Slices,
            variant: Some(1),
            slices: state.gallery.slices.clone(),
            geometric: state.gallery.geometric.clone(),
            wall: state.gallery.wall.clone(),
        });
        let encoded = toml::to_string(&state).unwrap();
        let decoded: CandidateState = toml::from_str(&encoded).unwrap();

        assert_eq!(decoded.gallery.presets[0].slices.skew_distance, 73.5);
        assert_eq!(decoded.gallery.presets[0].variant, Some(1));
    }

    #[test]
    fn slice_defaults_match_the_reference_pixel_geometry() {
        let settings = SliceSettings::default();

        assert_eq!(settings.width, 135.0);
        assert_eq!(settings.expanded_width, 924.0);
        assert_eq!(settings.height, 520.0);
        assert_eq!(settings.spacing, -30.0);
        assert_eq!(settings.skew_distance, 35.0);
        assert_eq!(settings.visible_count, 12);
        assert_eq!(settings.corner_radius, 16.0);
        assert!(!settings.wobble);
        assert_eq!(settings.wobble_strength, 1.0);
    }

    #[test]
    fn wall_defaults_match_the_observed_uniform_grid() {
        let settings = WallSettings::default();

        assert_eq!(settings.layout, WallLayout::Uniform);
        assert_eq!((settings.columns, settings.rows), (6, 3));
        assert_eq!((settings.card_width, settings.card_height), (300.0, 169.0));
        assert_eq!((settings.gap_x, settings.gap_y), (8.0, 8.0));
    }

    #[test]
    fn geometric_defaults_match_the_observed_bow_hexagon_grid() {
        let settings = GeometricSettings::default();

        assert_eq!(settings.shape, GeometricShape::Hexagon);
        assert_eq!(settings.curve, GeometricCurve::Arc);
        assert_eq!((settings.rows, settings.columns), (3, 7));
        assert_eq!(settings.radius, 140.0);
        assert_eq!((settings.gap_x, settings.gap_y), (6.0, 6.0));
    }

    #[test]
    fn automatic_rotation_defaults_to_the_observed_disabled_five_minute_interval() {
        let state = CandidateState::default();

        assert!(!state.slideshow_enabled);
        assert_eq!(state.slideshow_interval_seconds, 300);
    }

    #[test]
    fn manual_appearance_round_trips_with_the_state_version() {
        let mut state = CandidateState::default();
        state.appearance.preset = "custom".into();
        state.appearance.background = " 102030 ".into();
        state.appearance.surface = "#203040".into();
        state.appearance.text = "f0F1f2".into();
        state.appearance.muted = "#778899".into();
        state.appearance.accent = "#ff9900".into();
        state.appearance.overlay_opacity = 0.7;

        let expected_appearance = state.appearance.clone();
        state.sanitize();
        assert_eq!(state.appearance, expected_appearance);

        let encoded = toml::to_string(&state).unwrap();
        let decoded: CandidateState = toml::from_str(&encoded).unwrap();

        assert_eq!(decoded.version, CURRENT_STATE_VERSION);
        assert_eq!(decoded.appearance, state.appearance);
    }

    #[test]
    fn live_overlay_presentation_defaults_validates_and_round_trips_v2() {
        let legacy: CandidateState = toml::from_str(
            "version = 2\n[appearance]\nbackground = '#123456'\noverlay_opacity = 0.7",
        )
        .unwrap();
        assert_eq!(
            legacy.appearance.presentation,
            BackgroundPresentation::WallpaperBackdrop
        );
        assert_eq!(legacy.appearance.background, "#123456");
        assert_eq!(legacy.appearance.overlay_opacity, 0.7);
        let unknown: CandidateState =
            toml::from_str("version = 2\n[appearance]\npresentation = 'unknown-choice'").unwrap();
        assert_eq!(
            unknown.appearance.presentation,
            BackgroundPresentation::WallpaperBackdrop
        );
        assert!(
            toml::from_str::<CandidateState>("version = 2\n[appearance]\npresentation = 42")
                .is_err()
        );
        let mut live = legacy;
        live.appearance.presentation = BackgroundPresentation::LiveTransparentOverlay;
        live.sanitize();
        let encoded = toml::to_string(&live).unwrap();
        assert!(encoded.contains("live-transparent-overlay"));
        let decoded: CandidateState = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded.version, CURRENT_STATE_VERSION);
        assert_eq!(decoded.appearance, live.appearance);
    }

    #[test]
    fn fresh_candidate_uses_the_fixed_nord_palette_and_slow_motion_default() {
        let state = CandidateState::default();

        assert_eq!(state.appearance.preset, "nord");
        assert_eq!(state.appearance.background, "#2e3440");
        assert_eq!(state.appearance.surface, "#3b4252");
        assert_eq!(state.appearance.text, "#eceff4");
        assert_eq!(state.appearance.muted, "#4c566a");
        assert_eq!(state.appearance.accent, "#88c0d0");
        assert_eq!(state.gallery_motion_speed, GalleryMotionSpeed::Slow);
        assert_eq!(state.gallery_motion_speed.duration_ms(), 450);
    }

    #[test]
    fn motion_speed_round_trips_and_older_state_uses_slow_default() {
        let mut state = CandidateState::default();
        state.gallery_motion_speed = GalleryMotionSpeed::Fast;
        let encoded = toml::to_string(&state).unwrap();
        let decoded: CandidateState = toml::from_str(&encoded).unwrap();
        assert_eq!(decoded.gallery_motion_speed, GalleryMotionSpeed::Fast);

        let older_state = encoded
            .lines()
            .filter(|line| !line.starts_with("gallery_motion_speed ="))
            .collect::<Vec<_>>()
            .join("\n");
        let decoded_old: CandidateState = toml::from_str(&older_state).unwrap();
        assert_eq!(decoded_old.gallery_motion_speed, GalleryMotionSpeed::Slow);
    }

    #[test]
    fn manual_apply_blocks_automation_for_exactly_ten_minutes() {
        let mut state = CandidateState::default();
        state.begin_manual_holdoff(1_000);

        assert!(!state.automation_allowed(1_599));
        assert!(state.automation_allowed(1_600));
    }

    #[test]
    fn favorite_toggle_uses_stable_identity() {
        let mut state = CandidateState::default();

        assert!(state.toggle_favorite("workshop:123"));
        assert!(state.favorites.contains("workshop:123"));
        assert!(!state.toggle_favorite("workshop:123"));
        assert!(!state.favorites.contains("workshop:123"));
    }

    #[test]
    fn persisted_gallery_values_are_sanitized_before_rendering() {
        let mut state = CandidateState::default();
        state.gallery.slices.width = f32::NAN;
        state.gallery.slices.visible_count = 0;
        state.gallery.geometric.rows = 0;
        state.gallery.geometric.aspect = f32::INFINITY;
        state.gallery.wall.columns = 0;
        state.gallery.wall.cylinder_radius = -4.0;
        state.appearance.overlay_opacity = 7.0;

        state.sanitize();

        assert_eq!(state.gallery.slices.width, SliceSettings::default().width);
        assert_eq!(state.gallery.slices.visible_count, 1);
        assert_eq!(state.gallery.geometric.rows, 1);
        assert_eq!(
            state.gallery.geometric.aspect,
            GeometricSettings::default().aspect
        );
        assert_eq!(state.gallery.wall.columns, 1);
        assert_eq!(state.gallery.wall.cylinder_radius, 1.0);
        assert_eq!(state.appearance.overlay_opacity, 1.0);
    }

    #[test]
    fn malformed_persisted_appearance_is_repaired_without_losing_other_state() {
        let unique = format!(
            "wsl-candidate-appearance-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let directory = std::env::temp_dir().join(unique);
        let path = directory.join("candidate-state.toml");
        fs::create_dir_all(&directory).unwrap();

        let mut malformed = CandidateState::default();
        malformed.appearance.preset = "custom".into();
        malformed.appearance.background = "あabc".into();
        malformed.appearance.surface = "#12".into();
        malformed.appearance.text = "zzzzzz".into();
        malformed.appearance.muted = "#1234567".into();
        malformed.appearance.accent = "".into();
        malformed.gallery.mode = GalleryKind::Wall;
        malformed.gallery.wall.columns = 11;
        malformed.gallery.wall.rows = 4;
        malformed.favorites.insert("wallpaper:keep".into());
        malformed.playlists.push(Playlist {
            id: "playlist:keep".into(),
            name: "Keep this playlist".into(),
            order: PlaybackOrder::Shuffle,
            membership: PlaylistMembership::Explicit {
                wallpaper_ids: vec!["wallpaper:keep".into()],
            },
        });
        malformed.active_playlist_id = Some("playlist:keep".into());
        malformed.schedules.push(Schedule {
            id: "schedule:keep".into(),
            name: "Keep this schedule".into(),
            enabled: true,
            days: 0b0000010,
            minute_of_day: 615,
            priority: 7,
            creation_order: 3,
            playlist_id: "playlist:keep".into(),
            monitor_ids: vec!["DISPLAY1".into()],
        });

        let expected_gallery = malformed.gallery.clone();
        let expected_favorites = malformed.favorites.clone();
        let expected_playlists = malformed.playlists.clone();
        let expected_schedules = malformed.schedules.clone();
        fs::write(&path, toml::to_string_pretty(&malformed).unwrap()).unwrap();

        let loaded = CandidateState::load(&path);
        let defaults = AppearanceSettings::default();
        assert_eq!(loaded.appearance.preset, "custom");
        assert_eq!(loaded.appearance.background, defaults.background);
        assert_eq!(loaded.appearance.surface, defaults.surface);
        assert_eq!(loaded.appearance.text, defaults.text);
        assert_eq!(loaded.appearance.muted, defaults.muted);
        assert_eq!(loaded.appearance.accent, defaults.accent);
        assert_eq!(loaded.gallery, expected_gallery);
        assert_eq!(loaded.favorites, expected_favorites);
        assert_eq!(loaded.playlists, expected_playlists);
        assert_eq!(loaded.active_playlist_id.as_deref(), Some("playlist:keep"));
        assert_eq!(loaded.schedules, expected_schedules);

        loaded.save(&path).unwrap();
        let reloaded = CandidateState::load(&path);
        assert_eq!(reloaded.appearance, loaded.appearance);
        assert_eq!(reloaded.gallery, expected_gallery);
        assert_eq!(reloaded.favorites, expected_favorites);
        assert_eq!(reloaded.playlists, expected_playlists);
        assert_eq!(reloaded.schedules, expected_schedules);

        fs::remove_file(&path).unwrap();
        fs::remove_file(path.with_extension("toml.bak")).unwrap();
        fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn saving_partial_color_keeps_the_editable_draft_in_memory() {
        let unique = format!(
            "wsl-candidate-appearance-draft-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let directory = std::env::temp_dir().join(unique);
        let path = directory.join("candidate-state.toml");
        fs::create_dir_all(&directory).unwrap();

        let mut draft = CandidateState::default();
        draft.appearance.accent = "#1".into();
        draft.save(&path).unwrap();

        assert_eq!(draft.appearance.accent, "#1");
        let persisted = CandidateState::load(&path);
        assert_eq!(
            persisted.appearance.accent,
            AppearanceSettings::default().accent
        );

        fs::remove_file(&path).unwrap();
        fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn load_recovers_from_the_backup_when_primary_state_is_corrupt() {
        let unique = format!(
            "wsl-candidate-state-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let directory = std::env::temp_dir().join(unique);
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();
        fs::write(&path, "this is not valid toml = [").unwrap();

        let mut expected = CandidateState::default();
        expected.favorites.insert("workshop:backup".into());
        fs::write(&backup, toml::to_string_pretty(&expected).unwrap()).unwrap();

        let recovered = CandidateState::load(&path);

        assert!(recovered.favorites.contains("workshop:backup"));
        fs::remove_file(&path).unwrap();
        fs::remove_file(&backup).unwrap();
        fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn saving_after_corrupt_primary_recovery_preserves_the_valid_backup() {
        let unique = format!(
            "wsl-candidate-state-save-after-recovery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let directory = std::env::temp_dir().join(unique);
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();
        fs::write(&path, "this is not valid toml = [").unwrap();

        let mut expected = CandidateState::default();
        expected.favorites.insert("workshop:backup".into());
        fs::write(&backup, toml::to_string_pretty(&expected).unwrap()).unwrap();

        let mut recovered = CandidateState::load(&path);
        recovered.favorites.insert("workshop:new".into());
        recovered.save(&path).unwrap();

        let preserved = CandidateState::load(&backup);
        assert!(preserved.favorites.contains("workshop:backup"));
        assert!(!preserved.favorites.contains("workshop:new"));

        fs::remove_file(&path).unwrap();
        fs::remove_file(&backup).unwrap();
        fs::remove_dir(&directory).unwrap();
    }

    #[test]
    fn load_reports_primary_backup_first_use_and_failure_defaults() {
        let directory = test_root("load-sources");
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();

        let first_use = CandidateState::load_with_diagnostics(&path);
        assert_eq!(first_use.source, StateLoadSource::FirstUseDefaults);
        assert!(first_use
            .diagnostic
            .as_deref()
            .is_some_and(|message| message.contains("first use")));

        let primary = state_with_favorite("primary");
        primary.save(&path).unwrap();
        let loaded_primary = CandidateState::load_with_diagnostics(&path);
        assert_eq!(loaded_primary.source, StateLoadSource::Primary);
        assert!(loaded_primary.state.favorites.contains("primary"));

        fs::write(&path, "this is not valid toml = [").unwrap();
        let backup_state = state_with_favorite("backup");
        fs::write(&backup, toml::to_string_pretty(&backup_state).unwrap()).unwrap();
        let recovered = CandidateState::load_with_diagnostics(&path);
        assert_eq!(recovered.source, StateLoadSource::BackupRecovery);
        assert!(recovered.state.favorites.contains("backup"));
        assert!(recovered.diagnostic.is_some());

        fs::write(&path, "[unrelated]\nvalue = \"not state\"\n").unwrap();
        let unknown_primary = CandidateState::load_with_diagnostics(&path);
        assert_eq!(unknown_primary.source, StateLoadSource::BackupRecovery);
        assert!(unknown_primary.state.favorites.contains("backup"));

        fs::write(&backup, "[").unwrap();
        let failed = CandidateState::load_with_diagnostics(&path);
        assert_eq!(failed.source, StateLoadSource::FailureDefaults);
        assert!(failed.diagnostic.is_some());
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "[unrelated]\nvalue = \"not state\"\n"
        );
        assert_eq!(fs::read_to_string(&backup).unwrap(), "[");

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn empty_primary_recovers_from_valid_backup_without_writing_either_file() {
        let directory = test_root("empty-primary");
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();
        fs::write(&path, "\n  \t").unwrap();
        let expected = state_with_favorite("backup");
        fs::write(&backup, toml::to_string_pretty(&expected).unwrap()).unwrap();

        let outcome = CandidateState::load_with_diagnostics(&path);

        assert_eq!(outcome.source, StateLoadSource::BackupRecovery);
        assert!(outcome.state.favorites.contains("backup"));
        assert_eq!(fs::read_to_string(&path).unwrap(), "\n  \t");
        assert!(fs::read_to_string(&backup).unwrap().contains("backup"));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn unknown_only_backup_is_not_reported_as_recovered_state() {
        let directory = test_root("unknown-backup");
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();
        fs::write(&backup, "[unrelated]\nvalue = \"not state\"\n").unwrap();

        let outcome = CandidateState::load_with_diagnostics(&path);

        assert_eq!(outcome.source, StateLoadSource::FailureDefaults);
        assert!(outcome
            .diagnostic
            .as_deref()
            .is_some_and(|message| message.contains("backup")));
        assert_eq!(
            fs::read_to_string(&backup).unwrap(),
            "[unrelated]\nvalue = \"not state\"\n"
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn initial_and_replacement_saves_report_real_backup_presence() {
        let directory = test_root("save-outcome");
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();

        let initial = state_with_favorite("initial");
        let first = initial.save(&path).unwrap();
        assert_eq!(first.backup, BackupStatus::Unavailable);
        assert!(!backup.exists());

        let replacement = state_with_favorite("replacement");
        let second = replacement.save(&path).unwrap();
        assert_eq!(second.backup, BackupStatus::Published);
        assert!(second.backup.is_available());
        assert!(CandidateState::load(&path)
            .favorites
            .contains("replacement"));
        assert!(CandidateState::load(&backup).favorites.contains("initial"));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn save_with_missing_primary_preserves_a_valid_backup() {
        let directory = test_root("missing-primary-save");
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();
        let previous = state_with_favorite("previous");
        fs::write(&backup, toml::to_string_pretty(&previous).unwrap()).unwrap();

        let outcome = state_with_favorite("current").save(&path).unwrap();

        assert_eq!(outcome.backup, BackupStatus::Preserved);
        assert!(!CandidateState::load(&path).favorites.contains("previous"));
        assert!(CandidateState::load(&backup).favorites.contains("previous"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn valid_primary_repairs_a_malformed_backup_from_the_previous_primary() {
        let directory = test_root("malformed-backup-repair");
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();
        let previous = state_with_favorite("previous");
        previous.save(&path).unwrap();
        fs::write(&backup, "[unrelated]\nvalue = \"not state\"\n").unwrap();

        let outcome = state_with_favorite("current").save(&path).unwrap();

        assert_eq!(outcome.backup, BackupStatus::Published);
        assert!(CandidateState::load(&backup).favorites.contains("previous"));
        assert!(CandidateState::load(&path).favorites.contains("current"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn deterministic_save_failures_preserve_existing_state_and_report_stage() {
        let cases = [
            (
                SaveFailurePoint::PreparePrimary,
                ErrorKind::PermissionDenied,
                SaveStage::PreparePrimary,
                BackupStatus::Unavailable,
            ),
            (
                SaveFailurePoint::FlushPrimary,
                ErrorKind::StorageFull,
                SaveStage::PreparePrimary,
                BackupStatus::Unavailable,
            ),
            (
                SaveFailurePoint::PublishBackup,
                ErrorKind::PermissionDenied,
                SaveStage::PublishBackup,
                BackupStatus::Unavailable,
            ),
            (
                SaveFailurePoint::PublishPrimary,
                ErrorKind::StorageFull,
                SaveStage::PublishPrimary,
                BackupStatus::Published,
            ),
        ];

        for (index, (point, kind, stage, backup_status)) in cases.into_iter().enumerate() {
            let directory = test_root(&format!("save-failure-{index}"));
            let path = directory.join("candidate-state.toml");
            let backup = path.with_extension("toml.bak");
            fs::create_dir_all(&directory).unwrap();
            let initial = state_with_favorite("initial");
            initial.save(&path).unwrap();

            let error = state_with_favorite("replacement")
                .save_with_fault(&path, SaveFault { point, kind })
                .unwrap_err();

            assert_eq!(error.stage, stage);
            assert_eq!(error.backup, backup_status);
            assert!(CandidateState::load(&path).favorites.contains("initial"));
            if backup_status == BackupStatus::Published {
                assert!(CandidateState::load(&backup).favorites.contains("initial"));
            } else {
                assert!(!backup.exists());
            }
            fs::remove_dir_all(directory).unwrap();
        }
    }

    #[test]
    fn early_save_failures_report_an_existing_valid_backup() {
        let directory = test_root("early-failure-backup");
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();
        fs::write(
            &backup,
            toml::to_string_pretty(&state_with_favorite("backup")).unwrap(),
        )
        .unwrap();

        for point in [
            SaveFailurePoint::PreparePrimary,
            SaveFailurePoint::FlushPrimary,
        ] {
            let error = state_with_favorite("replacement")
                .save_with_fault(
                    &path,
                    SaveFault {
                        point,
                        kind: ErrorKind::StorageFull,
                    },
                )
                .unwrap_err();
            assert_eq!(error.backup, BackupStatus::Preserved);
            assert!(CandidateState::load(&backup).favorites.contains("backup"));
        }

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_save_after_backup_recovery_preserves_corrupt_primary_and_good_backup() {
        let directory = test_root("recovery-failure");
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();
        fs::write(&path, "this is not valid toml = [").unwrap();
        fs::write(
            &backup,
            toml::to_string_pretty(&state_with_favorite("backup")).unwrap(),
        )
        .unwrap();

        let recovered = CandidateState::load(&path);
        let error = recovered
            .save_with_fault(
                &path,
                SaveFault {
                    point: SaveFailurePoint::PublishPrimary,
                    kind: ErrorKind::StorageFull,
                },
            )
            .unwrap_err();

        assert_eq!(error.stage, SaveStage::PublishPrimary);
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "this is not valid toml = ["
        );
        assert!(CandidateState::load(&backup).favorites.contains("backup"));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn failed_save_keeps_partial_appearance_text_in_memory() {
        let directory = test_root("partial-draft-failure");
        let path = directory.join("candidate-state.toml");
        fs::create_dir_all(&directory).unwrap();
        let mut draft = CandidateState::default();
        draft.appearance.accent = "#1".into();

        let error = draft
            .save_with_fault(
                &path,
                SaveFault {
                    point: SaveFailurePoint::FlushPrimary,
                    kind: ErrorKind::StorageFull,
                },
            )
            .unwrap_err();

        assert_eq!(error.stage, SaveStage::PreparePrimary);
        assert_eq!(draft.appearance.accent, "#1");
        assert!(!path.exists());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn partial_primary_failure_with_destination_unchanged_reports_valid_primary_and_staged_recovery(
    ) {
        let directory = test_root("partial-primary-unchanged");
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();
        state_with_favorite("initial").save(&path).unwrap();

        let error = state_with_favorite("replacement")
            .save_with_publication_effect(
                &path,
                SaveFailurePoint::PublishPrimary,
                PublicationEffect::DestinationUnchanged,
            )
            .unwrap_err();

        assert_eq!(error.primary, Some(SaveFileStatus::Valid));
        assert_eq!(error.backup, BackupStatus::Preserved);
        assert!(CandidateState::load(&path).favorites.contains("initial"));
        assert!(CandidateState::load(&backup).favorites.contains("initial"));
        let staged = error
            .recovery_paths()
            .iter()
            .find(|candidate| {
                candidate
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().contains("state-tmp"))
            })
            .expect("validated staged state must remain recoverable");
        assert!(CandidateState::load(staged)
            .favorites
            .contains("replacement"));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn partial_primary_failure_with_destination_absent_preserves_staged_data_and_backup() {
        let directory = test_root("partial-primary-absent");
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();
        state_with_favorite("initial").save(&path).unwrap();

        let error = state_with_favorite("replacement")
            .save_with_publication_effect(
                &path,
                SaveFailurePoint::PublishPrimary,
                PublicationEffect::DestinationAbsent,
            )
            .unwrap_err();

        assert_eq!(error.primary, Some(SaveFileStatus::Missing));
        assert_eq!(error.backup, BackupStatus::Preserved);
        assert!(!path.exists());
        assert!(CandidateState::load(&backup).favorites.contains("initial"));
        assert!(error
            .recovery_paths()
            .iter()
            .any(|candidate| CandidateState::load(candidate)
                .favorites
                .contains("replacement")));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn partial_primary_failure_reports_original_at_the_known_replacefile_backup_path() {
        let directory = test_root("partial-primary-moved");
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();
        state_with_favorite("initial").save(&path).unwrap();

        let error = state_with_favorite("replacement")
            .save_with_publication_effect(
                &path,
                SaveFailurePoint::PublishPrimary,
                PublicationEffect::OriginalMovedToKnownRecovery,
            )
            .unwrap_err();

        assert_eq!(error.primary, Some(SaveFileStatus::Missing));
        assert_eq!(error.backup, BackupStatus::Preserved);
        let known_original = error
            .recovery_paths()
            .iter()
            .find(|candidate| {
                candidate
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().contains("replace-recovery"))
            })
            .expect("ReplaceFileW backup path must be reported");
        assert!(CandidateState::load(known_original)
            .favorites
            .contains("initial"));
        assert!(error
            .to_string()
            .contains("ERROR_UNABLE_TO_MOVE_REPLACEMENT_2"));
        assert!(CandidateState::load(&backup).favorites.contains("initial"));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn existing_valid_candidate_backup_survives_a_partial_backup_publication_failure() {
        let directory = test_root("partial-backup");
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();
        state_with_favorite("initial").save(&path).unwrap();
        fs::write(
            &backup,
            toml::to_string_pretty(&state_with_favorite("older-backup")).unwrap(),
        )
        .unwrap();

        let error = state_with_favorite("replacement")
            .save_with_publication_effect(
                &path,
                SaveFailurePoint::PublishBackup,
                PublicationEffect::DestinationUnchanged,
            )
            .unwrap_err();

        assert_eq!(error.primary, Some(SaveFileStatus::Valid));
        assert_eq!(error.backup, BackupStatus::Preserved);
        assert!(CandidateState::load(&path).favorites.contains("initial"));
        assert!(CandidateState::load(&backup)
            .favorites
            .contains("older-backup"));
        assert!(error
            .recovery_paths()
            .iter()
            .any(|candidate| candidate == &backup));

        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn partial_failure_without_a_prior_valid_state_keeps_the_only_validated_staged_copy() {
        let directory = test_root("partial-only-staged");
        let path = directory.join("candidate-state.toml");
        let backup = path.with_extension("toml.bak");
        fs::create_dir_all(&directory).unwrap();
        fs::write(&path, "not valid candidate state = [").unwrap();

        let error = state_with_favorite("replacement")
            .save_with_publication_effect(
                &path,
                SaveFailurePoint::PublishPrimary,
                PublicationEffect::DestinationAbsent,
            )
            .unwrap_err();

        assert_eq!(error.primary, Some(SaveFileStatus::Missing));
        assert_eq!(error.backup, BackupStatus::Unavailable);
        assert!(!path.exists());
        let staged = error
            .recovery_paths()
            .iter()
            .find(|candidate| {
                candidate
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().contains("state-tmp"))
            })
            .expect("staged state is the only known usable copy");
        assert!(CandidateState::load(staged)
            .favorites
            .contains("replacement"));
        assert!(!backup.exists());

        fs::remove_dir_all(directory).unwrap();
    }
}

#[cfg(test)]
#[path = "candidate_state/v3_tests.rs"]
mod v3_tests;

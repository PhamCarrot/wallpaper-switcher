use crate::file_publication;
use crate::transition::{TransitionConfig, TransitionEffect, TransitionMigration};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ThemeConfig {
    #[serde(default = "default_preset")]
    pub preset: String,
}

fn default_preset() -> String {
    "rose-pine-moon".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleConfig {
    #[serde(default = "default_accent_color")]
    pub accent_color: String,
    #[serde(default = "default_bg_color")]
    pub bg_color: String,
    #[serde(default = "default_surface_color")]
    pub surface_color: String,
    #[serde(default = "default_overlay_color")]
    pub overlay_color: String,
    #[serde(default = "default_muted_color")]
    pub muted_color: String,
    #[serde(default = "default_text_color")]
    pub text_color: String,
    #[serde(default = "default_highlight_color")]
    pub highlight_color: String,

    #[serde(default = "default_card_width")]
    pub card_width: u32,
    #[serde(default = "default_card_height")]
    pub card_height: u32,
    #[serde(default = "default_card_shear")]
    pub card_shear: f32,
    #[serde(default = "default_border_width")]
    pub border_width: f32,
    #[serde(default = "default_dim_unselected")]
    pub dim_unselected: f32,
    #[serde(default = "default_backdrop_opacity")]
    pub backdrop_opacity: f32,
    #[serde(default = "default_border_glow")]
    pub border_glow: bool,
    #[serde(default = "default_glow_radius")]
    pub glow_radius: f32,
    #[serde(default = "default_motion_blur")]
    pub motion_blur: bool,
    #[serde(default = "default_motion_blur_strength")]
    pub motion_blur_strength: f32,
}

fn default_accent_color() -> String {
    "#eb6f92".to_string()
} // Love
fn default_bg_color() -> String {
    "#232136".to_string()
} // Base
fn default_surface_color() -> String {
    "#2a273f".to_string()
} // Surface
fn default_overlay_color() -> String {
    "#393552".to_string()
} // Overlay
fn default_muted_color() -> String {
    "#6e6a86".to_string()
} // Muted
fn default_text_color() -> String {
    "#e0def4".to_string()
} // Text
fn default_highlight_color() -> String {
    "#44415a".to_string()
} // Highlight Med
fn default_card_width() -> u32 {
    300
}
fn default_card_height() -> u32 {
    533
}
fn default_card_shear() -> f32 {
    0.21
}
fn default_border_width() -> f32 {
    2.0
}
fn default_dim_unselected() -> f32 {
    0.40
}
fn default_backdrop_opacity() -> f32 {
    0.72
}
fn default_border_glow() -> bool {
    true
}
fn default_glow_radius() -> f32 {
    8.0
}
fn default_motion_blur() -> bool {
    true
}
fn default_motion_blur_strength() -> f32 {
    1.0
}

impl Default for StyleConfig {
    fn default() -> Self {
        Self {
            accent_color: default_accent_color(),
            bg_color: default_bg_color(),
            surface_color: default_surface_color(),
            overlay_color: default_overlay_color(),
            muted_color: default_muted_color(),
            text_color: default_text_color(),
            highlight_color: default_highlight_color(),
            card_width: default_card_width(),
            card_height: default_card_height(),
            card_shear: default_card_shear(),
            border_width: default_border_width(),
            dim_unselected: default_dim_unselected(),
            backdrop_opacity: default_backdrop_opacity(),
            border_glow: default_border_glow(),
            glow_radius: default_glow_radius(),
            motion_blur: default_motion_blur(),
            motion_blur_strength: default_motion_blur_strength(),
        }
    }
}

fn deserialize_flexible_bool<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: serde::Deserializer<'de>,
{
    struct BoolOrNumVisitor;

    impl<'de> serde::de::Visitor<'de> for BoolOrNumVisitor {
        type Value = bool;

        fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
            formatter.write_str("a boolean or boolean-like value (true/false, 1/0, 'true'/'false')")
        }

        fn visit_bool<E>(self, v: bool) -> Result<bool, E> {
            Ok(v)
        }

        fn visit_i64<E>(self, v: i64) -> Result<bool, E> {
            Ok(v != 0)
        }

        fn visit_u64<E>(self, v: u64) -> Result<bool, E> {
            Ok(v != 0)
        }

        fn visit_f64<E>(self, v: f64) -> Result<bool, E> {
            Ok(v > 0.0)
        }

        fn visit_str<E>(self, v: &str) -> Result<bool, E>
        where
            E: serde::de::Error,
        {
            match v.to_lowercase().trim() {
                "true" | "yes" | "1" | "on" | "enable" | "enabled" => Ok(true),
                "false" | "no" | "0" | "off" | "disable" | "disabled" => Ok(false),
                _ => Err(E::custom(format!("invalid boolean string '{}'", v))),
            }
        }
    }

    deserializer.deserialize_any(BoolOrNumVisitor)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiConfig {
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_flexible_bool",
        alias = "top_bar"
    )]
    pub show_top_bar: bool,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_flexible_bool",
        alias = "monitor_tabs"
    )]
    pub show_monitor_tabs: bool,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_flexible_bool",
        alias = "title"
    )]
    pub show_title: bool,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_flexible_bool",
        alias = "hidden_button"
    )]
    pub show_hidden_button: bool,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_flexible_bool",
        alias = "close_button"
    )]
    pub show_close_button: bool,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_flexible_bool",
        alias = "hint_bar"
    )]
    pub show_hint_bar: bool,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_flexible_bool",
        alias = "hidden_badge"
    )]
    pub show_hidden_badge: bool,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_flexible_bool",
        alias = "tint_excluded_cards"
    )]
    pub tint_excluded: bool,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_flexible_bool",
        alias = "dim_unselected_cards"
    )]
    pub dim_unselected: bool,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_flexible_bool",
        alias = "selection_border",
        alias = "show_border"
    )]
    pub show_selection_border: bool,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_flexible_bool"
    )]
    pub close_on_backdrop_click: bool,
    #[serde(
        default = "default_true",
        deserialize_with = "deserialize_flexible_bool"
    )]
    pub close_on_focus_loss: bool,

    // Fallbacks: allow style & geometry settings under [ui] as well
    #[serde(default)]
    pub backdrop_opacity: Option<f32>,
    #[serde(default)]
    pub card_width: Option<u32>,
    #[serde(default)]
    pub card_height: Option<u32>,
    #[serde(default)]
    pub card_shear: Option<f32>,
    #[serde(default)]
    pub border_width: Option<f32>,
    #[serde(default)]
    pub border_glow: Option<bool>,
    #[serde(default)]
    pub glow_radius: Option<f32>,
    #[serde(default)]
    pub motion_blur: Option<bool>,
    #[serde(default)]
    pub motion_blur_strength: Option<f32>,
    #[serde(default = "default_hint_bar_items")]
    pub hint_bar_items: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_hint_bar_items() -> Vec<String> {
    vec![
        "scroll".to_string(),
        "apply".to_string(),
        "apply_to_all".to_string(),
        "toggle_exclude".to_string(),
        "toggle_hidden".to_string(),
        "switch_monitor".to_string(),
        "reload_config".to_string(),
        "close".to_string(),
    ]
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            show_top_bar: true,
            show_monitor_tabs: true,
            show_title: true,
            show_hidden_button: true,
            show_close_button: true,
            show_hint_bar: true,
            show_hidden_badge: true,
            tint_excluded: true,
            dim_unselected: true,
            show_selection_border: true,
            close_on_backdrop_click: true,
            close_on_focus_loss: true,
            backdrop_opacity: None,
            card_width: None,
            card_height: None,
            card_shear: None,
            border_width: None,
            border_glow: None,
            glow_radius: None,
            motion_blur: None,
            motion_blur_strength: None,
            hint_bar_items: default_hint_bar_items(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeybindsConfig {
    #[serde(default = "default_toggle_gallery")]
    pub toggle_gallery: String,
    #[serde(default = "default_apply_wallpaper")]
    pub apply_wallpaper: String,
    #[serde(default = "default_apply_to_all")]
    pub apply_to_all: String,
    #[serde(default = "default_close")]
    pub close: String,
    #[serde(default = "default_left")]
    pub left: String,
    #[serde(default = "default_right")]
    pub right: String,
    #[serde(default = "default_page_left")]
    pub page_left: String,
    #[serde(default = "default_page_right")]
    pub page_right: String,
    #[serde(default = "default_first")]
    pub first: String,
    #[serde(default = "default_last")]
    pub last: String,
    #[serde(default = "default_switch_monitor")]
    pub switch_monitor: String,
    #[serde(default = "default_toggle_exclude")]
    pub toggle_exclude: String,
    #[serde(default = "default_toggle_hidden")]
    pub toggle_hidden: String,
    #[serde(default = "default_open_in_explorer")]
    pub open_in_explorer: String,
    #[serde(default = "default_reload_config")]
    pub reload_config: String,
}

fn default_toggle_gallery() -> String {
    "alt+w".to_string()
}
fn default_apply_wallpaper() -> String {
    "return".to_string()
}
fn default_apply_to_all() -> String {
    "ctrl+return".to_string()
}
fn default_close() -> String {
    "escape".to_string()
}
fn default_left() -> String {
    "left".to_string()
}
fn default_right() -> String {
    "right".to_string()
}
fn default_page_left() -> String {
    "pageup".to_string()
}
fn default_page_right() -> String {
    "pagedown".to_string()
}
fn default_first() -> String {
    "home".to_string()
}
fn default_last() -> String {
    "end".to_string()
}
fn default_switch_monitor() -> String {
    "tab".to_string()
}
fn default_toggle_exclude() -> String {
    "x".to_string()
}
fn default_toggle_hidden() -> String {
    "h".to_string()
}
fn default_open_in_explorer() -> String {
    "e".to_string()
}
fn default_reload_config() -> String {
    "f5".to_string()
}

impl Default for KeybindsConfig {
    fn default() -> Self {
        Self {
            toggle_gallery: default_toggle_gallery(),
            apply_wallpaper: default_apply_wallpaper(),
            apply_to_all: default_apply_to_all(),
            close: default_close(),
            left: default_left(),
            right: default_right(),
            page_left: default_page_left(),
            page_right: default_page_right(),
            first: default_first(),
            last: default_last(),
            switch_monitor: default_switch_monitor(),
            toggle_exclude: default_toggle_exclude(),
            toggle_hidden: default_toggle_hidden(),
            open_in_explorer: default_open_in_explorer(),
            reload_config: default_reload_config(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BehaviorConfig {
    #[serde(default = "default_true")]
    pub follow_cursor: bool,
    #[serde(default = "default_false")]
    pub show_excluded: bool,
    #[serde(default = "default_true")]
    pub include_default_projects: bool,
}

fn default_false() -> bool {
    false
}

impl Default for BehaviorConfig {
    fn default() -> Self {
        Self {
            follow_cursor: true,
            show_excluded: false,
            include_default_projects: true,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogConfig {
    #[serde(default)]
    pub local_folders: Vec<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlideshowConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_slideshow_interval_minutes")]
    pub interval_minutes: u64,
    #[serde(default = "default_slideshow_target")]
    pub target_monitor: String,
}

fn default_slideshow_interval_minutes() -> u64 {
    30
}

fn default_slideshow_target() -> String {
    "primary".to_string()
}

impl Default for SlideshowConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_minutes: default_slideshow_interval_minutes(),
            target_monitor: default_slideshow_target(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub theme: ThemeConfig,
    #[serde(default)]
    pub style: StyleConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub keybinds: KeybindsConfig,
    #[serde(default)]
    pub behavior: BehaviorConfig,
    #[serde(default)]
    pub catalog: CatalogConfig,
    #[serde(default)]
    pub slideshow: SlideshowConfig,
    /// Overrides the shared on-disk cache cap when greater than zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cache_max_mib: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poster_decode_threads: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_scan_threads: Option<u8>,
    #[serde(default)]
    pub transition: TransitionConfig,
    #[serde(default)]
    pub excluded_wallpapers: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub disabled_wallpapers: HashMap<String, Vec<String>>,
}

impl Default for Config {
    fn default() -> Self {
        let mut excluded = HashMap::new();
        excluded.insert("Monitor0".to_string(), Vec::new());
        excluded.insert("Monitor1".to_string(), Vec::new());

        let mut disabled = HashMap::new();
        disabled.insert("Monitor0".to_string(), Vec::new());
        disabled.insert("Monitor1".to_string(), Vec::new());

        Self {
            theme: ThemeConfig {
                preset: "rose-pine-moon".to_string(),
            },
            style: StyleConfig::default(),
            ui: UiConfig::default(),
            keybinds: KeybindsConfig::default(),
            behavior: BehaviorConfig::default(),
            catalog: CatalogConfig::default(),
            slideshow: SlideshowConfig::default(),
            cache_max_mib: None,
            poster_decode_threads: None,
            catalog_scan_threads: None,
            transition: TransitionConfig::default(),
            excluded_wallpapers: excluded,
            disabled_wallpapers: disabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigLoadSource {
    Primary,
    BackupRecovery,
    FirstUseDefaults,
    FailureDefaults,
}

#[derive(Debug, Clone)]
pub struct ConfigLoadOutcome {
    pub config: Config,
    pub source: ConfigLoadSource,
    pub diagnostic: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigBackupStatus {
    Published,
    Preserved,
    Unavailable,
}

impl ConfigBackupStatus {
    pub const fn is_available(self) -> bool {
        !matches!(self, Self::Unavailable)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigSaveStage {
    CreateDirectory,
    InspectExisting,
    PreparePrimary,
    PrepareBackup,
    PublishBackup,
    PublishPrimary,
}

impl std::fmt::Display for ConfigSaveStage {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::CreateDirectory => "create configuration directory",
            Self::InspectExisting => "inspect existing configuration files",
            Self::PreparePrimary => "prepare primary configuration temporary file",
            Self::PrepareBackup => "prepare configuration backup temporary file",
            Self::PublishBackup => "publish configuration backup",
            Self::PublishPrimary => "publish primary configuration",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFileStatus {
    Valid,
    Missing,
    Invalid,
    Unreadable,
}

impl std::fmt::Display for ConfigFileStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Valid => "valid",
            Self::Missing => "missing",
            Self::Invalid => "invalid",
            Self::Unreadable => "unreadable",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSaveOutcome {
    pub backup: ConfigBackupStatus,
}

impl ConfigSaveOutcome {
    pub fn backup_description(&self, backup_path: &Path) -> String {
        match self.backup {
            ConfigBackupStatus::Published => {
                format!(
                    "a valid configuration backup was published at {}",
                    backup_path.display()
                )
            }
            ConfigBackupStatus::Preserved => format!(
                "the existing valid configuration backup remains at {}",
                backup_path.display()
            ),
            ConfigBackupStatus::Unavailable => {
                "no valid configuration backup is available".to_string()
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigSaveError {
    pub stage: ConfigSaveStage,
    pub primary: ConfigFileStatus,
    pub backup: ConfigBackupStatus,
    recovery_paths: Vec<PathBuf>,
    message: String,
}

impl ConfigSaveError {
    pub(crate) fn new(
        stage: ConfigSaveStage,
        primary: ConfigFileStatus,
        backup: ConfigBackupStatus,
        error: impl ToString,
    ) -> Self {
        Self {
            stage,
            primary,
            backup,
            recovery_paths: Vec::new(),
            message: error.to_string(),
        }
    }

    fn observed(
        stage: ConfigSaveStage,
        primary: ConfigFileStatus,
        backup: ConfigBackupStatus,
        recovery_paths: Vec<PathBuf>,
        message: impl ToString,
    ) -> Self {
        Self {
            stage,
            primary,
            backup,
            recovery_paths,
            message: message.to_string(),
        }
    }

    pub fn backup_description(&self, backup_path: &Path) -> String {
        ConfigSaveOutcome {
            backup: self.backup,
        }
        .backup_description(backup_path)
    }

    pub fn recovery_paths(&self) -> &[PathBuf] {
        &self.recovery_paths
    }
}

impl std::fmt::Display for ConfigSaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "{} failed at {} (primary: {}; backup: {}): {}",
            "Configuration save",
            self.stage,
            self.primary,
            match self.backup {
                ConfigBackupStatus::Published => "published",
                ConfigBackupStatus::Preserved => "preserved",
                ConfigBackupStatus::Unavailable => "unavailable",
            },
            self.message
        )?;
        if !self.recovery_paths.is_empty() {
            write!(
                formatter,
                "; tracked recovery paths: {}",
                self.recovery_paths
                    .iter()
                    .map(|path| path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for ConfigSaveError {}

#[derive(Debug)]
enum ConfigFileInspection {
    Valid(String),
    Missing,
    Invalid(String),
    Unreadable(String),
}

impl ConfigFileInspection {
    fn status(&self) -> ConfigFileStatus {
        match self {
            Self::Valid(_) => ConfigFileStatus::Valid,
            Self::Missing => ConfigFileStatus::Missing,
            Self::Invalid(_) => ConfigFileStatus::Invalid,
            Self::Unreadable(_) => ConfigFileStatus::Unreadable,
        }
    }
}

#[derive(Debug)]
struct ConfigPublicationError {
    error: io::Error,
    replacement: PathBuf,
    destination: PathBuf,
    recovery: Option<PathBuf>,
    replace_file_called: bool,
    publication_succeeded: bool,
}

impl From<file_publication::StagedPublicationError> for ConfigPublicationError {
    fn from(publication: file_publication::StagedPublicationError) -> Self {
        Self {
            error: publication.error,
            replacement: publication.replacement,
            destination: publication.destination,
            recovery: publication.recovery,
            replace_file_called: publication.replace_file_called,
            publication_succeeded: publication.publication_succeeded,
        }
    }
}

pub fn log_debug(msg: &str) {
    log_debug_with_dir(msg, Config::config_dir);
}

#[derive(Debug)]
enum DebugDestination {
    #[cfg(test)]
    Stderr,
    File(PathBuf),
}

// Test builds must select their sink before evaluating any profile resolver.
// The build-wide policy also covers spawned workers and this binary's test children.
fn debug_destination(resolve_dir: impl FnOnce() -> PathBuf) -> DebugDestination {
    #[cfg(test)]
    {
        let _ = resolve_dir;
        DebugDestination::Stderr
    }
    #[cfg(not(test))]
    {
        file_debug_destination(resolve_dir())
    }
}

fn file_debug_destination(dir: PathBuf) -> DebugDestination {
    DebugDestination::File(dir.join("wallpaper-switcher.log"))
}

fn log_debug_with_dir(msg: &str, resolve_dir: impl FnOnce() -> PathBuf) {
    write_debug(debug_destination(resolve_dir), msg);
}

fn write_debug(destination: DebugDestination, msg: &str) {
    match destination {
        #[cfg(test)]
        DebugDestination::Stderr => {
            let _ = write_debug_record(&mut io::stderr().lock(), msg);
        }
        DebugDestination::File(log_file) => {
            if let Some(dir) = log_file.parent() {
                let _ = fs::create_dir_all(dir);
            }
            if let Ok(mut file) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_file)
            {
                let _ = write_debug_record(&mut file, msg);
            }
        }
    }
}

fn write_debug_record(writer: &mut impl Write, msg: &str) -> io::Result<()> {
    writeln!(writer, "[DEBUG] {}", msg)
}

fn configured_runtime_dir(value: Option<std::ffi::OsString>) -> Option<PathBuf> {
    value.filter(|path| !path.is_empty()).map(PathBuf::from)
}

fn apply_transition_section(
    cfg: &mut Config,
    value: &toml::Value,
    report: &mut impl FnMut(String),
) {
    let Some(table) = value.as_table() else {
        report("Invalid [transition] section: expected a table; keeping defaults".to_string());
        return;
    };

    let mut transition = cfg.transition.clone();
    if let Some(effect_value) = table.get("effect") {
        match effect_value.as_str() {
            Some(saved) => match TransitionEffect::parse_saved(saved) {
                Ok(parsed) => {
                    transition.effect = parsed.effect;
                    if parsed.migration == Some(TransitionMigration::RemovedSand) {
                        report(format!(
                            "Migrated removed [transition].effect {saved:?} to {:?}",
                            TransitionEffect::Crossfade.key()
                        ));
                    }
                }
                Err(error) => report(format!(
                    "Invalid [transition].effect: {error}; keeping {:?}",
                    transition.effect.key()
                )),
            },
            None => report(format!(
                "Invalid [transition].effect: expected a string, found {effect_value}; keeping {:?}",
                transition.effect.key()
            )),
        }
    }

    if let Some(duration_value) = table.get("duration_ms") {
        match duration_value.as_integer() {
            Some(duration) if duration >= 0 => {
                transition.duration_ms = (duration as u64).clamp(100, 10_000);
            }
            _ => report(format!(
                "Invalid [transition].duration_ms: expected a non-negative integer, found {duration_value}; keeping {}",
                transition.duration_ms
            )),
        }
    }

    cfg.transition = transition;
}

impl Config {
    pub fn config_dir() -> PathBuf {
        if let Some(config_dir) =
            configured_runtime_dir(std::env::var_os("WALLPAPER_SWITCHER_CONFIG_DIR"))
        {
            return config_dir;
        }
        if let Ok(xdg) = std::env::var("XDG_CONFIG_HOME") {
            return PathBuf::from(xdg).join("wallpaper-switcher");
        }
        if let Some(home) = dirs::home_dir() {
            return home.join(".config").join("wallpaper-switcher");
        }
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("wallpaper-switcher")
    }

    pub fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    pub fn cache_dir() -> PathBuf {
        Self::config_dir().join("cache")
    }

    pub fn last_modified() -> Option<std::time::SystemTime> {
        fs::metadata(Self::config_path())
            .ok()
            .and_then(|m| m.modified().ok())
    }

    pub fn load() -> Self {
        Self::load_with_diagnostics().config
    }

    pub fn load_read_only() -> Self {
        Self::load_read_only_with_diagnostics().config
    }

    pub fn load_with_diagnostics() -> ConfigLoadOutcome {
        Self::load_with_diagnostics_from_path(Self::config_path(), true)
    }

    pub fn load_read_only_with_diagnostics() -> ConfigLoadOutcome {
        Self::load_with_diagnostics_from_path(Self::config_path(), false)
    }

    fn load_with_diagnostics_from_path(path: PathBuf, create_default: bool) -> ConfigLoadOutcome {
        let backup = path.with_extension("toml.bak");
        let primary = inspect_config_file(&path);
        if matches!(primary, ConfigFileInspection::Valid(_)) {
            let config = Self::load_from_path(path.clone(), false);
            let diagnostic = config_section_load_diagnostic(&path, None);
            if let Some(message) = &diagnostic {
                log_debug(message);
            }
            return ConfigLoadOutcome {
                config,
                source: ConfigLoadSource::Primary,
                diagnostic,
            };
        }

        let backup_inspection = inspect_config_file(&backup);
        if matches!(backup_inspection, ConfigFileInspection::Valid(_)) {
            let recovery = format!(
                "Recovered configuration from the backup because the primary could not be loaded ({}).",
                config_inspection_description(&primary)
            );
            let diagnostic = config_section_load_diagnostic(&backup, Some(recovery));
            log_debug(&format!(
                "{} Primary: {}",
                diagnostic
                    .as_deref()
                    .unwrap_or("Recovered configuration from the backup."),
                path.display()
            ));
            return ConfigLoadOutcome {
                config: Self::load_from_path(backup, false),
                source: ConfigLoadSource::BackupRecovery,
                diagnostic,
            };
        }

        let first_use = matches!(primary, ConfigFileInspection::Missing)
            && matches!(backup_inspection, ConfigFileInspection::Missing);
        let mut diagnostic = (!first_use).then(|| {
            format!(
                "Using default configuration because neither the primary nor backup could be loaded (primary: {}; backup: {}).",
                config_inspection_description(&primary),
                config_inspection_description(&backup_inspection)
            )
        });
        let mut source = if first_use {
            ConfigLoadSource::FirstUseDefaults
        } else {
            ConfigLoadSource::FailureDefaults
        };
        if create_default && first_use {
            match Self::default().save_to_path(&path) {
                Ok(outcome) => {
                    log_debug(&format!(
                        "Created default configuration at {} ({}).",
                        path.display(),
                        outcome.backup_description(&backup)
                    ));
                }
                Err(error) => {
                    source = ConfigLoadSource::FailureDefaults;
                    diagnostic = Some(format!(
                        "Using default configuration; could not create {}: {}",
                        path.display(),
                        error
                    ));
                    log_debug(diagnostic.as_deref().unwrap_or_default());
                }
            }
        }
        if let Some(message) = &diagnostic {
            log_debug(message);
        }
        ConfigLoadOutcome {
            config: Self::default(),
            source,
            diagnostic,
        }
    }

    fn load_from_path(path: PathBuf, create_default: bool) -> Self {
        if !path.exists() {
            // Do not import we-gallery state. This application intentionally has
            // an independent runtime and configuration identity.
        }

        if !path.exists() {
            let cfg = Self::default();
            if create_default {
                if let Err(error) = cfg.save_to_path(&path) {
                    log_debug(&format!(
                        "Could not create default configuration {}: {}",
                        path.display(),
                        error
                    ));
                }
            }
            return cfg;
        }

        let contents = match fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                log_debug(&format!("Failed to read config {}: {}", path.display(), e));
                return Self::default();
            }
        };

        log_debug(&format!("Loading configuration from {}", path.display()));

        let val: toml::Value = match toml::from_str(&contents) {
            Ok(v) => v,
            Err(e) => {
                log_debug(&format!("TOML syntax error in {}: {}", path.display(), e));
                return Self::default();
            }
        };

        let mut cfg = Self::default();

        // 1. Theme preset
        if let Some(preset) = val
            .get("theme")
            .and_then(|t| t.get("preset"))
            .and_then(|p| p.as_str())
        {
            cfg.theme.preset = preset.to_string();
            cfg.apply_theme_preset();
        }

        // 2. Style section
        if let Some(style_val) = val.get("style") {
            match style_val.clone().try_into::<StyleConfig>() {
                Ok(user_style) => {
                    if let Some(tbl) = style_val.as_table() {
                        if tbl.contains_key("accent_color") {
                            cfg.style.accent_color = user_style.accent_color;
                        }
                        if tbl.contains_key("bg_color") {
                            cfg.style.bg_color = user_style.bg_color;
                        }
                        if tbl.contains_key("surface_color") {
                            cfg.style.surface_color = user_style.surface_color;
                        }
                        if tbl.contains_key("overlay_color") {
                            cfg.style.overlay_color = user_style.overlay_color;
                        }
                        if tbl.contains_key("muted_color") {
                            cfg.style.muted_color = user_style.muted_color;
                        }
                        if tbl.contains_key("text_color") {
                            cfg.style.text_color = user_style.text_color;
                        }
                        if tbl.contains_key("highlight_color") {
                            cfg.style.highlight_color = user_style.highlight_color;
                        }
                        if tbl.contains_key("card_width") {
                            cfg.style.card_width = user_style.card_width;
                        }
                        if tbl.contains_key("card_height") {
                            cfg.style.card_height = user_style.card_height;
                        }
                        if tbl.contains_key("card_shear") {
                            cfg.style.card_shear = user_style.card_shear;
                        }
                        if tbl.contains_key("border_width") {
                            cfg.style.border_width = user_style.border_width;
                        }
                        if tbl.contains_key("dim_unselected") {
                            cfg.style.dim_unselected = user_style.dim_unselected;
                        }
                        if tbl.contains_key("backdrop_opacity") {
                            cfg.style.backdrop_opacity = user_style.backdrop_opacity;
                        }
                        if tbl.contains_key("border_glow") {
                            cfg.style.border_glow = user_style.border_glow;
                        }
                        if tbl.contains_key("glow_radius") {
                            cfg.style.glow_radius = user_style.glow_radius;
                        }
                        if tbl.contains_key("motion_blur") {
                            cfg.style.motion_blur = user_style.motion_blur;
                        }
                        if tbl.contains_key("motion_blur_strength") {
                            cfg.style.motion_blur_strength = user_style.motion_blur_strength;
                        }
                    }
                }
                Err(e) => {
                    log_debug(&format!("Failed to deserialize [style] section: {}", e));
                }
            }
        }

        // 3. UI section
        if let Some(ui_val) = val.get("ui") {
            match ui_val.clone().try_into::<UiConfig>() {
                Ok(user_ui) => {
                    if let Some(bo) = user_ui.backdrop_opacity {
                        cfg.style.backdrop_opacity = bo;
                    }
                    if let Some(cw) = user_ui.card_width {
                        cfg.style.card_width = cw;
                    }
                    if let Some(ch) = user_ui.card_height {
                        cfg.style.card_height = ch;
                    }
                    if let Some(cs) = user_ui.card_shear {
                        cfg.style.card_shear = cs;
                    }
                    if let Some(bw) = user_ui.border_width {
                        cfg.style.border_width = bw;
                    }
                    if let Some(bg) = user_ui.border_glow {
                        cfg.style.border_glow = bg;
                    }
                    if let Some(gr) = user_ui.glow_radius {
                        cfg.style.glow_radius = gr;
                    }
                    if let Some(mb) = user_ui.motion_blur {
                        cfg.style.motion_blur = mb;
                    }
                    if let Some(mbs) = user_ui.motion_blur_strength {
                        cfg.style.motion_blur_strength = mbs;
                    }
                    log_debug(&format!(
                        "Loaded [ui] config: show_top_bar={}, show_hint_bar={}, show_selection_border={}, dim_unselected={}",
                        user_ui.show_top_bar, user_ui.show_hint_bar, user_ui.show_selection_border, user_ui.dim_unselected
                    ));
                    cfg.ui = user_ui;
                }
                Err(e) => {
                    log_debug(&format!("Failed to deserialize [ui] section: {}", e));
                }
            }
        }

        // 4. Keybinds section
        if let Some(kb_val) = val.get("keybinds") {
            match kb_val.clone().try_into::<KeybindsConfig>() {
                Ok(user_kb) => cfg.keybinds = user_kb,
                Err(e) => log_debug(&format!("Failed to deserialize [keybinds] section: {}", e)),
            }
        }

        // 5. Behavior section
        if let Some(beh_val) = val.get("behavior") {
            match beh_val.clone().try_into::<BehaviorConfig>() {
                Ok(user_beh) => cfg.behavior = user_beh,
                Err(e) => log_debug(&format!("Failed to deserialize [behavior] section: {}", e)),
            }
        }

        if let Some(catalog_val) = val.get("catalog") {
            match catalog_val.clone().try_into::<CatalogConfig>() {
                Ok(user_catalog) => cfg.catalog = user_catalog,
                Err(e) => log_debug(&format!("Failed to deserialize [catalog] section: {}", e)),
            }
        }

        if let Some(slideshow_val) = val.get("slideshow") {
            match slideshow_val.clone().try_into::<SlideshowConfig>() {
                Ok(mut slideshow) => {
                    slideshow.interval_minutes = slideshow.interval_minutes.max(1);
                    if slideshow.target_monitor.trim().is_empty() {
                        slideshow.target_monitor = default_slideshow_target();
                    }
                    cfg.slideshow = slideshow;
                }
                Err(e) => log_debug(&format!("Failed to deserialize [slideshow] section: {}", e)),
            }
        }

        if let Some(cap) = val.get("cache_max_mib") {
            cfg.cache_max_mib = cache_max_mib(cap);
        }
        if let Some(threads) = val.get("poster_decode_threads") {
            cfg.poster_decode_threads = poster_decode_threads(threads);
        }
        if let Some(threads) = val.get("catalog_scan_threads") {
            cfg.catalog_scan_threads = catalog_scan_threads(threads);
        }

        if let Some(transition_val) = val.get("transition") {
            apply_transition_section(&mut cfg, transition_val, &mut |message| log_debug(&message));
        }

        // 6. Excluded Wallpapers section (Layer 1: soft-hidden, toggleable with 'x', reveal with 'h')
        if let Some(ex_val) = val.get("excluded_wallpapers") {
            if let Ok(user_ex) = ex_val.clone().try_into::<HashMap<String, Vec<String>>>() {
                cfg.excluded_wallpapers = user_ex;
            }
        }

        // 7. Disabled / Permanently Hidden Wallpapers section (Layer 2: config-only, NEVER shown even with 'h')
        for key in &[
            "disabled_wallpapers",
            "blocked_wallpapers",
            "always_hidden_wallpapers",
            "blacklist",
        ] {
            if let Some(dis_val) = val.get(*key) {
                if let Ok(user_dis) = dis_val.clone().try_into::<HashMap<String, Vec<String>>>() {
                    cfg.disabled_wallpapers = user_dis;
                    break;
                }
            }
        }

        cfg
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        self.save_with_diagnostics()
            .map(|_| ())
            .map_err(|error| std::io::Error::new(std::io::ErrorKind::Other, error.to_string()))
    }

    pub fn save_with_diagnostics(&self) -> Result<ConfigSaveOutcome, ConfigSaveError> {
        self.save_to_path(&Self::config_path())
    }

    fn save_to_path(&self, path: &Path) -> Result<ConfigSaveOutcome, ConfigSaveError> {
        let mut replace_existing = |replacement: &Path, destination: &Path, backup: &Path| {
            file_publication::replace_existing_file(replacement, destination, backup)
        };
        self.save_to_path_with(path, &mut replace_existing)
    }

    fn save_to_path_with<F>(
        &self,
        path: &Path,
        replace_existing: &mut F,
    ) -> Result<ConfigSaveOutcome, ConfigSaveError>
    where
        F: FnMut(&Path, &Path, &Path) -> io::Result<()>,
    {
        let parent = path.parent().ok_or_else(|| {
            ConfigSaveError::new(
                ConfigSaveStage::CreateDirectory,
                ConfigFileStatus::Missing,
                ConfigBackupStatus::Unavailable,
                "configuration path has no parent",
            )
        })?;
        if let Err(error) = fs::create_dir_all(parent) {
            return Err(ConfigSaveError::new(
                ConfigSaveStage::CreateDirectory,
                ConfigFileStatus::Missing,
                ConfigBackupStatus::Unavailable,
                error,
            ));
        }

        let temporary = file_publication::unique_recovery_path(path, "config-tmp");
        let backup = path.with_extension("toml.bak");
        let backup_temporary = file_publication::unique_recovery_path(&backup, "config-backup-tmp");
        let existing_primary = inspect_config_file(path);
        let existing_backup = inspect_config_file(&backup);
        let primary_status = existing_primary.status();
        let mut backup_status = config_backup_status(&existing_backup);

        let result = (|| {
            let contents = toml::to_string_pretty(self).map_err(|error| {
                ConfigSaveError::new(
                    ConfigSaveStage::PreparePrimary,
                    primary_status,
                    backup_status,
                    error,
                )
            })?;
            write_complete_config_file(
                &temporary,
                contents.as_bytes(),
                ConfigSaveStage::PreparePrimary,
                primary_status,
                backup_status,
            )?;
            if !matches!(
                inspect_config_file(&temporary),
                ConfigFileInspection::Valid(_)
            ) {
                return Err(ConfigSaveError::new(
                    ConfigSaveStage::PreparePrimary,
                    primary_status,
                    backup_status,
                    "staged configuration did not pass TOML validation",
                ));
            }

            if let ConfigFileInspection::Unreadable(error) = &existing_primary {
                return Err(ConfigSaveError::new(
                    ConfigSaveStage::InspectExisting,
                    primary_status,
                    backup_status,
                    error,
                ));
            }
            if let ConfigFileInspection::Unreadable(error) = &existing_backup {
                return Err(ConfigSaveError::new(
                    ConfigSaveStage::InspectExisting,
                    primary_status,
                    ConfigBackupStatus::Unavailable,
                    error,
                ));
            }

            if let ConfigFileInspection::Valid(previous_primary) = &existing_primary {
                write_complete_config_file(
                    &backup_temporary,
                    previous_primary.as_bytes(),
                    ConfigSaveStage::PrepareBackup,
                    primary_status,
                    backup_status,
                )?;
                if !matches!(
                    inspect_config_file(&backup_temporary),
                    ConfigFileInspection::Valid(_)
                ) {
                    return Err(ConfigSaveError::new(
                        ConfigSaveStage::PrepareBackup,
                        primary_status,
                        backup_status,
                        "staged configuration backup did not pass TOML validation",
                    ));
                }
                publish_config_file_with(&backup_temporary, &backup, replace_existing).map_err(
                    |error| {
                        config_publication_save_error(
                            ConfigSaveStage::PublishBackup,
                            path,
                            &backup,
                            error,
                        )
                    },
                )?;
                if !matches!(inspect_config_file(&backup), ConfigFileInspection::Valid(_)) {
                    return Err(observed_config_save_error(
                        ConfigSaveStage::PublishBackup,
                        path,
                        &backup,
                        "published configuration backup could not be validated",
                    ));
                }
                backup_status = ConfigBackupStatus::Published;
            }

            publish_config_file_with(&temporary, path, replace_existing).map_err(|error| {
                config_publication_save_error(ConfigSaveStage::PublishPrimary, path, &backup, error)
            })?;
            if !matches!(inspect_config_file(path), ConfigFileInspection::Valid(_)) {
                return Err(observed_config_save_error(
                    ConfigSaveStage::PublishPrimary,
                    path,
                    &backup,
                    "published configuration could not be validated",
                ));
            }
            Ok(ConfigSaveOutcome {
                backup: backup_status,
            })
        })();

        let recovery_paths = result
            .as_ref()
            .err()
            .map(|error| error.recovery_paths().to_vec())
            .unwrap_or_default();
        if !recovery_paths
            .iter()
            .any(|candidate| candidate == &temporary)
        {
            let _ = file_publication::remove_if_present(&temporary);
        }
        if !recovery_paths
            .iter()
            .any(|candidate| candidate == &backup_temporary)
        {
            let _ = file_publication::remove_if_present(&backup_temporary);
        }
        result
    }

    pub fn apply_theme_preset(&mut self) {
        match self.theme.preset.to_lowercase().as_str() {
            "rose-pine-moon" => {
                // Rosé Pine Moon exact palette
                self.style.accent_color = "#eb6f92".to_string(); // Love
                self.style.bg_color = "#232136".to_string(); // Base
                self.style.surface_color = "#2a273f".to_string(); // Surface
                self.style.overlay_color = "#393552".to_string(); // Overlay
                self.style.muted_color = "#6e6a86".to_string(); // Muted
                self.style.text_color = "#e0def4".to_string(); // Text
                self.style.highlight_color = "#44415a".to_string(); // Highlight Med
            }
            "rose-pine" => {
                self.style.accent_color = "#eb6f92".to_string();
                self.style.bg_color = "#191724".to_string();
                self.style.surface_color = "#26233a".to_string();
                self.style.overlay_color = "#312f44".to_string();
                self.style.muted_color = "#6e6a86".to_string();
                self.style.text_color = "#e0def4".to_string();
                self.style.highlight_color = "#403d52".to_string();
            }
            "catppuccin-mocha" => {
                self.style.accent_color = "#f5c2e7".to_string(); // Pink
                self.style.bg_color = "#1e1e2e".to_string(); // Base
                self.style.surface_color = "#313244".to_string(); // Surface0
                self.style.overlay_color = "#45475a".to_string(); // Surface1
                self.style.muted_color = "#6c7086".to_string(); // Overlay0
                self.style.text_color = "#cdd6f4".to_string(); // Text
                self.style.highlight_color = "#585b70".to_string();
            }
            "tokyo-night" => {
                self.style.accent_color = "#bb9af7".to_string();
                self.style.bg_color = "#1a1b26".to_string();
                self.style.surface_color = "#24283b".to_string();
                self.style.overlay_color = "#414868".to_string();
                self.style.muted_color = "#565f89".to_string();
                self.style.text_color = "#c0caf5".to_string();
                self.style.highlight_color = "#7aa2f7".to_string();
            }
            "nord" => {
                self.style.accent_color = "#88c0d0".to_string();
                self.style.bg_color = "#2e3440".to_string();
                self.style.surface_color = "#3b4252".to_string();
                self.style.overlay_color = "#434c5e".to_string();
                self.style.muted_color = "#4c566a".to_string();
                self.style.text_color = "#eceff4".to_string();
                self.style.highlight_color = "#81a1c1".to_string();
            }
            _ => {} // custom
        }
    }

    pub fn is_excluded(&self, monitor_key: &str, id: &str) -> bool {
        self.excluded_wallpapers
            .get(monitor_key)
            .map(|list| list.iter().any(|item| item == id))
            .unwrap_or(false)
    }

    pub fn is_disabled(&self, monitor_key: &str, id: &str) -> bool {
        // Check per-monitor disabled list
        if let Some(list) = self.disabled_wallpapers.get(monitor_key) {
            if list.iter().any(|item| item == id) {
                return true;
            }
        }
        // Check global / wildcard disabled list if specified
        if let Some(list) = self
            .disabled_wallpapers
            .get("global")
            .or_else(|| self.disabled_wallpapers.get("*"))
        {
            if list.iter().any(|item| item == id) {
                return true;
            }
        }
        false
    }

    pub fn toggle_excluded(&mut self, monitor_key: &str, id: &str) -> bool {
        let list = self
            .excluded_wallpapers
            .entry(monitor_key.to_string())
            .or_default();
        if let Some(pos) = list.iter().position(|x| x == id) {
            list.remove(pos);
            false
        } else {
            list.push(id.to_string());
            true
        }
    }
}

fn inspect_config_file(path: &Path) -> ConfigFileInspection {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return ConfigFileInspection::Missing;
        }
        Err(error) => return ConfigFileInspection::Unreadable(error.to_string()),
    };
    match toml::from_str::<toml::Value>(&contents) {
        Ok(_) => ConfigFileInspection::Valid(contents),
        Err(error) => ConfigFileInspection::Invalid(error.to_string()),
    }
}

fn config_inspection_description(inspection: &ConfigFileInspection) -> String {
    match inspection {
        ConfigFileInspection::Valid(_) => "valid".to_string(),
        ConfigFileInspection::Missing => "missing".to_string(),
        ConfigFileInspection::Invalid(error) => format!("invalid TOML ({error})"),
        ConfigFileInspection::Unreadable(error) => format!("unreadable ({error})"),
    }
}

fn config_section_load_diagnostic(path: &Path, base: Option<String>) -> Option<String> {
    let mut messages = Vec::new();
    if let Some(base) = base {
        messages.push(base);
    }
    messages.extend(config_section_diagnostics(path).into_iter().map(|warning| {
        format!(
            "Configuration load warning for {}: {}",
            path.display(),
            warning
        )
    }));
    (!messages.is_empty()).then(|| messages.join(" "))
}

fn cache_max_mib(value: &toml::Value) -> Option<u32> {
    u32::try_from(value.as_integer()?)
        .ok()
        .filter(|&mib| mib > 0)
}

fn poster_decode_threads(value: &toml::Value) -> Option<u8> {
    u8::try_from(value.as_integer()?)
        .ok()
        .filter(|threads| (1..=4).contains(threads))
}

fn catalog_scan_threads(value: &toml::Value) -> Option<u8> {
    u8::try_from(value.as_integer()?)
        .ok()
        .filter(|threads| (1..=8).contains(threads))
}

fn config_section_diagnostics(path: &Path) -> Vec<String> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Vec::new();
    };
    let Ok(value) = toml::from_str::<toml::Value>(&contents) else {
        return Vec::new();
    };
    let mut diagnostics = Vec::new();
    if let Some(theme) = value.get("theme") {
        if theme.get("preset").is_some_and(|preset| !preset.is_str()) {
            diagnostics.push(
                "Failed to deserialize [theme].preset: expected a string; keeping the default"
                    .to_string(),
            );
        }
    }
    if let Some(style) = value.get("style") {
        if let Err(error) = style.clone().try_into::<StyleConfig>() {
            diagnostics.push(format!("Failed to deserialize [style] section: {error}"));
        }
    }
    if let Some(ui) = value.get("ui") {
        if let Err(error) = ui.clone().try_into::<UiConfig>() {
            diagnostics.push(format!("Failed to deserialize [ui] section: {error}"));
        }
    }
    if let Some(keybinds) = value.get("keybinds") {
        if let Err(error) = keybinds.clone().try_into::<KeybindsConfig>() {
            diagnostics.push(format!("Failed to deserialize [keybinds] section: {error}"));
        }
    }
    if let Some(behavior) = value.get("behavior") {
        if let Err(error) = behavior.clone().try_into::<BehaviorConfig>() {
            diagnostics.push(format!("Failed to deserialize [behavior] section: {error}"));
        }
    }
    if let Some(catalog) = value.get("catalog") {
        if let Err(error) = catalog.clone().try_into::<CatalogConfig>() {
            diagnostics.push(format!("Failed to deserialize [catalog] section: {error}"));
        }
    }
    if let Some(slideshow) = value.get("slideshow") {
        if let Err(error) = slideshow.clone().try_into::<SlideshowConfig>() {
            diagnostics.push(format!(
                "Failed to deserialize [slideshow] section: {error}"
            ));
        }
    }
    if value
        .get("cache_max_mib")
        .is_some_and(|cap| cache_max_mib(cap).is_none())
    {
        diagnostics
            .push("cache_max_mib must be a positive integer; using the automatic cap".into());
    }
    if value
        .get("poster_decode_threads")
        .is_some_and(|threads| poster_decode_threads(threads).is_none())
    {
        diagnostics.push("poster_decode_threads must be an integer from 1 to 4".into());
    }
    if value
        .get("catalog_scan_threads")
        .is_some_and(|threads| catalog_scan_threads(threads).is_none())
    {
        diagnostics.push("catalog_scan_threads must be an integer from 1 to 8".into());
    }
    if let Some(transition) = value.get("transition") {
        let mut defaults = Config::default();
        apply_transition_section(&mut defaults, transition, &mut |message| {
            diagnostics.push(message)
        });
    }
    if let Some(excluded) = value.get("excluded_wallpapers") {
        if excluded
            .clone()
            .try_into::<HashMap<String, Vec<String>>>()
            .is_err()
        {
            diagnostics.push(
                "Failed to deserialize [excluded_wallpapers] section; keeping the default"
                    .to_string(),
            );
        }
    }
    for key in &[
        "disabled_wallpapers",
        "blocked_wallpapers",
        "always_hidden_wallpapers",
        "blacklist",
    ] {
        if let Some(disabled) = value.get(*key) {
            if disabled
                .clone()
                .try_into::<HashMap<String, Vec<String>>>()
                .is_err()
            {
                diagnostics.push(format!(
                    "Failed to deserialize [{key}] section; keeping the default"
                ));
            }
            break;
        }
    }
    diagnostics
}

fn config_backup_status(inspection: &ConfigFileInspection) -> ConfigBackupStatus {
    if matches!(inspection, ConfigFileInspection::Valid(_)) {
        ConfigBackupStatus::Preserved
    } else {
        ConfigBackupStatus::Unavailable
    }
}

fn write_complete_config_file(
    path: &Path,
    contents: &[u8],
    stage: ConfigSaveStage,
    primary: ConfigFileStatus,
    backup: ConfigBackupStatus,
) -> Result<(), ConfigSaveError> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| ConfigSaveError::new(stage, primary, backup, error))?;
    file.write_all(contents)
        .map_err(|error| ConfigSaveError::new(stage, primary, backup, error))?;
    file.flush()
        .and_then(|()| file.sync_all())
        .map_err(|error| ConfigSaveError::new(stage, primary, backup, error))?;
    Ok(())
}

fn publish_config_file_with<F>(
    replacement: &Path,
    destination: &Path,
    replace_existing: &mut F,
) -> Result<(), ConfigPublicationError>
where
    F: FnMut(&Path, &Path, &Path) -> io::Result<()>,
{
    file_publication::publish_staged_file_with(
        replacement,
        destination,
        "config-replace-recovery",
        replace_existing,
    )
    .map_err(Into::into)
}

fn add_config_recovery_path(
    paths: &mut Vec<PathBuf>,
    path: &Path,
    inspection: &ConfigFileInspection,
) {
    if matches!(inspection, ConfigFileInspection::Valid(_))
        && !paths.iter().any(|candidate| candidate == path)
    {
        paths.push(path.to_path_buf());
    }
}

fn config_publication_save_error(
    stage: ConfigSaveStage,
    primary_path: &Path,
    backup_path: &Path,
    publication: ConfigPublicationError,
) -> ConfigSaveError {
    let primary = inspect_config_file(primary_path);
    let backup = inspect_config_file(backup_path);
    let mut recovery_paths = Vec::new();
    add_config_recovery_path(&mut recovery_paths, primary_path, &primary);
    add_config_recovery_path(&mut recovery_paths, backup_path, &backup);
    let staged = inspect_config_file(&publication.replacement);
    if !file_publication::failed_before_replace_file(&publication.error) {
        add_config_recovery_path(&mut recovery_paths, &publication.replacement, &staged);
    }
    if let Some(recovery) = &publication.recovery {
        let inspection = inspect_config_file(recovery);
        add_config_recovery_path(&mut recovery_paths, recovery, &inspection);
    }

    let error_name = publication
        .replace_file_called
        .then(|| file_publication::replace_file_error_name(&publication.error));
    let message = if publication.publication_succeeded {
        format!(
            "publication succeeded but tracked recovery cleanup for {} failed: {}",
            publication.destination.display(),
            publication.error
        )
    } else if let Some(error_name) = error_name {
        format!("{} ({error_name})", publication.error)
    } else {
        publication.error.to_string()
    };
    ConfigSaveError::observed(
        stage,
        primary.status(),
        config_backup_status(&backup),
        recovery_paths,
        message,
    )
}

fn observed_config_save_error(
    stage: ConfigSaveStage,
    primary_path: &Path,
    backup_path: &Path,
    message: impl ToString,
) -> ConfigSaveError {
    let primary = inspect_config_file(primary_path);
    let backup = inspect_config_file(backup_path);
    let mut recovery_paths = Vec::new();
    add_config_recovery_path(&mut recovery_paths, primary_path, &primary);
    add_config_recovery_path(&mut recovery_paths, backup_path, &backup);
    ConfigSaveError::observed(
        stage,
        primary.status(),
        config_backup_status(&backup),
        recovery_paths,
        message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn b2_config_cleanup_failure_keeps_success_facts_and_published_bytes() {
        let fixture = TestDirectory::new("b2-cleanup-after-success");
        let destination = fixture.config_path();
        let staged = destination.parent().unwrap().join("staged.toml");
        let mut config = Config::default();
        config.theme.preset = "previous".into();
        let previous = toml::to_string_pretty(&config).unwrap();
        config.theme.preset = "next".into();
        let next = toml::to_string_pretty(&config).unwrap();
        fs::write(&destination, &previous).unwrap();
        fs::write(&staged, &next).unwrap();
        let publication = publish_config_file_with(
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
        let recovery = publication.recovery.clone().unwrap();
        assert!(recovery
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("config-replace-recovery"));
        assert_eq!(
            fs::read(recovery.join("previous.toml")).unwrap(),
            previous.as_bytes()
        );
        assert_eq!(fs::read(&destination).unwrap(), next.as_bytes());
        assert!(!staged.exists());
        let error = config_publication_save_error(
            ConfigSaveStage::PublishPrimary,
            &destination,
            &destination.with_extension("toml.bak"),
            publication,
        );
        assert_eq!(error.stage, ConfigSaveStage::PublishPrimary);
        assert!(error
            .to_string()
            .contains("publication succeeded but tracked recovery cleanup"));
        assert!(error.recovery_paths().contains(&destination));
    }

    fn r7_fixture_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "r7-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn r7_default_logger_never_resolves_or_writes_a_profile() {
        let daily_standin = r7_fixture_root("daily-standin");
        let resolved = std::cell::Cell::new(false);
        log_debug_with_dir("r7 routing regression", || {
            resolved.set(true);
            daily_standin.clone()
        });
        assert!(
            !daily_standin.exists(),
            "unit-test logger created the injected daily-profile stand-in"
        );
        assert!(
            !resolved.get(),
            "unit-test logger resolved the application profile"
        );
    }

    #[test]
    fn r7_file_policy_retains_configured_and_default_destinations() {
        let fallback = PathBuf::from("owned-default");
        for (value, expected) in [
            (None, fallback.clone()),
            (Some("".into()), fallback.clone()),
            (
                Some("owned-configured".into()),
                PathBuf::from("owned-configured"),
            ),
        ] {
            let dir = configured_runtime_dir(value).unwrap_or_else(|| fallback.clone());
            let DebugDestination::File(actual) = file_debug_destination(dir) else {
                panic!("production file policy changed");
            };
            assert_eq!(actual, expected.join("wallpaper-switcher.log"));
        }
    }

    #[test]
    fn r7_owned_file_writer_appends_complete_production_records() {
        let fixture = TestDirectory::new("r7-file-append");
        let log = fixture.path.join("nested/wallpaper-switcher.log");
        write_debug(DebugDestination::File(log.clone()), "first 日本語");
        assert_eq!(fs::read(&log).unwrap(), "[DEBUG] first 日本語\n".as_bytes());
        write_debug(
            DebugDestination::File(log.clone()),
            "second\nexisting multiline format",
        );
        assert_eq!(
            fs::read(&log).unwrap(),
            "[DEBUG] first 日本語\n[DEBUG] second\nexisting multiline format\n".as_bytes()
        );
    }

    #[test]
    fn r7_owned_file_failure_is_best_effort_without_redirection() {
        let fixture = TestDirectory::new("r7-file-failure");
        let blocker = fixture.path.join("blocker");
        fs::write(&blocker, b"preserve blocker").unwrap();
        write_debug(
            DebugDestination::File(blocker.join("wallpaper-switcher.log")),
            "unavailable sink",
        );
        assert_eq!(fs::read(&blocker).unwrap(), b"preserve blocker");
        assert_eq!(fs::read_dir(&fixture.path).unwrap().count(), 1);

        struct BrokenWriter;
        impl Write for BrokenWriter {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "owned failure"))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        assert_eq!(
            write_debug_record(&mut BrokenWriter, "diagnostic")
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
    }

    const R7_CHILD_ROOT: &str = "WALLPAPER_SWITCHER_R7_LOGGING_FIXTURE";
    const R7_CAPTURE_LIMIT: usize = 32 * 1024;

    // This exact unit-test fixture is the only child entry point. No application main.
    #[test]
    fn r7_logging_child() {
        let Some(root) = std::env::var_os(R7_CHILD_ROOT).map(PathBuf::from) else {
            return;
        };
        assert!(matches!(
            debug_destination(|| panic!("test route resolved profile")),
            DebugDestination::Stderr
        ));
        crate::log_debug("r7 alias diagnostic");
        assert!(!crate::apply_trace::enabled());
        crate::apply_trace::event_for_lazy(99, "disabled", || panic!("disabled details evaluated"));
        crate::apply_trace::set_enabled(true);
        let built = std::cell::Cell::new(0);
        crate::apply_trace::with_operation(42, || {
            crate::apply_trace::event_lazy("r7-enabled", || {
                built.set(built.get() + 1);
                "source=r7-fixture\nitem=owned".into()
            });
            crate::apply_trace::with_operation(43, || {
                crate::apply_trace::event("r7-nested", "source=nested")
            });
            crate::apply_trace::event("r7-restored", "source=parent");
        });
        assert_eq!(built.get(), 1);
        crate::apply_trace::event("r7-unbound", "source=unbound");
        let workers: Vec<_> = (0..4)
            .map(|index| {
                std::thread::spawn(move || {
                    crate::log_debug(&format!("r7 worker diagnostic {index}"));
                    crate::apply_trace::event_for_lazy(70 + index, "r7-worker", || {
                        format!("source=worker-{index}")
                    });
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
        crate::apply_trace::set_enabled(false);
        crate::apply_trace::event_lazy("disabled-after", || panic!("disabled details evaluated"));

        // Exercise real failure/default and backup recovery callers with explicit owned I/O.
        let broken = root.join("broken.toml");
        let outcome = Config::load_with_diagnostics_from_path(broken.clone(), false);
        assert_eq!(outcome.source, ConfigLoadSource::FailureDefaults);
        assert!(outcome
            .diagnostic
            .unwrap()
            .contains("neither the primary nor backup"));
        let recovered = root.join("recovered.toml");
        let outcome = Config::load_with_diagnostics_from_path(recovered.clone(), false);
        assert_eq!(outcome.source, ConfigLoadSource::BackupRecovery);
        assert_eq!(outcome.config.theme.preset, "nord");
        assert!(outcome
            .diagnostic
            .unwrap()
            .contains("Recovered configuration"));
        assert_eq!(fs::read(broken).unwrap(), b"[invalid");
        assert_eq!(fs::read(recovered).unwrap(), b"[invalid");
    }

    fn r7_check_logging_child(profile_case: &str) {
        use std::io::Read;
        use std::process::{Command, Stdio};
        use std::time::{Duration, Instant};
        let fixture = TestDirectory::new(&format!("r7-child-{profile_case}"));
        fs::write(fixture.path.join("broken.toml"), b"[invalid").unwrap();
        fs::write(fixture.path.join("recovered.toml"), b"[invalid").unwrap();
        fs::write(
            fixture.path.join("recovered.toml.bak"),
            b"[theme]\npreset='nord'\n",
        )
        .unwrap();
        let standin = fixture.path.join("daily-standin");
        let blocker = fixture.path.join("blocked-profile");
        let mut command = Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "config::tests::r7_logging_child",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(R7_CHILD_ROOT, &fixture.path)
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        match profile_case {
            "absent" => {
                command.env_remove("WALLPAPER_SWITCHER_CONFIG_DIR");
            }
            "empty" => {
                command
                    .env("WALLPAPER_SWITCHER_CONFIG_DIR", "")
                    .env("XDG_CONFIG_HOME", &standin);
            }
            "invalid" => {
                fs::write(&blocker, b"owned profile blocker").unwrap();
                command
                    .env("WALLPAPER_SWITCHER_CONFIG_DIR", &blocker)
                    .env("XDG_CONFIG_HOME", &standin);
            }
            "configured" => {
                command
                    .env(
                        "WALLPAPER_SWITCHER_CONFIG_DIR",
                        fixture.path.join("unused-profile"),
                    )
                    .env("XDG_CONFIG_HOME", &standin);
            }
            _ => panic!("unknown fixture case"),
        }
        let mut child = command.spawn().expect("owned logging test child");
        let pid = child.id();
        let mut stderr = child.stderr.take().unwrap();
        // Fixed-size reads keep capture bounded while draining both normal and excess output.
        let reader = std::thread::spawn(move || {
            let mut retained = Vec::new();
            let mut overflow = false;
            let mut buffer = [0; 1024];
            loop {
                let count = stderr.read(&mut buffer).unwrap();
                if count == 0 {
                    break;
                }
                let keep = count.min(R7_CAPTURE_LIMIT - retained.len());
                retained.extend_from_slice(&buffer[..keep]);
                overflow |= keep < count;
            }
            (retained, overflow)
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() >= deadline {
                child.kill().expect("stop exact owned fixture child");
                let _ = child.wait();
                let _ = reader.join();
                panic!("owned logging test child exceeded deadline");
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let (bytes, overflow) = reader.join().unwrap();
        let stderr = String::from_utf8(bytes).unwrap();
        eprintln!("R7 child {profile_case} pid={pid} captured stderr:\n{stderr}");
        assert!(status.success(), "logging child failed: {stderr}");
        assert!(!overflow, "logging fixture exceeded bounded capture");
        assert!(stderr.contains("[DEBUG] r7 alias diagnostic\n"));
        for expected in [
            "op=42 stage=r7-enabled source=r7-fixture item=owned",
            "op=43 stage=r7-nested source=nested",
            "op=42 stage=r7-restored source=parent",
            "op=none stage=r7-unbound source=unbound",
        ] {
            assert!(
                stderr.lines().any(|line| line
                    .starts_with("[DEBUG] [APPLY-TRACE] timestamp_unix_ms=")
                    && line.contains(&format!("pid={pid} {expected}"))),
                "missing {expected}: {stderr}"
            );
        }
        for index in 0..4 {
            assert!(stderr.contains(&format!("[DEBUG] r7 worker diagnostic {index}\n")));
            assert!(stderr.lines().any(|line| line.contains(&format!(
                "pid={pid} op={} stage=r7-worker source=worker-{index}",
                70 + index
            ))));
        }
        assert!(!stderr.contains("stage=disabled"));
        assert!(
            stderr.contains("Using default configuration because neither the primary nor backup")
        );
        assert!(stderr.contains("Recovered configuration from the backup"));
        assert!(!standin.exists());
        assert!(!fixture.path.join("unused-profile").exists());
        assert!(!fixture.path.join("wallpaper-switcher.log").exists());
        assert_eq!(
            fs::read_dir(&fixture.path).unwrap().count(),
            if profile_case == "invalid" { 4 } else { 3 }
        );
        if profile_case == "invalid" {
            assert_eq!(fs::read(blocker).unwrap(), b"owned profile blocker");
        }
    }

    #[test]
    fn r7_absent_override_child_preserves_diagnostics_without_daily_logging() {
        r7_check_logging_child("absent");
    }

    #[test]
    fn r7_empty_override_child_cannot_restore_daily_fallback() {
        r7_check_logging_child("empty");
    }

    #[test]
    fn r7_invalid_override_child_cannot_restore_daily_fallback() {
        r7_check_logging_child("invalid");
    }

    #[test]
    fn r7_configured_override_child_still_uses_nonfilesystem_test_sink() {
        r7_check_logging_child("configured");
    }

    #[cfg(windows)]
    #[test]
    fn r3_long_config_replacement_preserves_exact_previous_bytes() {
        use std::os::windows::ffi::OsStrExt;
        let root = std::env::temp_dir().join(format!("r3-config-{}", std::process::id()));
        let directory = root
            .join("configuration 日本語 directory".repeat(4))
            .join("nested-valid-config-".repeat(6));
        let path = directory.join("config.toml");
        assert!(path.as_os_str().encode_wide().count() > 260);
        let mut config = Config::default();
        config.theme.preset = "first".into();
        config.save_to_path(&path).unwrap();
        let initial = fs::read(&path).unwrap();
        config.theme.preset = "second".into();
        config.save_to_path(&path).unwrap();
        assert_eq!(fs::read(path.with_extension("toml.bak")).unwrap(), initial);
        assert_eq!(
            Config::load_from_path(path.clone(), false).theme.preset,
            "second"
        );
        config.theme.preset = "third".into();
        config.save_to_path(&path).unwrap();
        assert_eq!(
            Config::load_from_path(path.clone(), false).theme.preset,
            "third"
        );
        eprintln!(
            "R3 config path={path:?}, UTF16 units={}",
            path.as_os_str().encode_wide().count()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn r3_config_before_call_rejection_cleans_stages_and_preserves_existing_config() {
        let fixture = TestDirectory::new("r3-before-call");
        let path = fixture.config_path();
        let current = Config::default();
        current.save_to_path(&path).unwrap();
        let original = fs::read(&path).unwrap();
        let mut next = current.clone();
        next.theme.preset = "new".into();
        let mut reject = |_: &Path, _: &Path, _: &Path| {
            Err(file_publication::before_replace_file_error(io::Error::new(
                io::ErrorKind::InvalidInput,
                "injected component validation rejection",
            )))
        };
        let error = next.save_to_path_with(&path, &mut reject).unwrap_err();
        assert!(error.to_string().contains("before ReplaceFileW"));
        assert_eq!(fs::read(&path).unwrap(), original);
        assert_eq!(fs::read(path.with_extension("toml.bak")).unwrap(), original);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 2);
        assert!(!error.recovery_paths().iter().any(|path| path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .contains("tmp")));
    }

    #[test]
    fn r3_actual_rename_failure_retains_valid_config_stage() {
        let fixture = TestDirectory::new("r3-rename-failure");
        let staged = fixture.config_path();
        Config::default().save_to_path(&staged).unwrap();
        let destination = staged
            .parent()
            .unwrap()
            .join("missing-parent")
            .join("config.toml");
        let publication = publish_config_file_with(&staged, &destination, &mut |_, _, _| {
            panic!("Missing destination uses rename")
        })
        .unwrap_err();
        assert!(!publication.replace_file_called);
        let error = config_publication_save_error(
            ConfigSaveStage::PublishPrimary,
            &destination,
            &destination.with_extension("toml.bak"),
            publication,
        );
        assert!(error.recovery_paths().contains(&staged));
        assert!(staged.is_file());
    }

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "wallpaper-switcher-{label}-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("test clock")
                    .as_nanos()
            ));
            fs::create_dir_all(&path).expect("create disposable config fixture");
            Self { path }
        }

        fn config_path(&self) -> PathBuf {
            self.path.join("config.toml")
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn disk_cache_override_survives_save_and_invalid_values_use_automatic_budget() {
        let fixture = TestDirectory::new("disk-cache-cap");
        let path = fixture.config_path();
        fs::write(&path, "cache_max_mib = 32\n[theme]\npreset = 'nord'\n").unwrap();
        let config = Config::load_from_path(path.clone(), false);
        assert_eq!(config.cache_max_mib, Some(32));
        config.save_to_path(&path).unwrap();
        assert_eq!(
            Config::load_from_path(path.clone(), false).cache_max_mib,
            Some(32)
        );
        fs::write(&path, "cache_max_mib = -1\n[theme]\npreset = 'nord'\n").unwrap();
        let outcome = Config::load_with_diagnostics_from_path(path, false);
        assert_eq!(outcome.config.cache_max_mib, None);
        assert_eq!(outcome.config.theme.preset, "nord");
        assert!(outcome.diagnostic.unwrap().contains("cache_max_mib"));
    }

    #[test]
    fn poster_decode_thread_override_survives_save_and_rejects_out_of_range_values() {
        let fixture = TestDirectory::new("poster-decode-threads");
        let path = fixture.config_path();
        fs::write(&path, "poster_decode_threads = 1\n").unwrap();
        let config = Config::load_from_path(path.clone(), false);
        assert_eq!(config.poster_decode_threads, Some(1));
        config.save_to_path(&path).unwrap();
        assert_eq!(
            Config::load_from_path(path.clone(), false).poster_decode_threads,
            Some(1)
        );
        fs::write(&path, "poster_decode_threads = 5\n").unwrap();
        let outcome = Config::load_with_diagnostics_from_path(path, false);
        assert_eq!(outcome.config.poster_decode_threads, None);
        assert!(outcome
            .diagnostic
            .unwrap()
            .contains("poster_decode_threads"));
    }

    #[test]
    fn p54_catalog_scan_override_roundtrips_and_invalid_values_keep_auto_policy() {
        let fixture = TestDirectory::new("catalog-scan-threads");
        let path = fixture.config_path();
        for threads in [1, 2, 8] {
            fs::write(&path, format!("catalog_scan_threads = {threads}\n")).unwrap();
            let config = Config::load_from_path(path.clone(), false);
            assert_eq!(config.catalog_scan_threads, Some(threads));
            config.save_to_path(&path).unwrap();
            assert_eq!(
                Config::load_from_path(path.clone(), false).catalog_scan_threads,
                Some(threads)
            );
        }
        for value in ["0", "9", "-1", "1.5", "'2'"] {
            fs::write(&path, format!("catalog_scan_threads = {value}\n")).unwrap();
            let outcome = Config::load_with_diagnostics_from_path(path.clone(), false);
            assert_eq!(outcome.config.catalog_scan_threads, None);
            assert!(outcome.diagnostic.unwrap().contains("catalog_scan_threads"));
        }
    }

    #[test]
    fn staged_config_save_publishes_and_preserves_a_valid_backup() {
        let fixture = TestDirectory::new("config-staged-save");
        let path = fixture.config_path();
        let backup = path.with_extension("toml.bak");

        let mut first = Config::default();
        first.theme.preset = "nord".to_string();
        let initial = first.save_to_path(&path).expect("initial config save");
        assert_eq!(initial.backup, ConfigBackupStatus::Unavailable);
        assert!(!backup.exists());

        let mut replacement = first.clone();
        replacement.theme.preset = "rose-pine".to_string();
        let outcome = replacement
            .save_to_path(&path)
            .expect("replacement config save");
        assert_eq!(outcome.backup, ConfigBackupStatus::Published);
        assert!(matches!(
            inspect_config_file(&path),
            ConfigFileInspection::Valid(_)
        ));
        assert!(matches!(
            inspect_config_file(&backup),
            ConfigFileInspection::Valid(_)
        ));

        let loaded = Config::load_from_path(path, false);
        assert_eq!(loaded.theme.preset, "rose-pine");
        let recovered = Config::load_from_path(backup, false);
        assert_eq!(recovered.theme.preset, "nord");
    }

    #[test]
    fn config_load_reports_backup_recovery_without_replacing_files() {
        let fixture = TestDirectory::new("config-recovery");
        let path = fixture.config_path();
        let backup = path.with_extension("toml.bak");
        let mut saved = Config::default();
        saved.theme.preset = "nord".to_string();
        fs::write(
            &backup,
            toml::to_string_pretty(&saved).expect("serialize backup config"),
        )
        .expect("write valid backup");
        fs::write(&path, "[style\naccent_color = \"#bad\"").expect("write corrupt primary");

        let outcome = Config::load_with_diagnostics_from_path(path.clone(), false);
        assert_eq!(outcome.source, ConfigLoadSource::BackupRecovery);
        assert_eq!(outcome.config.theme.preset, "nord");
        assert!(outcome
            .diagnostic
            .as_deref()
            .is_some_and(|message| message.contains("Recovered configuration from the backup")));
        assert_eq!(
            fs::read_to_string(&path).expect("primary remains for diagnosis"),
            "[style\naccent_color = \"#bad\""
        );
        assert!(matches!(
            inspect_config_file(&backup),
            ConfigFileInspection::Valid(_)
        ));
    }

    #[test]
    fn config_load_surfaces_semantic_section_warnings_for_valid_toml() {
        let fixture = TestDirectory::new("config-semantic-warning");
        let path = fixture.config_path();
        fs::write(
            &path,
            "[style]\ncard_width = \"not-a-number\"\n[behavior]\nshow_excluded = false\n",
        )
        .expect("write syntactically valid config");

        let outcome = Config::load_with_diagnostics_from_path(path, false);
        assert_eq!(outcome.source, ConfigLoadSource::Primary);
        assert!(outcome
            .diagnostic
            .as_deref()
            .is_some_and(|message| message.contains("[style] section")));
        assert!(!outcome.config.behavior.show_excluded);
    }

    #[test]
    fn failed_config_publication_reports_tracked_staged_and_backup_state() {
        let fixture = TestDirectory::new("config-publication-failure");
        let path = fixture.config_path();
        let mut current = Config::default();
        current.theme.preset = "nord".to_string();
        current.save_to_path(&path).expect("seed config");

        let mut replacement = current.clone();
        replacement.theme.preset = "rose-pine".to_string();
        let mut replacer = |staged: &Path, destination: &Path, recovery: &Path| {
            if destination == path.as_path() {
                fs::remove_file(destination).expect("simulate partial replacement");
                return Err(io::Error::from_raw_os_error(1176));
            }
            file_publication::replace_existing_file(staged, destination, recovery)
        };
        let error = replacement
            .save_to_path_with(&path, &mut replacer)
            .expect_err("publication failure should be reported");

        assert_eq!(error.stage, ConfigSaveStage::PublishPrimary);
        assert_eq!(error.primary, ConfigFileStatus::Missing);
        assert_eq!(error.backup, ConfigBackupStatus::Preserved);
        assert!(error
            .to_string()
            .contains("ERROR_UNABLE_TO_MOVE_REPLACEMENT"));
        let staged = error
            .recovery_paths()
            .iter()
            .find(|candidate| {
                candidate
                    .file_name()
                    .is_some_and(|name| name.to_string_lossy().contains("config-tmp"))
            })
            .expect("validated staged config is retained");
        assert!(matches!(
            inspect_config_file(staged),
            ConfigFileInspection::Valid(_)
        ));
        assert!(matches!(
            inspect_config_file(&path.with_extension("toml.bak")),
            ConfigFileInspection::Valid(_)
        ));
    }

    #[test]
    fn test_flexible_ui_parsing() {
        let toml_str = r#"
        [ui]
        show_top_bar = false
        show_hint_bar = "false"
        tint_excluded_cards = false
        dim_unselected = 0.5
        backdrop_opacity = 0.45
        card_width = 320
        "#;

        let val: toml::Value = toml::from_str(toml_str).unwrap();
        let ui_val = val.get("ui").unwrap();
        let ui: UiConfig = ui_val.clone().try_into().unwrap();

        assert_eq!(ui.show_top_bar, false);
        assert_eq!(ui.show_hint_bar, false);
        assert_eq!(ui.tint_excluded, false);
        assert_eq!(ui.dim_unselected, true);
        assert_eq!(ui.backdrop_opacity, Some(0.45));
        assert_eq!(ui.card_width, Some(320));
    }

    #[test]
    fn test_flexible_ui_bool_variants() {
        let toml_str = r#"
        [ui]
        show_top_bar = 0
        show_hint_bar = "off"
        show_title = "no"
        show_close_button = "disabled"
        show_hidden_button = 1
        "#;

        let val: toml::Value = toml::from_str(toml_str).unwrap();
        let ui_val = val.get("ui").unwrap();
        let ui: UiConfig = ui_val.clone().try_into().unwrap();

        assert_eq!(ui.show_top_bar, false);
        assert_eq!(ui.show_hint_bar, false);
        assert_eq!(ui.show_title, false);
        assert_eq!(ui.show_close_button, false);
        assert_eq!(ui.show_hidden_button, true);
    }

    #[test]
    fn test_two_layer_hidden_and_disabled() {
        let toml_str = r#"
        [excluded_wallpapers]
        Monitor0 = ["nsfw_wallpaper_1", "clutter_wp"]

        [disabled_wallpapers]
        Monitor0 = ["portrait_wallpaper_on_landscape"]
        global = ["globally_broken_wallpaper"]
        "#;

        let val: toml::Value = toml::from_str(toml_str).unwrap();
        let mut cfg = Config::default();
        if let Some(ex_val) = val.get("excluded_wallpapers") {
            if let Ok(user_ex) = ex_val.clone().try_into() {
                cfg.excluded_wallpapers = user_ex;
            }
        }
        if let Some(dis_val) = val.get("disabled_wallpapers") {
            if let Ok(user_dis) = dis_val.clone().try_into() {
                cfg.disabled_wallpapers = user_dis;
            }
        }

        // Layer 1: Soft-hidden (toggled via 'x', shown with 'h')
        assert!(cfg.is_excluded("Monitor0", "nsfw_wallpaper_1"));
        assert!(!cfg.is_excluded("Monitor1", "nsfw_wallpaper_1"));

        // Layer 2: Hard-disabled (never shown, config-only)
        assert!(cfg.is_disabled("Monitor0", "portrait_wallpaper_on_landscape"));
        assert!(!cfg.is_disabled("Monitor1", "portrait_wallpaper_on_landscape"));
        // Global disabled matches on any monitor
        assert!(cfg.is_disabled("Monitor0", "globally_broken_wallpaper"));
        assert!(cfg.is_disabled("Monitor1", "globally_broken_wallpaper"));
    }

    #[test]
    fn test_hint_bar_items_config() {
        let toml_str = r#"
        [ui]
        hint_bar_items = ["scroll", "apply", "explorer:Folder", "reload", "close"]
        "#;

        let val: toml::Value = toml::from_str(toml_str).unwrap();
        let ui_val = val.get("ui").unwrap();
        let ui: UiConfig = ui_val.clone().try_into().unwrap();

        assert_eq!(ui.hint_bar_items.len(), 5);
        assert_eq!(ui.hint_bar_items[0], "scroll");
        assert_eq!(ui.hint_bar_items[1], "apply");
        assert_eq!(ui.hint_bar_items[2], "explorer:Folder");
        assert_eq!(ui.hint_bar_items[3], "reload");
        assert_eq!(ui.hint_bar_items[4], "close");
    }

    #[test]
    fn test_catalog_local_folders_parse_as_paths() {
        let toml_str = r#"
        [catalog]
        local_folders = ["C:/Wallpapers", "D:/Pictures/Desktop"]
        "#;

        let val: toml::Value = toml::from_str(toml_str).unwrap();
        let catalog: CatalogConfig = val.get("catalog").unwrap().clone().try_into().unwrap();

        assert_eq!(catalog.local_folders.len(), 2);
        assert_eq!(catalog.local_folders[0], PathBuf::from("C:/Wallpapers"));
    }

    #[test]
    fn slideshow_defaults_off_at_thirty_minutes_on_primary() {
        let slideshow = SlideshowConfig::default();
        assert!(!slideshow.enabled);
        assert_eq!(slideshow.interval_minutes, 30);
        assert_eq!(slideshow.target_monitor, "primary");
    }

    #[test]
    fn explicit_runtime_directory_is_used_only_when_non_empty() {
        let expected = PathBuf::from(r"J:\isolated\wallpaper-switcher-candidate");

        assert_eq!(
            configured_runtime_dir(Some(expected.clone().into_os_string())),
            Some(expected)
        );
        assert_eq!(configured_runtime_dir(Some("".into())), None);
        assert_eq!(configured_runtime_dir(None), None);
    }

    #[test]
    fn read_only_load_uses_the_supplied_runtime_directory_without_creating_defaults() {
        let root = std::env::temp_dir().join(format!(
            "wallpaper-switcher-config-load-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("config.toml");
        fs::write(
            &path,
            r#"
[catalog]
local_folders = ["F:/PICTURE"]

[behavior]
include_default_projects = false

[slideshow]
enabled = false
"#,
        )
        .unwrap();

        let config = Config::load_from_path(path, false);

        assert_eq!(
            config.catalog.local_folders,
            vec![PathBuf::from("F:/PICTURE")]
        );
        assert!(!config.behavior.include_default_projects);
        assert!(!config.slideshow.enabled);
        assert!(root.join("candidate-state.toml").metadata().is_err());

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn invalid_transition_effect_is_reported_without_discarding_other_settings() {
        let mut cfg = Config::default();
        cfg.theme.preset = "nord".to_string();
        cfg.catalog.local_folders = vec![PathBuf::from(r"F:\PICTURE")];
        let transition: toml::Value = toml::from_str(
            r#"
            effect = "not-a-real-effect"
            duration_ms = 1200
            "#,
        )
        .unwrap();
        let mut diagnostics = Vec::new();

        apply_transition_section(&mut cfg, &transition, &mut |message| {
            diagnostics.push(message)
        });

        assert_eq!(cfg.transition.effect, TransitionEffect::Crossfade);
        assert_eq!(cfg.transition.duration_ms, 1200);
        assert_eq!(cfg.theme.preset, "nord");
        assert_eq!(
            cfg.catalog.local_folders,
            vec![PathBuf::from(r"F:\PICTURE")]
        );
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("unknown transition effect"));
        assert!(diagnostics[0].contains("not-a-real-effect"));
        assert!(diagnostics[0].contains("keeping \"crossfade\""));
    }

    #[test]
    fn removed_sand_effect_migrates_with_a_clear_notice() {
        let mut cfg = Config::default();
        let transition: toml::Value = toml::from_str(
            r#"
            effect = "Sand dissolve"
            duration_ms = 800
            "#,
        )
        .unwrap();
        let mut diagnostics = Vec::new();

        apply_transition_section(&mut cfg, &transition, &mut |message| {
            diagnostics.push(message)
        });

        assert_eq!(cfg.transition.effect, TransitionEffect::Crossfade);
        assert_eq!(cfg.transition.duration_ms, 800);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].contains("Migrated removed"));
        assert!(diagnostics[0].contains("Sand dissolve"));
        assert!(diagnostics[0].contains("crossfade"));
    }
}

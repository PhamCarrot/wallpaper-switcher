//! Saved local presentation and desired policy. Unset members retain the
//! existing source default; this data never dispatches provider/startup work.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FrontendPreferences {
    #[serde(flatten)]
    pub retained: super::local_library::Retained,
    pub library: LibraryPreferences,
    pub interface: InterfacePreferences,
    pub picker: PickerPreferences,
    pub source_browsers: SourceBrowserPreferences,
    pub source_motion: SourceMotionPreferences,
    pub launch: LaunchPreferences,
    pub tag_panel: TagPanelPreferences,
    pub filter_bar: FilterBarPreferences,
    pub visibility: VisibilityPreferences,
    pub desired_policy: DesiredPolicyPreferences,
    pub bindings: BTreeMap<BindingAction, String>,
    pub resolution_presets: Vec<ResolutionPreset>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct InterfacePreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ui_scale: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_fps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fast_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub standard_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slow_ms: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PickerPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub apply_on_picker_monitor: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close_on_selection: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hover_selects: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub start_position: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browse_position: Option<BrowsePosition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    // Legacy compatibility only: card badges and their authoring route are retired.
    pub show_type_badges: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub desired_monitor_id: Option<String>,
}

/// Source lastBrowsePosition: IDs, a layout signature and content camera, never provider state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BrowsePosition {
    pub selection: String,
    pub anchor: String,
    pub layout: String,
    pub camera: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LaunchPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub animation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fade_from: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TagPanelPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset_x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset_y: Option<f64>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FilterBarPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub visual_style: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orientation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_folder: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sticky: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset_x: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset_y: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_visible: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VisibilityPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mute: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub search: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub settings: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close: Option<bool>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_all: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_static: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_video: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_we: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_date: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_recent: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_color: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_pop: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_richness: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_minimalist: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_res: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub folder: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub favourites: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub random: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub colors: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub theme: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tagcloud: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub orient: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub playlists: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub download: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolution: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DesiredPolicyPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notify_on_change: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub restore_on_startup: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub random_include_static: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub random_include_video: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub random_include_we: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub random_include_favourites: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BindingAction {
    Select,
    Apply,
    RandomApply,
    Flip,
    Reveal,
    Favourite,
    Effects,
    /// Retired Effects Studio binding. Serialized for compatibility only.
    Studio,
    SceneProperties,
    Playlists,
    Settings,
    Help,
    ThemePanel,
    TagCloud,
    TagMode,
    FilterBar,
    FolderPrev,
    FolderNext,
    FolderToggle,
    HiddenFolders,
    ColorPrev,
    ColorNext,
    NavLeft,
    NavRight,
    NavUp,
    NavDown,
    PageBack,
    PageForward,
    JumpFirst,
    JumpLast,
    ModePrev,
    ModeNext,
    PresetNext,
    PresetPrev,
    Autocomplete,
    TypePrev,
    TypeNext,
    SortPrev,
    SortNext,
    RandomRotate,
    SearchMode,
    Downloads,
    SourceWallhaven,
    SourceSteam,
    SourceUnsplash,
    SourcePexels,
    SourceYoutube,
    SourceBing,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Orientation {
    Wide,
    Tall,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResolutionPreset {
    pub label: String,
    pub orientation: Orientation,
    pub from: (i64, i64),
    pub to: Option<(i64, i64)>,
    #[serde(flatten)]
    pub retained: super::local_library::Retained,
}
impl ResolutionPreset {
    pub fn matches_orientation(&self, orientation: super::OrientationFilter) -> bool {
        match orientation {
            super::OrientationFilter::Landscape => self.orientation == Orientation::Wide,
            super::OrientationFilter::Portrait => self.orientation == Orientation::Tall,
            _ => true,
        }
    }

    fn oriented(&self, pair: (i64, i64)) -> (i64, i64) {
        match self.orientation {
            Orientation::Wide => (pair.0.max(pair.1), pair.0.min(pair.1)),
            Orientation::Tall => (pair.0.min(pair.1), pair.0.max(pair.1)),
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        super::local_library::validate_retained(&self.retained)?;
        if !self.valid() {
            return Err("Invalid local resolution range.".into());
        }
        Ok(())
    }

    pub fn valid(&self) -> bool {
        let positive = |(w, h): (i64, i64)| w > 0 && h > 0;
        positive(self.from)
            && self.to.is_none_or(|to| {
                let from = self.oriented(self.from);
                let upper = self.oriented(to);
                positive(to) && from.0 <= upper.0 && from.1 <= upper.1
            })
    }

    /// Predicate identity excludes labels and inert future fields.
    pub fn range_key(&self) -> (bool, (i64, i64), Option<(i64, i64)>) {
        (
            self.orientation == Orientation::Tall,
            self.oriented(self.from),
            self.to.map(|p| self.oriented(p)),
        )
    }

    pub fn same_range(&self, other: &Self) -> bool {
        self.range_key() == other.range_key()
    }

    pub fn matches_dimensions(&self, width: u32, height: u32) -> bool {
        if !self.valid() || width == 0 || height == 0 {
            return false;
        }
        let shape = match self.orientation {
            Orientation::Wide => width >= height,
            Orientation::Tall => height > width,
        };
        let pair = self.oriented((i64::from(width), i64::from(height)));
        let lower = self.oriented(self.from);
        shape
            && pair.0 >= lower.0
            && pair.1 >= lower.1
            && self.to.is_none_or(|to| {
                let upper = self.oriented(to);
                pair.0 <= upper.0 && pair.1 <= upper.1
            })
    }

    pub fn summary(&self) -> String {
        let (w, h) = self.oriented(self.from);
        let range = self.to.map_or_else(
            || format!("{w}×{h}+"),
            |to| {
                let (tw, th) = self.oriented(to);
                format!("{w}×{h}–{tw}×{th}")
            },
        );
        let orientation = match self.orientation {
            Orientation::Wide => "wide",
            Orientation::Tall => "tall",
        };
        format!("{} · {orientation} · {range}", self.label)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserGridPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub columns: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rows: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_width: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thumb_height: Option<f64>,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceBrowserPreferences {
    pub wallhaven: BrowserGridPreferences,
    pub steam: BrowserGridPreferences,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub steam_visible: Option<bool>,
}

/// Retired source-form values retained for validated inert round-trip.
/// Config.transition remains the independent native execution owner.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceMotionPreferences {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shader: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview_fps: Option<f64>,
    #[serde(flatten)]
    pub retained: super::local_library::Retained,
}

/// W4 execution settings; legacy pending watch intent is deliberately separate.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LibraryPreferences {
    pub automatic_watch: Option<bool>,
    pub polling_seconds: Option<u32>,
    #[serde(flatten)]
    pub retained: super::local_library::Retained,
}
impl LibraryPreferences {
    pub fn watching(&self) -> bool {
        self.automatic_watch.unwrap_or(true)
    }
    pub fn interval(&self) -> u32 {
        self.polling_seconds.unwrap_or(60).clamp(1, 3600)
    }
    pub fn validate(&self) -> Result<(), String> {
        super::local_library::validate_retained(&self.retained)?;
        if self
            .polling_seconds
            .is_some_and(|s| !(1..=3600).contains(&s))
        {
            return Err("Library fallback interval must be 1–3600 seconds.".into());
        }
        Ok(())
    }
}

impl FrontendPreferences {
    pub fn validate(&self) -> Result<(), String> {
        super::local_library::validate_retained(&self.retained)?;
        super::local_library::validate_retained(&self.source_motion.retained)?;
        self.library.validate()?;
        for (path, value) in [
            ("transition.durationMs", self.source_motion.duration_ms),
            ("transition.fps", self.source_motion.fps),
            ("transition.previewFps", self.source_motion.preview_fps),
        ] {
            if value.is_some_and(|value| !value.is_finite()) {
                return Err(format!(
                    "Non-finite desired source transition value: {path}"
                ));
            }
        }
        // Source schema stores finite numeric values; integer browser getters
        // do not justify narrowing or clamping the saved local preference.
        for (path, value) in [
            (
                "components.wallpaperSelector.wallhavenColumns",
                self.source_browsers.wallhaven.columns,
            ),
            (
                "components.wallpaperSelector.wallhavenRows",
                self.source_browsers.wallhaven.rows,
            ),
            (
                "components.wallpaperSelector.wallhavenThumbWidth",
                self.source_browsers.wallhaven.thumb_width,
            ),
            (
                "components.wallpaperSelector.wallhavenThumbHeight",
                self.source_browsers.wallhaven.thumb_height,
            ),
            (
                "components.wallpaperSelector.steamColumns",
                self.source_browsers.steam.columns,
            ),
            (
                "components.wallpaperSelector.steamRows",
                self.source_browsers.steam.rows,
            ),
            (
                "components.wallpaperSelector.steamThumbWidth",
                self.source_browsers.steam.thumb_width,
            ),
            (
                "components.wallpaperSelector.steamThumbHeight",
                self.source_browsers.steam.thumb_height,
            ),
        ] {
            if value.is_some_and(|value| !value.is_finite()) {
                return Err(format!("Non-finite source browser preference: {path}"));
            }
        }
        if self
            .interface
            .ui_scale
            .is_some_and(|value| !value.is_finite())
        {
            return Err("Non-finite local preference: general.uiScale".into());
        }
        if self
            .interface
            .max_fps
            .is_some_and(|value| !value.is_finite())
        {
            return Err("Non-finite local preference: general.maxFps".into());
        }
        if self
            .interface
            .fast_ms
            .is_some_and(|value| !value.is_finite())
        {
            return Err("Non-finite local preference: motion.fastMs".into());
        }
        if self
            .interface
            .standard_ms
            .is_some_and(|value| !value.is_finite())
        {
            return Err("Non-finite local preference: motion.standardMs".into());
        }
        if self
            .interface
            .slow_ms
            .is_some_and(|value| !value.is_finite())
        {
            return Err("Non-finite local preference: motion.slowMs".into());
        }
        if self
            .launch
            .fade_from
            .is_some_and(|value| !value.is_finite())
        {
            return Err("Non-finite local preference: general.openFadeFrom".into());
        }
        if self
            .picker
            .browse_position
            .as_ref()
            .is_some_and(|p| !p.camera.is_finite())
        {
            return Err("Non-finite local preference: picker.browse_position.camera".into());
        }
        if self.tag_panel.width.is_some_and(|value| !value.is_finite()) {
            return Err(
                "Non-finite local preference: components.wallpaperSelector.tagCloudWidth".into(),
            );
        }
        if self
            .tag_panel
            .offset_x
            .is_some_and(|value| !value.is_finite())
        {
            return Err(
                "Non-finite local preference: components.wallpaperSelector.tagCloudOffsetX".into(),
            );
        }
        if self
            .tag_panel
            .offset_y
            .is_some_and(|value| !value.is_finite())
        {
            return Err(
                "Non-finite local preference: components.wallpaperSelector.tagCloudOffsetY".into(),
            );
        }
        if self
            .filter_bar
            .offset_x
            .is_some_and(|value| !value.is_finite())
        {
            return Err("Non-finite local preference: filterBar.offsetX".into());
        }
        if self
            .filter_bar
            .offset_y
            .is_some_and(|value| !value.is_finite())
        {
            return Err("Non-finite local preference: filterBar.offsetY".into());
        }
        if self
            .interface
            .ui_scale
            .is_some_and(|value| !(0.5..=2.0).contains(&value))
        {
            return Err("Local preference out of range: interface.ui_scale".into());
        }
        if self
            .interface
            .max_fps
            .is_some_and(|value| !(1.0..=1000.0).contains(&value))
        {
            return Err("Local preference out of range: interface.max_fps".into());
        }
        if self
            .interface
            .fast_ms
            .is_some_and(|value| !(35.0..=2000.0).contains(&value))
        {
            return Err("Local preference out of range: interface.fast_ms".into());
        }
        if self
            .interface
            .standard_ms
            .is_some_and(|value| !(35.0..=2000.0).contains(&value))
        {
            return Err("Local preference out of range: interface.standard_ms".into());
        }
        if self
            .interface
            .slow_ms
            .is_some_and(|value| !(35.0..=4000.0).contains(&value))
        {
            return Err("Local preference out of range: interface.slow_ms".into());
        }
        if self
            .launch
            .fade_from
            .is_some_and(|value| !(0.0..=100.0).contains(&value))
        {
            return Err("Local preference out of range: launch.fade_from".into());
        }
        if self.interface.language.as_deref().is_some_and(|value| {
            ![
                "auto", "en-US", "sv-SE", "es-ES", "pt-BR", "ru-RU", "zh-CN", "ja-JP", "ar-SA",
                "fr-FR", "bn-BD", "ur-PK", "hi-IN", "fa-IR", "tr-TR",
            ]
            .contains(&value)
        }) {
            return Err("Unknown local preference choice: interface.language".into());
        }
        if self
            .picker
            .start_position
            .as_deref()
            .is_some_and(|value| !["beginning", "applied", "browsing"].contains(&value))
        {
            return Err("Unknown local preference choice: picker.start_position".into());
        }
        if self
            .launch
            .animation
            .as_deref()
            .is_some_and(|value| !["none", "fade", "rise", "zoom"].contains(&value))
        {
            return Err("Unknown local preference choice: launch.animation".into());
        }
        if self
            .launch
            .speed
            .as_deref()
            .is_some_and(|value| !["fast", "standard", "slow"].contains(&value))
        {
            return Err("Unknown local preference choice: launch.speed".into());
        }
        if self
            .tag_panel
            .rows
            .as_deref()
            .is_some_and(|value| !["1", "2", "3"].contains(&value))
        {
            return Err("Unknown local preference choice: tag_panel.rows".into());
        }
        if self
            .filter_bar
            .visual_style
            .as_deref()
            .is_some_and(|value| !["match", "slices", "hex", "wall"].contains(&value))
        {
            return Err("Unknown local preference choice: filter_bar.visual_style".into());
        }
        if self
            .filter_bar
            .orientation
            .as_deref()
            .is_some_and(|value| !["horizontal", "vertical"].contains(&value))
        {
            return Err("Unknown local preference choice: filter_bar.orientation".into());
        }
        for binding in self.bindings.values() {
            if !binding.is_empty() && super::binding_codec::parse_binding(binding).is_none() {
                return Err("Invalid saved input binding.".into());
            }
        }
        for preset in &self.resolution_presets {
            preset.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod w4_tests {
    use super::*;
    #[test]
    fn w4_library_defaults_unknown_roundtrip_and_interval_validation() {
        let prefs: FrontendPreferences =
            toml::from_str("[library]\nfuture_switch = { value = 'inert' }\n").unwrap();
        assert!(prefs.library.watching());
        assert_eq!(prefs.library.interval(), 60);
        prefs.validate().unwrap();
        let roundtrip: FrontendPreferences =
            toml::from_str(&toml::to_string(&prefs).unwrap()).unwrap();
        assert_eq!(roundtrip, prefs);
        let mut bad = prefs;
        bad.library.polling_seconds = Some(0);
        assert!(bad.validate().is_err());
        bad.library.polling_seconds = Some(3601);
        assert!(bad.validate().is_err());
    }
}

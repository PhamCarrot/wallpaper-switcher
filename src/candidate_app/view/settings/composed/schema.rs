//! Narrow source presentation data; no config IO, daemon, mutation, or worker.
use super::keys;
// Exact source power defaults; this module has no power policy consumer.
const DEFAULT_BATTERY_FPS: u32 = 60;
const DEFAULT_BATTERY_VIDEO_IDLE_SECONDS: u32 = 120;
use std::marker::PhantomData;

use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    Boolean,
    Number,
    Text,
    Array,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DefaultValue {
    Boolean(bool),
    Number(f64),
    ResponsiveNumber { regular: f64, compact: f64 },
    Text(&'static str),
    EmptyArray,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumberBounds {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SettingSpec {
    pub path: &'static str,
    pub kind: ValueKind,
    pub default: DefaultValue,
    pub number_bounds: Option<NumberBounds>,
}

impl SettingSpec {
    pub const fn boolean(path: &'static str, default: bool) -> Self {
        Self {
            path,
            kind: ValueKind::Boolean,
            default: DefaultValue::Boolean(default),
            number_bounds: None,
        }
    }

    pub const fn number(path: &'static str, default: f64) -> Self {
        Self {
            path,
            kind: ValueKind::Number,
            default: DefaultValue::Number(default),
            number_bounds: None,
        }
    }

    pub const fn bounded_number(path: &'static str, default: f64, min: f64, max: f64) -> Self {
        Self {
            path,
            kind: ValueKind::Number,
            default: DefaultValue::Number(default),
            number_bounds: Some(NumberBounds { min, max }),
        }
    }

    pub const fn responsive_number(path: &'static str, regular: f64, compact: f64) -> Self {
        Self {
            path,
            kind: ValueKind::Number,
            default: DefaultValue::ResponsiveNumber { regular, compact },
            number_bounds: None,
        }
    }

    pub const fn text(path: &'static str, default: &'static str) -> Self {
        Self {
            path,
            kind: ValueKind::Text,
            default: DefaultValue::Text(default),
            number_bounds: None,
        }
    }

    pub const fn array(path: &'static str) -> Self {
        Self {
            path,
            kind: ValueKind::Array,
            default: DefaultValue::EmptyArray,
            number_bounds: None,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Setting<T> {
    spec: SettingSpec,
    marker: PhantomData<fn() -> T>,
}

impl<T> Clone for Setting<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Setting<T> {}

impl<T> Setting<T> {
    pub const fn path(self) -> &'static str {
        self.spec.path
    }

    pub const fn spec(self) -> SettingSpec {
        self.spec
    }
}

impl Setting<bool> {
    pub const fn boolean(path: &'static str, default: bool) -> Self {
        Self {
            spec: SettingSpec::boolean(path, default),
            marker: PhantomData,
        }
    }

    pub const fn default(self) -> bool {
        let DefaultValue::Boolean(default) = self.spec.default else {
            unreachable!()
        };
        default
    }
}

impl Setting<f64> {
    pub const fn number(path: &'static str, default: f64) -> Self {
        Self {
            spec: SettingSpec::number(path, default),
            marker: PhantomData,
        }
    }

    pub const fn bounded_number(path: &'static str, default: f64, min: f64, max: f64) -> Self {
        Self {
            spec: SettingSpec::bounded_number(path, default, min, max),
            marker: PhantomData,
        }
    }

    pub const fn responsive_number(path: &'static str, regular: f64, compact: f64) -> Self {
        Self {
            spec: SettingSpec::responsive_number(path, regular, compact),
            marker: PhantomData,
        }
    }

    pub const fn default(self) -> f64 {
        match self.spec.default {
            DefaultValue::Number(default) => default,
            DefaultValue::ResponsiveNumber { regular, .. } => regular,
            _ => unreachable!(),
        }
    }
}

impl Setting<String> {
    pub const fn text(path: &'static str, default: &'static str) -> Self {
        Self {
            spec: SettingSpec::text(path, default),
            marker: PhantomData,
        }
    }

    pub const fn default(self) -> &'static str {
        let DefaultValue::Text(default) = self.spec.default else {
            unreachable!()
        };
        default
    }
}

pub mod setting {
    use super::keys;
    use super::Setting;

    pub mod filter_bar {
        use super::{keys, Setting};

        pub const DEFAULT_FOLDER: Setting<String> =
            Setting::text(keys::filter_bar::DEFAULT_FOLDER, "*");
        pub const OFFSET_X: Setting<f64> = Setting::number(keys::filter_bar::OFFSET_X, 0.0);
        pub const OFFSET_Y: Setting<f64> = Setting::number(keys::filter_bar::OFFSET_Y, 0.0);
        pub const ORIENTATION: Setting<String> =
            Setting::text(keys::filter_bar::ORIENTATION, "horizontal");
        pub const LAST_SHOW_HIDDEN_FOLDERS: Setting<bool> =
            Setting::boolean(keys::filter_bar::LAST_SHOW_HIDDEN_FOLDERS, true);
        pub const STICKY: Setting<bool> = Setting::boolean(keys::filter_bar::STICKY, false);
        pub const VISUAL_STYLE: Setting<String> =
            Setting::text(keys::filter_bar::VISUAL_STYLE, "match");
    }

    pub mod general {
        use super::{keys, Setting};

        pub const APPLY_ON_PICKER_MONITOR: Setting<bool> =
            Setting::boolean(keys::general::APPLY_ON_PICKER_MONITOR, false);
        pub const CLOSE_ON_SELECTION: Setting<bool> =
            Setting::boolean(keys::general::CLOSE_ON_SELECTION, false);
        pub const FILTER_BAR_ALWAYS_VISIBLE: Setting<bool> =
            Setting::boolean(keys::general::FILTER_BAR_ALWAYS_VISIBLE, true);
        pub const HOVER_SELECTS: Setting<bool> =
            Setting::boolean(keys::general::HOVER_SELECTS, true);
        pub const MAX_FPS: Setting<f64> =
            Setting::bounded_number(keys::general::MAX_FPS, 120.0, 1.0, 1_000.0);
        pub const OPEN_FADE_FROM: Setting<f64> =
            Setting::bounded_number(keys::general::OPEN_FADE_FROM, 0.0, 0.0, 100.0);
        pub const RANDOM_INTERVAL: Setting<f64> =
            Setting::number(keys::general::RANDOM_INTERVAL, 300.0);
        pub const SETTINGS_STYLE: Setting<String> =
            Setting::text(keys::general::SETTINGS_STYLE, "editorial");
        pub const UI_SCALE: Setting<f64> =
            Setting::bounded_number(keys::general::UI_SCALE, 1.0, 0.5, 2.0);
    }

    pub mod motion {
        use super::{keys, Setting};

        pub const FAST_MS: Setting<f64> =
            Setting::bounded_number(keys::motion::FAST_MS, 180.0, 35.0, 2_000.0);
        pub const STANDARD_MS: Setting<f64> =
            Setting::bounded_number(keys::motion::STANDARD_MS, 250.0, 35.0, 2_000.0);
        pub const SLOW_MS: Setting<f64> =
            Setting::bounded_number(keys::motion::SLOW_MS, 450.0, 35.0, 4_000.0);
    }

    pub mod selector {
        pub const DEPTH_SHADOWS: Setting<bool> =
            Setting::boolean(keys::selector::DEPTH_SHADOWS, false);
        pub const SLICE_SHADOWS: Setting<bool> =
            Setting::boolean(keys::selector::SLICE_SHADOWS, true);
        pub const DEPTH_SELECTION_FRAME: Setting<bool> =
            Setting::boolean(keys::selector::DEPTH_SELECTION_FRAME, false);
        pub const DEPTH_HEIGHT: Setting<f64> =
            Setting::bounded_number(keys::selector::DEPTH_HEIGHT, 520.0, 100.0, 1600.0);
        pub const DEPTH_WIDTH_PX: Setting<f64> =
            Setting::bounded_number(keys::selector::DEPTH_WIDTH_PX, 280.0, 40.0, 2400.0);
        pub const DEPTH_SPACING_PX: Setting<f64> =
            Setting::bounded_number(keys::selector::DEPTH_SPACING_PX, 280.0, 0.0, 4000.0);
        pub const DEPTH_COUNT: Setting<f64> =
            Setting::bounded_number(keys::selector::DEPTH_COUNT, 5.0, 3.0, 21.0);
        pub const DEPTH_FALLOFF_FACTOR: Setting<f64> =
            Setting::bounded_number(keys::selector::DEPTH_FALLOFF_FACTOR, 0.05, 0.0, 1.0);
        pub const DEPTH_CORNERS: Setting<f64> =
            Setting::bounded_number(keys::selector::DEPTH_CORNERS, 0.0, 0.0, 100.0);
        pub const DEPTH_SKEW: Setting<f64> =
            Setting::bounded_number(keys::selector::DEPTH_SKEW, 0.0, -200.0, 200.0);
        pub const DEPTH_NAVIGATION_MS: Setting<f64> =
            Setting::bounded_number(keys::selector::DEPTH_NAVIGATION_MS, 1000.0, 35.0, 8000.0);
        pub const COLLECTION_SIZE: Setting<f64> =
            Setting::bounded_number(keys::selector::COLLECTION_SIZE, 42.0, 15.0, 65.0);
        pub const COLLECTION_SPACING: Setting<f64> =
            Setting::bounded_number(keys::selector::COLLECTION_SPACING, 17.0, 5.0, 30.0);
        pub const COLLECTION_COUNT: Setting<f64> =
            Setting::bounded_number(keys::selector::COLLECTION_COUNT, 7.0, 3.0, 11.0);
        pub const COLLECTION_TILT: Setting<f64> =
            Setting::bounded_number(keys::selector::COLLECTION_TILT, 52.0, 0.0, 75.0);
        pub const COLLECTION_CORNERS: Setting<f64> =
            Setting::bounded_number(keys::selector::COLLECTION_CORNERS, 2.0, 0.0, 100.0);
        pub const COLLECTION_SPEED: Setting<f64> =
            Setting::bounded_number(keys::selector::COLLECTION_SPEED, 100.0, 25.0, 300.0);
        use super::{keys, Setting};

        pub const DISPLAY_MODE: Setting<String> =
            Setting::text(keys::selector::DISPLAY_MODE, "slices");
        pub const FLIP_DURATION_MS: Setting<f64> =
            Setting::bounded_number(keys::selector::FLIP_DURATION_MS, 1500.0, 100.0, 5_000.0);
        pub const GRID_BORDER_WIDTH: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_BORDER_WIDTH, 2.0, 0.0, 32.0);
        pub const GRID_CYLINDER_BEND: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_CYLINDER_BEND, 65.0, -100.0, 100.0);
        pub const GRID_CYLINDER_RADIUS: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_CYLINDER_RADIUS, 720.0, 150.0, 2_400.0);
        pub const GRID_FILTER_BAR_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::GRID_FILTER_BAR_OFFSET_X, 0.0);
        pub const GRID_FILTER_BAR_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::GRID_FILTER_BAR_OFFSET_Y, 0.0);
        pub const GRID_CORNER_RADIUS: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_CORNER_RADIUS, 6.0, 0.0, 128.0);
        pub const GRID_GAP_X: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_GAP_X, 8.0, 0.0, 256.0);
        pub const GRID_GAP_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_GAP_Y, 8.0, 0.0, 256.0);
        pub const GRID_LAYOUT: Setting<String> =
            Setting::text(keys::selector::GRID_LAYOUT, "uniform");
        pub const GRID_FLOW_FREQUENCY: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_FLOW_FREQUENCY, 1.0, 0.1, 8.0);
        pub const GRID_FLOW_WAVE: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_FLOW_WAVE, 0.0, -200.0, 200.0);
        pub const GRID_ROUND_CORNERS: Setting<bool> =
            Setting::boolean(keys::selector::GRID_ROUND_CORNERS, true);
        pub const GRID_SEARCH_PANEL_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::GRID_SEARCH_PANEL_OFFSET_X, 0.0);
        pub const GRID_SEARCH_PANEL_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::GRID_SEARCH_PANEL_OFFSET_Y, 0.0);
        pub const GRID_SELECTED_SCALE: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_SELECTED_SCALE, 100.0, 100.0, 200.0);
        pub const GRID_SCALE_VARIANCE: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_SCALE_VARIANCE, 0.0, 0.0, 75.0);
        pub const GRID_SCATTER: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_SCATTER, 0.0, 0.0, 150.0);
        pub const GRID_STAGE_DEPTH_ANGLE: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_DEPTH_ANGLE, 0.0, -180.0, 180.0);
        pub const GRID_STAGE_PERSPECTIVE: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_PERSPECTIVE, 0.0, -80.0, 80.0);
        pub const GRID_STAGE_ROTATION: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_ROTATION, 0.0, -45.0, 45.0);
        pub const GRID_STAGE_SCALE: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_SCALE, 100.0, 25.0, 250.0);
        pub const GRID_STAGE_SHEAR_X: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_SHEAR_X, 0.0, -100.0, 100.0);
        pub const GRID_STAGE_SHEAR_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_SHEAR_Y, 0.0, -100.0, 100.0);
        pub const GRID_STAGE_X: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_X, 0.0, -100.0, 100.0);
        pub const GRID_STAGE_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGE_Y, 0.0, -100.0, 100.0);
        pub const GRID_STAGGER: Setting<f64> =
            Setting::bounded_number(keys::selector::GRID_STAGGER, 50.0, -100.0, 100.0);
        pub const HEX_ASPECT: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_ASPECT, 100.0, 50.0, 200.0);
        pub const HEX_CURVE: Setting<String> = Setting::text(keys::selector::HEX_CURVE, "arc");
        pub const HEX_CURVE_FREQUENCY: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_CURVE_FREQUENCY, 1.0, 0.25, 6.0);
        pub const HEX_FILTER_BAR_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::HEX_FILTER_BAR_OFFSET_X, 0.0);
        pub const HEX_FILTER_BAR_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::HEX_FILTER_BAR_OFFSET_Y, 0.0);
        pub const HEX_GAP_X: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_GAP_X, 6.0, -100.0, 256.0);
        pub const HEX_GAP_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_GAP_Y, 6.0, -100.0, 256.0);
        pub const HEX_LENS: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_LENS, 0.0, -50.0, 150.0);
        pub const HEX_LENS_RADIUS: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_LENS_RADIUS, 620.0, 100.0, 2400.0);
        pub const HEX_ORBIT: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_ORBIT, 0.0, -100.0, 100.0);
        pub const HEX_ORBIT_RADIUS: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_ORBIT_RADIUS, 620.0, 100.0, 2400.0);
        pub const HEX_SCATTER: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_SCATTER, 0.0, 0.0, 160.0);
        pub const HEX_SEARCH_PANEL_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::HEX_SEARCH_PANEL_OFFSET_X, 0.0);
        pub const HEX_SEARCH_PANEL_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::HEX_SEARCH_PANEL_OFFSET_Y, 0.0);
        pub const HEX_SHAPE: Setting<String> = Setting::text(keys::selector::HEX_SHAPE, "hexagon");
        pub const HAND_STAGE_X: Setting<f64> =
            Setting::bounded_number(keys::selector::HAND_STAGE_X, 0.0, -100.0, 100.0);
        pub const HAND_STAGE_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::HAND_STAGE_Y, 0.0, -100.0, 100.0);
        pub const HEX_STAGE_DEPTH_ANGLE: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_DEPTH_ANGLE, 0.0, -180.0, 180.0);
        pub const HEX_STAGE_PERSPECTIVE: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_PERSPECTIVE, 0.0, -80.0, 80.0);
        pub const HEX_STAGE_ROTATION: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_ROTATION, 0.0, -45.0, 45.0);
        pub const HEX_STAGE_SCALE: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_SCALE, 100.0, 25.0, 250.0);
        pub const HEX_STAGE_SHEAR_X: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_SHEAR_X, 0.0, -100.0, 100.0);
        pub const HEX_STAGE_SHEAR_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_SHEAR_Y, 0.0, -100.0, 100.0);
        pub const HEX_STAGE_X: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_X, 0.0, -100.0, 100.0);
        pub const HEX_STAGE_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGE_Y, 0.0, -100.0, 100.0);
        pub const HEX_STAGGER: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_STAGGER, 50.0, -100.0, 150.0);
        pub const HEX_TWIST: Setting<f64> =
            Setting::bounded_number(keys::selector::HEX_TWIST, 0.0, -180.0, 180.0);
        pub const SANDY_FILTER_BAR_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::SANDY_FILTER_BAR_OFFSET_X, 0.0);
        pub const SANDY_FILTER_BAR_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::SANDY_FILTER_BAR_OFFSET_Y, 0.0);
        pub const SANDY_SEARCH_PANEL_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::SANDY_SEARCH_PANEL_OFFSET_X, 0.0);
        pub const SANDY_SEARCH_PANEL_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::SANDY_SEARCH_PANEL_OFFSET_Y, 0.0);
        pub const SANDY_STAGE_X: Setting<f64> =
            Setting::bounded_number(keys::selector::SANDY_STAGE_X, 0.0, -100.0, 100.0);
        pub const SANDY_STAGE_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::SANDY_STAGE_Y, 0.0, -100.0, 100.0);
        pub const SLICE_FILTER_BAR_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::SLICE_FILTER_BAR_OFFSET_X, 0.0);
        pub const SLICE_FILTER_BAR_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::SLICE_FILTER_BAR_OFFSET_Y, 0.0);
        pub const SLICE_SEARCH_PANEL_OFFSET_X: Setting<f64> =
            Setting::number(keys::selector::SLICE_SEARCH_PANEL_OFFSET_X, 0.0);
        pub const SLICE_SEARCH_PANEL_OFFSET_Y: Setting<f64> =
            Setting::number(keys::selector::SLICE_SEARCH_PANEL_OFFSET_Y, 0.0);
        pub const SLICE_STAGE_X: Setting<f64> =
            Setting::bounded_number(keys::selector::SLICE_STAGE_X, 0.0, -100.0, 100.0);
        pub const SLICE_STAGE_Y: Setting<f64> =
            Setting::bounded_number(keys::selector::SLICE_STAGE_Y, 0.0, -100.0, 100.0);
    }

    pub mod transition {
        use super::{keys, Setting};

        pub const DURATION_MS: Setting<f64> =
            Setting::bounded_number(keys::transition::DURATION_MS, 600.0, 50.0, 10_000.0);
        pub const FPS: Setting<f64> =
            Setting::bounded_number(keys::transition::FPS, 0.0, 0.0, 1_000.0);
        pub const ENABLED: Setting<bool> = Setting::boolean(keys::transition::ENABLED, true);
        pub const PREVIEW_FPS: Setting<f64> =
            Setting::bounded_number(keys::transition::PREVIEW_FPS, 30.0, 1.0, 1_000.0);
        pub const SHADER: Setting<String> = Setting::text(keys::transition::SHADER, "random");
    }
}

const STATIC_SPECS: &[SettingSpec] = &[
    SettingSpec::bounded_number(super::keys::paper::LOAD_TIMEOUT_SECONDS, 3.0, 3.0, 60.0),
    SettingSpec::text(super::keys::selector::START_POSITION, "beginning"),
    SettingSpec::text(super::keys::selector::LAST_APPLIED_KEY, ""),
    setting::filter_bar::DEFAULT_FOLDER.spec(),
    setting::filter_bar::OFFSET_X.spec(),
    setting::filter_bar::OFFSET_Y.spec(),
    setting::filter_bar::ORIENTATION.spec(),
    setting::filter_bar::STICKY.spec(),
    setting::filter_bar::LAST_SHOW_HIDDEN_FOLDERS.spec(),
    setting::filter_bar::VISUAL_STYLE.spec(),
    setting::general::APPLY_ON_PICKER_MONITOR.spec(),
    setting::general::CLOSE_ON_SELECTION.spec(),
    setting::general::FILTER_BAR_ALWAYS_VISIBLE.spec(),
    setting::general::HOVER_SELECTS.spec(),
    setting::general::MAX_FPS.spec(),
    setting::general::OPEN_FADE_FROM.spec(),
    setting::general::RANDOM_INTERVAL.spec(),
    setting::general::SETTINGS_STYLE.spec(),
    setting::general::UI_SCALE.spec(),
    setting::motion::FAST_MS.spec(),
    setting::motion::STANDARD_MS.spec(),
    setting::motion::SLOW_MS.spec(),
    setting::selector::DEPTH_SHADOWS.spec(),
    setting::selector::SLICE_SHADOWS.spec(),
    setting::selector::DEPTH_SELECTION_FRAME.spec(),
    setting::selector::DEPTH_HEIGHT.spec(),
    setting::selector::DEPTH_WIDTH_PX.spec(),
    setting::selector::DEPTH_SPACING_PX.spec(),
    setting::selector::DEPTH_COUNT.spec(),
    setting::selector::DEPTH_FALLOFF_FACTOR.spec(),
    setting::selector::DEPTH_CORNERS.spec(),
    setting::selector::DEPTH_SKEW.spec(),
    setting::selector::DEPTH_NAVIGATION_MS.spec(),
    setting::selector::COLLECTION_SIZE.spec(),
    setting::selector::COLLECTION_SPACING.spec(),
    setting::selector::COLLECTION_COUNT.spec(),
    setting::selector::COLLECTION_TILT.spec(),
    setting::selector::COLLECTION_CORNERS.spec(),
    setting::selector::COLLECTION_SPEED.spec(),
    setting::selector::DISPLAY_MODE.spec(),
    setting::selector::FLIP_DURATION_MS.spec(),
    setting::selector::GRID_BORDER_WIDTH.spec(),
    setting::selector::GRID_CYLINDER_BEND.spec(),
    setting::selector::GRID_CYLINDER_RADIUS.spec(),
    setting::selector::GRID_FILTER_BAR_OFFSET_X.spec(),
    setting::selector::GRID_FILTER_BAR_OFFSET_Y.spec(),
    setting::selector::GRID_CORNER_RADIUS.spec(),
    setting::selector::GRID_GAP_X.spec(),
    setting::selector::GRID_GAP_Y.spec(),
    setting::selector::GRID_LAYOUT.spec(),
    setting::selector::GRID_FLOW_FREQUENCY.spec(),
    setting::selector::GRID_FLOW_WAVE.spec(),
    setting::selector::GRID_ROUND_CORNERS.spec(),
    setting::selector::GRID_SEARCH_PANEL_OFFSET_X.spec(),
    setting::selector::GRID_SEARCH_PANEL_OFFSET_Y.spec(),
    setting::selector::GRID_SELECTED_SCALE.spec(),
    setting::selector::GRID_SCALE_VARIANCE.spec(),
    setting::selector::GRID_SCATTER.spec(),
    setting::selector::GRID_STAGE_DEPTH_ANGLE.spec(),
    setting::selector::GRID_STAGE_PERSPECTIVE.spec(),
    setting::selector::GRID_STAGE_ROTATION.spec(),
    setting::selector::GRID_STAGE_SCALE.spec(),
    setting::selector::GRID_STAGE_SHEAR_X.spec(),
    setting::selector::GRID_STAGE_SHEAR_Y.spec(),
    setting::selector::GRID_STAGE_X.spec(),
    setting::selector::GRID_STAGE_Y.spec(),
    setting::selector::GRID_STAGGER.spec(),
    setting::selector::HAND_STAGE_X.spec(),
    setting::selector::HAND_STAGE_Y.spec(),
    setting::selector::HEX_ASPECT.spec(),
    setting::selector::HEX_CURVE.spec(),
    setting::selector::HEX_CURVE_FREQUENCY.spec(),
    setting::selector::HEX_FILTER_BAR_OFFSET_X.spec(),
    setting::selector::HEX_FILTER_BAR_OFFSET_Y.spec(),
    setting::selector::HEX_GAP_X.spec(),
    setting::selector::HEX_GAP_Y.spec(),
    setting::selector::HEX_LENS.spec(),
    setting::selector::HEX_LENS_RADIUS.spec(),
    setting::selector::HEX_ORBIT.spec(),
    setting::selector::HEX_ORBIT_RADIUS.spec(),
    setting::selector::HEX_SCATTER.spec(),
    setting::selector::HEX_SEARCH_PANEL_OFFSET_X.spec(),
    setting::selector::HEX_SEARCH_PANEL_OFFSET_Y.spec(),
    setting::selector::HEX_SHAPE.spec(),
    setting::selector::HEX_STAGE_DEPTH_ANGLE.spec(),
    setting::selector::HEX_STAGE_PERSPECTIVE.spec(),
    setting::selector::HEX_STAGE_ROTATION.spec(),
    setting::selector::HEX_STAGE_SCALE.spec(),
    setting::selector::HEX_STAGE_SHEAR_X.spec(),
    setting::selector::HEX_STAGE_SHEAR_Y.spec(),
    setting::selector::HEX_STAGE_X.spec(),
    setting::selector::HEX_STAGE_Y.spec(),
    setting::selector::HEX_STAGGER.spec(),
    setting::selector::HEX_TWIST.spec(),
    setting::selector::SANDY_FILTER_BAR_OFFSET_X.spec(),
    setting::selector::SANDY_FILTER_BAR_OFFSET_Y.spec(),
    setting::selector::SANDY_SEARCH_PANEL_OFFSET_X.spec(),
    setting::selector::SANDY_SEARCH_PANEL_OFFSET_Y.spec(),
    setting::selector::SANDY_STAGE_X.spec(),
    setting::selector::SANDY_STAGE_Y.spec(),
    setting::selector::SLICE_FILTER_BAR_OFFSET_X.spec(),
    setting::selector::SLICE_FILTER_BAR_OFFSET_Y.spec(),
    setting::selector::SLICE_SEARCH_PANEL_OFFSET_X.spec(),
    setting::selector::SLICE_SEARCH_PANEL_OFFSET_Y.spec(),
    setting::selector::SLICE_STAGE_X.spec(),
    setting::selector::SLICE_STAGE_Y.spec(),
    setting::transition::DURATION_MS.spec(),
    setting::transition::FPS.spec(),
    setting::transition::ENABLED.spec(),
    setting::transition::PREVIEW_FPS.spec(),
    setting::transition::SHADER.spec(),
    SettingSpec::boolean(super::keys::selector::LIVE_PREVIEW, true),
    SettingSpec::boolean(super::keys::selector::HEX_ARC, true),
    SettingSpec::boolean("filterBar.show.sort", true),
    SettingSpec::boolean("filterBar.show.mute", true),
    SettingSpec::boolean("filterBar.show.search", true),
    SettingSpec::boolean("filterBar.show.settings", true),
    SettingSpec::boolean("filterBar.show.display", true),
    SettingSpec::boolean("filterBar.show.close", true),
    SettingSpec::boolean("filterBar.show.sort.recent", true),
    SettingSpec::boolean("filterBar.show.sort.date", true),
    SettingSpec::boolean("filterBar.show.sort.color", true),
    SettingSpec::boolean("filterBar.show.sort.richness", true),
    SettingSpec::boolean("filterBar.show.sort.minimalist", true),
    SettingSpec::boolean("filterBar.show.sort.res", true),
    SettingSpec::boolean("filterBar.show.sort.pop", true),
    SettingSpec::boolean(super::keys::filter_bar::SHOW_COLORS, true),
    SettingSpec::boolean(super::keys::filter_bar::SHOW_FAVOURITES, true),
    SettingSpec::boolean(super::keys::filter_bar::SHOW_FOLDER, true),
    SettingSpec::boolean(super::keys::filter_bar::SHOW_ORIENT, true),
    SettingSpec::boolean(super::keys::filter_bar::SHOW_DOWNLOAD, true),
    SettingSpec::boolean(super::keys::filter_bar::SHOW_PLAYLISTS, true),
    SettingSpec::boolean(super::keys::filter_bar::SHOW_RANDOM, true),
    SettingSpec::boolean(super::keys::filter_bar::SHOW_RESOLUTION, true),
    SettingSpec::boolean(super::keys::filter_bar::SHOW_TAG_CLOUD, true),
    SettingSpec::boolean(super::keys::filter_bar::SHOW_TAGGING, true),
    SettingSpec::boolean(super::keys::filter_bar::SHOW_THEME, true),
    SettingSpec::boolean(super::keys::selector::SANDY_LOD_AUTO, true),
    SettingSpec::boolean(super::keys::selector::HAND_BACKDROP, true),
    SettingSpec::boolean(super::keys::selector::HAND_BOB, false),
    SettingSpec::boolean(super::keys::selector::HAND_GHOSTS, true),
    SettingSpec::boolean(super::keys::selector::HAND_REVEAL_FILL, true),
    SettingSpec::boolean(super::keys::selector::HAND_MOVE_CASCADE, true),
    SettingSpec::boolean(super::keys::selector::HAND_MOVE_CORKSCREW, true),
    SettingSpec::boolean(super::keys::selector::HAND_MOVE_RIBBON, true),
    SettingSpec::boolean(super::keys::selector::HAND_MOVE_SHUFFLE, true),
    SettingSpec::boolean(super::keys::selector::HAND_MOVE_SPIRAL, true),
    SettingSpec::boolean(super::keys::selector::FLIP_SHADER, true),
    SettingSpec::boolean(super::keys::selector::FLIP_BACK_REVEAL, true),
    SettingSpec::boolean(super::keys::noctalia::HOVER_PREVIEW, true),
    SettingSpec::boolean(super::keys::dms::HOVER_PREVIEW, true),
    SettingSpec::boolean(super::keys::features::MATUGEN, true),
    SettingSpec::boolean(super::keys::features::STEAM, true),
    SettingSpec::boolean(super::keys::features::WALLHAVEN, true),
    SettingSpec::boolean(super::keys::library::POLLING_FALLBACK, false),
    SettingSpec::boolean(super::keys::wallpaper::MUTE, true),
    SettingSpec::boolean(super::keys::niri::BACKDROP_BLUR_STATIC, true),
    SettingSpec::boolean(super::keys::niri::BACKDROP_BLUR_VIDEO, false),
    SettingSpec::boolean(super::keys::niri::BACKDROP_BLUR_WE, false),
    SettingSpec::boolean(super::keys::niri::BACKDROP_FOLLOW_WALLPAPER, true),
    SettingSpec::boolean(super::keys::general::RANDOM_INCLUDE_STATIC, true),
    SettingSpec::boolean(super::keys::general::RANDOM_INCLUDE_VIDEO, true),
    SettingSpec::boolean(super::keys::general::RANDOM_INCLUDE_WE, true),
    SettingSpec::boolean(super::keys::general::RANDOM_INCLUDE_FAVOURITES, false),
    SettingSpec::boolean(super::keys::general::NOTIFY_ON_WALLPAPER_CHANGE, true),
    SettingSpec::boolean(super::keys::system::RESTORE_ON_STARTUP, false),
    SettingSpec::boolean(super::keys::performance::BATTERY_SAVER, true),
    SettingSpec::boolean(super::keys::effects::AUTO_RECOLOR, false),
    SettingSpec::boolean(super::keys::general::RANDOM_ROTATE, false),
    SettingSpec::boolean(super::keys::general::WEATHER_MATCH, false),
    SettingSpec::boolean(super::keys::selector::ROUND_CORNERS, false),
    SettingSpec::boolean(super::keys::selector::SLICE_WOBBLE, false),
    SettingSpec::boolean(super::keys::selector::SLICE_PARALLAX, false),
    SettingSpec::boolean(super::keys::selector::HEX_PARALLAX, false),
    SettingSpec::boolean(super::keys::paper::PERFORMANCE_MODE, false),
    SettingSpec::boolean(super::keys::paper::VIDEO_MULTI_PROCESS, true),
    SettingSpec::boolean(super::keys::niri::OVERVIEW_ONLY_PLAYBACK, false),
    SettingSpec::boolean(super::keys::niri::FULL_WIDTH_PAUSE, false),
    SettingSpec::text(super::keys::paper::WALLPAPER_LAYER, "bottom"),
    SettingSpec::boolean(super::keys::performance::AUTO_DELETE_IMAGE_TRASH, false),
    SettingSpec::boolean(super::keys::performance::AUTO_OPTIMIZE_IMAGES, false),
    SettingSpec::boolean(
        super::keys::performance::BATTERY_WALLPAPER_PERFORMANCE,
        false,
    ),
    SettingSpec::boolean(super::keys::system::PICK_ONLY_MODE, false),
    SettingSpec::boolean(super::keys::system::POST_PROCESS_ON_RESTORE, false),
    SettingSpec::boolean(super::keys::schedule::APPLY_ON_START, true),
    SettingSpec::boolean(super::keys::schedule::ENABLED, false),
    SettingSpec::boolean(super::keys::sources::BING_ENABLED, false),
    SettingSpec::boolean(super::keys::sources::PEXELS_ENABLED, false),
    SettingSpec::boolean(super::keys::sources::UNSPLASH_ENABLED, false),
    SettingSpec::boolean(super::keys::sources::YOUTUBE_ENABLED, false),
    SettingSpec::boolean(super::keys::transition::PREVIEW, true),
    SettingSpec::boolean(super::keys::video_preview::ENABLED, true),
    SettingSpec::boolean(super::keys::we_render::DISABLE_PARTICLES, false),
    SettingSpec::boolean(super::keys::workspace::ENABLED, false),
    SettingSpec::boolean(super::keys::niri::BACKDROP_AUTO_THEME, false),
    SettingSpec::boolean(super::keys::niri::OVERVIEW_BACKDROP, false),
    SettingSpec::text(super::keys::niri::OVERVIEW_MODE, "separate"),
    SettingSpec::boolean(super::keys::selector::SANDY_OUTGOING_LIVE, true),
    SettingSpec::boolean(super::keys::selector::SANDY_SWAP_LOOP, false),
    SettingSpec::boolean(super::keys::paper::AWWW_INVERT_Y, false),
    SettingSpec::boolean(super::keys::transition::SAND_SHARP, false),
    SettingSpec::text(super::keys::paper::ENGINE, "skwd-paper"),
    SettingSpec::text(super::keys::display::FILL_MODE, "fill"),
    SettingSpec::text(super::keys::display::FILL_COLOR, "000000ff"),
    SettingSpec::text(super::keys::display::BACKGROUND_MODE, "color"),
    SettingSpec::text(super::keys::display::THEME_OUTPUT, ""),
    SettingSpec::text(super::keys::matugen::SCHEME_TYPE, "scheme-fidelity"),
    SettingSpec::text(super::keys::matugen::MODE, "dark"),
    SettingSpec::text(super::keys::noctalia::THEME_MODE, "follow"),
    SettingSpec::text(super::keys::steam::BACKEND, "steam"),
    SettingSpec::text(super::keys::we_render::SCALING, "default"),
    SettingSpec::text(super::keys::we_render::CLAMP, "clamp"),
    SettingSpec::text(super::keys::paper::AWWW_TRANSITION_TYPE, "wipe"),
    SettingSpec::text(super::keys::paper::AWWW_FILTER, "Lanczos3"),
    SettingSpec::text(super::keys::paper::AWWW_TRANSITION_POS, "center"),
    SettingSpec::text(super::keys::performance::IMAGE_OPTIMIZE_PRESET, "balanced"),
    SettingSpec::text(super::keys::performance::IMAGE_OPTIMIZE_RESOLUTION, "2k"),
    SettingSpec::text(super::keys::performance::GPU_DEVICE, "auto"),
    SettingSpec::text(super::keys::performance::GPU_PREFERENCE, "auto"),
    SettingSpec::text(super::keys::effects::AUTO_THEME, "Catppuccin"),
    SettingSpec::text(
        super::keys::system::EXTERNAL_MATUGEN_COMMAND,
        "matugen -c %config% image %path% -t %scheme% -m %mode% --source-color-index %index%",
    ),
    SettingSpec::text(super::keys::system::MONITOR, ""),
    SettingSpec::text(super::keys::general::LOCALE, ""),
    SettingSpec::text(super::keys::general::LANGUAGE, "auto"),
    SettingSpec::text(super::keys::launch::ANIMATION, "fade"),
    SettingSpec::text(super::keys::motion::FILTER_SWAP_SPEED, "slow"),
    SettingSpec::text(super::keys::motion::LAUNCH_SPEED, "standard"),
    SettingSpec::text(super::keys::selector::FLIP_EFFECT, "Ignite"),
    SettingSpec::text(super::keys::paper::VIDEO_ENGINE, "vulkan"),
    SettingSpec::text(super::keys::tagging::DEFAULT_SEARCH_MODE, "tags"),
    SettingSpec::boolean(super::keys::semantic::ENABLED, true),
    SettingSpec::text(super::keys::semantic::INDEX_PROFILE, "full"),
    SettingSpec::text(super::keys::semantic::MANIFEST, ""),
    SettingSpec::text(super::keys::theme::BACKEND, "skwd-iris"),
    SettingSpec::text(super::keys::theme::POLICY, ""),
    SettingSpec::text(super::keys::theme::AUTHORITY, ""),
    SettingSpec::text(super::keys::theme::ENGINE, ""),
    SettingSpec::text(super::keys::theme::MODE, "dark"),
    SettingSpec::text(super::keys::theme::SCHEME, "tonal-spot"),
    SettingSpec::text(super::keys::theme::STYLE, "natural"),
    SettingSpec::text(super::keys::transition::FAMILY, "random"),
    SettingSpec::boolean(super::keys::sources::WALLHAVEN_SHOW_APPLY_BUTTON, false),
    SettingSpec::boolean(super::keys::sources::STEAM_SHOW_APPLY_BUTTON, false),
    SettingSpec::boolean(super::keys::sources::UNSPLASH_SHOW_APPLY_BUTTON, false),
    SettingSpec::boolean(super::keys::sources::PEXELS_SHOW_APPLY_BUTTON, false),
    SettingSpec::boolean(super::keys::sources::YOUTUBE_SHOW_APPLY_BUTTON, false),
    SettingSpec::text(super::keys::sources::BING_MARKET, "en-US"),
    SettingSpec::text(super::keys::sources::PEXELS_API_KEY, ""),
    SettingSpec::text(super::keys::sources::UNSPLASH_ACCESS_KEY, ""),
    SettingSpec::text(super::keys::steam::DEFAULT_SORT, "3"),
    SettingSpec::text(super::keys::steam::DEFAULT_TREND_DAYS, "7"),
    SettingSpec::text(super::keys::steam::DEFAULT_TYPE, ""),
    SettingSpec::text(super::keys::steam::DEFAULT_CATEGORY, ""),
    SettingSpec::text(super::keys::steam::DEFAULT_RESOLUTION, ""),
    SettingSpec::boolean(super::keys::steam::DEFAULT_NSFW, false),
    SettingSpec::text(super::keys::steam::API_KEY, ""),
    SettingSpec::text(super::keys::steam::USERNAME, ""),
    SettingSpec::text(super::keys::wallhaven::DEFAULT_SORT, "toplist"),
    SettingSpec::text(super::keys::wallhaven::DEFAULT_TOP_RANGE, "1M"),
    SettingSpec::text(super::keys::wallhaven::DEFAULT_ATLEAST, ""),
    SettingSpec::text(super::keys::wallhaven::DEFAULT_ATMOST, ""),
    SettingSpec::text(super::keys::wallhaven::DEFAULT_RATIOS, ""),
    SettingSpec::boolean(super::keys::wallhaven::DEFAULT_GENERAL, true),
    SettingSpec::boolean(super::keys::wallhaven::DEFAULT_ANIME, true),
    SettingSpec::boolean(super::keys::wallhaven::DEFAULT_PEOPLE, true),
    SettingSpec::boolean(super::keys::wallhaven::DEFAULT_SFW, true),
    SettingSpec::boolean(super::keys::wallhaven::DEFAULT_SKETCHY, false),
    SettingSpec::boolean(super::keys::wallhaven::DEFAULT_NSFW, false),
    SettingSpec::text(super::keys::wallhaven::API_KEY, ""),
    SettingSpec::text(super::keys::wallhaven::USERNAME, ""),
    SettingSpec::text(super::keys::schedule::LATITUDE, ""),
    SettingSpec::text(super::keys::schedule::LONGITUDE, ""),
    SettingSpec::text(super::keys::paths::DMS_BIN, ""),
    SettingSpec::text(super::keys::paths::CAELESTIA_BIN, ""),
    SettingSpec::text(super::keys::paths::NOCTALIA_BIN, ""),
    SettingSpec::text(super::keys::paths::STEAM, ""),
    SettingSpec::text(super::keys::paths::STEAM_WE_ASSETS, ""),
    SettingSpec::text(super::keys::paths::STEAM_WORKSHOP, ""),
    SettingSpec::text(super::keys::paths::VIDEO_WALLPAPER, ""),
    SettingSpec::text(super::keys::paths::WALLPAPER, ""),
    SettingSpec::text(super::keys::matugen::DEFAULT_CONFIG, ""),
    SettingSpec::text(super::keys::niri::BACKDROP, ""),
    SettingSpec::text(super::keys::niri::BACKDROP_THEME, ""),
    SettingSpec::text(super::keys::plasma::LOCK_SCREEN_DYNAMIC, "poster"),
    SettingSpec::text(super::keys::plasma::LOCK_SCREEN_IMAGE, ""),
    SettingSpec::text(super::keys::plasma::LOCK_SCREEN_MODE, "off"),
    SettingSpec::text(super::keys::selector::SANDY_SWAP_STYLE, "vortex"),
    SettingSpec::text(super::keys::selector::HAND_CUT, "straight"),
    SettingSpec::text(super::keys::selector::HAND_CUT_VARIANCE, "none"),
    SettingSpec::text(super::keys::selector::HAND_MOVE, "cycle"),
    SettingSpec::text(super::keys::selector::HAND_RIBBON_AXIS, "rows"),
    SettingSpec::text(super::keys::paper::AWWW_TRANSITION_BEZIER, ""),
    SettingSpec::text(super::keys::theme::CUSTOM_COLORS, ""),
    SettingSpec::text(super::keys::theme::STATIC_THEME, "nord"),
    SettingSpec::text(super::keys::theme::WALLUST_PALETTE, ""),
    SettingSpec::text(super::keys::theme::WALLUST_COLORSPACE, ""),
    SettingSpec::text(super::keys::theme::PYWAL_SATURATE, ""),
    SettingSpec::text(super::keys::theme::NOCTALIA_SCHEME, ""),
    SettingSpec::boolean(super::keys::theme::NOCTALIA_PURE_BLACK, false),
    SettingSpec::text(super::keys::transition::SAND_PRIMARY, ""),
    SettingSpec::text(super::keys::transition::SAND_QUALITY, "auto"),
    SettingSpec::text(super::keys::transition::SAND_SCOPE, "all"),
    SettingSpec::text(super::keys::keybind::APPLY, ""),
    SettingSpec::text(super::keys::keybind::AUTOCOMPLETE, ""),
    SettingSpec::text(super::keys::keybind::COLOR_NEXT, ""),
    SettingSpec::text(super::keys::keybind::COLOR_PREV, ""),
    SettingSpec::text(super::keys::keybind::EFFECTS, ""),
    SettingSpec::text(super::keys::keybind::FAVOURITE, ""),
    SettingSpec::text(super::keys::keybind::FILTER_BAR, ""),
    SettingSpec::text(super::keys::keybind::FOLDER_PREV, ""),
    SettingSpec::text(super::keys::keybind::FOLDER_NEXT, ""),
    SettingSpec::text(super::keys::keybind::FOLDER_TOGGLE, ""),
    SettingSpec::text(super::keys::keybind::HIDDEN_FOLDERS, ""),
    SettingSpec::text(super::keys::keybind::JUMP_FIRST, ""),
    SettingSpec::text(super::keys::keybind::JUMP_LAST, ""),
    SettingSpec::text(super::keys::keybind::FLIP, ""),
    SettingSpec::text(super::keys::keybind::REVEAL, ""),
    SettingSpec::text(super::keys::keybind::HELP, ""),
    SettingSpec::text(super::keys::keybind::MODE_NEXT, ""),
    SettingSpec::text(super::keys::keybind::MODE_PREV, ""),
    SettingSpec::text(super::keys::keybind::NAV_DOWN, ""),
    SettingSpec::text(super::keys::keybind::NAV_LEFT, ""),
    SettingSpec::text(super::keys::keybind::NAV_RIGHT, ""),
    SettingSpec::text(super::keys::keybind::NAV_UP, ""),
    SettingSpec::text(super::keys::keybind::PAGE_BACK, ""),
    SettingSpec::text(super::keys::keybind::PAGE_FORWARD, ""),
    SettingSpec::text(super::keys::keybind::PLAYLISTS, ""),
    SettingSpec::text(super::keys::keybind::SCENE_PROPERTIES, ""),
    SettingSpec::text(super::keys::keybind::SELECT, ""),
    SettingSpec::text(super::keys::keybind::SETTINGS, ""),
    SettingSpec::text(super::keys::keybind::TAG_CLOUD, ""),
    SettingSpec::text(super::keys::keybind::TAG_MODE, ""),
    SettingSpec::text(super::keys::keybind::THEME_PANEL, ""),
    SettingSpec::text(super::keys::keybind::DOWNLOADS, ""),
    SettingSpec::text(super::keys::keybind::RANDOM_APPLY, ""),
    SettingSpec::text(super::keys::keybind::RANDOM_ROTATE, ""),
    SettingSpec::text(super::keys::keybind::SEARCH_MODE, ""),
    SettingSpec::text(super::keys::keybind::SORT_NEXT, ""),
    SettingSpec::text(super::keys::keybind::SORT_PREV, ""),
    SettingSpec::text(super::keys::keybind::SOURCE_BING, ""),
    SettingSpec::text(super::keys::keybind::SOURCE_PEXELS, ""),
    SettingSpec::text(super::keys::keybind::SOURCE_STEAM, ""),
    SettingSpec::text(super::keys::keybind::SOURCE_UNSPLASH, ""),
    SettingSpec::text(super::keys::keybind::SOURCE_WALLHAVEN, ""),
    SettingSpec::text(super::keys::keybind::SOURCE_YOUTUBE, ""),
    SettingSpec::text(super::keys::keybind::TYPE_NEXT, ""),
    SettingSpec::text(super::keys::keybind::TYPE_PREV, ""),
    SettingSpec::number(super::keys::paper::AWWW_TRANSITION_DURATION_MS, 1000.0),
    SettingSpec::number(super::keys::paper::AWWW_TRANSITION_FPS, 60.0),
    SettingSpec::number(super::keys::paper::AWWW_TRANSITION_STEP, 90.0),
    SettingSpec::number(super::keys::paper::AWWW_TRANSITION_ANGLE, 45.0),
    SettingSpec::number(super::keys::paper::AWWW_TRANSITION_WAVE_WIDTH, 20.0),
    SettingSpec::number(super::keys::paper::AWWW_TRANSITION_WAVE_HEIGHT, 20.0),
    SettingSpec::bounded_number(super::keys::performance::IMAGE_TRASH_DAYS, 7.0, 0.0, 3650.0),
    SettingSpec::bounded_number(super::keys::performance::MAX_THUMB_JOBS, 16.0, 1.0, 32.0),
    SettingSpec::bounded_number(
        super::keys::library::POLLING_INTERVAL_SECONDS,
        60.0,
        15.0,
        3600.0,
    ),
    SettingSpec::bounded_number(
        super::keys::performance::BATTERY_FPS,
        DEFAULT_BATTERY_FPS as f64,
        0.0,
        360.0,
    ),
    SettingSpec::number(
        super::keys::performance::BATTERY_VIDEO_IDLE_SECONDS,
        DEFAULT_BATTERY_VIDEO_IDLE_SECONDS as f64,
    ),
    SettingSpec::bounded_number(super::keys::we_render::FPS, 30.0, 1.0, 240.0),
    SettingSpec::number(super::keys::matugen::CONTRAST, 0.0),
    SettingSpec::bounded_number(super::keys::wallpaper::VOLUME, 100.0, 0.0, 100.0),
    SettingSpec::bounded_number(super::keys::matugen::COLOR_INDEX, 0.0, 0.0, 3.0),
    SettingSpec::responsive_number(super::keys::selector::EXPANDED_WIDTH, 924.0, 600.0),
    SettingSpec::responsive_number(super::keys::selector::SLICE_HEIGHT, 520.0, 360.0),
    SettingSpec::responsive_number(super::keys::selector::SLICE_WIDTH, 135.0, 90.0),
    SettingSpec::responsive_number(super::keys::selector::VISIBLE_COUNT, 12.0, 8.0),
    SettingSpec::responsive_number(super::keys::selector::SKEW_OFFSET, 35.0, 25.0),
    SettingSpec::bounded_number(super::keys::selector::SLICE_EDGE_TILT, 0.0, -240.0, 240.0),
    SettingSpec::number(super::keys::selector::SLICE_SPACING, 0.0),
    SettingSpec::number(super::keys::selector::SLICE_WOBBLE_STRENGTH, 100.0),
    SettingSpec::number(super::keys::selector::CORNER_TL, 16.0),
    SettingSpec::number(super::keys::selector::CORNER_TR, 16.0),
    SettingSpec::number(super::keys::selector::CORNER_BR, 16.0),
    SettingSpec::number(super::keys::selector::CORNER_BL, 16.0),
    SettingSpec::responsive_number(super::keys::selector::TAG_CLOUD_WIDTH, 760.0, 540.0),
    SettingSpec::responsive_number(super::keys::selector::TAG_CLOUD_HEIGHT, 168.0, 150.0),
    SettingSpec::text(super::keys::selector::TAG_CLOUD_ROWS, "2"),
    SettingSpec::number(super::keys::selector::TAG_CLOUD_OFFSET_X, 0.0),
    SettingSpec::number(super::keys::selector::TAG_CLOUD_OFFSET_Y, 0.0),
    SettingSpec::number(super::keys::selector::WALLHAVEN_COLUMNS, 6.0),
    SettingSpec::number(super::keys::selector::WALLHAVEN_ROWS, 3.0),
    SettingSpec::number(super::keys::selector::WALLHAVEN_THUMB_WIDTH, 300.0),
    SettingSpec::number(super::keys::selector::WALLHAVEN_THUMB_HEIGHT, 169.0),
    SettingSpec::number(super::keys::selector::STEAM_COLUMNS, 6.0),
    SettingSpec::number(super::keys::selector::STEAM_ROWS, 3.0),
    SettingSpec::number(super::keys::selector::STEAM_THUMB_WIDTH, 300.0),
    SettingSpec::number(super::keys::selector::STEAM_THUMB_HEIGHT, 169.0),
    SettingSpec::responsive_number(super::keys::selector::GRID_COLUMNS, 6.0, 4.0),
    SettingSpec::number(super::keys::selector::GRID_ROWS, 3.0),
    SettingSpec::responsive_number(super::keys::selector::GRID_THUMB_WIDTH, 300.0, 220.0),
    SettingSpec::responsive_number(super::keys::selector::GRID_THUMB_HEIGHT, 169.0, 124.0),
    SettingSpec::responsive_number(super::keys::selector::HEX_RADIUS, 140.0, 100.0),
    SettingSpec::number(super::keys::selector::HEX_ROWS, 3.0),
    SettingSpec::responsive_number(super::keys::selector::HEX_COLS, 7.0, 5.0),
    SettingSpec::number(super::keys::selector::HEX_SCROLL_STEP, 1.0),
    SettingSpec::number(super::keys::selector::HEX_ARC_INTENSITY_X10, 12.0),
    SettingSpec::number(super::keys::selector::HEX_ARC_INTENSITY, 1.2),
    SettingSpec::bounded_number(super::keys::selector::HAND_COUNT, 5.0, 2.0, 16.0),
    SettingSpec::bounded_number(super::keys::selector::HAND_FAN_ANGLE, 12.0, -60.0, 60.0),
    SettingSpec::bounded_number(super::keys::selector::HAND_FAN_ROLL, 8.5, -45.0, 45.0),
    SettingSpec::bounded_number(super::keys::selector::HAND_ARCH, 20.0, -200.0, 200.0),
    SettingSpec::bounded_number(super::keys::selector::HAND_CORNER_RADIUS, 0.0, 0.0, 200.0),
    SettingSpec::bounded_number(super::keys::selector::HAND_SKEW, 0.0, -200.0, 200.0),
    SettingSpec::bounded_number(super::keys::selector::HAND_BACKDROP_BLUR, 100.0, 0.0, 400.0),
    SettingSpec::number(super::keys::selector::HAND_CARD_WIDTH, 168.0),
    SettingSpec::number(super::keys::selector::HAND_CARD_HEIGHT, 432.0),
    SettingSpec::number(super::keys::selector::HAND_SPREAD, 126.0),
    SettingSpec::bounded_number(super::keys::selector::HAND_RIBBONS, 6.0, 2.0, 14.0),
    SettingSpec::number(super::keys::selector::HAND_SPEED, 100.0),
    SettingSpec::number(super::keys::selector::HAND_TILT, 100.0),
    SettingSpec::bounded_number(
        super::keys::selector::HAND_PERSPECTIVE,
        1700.0,
        400.0,
        6000.0,
    ),
    SettingSpec::responsive_number(super::keys::selector::SANDY_CENTER, 440.0, 330.0),
    SettingSpec::responsive_number(super::keys::selector::SANDY_SLICE_WIDTH, 96.0, 68.0),
    SettingSpec::responsive_number(super::keys::selector::SANDY_SLICE_HEIGHT, 180.0, 130.0),
    SettingSpec::responsive_number(super::keys::selector::SANDY_SKEW, 12.0, 8.0),
    SettingSpec::responsive_number(super::keys::selector::SANDY_SPACING, 26.0, 20.0),
    SettingSpec::bounded_number(super::keys::selector::SANDY_DURATION, 1250.0, 200.0, 6000.0),
    SettingSpec::bounded_number(super::keys::selector::SANDY_BLEND, 700.0, 100.0, 4000.0),
    SettingSpec::responsive_number(super::keys::selector::SANDY_STRANDS, 22.0, 18.0),
    SettingSpec::number(super::keys::selector::SANDY_TWIST, 100.0),
    SettingSpec::number(super::keys::selector::SANDY_ORBIT, 100.0),
    SettingSpec::number(super::keys::selector::SANDY_TURBULENCE, 100.0),
    SettingSpec::number(super::keys::selector::SANDY_WAIST, 100.0),
    SettingSpec::number(super::keys::selector::SANDY_FRONT, 100.0),
    SettingSpec::number(super::keys::selector::SANDY_ARC, 100.0),
    SettingSpec::number(super::keys::selector::SANDY_EDGE_SPEED, 100.0),
    SettingSpec::number(super::keys::selector::SANDY_FAN, 100.0),
    SettingSpec::number(super::keys::selector::SANDY_RING_SPIN, 100.0),
    SettingSpec::number(super::keys::selector::SANDY_RING_SIZE, 100.0),
    SettingSpec::number(super::keys::selector::SANDY_RING_WAVE, 100.0),
    SettingSpec::number(super::keys::selector::SANDY_RING_SOFT, 100.0),
    SettingSpec::number(super::keys::selector::SANDY_RING_BLEND, 100.0),
    SettingSpec::number(super::keys::selector::SANDY_RING_HOLD, 200.0),
    SettingSpec::bounded_number(super::keys::selector::SANDY_GRAIN, 3.0, 1.0, 32.0),
    SettingSpec::bounded_number(super::keys::selector::SANDY_RES_SCALE, 100.0, 25.0, 100.0),
    SettingSpec::bounded_number(super::keys::selector::SANDY_LOD, 2.0, 1.0, 4.0),
    SettingSpec::number(super::keys::paper::IDLE_PAUSE_SECONDS, 0.0),
    SettingSpec::boolean(super::keys::playback::PROCESS_ENABLED, false),
    SettingSpec::text(super::keys::playback::PROCESSES, ""),
    SettingSpec::boolean(super::keys::playback::FULLSCREEN, false),
    SettingSpec::boolean(super::keys::playback::MAXIMIZED, false),
    SettingSpec::text(super::keys::playback::FULLSCREEN_SCOPE, "all"),
    SettingSpec::number(super::keys::playback::RESUME_DELAY, 1.0),
    SettingSpec::boolean(super::keys::playback::MUTE_ON_OTHER_AUDIO, false),
    SettingSpec::number(super::keys::video_preview::DELAY_MS, 250.0),
    SettingSpec::number(super::keys::sources::YOUTUBE_MAX_HEIGHT, 2160.0),
    SettingSpec::number(super::keys::sources::YOUTUBE_MAX_MINUTES, 3.0),
    SettingSpec::number(super::keys::transition::SAND_FPS, 0.0),
    SettingSpec::bounded_number(
        super::keys::niri::BACKDROP_BLUR_STATIC_RADIUS,
        20.0,
        0.0,
        100.0,
    ),
    SettingSpec::bounded_number(
        super::keys::niri::BACKDROP_BLUR_VIDEO_RADIUS,
        20.0,
        0.0,
        100.0,
    ),
    SettingSpec::bounded_number(super::keys::niri::BACKDROP_BLUR_WE_RADIUS, 20.0, 0.0, 100.0),
    SettingSpec::bounded_number(super::keys::niri::BACKDROP_DIM, 0.0, 0.0, 100.0),
    SettingSpec::array(super::keys::filter_bar::RESOLUTION_PRESETS),
    SettingSpec::array(super::keys::integrations::LIST),
    SettingSpec::array(super::keys::post_processing::LIST),
    SettingSpec::array(super::keys::semantic::MODELS),
    SettingSpec::array(super::keys::theme::TARGETS),
    SettingSpec::array(super::keys::workspace::WALLPAPERS),
    SettingSpec::text(super::keys::selector::PRESET_DRAFT_NAME, ""),
];

pub fn all() -> &'static [SettingSpec] {
    STATIC_SPECS
}

pub fn find(path: &str) -> Option<&'static SettingSpec> {
    STATIC_SPECS.iter().find(|spec| spec.path == path)
}

pub fn value_kind(path: &str) -> Option<ValueKind> {
    if path.starts_with("display.outputLocks.")
        || path.starts_with("filterBar.show.")
        || path.ends_with("hexArc")
        || path.ends_with(".livePreview")
        || path.starts_with("integrations.") && path.ends_with(".enabled")
    {
        return Some(ValueKind::Boolean);
    }
    if path.starts_with("display.fillModes.")
        || path.starts_with("transition.shaderScopes.")
        || path.starts_with("display.backgroundModes.")
        || path.starts_with("display.backgroundColors.")
    {
        return Some(ValueKind::Text);
    }
    if path.ends_with("displayMode") {
        return Some(ValueKind::Text);
    }
    if path.starts_with("filterBar.resolutionPresets.") {
        return if path.ends_with(".width") || path.ends_with(".height") {
            Some(ValueKind::Number)
        } else if path.ends_with(".label")
            || path.ends_with(".orientation")
            || path.ends_with(".from")
            || path.ends_with(".to")
        {
            Some(ValueKind::Text)
        } else {
            None
        };
    }
    if path.starts_with("postProcessing.") || path.starts_with("integrations.") {
        return if path.ends_with(".livePreview") || path.ends_with(".enabled") {
            Some(ValueKind::Boolean)
        } else {
            Some(ValueKind::Text)
        };
    }
    find(path).map(|spec| spec.kind)
}

pub fn boolean_default(path: &str) -> Option<bool> {
    if path.starts_with("display.outputLocks.") {
        return Some(false);
    }
    if path.starts_with("filterBar.show.")
        || path.ends_with("hexArc")
        || path.ends_with(".livePreview")
        || path.starts_with("integrations.") && path.ends_with(".enabled")
    {
        return Some(true);
    }
    find(path).and_then(|spec| match spec.default {
        DefaultValue::Boolean(value) => Some(value),
        _ => None,
    })
}

pub fn number_default(path: &str) -> Option<f64> {
    find(path).and_then(|spec| match spec.default {
        DefaultValue::Number(value) => Some(value),
        DefaultValue::ResponsiveNumber { regular, .. } => Some(regular),
        _ => None,
    })
}

pub fn text_default(path: &str) -> Option<&'static str> {
    if path.ends_with("displayMode") {
        return Some("slices");
    }
    find(path).and_then(|spec| match spec.default {
        DefaultValue::Text(value) => Some(value),
        _ => None,
    })
}

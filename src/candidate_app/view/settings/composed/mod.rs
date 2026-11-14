#![allow(dead_code, unused_imports)]
macro_rules! tr_args {
    ($catalog:expr, $key:literal $(, $name:ident => $value:expr)+ $(,)?) => {{
        let mut args = ::fluent::FluentArgs::new();
        $(args.set(stringify!($name), $value);)+
        $catalog.format($key, Some(&args)).expect("verified source translation arguments")
    }};
}

mod action;
pub(super) mod background;
mod builder;
mod capabilities;
pub mod catalog;
pub(super) mod display;
pub mod draft;
pub mod input;
mod keybindings;
pub mod keys;
pub mod media;
mod model;
pub mod schema;
mod source;
pub mod status;
mod tables;
mod tabs;
pub mod transitions;
pub use action::*;
pub use keybindings::*;
pub use model::*;
pub use source::*;

#[cfg(test)]
mod composition_tests;
#[cfg(test)]
pub(super) mod test_source;

#[cfg(test)]
mod tests;

use builder::Builder;
pub use schema::*;
pub(in crate::candidate_app) use tables::SHADERS;
pub use tables::{
    is_picker_layout_section, visible_tabs, PICKER_CONTROLS_SECTION, PICKER_LAYOUT_SECTION,
};
pub(crate) use tabs::build_tab_with_runtime_status;
#[cfg(test)]
pub(crate) use tabs::compatibility_owner_rows;

pub const PRESET_NAME_KEY: &str = keys::selector::PRESET_DRAFT_NAME;
pub const EFFECT_NAMES: [&str; 12] = [
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

pub mod wallpaper_kind {
    pub const STATIC: &str = "static";
    pub const VIDEO: &str = "video";
    pub const WE: &str = "we";
}

pub struct Mode(&'static str);
impl Mode {
    pub const ALL: [Self; 4] = [Self("slices"), Self("depth"), Self("hex"), Self("wall")];
    pub fn from_key(key: &str) -> Self {
        Self(match key {
            "hex" => "hex",
            "wall" | "grid" => "wall",
            "depth" => "depth",
            _ => "slices",
        })
    }
    pub fn as_key(&self) -> &'static str {
        self.0
    }
}
pub fn language_choice(raw: &str) -> &'static str {
    if raw == "auto" || raw.trim().is_empty() {
        "auto"
    } else {
        crate::candidate_app::localization::normalized_tag(raw).unwrap_or("en-US")
    }
}

pub mod binding_capture;
pub mod collections;
pub mod navigation;
pub mod search;
pub use tabs::build_tab;
pub fn format_config_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// Stable widget IDs shared by renderer focus and its application reducer.
pub fn workbench_input_id(key: &str) -> iced::widget::Id {
    format!("settings-control:{key}").into()
}
pub fn settings_search_input_id() -> iced::widget::Id {
    "settings-search".into()
}

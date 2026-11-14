//! Source-derived pure control projection; explicit locale and -owned consumers.
use crate::candidate_app::view::settings::composed::{transitions, wallpaper_kind, SettingsSource};

pub const TABS: [&str; 10] = [
    "picker",
    "filter",
    "position",
    "motion",
    "playback",
    "performance",
    "library",
    "automation",
    "theme",
    "language",
];

pub(super) const MOTION_SPEEDS: [&str; 3] = ["fast", "standard", "slow"];

pub fn canonical_category(tab: String) -> String {
    match tab.as_str() {
        "search" => String::from("filter"),
        _ => tab,
    }
}

pub fn visible_tabs(cfg: &dyn SettingsSource) -> Vec<(&'static str, &'static str)> {
    let catalog = cfg.catalog();
    let tr = |key| catalog.text_static(key);

    TABS.iter()
        .map(|key| (*key, tr(tab_label_key(key))))
        .collect()
}

fn tab_label_key(tab: &str) -> &'static str {
    match tab {
        "filter" => "settings-tab-filter",
        "position" => "settings-tab-position",
        "displays" => "settings-tab-displays",
        "motion" => "settings-tab-motion",
        "playback" => "settings-tab-playback",
        "performance" => "settings-tab-performance",
        "library" => "settings-tab-library",
        "sources" => "settings-tab-sources",
        "automation" => "settings-tab-automation",
        "theme" => "settings-tab-theme",
        "integrations" => "settings-tab-integrations",
        "language" => "settings-tab-language",
        _ => "settings-tab-picker",
    }
}

pub(super) fn motion_speed_options(
    catalog: &crate::candidate_app::localization::Catalog,
) -> [(&'static str, &'static str); 3] {
    let tr = |key| catalog.text_static(key);

    MOTION_SPEEDS.map(|key| {
        let label = match key {
            "fast" => "settings-motion-fast",
            "slow" => "settings-motion-slow",
            _ => "settings-motion-standard",
        };
        (key, tr(label))
    })
}

pub const PICKER_LAYOUT_SECTION: usize = 1;
pub const PICKER_CONTROLS_SECTION: usize = 3;

pub fn is_picker_layout_section(tab: &str, section: usize) -> bool {
    tab == "picker" && section == PICKER_LAYOUT_SECTION
}

pub(super) const POST_TYPES: [&str; 4] = [
    "all",
    wallpaper_kind::STATIC,
    wallpaper_kind::VIDEO,
    wallpaper_kind::WE,
];
pub(super) const FILL_MODES: [&str; 6] = ["fill", "fit", "stretch", "center", "tile", "span"];
pub(super) const ENGINES: [(&str, &str); 2] = [("skwd-paper", "Skwd-paper"), ("awww", "awww")];
pub(super) const VIDEO_ENGINES: [&str; 2] = ["vulkan", "tinier"];
pub(super) const AWWW_TYPES: [&str; 14] = [
    "none", "simple", "fade", "wipe", "wave", "grow", "center", "outer", "left", "right", "top",
    "bottom", "any", "random",
];
pub(super) const AWWW_FILTERS: [(&str, &str); 5] = [
    ("Nearest", "Nearest"),
    ("Bilinear", "Bilinear"),
    ("CatmullRom", "CatmullRom"),
    ("Mitchell", "Mitchell"),
    ("Lanczos3", "Lanczos3"),
];
pub const SHADERS: [&str; transitions::TRANSITIONS.len()] = {
    let mut out = [""; transitions::TRANSITIONS.len()];
    let mut i = 0;
    while i < transitions::TRANSITIONS.len() {
        out[i] = transitions::TRANSITIONS[i].key;
        i += 1;
    }
    out
};
pub(crate) const NIRI_SNIPPET: &str = "layer-rule {\n    match namespace=\"^skwd-paper-backdrop$\"\n    place-within-backdrop true\n}\n\nlayer-rule {\n    match namespace=\"^skwd-paper-stationary$\"\n    place-within-backdrop true\n}\n\nlayout {\n    background-color \"transparent\"\n}\n\noverview {\n    workspace-shadow { off; }\n}";

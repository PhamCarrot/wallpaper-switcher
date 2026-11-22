//! Source-derived pure control projection; explicit locale and -owned consumers.
use crate::candidate_app::view::settings::composed::{keys, EFFECT_NAMES};

use super::super::tables::motion_speed_options;
use super::builder::{folder_dropdown_options, type_chip_label};
use super::{ActionId, Builder, Control, PRESET_NAME_KEY};

pub(super) fn tab_selector(builder: &mut Builder<'_>) {
    let catalog = builder.cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let cfg = builder.cfg;
    let mode = cfg.display_mode();
    builder.card(tr("settings-selector-layout-card"), "");
    builder.dropdown(
        tr("settings-selector-start-position-label"),
        tr("settings-selector-start-position-desc"),
        keys::selector::START_POSITION,
        &[
            (
                "beginning",
                tr("settings-selector-start-position-beginning"),
            ),
            ("applied", tr("settings-selector-start-position-applied")),
            ("browsing", tr("settings-selector-start-position-browsing")),
        ],
    );
    let display_mode =
        crate::candidate_app::view::settings::composed::Mode::from_key(&mode).as_key();
    builder.row(
        tr("settings-selector-display-mode-label"),
        tr("settings-selector-display-mode-desc"),
        Control::Chips {
            path: keys::selector::DISPLAY_MODE.to_string(),
            options: crate::candidate_app::view::settings::composed::Mode::ALL
                .iter()
                .map(|mode| {
                    (
                        mode.as_key().to_string(),
                        tr(mode_label_key(mode.as_key())).to_string(),
                    )
                })
                .collect(),
            current: display_mode.to_string(),
            disabled: Vec::new(),
            background: None,
        },
    );
    let filter_speed = match cfg.text(keys::motion::FILTER_SWAP_SPEED).as_str() {
        "fast" | "standard" | "slow" => cfg.text(keys::motion::FILTER_SWAP_SPEED),
        _ => String::from("slow"),
    };
    builder.dropdown_cur(
        tr("settings-selector-filter-motion-label"),
        tr("settings-selector-filter-motion-desc"),
        keys::motion::FILTER_SWAP_SPEED,
        &motion_speed_options(catalog),
        filter_speed,
    );
    {
        builder.card(
            tr("settings-selector-presets-card"),
            tr("settings-selector-presets-card-desc"),
        );
        let pmode = crate::candidate_state::GalleryKind::from_key(&cfg.selector_mode())
            .expect("four supported modes");
        builder.text_field(
            tr("settings-selector-preset-name-label"),
            tr("settings-selector-preset-name-desc"),
            PRESET_NAME_KEY,
            tr("settings-selector-preset-name-placeholder"),
        );
        let items = cfg.mode_profile_rows(pmode);
        builder.row("", "", Control::Presets { mode: pmode, items });
    }
    builder.card(tr("settings-selector-live-preview-card"), "");
    builder.toggle(
        tr("settings-selector-live-preview-label"),
        tr("settings-selector-live-preview-desc"),
        keys::selector::LIVE_PREVIEW,
    );
    match mode.as_str() {
        "depth" => {
            builder.card(
                tr("settings-selector-depth-card"),
                tr("settings-selector-depth-desc"),
            );
            builder.num(
                tr("settings-selector-depth-height-label"),
                tr("settings-selector-depth-height-desc"),
                keys::selector::DEPTH_HEIGHT,
                "px",
            );
            builder.num(
                tr("settings-selector-depth-width-label"),
                tr("settings-selector-depth-width-desc"),
                keys::selector::DEPTH_WIDTH_PX,
                "px",
            );
            builder.num(
                tr("settings-selector-depth-spacing-label"),
                tr("settings-selector-depth-spacing-desc"),
                keys::selector::DEPTH_SPACING_PX,
                "px",
            );
            builder.num(
                tr("settings-selector-depth-count-label"),
                tr("settings-selector-depth-count-desc"),
                keys::selector::DEPTH_COUNT,
                "",
            );
            builder.num(
                tr("settings-selector-depth-falloff-label"),
                tr("settings-selector-depth-falloff-desc"),
                keys::selector::DEPTH_FALLOFF_FACTOR,
                "",
            );
            builder.num(
                tr("settings-selector-depth-corners-label"),
                tr("settings-selector-depth-corners-desc"),
                keys::selector::DEPTH_CORNERS,
                "px",
            );
            builder.num(
                tr("settings-selector-depth-skew-label"),
                tr("settings-selector-depth-skew-desc"),
                keys::selector::DEPTH_SKEW,
                "px",
            );
            builder.num(
                tr("settings-selector-depth-speed-label"),
                tr("settings-selector-depth-speed-desc"),
                keys::selector::DEPTH_NAVIGATION_MS,
                "ms",
            );
            builder.toggle(
                tr("settings-selector-depth-frame-label"),
                tr("settings-selector-depth-frame-desc"),
                keys::selector::DEPTH_SELECTION_FRAME,
            );
            builder.toggle(
                tr("settings-selector-shadows-label"),
                tr("settings-selector-shadows-desc"),
                keys::selector::DEPTH_SHADOWS,
            );
        }

        "hex" => {
            builder.card(
                tr("settings-selector-composition-card"),
                tr("settings-selector-composition-card-desc-hex"),
            );
            builder.toggle(
                tr("settings-selector-parallax-label"),
                tr("settings-selector-parallax-desc"),
                keys::selector::HEX_PARALLAX,
            );
            builder.chips(
                tr("settings-selector-hex-curve-label"),
                tr("settings-selector-hex-curve-desc"),
                keys::selector::HEX_CURVE,
                &[
                    ("flat", tr("settings-selector-hex-curve-flat")),
                    ("arc", tr("settings-selector-hex-curve-arc")),
                    ("wave", tr("settings-selector-hex-curve-wave")),
                    ("s", tr("settings-selector-hex-curve-s")),
                    ("cylinder", tr("settings-selector-hex-curve-cylinder")),
                ],
            );
            builder.chips(
                tr("settings-selector-hex-shape-label"),
                tr("settings-selector-hex-shape-desc"),
                keys::selector::HEX_SHAPE,
                &[
                    ("triangle", tr("settings-selector-hex-shape-triangle")),
                    ("hexagon", tr("settings-selector-hex-shape-hexagon")),
                    ("diamond", tr("settings-selector-hex-shape-diamond")),
                    ("rhombus", tr("settings-selector-hex-shape-rhombus")),
                ],
            );
            let curve = cfg.text(keys::selector::HEX_CURVE);
            if curve != "flat" {
                builder.num(
                    tr("settings-selector-hex-intensity-label"),
                    tr("settings-selector-hex-intensity-desc"),
                    keys::selector::HEX_ARC_INTENSITY_X10,
                    "",
                );
            }
            if curve == "wave" {
                builder.num(
                    tr("settings-selector-hex-waves-label"),
                    tr("settings-selector-hex-waves-desc"),
                    keys::selector::HEX_CURVE_FREQUENCY,
                    "x",
                );
            }
            if curve == "cylinder" {
                builder.num(
                    tr("settings-selector-hex-orbit-radius-label"),
                    tr("settings-selector-hex-orbit-radius-desc"),
                    keys::selector::HEX_ORBIT_RADIUS,
                    "px",
                );
            }
            builder.num(
                tr("settings-selector-hex-rows-label"),
                "",
                keys::selector::HEX_ROWS,
                "",
            );
            builder.num(
                tr("settings-selector-hex-cols-label"),
                "",
                keys::selector::HEX_COLS,
                "",
            );
            builder.card(
                tr("settings-selector-cells-card"),
                tr("settings-selector-cells-card-desc"),
            );
            builder.num(
                tr("settings-selector-hex-cell-size-label"),
                "",
                keys::selector::HEX_RADIUS,
                "px",
            );
            builder.num(
                tr("settings-selector-hex-gap-x-label"),
                "",
                keys::selector::HEX_GAP_X,
                "px",
            );
            builder.num(
                tr("settings-selector-hex-gap-y-label"),
                "",
                keys::selector::HEX_GAP_Y,
                "px",
            );
        }
        "wall" | "grid" => {
            builder.card(
                tr("settings-selector-composition-card"),
                tr("settings-selector-composition-card-desc-wall"),
            );
            builder.chips(
                tr("settings-selector-wall-mode-label"),
                tr("settings-selector-wall-mode-desc"),
                keys::selector::GRID_LAYOUT,
                &[
                    ("uniform", tr("settings-selector-wall-mode-uniform")),
                    ("brick", tr("settings-selector-wall-mode-brick")),
                    ("masonry", tr("settings-selector-wall-mode-masonry")),
                    ("justified", tr("settings-selector-wall-mode-justified")),
                    ("editorial", tr("settings-selector-wall-mode-editorial")),
                    ("cylinder", tr("settings-selector-wall-mode-cylinder")),
                ],
            );
            builder.num(
                tr("settings-selector-grid-columns-label"),
                "",
                keys::selector::GRID_COLUMNS,
                "",
            );
            builder.num(
                tr("settings-selector-grid-rows-label"),
                "",
                keys::selector::GRID_ROWS,
                "",
            );
            let wall_mode = cfg.text(keys::selector::GRID_LAYOUT);
            if matches!(wall_mode.as_str(), "brick" | "editorial") {
                builder.num(
                    tr("settings-selector-grid-stagger-label"),
                    tr("settings-selector-grid-stagger-desc"),
                    keys::selector::GRID_STAGGER,
                    "%",
                );
            }
            if wall_mode == "cylinder" {
                builder.num(
                    tr("settings-selector-grid-bend-label"),
                    tr("settings-selector-grid-bend-desc"),
                    keys::selector::GRID_CYLINDER_BEND,
                    "%",
                );
                builder.num(
                    tr("settings-selector-grid-cylinder-radius-label"),
                    tr("settings-selector-grid-cylinder-radius-desc"),
                    keys::selector::GRID_CYLINDER_RADIUS,
                    "px",
                );
            }
            builder.card(
                tr("settings-selector-cards-card"),
                tr("settings-selector-cards-card-desc"),
            );
            builder.num(
                tr("settings-selector-card-width-label"),
                "",
                keys::selector::GRID_THUMB_WIDTH,
                "px",
            );
            builder.num(
                tr("settings-selector-card-height-label"),
                "",
                keys::selector::GRID_THUMB_HEIGHT,
                "px",
            );
            builder.num(
                tr("settings-selector-grid-gap-x-label"),
                tr("settings-selector-grid-gap-x-desc"),
                keys::selector::GRID_GAP_X,
                "px",
            );
            builder.num(
                tr("settings-selector-grid-gap-y-label"),
                tr("settings-selector-grid-gap-y-desc"),
                keys::selector::GRID_GAP_Y,
                "px",
            );
            builder.toggle(
                tr("settings-selector-grid-round-label"),
                tr("settings-selector-grid-round-desc"),
                keys::selector::GRID_ROUND_CORNERS,
            );
            builder.num(
                tr("settings-selector-grid-corner-radius-label"),
                tr("settings-selector-grid-corner-radius-desc"),
                keys::selector::GRID_CORNER_RADIUS,
                "px",
            );
        }

        _ => {
            builder.card(tr("settings-selector-slice-size-card"), "");
            builder.num(
                tr("settings-selector-slice-height-label"),
                "",
                keys::selector::SLICE_HEIGHT,
                "",
            );
            builder.num(
                tr("settings-selector-visible-count-label"),
                "",
                keys::selector::VISIBLE_COUNT,
                "",
            );
            builder.num(
                tr("settings-selector-expanded-width-label"),
                "",
                keys::selector::EXPANDED_WIDTH,
                "",
            );
            builder.num(
                tr("settings-selector-slice-width-label"),
                "",
                keys::selector::SLICE_WIDTH,
                "",
            );
            builder.num(
                tr("settings-selector-slice-gap-label"),
                tr("settings-selector-slice-gap-desc"),
                keys::selector::SLICE_SPACING,
                "px",
            );
            builder.num(
                tr("settings-selector-skew-label"),
                "",
                keys::selector::SKEW_OFFSET,
                "",
            );
            builder.num(
                tr("settings-selector-edge-tilt-label"),
                tr("settings-selector-edge-tilt-desc"),
                keys::selector::SLICE_EDGE_TILT,
                "px",
            );
            builder.toggle(
                tr("settings-selector-parallax-label"),
                tr("settings-selector-parallax-desc"),
                keys::selector::SLICE_PARALLAX,
            );
            builder.toggle(
                tr("settings-selector-shadows-label"),
                tr("settings-selector-shadows-desc"),
                keys::selector::SLICE_SHADOWS,
            );
            builder.toggle(
                tr("settings-selector-wobble-label"),
                tr("settings-selector-wobble-desc"),
                keys::selector::SLICE_WOBBLE,
            );
            builder.num(
                tr("settings-selector-wobble-strength-label"),
                tr("settings-selector-wobble-strength-desc"),
                keys::selector::SLICE_WOBBLE_STRENGTH,
                "%",
            );
            builder.card(tr("settings-selector-corners-card"), "");
            builder.toggle(
                tr("settings-selector-round-corners-label"),
                tr("settings-selector-round-corners-desc"),
                keys::selector::ROUND_CORNERS,
            );
            builder.num(
                tr("settings-selector-corner-tl-label"),
                tr("settings-selector-corner-tl-desc"),
                keys::selector::CORNER_TL,
                "px",
            );
            builder.num(
                tr("settings-selector-corner-tr-label"),
                tr("settings-selector-corner-tr-desc"),
                keys::selector::CORNER_TR,
                "px",
            );
            builder.num(
                tr("settings-selector-corner-br-label"),
                tr("settings-selector-corner-br-desc"),
                keys::selector::CORNER_BR,
                "px",
            );
            builder.num(
                tr("settings-selector-corner-bl-label"),
                tr("settings-selector-corner-bl-desc"),
                keys::selector::CORNER_BL,
                "px",
            );
        }
    }
    builder.card(
        tr("settings-selector-card-flip-card"),
        tr("settings-selector-card-flip-card-desc"),
    );
    let flip_opts: Vec<(String, String)> = EFFECT_NAMES
        .iter()
        .map(|name| ((*name).to_string(), tr(effect_label_key(name)).to_string()))
        .collect();
    let flip_cur = {
        let cur = cfg.text(keys::selector::FLIP_EFFECT);
        if EFFECT_NAMES.contains(&cur.as_str()) {
            cur
        } else {
            EFFECT_NAMES[0].to_string()
        }
    };
    builder.row(
        tr("settings-selector-flip-effect-label"),
        tr("settings-selector-flip-effect-desc"),
        Control::Dropdown {
            palettes: Vec::new(),
            path: keys::selector::FLIP_EFFECT.to_string(),
            options: flip_opts,
            current: flip_cur,
        },
    );
    builder.toggle_default_true(
        tr("settings-selector-flip-shader-label"),
        tr("settings-selector-flip-shader-desc"),
        keys::selector::FLIP_SHADER,
    );
    builder.toggle_default_true(
        tr("settings-selector-flip-back-label"),
        tr("settings-selector-flip-back-desc"),
        keys::selector::FLIP_BACK_REVEAL,
    );
    builder.num(
        tr("settings-selector-flip-duration-label"),
        tr("settings-selector-flip-duration-desc"),
        keys::selector::FLIP_DURATION_MS,
        "ms",
    );
    builder.card(
        tr("settings-selector-video-preview-card"),
        tr("settings-selector-video-preview-card-desc"),
    );
    builder.toggle(
        tr("settings-selector-video-preview-label"),
        tr("settings-selector-video-preview-desc"),
        keys::video_preview::ENABLED,
    );
    builder.num(
        tr("settings-selector-preview-delay-label"),
        tr("settings-selector-preview-delay-desc"),
        keys::video_preview::DELAY_MS,
        "ms",
    );
}

fn mode_label_key(mode: &str) -> &'static str {
    match mode {
        "hex" => "settings-selector-mode-hex",
        "wall" => "settings-selector-mode-wall",
        "sandy" => "settings-selector-mode-sandy",
        "hand" => "settings-selector-mode-hand",
        "depth" => "settings-selector-mode-depth",
        "collection" => "settings-selector-mode-collection",
        _ => "settings-selector-mode-slices",
    }
}

fn swap_style_label_key(style: &str) -> &'static str {
    match style {
        "hourglass" => "settings-selector-swap-style-hourglass",
        "castle" => "settings-selector-swap-style-castle",
        "saltation" => "settings-selector-swap-style-saltation",
        "pour" => "settings-selector-swap-style-pour",
        "orbit" => "settings-selector-swap-style-orbit",
        "burst" => "settings-selector-swap-style-burst",
        "weave" => "settings-selector-swap-style-weave",
        "bloom" => "settings-selector-swap-style-bloom",
        "flock" => "settings-selector-swap-style-flock",
        "ring" => "settings-selector-swap-style-ring",
        _ => "settings-selector-swap-style-vortex",
    }
}

fn effect_label_key(name: &str) -> &'static str {
    match name {
        "Edge Fracture" => "effects-name-edge-fracture",
        "Tonal Wipe" => "effects-name-tonal-wipe",
        "Ash" => "effects-name-ash",
        "Depth Parallax" => "effects-name-depth-parallax",
        "Bokeh Bloom" => "effects-name-bokeh-bloom",
        "Light Streaks" => "effects-name-light-streaks",
        "Voxel Extrude" => "effects-name-voxel-extrude",
        "Pixel Sort" => "effects-name-pixel-sort",
        "Rack Focus" => "effects-name-rack-focus",
        "Topographic" => "effects-name-topographic",
        "Tonal Layers" => "effects-name-tonal-layers",
        _ => "effects-name-ignite",
    }
}

pub(super) fn tab_filter(builder: &mut Builder<'_>, folders: &[String]) {
    let catalog = builder.cfg.catalog();
    let tr = |key| catalog.text_static(key);

    builder.card(
        tr("settings-filter-appearance-card"),
        tr("settings-filter-appearance-card-desc"),
    );
    let visual_style = match builder
        .cfg
        .text(crate::candidate_app::view::settings::composed::keys::filter_bar::VISUAL_STYLE)
        .as_str()
    {
        "slices" | "sandy" => "slices",
        "hex" => "hex",
        "wall" => "wall",
        _ => "match",
    };
    builder.dropdown_cur(
        tr("settings-filter-visual-style-label"),
        tr("settings-filter-visual-style-desc"),
        crate::candidate_app::view::settings::composed::keys::filter_bar::VISUAL_STYLE,
        &[
            ("match", tr("settings-filter-visual-style-match")),
            ("slices", tr("settings-filter-visual-style-slices")),
            ("hex", tr("settings-filter-visual-style-hex")),
            ("wall", tr("settings-filter-visual-style-wall")),
        ],
        visual_style.to_string(),
    );
    builder.dropdown_setting(
        tr("settings-filter-orientation-label"),
        tr("settings-filter-orientation-desc"),
        crate::candidate_app::view::settings::composed::schema::setting::filter_bar::ORIENTATION,
        &[
            ("horizontal", tr("settings-filter-orientation-horizontal")),
            ("vertical", tr("settings-filter-orientation-vertical")),
        ],
    );
    builder.card(
        tr("settings-filter-type-chips-card"),
        tr("settings-filter-type-chips-card-desc"),
    );
    for key in crate::candidate_app::view::settings::composed::catalog::TYPES {
        let tkey = crate::candidate_app::view::settings::composed::catalog::type_key(key);
        builder.toggle(
            type_chip_label(catalog, key),
            "",
            &format!("filterBar.show.type.{tkey}"),
        );
    }
    builder.card(
        tr("settings-filter-sort-options-card"),
        tr("settings-filter-sort-options-card-desc"),
    );
    for (label, key) in [
        ("Newest", "date"),
        ("Recently applied · local confirmed", "recent"),
        ("Colour", "color"),
        ("Colour pop", "pop"),
        ("Richness", "richness"),
        ("Minimalist", "minimalist"),
        ("Resolution", "res"),
    ] {
        builder.toggle(
            label,
            "Show this sort in the rail, Search and keyboard cycle.",
            &format!("filterBar.show.sort.{key}"),
        );
    }
    builder.card(
        tr("settings-filter-other-controls-card"),
        tr("settings-filter-other-controls-card-desc"),
    );
    builder.toggle(
        tr("settings-filter-show-folder-label"),
        tr("settings-filter-show-folder-desc"),
        keys::filter_bar::SHOW_FOLDER,
    );
    builder.toggle(
        tr("settings-filter-show-favourites-label"),
        tr("settings-filter-show-favourites-desc"),
        keys::filter_bar::SHOW_FAVOURITES,
    );
    builder.toggle(
        tr("settings-filter-show-shape-label"),
        tr("settings-filter-show-shape-desc"),
        keys::filter_bar::SHOW_ORIENT,
    );
    builder.toggle(
        tr("settings-filter-show-playlists-label"),
        tr("settings-filter-show-playlists-desc"),
        keys::filter_bar::SHOW_PLAYLISTS,
    );
    builder.toggle(
        tr("settings-filter-show-resolution-label"),
        tr("settings-filter-show-resolution-desc"),
        keys::filter_bar::SHOW_RESOLUTION,
    );
    builder.toggle_default_true(
        tr("repair-rail-colours"),
        tr("repair-rail-hide-note"),
        "filterBar.show.colors",
    );
    builder.toggle_default_true(
        tr("repair-rail-sort"),
        tr("repair-rail-hide-note"),
        "filterBar.show.sort",
    );
    builder.toggle_default_true(
        tr("repair-rail-mute"),
        tr("repair-rail-hide-note"),
        "filterBar.show.mute",
    );
    builder.toggle_default_true(
        &format!("{} / {}", tr("repair-rail-search"), tr("repair-rail-tags")),
        tr("repair-rail-hide-note"),
        "filterBar.show.search",
    );
    builder.toggle_default_true(
        tr("repair-rail-settings"),
        tr("repair-rail-hide-note"),
        "filterBar.show.settings",
    );
    builder.toggle_default_true(
        tr("settings-filter-show-random-label"),
        tr("settings-filter-show-random-desc"),
        "filterBar.show.random",
    );
    builder.toggle_default_true(
        tr("repair-rail-close"),
        tr("repair-rail-hide-note"),
        "filterBar.show.close",
    );
    builder.card(
        tr("settings-filter-resolution-presets-card"),
        tr("settings-filter-resolution-presets-card-desc"),
    );
    builder.action_chips(
        tr("settings-filter-resolution-add-label"),
        tr("settings-filter-resolution-add-desc"),
        &[
            (
                ActionId::CreateResolutionPresetBand("fhd"),
                tr("settings-filter-resolution-band-fhd"),
            ),
            (
                ActionId::CreateResolutionPresetBand("qhd"),
                tr("settings-filter-resolution-band-qhd"),
            ),
            (
                ActionId::CreateResolutionPresetBand("4k"),
                tr("settings-filter-resolution-band-4k"),
            ),
            (
                ActionId::CreateResolutionPresetBand("5k"),
                tr("settings-filter-resolution-band-5k"),
            ),
            (
                ActionId::CreateResolutionPresetBand("8k"),
                tr("settings-filter-resolution-band-8k"),
            ),
            (
                ActionId::CreateResolutionPresetBand("custom"),
                tr("settings-filter-resolution-band-custom"),
            ),
        ],
    );
    builder.action_chips(
        tr("pk02-active-resolution"),
        &builder.cfg.text("filterBar.activeResolution"),
        &[
            (ActionId::CycleResolution, tr("pk02-next-resolution")),
            (ActionId::ClearResolution, tr("pk02-clear-resolution")),
        ],
    );
    let resolution_presets = builder.cfg.array_len(keys::filter_bar::RESOLUTION_PRESETS);
    for idx in 0..resolution_presets {
        let base = format!("{}.{idx}", keys::filter_bar::RESOLUTION_PRESETS);
        let label_path = format!("{base}.label");
        let orientation_path = format!("{base}.orientation");
        let from_path = format!("{base}.from");
        let to_path = format!("{base}.to");
        let configured_label = builder.cfg.text(&label_path);
        let title = if configured_label.trim().is_empty() {
            tr_args!(catalog, "settings-filter-preset-item-label", index => idx + 1)
        } else {
            configured_label
        };
        let from = builder.cfg.text(&from_path);
        let to = builder.cfg.text(&to_path);
        let orientation = builder.cfg.text(&orientation_path);
        let orientation_label = if orientation == "tall" {
            tr("settings-filter-preset-orientation-tall")
        } else {
            tr("settings-filter-preset-orientation-wide")
        };
        let display_bound = |value: &str| {
            crate::candidate_app::view::settings::composed::catalog::parse_resolution(value)
                .map_or_else(
                    || value.to_string(),
                    |(width, height)| {
                        let (width, height) = if orientation == "tall" {
                            (width.min(height), width.max(height))
                        } else {
                            (width.max(height), width.min(height))
                        };
                        format!("{width} × {height}")
                    },
                )
        };
        let dimensions = if to.trim().is_empty() {
            format!("{}+", display_bound(&from))
        } else {
            format!("{} - {}", display_bound(&from), display_bound(&to))
        };
        let range = format!("{orientation_label} · {dimensions}");
        let custom_id = format!("{base}.custom");
        let custom_summary = dimensions;
        builder.details(
            &title,
            tr("settings-filter-preset-item-desc"),
            base,
            range,
            |builder| {
                builder.text_field(
                    tr("settings-filter-preset-label-label"),
                    tr("settings-filter-preset-name-desc"),
                    &label_path,
                    tr("settings-filter-preset-name-placeholder"),
                );
                builder.chips(
                    tr("settings-filter-preset-orientation-label"),
                    tr("settings-filter-preset-orientation-desc"),
                    &orientation_path,
                    &[
                        ("wide", tr("settings-filter-preset-orientation-wide")),
                        ("tall", tr("settings-filter-preset-orientation-tall")),
                    ],
                );
                builder.details(
                    tr("settings-filter-resolution-band-custom"),
                    tr("settings-filter-resolution-custom-desc"),
                    custom_id,
                    custom_summary,
                    |builder| {
                        builder.resolution(
                            tr("settings-filter-preset-from-label"),
                            tr("settings-filter-preset-from-desc"),
                            &from_path,
                            tr("settings-filter-preset-resolution-width-placeholder"),
                            tr("settings-filter-preset-resolution-height-placeholder"),
                        );
                        builder.resolution(
                            tr("settings-filter-preset-to-label"),
                            tr("settings-filter-preset-to-desc"),
                            &to_path,
                            tr("settings-filter-preset-resolution-width-placeholder"),
                            tr("settings-filter-preset-resolution-height-placeholder"),
                        );
                    },
                );
                builder.action(
                    "",
                    tr("settings-filter-preset-remove-desc"),
                    ActionId::RemoveResolutionPreset(idx as u16),
                    tr("settings-filter-remove-action"),
                );
            },
        );
    }
    builder.card(
        tr("settings-filter-default-folder-card"),
        tr("settings-filter-default-folder-card-desc"),
    );
    let folder_opts = folder_dropdown_options(catalog, folders);
    let folder_refs: Vec<(&str, &str)> = folder_opts
        .iter()
        .map(|(key, label)| (key.as_str(), label.as_str()))
        .collect();
    builder.dropdown_setting(
        tr("settings-filter-default-folder-label"),
        tr("settings-filter-default-folder-desc"),
        crate::candidate_app::view::settings::composed::schema::setting::filter_bar::DEFAULT_FOLDER,
        &folder_refs,
    );
    builder.toggle_setting(
        tr("settings-filter-sticky-label"),
        tr("settings-filter-sticky-desc"),
        crate::candidate_app::view::settings::composed::schema::setting::filter_bar::STICKY,
    );
    builder.card(
        tr("settings-filter-visibility-card"),
        tr("settings-filter-visibility-card-desc"),
    );
    builder.toggle_setting(
        tr("settings-filter-always-visible-label"),
        tr("settings-filter-always-visible-desc"),
        crate::candidate_app::view::settings::composed::schema::setting::general::FILTER_BAR_ALWAYS_VISIBLE,
    );
    builder.card(
        tr("settings-filter-weather-card"),
        tr("settings-filter-weather-card-desc"),
    );
    builder.toggle(
        tr("settings-filter-weather-label"),
        tr("settings-filter-weather-desc"),
        keys::general::WEATHER_MATCH,
    );
    builder.text_field(
        tr("settings-filter-location-label"),
        tr("settings-filter-location-desc"),
        keys::general::LOCALE,
        tr("settings-filter-location-placeholder"),
    );
}

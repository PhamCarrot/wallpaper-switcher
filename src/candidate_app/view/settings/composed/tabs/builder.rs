//! Source-derived pure control projection; explicit locale and -owned consumers.
use crate::candidate_app::view::settings::composed::media::MediaKind;
use crate::candidate_app::view::settings::composed::wallpaper_kind::{STATIC, VIDEO, WE};
use crate::candidate_app::view::settings::composed::{keys, SettingsSource};

use super::launch_tab::tab_launch;
use super::media_tabs::{
    tab_paper, tab_search, tab_sources, tab_steam, tab_wallhaven, tab_wallpaper_engine,
};
use super::motion_tab::tab_motion;
use super::position_tab::tab_position;
use super::selector_tab::{tab_filter, tab_selector};
use super::system_tabs::{
    tab_general, tab_keybinds, tab_language, tab_paths, tab_performance, tab_postprocessing,
    tab_schedule,
};
use super::theme_tabs::{tab_integrations, tab_matugen, tab_niri, tab_theme};
use super::{Builder, Card, Control, Row};

pub(super) fn type_chip_label(
    catalog: &crate::candidate_app::localization::Catalog,
    kind: &str,
) -> &'static str {
    let tr = |key| catalog.text_static(key);

    match kind {
        STATIC => tr("settings-filter-type-images"),
        VIDEO => tr("settings-filter-type-videos"),
        WE => tr("settings-filter-type-we"),
        _ => tr("settings-filter-type-all"),
    }
}

pub(super) fn folder_dropdown_options(
    catalog: &crate::candidate_app::localization::Catalog,
    folders: &[String],
) -> Vec<(String, String)> {
    let tr = |key| catalog.text_static(key);

    let mut out = vec![
        (
            String::from("*"),
            tr("settings-filter-all-folders").to_string(),
        ),
        (String::new(), tr("settings-filter-main-folder").to_string()),
    ];
    for folder in folders {
        if matches!(folder.as_str(), "" | "*") {
            continue;
        }
        out.push((folder.clone(), folder.clone()));
    }
    out
}

pub fn build_tab(
    tab: &str,
    cfg: &dyn SettingsSource,
    themes: &[String],
    folders: &[String],
    analysis: &str,
    backends: &[String],
) -> Vec<(Card, Vec<Row>)> {
    build_tab_with_outputs(tab, cfg, themes, folders, analysis, backends, &[])
}

pub(crate) fn build_tab_with_outputs(
    tab: &str,
    cfg: &dyn SettingsSource,
    themes: &[String],
    folders: &[String],
    analysis: &str,
    backends: &[String],
    outputs: &[String],
) -> Vec<(Card, Vec<Row>)> {
    build_tab_with_output_statuses(
        tab,
        cfg,
        themes,
        folders,
        analysis,
        backends,
        outputs,
        &[],
        &std::collections::HashMap::new(),
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_tab_with_output_statuses(
    tab: &str,
    cfg: &dyn SettingsSource,
    themes: &[String],
    folders: &[String],
    analysis: &str,
    backends: &[String],
    outputs: &[String],
    statuses: &[crate::candidate_app::view::settings::composed::status::OutputStatus],
    previews: &std::collections::HashMap<String, String>,
) -> Vec<(Card, Vec<Row>)> {
    build_tab_with_runtime_status(
        tab, cfg, themes, folders, analysis, backends, outputs, statuses, previews, None, None,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_tab_with_runtime_status(
    tab: &str,
    cfg: &dyn SettingsSource,
    themes: &[String],
    folders: &[String],
    analysis: &str,
    backends: &[String],
    outputs: &[String],
    statuses: &[crate::candidate_app::view::settings::composed::status::OutputStatus],
    previews: &std::collections::HashMap<String, String>,
    library_watch: Option<
        &crate::candidate_app::view::settings::composed::status::LibraryWatchStatus,
    >,
    playback: Option<&crate::candidate_app::view::settings::composed::status::PlaybackStatus>,
    app_themes: Option<&crate::candidate_app::view::settings::composed::status::AppThemesResult>,
) -> Vec<(Card, Vec<Row>)> {
    match tab {
        "picker" => compose_picker(cfg, themes, outputs),
        "filter" => compose_filter_search(cfg, folders, analysis),
        "position" => compose_position(cfg),
        "displays" => Vec::new(),
        "motion" => compose_motion(cfg),
        "playback" => compose_playback(cfg, themes, outputs, playback),
        "performance" => compose_performance(cfg),
        "library" => compose_library(cfg, themes, outputs, library_watch),
        "sources" => Vec::new(),
        "automation" => compose_automation(cfg, themes, folders, outputs),
        "theme" => compose_theme(cfg, backends, app_themes),
        "integrations" => Vec::new(),
        "language" => collect(cfg, tab_language),
        _ => Vec::new(),
    }
}

fn compose_displays(
    cfg: &dyn SettingsSource,
    statuses: &[crate::candidate_app::view::settings::composed::status::OutputStatus],
    previews: &std::collections::HashMap<String, String>,
) -> Vec<(Card, Vec<Row>)> {
    let catalog = cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let mut rows = Vec::new();
    for output in statuses {
        let current = if output.kind == MediaKind::WallpaperEngine {
            output.we_id.as_str()
        } else if !output.current.is_empty() {
            output.current.as_str()
        } else {
            output.path.as_str()
        };
        let current = std::path::Path::new(current)
            .file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or(current);
        let (width, height) = output.logical_size();
        let metadata = if output.is_connected() {
            tr_args!(catalog,
                "settings-displays-monitor-desc",
                width => width,
                height => height,
                kind => output.kind.as_key()
            )
        } else {
            tr_args!(catalog,
                "settings-displays-monitor-offline-desc",
                width => width,
                height => height
            )
        };
        let fill_path = format!("{}.{name}", keys::display::FILL_MODES, name = output.name);
        let fill = if output.fill.is_empty() {
            cfg.text(keys::display::FILL_MODE)
        } else {
            output.fill.clone()
        };
        let fill_modes = super::super::tables::FILL_MODES.map(|key| {
            let label = match key {
                "fit" => "settings-paper-fill-fit",
                "stretch" => "settings-paper-fill-stretch",
                "center" => "settings-paper-fill-center",
                "tile" => "settings-paper-fill-tile",
                "span" => "settings-paper-fill-span",
                _ => "settings-paper-fill-fill",
            };
            (key.to_string(), tr(label).to_string())
        });
        let placement = Row {
            title: tr("settings-displays-placement-label").to_string(),
            desc: tr("settings-displays-placement-desc").to_string(),
            control: Control::Chips {
                path: fill_path,
                options: fill_modes.into(),
                current: fill,
                disabled: Vec::new(),
                background: Some(crate::candidate_app::view::settings::composed::background::BackgroundControl::new(
                    cfg,
                    &output.name,
                )),
            },
        };
        let lock_path = format!("{}.{name}", keys::display::OUTPUT_LOCKS, name = output.name);
        let lock = Row {
            title: tr("settings-displays-lock-label").to_string(),
            desc: tr("settings-displays-lock-desc").to_string(),
            control: Control::Toggle {
                value: cfg.flag(&lock_path),
                path: lock_path,
            },
        };
        let mut controls = vec![placement, lock];
        if output.is_connected() {
            use crate::candidate_app::view::settings::composed::display::DisplayControl;
            let mut live = vec![DisplayControl::Colours {
                target: output.target().to_string(),
                active: cfg.text(keys::display::THEME_OUTPUT) == output.name,
            }];
            if output.kind.has_audio_controls() {
                live.push(DisplayControl::Audio {
                    target: output.target().to_string(),
                    muted: output.mute,
                    volume: output.volume,
                    source_preview: false,
                });
                live.push(DisplayControl::Playback {
                    target: output.target().to_string(),
                    paused: output.paused,
                    manual: output.manual_paused,
                });
            }
            controls.extend(live.into_iter().map(|control| Row {
                title: control.label(catalog).to_string(),
                desc: String::new(),
                control: Control::Display(control),
            }));
        }
        rows.push(Row {
            title: output.name.clone(),
            desc: metadata,
            control: Control::StackBar {
                id: format!("display:{}", output.target()),
                summary: current.to_string(),
                preview: previews.get(&output.name).cloned(),
                rows: controls,
            },
        });
    }
    if rows.is_empty() {
        rows.push(Row {
            title: tr("settings-displays-empty-label").to_string(),
            desc: tr("settings-displays-empty-desc").to_string(),
            control: Control::Static,
        });
    }
    vec![(
        Card {
            title: tr("settings-displays-card"),
            subtitle: tr("settings-displays-card-desc"),
        },
        rows,
    )]
}

fn collect(
    cfg: &dyn SettingsSource,
    build: impl FnOnce(&mut Builder<'_>),
) -> Vec<(Card, Vec<Row>)> {
    let mut builder = Builder {
        cfg,
        cards: Vec::new(),
    };
    build(&mut builder);
    builder.cards
}

fn take_card(cards: &mut Vec<(Card, Vec<Row>)>, title: &str) -> Vec<Row> {
    cards
        .iter()
        .position(|(card, _)| card.title == title)
        .map_or_else(Vec::new, |index| cards.remove(index).1)
}

fn take_rows(rows: &mut Vec<Row>, titles: &[&str]) -> Vec<Row> {
    let (selected, remaining): (Vec<_>, Vec<_>) = std::mem::take(rows)
        .into_iter()
        .partition(|row| titles.contains(&row.title.as_str()));
    *rows = remaining;
    selected
}

fn section(
    cards: &mut Vec<(Card, Vec<Row>)>,
    title: &'static str,
    subtitle: &'static str,
    rows: Vec<Row>,
) {
    if !rows.is_empty() {
        cards.push((Card { title, subtitle }, rows));
    }
}

fn segmented_rows(cards: Vec<(Card, Vec<Row>)>) -> Vec<Row> {
    let mut rows = Vec::new();
    for (card, mut segment_rows) in cards {
        if segment_rows.is_empty() {
            continue;
        }
        rows.push(Row {
            title: card.title.to_string(),
            desc: card.subtitle.to_string(),
            control: Control::Segment,
        });
        rows.append(&mut segment_rows);
    }
    rows
}

fn prefixed(mut rows: Vec<Row>, prefix: &str) -> Vec<Row> {
    for row in &mut rows {
        row.title = format!("{prefix} {}", row.title);
    }
    rows
}

fn details(title: &str, desc: &str, id: &str, summary: &str, rows: Vec<Row>) -> Row {
    Row {
        title: title.to_string(),
        desc: desc.to_string(),
        control: Control::Details {
            id: id.to_string(),
            summary: summary.to_string(),
            rows,
        },
    }
}

fn enabled_status(
    catalog: &crate::candidate_app::localization::Catalog,
    enabled: bool,
) -> &'static str {
    let tr = |key| catalog.text_static(key);

    tr(if enabled {
        "settings-control-enabled"
    } else {
        "settings-control-disabled"
    })
}

fn compose_picker(
    cfg: &dyn SettingsSource,
    themes: &[String],
    outputs: &[String],
) -> Vec<(Card, Vec<Row>)> {
    let catalog = cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let mut out = Vec::new();
    let mut general = collect(cfg, |builder| tab_general(builder, themes, outputs));
    let mut selector = collect(cfg, tab_selector);
    let keys = collect(cfg, tab_keybinds);

    let mut general_rows = take_card(&mut general, tr("settings-general-general-card"));
    let windows_rows = take_rows(
        &mut general_rows,
        &[
            tr("settings-general-monitor-label"),
            tr("r5-windows-target-title"),
        ],
    );
    let mut layout = take_card(&mut selector, tr("settings-selector-layout-card"));
    general_rows.extend(take_rows(
        &mut take_card(&mut general, tr("settings-general-behaviour-card")),
        &[
            tr("settings-general-close-on-selection-label"),
            tr("settings-general-hover-selects-label"),
        ],
    ));
    general_rows.extend(take_rows(
        &mut layout,
        &[tr("settings-selector-start-position-label")],
    ));
    section(&mut out, tr("settings-section-general"), "", general_rows);

    let _filter_motion = take_rows(&mut layout, &[tr("settings-selector-filter-motion-label")]);
    layout.extend(take_card(
        &mut selector,
        tr("settings-selector-presets-card"),
    ));
    for title in [
        tr("settings-selector-composition-card"),
        tr("settings-selector-cells-card"),
        tr("settings-selector-cards-card"),
        tr("settings-selector-performance-card"),
        tr("settings-selector-sandy-card"),
        tr("settings-selector-hand-card"),
        tr("settings-selector-depth-card"),
        tr("settings-selector-mode-collection"),
        tr("settings-selector-ribbons-card"),
        tr("settings-selector-hand-effects-card"),
        tr("settings-selector-slice-size-card"),
        tr("settings-selector-corners-card"),
    ] {
        let rows = take_card(&mut selector, title);
        if rows.is_empty() {
            continue;
        }
        layout.extend(prefixed(rows, &format!("{title} ·")));
    }
    section(&mut out, tr("settings-section-layout-presets"), "", layout);

    section(
        &mut out,
        tr("settings-section-tag-panel"),
        tr("settings-section-tag-panel-note"),
        take_card(&mut selector, tr("settings-selector-tag-cloud-card")),
    );

    section(
        &mut out,
        tr("settings-section-controls"),
        tr("settings-keybinds-controls-card-desc"),
        segmented_rows(keys),
    );
    section(&mut out, tr("settings-section-windows"), "", windows_rows);
    section(
        &mut out,
        "Appearance",
        "Gallery background presentation",
        vec![
            Row {
                title: "Background presentation".into(),
                desc: "Live shows windows behind empty space and keeps gallery input.".into(),
                control: Control::Chips {
                    path: "appearance.presentation".into(),
                    options: vec![
                        ("wallpaper".into(), "Blurred wallpaper".into()),
                        ("live".into(), "Live desktop-through".into()),
                    ],
                    current: cfg.text("appearance.presentation"),
                    disabled: Vec::new(),
                    background: None,
                },
            },
            Row {
                title: "Overlay opacity".into(),
                desc: "Applies to Blurred wallpaper; saved separately from Theme colors.".into(),
                control: Control::ActionBtn {
                    id: super::ActionId::CycleBackgroundOpacity,
                    label: format!(
                        "{}%",
                        (cfg.number("appearance.overlayOpacity") * 100.0).round() as u8
                    ),
                },
            },
        ],
    );
    out
}

fn compose_filter_search(
    cfg: &dyn SettingsSource,
    folders: &[String],
    analysis: &str,
) -> Vec<(Card, Vec<Row>)> {
    let catalog = cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let mut out = Vec::new();
    let mut filter = collect(cfg, |builder| tab_filter(builder, folders));

    section(
        &mut out,
        tr("settings-section-appearance"),
        tr("settings-filter-appearance-card-desc"),
        take_card(&mut filter, tr("settings-filter-appearance-card")),
    );
    let mut buttons = take_card(&mut filter, tr("settings-filter-type-chips-card"));
    buttons.extend(take_card(
        &mut filter,
        tr("settings-filter-sort-options-card"),
    ));
    buttons.extend(take_card(
        &mut filter,
        tr("settings-filter-other-controls-card"),
    ));
    section(
        &mut out,
        tr("settings-section-buttons"),
        tr("settings-filter-type-chips-card-desc"),
        buttons,
    );

    section(
        &mut out,
        tr("settings-section-resolution-presets"),
        tr("settings-filter-resolution-presets-card-desc"),
        take_card(&mut filter, tr("settings-filter-resolution-presets-card")),
    );
    section(
        &mut out,
        tr("settings-section-visibility"),
        tr("settings-filter-visibility-card-desc"),
        take_card(&mut filter, tr("settings-filter-visibility-card")),
    );
    section(
        &mut out,
        tr("settings-filter-default-folder-card"),
        "",
        take_card(&mut filter, tr("settings-filter-default-folder-card")),
    );
    let mut search = collect(cfg, tab_search);
    let search_summary = match cfg.text(keys::tagging::DEFAULT_SEARCH_MODE).as_str() {
        "describe" => tr("settings-tagging-mode-describe"),
        _ => tr("settings-tagging-mode-tags"),
    };
    let rows = vec![details(
        tr("settings-tagging-search-card"),
        tr("settings-tagging-search-card-desc"),
        "search.discovery",
        search_summary,
        take_card(&mut search, tr("settings-tagging-search-card")),
    )];
    let mut model_rows = take_card(&mut search, tr("settings-tagging-models-card"));
    if !analysis.trim().is_empty() {
        model_rows.insert(
            0,
            Row {
                title: tr("settings-semantic-model-status-label").to_string(),
                desc: analysis.to_string(),
                control: Control::Static,
            },
        );
    }
    out
}

fn compose_position(cfg: &dyn SettingsSource) -> Vec<(Card, Vec<Row>)> {
    let catalog = cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let mut out = Vec::new();
    let mut position = collect(cfg, tab_position);
    for (title, subtitle) in [
        (
            tr("settings-position-slices-card"),
            tr("settings-position-picker-card-desc"),
        ),
        (
            tr("settings-position-hex-card"),
            tr("settings-position-picker-card-desc"),
        ),
        (
            tr("settings-position-wall-card"),
            tr("settings-position-picker-card-desc"),
        ),
        (
            tr("settings-position-sandy-card"),
            tr("settings-position-picker-card-desc"),
        ),
        (
            tr("settings-position-hand-card"),
            tr("settings-position-picker-card-desc"),
        ),
        (
            tr("settings-filter-position-card"),
            tr("settings-filter-position-card-desc"),
        ),
    ] {
        section(&mut out, title, subtitle, take_card(&mut position, title));
    }
    out
}

fn compose_motion(cfg: &dyn SettingsSource) -> Vec<(Card, Vec<Row>)> {
    let catalog = cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let mut out = Vec::new();
    let mut motion = collect(cfg, tab_motion);
    let mut launch = collect(cfg, tab_launch);
    let mut selector = collect(cfg, tab_selector);
    let mut paper = collect(cfg, tab_paper);

    section(
        &mut out,
        tr("settings-section-shared-speeds"),
        tr("settings-motion-motion-card-desc"),
        take_card(&mut motion, tr("settings-motion-motion-card")),
    );
    section(
        &mut out,
        tr("settings-section-launch"),
        tr("settings-launch-launch-card-desc"),
        take_card(&mut launch, tr("settings-launch-launch-card")),
    );
    let mut picker_motion = take_rows(
        &mut take_card(&mut selector, tr("settings-selector-layout-card")),
        &[tr("settings-selector-filter-motion-label")],
    );
    picker_motion.extend(take_card(
        &mut selector,
        tr("settings-selector-card-flip-card"),
    ));
    section(
        &mut out,
        tr("settings-section-picker-motion"),
        tr("settings-selector-card-flip-card-desc"),
        picker_motion,
    );

    let mut sandy = take_card(&mut selector, tr("settings-selector-sand-card"));
    sandy.extend(take_card(&mut selector, tr("settings-selector-ring-card")));
    out
}

fn compose_playback(
    cfg: &dyn SettingsSource,
    themes: &[String],
    outputs: &[String],
    playback: Option<&crate::candidate_app::view::settings::composed::status::PlaybackStatus>,
) -> Vec<(Card, Vec<Row>)> {
    let catalog = cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let mut out = Vec::new();
    let mut paper = collect(cfg, tab_paper);
    let mut general = collect(cfg, |builder| tab_general(builder, themes, outputs));
    let mut workshop = collect(cfg, tab_wallpaper_engine);

    let mut audio = take_card(&mut general, tr("settings-general-audio-card"));
    audio.retain(|row| !matches!(&row.control, Control::Toggle { path, .. } if path == keys::playback::MUTE_ON_OTHER_AUDIO));
    if let Some(status) = playback.filter(|_| cfg.flag(keys::playback::MUTE_ON_OTHER_AUDIO)) {
        let title = if status.audio_ducked {
            tr("settings-general-other-audio-ducked")
        } else if status.other_audio_supported {
            tr("settings-general-other-audio-listening")
        } else {
            tr("settings-general-other-audio-unavailable")
        };
        audio.push(Row {
            title: title.to_string(),
            desc: String::new(),
            control: Control::Static,
        });
    }
    section(
        &mut out,
        tr("settings-section-audio"),
        tr("settings-general-audio-card-desc"),
        audio,
    );

    out
}

fn compose_performance(cfg: &dyn SettingsSource) -> Vec<(Card, Vec<Row>)> {
    let catalog = cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let mut out = Vec::new();
    let mut performance = collect(cfg, tab_performance);
    let mut paper = collect(cfg, tab_paper);
    section(
        &mut out,
        " animation pacing",
        "",
        vec![Row {
            title: " animation pacing".into(),
            desc: "Maximum FPS / frame rate for Picker animation".into(),
            control: Control::Number {
                key: keys::general::MAX_FPS.into(),
                path: keys::general::MAX_FPS.into(),
                unit: "fps",
            },
        }],
    );

    out
}

fn compose_library(
    cfg: &dyn SettingsSource,
    themes: &[String],
    outputs: &[String],
    library_watch: Option<
        &crate::candidate_app::view::settings::composed::status::LibraryWatchStatus,
    >,
) -> Vec<(Card, Vec<Row>)> {
    let catalog = cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let mut out = Vec::new();
    let mut paths = collect(cfg, tab_paths);
    let mut performance = collect(cfg, tab_performance);
    let mut general = collect(cfg, |builder| tab_general(builder, themes, outputs));
    let mut images = take_card(&mut performance, tr("settings-performance-image-opt-card"));
    images.extend(take_card(
        &mut performance,
        tr("settings-performance-trash-card"),
    ));
    section(
        &mut out,
        tr("r5-library-paths"),
        tr("r5-config-note"),
        vec![Row {
            title: tr("r5-library-manage").into(),
            desc: tr("r5-library-manage-note").into(),
            control: Control::ActionBtn {
                id: super::ActionId::OpenLibraryTools,
                label: tr("r5-library-open").into(),
            },
        }],
    );
    out
}

fn library_watch_rows(
    cfg: &dyn SettingsSource,
    status: Option<&crate::candidate_app::view::settings::composed::status::LibraryWatchStatus>,
) -> Vec<Row> {
    let catalog = cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let (title, desc) = library_watch_message(catalog, status);
    vec![
        Row {
            title,
            desc,
            control: Control::Static,
        },
        Row {
            title: tr("settings-library-watch-fallback-label").to_string(),
            desc: tr("settings-library-watch-fallback-desc").to_string(),
            control: Control::Toggle {
                value: cfg.flag(keys::library::POLLING_FALLBACK),
                path: keys::library::POLLING_FALLBACK.to_string(),
            },
        },
        Row {
            title: tr("settings-library-watch-interval-label").to_string(),
            desc: tr("settings-library-watch-interval-desc").to_string(),
            control: Control::Number {
                key: keys::library::POLLING_INTERVAL_SECONDS.to_string(),
                path: keys::library::POLLING_INTERVAL_SECONDS.to_string(),
                unit: "s",
            },
        },
    ]
}

fn library_watch_message(
    catalog: &crate::candidate_app::localization::Catalog,
    status: Option<&crate::candidate_app::view::settings::composed::status::LibraryWatchStatus>,
) -> (String, String) {
    let tr = |key| catalog.text_static(key);

    let Some(status) = status else {
        return (
            tr("settings-library-watch-unknown-label").to_string(),
            tr("settings-library-watch-unknown-desc").to_string(),
        );
    };
    let convergence = convergence_label(catalog, status.last_successful_convergence_unix_ms);
    match status.mode.as_str() {
        crate::candidate_app::view::settings::composed::status::LibraryWatchMode::POLLING
            if !status.ok =>
        {
            (
                tr("settings-library-watch-poll-failed-label").to_string(),
                tr_args!(catalog,
                    "settings-library-watch-poll-failed-desc",
                    interval => status.interval_seconds.unwrap_or(60)
                ),
            )
        }
        crate::candidate_app::view::settings::composed::status::LibraryWatchMode::POLLING => (
            tr("settings-library-watch-polling-label").to_string(),
            tr_args!(catalog,
                "settings-library-watch-polling-desc",
                count => status.roots.iter().filter(|root| root.mode == crate::candidate_app::view::settings::composed::status::LibraryWatchMode::POLLING).count(),
                interval => status.interval_seconds.unwrap_or(60),
                budget => status.entry_budget_per_root.unwrap_or(4096),
                convergence => convergence
            ),
        ),
        crate::candidate_app::view::settings::composed::status::LibraryWatchMode::RECOVERING => (
            tr("settings-library-watch-recovering-label").to_string(),
            tr("settings-library-watch-recovering-desc").to_string(),
        ),
        crate::candidate_app::view::settings::composed::status::LibraryWatchMode::UNAVAILABLE => (
            tr("settings-library-watch-unavailable-label").to_string(),
            tr("settings-library-watch-unavailable-desc").to_string(),
        ),
        crate::candidate_app::view::settings::composed::status::LibraryWatchMode::NATIVE
            if status.last_successful_convergence_unix_ms.is_some() =>
        {
            (
                tr("settings-library-watch-recovered-label").to_string(),
                tr_args!(catalog,
                    "settings-library-watch-recovered-desc",
                    convergence => convergence
                ),
            )
        }
        crate::candidate_app::view::settings::composed::status::LibraryWatchMode::NATIVE => (
            tr("settings-library-watch-native-label").to_string(),
            tr("settings-library-watch-native-desc").to_string(),
        ),
        _ => (
            tr("settings-library-watch-unknown-label").to_string(),
            tr("settings-library-watch-unknown-desc").to_string(),
        ),
    }
}

fn convergence_label(
    catalog: &crate::candidate_app::localization::Catalog,
    timestamp_ms: Option<u64>,
) -> String {
    let tr = |key| catalog.text_static(key);

    let Some(timestamp_ms) = timestamp_ms else {
        return tr("settings-library-watch-convergence-never").to_string();
    };
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |value| {
            u64::try_from(value.as_millis()).unwrap_or(u64::MAX)
        });
    let seconds = now_ms.saturating_sub(timestamp_ms) / 1000;
    if seconds < 60 {
        tr_args!(catalog, "settings-library-watch-convergence-seconds", value => seconds)
    } else if seconds < 3600 {
        tr_args!(catalog, "settings-library-watch-convergence-minutes", value => seconds / 60)
    } else {
        tr_args!(catalog, "settings-library-watch-convergence-hours", value => seconds / 3600)
    }
}

fn compose_sources(
    cfg: &dyn SettingsSource,
    themes: &[String],
    outputs: &[String],
) -> Vec<(Card, Vec<Row>)> {
    let catalog = cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let mut out = Vec::new();
    let mut general = collect(cfg, |builder| tab_general(builder, themes, outputs));
    let mut wallhaven = collect(cfg, tab_wallhaven);
    let mut steam = collect(cfg, tab_steam);
    let mut providers = collect(cfg, tab_sources);

    let mut features = take_card(&mut general, tr("settings-general-features-card"));
    let mut wallhaven_rows = take_rows(
        &mut features,
        &[tr("settings-general-feature-wallhaven-label")],
    );
    wallhaven_rows.extend(take_card(&mut wallhaven, tr("settings-wallhaven-api-card")));
    wallhaven_rows.extend(take_card(
        &mut wallhaven,
        tr("settings-wallhaven-grid-card"),
    ));
    wallhaven_rows.extend(take_card(
        &mut wallhaven,
        tr("settings-wallhaven-thumb-card"),
    ));
    wallhaven_rows.extend(take_card(
        &mut wallhaven,
        tr("settings-source-defaults-card"),
    ));

    let mut steam_rows = take_rows(&mut features, &[tr("settings-general-feature-steam-label")]);
    steam_rows.extend(take_card(&mut steam, tr("settings-steam-backend-card")));
    steam_rows.extend(take_card(&mut steam, tr("settings-steam-paths-card")));
    steam_rows.extend(take_card(&mut steam, tr("settings-steam-grid-card")));
    steam_rows.extend(take_card(&mut steam, tr("settings-steam-thumb-card")));
    steam_rows.extend(take_card(&mut steam, tr("settings-source-defaults-card")));

    let rows = vec![
        details(
            tr("settings-sources-wallhaven-card"),
            tr("settings-sources-wallhaven-card-desc"),
            "source.wallhaven",
            enabled_status(catalog, cfg.flag(keys::features::WALLHAVEN)),
            wallhaven_rows,
        ),
        details(
            tr("settings-sources-unsplash-card"),
            tr("settings-sources-unsplash-card-desc"),
            "source.unsplash",
            enabled_status(catalog, cfg.flag(keys::sources::UNSPLASH_ENABLED)),
            take_card(&mut providers, tr("settings-sources-unsplash-card")),
        ),
        details(
            tr("settings-sources-pexels-card"),
            tr("settings-sources-pexels-card-desc"),
            "source.pexels",
            enabled_status(catalog, cfg.flag(keys::sources::PEXELS_ENABLED)),
            take_card(&mut providers, tr("settings-sources-pexels-card")),
        ),
        details(
            tr("settings-sources-bing-card"),
            tr("settings-sources-bing-card-desc"),
            "source.bing",
            enabled_status(catalog, cfg.flag(keys::sources::BING_ENABLED)),
            take_card(&mut providers, tr("settings-sources-bing-card")),
        ),
        details(
            tr("settings-sources-workshop-card"),
            tr("settings-sources-workshop-card-desc"),
            "source.workshop",
            enabled_status(catalog, cfg.flag(keys::features::STEAM)),
            steam_rows,
        ),
        details(
            tr("settings-sources-youtube-card"),
            tr("settings-sources-youtube-card-desc"),
            "source.youtube",
            enabled_status(catalog, cfg.flag(keys::sources::YOUTUBE_ENABLED)),
            take_card(&mut providers, tr("settings-sources-youtube-card")),
        ),
    ];
    section(
        &mut out,
        tr("settings-section-source-providers"),
        tr("settings-sources-providers-section-desc"),
        rows,
    );
    out
}

fn compose_automation(
    cfg: &dyn SettingsSource,
    themes: &[String],
    folders: &[String],
    outputs: &[String],
) -> Vec<(Card, Vec<Row>)> {
    let catalog = cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let mut out = Vec::new();
    let mut general = collect(cfg, |builder| tab_general(builder, themes, outputs));
    let mut schedule = collect(cfg, tab_schedule);
    let mut random = take_card(&mut general, tr("settings-general-random-card"));
    for row in &mut random {
        match &row.control {
            Control::Toggle { path, .. } if path == keys::general::RANDOM_ROTATE => {
                row.title = "Enable random rotation".into();
                row.desc = "Independent library pool on every eligible unlocked display. Enabled random waits while playlist playback is active. No immediate Apply.".into();
            }
            Control::Toggle { path, .. } if path == keys::general::RANDOM_INCLUDE_FAVOURITES => {
                row.title = "Favourites only (restriction)".into();
                row.desc = "Restrict the independent random pool to local favourites; fresh default off. Saving a choice activates the displayed rotation choices.".into();
            }
            Control::Toggle { .. } => row.desc = "Independent library pool; picker text, colour and folder filters do not apply. Save deliberately to activate displayed media choices.".into(),
            _ => {}
        }
    }
    random.insert(
        0,
        Row {
            title: "Execution policy".into(),
            desc: cfg.text("w6.policyStatus"),
            control: Control::Static,
        },
    );
    if cfg.flag("pk03.canResumeRandom") {
        random.push(Row { title: "Resume random rotation".into(),
            desc: "Explicitly clear the safety stop; wait a full interval. Resolve provider ownership first.".into(),
            control: Control::ActionBtn { id: super::ActionId::ResumeRandomRotation, label: "Resume random".into() } });
    }
    if cfg.flag("pk03.playlistPlaying") {
        random.push(Row {
            title: "Stop playlist".into(),
            desc: "Enabled random rotation resumes after a full interval. No immediate Apply."
                .into(),
            control: Control::ActionBtn {
                id: super::ActionId::StopPlaylist,
                label: "Stop playlist".into(),
            },
        });
    }
    section(&mut out, tr("settings-section-random-rotation"), "", random);
    let restore = take_card(&mut general, tr("settings-general-behaviour-card")).into_iter()
        .filter(|row| matches!(&row.control, Control::Toggle { path, .. } if path == keys::system::RESTORE_ON_STARTUP))
        .map(|mut row| { row.title = "Reapply confirmed assignments on startup (opt-in)".into();
            row.desc = "Off by default. Uses last confirmed per-display item/source; unavailable or restricted assignments are skipped. Separate from timer resumption and startup registration. Preview never applies.".into(); row }).collect();
    section(
        &mut out,
        "Startup reapply",
        "Does not start Theme following, post-processing, notifications or helpers.",
        restore,
    );
    let mut schedules = take_card(&mut schedule, tr("settings-schedule-schedule-card"));
    schedules.retain(|row| {
        matches!(
            &row.control,
            Control::ActionBtn {
                id: super::ActionId::OpenScheduleEditor,
                ..
            }
        )
    });
    section(
        &mut out,
        tr("settings-section-schedule"),
        tr("settings-schedule-schedule-card-desc"),
        schedules,
    );
    out
}

fn compose_theme(
    cfg: &dyn SettingsSource,
    backends: &[String],
    apps: Option<&crate::candidate_app::view::settings::composed::status::AppThemesResult>,
) -> Vec<(Card, Vec<Row>)> {
    let catalog = cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let mut out = Vec::new();
    section(
        &mut out,
        tr("settings-theme-designer-label"),
        tr("settings-theme-profile-desc"),
        vec![Row {
            title: tr("settings-theme-designer-label").into(),
            desc: tr("settings-theme-designer-desc").into(),
            control: Control::ActionBtn {
                id: super::ActionId::OpenThemeDesigner,
                label: tr("settings-theme-designer-action").into(),
            },
        }],
    );

    out
}

fn compose_integrations(cfg: &dyn SettingsSource, themes: &[String]) -> Vec<(Card, Vec<Row>)> {
    let mut out = collect(cfg, tab_integrations);
    if cfg.is_niri() {
        out.extend(collect(cfg, |builder| tab_niri(builder, themes)));
    }
    out
}

/// Explicit compatibility-owner fixture made from maintained raw row producers.
/// It is not public UI evidence and cannot exist in a normal binary.
#[cfg(test)]
pub(crate) fn compatibility_owner_rows(
    tab: &str,
    cfg: &dyn SettingsSource,
    outputs: &[String],
    statuses: &[crate::candidate_app::view::settings::composed::status::OutputStatus],
    previews: &std::collections::HashMap<String, String>,
) -> Vec<(Card, Vec<Row>)> {
    let mut rows = Vec::new();
    match tab {
        "filter" => {
            rows.extend(collect(cfg, |b| tab_filter(b, &[])));
            rows.extend(collect(cfg, tab_search));
        }
        "playback" => {
            rows.extend(collect(cfg, |b| tab_general(b, &[], outputs)));
            rows.extend(collect(cfg, tab_paper));
        }
        "automation" => {
            rows.extend(collect(cfg, tab_schedule));
            rows.extend(collect(cfg, tab_postprocessing));
            rows.extend(compose_displays(cfg, statuses, previews));
        }
        "theme" => {
            rows.extend(collect(cfg, |b| tab_theme(b, &[])));
            rows.extend(collect(cfg, tab_matugen));
            rows.extend(compose_integrations(cfg, &[]));
        }
        "library" => {
            rows.extend(compose_sources(cfg, &[], outputs));
            rows.extend(collect(cfg, tab_performance));
        }
        "motion" => {
            rows.extend(collect(cfg, tab_motion));
            rows.extend(collect(cfg, tab_paper));
        }
        _ => return build_tab(tab, cfg, &[], &[], "", &[]),
    }
    rows
}

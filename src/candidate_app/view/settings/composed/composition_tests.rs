use super::test_source::FakeSettingsSource;
use super::*;

fn cards(tab: &str, source: &FakeSettingsSource) -> Vec<(Card, Vec<Row>)> {
    build_tab_with_runtime_status(
        tab,
        source,
        &[],
        &[],
        "",
        &[],
        &[],
        &[],
        &Default::default(),
        None,
        None,
        None,
    )
}

fn paths(rows: &[Row]) -> Vec<&str> {
    let mut result = Vec::new();
    for row in rows {
        match &row.control {
            Control::Toggle { path, .. }
            | Control::Dropdown { path, .. }
            | Control::Chips { path, .. }
            | Control::Number { path, .. }
            | Control::TextField { path, .. }
            | Control::Resolution { path, .. }
            | Control::KeyBinding { path, .. } => result.push(path.as_str()),
            Control::Details { rows, .. } | Control::StackBar { rows, .. } => {
                result.extend(paths(rows))
            }
            Control::MotionWeights { weights } => {
                result.extend(weights.iter().map(|(_, key, _)| key.as_str()))
            }
            _ => {}
        }
    }
    result
}
fn all_paths(cards: &[(Card, Vec<Row>)]) -> Vec<&str> {
    cards.iter().flat_map(|(_, rows)| paths(rows)).collect()
}
fn titles<'a>(cards: &'a [(Card, Vec<Row>)]) -> Vec<&'static str> {
    cards.iter().map(|(card, _)| card.title).collect()
}

#[test]
fn actual_categories_build_source_sections_and_only_four_public_modes() {
    let source = FakeSettingsSource::default();
    let tabs = visible_tabs(&source);
    assert_eq!(
        tabs.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
        [
            "picker",
            "filter",
            "position",
            "displays",
            "motion",
            "playback",
            "performance",
            "library",
            "automation",
            "theme",
            "language"
        ]
    );
    for (id, _) in tabs {
        let built = cards(id, &source);
        assert!(!built.is_empty(), "{id} has no actual composed sections");
        assert!(
            built.iter().all(|(_, rows)| !rows.is_empty()),
            "{id} contains placeholder section"
        );
        println!(
            "inventory/{id}: {} sections, {} actual setting paths",
            built.len(),
            all_paths(&built).len()
        );
    }
    for mode in ["slices", "hex", "wall", "depth"] {
        let source = FakeSettingsSource::default().with_text(keys::selector::DISPLAY_MODE, mode);
        let picker = cards("picker", &source);
        assert_eq!(
            titles(&picker),
            [
                "General",
                "Layout & presets",
                "Controls",
                "Windows",
                "Appearance"
            ]
        );
        let chips = picker
            .iter()
            .flat_map(|(_, rows)| rows)
            .find_map(|row| match &row.control {
                Control::Chips { path, options, .. } if path == keys::selector::DISPLAY_MODE => {
                    Some(options)
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(
            chips.iter().map(|(id, _)| id.as_str()).collect::<Vec<_>>(),
            ["slices", "depth", "hex", "wall"]
        );
        assert!(all_paths(&picker).iter().all(|path| !path.contains("sandy")
            && !path.contains("hand.")
            && !path.contains("collection")));
        assert_eq!(
            titles(&picker)[1],
            source
                .catalog()
                .text_static("settings-section-layout-presets")
        );
    }
}

#[test]
fn p1_copy_contract_uses_pacing_and_random_rotation_language() {
    let source = FakeSettingsSource::default();
    let performance = cards("performance", &source);
    let pacing = performance
        .iter()
        .flat_map(|(_, rows)| rows)
        .find(|row| row.title == "Animation pacing")
        .expect("pacing row");
    assert_eq!(pacing.desc, "Maximum frame rate for picker motion.");

    let automation = cards("automation", &source);
    let mut rows = automation.iter().flat_map(|(_, rows)| rows);
    let random = rows
        .find(|row| row.title == "Enable random rotation")
        .expect("random rotation row");
    assert!(random.desc.is_empty());
    assert!(automation[0]
        .1
        .iter()
        .all(|row| row.desc.is_empty() && row.title != "Execution policy"));
}

#[test]
fn moved_motion_and_removed_performance_duplicates_follow_composition() {
    let source = FakeSettingsSource::default();
    let picker = cards("picker", &source);
    assert!(!paths(&picker[1].1).contains(&keys::motion::FILTER_SWAP_SPEED));
    let motion = cards("motion", &source);
    assert_eq!(
        all_paths(&motion)
            .iter()
            .filter(|path| **path == keys::motion::FILTER_SWAP_SPEED)
            .count(),
        1
    );
    assert_eq!(
        all_paths(&motion)
            .iter()
            .filter(|path| **path == keys::selector::FLIP_EFFECT)
            .count(),
        1
    );
    let performance = cards("performance", &source);
    let performance_paths = all_paths(&performance);
    assert!(performance_paths.contains(&keys::general::MAX_FPS));
    assert!(!performance_paths.contains(&keys::transition::PREVIEW_FPS));
    assert!(!performance_paths.contains(&keys::selector::LIVE_PREVIEW));
    let position = cards("position", &source);
    assert_eq!(position.len(), 5);
    assert!(all_paths(&position).contains(&keys::selector::DEPTH_STAGE_X));
    assert!(all_paths(&position).contains(&keys::selector::DEPTH_STAGE_Y));
    assert!(all_paths(&position)
        .iter()
        .all(|path| !path.contains("sandy") && !path.contains("hand")));
    let public: Vec<_> = visible_tabs(&source)
        .iter()
        .flat_map(|(tab, _)| {
            let model = cards(tab, &source);
            all_paths(&model)
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    for absent in [
        keys::general::APPLY_ON_PICKER_MONITOR,
        keys::general::NOTIFY_ON_WALLPAPER_CHANGE,
        "filterBar.show.tagcloud",
        keys::selector::TAG_CLOUD_ROWS,
        keys::selector::TAG_CLOUD_OFFSET_X,
        keys::selector::TAG_CLOUD_OFFSET_Y,
        keys::selector::TAG_CLOUD_WIDTH,
        keys::schedule::ENABLED,
        keys::schedule::APPLY_ON_START,
    ] {
        assert!(
            !public.iter().any(|path| path == absent),
            "inactive public row {absent}"
        );
    }
    // W6 exposes these only after wiring typed, explicitly saved execution policy.
    for delivered in [
        keys::system::RESTORE_ON_STARTUP,
        keys::general::RANDOM_INCLUDE_STATIC,
        keys::general::RANDOM_INCLUDE_VIDEO,
        keys::general::RANDOM_INCLUDE_WE,
        keys::general::RANDOM_INCLUDE_FAVOURITES,
        keys::filter_bar::DEFAULT_FOLDER,
        keys::filter_bar::STICKY,
    ] {
        assert_eq!(
            public
                .iter()
                .filter(|path| path.as_str() == delivered)
                .count(),
            1,
            "delivered W3 control {delivered}"
        );
    }
    assert!(public
        .iter()
        .any(|path| path == keys::general::RANDOM_ROTATE));
    assert!(public
        .iter()
        .any(|path| path == keys::general::RANDOM_INTERVAL));
}

#[test]
fn sources_remain_expandable_and_false_execution_preferences_do_not_remove_local_forms() {
    let source = FakeSettingsSource::default()
        .with_flag(keys::features::WALLHAVEN, false)
        .with_flag(keys::features::STEAM, false);
    assert!(cards("sources", &source).is_empty());
    assert!(cards("displays", &source)[0]
        .1
        .iter()
        .any(|row| matches!(row.control, Control::PlatformForm)));
    assert!(cards("integrations", &source).is_empty());
    assert!(!all_paths(&cards("playback", &source)).contains(&keys::we_render::FPS));
}

#[test]
fn e3_retired_transitions_never_compose_and_theme_controls_remain_conditional() {
    for locale in ["en-US", "zh-CN"] {
        for enabled in [false, true] {
            let source = FakeSettingsSource::default()
                .with_locale(locale)
                .with_flag(keys::transition::ENABLED, enabled)
                .with_text(keys::transition::SHADER, "sand-donut");
            let motion = cards("motion", &source);
            assert_eq!(
                motion.len(),
                4,
                "shared, launch, Picker and wallpaper transition motion"
            );
            let paths = all_paths(&motion);
            assert!(paths.contains(&keys::launch::ANIMATION));
            assert!(paths.contains(&keys::selector::FLIP_DURATION_MS));
            assert!(paths.contains(&keys::wallpaper_transition::ENABLED));
            assert!(paths.contains(&keys::wallpaper_transition::DURATION_MS));
            assert!(paths.contains(&keys::wallpaper_transition::FAMILY));
            let transition_rows = motion.iter().flat_map(|(_, rows)| rows);
            let family = transition_rows
                .clone()
                .find_map(|row| match &row.control {
                    Control::Chips {
                        path,
                        options,
                        disabled,
                        ..
                    } if path == keys::wallpaper_transition::FAMILY => Some((options, disabled)),
                    _ => None,
                })
                .expect("wallpaper transition family chips");
            assert_eq!(
                family
                    .0
                    .iter()
                    .map(|(key, _)| key.as_str())
                    .collect::<Vec<_>>(),
                ["random", "fade", "wipe", "warp", "break"]
            );
            assert!(family.1.is_empty());
            assert!(transition_rows.clone().any(|row| matches!(&row.control,
                Control::Dropdown { path, options, .. } if path == keys::launch::ANIMATION
                    && options.iter().map(|(key, _)| key.as_str()).eq(["none", "fade", "rise", "zoom"])
            )));
            assert!(transition_rows.clone().any(|row| {
                matches!(
                    &row.control,
                    Control::Dropdown { path, options, .. }
                        if path == keys::wallpaper_transition::EFFECT
                            && options.iter().any(|(key, _)| key == "crossfade")
                )
            }));
            for (tab, _) in visible_tabs(&source) {
                assert!(all_paths(&cards(tab, &source))
                    .iter()
                    .all(|path| !path.starts_with("transition.")));
                assert!(all_paths(&compatibility_owner_rows(
                    tab,
                    &source,
                    &[],
                    &[],
                    &Default::default()
                ))
                .iter()
                .all(|path| !path.starts_with("transition.")));
            }
            for query in [
                "transition.shader",
                "sand-donut",
                "previewFps",
                "shaderScopes",
            ] {
                assert!(
                    search::search_composed(query, &source, |tab| cards(tab, &source)).is_empty(),
                    "retired search {locale}: {query}"
                );
            }
            let retained_shader_query = if locale == "zh-CN" {
                "着色器"
            } else {
                "shader"
            };
            let hits =
                search::search_composed(retained_shader_query, &source, |tab| cards(tab, &source));
            assert!(
                hits.iter()
                    .any(|hit| hit.tab == "motion" && hit.section == 2),
                "retained card-flip shader remains searchable"
            );
            assert!(hits
                .iter()
                .all(|hit| hit.tab == "motion" && matches!(hit.section, 2 | 3)));
        }
    }
    let fixed = FakeSettingsSource::default().with_text(keys::theme::POLICY, "fixed");
    let fixed = cards("theme", &fixed);
    assert!(!all_paths(&fixed).contains(&keys::theme::AUTHORITY));
    let generated = FakeSettingsSource::default()
        .with_text(keys::theme::POLICY, "wallpaper")
        .with_text(keys::theme::AUTHORITY, "skwd")
        .with_text(keys::theme::ENGINE, "matugen");
    let generated = cards("theme", &generated);
    assert!(!all_paths(&generated).contains(&keys::theme::AUTHORITY));
    assert!(!all_paths(&generated).contains(&keys::matugen::SCHEME_TYPE));
    assert!(!all_paths(&fixed).contains(&keys::system::EXTERNAL_MATUGEN_COMMAND));
}

#[test]
fn independently_localized_composition_never_changes_another_catalog() {
    let english = std::thread::spawn(|| {
        let source = FakeSettingsSource::default();
        let first = titles(&cards("picker", &source));
        for _ in 0..4 {
            assert_eq!(first, titles(&cards("picker", &source)));
        }
        first
    });
    let japanese = std::thread::spawn(|| {
        let source = FakeSettingsSource::default().with_locale("zh-CN");
        let first = titles(&cards("picker", &source));
        for _ in 0..4 {
            assert_eq!(first, titles(&cards("picker", &source)));
        }
        first
    });
    assert_ne!(english.join().unwrap(), japanese.join().unwrap());
}

#[test]
fn preset_editor_and_resolution_parser_retain_source_identity_and_notation() {
    assert_eq!(
        PRESET_NAME_KEY,
        "components.wallpaperSelector.presetDraftName"
    );
    let source = FakeSettingsSource::default();
    let picker = cards("picker", &source);
    assert!(paths(&picker[1].1).contains(&PRESET_NAME_KEY));
    for value in ["1920x1080", "1920X1080", "1920×1080", " 1920 × 1080 "] {
        assert_eq!(catalog::parse_resolution(value), Some((1920, 1080)));
    }
    assert_eq!(
        catalog::parse_resolution("4294967296x1080"),
        Some((4294967296, 1080))
    );
    for value in [
        "0x1080",
        "-1x1080",
        "1920x0",
        "1920",
        "1920xx1080",
        "NaNx1080",
    ] {
        assert_eq!(catalog::parse_resolution(value), None);
    }
}

#[test]
fn composed_search_keeps_six_result_cap_and_exact_nested_target_identity() {
    let source = FakeSettingsSource::default();
    let hits = search::search_settings("animation pacing", &source, &[], &[], "", &[], None);
    assert!(!hits.is_empty());
    assert!(hits.len() <= 6);
    let hit = hits
        .iter()
        .find(|hit| hit.tab == "performance")
        .expect("real source renderer cap");
    let actual = cards("performance", &source);
    assert_eq!(actual[hit.section].0.title, hit.section_title);
    let row = &actual[hit.section].1[hit.row];
    assert!(matches!(&row.control,Control::Number{path,..} if path==keys::general::MAX_FPS));
    let nested = search::search_settings("wallhaven api", &source, &[], &[], "", &[], None);
    assert!(
        nested.is_empty(),
        "removed providers must not remain searchable"
    );
    assert!(search::search_settings(
        "zz-no-such-visible-control",
        &source,
        &[],
        &[],
        "",
        &[],
        None
    )
    .is_empty());
    assert!(search::search_settings("sandy grain", &source, &[], &[], "", &[], None).is_empty());
}

#[test]
fn feedback_optional_rail_controls_are_public_and_badge_setting_is_retired() {
    let source = FakeSettingsSource::default();
    let filter = cards("filter", &source);
    let paths = all_paths(&filter);
    for key in [
        "colors", "sort", "mute", "search", "settings", "random", "close",
    ] {
        assert!(
            paths.contains(&format!("filterBar.show.{key}").as_str()),
            "missing hide control {key}"
        );
    }
    assert!(!paths.contains(&"filterBar.show.display"));
    assert!(!paths.contains(&"filterBar.show.tagcloud"));
    assert!(!paths.contains(&"filterBar.show.theme"));
    assert!(visible_tabs(&source).iter().any(|(id, _)| *id == "theme"));
    assert_eq!(
        paths
            .iter()
            .filter(|path| **path == "filterBar.show.search")
            .count(),
        1
    );
    assert!(!all_paths(&cards("picker", &source)).contains(&keys::selector::SHOW_TYPE_BADGES));
}

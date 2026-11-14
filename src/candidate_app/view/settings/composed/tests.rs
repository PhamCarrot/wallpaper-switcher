use super::*;
use std::collections::HashSet;

#[test]
fn source_descriptor_inventory_binds_retained_actions_to_distinct_editable_paths() {
    assert_eq!(KEY_BINDINGS.len(), 33);
    assert_eq!(input::InputAction::ALL.len(), 45);
    assert!(KEY_BINDINGS.iter().all(|entry| entry.path != "keys.studio"));
    assert!(schema::find("keys.studio").is_none());
    let mut paths = HashSet::new();
    for entry in KEY_BINDINGS {
        assert!(paths.insert(entry.path));
        assert!(!entry.title_key.is_empty());
        assert!(input::parse_binding(entry.action.default_binding()).is_some());
    }
    assert_eq!(KeyBindingGroup::ALL.len(), 6);
}

#[test]
fn output_observation_requires_explicit_connection_and_stable_target() {
    let mut output = status::OutputStatus::default();
    assert!(!output.is_connected());
    output.name = "Display 1".into();
    output.connected = true;
    assert!(
        !output.is_connected(),
        "a display label is not stable target identity"
    );
    output.target = r"\\?\DISPLAY#fixture-physical-id".into();
    assert!(output.is_connected());
    assert_eq!(output.target(), r"\\?\DISPLAY#fixture-physical-id");
    output.connected = false;
    assert!(!output.is_connected());
}

#[test]
fn presentation_metadata_retains_actual_defaults_and_numeric_bounds() {
    let scale = schema::find(keys::general::UI_SCALE).unwrap();
    assert_eq!(scale.default, schema::DefaultValue::Number(1.0));
    assert_eq!(
        scale.number_bounds,
        Some(schema::NumberBounds { min: 0.5, max: 2.0 })
    );
    assert_eq!(schema::number_default(keys::motion::FAST_MS), Some(180.0));
    assert_eq!(
        schema::number_default(keys::motion::STANDARD_MS),
        Some(250.0)
    );
    assert_eq!(schema::number_default(keys::motion::SLOW_MS), Some(450.0));
    assert_eq!(schema::number_default(keys::general::MAX_FPS), Some(120.0));
    assert_eq!(
        schema::boolean_default(keys::general::RANDOM_ROTATE),
        Some(false)
    );
    assert!(schema::find("invented.control").is_none());
}

#[test]
fn settings_cleanup_public_performance_and_search_keep_pacing_without_device_rows() {
    use crate::candidate_app::view::settings::composed;
    for locale in ["en-US", "zh-CN"] {
        let source = composed::test_source::FakeSettingsSource::default().with_locale(locale);
        let cards = composed::build_tab("performance", &source, &[], &[], "", &[]);
        let rows = cards.iter().flat_map(|(_, rows)| rows).collect::<Vec<_>>();
        assert_eq!(
            rows.len(),
            1,
            "preserve the actual existing public controls"
        );
        assert!(
            matches!(&rows[0].control, Control::Number { path, .. } if path == composed::keys::general::MAX_FPS)
        );
        for query in [
            "Renderer",
            "device",
            source
                .catalog()
                .text_static("settings-performance-device-label"),
            source
                .catalog()
                .text_static("settings-performance-we-renderer-label"),
        ] {
            let results =
                composed::search::search_settings(query, &source, &[], &[], "", &[], None);
            assert!(
                !results.iter().any(|result| result.tab == "performance"),
                "{locale}/{query}"
            );
        }
        assert!(
            composed::search::search_settings("FPS", &source, &[], &[], "", &[], None)
                .iter()
                .any(|result| result.tab == "performance")
        );
        let compatible = composed::compatibility_owner_rows(
            "library",
            &source,
            &[],
            &[],
            &std::collections::HashMap::new(),
        );
        let rows = compatible
            .iter()
            .flat_map(|(_, rows)| rows)
            .collect::<Vec<_>>();
        assert!(!rows.iter().any(|row| matches!(&row.control, Control::Dropdown {path,..} if path == composed::keys::performance::GPU_DEVICE)));
        assert!(rows.iter().any(|row| matches!(&row.control, Control::Toggle {path,..} if path == composed::keys::video_preview::ENABLED)));
        assert!(rows.iter().any(|row| matches!(&row.control, Control::Number {path,..} if path == composed::keys::general::MAX_FPS)));
        assert!(!rows.iter().any(|row| row.title
            == source
                .catalog()
                .text_static("settings-performance-we-renderer-label")));
    }
}

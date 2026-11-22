use super::*;
use crate::candidate_app::view::settings::composed::test_source::FakeSettingsSource;

#[test]
fn retired_tag_rows_are_absent_from_settings_search() {
    let cfg = FakeSettingsSource::default();
    for query in ["Clickable tag rows", "tagCloudRows"] {
        let results = search_settings(query, &cfg, &[], &[], "", &[], None);
        assert!(results.is_empty(), "{query}: {results:?}");
    }
}

#[test]
fn search_aliases_across_tabs() {
    let cfg = FakeSettingsSource::default();
    let empty = Vec::new();
    let battery = search_settings("laptop power", &cfg, &empty, &empty, "", &empty, None);
    assert!(battery.is_empty());

    let frame_rate = search_settings("frame rate", &cfg, &empty, &empty, "", &empty, None);
    assert!(frame_rate
        .iter()
        .any(|result| result.title.to_lowercase().contains("animation pacing")));
    let engine = search_settings("we workshop", &cfg, &empty, &empty, "", &empty, None);
    assert!(engine.iter().all(|result| result.tab != "sources"));
}

#[test]
fn blank_search_empty() {
    let cfg = FakeSettingsSource::default();
    assert!(search_settings("   ", &cfg, &[], &[], "", &[], None).is_empty());
}

#[test]
fn nested_source_controls_match_case_insensitive_search() {
    let cfg = FakeSettingsSource::default();
    for query in [
        "show apply button",
        "SHOW APPLY BUTTON",
        "Show Apply button",
    ] {
        let results = search_settings(query, &cfg, &[], &[], "", &[], None);
        assert!(results.is_empty(), "{query}");
        assert!(results.iter().all(|result| result.tab == "sources"));
    }
}

#[test]
fn turkish_search_matches_dotted_and_dotless_i() {
    for (title, query) in [
        ("İndirmeler", "indirmeler"),
        ("Işık", "ışık"),
        ("IŞIK", "ışık"),
        ("İNDİRMELER", "indirmeler"),
    ] {
        assert_eq!(normalized(title), normalized(query), "{title}: {query}");
    }
}

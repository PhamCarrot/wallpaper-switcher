//! Local Cargo integration-test harness for the integrated
//! `theme_roles` module.
//!
//! This is the locally validated pilot harness (26 tests, all passed at
//! pilot intake) carried into the session-theme integration slice with
//! its assertions unchanged, plus the sibling `rgb_color` declaration
//! the integrated module now requires and a small set of RGB-owner
//! regression tests. Run locally with
//! `cargo test --locked --offline --test theme_roles_contract`. A
//! zero-test run is a harness failure, not a pass.
//!
//! The `#[path]` declarations below are the harness: they make the
//! production modules part of this test target, so selecting this
//! target actually compiles and exercises them.
//!
//! All fixtures are small, synthetic values generated in this file. They
//! are NOT captured Matugen output and no fixture asserts that any
//! generator agrees with itself.

#[path = "../src/rgb_color.rs"]
mod rgb_color;

#[path = "../src/theme_roles.rs"]
mod theme_roles;

use std::collections::HashSet;
use std::str::FromStr;

use serde_json::{json, Map, Value};
use theme_roles::{
    import_document, normalize_hex_color, parse_hex_rgb_bytes, ColorVariant, ImportOutcome,
    InvalidColor, RoleColors, ThemeDocument, ThemeImportError, ThemeRole, ROLE_COUNT, ROLE_KEYS,
    SOURCE_ROLE,
};

/// The 50 donor role keys, written out independently in this test file
/// (transcribed from `contracts/theme-role-keys.json` / the donor
/// `palette.rs`), so the module's own constant is checked against a
/// second source rather than against itself.
const EXPECTED_KEYS: [&str; 50] = [
    "primary",
    "on_primary",
    "tertiary",
    "surface",
    "on_surface",
    "surface_variant",
    "surface_container",
    "background",
    "outline",
    "error",
    "error_container",
    "inverse_on_surface",
    "inverse_primary",
    "inverse_surface",
    "on_background",
    "on_error",
    "on_error_container",
    "on_primary_container",
    "on_primary_fixed",
    "on_primary_fixed_variant",
    "on_secondary",
    "on_secondary_container",
    "on_secondary_fixed",
    "on_secondary_fixed_variant",
    "on_surface_variant",
    "on_tertiary",
    "on_tertiary_container",
    "on_tertiary_fixed",
    "on_tertiary_fixed_variant",
    "outline_variant",
    "primary_container",
    "primary_fixed",
    "primary_fixed_dim",
    "scrim",
    "secondary",
    "secondary_container",
    "secondary_fixed",
    "secondary_fixed_dim",
    "shadow",
    "surface_bright",
    "surface_container_high",
    "surface_container_highest",
    "surface_container_low",
    "surface_container_lowest",
    "surface_dim",
    "surface_tint",
    "tertiary_container",
    "tertiary_fixed",
    "tertiary_fixed_dim",
    "source_color",
];

/// Synthetic dark color for the role at `index`: distinct per role.
fn dark_hex(index: usize) -> String {
    format!("#{index:02x}1122")
}

/// Synthetic light color for the role at `index`: distinct per role and
/// different from that role's dark color.
fn light_hex(index: usize) -> String {
    format!("#{index:02x}ddee")
}

/// A complete, valid, donor-shaped document built from EXPECTED_KEYS.
fn valid_document_value() -> Value {
    let mut colors = Map::new();
    for (index, key) in EXPECTED_KEYS.iter().enumerate() {
        let mut role_node = Map::new();
        role_node.insert("dark".to_string(), json!({ "color": dark_hex(index) }));
        role_node.insert("light".to_string(), json!({ "color": light_hex(index) }));
        colors.insert((*key).to_string(), Value::Object(role_node));
    }
    let mut root = Map::new();
    root.insert("colors".to_string(), Value::Object(colors));
    Value::Object(root)
}

fn colors_mut(value: &mut Value) -> &mut Map<String, Value> {
    value
        .get_mut("colors")
        .and_then(Value::as_object_mut)
        .expect("fixture has a colors object")
}

fn role_node_mut<'a>(value: &'a mut Value, key: &str) -> &'a mut Map<String, Value> {
    colors_mut(value)
        .get_mut(key)
        .and_then(Value::as_object_mut)
        .expect("fixture role node is an object")
}

fn set_color(value: &mut Value, key: &str, variant: &str, color: Value) {
    let role_node = role_node_mut(value, key);
    let variant_node = role_node
        .get_mut(variant)
        .and_then(Value::as_object_mut)
        .expect("fixture variant node is an object");
    variant_node.insert("color".to_string(), color);
}

fn import_valid() -> ImportOutcome {
    import_document(&valid_document_value()).expect("synthetic fixture must import")
}

// ---------------------------------------------------------------------
// Role catalog: exact keys, uniqueness, name/index conversion.
// ---------------------------------------------------------------------

#[test]
fn role_keys_match_the_independent_expected_list_exactly() {
    assert_eq!(ROLE_COUNT, 50);
    assert_eq!(ROLE_KEYS.len(), 50);
    assert_eq!(ROLE_KEYS, EXPECTED_KEYS);
    assert_eq!(SOURCE_ROLE, "source_color");
    assert_eq!(ROLE_KEYS[49], SOURCE_ROLE);
}

#[test]
fn role_keys_are_unique() {
    let unique: HashSet<&&str> = ROLE_KEYS.iter().collect();
    assert_eq!(unique.len(), 50);
}

#[test]
fn name_and_index_conversion_round_trips_for_every_role() {
    assert_eq!(ThemeRole::ALL.len(), ROLE_COUNT);
    for (index, key) in EXPECTED_KEYS.iter().enumerate() {
        let role = ThemeRole::ALL[index];
        assert_eq!(role.index(), index, "{key}");
        assert_eq!(role.name(), *key);
        assert_eq!(ThemeRole::from_name(key), Some(role), "{key}");
        assert_eq!(ThemeRole::from_index(index), Some(role), "{key}");
        assert_eq!(ThemeRole::from_str(key), Ok(role), "{key}");
        assert_eq!(role.to_string(), *key);
    }
    // First and last roles pinned explicitly.
    assert_eq!(ThemeRole::ALL[0], ThemeRole::Primary);
    assert_eq!(ThemeRole::ALL[49], ThemeRole::SourceColor);
    assert_eq!(ThemeRole::SourceColor.name(), "source_color");
}

#[test]
fn unknown_role_names_and_indexes_are_rejected() {
    for bad in [
        // The donor UI_KEYS camelCase names are not role keys.
        "primaryText",
        "surfaceText",
        "surfaceVariant",
        "surfaceContainer",
        "sourceColor",
        // Case, whitespace and separator lookalikes are not folded.
        "Primary",
        "PRIMARY",
        " primary",
        "primary ",
        "on-primary",
        "on primary",
        // Near misses.
        "",
        "source",
        "base00",
        "default",
    ] {
        assert_eq!(ThemeRole::from_name(bad), None, "{bad:?}");
        assert!(ThemeRole::from_str(bad).is_err(), "{bad:?}");
    }
    assert_eq!(ThemeRole::from_index(50), None);
    assert_eq!(ThemeRole::from_index(usize::MAX), None);
}

#[test]
fn variant_names_are_exactly_dark_and_light() {
    assert_eq!(ColorVariant::Dark.name(), "dark");
    assert_eq!(ColorVariant::Light.name(), "light");
    assert_eq!(ColorVariant::ALL, [ColorVariant::Dark, ColorVariant::Light]);
    assert_eq!(ColorVariant::from_name("dark"), Some(ColorVariant::Dark));
    assert_eq!(ColorVariant::from_name("light"), Some(ColorVariant::Light));
    assert_eq!(ColorVariant::Dark.other(), ColorVariant::Light);
    assert_eq!(ColorVariant::Light.other(), ColorVariant::Dark);
    // The donor's derived "default" entry is not a variant.
    for bad in ["default", "Dark", "LIGHT", "", " dark"] {
        assert_eq!(ColorVariant::from_name(bad), None, "{bad:?}");
        assert!(ColorVariant::from_str(bad).is_err(), "{bad:?}");
    }
}

#[test]
fn role_and_variant_serde_uses_exact_key_strings() {
    assert_eq!(
        serde_json::to_string(&ThemeRole::OnPrimaryFixedVariant).unwrap(),
        "\"on_primary_fixed_variant\""
    );
    assert_eq!(
        serde_json::from_str::<ThemeRole>("\"source_color\"").unwrap(),
        ThemeRole::SourceColor
    );
    assert!(serde_json::from_str::<ThemeRole>("\"primaryText\"").is_err());
    assert_eq!(
        serde_json::to_string(&ColorVariant::Light).unwrap(),
        "\"light\""
    );
    assert!(serde_json::from_str::<ColorVariant>("\"default\"").is_err());
}

// ---------------------------------------------------------------------
// RGB string rule: Lite parse_hex_rgb_bytes semantics.
// ---------------------------------------------------------------------

#[test]
fn hex_bytes_parse_like_lite_for_valid_forms() {
    assert_eq!(parse_hex_rgb_bytes("#ff8000"), Some([255, 128, 0]));
    assert_eq!(
        parse_hex_rgb_bytes("  A1b2C3  "),
        Some([0xa1, 0xb2, 0xc3]),
        "no leading # is permitted"
    );
    assert_eq!(
        parse_hex_rgb_bytes("\t#AaBbCc\n"),
        Some([0xaa, 0xbb, 0xcc]),
        "outer whitespace is trimmed"
    );
    assert_eq!(parse_hex_rgb_bytes("000000"), Some([0, 0, 0]));
}

#[test]
fn hex_bytes_reject_malformed_lengths_repeated_hash_alpha_and_non_ascii() {
    for bad in [
        "",
        "#",
        "#12",
        "#12345",
        "#1234567",
        "12345",
        "1234567",
        "zzzzzz",
        "#12xz45",
        "##ff8000", // donor parse_seed would strip both; Lite strips one
        "##",
        "#ff800080", // 8-digit alpha form is not RGB
        "ff800080",
        "# ff8000", // interior whitespace is not trimmed away
    ] {
        assert_eq!(parse_hex_rgb_bytes(bad), None, "{bad:?}");
        assert_eq!(normalize_hex_color(bad), None, "{bad:?}");
    }
    // Six-byte multibyte input and other non-ASCII lookalikes.
    assert_eq!(parse_hex_rgb_bytes("あabc"), None);
    for bad in ["＃f06e44", "#é12345", "éééééé", "#f06e4é"] {
        assert_eq!(normalize_hex_color(bad), None, "{bad:?}");
    }
}

#[test]
fn normalize_hex_color_produces_lowercase_rrggbb() {
    assert_eq!(normalize_hex_color("#f06e44").as_deref(), Some("#f06e44"));
    assert_eq!(normalize_hex_color("f06e44").as_deref(), Some("#f06e44"));
    assert_eq!(normalize_hex_color("#F06E44").as_deref(), Some("#f06e44"));
    assert_eq!(
        normalize_hex_color("  #F06E44  ").as_deref(),
        Some("#f06e44")
    );
    assert_eq!(normalize_hex_color("ABCDEF").as_deref(), Some("#abcdef"));
}

#[test]
fn role_colors_try_new_validates_and_normalizes_both_variants() {
    let colors = RoleColors::try_new(" #AABBCC ", "ddeeff").unwrap();
    assert_eq!(colors.dark(), "#aabbcc");
    assert_eq!(colors.light(), "#ddeeff");
    assert_eq!(colors.get(ColorVariant::Dark), "#aabbcc");
    assert_eq!(colors.get(ColorVariant::Light), "#ddeeff");
    assert_eq!(colors.rgb_bytes(ColorVariant::Dark), [0xaa, 0xbb, 0xcc]);
    assert_eq!(colors.rgb_bytes(ColorVariant::Light), [0xdd, 0xee, 0xff]);

    // Dark is validated first; nothing is defaulted.
    let error = RoleColors::try_new("##aabbcc", "#ddeeff").unwrap_err();
    assert_eq!(
        error,
        InvalidColor {
            variant: ColorVariant::Dark,
            raw: "##aabbcc".to_string(),
        }
    );
    let error = RoleColors::try_new("#aabbcc", "#ddeeff00").unwrap_err();
    assert_eq!(
        error,
        InvalidColor {
            variant: ColorVariant::Light,
            raw: "#ddeeff00".to_string(),
        }
    );
}

// ---------------------------------------------------------------------
// Import: success paths.
// ---------------------------------------------------------------------

#[test]
fn valid_document_imports_with_distinct_dark_and_light_per_role() {
    let outcome = import_valid();
    assert_eq!(outcome.ignored_roles, Vec::<String>::new());
    let document = &outcome.document;
    for (index, role) in ThemeRole::ALL.iter().enumerate() {
        assert_eq!(
            document.color(*role, ColorVariant::Dark),
            dark_hex(index),
            "{} dark",
            role.name()
        );
        assert_eq!(
            document.color(*role, ColorVariant::Light),
            light_hex(index),
            "{} light",
            role.name()
        );
        assert_ne!(
            document.color(*role, ColorVariant::Dark),
            document.color(*role, ColorVariant::Light),
            "{} variants must stay distinct",
            role.name()
        );
        assert_eq!(document.role_colors(*role).dark(), dark_hex(index));
    }
}

#[test]
fn accessors_are_deterministic_and_in_role_order() {
    let document = import_valid().document;
    let dark = document.colors_for(ColorVariant::Dark);
    let light = document.colors_for(ColorVariant::Light);
    for (index, role) in ThemeRole::ALL.iter().enumerate() {
        assert_eq!(dark[index], dark_hex(index));
        assert_eq!(light[index], light_hex(index));
        assert_eq!(dark[index], document.color(*role, ColorVariant::Dark));
    }
    let iterated: Vec<ThemeRole> = document.iter().map(|(role, _)| role).collect();
    assert_eq!(iterated, ThemeRole::ALL.to_vec());
    let iterated_colors: Vec<&str> = document.iter().map(|(_, colors)| colors.light()).collect();
    assert_eq!(iterated_colors, light);
    assert_eq!(
        document
            .role_colors(ThemeRole::SourceColor)
            .rgb_bytes(ColorVariant::Dark),
        [49, 0x11, 0x22]
    );
}

#[test]
fn import_normalizes_mixed_case_missing_hash_and_whitespace() {
    let mut value = valid_document_value();
    set_color(&mut value, "primary", "dark", json!("  #A1B2C3 "));
    set_color(&mut value, "primary", "light", json!("F06E44"));
    set_color(&mut value, "source_color", "dark", json!("\t#00FF00\n"));
    let document = import_document(&value).unwrap().document;
    assert_eq!(
        document.color(ThemeRole::Primary, ColorVariant::Dark),
        "#a1b2c3"
    );
    assert_eq!(
        document.color(ThemeRole::Primary, ColorVariant::Light),
        "#f06e44"
    );
    assert_eq!(
        document.color(ThemeRole::SourceColor, ColorVariant::Dark),
        "#00ff00"
    );
}

#[test]
fn import_ignores_top_level_metadata_and_the_derived_default_entry() {
    let mut value = valid_document_value();
    // Unrelated top-level metadata is ignored, whatever it contains.
    let root = value.as_object_mut().unwrap();
    root.insert("mode".to_string(), json!("dark"));
    root.insert("is_dark_mode".to_string(), json!(true));
    root.insert("base16".to_string(), json!({ "base00": "#000000" }));
    root.insert("anything_else".to_string(), json!([1, 2, 3]));
    // A per-role "default" entry that disagrees with both variants must
    // never be consulted as independent state.
    role_node_mut(&mut value, "primary")
        .insert("default".to_string(), json!({ "color": "#c0ffee" }));

    let document = import_document(&value).unwrap().document;
    assert_eq!(
        document.color(ThemeRole::Primary, ColorVariant::Dark),
        dark_hex(0)
    );
    assert_eq!(
        document.color(ThemeRole::Primary, ColorVariant::Light),
        light_hex(0)
    );
}

#[test]
fn unknown_roles_inside_colors_are_reported_sorted_not_imported() {
    let mut value = valid_document_value();
    for unknown in ["zzz_unknown", "primaryText", "aaa_unknown"] {
        colors_mut(&mut value).insert(
            unknown.to_string(),
            json!({
                "dark": { "color": "#010203" },
                "light": { "color": "#040506" },
            }),
        );
    }
    let outcome = import_document(&value).unwrap();
    assert_eq!(
        outcome.ignored_roles,
        vec![
            "aaa_unknown".to_string(),
            "primaryText".to_string(),
            "zzz_unknown".to_string(),
        ],
        "unknown roles are reported sorted, including the camelCase UI name"
    );
    // They do not masquerade as supported data and do not round-trip.
    let round_tripped = outcome.document.to_value();
    let colors = round_tripped["colors"].as_object().unwrap();
    assert_eq!(colors.len(), 50);
    for unknown in ["zzz_unknown", "primaryText", "aaa_unknown"] {
        assert!(!colors.contains_key(unknown), "{unknown}");
    }
}

#[test]
fn import_does_not_mutate_its_input_on_success_or_error() {
    let value = valid_document_value();
    let snapshot = value.clone();
    assert!(import_document(&value).is_ok());
    assert_eq!(value, snapshot);

    let mut bad = valid_document_value();
    set_color(&mut bad, "error", "dark", json!("#12xz45"));
    let snapshot = bad.clone();
    assert!(import_document(&bad).is_err());
    assert_eq!(bad, snapshot);
}

// ---------------------------------------------------------------------
// Import: structural and per-role failures. No value is ever defaulted:
// every one of these asserts an Err, never an Ok with filler colors.
// ---------------------------------------------------------------------

#[test]
fn invalid_root_shapes_are_errors() {
    for bad in [
        Value::Null,
        json!([]),
        json!("colors"),
        json!(42),
        json!(true),
    ] {
        assert_eq!(
            import_document(&bad).unwrap_err(),
            ThemeImportError::RootNotObject,
            "{bad}"
        );
    }
    assert_eq!(
        import_document(&json!({})).unwrap_err(),
        ThemeImportError::MissingColors
    );
    assert_eq!(
        import_document(&json!({ "mode": "dark" })).unwrap_err(),
        ThemeImportError::MissingColors,
        "metadata alone is not a document"
    );
    for bad_colors in [json!([]), json!(null), json!("primary"), json!(7)] {
        assert_eq!(
            import_document(&json!({ "colors": bad_colors })).unwrap_err(),
            ThemeImportError::ColorsNotObject
        );
    }
}

#[test]
fn removing_any_single_role_fails_with_that_missing_role() {
    for (index, key) in EXPECTED_KEYS.iter().enumerate() {
        let mut value = valid_document_value();
        colors_mut(&mut value).remove(*key);
        assert_eq!(
            import_document(&value).unwrap_err(),
            ThemeImportError::MissingRole {
                role: ThemeRole::ALL[index],
            },
            "removing {key}"
        );
    }
}

#[test]
fn removing_any_single_variant_fails_with_that_missing_variant() {
    for (index, key) in EXPECTED_KEYS.iter().enumerate() {
        for variant in ["dark", "light"] {
            let mut value = valid_document_value();
            role_node_mut(&mut value, key).remove(variant);
            let expected_variant = if variant == "dark" {
                ColorVariant::Dark
            } else {
                ColorVariant::Light
            };
            assert_eq!(
                import_document(&value).unwrap_err(),
                ThemeImportError::MissingVariant {
                    role: ThemeRole::ALL[index],
                    variant: expected_variant,
                },
                "removing {key}.{variant}"
            );
        }
    }
}

#[test]
fn malformed_role_and_variant_nodes_are_named_errors() {
    // Role node is not an object.
    let mut value = valid_document_value();
    colors_mut(&mut value).insert("tertiary".to_string(), json!("#123456"));
    assert_eq!(
        import_document(&value).unwrap_err(),
        ThemeImportError::RoleNotObject {
            role: ThemeRole::Tertiary,
        }
    );

    // Variant node is not an object.
    let mut value = valid_document_value();
    role_node_mut(&mut value, "surface").insert("dark".to_string(), json!("#123456"));
    assert_eq!(
        import_document(&value).unwrap_err(),
        ThemeImportError::VariantNotObject {
            role: ThemeRole::Surface,
            variant: ColorVariant::Dark,
        }
    );

    // Variant object without a color key.
    let mut value = valid_document_value();
    role_node_mut(&mut value, "surface")
        .get_mut("light")
        .and_then(Value::as_object_mut)
        .unwrap()
        .remove("color");
    assert_eq!(
        import_document(&value).unwrap_err(),
        ThemeImportError::MissingColor {
            role: ThemeRole::Surface,
            variant: ColorVariant::Light,
        }
    );
}

#[test]
fn non_string_color_values_are_named_errors() {
    for bad_color in [
        json!(0xff8000),
        json!(null),
        json!(true),
        json!(["#ff8000"]),
    ] {
        let mut value = valid_document_value();
        set_color(&mut value, "outline", "light", bad_color.clone());
        assert_eq!(
            import_document(&value).unwrap_err(),
            ThemeImportError::ColorNotString {
                role: ThemeRole::Outline,
                variant: ColorVariant::Light,
            },
            "{bad_color}"
        );
    }
}

#[test]
fn invalid_color_strings_are_named_errors_with_the_raw_value() {
    for bad in [
        "#12xz45",
        "#12345",
        "#1234567",
        "##aabbcc",
        "#aabbccdd",
        "",
        "#",
    ] {
        let mut value = valid_document_value();
        set_color(&mut value, "error", "dark", json!(bad));
        assert_eq!(
            import_document(&value).unwrap_err(),
            ThemeImportError::InvalidColor {
                role: ThemeRole::Error,
                variant: ColorVariant::Dark,
                raw: bad.to_string(),
            },
            "{bad:?}"
        );
    }

    // The diagnostic names the role, the variant and the raw string.
    let mut value = valid_document_value();
    set_color(&mut value, "error", "dark", json!("#12xz45"));
    let message = import_document(&value).unwrap_err().to_string();
    assert!(message.contains("error"), "{message}");
    assert!(message.contains("dark"), "{message}");
    assert!(message.contains("#12xz45"), "{message}");
}

#[test]
fn the_first_defect_in_role_order_dark_before_light_is_reported() {
    // tertiary (index 2) has an invalid dark color; surface_variant
    // (index 5) is missing entirely. The earlier role wins.
    let mut value = valid_document_value();
    set_color(&mut value, "tertiary", "dark", json!("not-a-color"));
    colors_mut(&mut value).remove("surface_variant");
    assert_eq!(
        import_document(&value).unwrap_err(),
        ThemeImportError::InvalidColor {
            role: ThemeRole::Tertiary,
            variant: ColorVariant::Dark,
            raw: "not-a-color".to_string(),
        }
    );

    // Within one role, dark is validated before light.
    let mut value = valid_document_value();
    set_color(&mut value, "primary", "dark", json!("#zzzzzz"));
    set_color(&mut value, "primary", "light", json!("#yyyyyy"));
    assert_eq!(
        import_document(&value).unwrap_err(),
        ThemeImportError::InvalidColor {
            role: ThemeRole::Primary,
            variant: ColorVariant::Dark,
            raw: "#zzzzzz".to_string(),
        }
    );
}

// ---------------------------------------------------------------------
// Serialization: standalone candidate document contract.
// ---------------------------------------------------------------------

#[test]
fn to_value_emits_the_donor_shape_without_default_or_metadata() {
    let document = import_valid().document;
    let value = document.to_value();
    let colors = value["colors"].as_object().expect("colors object");
    assert_eq!(colors.len(), 50);
    for (index, key) in EXPECTED_KEYS.iter().enumerate() {
        assert_eq!(
            value["colors"][key]["dark"]["color"],
            json!(dark_hex(index)),
            "{key} dark"
        );
        assert_eq!(
            value["colors"][key]["light"]["color"],
            json!(light_hex(index)),
            "{key} light"
        );
        assert!(
            value["colors"][key].get("default").is_none(),
            "{key}: derived default is not emitted"
        );
    }
    assert!(value.get("base16").is_none());
    assert!(value.get("mode").is_none());
    assert!(value.get("is_dark_mode").is_none());
}

#[test]
fn document_round_trips_through_value_and_through_serde_json() {
    let document = import_valid().document;

    let outcome = import_document(&document.to_value()).unwrap();
    assert_eq!(outcome.ignored_roles, Vec::<String>::new());
    assert_eq!(outcome.document, document);

    let text = serde_json::to_string(&document).unwrap();
    assert!(text.contains("\"source_color\""), "{text}");
    assert!(text.contains("\"on_primary\""), "{text}");
    assert!(!text.contains("primaryText"), "{text}");
    let restored: ThemeDocument = serde_json::from_str(&text).unwrap();
    assert_eq!(restored, document);
}

#[test]
fn strict_serde_deserialize_rejects_unknown_roles_and_invalid_colors() {
    // Unlike the foreign-document adapter, the candidate's own format
    // must not silently drop an entry on a round trip.
    let document = import_valid().document;
    let mut value = document.to_value();
    colors_mut(&mut value).insert(
        "mystery_role".to_string(),
        json!({
            "dark": { "color": "#010203" },
            "light": { "color": "#040506" },
        }),
    );
    let text = serde_json::to_string(&value).unwrap();
    assert!(serde_json::from_str::<ThemeDocument>(&text).is_err());

    let mut value = document.to_value();
    set_color(&mut value, "primary", "dark", json!("#12345"));
    let text = serde_json::to_string(&value).unwrap();
    assert!(serde_json::from_str::<ThemeDocument>(&text).is_err());
}

#[test]
fn rgb_owner_is_the_decoder_behind_every_compatibility_entry_point() {
    // Exact expectations written independently of both implementations.
    let cases: [(&str, Option<[u8; 3]>); 8] = [
        ("#ff8000", Some([255, 128, 0])),
        ("  A1b2C3  ", Some([0xa1, 0xb2, 0xc3])),
        ("000000", Some([0, 0, 0])),
        ("##ff8000", None),
        ("#ff800080", None),
        ("#12345", None),
        ("zzzzzz", None),
        ("", None),
    ];
    for (input, expected) in cases {
        assert_eq!(
            rgb_color::parse_hex_rgb_bytes(input),
            expected,
            "owner {input:?}"
        );
        assert_eq!(
            parse_hex_rgb_bytes(input),
            expected,
            "theme_roles re-export {input:?}"
        );
        assert_eq!(
            normalize_hex_color(input),
            expected.map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}")),
            "normalization {input:?}"
        );
    }
}

#[test]
fn rgb_owner_rejects_non_ascii_and_unicode_lookalikes() {
    assert_eq!(rgb_color::parse_hex_rgb_bytes("あabc"), None);
    assert_eq!(parse_hex_rgb_bytes("あabc"), None);
    assert_eq!(rgb_color::parse_hex_rgb_bytes("#１２３４５６"), None);
}

/// Independent oracle for the editor navigation metadata, transcribed
/// from the role-editor handoff descriptor inventory (donor roles.rs
/// descriptors + en-US labels): (key, group label, label, consumer)
/// in donor display order.
const EXPECTED_DESCRIPTORS: [(&str, &str, &str, bool); 50] = [
    ("primary", "Wall interface", "Accent", true),
    ("on_primary", "Wall interface", "Accent text", true),
    ("tertiary", "Wall interface", "Second accent", true),
    ("surface", "Wall interface", "Panel", true),
    ("on_surface", "Wall interface", "Panel text", true),
    ("surface_variant", "Wall interface", "Panel alt", true),
    ("surface_container", "Wall interface", "Panel raised", true),
    ("background", "Wall interface", "Backdrop", true),
    ("outline", "Wall interface", "Outline", true),
    (
        "inverse_primary",
        "Primary and secondary",
        "Inverse primary",
        false,
    ),
    (
        "on_primary_container",
        "Primary and secondary",
        "Primary container text",
        false,
    ),
    (
        "on_primary_fixed",
        "Primary and secondary",
        "Fixed primary text",
        false,
    ),
    (
        "on_primary_fixed_variant",
        "Primary and secondary",
        "Fixed primary text alt",
        false,
    ),
    (
        "on_secondary",
        "Primary and secondary",
        "Secondary text",
        false,
    ),
    (
        "on_secondary_container",
        "Primary and secondary",
        "Secondary container text",
        false,
    ),
    (
        "on_secondary_fixed",
        "Primary and secondary",
        "Fixed secondary text",
        false,
    ),
    (
        "on_secondary_fixed_variant",
        "Primary and secondary",
        "Fixed secondary text alt",
        false,
    ),
    (
        "primary_container",
        "Primary and secondary",
        "Primary container",
        false,
    ),
    (
        "primary_fixed",
        "Primary and secondary",
        "Fixed primary",
        false,
    ),
    (
        "primary_fixed_dim",
        "Primary and secondary",
        "Fixed primary dim",
        false,
    ),
    ("secondary", "Primary and secondary", "Secondary", false),
    (
        "secondary_container",
        "Primary and secondary",
        "Secondary container",
        false,
    ),
    (
        "secondary_fixed",
        "Primary and secondary",
        "Fixed secondary",
        false,
    ),
    (
        "secondary_fixed_dim",
        "Primary and secondary",
        "Fixed secondary dim",
        false,
    ),
    ("on_tertiary", "Tertiary", "Tertiary text", false),
    (
        "on_tertiary_container",
        "Tertiary",
        "Tertiary container text",
        false,
    ),
    (
        "on_tertiary_fixed",
        "Tertiary",
        "Fixed tertiary text",
        false,
    ),
    (
        "on_tertiary_fixed_variant",
        "Tertiary",
        "Fixed tertiary text alt",
        false,
    ),
    (
        "tertiary_container",
        "Tertiary",
        "Tertiary container",
        false,
    ),
    ("tertiary_fixed", "Tertiary", "Fixed tertiary", false),
    (
        "tertiary_fixed_dim",
        "Tertiary",
        "Fixed tertiary dim",
        false,
    ),
    (
        "inverse_on_surface",
        "Surfaces and outlines",
        "Inverse surface text",
        false,
    ),
    (
        "inverse_surface",
        "Surfaces and outlines",
        "Inverse surface",
        false,
    ),
    (
        "on_background",
        "Surfaces and outlines",
        "Background text",
        false,
    ),
    (
        "on_surface_variant",
        "Surfaces and outlines",
        "Surface variant text",
        false,
    ),
    (
        "outline_variant",
        "Surfaces and outlines",
        "Outline variant",
        false,
    ),
    (
        "surface_bright",
        "Surfaces and outlines",
        "Bright surface",
        false,
    ),
    (
        "surface_container_high",
        "Surfaces and outlines",
        "High container",
        false,
    ),
    (
        "surface_container_highest",
        "Surfaces and outlines",
        "Highest container",
        false,
    ),
    (
        "surface_container_low",
        "Surfaces and outlines",
        "Low container",
        false,
    ),
    (
        "surface_container_lowest",
        "Surfaces and outlines",
        "Lowest container",
        false,
    ),
    ("surface_dim", "Surfaces and outlines", "Dim surface", false),
    (
        "surface_tint",
        "Surfaces and outlines",
        "Surface tint",
        false,
    ),
    ("error", "Errors", "Error", false),
    ("error_container", "Errors", "Error container", false),
    ("on_error", "Errors", "Error text", false),
    (
        "on_error_container",
        "Errors",
        "Error container text",
        false,
    ),
    ("scrim", "Source and effects", "Scrim", false),
    ("shadow", "Source and effects", "Shadow", false),
    ("source_color", "Source and effects", "Source colour", false),
];

#[test]
fn theme_role_editor_descriptors_match_the_donor_oracle() {
    use theme_roles::{role_descriptor, RoleGroup, ROLE_DESCRIPTORS};
    assert_eq!(ROLE_DESCRIPTORS.len(), 50);
    for (position, (key, group_label, label, consumer)) in EXPECTED_DESCRIPTORS.iter().enumerate() {
        let descriptor = &ROLE_DESCRIPTORS[position];
        assert_eq!(descriptor.role.name(), *key, "position {position}");
        assert_eq!(
            descriptor.group.label(),
            *group_label,
            "position {position}"
        );
        assert_eq!(descriptor.label, *label, "position {position}");
        assert_eq!(descriptor.has_consumer, *consumer, "position {position}");
        assert_eq!(role_descriptor(descriptor.role).role, descriptor.role);
    }
    let group_labels: Vec<&str> = RoleGroup::ALL.iter().map(|group| group.label()).collect();
    assert_eq!(
        group_labels,
        [
            "Wall interface",
            "Primary and secondary",
            "Tertiary",
            "Surfaces and outlines",
            "Errors",
            "Source and effects"
        ]
    );
    // Exactly the nine mapped roles report a consumer.
    assert_eq!(
        ROLE_DESCRIPTORS
            .iter()
            .filter(|descriptor| descriptor.has_consumer)
            .count(),
        9
    );
}

#[test]
fn theme_role_editor_filter_uses_donor_label_and_key_semantics() {
    use theme_roles::filtered_role_descriptors;
    let keys = |filter: &str| -> Vec<&str> {
        filtered_role_descriptors(filter)
            .iter()
            .map(|descriptor| descriptor.role.name())
            .collect()
    };
    assert_eq!(keys("").len(), 50);
    assert_eq!(keys("scrim"), ["scrim"]);
    assert_eq!(keys("source"), ["source_color"]);
    assert_eq!(keys("zzz-no-match"), Vec::<&str>::new());
    // Trim + case-insensitive label substring, display order kept.
    assert_eq!(
        keys("  Panel "),
        [
            "surface",
            "on_surface",
            "surface_variant",
            "surface_container"
        ]
    );
    // Key substring match reaches roles whose labels differ.
    assert_eq!(
        keys("on_primary"),
        [
            "on_primary",
            "on_primary_container",
            "on_primary_fixed",
            "on_primary_fixed_variant"
        ]
    );
}

#[test]
fn theme_role_editor_replaced_changes_exactly_one_cell() {
    let document = import_valid().document;
    for role in ThemeRole::ALL {
        for variant in ColorVariant::ALL {
            let replaced = document.replaced(role, variant, "#010203");
            for other in ThemeRole::ALL {
                for other_variant in ColorVariant::ALL {
                    let expected = if other == role && other_variant == variant {
                        "#010203"
                    } else {
                        document.color(other, other_variant)
                    };
                    assert_eq!(
                        replaced.color(other, other_variant),
                        expected,
                        "cell {} {:?}",
                        other.name(),
                        other_variant
                    );
                }
            }
            // The source document is immutable.
            assert_ne!(document.color(role, variant), "#010203");
        }
    }
}

#[test]
fn theme_role_editor_fixtures_import_through_the_production_path() {
    for (text, primary_dark, primary_light, source_dark) in [
        (
            include_str!("fixtures/role-editor/sentinel-full.json"),
            "#0a4682",
            "#a0d23c",
            "#3b77b3",
        ),
        (
            include_str!("fixtures/role-editor/readable-full.json"),
            "#a8c7fa",
            "#415f91",
            "#a8c7fa",
        ),
    ] {
        let value: Value = serde_json::from_str(text).expect("fixture is valid JSON");
        let imported = import_document(&value).expect("fixture imports");
        assert!(imported.ignored_roles.is_empty());
        assert_eq!(
            imported
                .document
                .color(ThemeRole::Primary, ColorVariant::Dark),
            primary_dark
        );
        assert_eq!(
            imported
                .document
                .color(ThemeRole::Primary, ColorVariant::Light),
            primary_light
        );
        assert_eq!(
            imported
                .document
                .color(ThemeRole::SourceColor, ColorVariant::Dark),
            source_dark
        );
        // Whole-document round trip retains all 100 values.
        let reparsed: Value =
            serde_json::from_str(&serde_json::to_string(&imported.document.to_value()).unwrap())
                .unwrap();
        let reimported = import_document(&reparsed).expect("round trip imports");
        assert_eq!(reimported.document, imported.document);
    }
}

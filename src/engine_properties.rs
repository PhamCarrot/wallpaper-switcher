use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum EnginePropertyKind {
    Bool,
    Slider { min: f64, max: f64, step: f64 },
    Combo { options: Vec<EnginePropertyOption> },
    Color,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnginePropertyOption {
    pub label: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EngineProperty {
    pub key: String,
    pub label: String,
    pub kind: EnginePropertyKind,
    pub default_value: serde_json::Value,
    order: i64,
}

const MAX_DIAGNOSTIC_CHARS: usize = 160;
const MAX_SKIPPED_DEFINITION_EXAMPLES: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkippedPropertyDefinitions {
    pub count: usize,
    pub examples: Vec<String>,
}

impl SkippedPropertyDefinitions {
    fn new() -> Self {
        Self {
            count: 0,
            examples: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PropertySchemaState {
    ReadFailure(String),
    InvalidJson(String),
    MalformedSchema(String),
    ValidNoPropertySection,
    ValidNoSupportedProperties { skipped: SkippedPropertyDefinitions },
    Supported { skipped: SkippedPropertyDefinitions },
}

#[derive(Debug, Clone, PartialEq)]
pub struct PropertySchema {
    pub properties: Vec<EngineProperty>,
    pub state: PropertySchemaState,
}

pub fn read_supported(path: &Path) -> PropertySchema {
    let contents = match std::fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) => {
            return PropertySchema {
                properties: Vec::new(),
                state: PropertySchemaState::ReadFailure(bounded_text(&format!(
                    "{} ({error})",
                    path.display()
                ))),
            };
        }
    };
    let root = match serde_json::from_str::<serde_json::Value>(&contents) {
        Ok(root) => root,
        Err(error) => {
            return PropertySchema {
                properties: Vec::new(),
                state: PropertySchemaState::InvalidJson(bounded_text(&error.to_string())),
            };
        }
    };
    let Some(root) = root.as_object() else {
        return PropertySchema {
            properties: Vec::new(),
            state: PropertySchemaState::MalformedSchema(
                "project root must be a JSON object".into(),
            ),
        };
    };
    let Some(general) = root.get("general") else {
        return PropertySchema {
            properties: Vec::new(),
            state: PropertySchemaState::ValidNoPropertySection,
        };
    };
    let Some(general) = general.as_object() else {
        return PropertySchema {
            properties: Vec::new(),
            state: PropertySchemaState::MalformedSchema(
                "the `general` section must be a JSON object".into(),
            ),
        };
    };
    let Some(properties) = general.get("properties") else {
        return PropertySchema {
            properties: Vec::new(),
            state: PropertySchemaState::ValidNoPropertySection,
        };
    };
    let Some(properties) = properties.as_object() else {
        return PropertySchema {
            properties: Vec::new(),
            state: PropertySchemaState::MalformedSchema(
                "the `general.properties` section must be a JSON object".into(),
            ),
        };
    };

    let mut skipped = SkippedPropertyDefinitions::new();
    let mut result = properties
        .iter()
        .filter_map(|(key, definition)| match parse_property(key, definition) {
            Ok(property) => Some(property),
            Err(reason) => {
                skipped.count += 1;
                if skipped.examples.len() < MAX_SKIPPED_DEFINITION_EXAMPLES {
                    skipped.examples.push(bounded_text(&format!(
                        "{}: {}",
                        bounded_text(key),
                        bounded_text(&reason)
                    )));
                }
                None
            }
        })
        .collect::<Vec<_>>();
    result.sort_by(|left, right| {
        left.order
            .cmp(&right.order)
            .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
    });

    let state = if result.is_empty() {
        PropertySchemaState::ValidNoSupportedProperties { skipped }
    } else {
        PropertySchemaState::Supported { skipped }
    };
    PropertySchema {
        properties: result,
        state,
    }
}

fn parse_property(key: &str, definition: &serde_json::Value) -> Result<EngineProperty, String> {
    let definition = definition
        .as_object()
        .ok_or_else(|| "definition must be a JSON object".to_string())?;
    let kind_name = definition
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| "missing string `type`".to_string())?
        .to_ascii_lowercase();
    let default_value = definition
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let kind = match kind_name.as_str() {
        "bool" => EnginePropertyKind::Bool,
        "slider" => {
            let min = definition
                .get("min")
                .and_then(serde_json::Value::as_f64)
                .ok_or_else(|| "slider `min` must be a number".to_string())?;
            let max = definition
                .get("max")
                .and_then(serde_json::Value::as_f64)
                .ok_or_else(|| "slider `max` must be a number".to_string())?;
            if min > max {
                return Err("slider `min` must not exceed `max`".into());
            }
            let step = definition
                .get("step")
                .and_then(serde_json::Value::as_f64)
                .filter(|step| *step > 0.0)
                .unwrap_or_else(|| ((max - min) / 10.0).max(0.01));
            EnginePropertyKind::Slider { min, max, step }
        }
        "combo" => {
            let options = definition
                .get("options")
                .and_then(serde_json::Value::as_array)
                .ok_or_else(|| "combo `options` must be an array".to_string())?
                .iter()
                .enumerate()
                .map(|(index, option)| {
                    let value = option
                        .get("value")
                        .cloned()
                        .ok_or_else(|| format!("combo option {index} is missing a `value`"))?;
                    let label = option
                        .get("label")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                        .unwrap_or_else(|| display_value(&value));
                    Ok(EnginePropertyOption { label, value })
                })
                .collect::<Result<Vec<_>, String>>()?;
            if options.is_empty() {
                return Err("combo has no usable options".into());
            }
            EnginePropertyKind::Combo { options }
        }
        "textinput" | "text" => return Err("editable Text properties are not supported".into()),
        "color" => EnginePropertyKind::Color,
        _ => return Err(format!("unsupported property type `{kind_name}`")),
    };
    let label = definition
        .get("text")
        .and_then(serde_json::Value::as_str)
        .filter(|label| !label.trim().is_empty() && !label.starts_with("ui_"))
        .map(str::to_string)
        .unwrap_or_else(|| humanize(key));
    Ok(EngineProperty {
        key: key.to_string(),
        label,
        kind,
        default_value,
        order: definition
            .get("order")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(i64::MAX),
    })
}

/// Presentation only: never normalizes the stored or submitted property string.
pub(crate) fn color_preview_rgb(value: &str) -> Option<[f32; 3]> {
    if let Some(rgb) = crate::rgb_color::parse_hex_rgb_bytes(value) {
        return Some(rgb.map(|channel| channel as f32 / 255.0));
    }
    let mut components = value.split_whitespace();
    let mut rgb = [0.0; 3];
    for channel in &mut rgb {
        let value = components.next()?.parse::<f64>().ok()?;
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return None;
        }
        *channel = value as f32;
    }
    components.next().is_none().then_some(rgb)
}

pub fn accepts_value(property: &EngineProperty, value: &serde_json::Value) -> bool {
    match &property.kind {
        EnginePropertyKind::Bool => value.is_boolean(),
        EnginePropertyKind::Slider { min, max, .. } => value
            .as_f64()
            .is_some_and(|number| number.is_finite() && number >= *min && number <= *max),
        EnginePropertyKind::Combo { options } => {
            options.iter().any(|option| option.value == *value)
        }
        EnginePropertyKind::Color => value.is_string(),
    }
}

fn bounded_text(value: &str) -> String {
    let mut characters = value.chars();
    let bounded = characters
        .by_ref()
        .take(MAX_DIAGNOSTIC_CHARS)
        .collect::<String>();
    if characters.next().is_some() {
        format!("{bounded}…")
    } else {
        bounded
    }
}

#[cfg(test)]
pub fn step_value(property: &EngineProperty, current: &serde_json::Value) -> serde_json::Value {
    match &property.kind {
        EnginePropertyKind::Bool => serde_json::Value::Bool(!current.as_bool().unwrap_or(false)),
        EnginePropertyKind::Slider { min, max, step } => {
            let current = current.as_f64().unwrap_or(*min);
            let next = if current + step > *max + f64::EPSILON {
                *min
            } else {
                (current + step).clamp(*min, *max)
            };
            serde_json::Number::from_f64(next)
                .map(serde_json::Value::Number)
                .unwrap_or_else(|| property.default_value.clone())
        }
        EnginePropertyKind::Combo { options } => {
            let current_index = options
                .iter()
                .position(|option| &option.value == current)
                .unwrap_or(options.len() - 1);
            options[(current_index + 1) % options.len()].value.clone()
        }
        EnginePropertyKind::Color => current.clone(),
    }
}

pub fn display_value(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Bool(value) => if *value { "On" } else { "Off" }.to_string(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Null => "Default".to_string(),
        value => value.to_string(),
    }
}

fn humanize(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 8);
    for (index, character) in value.chars().enumerate() {
        if index > 0 && (character == '_' || character.is_ascii_uppercase()) {
            output.push(' ');
        }
        if character != '_' {
            output.push(if index == 0 {
                character.to_ascii_uppercase()
            } else {
                character
            });
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn color_preview_normalized_blue_and_hex_preserve_raw_identity() {
        let raw = "0.33725 0.53333 0.93333";
        let vector = color_preview_rgb(raw).unwrap();
        let hex = color_preview_rgb("#5688EE").unwrap();
        assert_eq!(vector, [0.33725, 0.53333, 0.93333]);
        for (a, b) in vector.into_iter().zip(hex) {
            assert!((a - b).abs() < 0.00001);
        }
        assert_eq!(color_preview_rgb("  5688ee  "), Some(hex));
        assert_eq!(color_preview_rgb("\t0  .5\n1 "), Some([0.0, 0.5, 1.0]));
        let property = EngineProperty {
            key: "color".into(),
            label: "Color".into(),
            kind: EnginePropertyKind::Color,
            default_value: serde_json::json!(raw),
            order: 0,
        };
        assert!(accepts_value(&property, &property.default_value));
        assert_eq!(
            step_value(&property, &property.default_value),
            serde_json::json!(raw)
        );
        assert_eq!(display_value(&property.default_value), raw);
        assert!(accepts_value(
            &property,
            &serde_json::json!("unpreviewable raw text")
        ));
    }

    #[test]
    fn color_preview_rejects_malformed_nonfinite_range_and_extra_components() {
        for raw in [
            "",
            "0 1",
            "0 0 1 1",
            "0 0 1 trailing",
            "0,0,1",
            "red",
            "NaN 0 1",
            "0 inf 1",
            "0 0 -inf",
            "1e999 0 0",
            "-0.01 0 1",
            "0 1.001 0",
            "1.0000000001 0 0",
            "86 136 238",
            "#abc",
            "#5688EEFF",
            "##5688EE",
        ] {
            assert_eq!(color_preview_rgb(raw), None, "{raw:?}");
        }
    }

    fn fixture_root() -> std::path::PathBuf {
        static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "wallpaper-switcher-properties-{}-{}",
            std::process::id(),
            NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn write_fixture(root: &std::path::Path, contents: &str) -> std::path::PathBuf {
        let path = root.join("project.json");
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn p2_text_is_skipped_while_color_and_combo_strings_keep_json_identity() {
        let root = fixture_root();
        let path = write_fixture(
            &root,
            r##"{"general":{"properties":{
            "retired_text":{"type":"text","value":"hidden"},
            "retired_input":{"type":"textinput","value":"hidden input"},
            "color":{"type":"color","value":"#123456","order":1},
            "choice":{"type":"combo","value":"quoted \"value\" \\ path","order":2,
                "options":[{"label":"Quoted","value":"quoted \"value\" \\ path"}]}
        }}}"##,
        );
        let schema = read_supported(&path);
        assert_eq!(
            schema
                .properties
                .iter()
                .map(|p| p.key.as_str())
                .collect::<Vec<_>>(),
            vec!["color", "choice"]
        );
        let PropertySchemaState::Supported { skipped } = &schema.state else {
            panic!("expected supported schema")
        };
        assert_eq!(skipped.count, 2);
        assert!(skipped
            .examples
            .iter()
            .all(|message| message.contains("Text")));
        assert!(accepts_value(
            &schema.properties[0],
            &serde_json::json!("#654321")
        ));
        assert!(accepts_value(
            &schema.properties[1],
            &serde_json::json!(r#"quoted "value" \ path"#)
        ));
        assert!(!accepts_value(
            &schema.properties[1],
            &serde_json::json!("not an option")
        ));
        assert!(!accepts_value(
            &schema.properties[0],
            &serde_json::json!(false)
        ));
        let encoded = serde_json::to_string(&schema.properties[1].default_value).unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(&encoded).unwrap(),
            schema.properties[1].default_value
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reads_only_supported_properties_and_keeps_provider_order() {
        let root = fixture_root();
        let path = write_fixture(
            &root,
            r#"{"general":{"properties":{"speed":{"type":"slider","text":"Speed","value":2,"min":0,"max":4,"step":1,"order":2},"enabled":{"type":"bool","value":true,"order":1},"asset":{"type":"file","value":"x"}}}}"#,
        );

        let schema = read_supported(&path);

        assert_eq!(schema.properties.len(), 2);
        assert_eq!(schema.properties[0].key, "enabled");
        assert_eq!(schema.properties[1].key, "speed");
        let PropertySchemaState::Supported { skipped } = schema.state else {
            panic!("expected supported property schema");
        };
        assert_eq!(skipped.count, 1);
        assert!(skipped.examples[0].contains("asset"));
        assert_eq!(
            step_value(&schema.properties[1], &serde_json::json!(4)),
            serde_json::json!(0.0)
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn schema_failure_and_valid_empty_states_remain_distinct() {
        let root = fixture_root();
        let missing = read_supported(&root.join("missing.json"));
        assert!(matches!(missing.state, PropertySchemaState::ReadFailure(_)));

        let invalid_json = write_fixture(&root, "{not-json");
        let invalid_json = read_supported(&invalid_json);
        assert!(matches!(
            invalid_json.state,
            PropertySchemaState::InvalidJson(_)
        ));

        let malformed = write_fixture(&root, r#"{"general":{"properties":[]}}"#);
        let malformed = read_supported(&malformed);
        assert!(matches!(
            malformed.state,
            PropertySchemaState::MalformedSchema(_)
        ));

        let no_section = write_fixture(&root, r#"{"general":{"title":"No controls"}}"#);
        let no_section = read_supported(&no_section);
        assert!(matches!(
            no_section.state,
            PropertySchemaState::ValidNoPropertySection
        ));

        let empty = write_fixture(&root, r#"{"general":{"properties":{}}}"#);
        let empty = read_supported(&empty);
        assert!(matches!(
            empty.state,
            PropertySchemaState::ValidNoSupportedProperties { .. }
        ));

        let unsupported = write_fixture(
            &root,
            r#"{"general":{"properties":{"asset":{"type":"file","value":"x"}}}}"#,
        );
        let unsupported = read_supported(&unsupported);
        assert!(matches!(
            unsupported.state,
            PropertySchemaState::ValidNoSupportedProperties { .. }
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn mixed_valid_and_invalid_definitions_keep_rows_and_bound_diagnostics() {
        let root = fixture_root();
        let path = write_fixture(
            &root,
            r#"{"general":{"properties":{"speed":{"type":"slider","text":"Speed","value":2,"min":0,"max":4,"order":2},"enabled":{"type":"bool","value":true,"order":1},"broken":{"type":"slider","min":0},"reversed":{"type":"slider","min":4,"max":0},"unsupported":{"type":"file","value":"x"},"empty_combo":{"type":"combo","options":[]},"malformed_combo":{"type":"combo","options":[{"label":"Missing value"}]}}}}"#,
        );

        let schema = read_supported(&path);

        assert_eq!(
            schema
                .properties
                .iter()
                .map(|property| property.key.as_str())
                .collect::<Vec<_>>(),
            vec!["enabled", "speed"]
        );
        let PropertySchemaState::Supported { skipped } = schema.state else {
            panic!("expected supported property schema");
        };
        assert_eq!(skipped.count, 5);
        assert_eq!(skipped.examples.len(), 5);
        assert!(skipped
            .examples
            .iter()
            .any(|entry| entry.contains("broken")));
        assert!(skipped
            .examples
            .iter()
            .any(|entry| entry.contains("reversed")));
        assert!(skipped
            .examples
            .iter()
            .any(|entry| entry.contains("unsupported")));
        assert!(skipped
            .examples
            .iter()
            .any(|entry| entry.contains("malformed_combo")));
        std::fs::remove_dir_all(root).unwrap();
    }
}

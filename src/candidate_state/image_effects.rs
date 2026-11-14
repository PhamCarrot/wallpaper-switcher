//! Source-derived local effect forms. These records never grant execution.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Debug, Deserialize)]
pub struct Definition {
    pub id: String,
    pub label: String,
    pub description: String,
    pub category: String,
    pub params: Vec<Parameter>,
}
#[derive(Debug, Deserialize)]
pub struct Parameter {
    pub id: String,
    pub label: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub default: Value,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    #[serde(default)]
    pub options: Vec<Choice>,
}
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Choice {
    pub mode: String,
    pub label: String,
}
impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.label)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectChoice(pub String);
impl std::fmt::Display for EffectChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match definition(&self.0) {
            Some(d) => write!(f, "{} · {}", d.category, d.label),
            None => f.write_str(&self.0),
        }
    }
}
pub fn definitions() -> &'static [Definition] {
    static DEFINITIONS: OnceLock<Vec<Definition>> = OnceLock::new();
    DEFINITIONS.get_or_init(|| {
        serde_json::from_str(include_str!("../../assets/effects/catalog.json"))
            .expect("bundled source effect forms")
    })
}
pub fn definition(id: &str) -> Option<&'static Definition> {
    definitions().iter().find(|effect| effect.id == id)
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingEffect {
    pub effect: String,
    pub enabled: bool,
    pub params: BTreeMap<String, Value>,
    #[serde(flatten)]
    pub retained: BTreeMap<String, Value>,
}
impl PendingEffect {
    pub fn new(id: &str) -> Option<Self> {
        let definition = definition(id)?;
        Some(Self {
            effect: id.into(),
            enabled: true,
            params: definition
                .params
                .iter()
                .map(|p| (p.id.clone(), p.default.clone()))
                .collect(),
            retained: BTreeMap::new(),
        })
    }
    pub fn set(&mut self, field: &str, value: Value) -> Result<(), String> {
        let parameter = definition(&self.effect)
            .and_then(|d| d.params.iter().find(|p| p.id == field))
            .ok_or("Unknown effect parameter; stored data was retained.")?;
        let value = parameter.validate(value)?;
        self.params.insert(field.into(), value);
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        let definition =
            definition(&self.effect).ok_or("Unknown effect; pending data was retained.")?;
        if self.params.len() != definition.params.len() {
            return Err(
                "Incomplete or unfamiliar effect parameters; pending data was retained.".into(),
            );
        }
        for parameter in &definition.params {
            parameter.validate(
                self.params
                    .get(&parameter.id)
                    .cloned()
                    .ok_or("Missing effect parameter")?,
            )?;
        }
        Ok(())
    }
}
impl Parameter {
    pub fn validate(&self, value: Value) -> Result<Value, String> {
        match self.kind.as_str() {
            "number" | "integer" => {
                let n = value
                    .as_f64()
                    .or_else(|| value.as_str().and_then(|s| s.trim().parse().ok()))
                    .filter(|n| n.is_finite())
                    .ok_or("Enter a finite number.")?;
                if self.min.is_some_and(|min| n < min)
                    || self.max.is_some_and(|max| n > max)
                    || (self.kind == "integer" && n.fract() != 0.0)
                {
                    return Err("Value is outside the source parameter range.".into());
                }
                Ok(if self.kind == "integer" {
                    Value::from(n as i64)
                } else {
                    Value::from(n)
                })
            }
            "dropdown" => {
                let text = value.as_str().ok_or("Choose an available value.")?;
                if !self.options.iter().any(|option| option.mode == text) {
                    return Err("Choose an available source value.".into());
                }
                Ok(value)
            }
            "color" => {
                let text = value.as_str().ok_or("Enter a #RRGGBB colour.")?;
                if text.len() != 7
                    || !text.starts_with('#')
                    || !text[1..].bytes().all(|c| c.is_ascii_hexdigit())
                {
                    return Err("Enter a #RRGGBB colour.".into());
                }
                Ok(Value::String(text.to_ascii_lowercase()))
            }
            _ => Err("This parameter form is unavailable.".into()),
        }
    }
}
pub fn validate_stack(value: Value) -> Result<Value, String> {
    let effects: Vec<PendingEffect> =
        serde_json::from_value(value.clone()).map_err(|_| "Invalid pending image stack.")?;
    if effects.len() > 16 {
        return Err("At most16 image effects are supported in a local stack.".into());
    }
    for effect in effects {
        effect.validate()?;
    }
    Ok(value)
}

//! Source condition encoding and local validation only. No clock or scheduler.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
pub const KINDS: [&str; 11] = [
    "Weekday",
    "Time window",
    "Time after",
    "Time before",
    "Date",
    "Year",
    "Weather",
    "Power",
    "Battery",
    "Output",
    "Output count",
];
pub const WEEKDAYS: [&str; 7] = ["mon", "tue", "wed", "thu", "fri", "sat", "sun"];
pub const WEATHER: [&str; 8] = [
    "clear", "sunny", "cloudy", "rainy", "snowy", "stormy", "foggy", "windy",
];
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    pub kind: String,
    #[serde(default)]
    pub negated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operator: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<Node>>,
    #[serde(flatten)]
    pub retained: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Condition {
    pub version: u8,
    pub root: Node,
    #[serde(flatten)]
    pub retained: BTreeMap<String, Value>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub priority: i64,
    pub name: String,
    pub enabled: bool,
    pub condition: Condition,
    pub set: String,
    pub mode: String,
    #[serde(flatten)]
    pub retained: BTreeMap<String, Value>,
}
impl Node {
    pub fn group(any: bool) -> Self {
        Self {
            kind: "group".into(),
            negated: false,
            value: None,
            operator: Some(if any { "any" } else { "all" }.into()),
            children: Some(Vec::new()),
            retained: BTreeMap::new(),
        }
    }
    pub fn predicate(kind: &str) -> Option<Self> {
        let value = match kind {
            "Weekday" => "weekday:sat,sun",
            "Time window" => "time:sunrise..sunset",
            "Time after" => "time:>=20:00",
            "Time before" => "time:<08:00",
            "Date" => "date:12-25",
            "Year" => "year:2026",
            "Weather" => "weather:cloudy",
            "Power" => "power:battery",
            "Battery" => "battery:<=30",
            "Output" => "output:DP-1",
            "Output count" => "outputs:>=2",
            _ => return None,
        };
        Some(Self {
            kind: "predicate".into(),
            negated: false,
            value: Some(value.into()),
            operator: None,
            children: None,
            retained: BTreeMap::new(),
        })
    }
    pub fn at_mut(&mut self, path: &[usize]) -> Option<&mut Self> {
        if let Some((&head, tail)) = path.split_first() {
            self.children.as_mut()?.get_mut(head)?.at_mut(tail)
        } else {
            Some(self)
        }
    }
    pub fn count(&self) -> usize {
        1 + self
            .children
            .as_ref()
            .map_or(0, |v| v.iter().map(Self::count).sum())
    }
    fn validate(&self, depth: usize, remaining: &mut usize) -> Result<(), String> {
        if depth > 32 || *remaining == 0 {
            return Err("Condition tree exceeds source limits (depth32,256nodes).".into());
        }
        *remaining -= 1;
        match self.kind.as_str() {
            "group" if matches!(self.operator.as_deref(), Some("all" | "any")) => {
                for child in self
                    .children
                    .as_ref()
                    .ok_or("Missing condition children.")?
                {
                    child.validate(depth + 1, remaining)?;
                }
            }
            "predicate" if self.value.is_some() => {} // Unfamiliar imported clauses remain preservation-only text.
            _ => {
                return Err("Unsupported condition shape; its original record was retained.".into())
            }
        }
        Ok(())
    }
}
impl Default for Rule {
    fn default() -> Self {
        Self {
            priority: 10,
            name: String::new(),
            enabled: true,
            condition: Condition {
                version: 1,
                root: Node::group(false),
                retained: BTreeMap::new(),
            },
            set: "random".into(),
            mode: String::new(),
            retained: BTreeMap::new(),
        }
    }
}
impl Rule {
    pub fn source_version(&mut self) {
        fn advanced(node: &Node) -> bool {
            node.value.as_deref().is_some_and(|v| {
                ["power:", "battery:", "output:", "outputs:"]
                    .iter()
                    .any(|p| v.starts_with(p))
                    || ["sunrise+", "sunrise-", "sunset+", "sunset-"]
                        .iter()
                        .any(|p| v.contains(p))
            }) || node
                .children
                .as_ref()
                .is_some_and(|v| v.iter().any(advanced))
        }
        if self.condition.version <= 2 {
            self.condition.version = if advanced(&self.condition.root) { 2 } else { 1 };
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if ![1, 2].contains(&self.condition.version)
            || !["", "light", "dark"].contains(&self.mode.as_str())
        {
            return Err("Unsupported rule version or Theme mode; original data retained.".into());
        }
        if self.condition.root.kind != "group" {
            return Err("The rule requires a root All/Any group.".into());
        }
        self.condition.root.validate(0, &mut 256)
    }
}
pub fn validate_records(value: &Value) -> Result<(), String> {
    let rules: Vec<Rule> = serde_json::from_value(value.clone())
        .map_err(|_| "Unsupported saved rule data; original records retained.")?;
    for rule in rules {
        rule.validate()?;
    }
    Ok(())
}
pub fn valid_time(text: &str) -> bool {
    for solar in ["sunrise", "sunset"] {
        if let Some(rest) = text.strip_prefix(solar) {
            return rest.is_empty() || rest.parse::<i32>().is_ok_and(|n| (-720..=720).contains(&n));
        }
    }
    text.split_once(':').is_some_and(|(h, m)| {
        h.parse::<u32>().is_ok_and(|n| n < 24) && m.parse::<u32>().is_ok_and(|n| n < 60)
    })
}
fn comparison(text: &str) -> (&str, &str) {
    for op in [">=", "<=", ">", "<"] {
        if let Some(value) = text.strip_prefix(op) {
            return (op, value);
        }
    }
    ("", text)
}
pub fn valid_predicate(text: &str) -> bool {
    let Some((kind, value)) = text.split_once(':') else {
        return false;
    };
    match kind {
        "weekday" => !value.is_empty() && value.split(',').all(|v| WEEKDAYS.contains(&v)),
        "weather" => !value.is_empty() && value.split(',').all(|v| WEATHER.contains(&v)),
        "time" => {
            if let Some((a, b)) = value.split_once("..") {
                valid_time(a) && valid_time(b)
            } else {
                let (op, v) = comparison(value);
                !op.is_empty() && valid_time(v)
            }
        }
        "date" => {
            let dates: Vec<_> = value.split("..").collect();
            dates.len() <= 2
                && dates.iter().all(|date| {
                    let parts: Vec<_> = date.split('-').collect();
                    let fields = if parts.len() == 3
                        && parts[0].len() == 4
                        && parts[0].parse::<u32>().is_ok_and(|v| v > 0)
                    {
                        &parts[1..]
                    } else {
                        parts.as_slice()
                    };
                    fields.len() == 2
                        && fields[0]
                            .parse::<u32>()
                            .is_ok_and(|v| (1..=12).contains(&v))
                        && fields[1]
                            .parse::<u32>()
                            .is_ok_and(|v| (1..=31).contains(&v))
                })
        }
        "year" => {
            let (_, v) = comparison(value);
            v.len() == 4 && v.parse::<i64>().is_ok()
        }
        "power" => ["battery", "external"].contains(&value),
        "battery" => comparison(value).1.parse::<u8>().is_ok_and(|n| n <= 100),
        "output" => !value.is_empty() && value.len() <= 128,
        "outputs" => comparison(value).1.parse::<u32>().is_ok_and(|n| n <= 64),
        _ => false,
    }
}

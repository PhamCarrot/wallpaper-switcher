//! Local intent and confirmed usage. Catalog/source metadata is never written here.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub type Retained = BTreeMap<String, toml::Value>;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalLibraryState {
    pub tag_overrides: BTreeMap<String, TagOverride>,
    pub usage: UsageHistory,
    /// Absent means legacy desired policy has never been deliberately activated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rotation: Option<RotationPreferences>,
    pub startup: StartupPreferences,
    #[serde(flatten)]
    pub retained: Retained,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UsageHistory {
    pub items: BTreeMap<String, ItemUsage>,
    pub last_successful_random_id: Option<String>,
    pub targets: BTreeMap<String, ConfirmedAssignment>,
    #[serde(flatten)]
    pub retained: Retained,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ItemUsage {
    #[serde(with = "count_codec")]
    pub confirmed_count: u64,
    pub last_confirmed_unix: u64,
    #[serde(flatten)]
    pub retained: Retained,
}

// TOML integers are signed; preserve a saturated u64 without losing precision.
mod count_codec {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
        if *value <= i64::MAX as u64 {
            serializer.serialize_u64(*value)
        } else {
            serializer.serialize_str(&value.to_string())
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Count {
            Number(u64),
            Text(String),
        }
        match Count::deserialize(d)? {
            Count::Number(n) => Ok(n),
            Count::Text(s) => s.parse().map_err(serde::de::Error::custom),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ConfirmedAssignment {
    pub item_id: String,
    pub source: String,
    pub kind: String,
    pub file_target: std::path::PathBuf,
    pub project_json_path: std::path::PathBuf,
    #[serde(flatten)]
    pub retained: Retained,
}

impl ConfirmedAssignment {
    pub fn capture(item: &crate::scanner::WallpaperItem) -> Self {
        Self {
            item_id: item.id.clone(),
            source: source_key(item.source).into(),
            kind: item.wallpaper_type.clone(),
            file_target: item.file_target.clone(),
            project_json_path: item.project_json_path.clone(),
            retained: Default::default(),
        }
    }
    pub fn matches(&self, item: &crate::scanner::WallpaperItem) -> bool {
        self.item_id == item.id
            && self.source == source_key(item.source)
            && self.kind == item.wallpaper_type
            && self.file_target == item.file_target
            && self.project_json_path == item.project_json_path
    }
}

pub fn source_key(source: crate::scanner::WallpaperSource) -> &'static str {
    use crate::scanner::WallpaperSource;
    match source {
        WallpaperSource::LocalImage => "image",
        WallpaperSource::LocalVideo => "video",
        WallpaperSource::WallpaperEngine => "we",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RotationPreferences {
    pub images: bool,
    pub videos: bool,
    pub we: bool,
    pub favourites_only: bool,
    #[serde(flatten)]
    pub retained: Retained,
}
impl Default for RotationPreferences {
    fn default() -> Self {
        // Inspected effective schema defaults for media; W6 deliberately changes favourites.
        Self {
            images: true,
            videos: true,
            we: true,
            favourites_only: false,
            retained: Default::default(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StartupPreferences {
    pub reapply_enabled: bool,
    #[serde(flatten)]
    pub retained: Retained,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct TagOverride {
    pub added: BTreeSet<String>,
    pub removed: BTreeSet<String>,
    #[serde(flatten)]
    pub retained: Retained,
}

pub fn normalize_tag(raw: &str) -> Result<String, String> {
    let tag = raw.trim().to_lowercase();
    if tag.is_empty() || tag.len() > 128 || tag.chars().any(char::is_control) {
        return Err("Tags must contain 1–128 UTF-8 bytes and no control characters.".into());
    }
    Ok(tag)
}

pub fn validate_retained(data: &Retained) -> Result<(), String> {
    fn count(value: &toml::Value, depth: usize, nodes: &mut usize) -> bool {
        *nodes += 1;
        if depth > 16 || *nodes > 4096 {
            return false;
        }
        match value {
            toml::Value::Table(t) => t
                .iter()
                .all(|(k, v)| k.len() <= 256 && count(v, depth + 1, nodes)),
            toml::Value::Array(a) => a.iter().all(|v| count(v, depth + 1, nodes)),
            _ => true,
        }
    }
    let mut nodes = 0;
    if data.len() > 128
        || !data
            .iter()
            .all(|(k, v)| k.len() <= 256 && count(v, 0, &mut nodes))
        || toml::to_string(data).map_err(|e| e.to_string())?.len() > 65_536
    {
        return Err("Retained inactive data exceeds the 64 KiB/4096-value/16-level local record bound; original files were retained.".into());
    }
    Ok(())
}

impl LocalLibraryState {
    pub fn effective_tags(&self, id: &str, source: &BTreeSet<String>) -> BTreeSet<String> {
        let mut tags = source.clone();
        if let Some(local) = self.tag_overrides.get(id) {
            tags.extend(local.added.iter().cloned());
            tags.retain(|tag| !local.removed.contains(tag));
        }
        tags
    }

    pub fn edit(&mut self, id: &str, raw: &str, add: bool) -> Result<(), String> {
        let tag = normalize_tag(raw)?;
        let mut proposed = self.clone();
        let local = proposed.tag_overrides.entry(id.into()).or_default();
        if add {
            local.removed.remove(&tag);
            local.added.insert(tag);
        } else {
            local.added.remove(&tag);
            local.removed.insert(tag);
        }
        proposed.validate()?;
        *self = proposed;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), String> {
        validate_retained(&self.retained)?;
        self.usage.validate()?;
        validate_retained(&self.startup.retained)?;
        if let Some(rotation) = &self.rotation {
            validate_retained(&rotation.retained)?;
        }
        if self.tag_overrides.len() > 4096 {
            return Err("At most 4096 local item tag records are supported.".into());
        }
        for (id, local) in &self.tag_overrides {
            if id.is_empty() || id.len() > 4096 || local.added.len() + local.removed.len() > 256 {
                return Err("Local tag identity or set exceeds its bound.".into());
            }
            validate_retained(&local.retained)?;
            for tag in local.added.iter().chain(&local.removed) {
                if normalize_tag(tag)? != *tag {
                    return Err("Local tags must be normalized.".into());
                }
            }
            if !local.added.is_disjoint(&local.removed) {
                return Err("Local added/removed tags overlap.".into());
            }
        }
        Ok(())
    }

    pub fn has_retained(&self) -> bool {
        !self.retained.is_empty()
            || self.tag_overrides.values().any(|r| !r.retained.is_empty())
            || !self.usage.retained.is_empty()
            || self.usage.items.values().any(|r| !r.retained.is_empty())
            || self.usage.targets.values().any(|r| !r.retained.is_empty())
            || !self.startup.retained.is_empty()
            || self
                .rotation
                .as_ref()
                .is_some_and(|r| !r.retained.is_empty())
    }
}

impl UsageHistory {
    pub fn validate(&self) -> Result<(), String> {
        validate_retained(&self.retained)?;
        let valid_id =
            |id: &str| !id.is_empty() && id.len() <= 4096 && !id.chars().any(char::is_control);
        if self.items.len() > 16_384
            || self.targets.len() > 128
            || self
                .last_successful_random_id
                .as_deref()
                .is_some_and(|id| !valid_id(id))
        {
            return Err("Usage history exceeds its bounded item/target identity limits; original files retained.".into());
        }
        for (id, usage) in &self.items {
            if !valid_id(id) || usage.last_confirmed_unix > i64::MAX as u64 {
                return Err("Invalid usage identity/time.".into());
            }
            validate_retained(&usage.retained)?;
        }
        for (target, binding) in &self.targets {
            if !valid_id(target)
                || !valid_id(&binding.item_id)
                || !matches!(binding.source.as_str(), "image" | "video" | "we")
                || binding.kind.len() > 128
                || binding.file_target.as_os_str().len() > 32_768
                || binding.project_json_path.as_os_str().len() > 32_768
            {
                return Err("Invalid confirmed target/source assignment.".into());
            }
            validate_retained(&binding.retained)?;
        }
        Ok(())
    }
}

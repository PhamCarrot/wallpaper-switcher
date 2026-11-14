//! Durable local frontend data. Pending records have no execution state.
use crate::theme_roles::{ColorVariant, ThemeDocument};
use serde::{Deserialize, Serialize};

#[path = "frontend/starter_palettes.rs"]
mod starter_palettes;
pub use starter_palettes::starter_palettes;

pub fn nord_document() -> ThemeDocument {
    serde_json::from_str(include_str!("../../assets/themes/nord.json"))
        .expect("P0 source-derived complete Nord fixture")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ThemeOrigin {
    BuiltInNord,
    Custom { record_id: u64 },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedThemeSnapshot {
    pub origin: ThemeOrigin,
    pub variant: ColorVariant,
    pub document: ThemeDocument,
}
impl Default for AppliedThemeSnapshot {
    fn default() -> Self {
        Self {
            origin: ThemeOrigin::BuiltInNord,
            variant: ColorVariant::Dark,
            document: nord_document(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NamedThemeRecord {
    pub id: u64,
    pub name: String,
    pub document: ThemeDocument,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeLibrary {
    pub next_id: u64,
    pub records: Vec<NamedThemeRecord>,
}
impl Default for ThemeLibrary {
    fn default() -> Self {
        Self {
            next_id: 1,
            records: Vec::new(),
        }
    }
}
impl ThemeLibrary {
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::BTreeSet::new();
        let mut names = std::collections::BTreeSet::new();
        for entry in &self.records {
            if entry.id == 0 || entry.id >= self.next_id || !ids.insert(entry.id) {
                return Err("Invalid or duplicate Theme identity/counter.".into());
            }
            if entry.name.trim().is_empty() || !names.insert(entry.name.as_str()) {
                return Err("Empty or duplicate Theme name.".into());
            }
        }
        if self.next_id == 0 {
            return Err("Invalid Theme identity counter.".into());
        }
        Ok(())
    }
    /// The controller owns overwrite confirmation. This helper only updates
    /// an already resolved ID and never changes an applied snapshot.
    pub fn save(
        &mut self,
        target: Option<u64>,
        name: &str,
        document: ThemeDocument,
    ) -> Result<u64, String> {
        self.validate()?;
        let name = name.trim();
        if name.is_empty() {
            return Err("Enter a Theme name.".into());
        }
        if self
            .records
            .iter()
            .any(|r| r.name == name && Some(r.id) != target)
        {
            return Err("A different Theme already has that exact name.".into());
        }
        if let Some(id) = target {
            let entry = self
                .records
                .iter_mut()
                .find(|r| r.id == id)
                .ok_or("Theme no longer exists.")?;
            entry.name = name.into();
            entry.document = document;
            return Ok(id);
        }
        let id = self.next_id;
        let next = id.checked_add(1).ok_or("Theme identity exhausted.")?;
        self.records.push(NamedThemeRecord {
            id,
            name: name.into(),
            document,
        });
        self.next_id = next;
        Ok(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeferredCapability {
    ThemeGeneration,
    WallpaperBoundProfiles,
    ExpandedScheduling,
    ExtraImageEffects,
    SourceBrowsing,
    SemanticSearch,
    MediaPlaybackPolicy,
    PlatformIntegration,
    Optimization,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PendingOnly {
    Pending,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingSetting {
    pub capability: DeferredCapability,
    pub source_path: String,
    pub value_json: String,
    pub status: PendingOnly,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingPreset {
    pub id: u64,
    pub capability: DeferredCapability,
    pub name: String,
    pub settings: Vec<PendingSetting>,
    pub status: PendingOnly,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PendingFrontendState {
    pub next_id: u64,
    pub settings: Vec<PendingSetting>,
    pub presets: Vec<PendingPreset>,
}
impl Default for PendingFrontendState {
    fn default() -> Self {
        Self {
            next_id: 1,
            settings: Vec::new(),
            presets: Vec::new(),
        }
    }
}
impl PendingFrontendState {
    pub fn validate(&self) -> Result<(), String> {
        fn settings(entries: &[PendingSetting]) -> Result<(), String> {
            let mut paths = std::collections::BTreeSet::new();
            for entry in entries {
                if entry.source_path.is_empty()
                    || !paths.insert((format!("{:?}", entry.capability), &entry.source_path))
                {
                    return Err("Empty or duplicate pending source identity.".into());
                }
                serde_json::from_str::<serde_json::Value>(&entry.value_json)
                    .map_err(|_| "Invalid pending JSON value.")?;
            }
            Ok(())
        }
        settings(&self.settings)?;
        let mut ids = std::collections::BTreeSet::new();
        if self.next_id == 0 {
            return Err("Invalid pending identity counter.".into());
        }
        for preset in &self.presets {
            if preset.id == 0 || preset.id >= self.next_id || !ids.insert(preset.id) {
                return Err("Invalid or duplicate pending identity/counter.".into());
            }
            settings(&preset.settings)?;
        }
        Ok(())
    }
}

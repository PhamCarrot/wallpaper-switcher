//! Mode-local profile identities and snapshots. Shared flip settings are absent
//! by construction. P3 holds this data locally; P4 adds existing-owner codec IO.
use crate::candidate_state::{GalleryPreset, GeometricCurve, GeometricShape, WallLayout};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageTransform {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub rotation_degrees: f32,
    pub perspective: f32,
    pub shear_x: f32,
    pub shear_y: f32,
    pub depth_angle_degrees: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlicesProfileSnapshot {
    pub shadows: bool,
    pub parallax: bool,
    pub offset_x: f32,
    pub offset_y: f32,
    pub height: f32,
    pub visible_count: u64,
    pub expanded_width: f32,
    pub width: f32,
    pub spacing: f32,
    pub skew_distance: f32,
    pub edge_tilt_distance: f32,
    pub wobble: bool,
    pub wobble_strength: f32,
    pub round_corners: bool,
    pub corners: [f32; 4],
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HexProfileSnapshot {
    pub parallax: bool,
    pub radius: f32,
    pub rows: u64,
    pub columns: u64,
    pub scroll_step: u64,
    pub arc: bool,
    pub curve_strength: f32,
    pub curve: GeometricCurve,
    pub shape: GeometricShape,
    pub curve_frequency: f32,
    pub gap_x: f32,
    pub gap_y: f32,
    pub aspect: f32,
    pub stagger: f32,
    pub lens: f32,
    pub lens_radius: f32,
    pub orbit: f32,
    pub orbit_radius: f32,
    pub twist: f32,
    pub scatter: f32,
    pub stage: StageTransform,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WallProfileSnapshot {
    pub columns: u64,
    pub rows: u64,
    pub card_width: f32,
    pub card_height: f32,
    pub gap_x: f32,
    pub gap_y: f32,
    pub round_corners: bool,
    pub corner_radius: f32,
    pub border_width: f32,
    pub layout: WallLayout,
    pub stagger: f32,
    pub selected_scale: f32,
    pub flow_wave: f32,
    pub flow_frequency: f32,
    pub scatter: f32,
    pub scale_variance: f32,
    pub cylinder_bend: f32,
    pub cylinder_radius: f32,
    pub stage: StageTransform,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DepthProfileSnapshot {
    pub shadows: bool,
    pub selection_frame: bool,
    pub height: f32,
    pub width: f32,
    pub spacing: f32,
    pub visible_count: f32,
    pub falloff: f32,
    pub corner_radius: f32,
    pub skew_distance: f32,
    pub navigation_ms: f32,
}

pub use crate::candidate_state::GalleryKind as Mode;
impl Mode {
    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "slices" => Self::Slices,
            "hex" => Self::Geometric,
            "wall" | "grid" => Self::Wall,
            "depth" => Self::Depth,
            _ => return None,
        })
    }
    pub fn key(self) -> &'static str {
        match self {
            Self::Slices => "slices",
            Self::Geometric => "hex",
            Self::Wall => "wall",
            Self::Depth => "depth",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct NamedModeProfile<T> {
    pub id: u64,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub snapshot: Option<T>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub legacy_snapshot: Option<GalleryPreset>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(bound(deserialize = "T: Deserialize<'de>"))]
pub struct ModeProfileList<T> {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_id: Option<u64>,
    pub entries: Vec<NamedModeProfile<T>>,
}
impl<T> Default for ModeProfileList<T> {
    fn default() -> Self {
        Self {
            selected_id: None,
            entries: Vec::new(),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileRow {
    pub id: u64,
    pub name: String,
    pub selected: bool,
    pub available: bool,
}
impl<T> ModeProfileList<T> {
    pub fn rows(&self) -> Vec<ProfileRow> {
        self.entries
            .iter()
            .map(|entry| ProfileRow {
                id: entry.id,
                name: entry.name.clone(),
                selected: self.selected_id == Some(entry.id),
                available: entry.snapshot.is_some(),
            })
            .collect()
    }
    pub fn selected(&self) -> Option<&NamedModeProfile<T>> {
        self.selected_id
            .and_then(|id| self.entries.iter().find(|entry| entry.id == id))
    }
    pub fn rename_selected(&mut self, id: u64, raw: &str) -> bool {
        if self.selected_id != Some(id) {
            return false;
        }
        let name = raw.trim();
        if name.is_empty() {
            return false;
        }
        let Some(entry) = self.entries.iter_mut().find(|entry| entry.id == id) else {
            return false;
        };
        if entry.name == name {
            return false;
        }
        entry.name = name.into();
        true
    }
    pub fn delete(&mut self, id: u64) -> bool {
        let Some(index) = self.entries.iter().position(|entry| entry.id == id) else {
            return false;
        };
        self.entries.remove(index);
        if self.selected_id == Some(id) {
            self.selected_id = None;
        }
        true
    }
    fn save(
        &mut self,
        id: u64,
        snapshot: T,
        label: impl Fn(usize) -> String,
    ) -> Result<(), String> {
        if self.entries.iter().any(|entry| entry.id == id) {
            return Err("Profile identity already exists.".into());
        }
        let name = (1..=self.entries.len() + 1)
            .map(label)
            .find(|name| !self.entries.iter().any(|entry| entry.name == *name))
            .ok_or("Could not generate a unique profile label.")?;
        self.entries.push(NamedModeProfile {
            id,
            name,
            snapshot: Some(snapshot),
            legacy_snapshot: None,
        });
        self.selected_id = Some(id);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Snapshot {
    Slices(SlicesProfileSnapshot),
    Hex(HexProfileSnapshot),
    Wall(WallProfileSnapshot),
    Depth(DepthProfileSnapshot),
}
impl Snapshot {
    pub fn mode(&self) -> Mode {
        match self {
            Self::Slices(_) => Mode::Slices,
            Self::Hex(_) => Mode::Geometric,
            Self::Wall(_) => Mode::Wall,
            Self::Depth(_) => Mode::Depth,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModeProfiles {
    pub next_id: u64,
    pub slices: ModeProfileList<SlicesProfileSnapshot>,
    pub hex: ModeProfileList<HexProfileSnapshot>,
    pub wall: ModeProfileList<WallProfileSnapshot>,
    pub depth: ModeProfileList<DepthProfileSnapshot>,
}
impl Default for ModeProfiles {
    fn default() -> Self {
        Self {
            next_id: 1,
            slices: Default::default(),
            hex: Default::default(),
            wall: Default::default(),
            depth: Default::default(),
        }
    }
}
impl ModeProfiles {
    pub fn rows(&self, mode: Mode) -> Vec<ProfileRow> {
        match mode {
            Mode::Slices => self.slices.rows(),
            Mode::Geometric => self.hex.rows(),
            Mode::Wall => self.wall.rows(),
            Mode::Depth => self.depth.rows(),
        }
    }
    pub fn selected_id(&self, mode: Mode) -> Option<u64> {
        match mode {
            Mode::Slices => self.slices.selected_id,
            Mode::Geometric => self.hex.selected_id,
            Mode::Wall => self.wall.selected_id,
            Mode::Depth => self.depth.selected_id,
        }
    }
    pub fn selected_name(&self, mode: Mode) -> Option<&str> {
        match mode {
            Mode::Slices => self.slices.selected().map(|entry| entry.name.as_str()),
            Mode::Geometric => self.hex.selected().map(|entry| entry.name.as_str()),
            Mode::Wall => self.wall.selected().map(|entry| entry.name.as_str()),
            Mode::Depth => self.depth.selected().map(|entry| entry.name.as_str()),
        }
    }
    pub fn save(
        &mut self,
        snapshot: Snapshot,
        label: impl Fn(usize) -> String,
    ) -> Result<u64, String> {
        let id = self.next_id;
        let next = id
            .checked_add(1)
            .ok_or("Profile identities are exhausted.")?;
        if id == 0
            || [Mode::Slices, Mode::Geometric, Mode::Wall, Mode::Depth]
                .into_iter()
                .any(|mode| self.rows(mode).iter().any(|entry| entry.id == id))
        {
            return Err("Invalid next profile identity.".into());
        }
        match snapshot {
            Snapshot::Slices(value) => self.slices.save(id, value, label)?,
            Snapshot::Hex(value) => self.hex.save(id, value, label)?,
            Snapshot::Wall(value) => self.wall.save(id, value, label)?,
            Snapshot::Depth(value) => self.depth.save(id, value, label)?,
        }
        self.next_id = next;
        Ok(id)
    }
    pub fn rename_selected(&mut self, mode: Mode, id: u64, name: &str) -> bool {
        match mode {
            Mode::Slices => self.slices.rename_selected(id, name),
            Mode::Geometric => self.hex.rename_selected(id, name),
            Mode::Wall => self.wall.rename_selected(id, name),
            Mode::Depth => self.depth.rename_selected(id, name),
        }
    }
    pub fn delete(&mut self, mode: Mode, id: u64) -> bool {
        match mode {
            Mode::Slices => self.slices.delete(id),
            Mode::Geometric => self.hex.delete(id),
            Mode::Wall => self.wall.delete(id),
            Mode::Depth => self.depth.delete(id),
        }
    }
    /// Resolve only the exact mode/ID; selection changes after actual acceptance.
    pub fn snapshot(&self, mode: Mode, id: u64) -> Option<Snapshot> {
        match mode {
            Mode::Slices => self
                .slices
                .entries
                .iter()
                .find(|entry| entry.id == id)?
                .snapshot
                .clone()
                .map(Snapshot::Slices),
            Mode::Geometric => self
                .hex
                .entries
                .iter()
                .find(|entry| entry.id == id)?
                .snapshot
                .clone()
                .map(Snapshot::Hex),
            Mode::Wall => self
                .wall
                .entries
                .iter()
                .find(|entry| entry.id == id)?
                .snapshot
                .clone()
                .map(Snapshot::Wall),
            Mode::Depth => self
                .depth
                .entries
                .iter()
                .find(|entry| entry.id == id)?
                .snapshot
                .clone()
                .map(Snapshot::Depth),
        }
    }
    pub fn select_for_rename(&mut self, mode: Mode, id: u64) -> bool {
        if !self.rows(mode).iter().any(|entry| entry.id == id) {
            return false;
        }
        match mode {
            Mode::Slices => self.slices.selected_id = Some(id),
            Mode::Geometric => self.hex.selected_id = Some(id),
            Mode::Wall => self.wall.selected_id = Some(id),
            Mode::Depth => self.depth.selected_id = Some(id),
        }
        true
    }
    pub fn select(&mut self, mode: Mode, id: u64) -> bool {
        if self.snapshot(mode, id).is_none() {
            return false;
        }
        match mode {
            Mode::Slices => self.slices.selected_id = Some(id),
            Mode::Geometric => self.hex.selected_id = Some(id),
            Mode::Wall => self.wall.selected_id = Some(id),
            Mode::Depth => self.depth.selected_id = Some(id),
        }
        true
    }
}

/// Reuse the existing current-renderer bounds without normalizing the actual
/// proposal or changing other modes. P4 validates new source-only codec fields.
pub(crate) fn current_geometry_is_valid(gallery: &super::GallerySettings) -> bool {
    match gallery.mode {
        Mode::Slices => {
            let mut value = gallery.slices.clone();
            super::sanitize_slices(&mut value);
            value == gallery.slices
        }
        Mode::Geometric => {
            let mut value = gallery.geometric.clone();
            super::sanitize_geometric(&mut value);
            value == gallery.geometric
        }
        Mode::Wall => {
            let mut value = gallery.wall.clone();
            super::sanitize_wall(&mut value);
            value == gallery.wall
        }
        Mode::Depth => {
            let mut before = gallery.depth.clone();
            before.detail = super::DepthDetailSettings::default();
            let mut value = before.clone();
            super::sanitize_depth(&mut value, &mut Vec::new());
            value == before
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn depth(height: f32) -> Snapshot {
        Snapshot::Depth(DepthProfileSnapshot {
            shadows: false,
            selection_frame: true,
            height,
            width: 280.0,
            spacing: 280.0,
            visible_count: 5.25,
            falloff: 0.8,
            corner_radius: 4.0,
            skew_distance: -2.0,
            navigation_ms: 250.5,
        })
    }
    #[test]
    fn generated_save_rename_duplicate_labels_and_exact_delete_keep_identities() {
        let mut profiles = ModeProfiles::default();
        let a = profiles
            .save(depth(500.0), |n| format!("Layout {n}"))
            .unwrap();
        let b = profiles
            .save(depth(700.0), |n| format!("Layout {n}"))
            .unwrap();
        assert_ne!(a, b);
        assert_eq!(profiles.selected_name(Mode::Depth), Some("Layout 2"));
        assert!(!profiles.rename_selected(Mode::Depth, a, "wrong owner"));
        assert!(!profiles.rename_selected(Mode::Depth, b, "  "));
        assert!(profiles.rename_selected(Mode::Depth, b, " Layout 1 "));
        assert_eq!(
            profiles
                .rows(Mode::Depth)
                .iter()
                .map(|entry| entry.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Layout 1", "Layout 1"]
        );
        assert_eq!(profiles.snapshot(Mode::Depth, a), Some(depth(500.0)));
        assert_eq!(profiles.snapshot(Mode::Depth, b), Some(depth(700.0)));
        assert!(profiles.delete(Mode::Depth, a));
        assert_eq!(profiles.selected_id(Mode::Depth), Some(b));
        let c = profiles
            .save(depth(900.0), |n| format!("Layout {n}"))
            .unwrap();
        assert_eq!(profiles.selected_name(Mode::Depth), Some("Layout 2"));
        assert!(c > b);
    }
    #[test]
    fn unavailable_snapshots_and_exhaustion_do_not_publish_selection_or_mutate_lists() {
        let mut profiles = ModeProfiles::default();
        profiles.depth.entries.push(NamedModeProfile {
            id: 1,
            name: "Legacy".into(),
            snapshot: None,
            legacy_snapshot: None,
        });
        profiles.next_id = 2;
        assert!(profiles.snapshot(Mode::Depth, 1).is_none());
        assert!(!profiles.select(Mode::Depth, 1));
        assert!(profiles.select_for_rename(Mode::Depth, 1));
        assert!(profiles.rename_selected(Mode::Depth, 1, "Retained legacy"));
        assert!(profiles.snapshot(Mode::Depth, 1).is_none());
        profiles.next_id = u64::MAX;
        let before = profiles.clone();
        assert!(profiles.save(depth(500.0), |n| n.to_string()).is_err());
        assert_eq!(profiles, before);
        assert!(profiles.delete(Mode::Depth, 1));
        assert!(profiles.depth.entries.is_empty());
    }
}

impl Default for StageTransform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            rotation_degrees: 0.0,
            perspective: 0.0,
            shear_x: 0.0,
            shear_y: 0.0,
            depth_angle_degrees: 0.0,
        }
    }
}
impl Snapshot {
    /// Only fields present in the legacy selected mode are projected. The
    /// original whole preset remains separately preserved, including absence.
    fn from_legacy(preset: &GalleryPreset) -> Option<Self> {
        Some(match preset.mode {
            Mode::Slices => {
                let g = &preset.slices;
                Self::Slices(SlicesProfileSnapshot {
                    shadows: true,
                    parallax: false,
                    offset_x: g.offset_x,
                    offset_y: g.offset_y,
                    height: g.height,
                    visible_count: g.visible_count as u64,
                    expanded_width: g.expanded_width,
                    width: g.width,
                    spacing: g.spacing,
                    skew_distance: g.skew_distance,
                    edge_tilt_distance: g.edge_tilt_distance,
                    wobble: g.wobble,
                    wobble_strength: g.wobble_strength,
                    round_corners: g.corner_radius > 0.0,
                    corners: [g.corner_radius; 4],
                })
            }
            Mode::Geometric => {
                let g = &preset.geometric;
                Self::Hex(HexProfileSnapshot {
                    parallax: false,
                    radius: g.radius,
                    rows: g.rows as u64,
                    columns: g.columns as u64,
                    scroll_step: 1,
                    arc: g.curve != GeometricCurve::Flat,
                    curve_strength: g.curve_strength,
                    curve: g.curve,
                    shape: g.shape,
                    curve_frequency: g.curve_frequency,
                    gap_x: g.gap_x,
                    gap_y: g.gap_y,
                    aspect: g.aspect,
                    stagger: g.stagger,
                    lens: g.lens,
                    lens_radius: 620.0,
                    orbit: g.orbit,
                    orbit_radius: g.orbit_radius,
                    twist: g.twist,
                    scatter: g.scatter,
                    stage: StageTransform::default(),
                })
            }
            Mode::Wall => {
                let g = &preset.wall;
                Self::Wall(WallProfileSnapshot {
                    columns: g.columns as u64,
                    rows: g.rows as u64,
                    card_width: g.card_width,
                    card_height: g.card_height,
                    gap_x: g.gap_x,
                    gap_y: g.gap_y,
                    round_corners: true,
                    corner_radius: g.corner_radius,
                    border_width: 2.0,
                    layout: g.layout,
                    stagger: g.stagger,
                    selected_scale: g.selected_scale,
                    flow_wave: g.flow_wave,
                    flow_frequency: g.flow_frequency,
                    scatter: g.scatter,
                    scale_variance: g.scale_variance,
                    cylinder_bend: g.cylinder_bend,
                    cylinder_radius: g.cylinder_radius,
                    stage: StageTransform::default(),
                })
            }
            Mode::Depth => {
                let g = preset.depth.as_ref()?.effective_current();
                Self::Depth(DepthProfileSnapshot {
                    shadows: g.shadows,
                    selection_frame: g.selection_frame,
                    height: g.height,
                    width: g.width,
                    spacing: g.spacing,
                    visible_count: g.visible_count,
                    falloff: g.falloff,
                    corner_radius: g.corner_radius,
                    skew_distance: g.skew_distance,
                    navigation_ms: g.navigation_ms,
                })
            }
        })
    }
}
impl ModeProfiles {
    pub fn from_legacy(presets: &[GalleryPreset], source_version: i64) -> Result<Self, String> {
        let mut result = Self::default();
        for preset in presets {
            let id = result.next_id;
            result.next_id = id.checked_add(1).ok_or("Profile identity exhausted.")?;
            let mut effective = preset.clone();
            if source_version < 2 {
                effective.depth = None;
            }
            super::sanitize_slices(&mut effective.slices);
            super::sanitize_geometric(&mut effective.geometric);
            super::sanitize_wall(&mut effective.wall);
            let snapshot = Snapshot::from_legacy(&effective);
            macro_rules! append {
                ($list:ident, $snapshot:expr) => {
                    result.$list.entries.push(NamedModeProfile {
                        id,
                        name: preset.name.clone(),
                        snapshot: $snapshot,
                        legacy_snapshot: Some(preset.clone()),
                    })
                };
            }
            match snapshot {
                Some(Snapshot::Slices(s)) => append!(slices, Some(s)),
                Some(Snapshot::Hex(s)) => append!(hex, Some(s)),
                Some(Snapshot::Wall(s)) => append!(wall, Some(s)),
                Some(Snapshot::Depth(s)) => append!(depth, Some(s)),
                None => append!(depth, None),
            }
        }
        Ok(result)
    }
    pub fn validate(&self) -> Result<(), String> {
        fn list<T: Serialize>(
            list: &ModeProfileList<T>,
            next: u64,
            ids: &mut std::collections::BTreeSet<u64>,
        ) -> Result<(), String> {
            fn finite(value: &serde_json::Value) -> bool {
                match value {
                    serde_json::Value::Null => false,
                    serde_json::Value::Array(v) => v.iter().all(finite),
                    serde_json::Value::Object(v) => v.values().all(finite),
                    _ => true,
                }
            }
            for entry in &list.entries {
                if entry.id == 0 || entry.id >= next || !ids.insert(entry.id) {
                    return Err("Invalid or duplicate mode-profile identity/counter.".into());
                }
                if let Some(snapshot) = &entry.snapshot {
                    if !finite(&serde_json::to_value(snapshot).map_err(|e| e.to_string())?) {
                        return Err("Non-finite mode-profile value.".into());
                    }
                }
            }
            if list
                .selected_id
                .is_some_and(|id| !list.entries.iter().any(|entry| entry.id == id))
            {
                return Err("Selected mode-profile identity is missing.".into());
            }
            Ok(())
        }
        if self.next_id == 0 {
            return Err("Invalid mode-profile identity counter.".into());
        }
        let mut ids = std::collections::BTreeSet::new();
        list(&self.slices, self.next_id, &mut ids)?;
        list(&self.hex, self.next_id, &mut ids)?;
        list(&self.wall, self.next_id, &mut ids)?;
        list(&self.depth, self.next_id, &mut ids)
    }
}

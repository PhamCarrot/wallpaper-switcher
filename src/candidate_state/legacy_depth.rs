//! Frozen v2 preset payload. Current float geometry must never rewrite these integers.
use super::{DepthDetailSettings, DepthSettings};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LegacyDepthSettings {
    pub height: f32,
    pub width: f32,
    pub spacing: f32,
    pub visible_count: usize,
    pub falloff: f32,
    pub corner_radius: f32,
    pub skew_distance: f32,
    pub navigation_ms: u64,
    pub selection_frame: bool,
    pub shadows: bool,
    pub detail: DepthDetailSettings,
}

impl Default for LegacyDepthSettings {
    fn default() -> Self {
        let geometry = crate::donor_layout::DepthParams::default();
        Self {
            height: geometry.height,
            width: geometry.width,
            spacing: geometry.spacing,
            visible_count: geometry.visible_count,
            falloff: geometry.falloff,
            corner_radius: geometry.corner_radius,
            skew_distance: geometry.skew,
            navigation_ms: 1000,
            selection_frame: false,
            shadows: false,
            detail: DepthDetailSettings::default(),
        }
    }
}

impl LegacyDepthSettings {
    pub(crate) fn current_projection(&self) -> DepthSettings {
        DepthSettings {
            offset_x: 0.0,
            offset_y: 0.0,
            height: self.height,
            width: self.width,
            spacing: self.spacing,
            // Clamp historical integer values BEFORE converting the effective copy.
            visible_count: self.visible_count.clamp(3, 21) as f32,
            falloff: self.falloff,
            corner_radius: self.corner_radius,
            skew_distance: self.skew_distance,
            navigation_ms: self.navigation_ms.clamp(35, 8000) as f32,
            selection_frame: self.selection_frame,
            shadows: self.shadows,
            detail: self.detail.clone(),
        }
    }
    pub(crate) fn effective_current(&self) -> DepthSettings {
        let mut current = self.current_projection();
        super::sanitize_depth(&mut current, &mut Vec::new());
        current
    }
}

// Historical fixture construction only. Production never authors old preset records.
#[cfg(test)]
impl TryFrom<&DepthSettings> for LegacyDepthSettings {
    type Error = &'static str;
    fn try_from(current: &DepthSettings) -> Result<Self, Self::Error> {
        if !current.visible_count.is_finite()
            || current.visible_count < 0.0
            || current.visible_count >= usize::MAX as f32
            || current.visible_count.fract() != 0.0
            || !current.navigation_ms.is_finite()
            || current.navigation_ms < 0.0
            || current.navigation_ms >= u64::MAX as f32
            || current.navigation_ms.fract() != 0.0
        {
            return Err("A historical Depth fixture requires exactly representable integer count and timing.");
        }
        Ok(Self {
            height: current.height,
            width: current.width,
            spacing: current.spacing,
            visible_count: current.visible_count as usize,
            falloff: current.falloff,
            corner_radius: current.corner_radius,
            skew_distance: current.skew_distance,
            navigation_ms: current.navigation_ms as u64,
            selection_frame: current.selection_frame,
            shadows: current.shadows,
            detail: current.detail.clone(),
        })
    }
}

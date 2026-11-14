use super::*;
use crate::{
    apply_trace, candidate_state, donor_back_layout, donor_detail, donor_layout, donor_rail,
    donor_scene, gpu_gallery, library, poster_worker,
};
use view::settings::composed::SettingsSource;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GalleryNumberField {
    DepthHeight,
    DepthWidth,
    DepthSpacing,
    DepthCount,
    DepthFalloff,
    DepthCorners,
    DepthSkew,
    DepthNavigation,
    DepthFlipDuration,

    SliceVisibleCount,
    SliceWidth,
    SliceExpandedWidth,
    SliceHeight,
    SliceSpacing,
    SliceSkewDistance,
    SliceEdgeTilt,
    SliceCornerRadius,
    SliceWobbleStrength,
    GeometricRadius,
    GeometricRows,
    GeometricColumns,
    GeometricGapX,
    GeometricGapY,
    GeometricCurveStrength,
    GeometricCurveFrequency,
    GeometricLens,
    GeometricOrbit,
    GeometricOrbitRadius,
    GeometricTwist,
    GeometricScatter,
    WallColumns,
    WallRows,
    WallCardWidth,
    WallCardHeight,
    WallGapX,
    WallGapY,
    WallStagger,
    WallCornerRadius,
    WallCylinderBend,
    WallCylinderRadius,
}

/// Presented geometry belongs to the existing motion owner; persistent values
/// remain targets. Count approaches as a number before donor's integer cast.
#[derive(Debug, Clone, Copy)]
pub(super) struct DepthPresentation {
    pub(super) params: donor_layout::DepthParams,
    count: f32,
    pub entrance: GalleryMotionState,
    pub launch: donor_scene::LaunchAnimation,
    morph_ms: f32,
}
impl DepthPresentation {
    pub(super) fn target(state: &CandidateState) -> Self {
        let params = gallery_layout_params(state, 0.0, 0.0).depth;
        Self {
            params,
            count: params.visible_count as f32,
            entrance: GalleryMotionState::source(1.0, 1.0, 250.0, 1.0),
            launch: donor_scene::LaunchAnimation::Fade,
            morph_ms: state
                .frontend_preferences
                .interface
                .standard_ms
                .unwrap_or(250.0) as f32,
        }
    }
    pub(super) fn tick(&mut self, target: Self, dt: f32) -> bool {
        if !dt.is_finite() || dt <= 0.0 {
            return self.unsettled(target);
        }
        self.morph_ms = target.morph_ms;
        self.entrance.tick(dt);
        let alpha = 1.0 - (-dt.min(0.05) / (self.morph_ms / 4000.0)).exp();
        fn approach(current: &mut f32, target: f32, alpha: f32, precision: f32) {
            *current += (target - *current) * alpha;
            if (*current - target).abs() < precision {
                *current = target;
            }
        }
        approach(&mut self.params.width, target.params.width, alpha, 0.05);
        approach(&mut self.params.height, target.params.height, alpha, 0.05);
        approach(&mut self.params.spacing, target.params.spacing, alpha, 0.05);
        approach(
            &mut self.params.corner_radius,
            target.params.corner_radius,
            alpha,
            0.05,
        );
        approach(&mut self.params.skew, target.params.skew, alpha, 0.05);
        approach(
            &mut self.params.falloff,
            target.params.falloff,
            alpha,
            0.00001,
        );
        approach(&mut self.count, target.count, alpha, 0.001);
        self.params.visible_count = self.count as usize;
        self.unsettled(target)
    }
    pub(super) fn unsettled(&self, target: Self) -> bool {
        !self.entrance.settled()
            || self.params.width != target.params.width
            || self.params.height != target.params.height
            || self.params.spacing != target.params.spacing
            || self.params.corner_radius != target.params.corner_radius
            || self.params.skew != target.params.skew
            || self.params.falloff != target.params.falloff
            || self.count != target.count
    }
}
pub(super) fn advance_gallery_motion(
    camera: &mut GalleryMotionState,
    selection: &mut HashMap<String, GalleryMotionState>,
    presentation: &mut Option<DepthPresentation>,
    target: DepthPresentation,
    dt: f32,
) -> bool {
    if let Some(current) = presentation {
        camera.set_absolute_extent(current.params.spacing, current.params.width);
    }
    let mut moving = camera.tick(dt);
    for spring in selection.values_mut() {
        moving |= spring.tick(dt);
    }
    if let Some(current) = presentation {
        moving |= current.tick(target, dt);
    }
    moving
}
pub(super) const DEPTH_POSITION_FIELDS: [GalleryNumberField; 7] = [
    GalleryNumberField::DepthHeight,
    GalleryNumberField::DepthWidth,
    GalleryNumberField::DepthSpacing,
    GalleryNumberField::DepthCount,
    GalleryNumberField::DepthFalloff,
    GalleryNumberField::DepthCorners,
    GalleryNumberField::DepthSkew,
];
impl GalleryNumberField {
    pub(super) const fn mode(self) -> GalleryKind {
        match self {
            Self::DepthHeight
            | Self::DepthWidth
            | Self::DepthSpacing
            | Self::DepthCount
            | Self::DepthFalloff
            | Self::DepthCorners
            | Self::DepthSkew
            | Self::DepthNavigation
            | Self::DepthFlipDuration => GalleryKind::Depth,
            Self::SliceVisibleCount
            | Self::SliceWidth
            | Self::SliceExpandedWidth
            | Self::SliceHeight
            | Self::SliceSpacing
            | Self::SliceSkewDistance
            | Self::SliceEdgeTilt
            | Self::SliceCornerRadius
            | Self::SliceWobbleStrength => GalleryKind::Slices,
            Self::GeometricRadius
            | Self::GeometricRows
            | Self::GeometricColumns
            | Self::GeometricGapX
            | Self::GeometricGapY
            | Self::GeometricCurveStrength
            | Self::GeometricCurveFrequency
            | Self::GeometricLens
            | Self::GeometricOrbit
            | Self::GeometricOrbitRadius
            | Self::GeometricTwist
            | Self::GeometricScatter => GalleryKind::Geometric,
            Self::WallColumns
            | Self::WallRows
            | Self::WallCardWidth
            | Self::WallCardHeight
            | Self::WallGapX
            | Self::WallGapY
            | Self::WallStagger
            | Self::WallCornerRadius
            | Self::WallCylinderBend
            | Self::WallCylinderRadius => GalleryKind::Wall,
        }
    }

    pub(super) const fn step(self) -> f32 {
        match self {
            Self::DepthHeight => 10.0,
            Self::DepthWidth => 10.0,
            Self::DepthSpacing => 10.0,
            Self::DepthCount => 1.0,
            Self::DepthFalloff => 0.01,
            Self::DepthCorners => 1.0,
            Self::DepthSkew => 1.0,
            Self::DepthNavigation => 50.0,
            Self::DepthFlipDuration => 50.0,

            Self::SliceVisibleCount
            | Self::GeometricRows
            | Self::GeometricColumns
            | Self::WallColumns
            | Self::WallRows => 1.0,
            Self::SliceWidth
            | Self::WallCardWidth
            | Self::WallCardHeight
            | Self::GeometricRadius
            | Self::WallCylinderRadius => 10.0,
            Self::SliceExpandedWidth | Self::SliceHeight => 10.0,
            Self::SliceSpacing | Self::SliceSkewDistance | Self::SliceEdgeTilt => 2.0,
            Self::SliceCornerRadius => 1.0,
            Self::SliceWobbleStrength
            | Self::GeometricCurveStrength
            | Self::GeometricCurveFrequency => 0.1,
            Self::GeometricGapX | Self::GeometricGapY | Self::WallGapX | Self::WallGapY => 1.0,
            Self::WallStagger | Self::WallCylinderBend | Self::GeometricOrbit => 0.05,
            Self::GeometricLens => 0.05,
            Self::GeometricTwist => 5.0,
            Self::GeometricScatter => 5.0,
            Self::GeometricOrbitRadius => 10.0,
            Self::WallCornerRadius => 1.0,
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::DepthHeight => "Height",
            Self::DepthWidth => "Width",
            Self::DepthSpacing => "Spacing",
            Self::DepthCount => "Card count",
            Self::DepthFalloff => "Falloff",
            Self::DepthCorners => "Corners",
            Self::DepthSkew => "Skew",
            Self::DepthNavigation => "Navigation duration",
            Self::DepthFlipDuration => "Flip duration",

            Self::SliceVisibleCount => "Visible cards",
            Self::SliceWidth => "Card width",
            Self::SliceExpandedWidth => "Selected card width",
            Self::SliceHeight => "Card height",
            Self::SliceSpacing => "Spacing",
            Self::SliceSkewDistance => "Skew distance",
            Self::SliceEdgeTilt => "Edge tilt",
            Self::SliceCornerRadius => "Corner radius",
            Self::SliceWobbleStrength => "Wobble strength",
            Self::GeometricRadius => "Tile radius",
            Self::GeometricRows => "Rows",
            Self::GeometricColumns => "Columns",
            Self::GeometricGapX => "Horizontal gap",
            Self::GeometricGapY => "Vertical gap",
            Self::GeometricCurveStrength => "Curve strength",
            Self::GeometricCurveFrequency => "Curve frequency",
            Self::GeometricLens => "Lens",
            Self::GeometricOrbit => "Orbit",
            Self::GeometricOrbitRadius => "Orbit radius",
            Self::GeometricTwist => "Twist",
            Self::GeometricScatter => "Scatter",
            Self::WallColumns => "Columns",
            Self::WallRows => "Rows",
            Self::WallCardWidth => "Card width",
            Self::WallCardHeight => "Card height",
            Self::WallGapX => "Horizontal gap",
            Self::WallGapY => "Vertical gap",
            Self::WallStagger => "Row stagger",
            Self::WallCornerRadius => "Corner radius",
            Self::WallCylinderBend => "Cylinder bend",
            Self::WallCylinderRadius => "Cylinder radius",
        }
    }
}

pub(super) fn gallery_number_value(state: &CandidateState, field: GalleryNumberField) -> f32 {
    match field {
        GalleryNumberField::DepthHeight => state.gallery.depth.height,
        GalleryNumberField::DepthWidth => state.gallery.depth.width,
        GalleryNumberField::DepthSpacing => state.gallery.depth.spacing,
        GalleryNumberField::DepthCount => state.gallery.depth.visible_count,
        GalleryNumberField::DepthFalloff => state.gallery.depth.falloff,
        GalleryNumberField::DepthCorners => state.gallery.depth.corner_radius,
        GalleryNumberField::DepthSkew => state.gallery.depth.skew_distance,
        GalleryNumberField::DepthNavigation => state.gallery.depth.navigation_ms,
        GalleryNumberField::DepthFlipDuration => state.card_flip().duration_ms as f32,

        GalleryNumberField::SliceVisibleCount => state.gallery.slices.visible_count as f32,
        GalleryNumberField::SliceWidth => state.gallery.slices.width,
        GalleryNumberField::SliceExpandedWidth => state.gallery.slices.expanded_width,
        GalleryNumberField::SliceHeight => state.gallery.slices.height,
        GalleryNumberField::SliceSpacing => state.gallery.slices.spacing,
        GalleryNumberField::SliceSkewDistance => state.gallery.slices.skew_distance,
        GalleryNumberField::SliceEdgeTilt => state.gallery.slices.edge_tilt_distance,
        GalleryNumberField::SliceCornerRadius => state.gallery.slices.corner_radius,
        GalleryNumberField::SliceWobbleStrength => state.gallery.slices.wobble_strength,
        GalleryNumberField::GeometricRadius => state.gallery.geometric.radius,
        GalleryNumberField::GeometricRows => state.gallery.geometric.rows as f32,
        GalleryNumberField::GeometricColumns => state.gallery.geometric.columns as f32,
        GalleryNumberField::GeometricGapX => state.gallery.geometric.gap_x,
        GalleryNumberField::GeometricGapY => state.gallery.geometric.gap_y,
        GalleryNumberField::GeometricCurveStrength => state.gallery.geometric.curve_strength,
        GalleryNumberField::GeometricCurveFrequency => state.gallery.geometric.curve_frequency,
        GalleryNumberField::GeometricLens => state.gallery.geometric.lens,
        GalleryNumberField::GeometricOrbit => state.gallery.geometric.orbit,
        GalleryNumberField::GeometricOrbitRadius => state.gallery.geometric.orbit_radius,
        GalleryNumberField::GeometricTwist => state.gallery.geometric.twist,
        GalleryNumberField::GeometricScatter => state.gallery.geometric.scatter,
        GalleryNumberField::WallColumns => state.gallery.wall.columns as f32,
        GalleryNumberField::WallRows => state.gallery.wall.rows as f32,
        GalleryNumberField::WallCardWidth => state.gallery.wall.card_width,
        GalleryNumberField::WallCardHeight => state.gallery.wall.card_height,
        GalleryNumberField::WallGapX => state.gallery.wall.gap_x,
        GalleryNumberField::WallGapY => state.gallery.wall.gap_y,
        GalleryNumberField::WallStagger => state.gallery.wall.stagger,
        GalleryNumberField::WallCornerRadius => state.gallery.wall.corner_radius,
        GalleryNumberField::WallCylinderBend => state.gallery.wall.cylinder_bend,
        GalleryNumberField::WallCylinderRadius => state.gallery.wall.cylinder_radius,
    }
}

pub(super) fn set_gallery_number_value(
    state: &mut CandidateState,
    field: GalleryNumberField,
    value: f32,
) -> bool {
    if !value.is_finite()
        || (field != GalleryNumberField::DepthFlipDuration && state.gallery.mode != field.mode())
    {
        return false;
    }
    let value = match field {
        GalleryNumberField::DepthHeight => value.clamp(100.0, 1600.0),
        GalleryNumberField::DepthWidth => value.clamp(40.0, 2400.0),
        GalleryNumberField::DepthSpacing => value.clamp(0.0, 4000.0),
        GalleryNumberField::DepthCount => value.clamp(3.0, 21.0),
        GalleryNumberField::DepthFalloff => value.clamp(0.0, 1.0),
        GalleryNumberField::DepthCorners => value.clamp(0.0, 100.0),
        GalleryNumberField::DepthSkew => value.clamp(-200.0, 200.0),
        GalleryNumberField::DepthNavigation => value.clamp(35.0, 8000.0),
        GalleryNumberField::DepthFlipDuration => value.round().clamp(100.0, 5000.0),

        GalleryNumberField::SliceVisibleCount => value.round().clamp(1.0, 256.0),
        GalleryNumberField::SliceWidth => value.clamp(1.0, 4096.0),
        GalleryNumberField::SliceExpandedWidth => value.clamp(1.0, 4096.0),
        GalleryNumberField::SliceHeight => value.clamp(1.0, 4096.0),
        GalleryNumberField::SliceSpacing
        | GalleryNumberField::SliceSkewDistance
        | GalleryNumberField::SliceEdgeTilt => value.clamp(-4096.0, 4096.0),
        GalleryNumberField::SliceCornerRadius => value.clamp(0.0, 2048.0),
        GalleryNumberField::SliceWobbleStrength => value.clamp(0.0, 10.0),
        GalleryNumberField::GeometricRadius => value.clamp(1.0, 2048.0),
        GalleryNumberField::GeometricRows | GalleryNumberField::GeometricColumns => {
            value.round().clamp(1.0, 128.0)
        }
        GalleryNumberField::GeometricGapX | GalleryNumberField::GeometricGapY => {
            value.clamp(-2048.0, 2048.0)
        }
        GalleryNumberField::GeometricCurveStrength => value.clamp(-10.0, 10.0),
        GalleryNumberField::GeometricCurveFrequency => value.clamp(-20.0, 20.0),
        GalleryNumberField::GeometricLens => value.clamp(-0.85, 8.0),
        GalleryNumberField::GeometricOrbit => value.clamp(-1.0, 1.0),
        GalleryNumberField::GeometricOrbitRadius => value.clamp(0.0, 8192.0),
        GalleryNumberField::GeometricTwist => value.clamp(-720.0, 720.0),
        GalleryNumberField::GeometricScatter => value.clamp(0.0, 4096.0),
        GalleryNumberField::WallColumns | GalleryNumberField::WallRows => {
            value.round().clamp(1.0, 128.0)
        }
        GalleryNumberField::WallCardWidth | GalleryNumberField::WallCardHeight => {
            value.clamp(1.0, 4096.0)
        }
        GalleryNumberField::WallGapX | GalleryNumberField::WallGapY => value.clamp(-2048.0, 2048.0),
        GalleryNumberField::WallStagger => value.clamp(-4.0, 4.0),
        GalleryNumberField::WallCornerRadius => value.clamp(0.0, 2048.0),
        GalleryNumberField::WallCylinderBend => value.clamp(-1.0, 1.0),
        GalleryNumberField::WallCylinderRadius => value.clamp(1.0, 8192.0),
    };
    if (gallery_number_value(state, field) - value).abs() < f32::EPSILON {
        return false;
    }
    match field {
        GalleryNumberField::DepthHeight => state.gallery.depth.height = value,
        GalleryNumberField::DepthWidth => state.gallery.depth.width = value,
        GalleryNumberField::DepthSpacing => state.gallery.depth.spacing = value,
        GalleryNumberField::DepthCount => state.gallery.depth.visible_count = value,
        GalleryNumberField::DepthFalloff => state.gallery.depth.falloff = value,
        GalleryNumberField::DepthCorners => state.gallery.depth.corner_radius = value,
        GalleryNumberField::DepthSkew => state.gallery.depth.skew_distance = value,
        GalleryNumberField::DepthNavigation => state.gallery.depth.navigation_ms = value,
        GalleryNumberField::DepthFlipDuration => state.card_flip_mut().duration_ms = value as u64,

        GalleryNumberField::SliceVisibleCount => {
            state.gallery.slices.visible_count = value as usize
        }
        GalleryNumberField::SliceWidth => state.gallery.slices.width = value,
        GalleryNumberField::SliceExpandedWidth => state.gallery.slices.expanded_width = value,
        GalleryNumberField::SliceHeight => state.gallery.slices.height = value,
        GalleryNumberField::SliceSpacing => state.gallery.slices.spacing = value,
        GalleryNumberField::SliceSkewDistance => state.gallery.slices.skew_distance = value,
        GalleryNumberField::SliceEdgeTilt => state.gallery.slices.edge_tilt_distance = value,
        GalleryNumberField::SliceCornerRadius => state.gallery.slices.corner_radius = value,
        GalleryNumberField::SliceWobbleStrength => state.gallery.slices.wobble_strength = value,
        GalleryNumberField::GeometricRadius => state.gallery.geometric.radius = value,
        GalleryNumberField::GeometricRows => state.gallery.geometric.rows = value as usize,
        GalleryNumberField::GeometricColumns => state.gallery.geometric.columns = value as usize,
        GalleryNumberField::GeometricGapX => state.gallery.geometric.gap_x = value,
        GalleryNumberField::GeometricGapY => state.gallery.geometric.gap_y = value,
        GalleryNumberField::GeometricCurveStrength => {
            state.gallery.geometric.curve_strength = value
        }
        GalleryNumberField::GeometricCurveFrequency => {
            state.gallery.geometric.curve_frequency = value
        }
        GalleryNumberField::GeometricLens => state.gallery.geometric.lens = value,
        GalleryNumberField::GeometricOrbit => state.gallery.geometric.orbit = value,
        GalleryNumberField::GeometricOrbitRadius => state.gallery.geometric.orbit_radius = value,
        GalleryNumberField::GeometricTwist => state.gallery.geometric.twist = value,
        GalleryNumberField::GeometricScatter => state.gallery.geometric.scatter = value,
        GalleryNumberField::WallColumns => state.gallery.wall.columns = value as usize,
        GalleryNumberField::WallRows => state.gallery.wall.rows = value as usize,
        GalleryNumberField::WallCardWidth => state.gallery.wall.card_width = value,
        GalleryNumberField::WallCardHeight => state.gallery.wall.card_height = value,
        GalleryNumberField::WallGapX => state.gallery.wall.gap_x = value,
        GalleryNumberField::WallGapY => state.gallery.wall.gap_y = value,
        GalleryNumberField::WallStagger => state.gallery.wall.stagger = value,
        GalleryNumberField::WallCornerRadius => state.gallery.wall.corner_radius = value,
        GalleryNumberField::WallCylinderBend => state.gallery.wall.cylinder_bend = value,
        GalleryNumberField::WallCylinderRadius => state.gallery.wall.cylinder_radius = value,
    }
    state.gallery.variant = None;
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CardClickAction {
    Select,
    OpenDetail,
}

pub(super) fn card_click_action(selected: Option<usize>, clicked: usize) -> CardClickAction {
    match browse_intent(selected, clicked) {
        BrowseIntent::OpenDetail => CardClickAction::OpenDetail,
        BrowseIntent::Select => CardClickAction::Select,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PosterRequestIdentity {
    pub(super) index: usize,
    pub(super) item_id: String,
    pub(super) source: std::path::PathBuf,
    pub(super) is_video: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PosterSlot {
    pub(super) layer: u32,
    pub(super) is_video: bool,
    pub(super) generation: u64,
    pub(super) catalog_generation: u64,
    pub(super) item_id: String,
    pub(super) source: std::path::PathBuf,
}

/// A retained body owns the exact accepted poster object/layer and its
/// catalog/media/generation identity, never a catalog callback or bare layer.
#[derive(Debug, Clone)]
pub(super) struct DepthGhost {
    pub(super) body: gpu_gallery::DepthSnapshotCard,
    pub(super) phase: f32,
    pub(super) slot: PosterSlot,
    pub(super) index: usize,
    pub(super) poster: Poster,
}
#[derive(Debug, Clone)]
pub(super) struct DepthCompletedCell {
    catalog_generation: u64,
    pub(super) identity: PosterRequestIdentity,
}
#[derive(Debug, Clone)]
pub(super) struct DepthRoll {
    pub(super) wave: f32,
    pub(super) old: Vec<DepthGhost>,
    pub(super) incoming: HashMap<(i32, i32), f32>,
    // Completed same-cell reuse carries no texture/old geometry/callback.
    // Pruned to the <=24 current cells and cleared at the roll boundary.
    pub(super) completed: HashMap<(i32, i32), DepthCompletedCell>,
}
impl DepthRoll {
    pub(super) fn advance_to(&mut self, wave: f32) {
        let step = (wave - self.wave).max(0.0);
        self.wave = wave.max(self.wave).min(1.6);
        for old in &mut self.old {
            old.phase += step;
        }
        self.old.retain(|old| old.phase < 1.0);
        for phase in self.incoming.values_mut() {
            *phase += step;
        }
        self.incoming.retain(|_, phase| *phase < 1.0);
        if self.wave >= 1.6 {
            self.old.clear();
            self.incoming.clear();
            self.completed.clear();
        }
    }
    fn active(&self) -> bool {
        self.wave < 1.6 || !self.old.is_empty() || !self.incoming.is_empty()
    }
}

/// Stable allocation pins outgoing layers before accepting live/current demand.
/// The caller has already checked the exact upcoming-live/outgoing union and
/// bounded prefetch. Same identity reuses the exact resident object without decode.
pub(super) fn depth_poster_sources(
    requests: &[PosterRequestIdentity],
    slots: &HashMap<usize, PosterSlot>,
    ghosts: &[DepthGhost],
    catalog_generation: u64,
) -> Vec<poster_worker::PosterSource> {
    poster_sources_bounded(
        requests,
        slots,
        ghosts,
        catalog_generation,
        gpu_gallery::MAX_POSTERS,
    )
}

pub(super) fn poster_sources_bounded(
    requests: &[PosterRequestIdentity],
    slots: &HashMap<usize, PosterSlot>,
    ghosts: &[DepthGhost],
    catalog_generation: u64,
    capacity: usize,
) -> Vec<poster_worker::PosterSource> {
    let mut used: HashSet<u32> = ghosts.iter().map(|g| g.slot.layer).collect();
    let mut sources = Vec::new();
    for request in requests {
        if ghosts
            .iter()
            .any(|g| ghost_matches_request(g, request, catalog_generation))
        {
            continue;
        }
        let existing = slots
            .get(&request.index)
            .filter(|slot| {
                slot.catalog_generation == catalog_generation
                    && slot.item_id == request.item_id
                    && slot.source == request.source
                    && slot.is_video == request.is_video
                    && slot.layer < capacity as u32
                    && !used.contains(&slot.layer)
            })
            .map(|slot| slot.layer);
        let layer = existing.or_else(|| (0..capacity as u32).find(|layer| !used.contains(layer)));
        let Some(layer) = layer else {
            break;
        };
        used.insert(layer);
        sources.push(poster_worker::PosterSource {
            index: request.index,
            layer,
            item_id: request.item_id.clone(),
            path: request.source.clone(),
            is_video: request.is_video,
            still: None,
        });
    }
    sources
}
pub(super) fn ghost_matches_request(
    g: &DepthGhost,
    r: &PosterRequestIdentity,
    catalog_generation: u64,
) -> bool {
    g.slot.catalog_generation == catalog_generation
        && g.index == r.index
        && g.slot.item_id == r.item_id
        && g.slot.source == r.source
        && g.slot.is_video == r.is_video
}

/// Current cut frame also owns the upcoming live-window preload identities.
/// Reserve those first, exact outgoing second, then bounded adjacent prefetch.
pub(super) fn depth_poster_requests(
    frame: &gpu_gallery::DepthSnapshot,
    filtered: &[usize],
    wallpapers: &[WallpaperItem],
    ghosts: &[DepthGhost],
) -> Vec<PosterRequestIdentity> {
    let mut union: HashSet<usize> = ghosts.iter().map(|g| g.index).collect();
    frame
        .poster_indices(filtered)
        .into_iter()
        .filter_map(|index| {
            if !union.contains(&index) && union.len() == gpu_gallery::MAX_POSTERS {
                return None;
            }
            union.insert(index);
            let wallpaper = wallpapers.get(index)?;
            Some(PosterRequestIdentity {
                index,
                item_id: wallpaper.id.clone(),
                source: poster_source_for_item(wallpaper),
                is_video: wallpaper.source == WallpaperSource::LocalVideo,
            })
        })
        .collect()
}

pub(super) fn poster_source_for_item(wallpaper: &WallpaperItem) -> std::path::PathBuf {
    library::media_still::MediaStill::select(wallpaper, 0)
        .map(|still| still.path)
        .unwrap_or_else(|| {
            wallpaper
                .preview_path
                .clone()
                .unwrap_or_else(|| wallpaper.file_target.clone())
        })
}

pub(super) fn poster_slot_matches(slot: &PosterSlot, wallpaper: &WallpaperItem) -> bool {
    poster_slot_matches_source(slot, wallpaper, &poster_source_for_item(wallpaper))
}

pub(super) fn poster_slot_matches_source(
    slot: &PosterSlot,
    wallpaper: &WallpaperItem,
    source: &std::path::Path,
) -> bool {
    slot.item_id == wallpaper.id
        && slot.source.as_path() == source
        && slot.is_video == (wallpaper.source == WallpaperSource::LocalVideo)
}

pub(super) fn poster_completion_matches(
    decoded: &poster_worker::DecodedPoster,
    wallpaper: &WallpaperItem,
) -> bool {
    wallpaper.id == decoded.source.item_id
        && poster_source_for_item(wallpaper) == decoded.source.path
        && (wallpaper.source == WallpaperSource::LocalVideo) == decoded.source.is_video
        && decoded.poster.source == decoded.source.path
}

pub(super) fn poster_completion_is_current(
    generation: u64,
    current_generation: u64,
    visible: bool,
) -> bool {
    visible && generation == current_generation
}

pub(super) fn poster_mode_requires_invalidation(current: GalleryMode, next: GalleryMode) -> bool {
    current != next
}

pub(super) fn poster_request_changed(
    previous: Option<&[PosterRequestIdentity]>,
    next: &[PosterRequestIdentity],
) -> bool {
    let Some(previous) = previous else {
        return true;
    };
    let mut previous = previous.to_vec();
    let mut next = next.to_vec();
    previous.sort_by(|left, right| left.index.cmp(&right.index));
    next.sort_by(|left, right| left.index.cmp(&right.index));
    previous != next
}

pub(super) fn gallery_mode(kind: GalleryKind) -> GalleryMode {
    match kind {
        GalleryKind::Slices => GalleryMode::Slices,
        GalleryKind::Geometric => GalleryMode::Geometric,
        GalleryKind::Wall => GalleryMode::Wall,
        GalleryKind::Depth => GalleryMode::Depth,
    }
}

pub(super) fn gallery_kind(mode: GalleryMode) -> GalleryKind {
    match mode {
        GalleryMode::Slices => GalleryKind::Slices,
        GalleryMode::Geometric => GalleryKind::Geometric,
        GalleryMode::Wall => GalleryKind::Wall,
        GalleryMode::Depth => GalleryKind::Depth,
    }
}

/// The packaged rail exposes the three observed orientation labels. Keep this
/// state separate from gallery arrangements: WIDE/TALL are not card-geometry
/// names, and the exact packaged interaction still needs visual acceptance.
pub(super) fn shape_chip_label(orientation: OrientationFilter) -> &'static str {
    match orientation {
        OrientationFilter::Any => "SHAPE",
        OrientationFilter::Landscape => "WIDE",
        OrientationFilter::Portrait => "TALL",
        OrientationFilter::Square => "SQUARE",
    }
}

pub(super) fn next_shape_filter(orientation: OrientationFilter) -> OrientationFilter {
    match orientation {
        OrientationFilter::Any => OrientationFilter::Landscape,
        OrientationFilter::Landscape => OrientationFilter::Portrait,
        OrientationFilter::Portrait | OrientationFilter::Square => OrientationFilter::Any,
    }
}

pub(super) fn geometric_curve_choices() -> [(StateGeometricCurve, &'static str); 5] {
    [
        (StateGeometricCurve::Flat, "Plane"),
        (StateGeometricCurve::Arc, "Bow"),
        (StateGeometricCurve::Wave, "Ribbon"),
        (StateGeometricCurve::S, "S sweep"),
        (StateGeometricCurve::Cylinder, "Cylinder"),
    ]
}

pub(super) fn geometric_shape_choices() -> [(StateGeometricShape, &'static str); 4] {
    [
        (StateGeometricShape::Triangle, "Triangle"),
        (StateGeometricShape::Hexagon, "Hexagon"),
        (StateGeometricShape::Diamond, "Diamond"),
        (StateGeometricShape::Rhombus, "Rhombus"),
    ]
}

pub(super) fn wall_layout_choices() -> [(StateWallLayout, &'static str); 6] {
    [
        (StateWallLayout::Uniform, "Uniform"),
        (StateWallLayout::Brick, "Brick"),
        (StateWallLayout::Masonry, "Masonry"),
        (StateWallLayout::Justified, "Justified"),
        (StateWallLayout::Editorial, "Editorial"),
        (StateWallLayout::Cylinder, "Cylinder"),
    ]
}

pub(super) fn choose_geometric_curve(
    gallery: &mut candidate_state::GallerySettings,
    curve: StateGeometricCurve,
) -> bool {
    if gallery.geometric.curve == curve {
        return false;
    }
    gallery.geometric.curve = curve;
    gallery.variant = None;
    true
}

pub(super) fn choose_geometric_shape(
    gallery: &mut candidate_state::GallerySettings,
    shape: StateGeometricShape,
) -> bool {
    if gallery.geometric.shape == shape {
        return false;
    }
    gallery.geometric.shape = shape;
    gallery.variant = None;
    true
}

pub(super) fn configure_wall_variant_geometry(
    wall: &mut candidate_state::WallSettings,
    variant: usize,
    scale: f32,
) {
    if variant % 6 == 0 {
        wall.columns = 6;
        wall.rows = 3;
        wall.card_width = 300.0;
        wall.card_height = 169.0;
        wall.gap_x = 8.0;
        wall.gap_y = 8.0;
        wall.layout = StateWallLayout::Uniform;
    } else {
        wall.columns = 6;
        wall.rows = if variant % 6 == 4 { 2 } else { 3 };
        wall.card_width = 300.0 * scale;
        wall.card_height = 169.0 * scale;
        wall.gap_x = 16.0 * scale;
        wall.gap_y = 16.0 * scale;
        wall.layout = match variant % 6 {
            1 => StateWallLayout::Brick,
            2 => StateWallLayout::Masonry,
            3 => StateWallLayout::Justified,
            4 => StateWallLayout::Editorial,
            _ => StateWallLayout::Cylinder,
        };
    }
}

fn transition_gallery_mode(
    gallery: &mut candidate_state::GallerySettings,
    current_mode: GalleryKind,
    mode: GalleryKind,
) -> bool {
    let changed = current_mode != mode;
    gallery.mode = mode;
    if changed {
        gallery.variant = None;
    }
    changed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GalleryNavigationEffect {
    InvalidatePosterWork,
    ResetWallScroll,
    PersistState,
    SchedulePosters,
}

pub(super) fn navigate_gallery_mode(
    gallery: &mut candidate_state::GallerySettings,
    current_mode: GalleryKind,
    next_mode: GalleryKind,
    mut emit: impl FnMut(GalleryNavigationEffect),
) -> bool {
    let changed = transition_gallery_mode(gallery, current_mode, next_mode);
    if changed
        && poster_mode_requires_invalidation(gallery_mode(current_mode), gallery_mode(next_mode))
    {
        emit(GalleryNavigationEffect::InvalidatePosterWork);
        emit(GalleryNavigationEffect::ResetWallScroll);
    }
    emit(GalleryNavigationEffect::PersistState);
    emit(GalleryNavigationEffect::SchedulePosters);
    changed
}

pub(super) fn gallery_variant_for_viewport(
    gallery: &candidate_state::GallerySettings,
) -> Option<usize> {
    gallery.variant.map(usize::from)
}

pub(super) fn apply_gallery_viewport_policy(
    gallery: &mut candidate_state::GallerySettings,
    active_mode: GalleryKind,
    viewport: [f32; 2],
) {
    if let Some(variant) = gallery_variant_for_viewport(gallery) {
        apply_gallery_variant_geometry(gallery, active_mode, variant, viewport);
    }
}

fn apply_gallery_variant_geometry(
    gallery: &mut candidate_state::GallerySettings,
    mode: GalleryKind,
    variant: usize,
    viewport: [f32; 2],
) {
    let scale = (viewport[0] / 2560.0)
        .min(viewport[1] / 1440.0)
        .clamp(0.6, 1.5);
    match mode {
        GalleryKind::Slices => {
            let slices = &mut gallery.slices;
            match variant % 3 {
                1 => {
                    slices.height = (viewport[1] * 0.74).min(590.0);
                    slices.expanded_width = (viewport[0] * 0.36).min(590.0);
                    slices.width = (viewport[0] * 0.042).clamp(62.0, 94.0);
                    slices.skew_distance = 42.0;
                    slices.spacing = -14.0;
                    slices.visible_count = 14;
                }
                2 => {
                    slices.height = (viewport[1] * 0.62).min(480.0);
                    slices.expanded_width = (viewport[0] * 0.52).min(820.0);
                    slices.width = (viewport[0] * 0.065).clamp(86.0, 132.0);
                    slices.skew_distance = 30.0;
                    slices.spacing = 3.0;
                    slices.visible_count = 10;
                }
                _ => {
                    slices.height = 520.0;
                    slices.expanded_width = 924.0;
                    slices.width = 135.0;
                    slices.skew_distance = 35.0;
                    slices.spacing = -30.0;
                    slices.visible_count = 12;
                }
            }
        }
        GalleryKind::Geometric => {
            let geometric = &mut gallery.geometric;
            let variant = variant % 4;
            geometric.rows = if variant == 1 { 4 } else { 3 };
            geometric.columns = if variant == 2 {
                let radius = (140.0 * scale).clamp(70.0, 168.0);
                let step_x = radius * 1.5 + 6.0 * scale;
                (((viewport[0] - radius).max(1.0) / step_x).ceil() as usize + 1).max(3)
            } else if viewport[0] <= 1600.0 {
                5
            } else {
                7
            };
            geometric.radius = match variant {
                1 => (viewport[0] / 18.5).clamp(68.0, 104.0),
                2 => (140.0 * scale).clamp(70.0, 168.0),
                3 => (viewport[0] / 28.0).clamp(52.0, 74.0),
                _ if viewport[0] <= 1600.0 => 100.0,
                _ => 140.0,
            };
            geometric.gap_x = if variant == 2 { 6.0 * scale } else { 6.0 };
            geometric.gap_y = geometric.gap_x;
            geometric.aspect = if variant == 3 { 1.08 } else { 1.0 };
            geometric.shape = match variant {
                1 => StateGeometricShape::Triangle,
                2 => StateGeometricShape::Diamond,
                3 => StateGeometricShape::Rhombus,
                _ => StateGeometricShape::Hexagon,
            };
            geometric.curve = match variant {
                1 => StateGeometricCurve::Wave,
                3 => StateGeometricCurve::Cylinder,
                _ => StateGeometricCurve::Arc,
            };
            geometric.curve_strength = if variant == 3 { 0.65 } else { 1.2 };
            geometric.curve_frequency = 1.0;
            geometric.lens = 0.0;
            geometric.orbit = 0.0;
            geometric.orbit_radius = 0.0;
            geometric.twist = 0.0;
            geometric.scatter = 0.0;
        }
        GalleryKind::Depth => {
            gallery.variant = None;
        }
        GalleryKind::Wall => {
            let wall = &mut gallery.wall;
            configure_wall_variant_geometry(wall, variant, scale);
            wall.stagger = 0.5;
            wall.selected_scale = 1.08;
            wall.flow_wave = 0.0;
            wall.flow_frequency = 1.0;
            wall.scatter = 0.0;
            wall.scale_variance = 0.0;
            wall.cylinder_bend = 0.65;
            wall.cylinder_radius = 720.0 * scale;
        }
    }
}

pub(super) fn activate_gallery_variant(
    gallery: &mut candidate_state::GallerySettings,
    mode: GalleryKind,
    variant: usize,
    viewport: [f32; 2],
) {
    if mode == GalleryKind::Depth {
        gallery.variant = None;
        return;
    }
    gallery.variant = Some(variant as u8);
    apply_gallery_variant_geometry(gallery, mode, variant, viewport);
}

#[cfg(test)]
pub(super) fn apply_saved_gallery_preset(
    gallery: &mut candidate_state::GallerySettings,
    preset: &candidate_state::GalleryPreset,
) {
    gallery.mode = preset.mode;
    if preset.mode == GalleryKind::Depth {
        gallery.variant = None;
        if let Some(depth) = &preset.depth {
            gallery.depth = depth.effective_current();
        }
        return;
    }
    gallery.variant = preset.variant;
    gallery.slices = preset.slices.clone();
    gallery.geometric = preset.geometric.clone();
    gallery.wall = preset.wall.clone();
}

pub(super) fn next_gallery_variant_index(current: Option<u8>, variant_count: usize) -> usize {
    if variant_count == 0 {
        return 0;
    }
    current.map_or(0, |variant| (usize::from(variant) + 1) % variant_count)
}

pub(super) fn gallery_layout_params(
    state: &CandidateState,
    motion: f32,
    wall_scroll_offset: f32,
) -> GalleryLayoutParams {
    let slices = &state.gallery.slices;
    let geometric = &state.gallery.geometric;
    let wall = &state.gallery.wall;
    GalleryLayoutParams {
        depth: donor_layout::DepthParams {
            width: state.gallery.depth.width,
            height: state.gallery.depth.height,
            spacing: state.gallery.depth.spacing,
            // beta.24 startup.rs:254 casts the bounded canonical float only
            // at the integer rendering seam; never overwrite the saved value.
            visible_count: state.gallery.depth.visible_count as usize,
            falloff: state.gallery.depth.falloff,
            corner_radius: state.gallery.depth.corner_radius,
            skew: state.gallery.depth.skew_distance,
        },
        depth_camera: 0.0,
        slices: SliceParams {
            offset_x: slices.offset_x,
            offset_y: slices.offset_y,
            width: slices.width,
            expanded_width: slices.expanded_width,
            height: slices.height,
            spacing: slices.spacing,
            skew_distance: slices.skew_distance,
            edge_tilt_distance: slices.edge_tilt_distance,
            visible_count: slices.visible_count,
            corner_radius: slices.corner_radius,
            wobble: slices.wobble,
            wobble_strength: slices.wobble_strength,
            motion,
        },
        geometric: GeometricParams {
            radius: geometric.radius,
            rows: geometric.rows,
            columns: geometric.columns,
            shape: match geometric.shape {
                StateGeometricShape::Hexagon => GeometricShape::Hexagon,
                StateGeometricShape::Circle => GeometricShape::Circle,
                StateGeometricShape::Triangle => GeometricShape::Triangle,
                StateGeometricShape::Diamond => GeometricShape::Diamond,
                StateGeometricShape::Rhombus => GeometricShape::Rhombus,
            },
            curve: match geometric.curve {
                StateGeometricCurve::Flat => GeometricCurve::Flat,
                StateGeometricCurve::Arc => GeometricCurve::Arc,
                StateGeometricCurve::Wave => GeometricCurve::Wave,
                StateGeometricCurve::S => GeometricCurve::S,
                StateGeometricCurve::Cylinder => GeometricCurve::Cylinder,
            },
            curve_strength: geometric.curve_strength,
            curve_frequency: geometric.curve_frequency,
            gap_x: geometric.gap_x,
            gap_y: geometric.gap_y,
            aspect: geometric.aspect,
            stagger: geometric.stagger,
            lens: geometric.lens,
            orbit: geometric.orbit,
            orbit_radius: geometric.orbit_radius,
            twist: geometric.twist,
            scatter: geometric.scatter,
            motion,
            stage: donor_layout::StageParams::default(),
        },
        wall: WallParams {
            columns: wall.columns,
            rows: wall.rows,
            card_width: wall.card_width,
            card_height: wall.card_height,
            gap_x: wall.gap_x,
            gap_y: wall.gap_y,
            corner_radius: if wall.round_corners.unwrap_or(true) {
                wall.corner_radius
            } else {
                0.0
            },
            layout: match wall.layout {
                StateWallLayout::Uniform => WallLayout::Uniform,
                StateWallLayout::Brick => WallLayout::Brick,
                StateWallLayout::Masonry => WallLayout::Masonry,
                StateWallLayout::Justified => WallLayout::Justified,
                StateWallLayout::Editorial => WallLayout::Editorial,
                StateWallLayout::Cylinder => WallLayout::Cylinder,
            },
            stagger: wall.stagger,
            selected_scale: wall.selected_scale,
            flow_wave: wall.flow_wave,
            flow_frequency: wall.flow_frequency,
            scatter: wall.scatter,
            scale_variance: wall.scale_variance,
            cylinder_bend: wall.cylinder_bend,
            cylinder_radius: wall.cylinder_radius,
            rail_height: Some(24.0),
            rail_width: None,
            rail_rtl: false,
            rail_gap: 8.0,
            border_width: wall.border_width.unwrap_or(2.0),
            user_engaged: false,
            scroll_offset: wall_scroll_offset,
            motion,
            stage: donor_layout::StageParams::default(),
        },
    }
}

pub(super) fn trace_gallery_mouse_consumption(
    sequence: Option<u64>,
    message_kind: &str,
    outcome: &str,
    index: usize,
    selected_fields: String,
) {
    let Some(sequence) = sequence else {
        return;
    };
    apply_trace::event_lazy("gallery-mouse-consumption", || {
        format!(
            "sequence={sequence} receipt=iced_widget message_kind={message_kind} resulting_action={outcome} card_index={index} {selected_fields}"
        )
    });
}

pub(super) fn view_legacy_selection(app: &GpuCandidate) -> usize {
    app.selected
        .and_then(|selected| {
            app.filtered_indices.iter().position(|index| {
                #[cfg(test)]
                app.view_legacy_lookups.fetch_add(1, Ordering::Relaxed);
                *index == selected
            })
        })
        .unwrap_or(0)
}

pub(super) fn gallery_layout_params_for_app(app: &GpuCandidate) -> GalleryLayoutParams {
    let mut params = gallery_layout_params(&app.state, app.motion, app.wall_scroll_offset);
    if app.gallery_mode == GalleryMode::Wall {
        let mut model = gallery_rail_model(app);
        // Forced rail paint must not move the independent Wall gallery frame.
        model.mode = donor_rail::RailMode::Wall;
        let visible =
            app.config.ui.show_top_bar && !app.panel_state.settings_open && !app.depth_panel_open();
        let size = donor_rail::footprint(&model);
        params.wall.rail_height = (visible && !model.vertical).then_some(size[1]);
        params.wall.rail_width = (visible && model.vertical).then_some(size[0]);
        params.wall.rail_rtl = app.settings_locale.language.rtl;
        // Normal donor scene motion scale is1; only its demo commands change it.
        params.wall.rail_gap = 8.0;
        if let Some(p) = &app.wall_presentation {
            params.wall = p.frame_params(params.wall);
        }
        params.wall.motion = 0.0;
    }
    params
}

pub(super) fn prioritized_poster_indices(filtered: &[usize], selected: usize) -> Vec<usize> {
    if filtered.is_empty() {
        return Vec::new();
    }
    let selected_position = filtered
        .iter()
        .position(|index| *index == selected)
        .unwrap_or(0);
    let mut indices = Vec::with_capacity(gpu_gallery::MAX_POSTERS.min(filtered.len()));
    indices.push(filtered[selected_position]);
    for distance in 1..filtered.len() {
        if let Some(left) = selected_position.checked_sub(distance) {
            indices.push(filtered[left]);
            if indices.len() == gpu_gallery::MAX_POSTERS {
                break;
            }
        }
        let right = selected_position + distance;
        if right < filtered.len() {
            indices.push(filtered[right]);
            if indices.len() == gpu_gallery::MAX_POSTERS {
                break;
            }
        }
    }
    indices
}

pub(super) fn filtered_poster_aspects(
    catalog: &[library::CatalogEntry],
    filtered_indices: &[usize],
) -> Vec<f32> {
    filtered_indices
        .iter()
        .map(|index| {
            catalog
                .get(*index)
                .filter(|entry| entry.width > 0 && entry.height > 0)
                .map(|entry| entry.width as f32 / entry.height as f32)
                .unwrap_or(16.0 / 9.0)
        })
        .collect()
}

pub(super) fn push_unique_poster_index(indices: &mut Vec<usize>, index: usize) {
    if indices.len() < gpu_gallery::MAX_POSTERS && !indices.contains(&index) {
        indices.push(index);
    }
}

/// Choose Wall requests from the same visible cards that the renderer draws.
///
/// If all visible cards fit, visibility is the complete first priority and
/// selection/adjacent prefetch use only the remaining slots. If visibility
/// exceeds the fixed texture budget, the selected item is retained first and
/// the remaining slots follow the renderer's deterministic card order.
pub(super) fn wall_visible_poster_indices(
    filtered_indices: &[usize],
    selected_position: usize,
    viewport: [f32; 2],
    layout_params: GalleryLayoutParams,
    poster_aspects: &[f32],
) -> Vec<usize> {
    if filtered_indices.is_empty() {
        return Vec::new();
    }
    let selected_position = selected_position.min(filtered_indices.len() - 1);
    let visible_cards = gpu_gallery::layout_gallery_with_aspects(
        GalleryMode::Wall,
        filtered_indices.len(),
        selected_position,
        viewport,
        layout_params,
        poster_aspects,
    );
    let visible_indices: Vec<_> = visible_cards
        .iter()
        .filter_map(|card| filtered_indices.get(card.index).copied())
        .collect();
    let selected_index = filtered_indices.get(selected_position).copied();
    let limit = gpu_gallery::ordinary_poster_limit();
    let mut indices = Vec::with_capacity(limit.min(filtered_indices.len()));
    let mut push = |index| {
        if indices.len() < limit && !indices.contains(&index) {
            indices.push(index);
        }
    };
    if visible_indices.len() > limit {
        if let Some(index) = selected_index {
            push(index);
        }
    }
    for index in visible_indices {
        push(index);
    }
    if let Some(index) = selected_index {
        push(index);
        for neighbor in prioritized_poster_indices(filtered_indices, index) {
            push(neighbor);
        }
    }
    indices
}

pub(super) fn poster_indices_for_view(
    mode: GalleryMode,
    filtered_indices: &[usize],
    selected: Option<usize>,
    viewport: [f32; 2],
    layout_params: GalleryLayoutParams,
    poster_aspects: &[f32],
) -> Vec<usize> {
    if filtered_indices.is_empty() {
        return Vec::new();
    }
    let selected_position = selected
        .and_then(|index| {
            filtered_indices
                .iter()
                .position(|candidate| *candidate == index)
        })
        .unwrap_or(0);
    match mode {
        GalleryMode::Wall => wall_visible_poster_indices(
            filtered_indices,
            selected_position,
            viewport,
            layout_params,
            poster_aspects,
        ),
        GalleryMode::Depth => gpu_gallery::layout_gallery_with_aspects(
            mode,
            filtered_indices.len(),
            selected_position,
            viewport,
            layout_params,
            poster_aspects,
        )
        .iter()
        .rev()
        .filter_map(|card| filtered_indices.get(card.index).copied())
        .take(gpu_gallery::MAX_POSTERS)
        .collect(),
        GalleryMode::Slices | GalleryMode::Geometric => prioritized_poster_indices(
            filtered_indices,
            selected.unwrap_or(filtered_indices[selected_position]),
        ),
    }
}

pub(super) fn depth_wheel_steps(remainder: &mut f32, delta: f32) -> isize {
    if !delta.is_finite() {
        return 0;
    }
    if delta.abs() >= 1.0 {
        *remainder = 0.0;
        return delta.round() as isize;
    }
    *remainder += delta;
    let steps = remainder.trunc();
    *remainder -= steps;
    steps as isize
}

#[derive(Debug, PartialEq)]
pub(super) enum DepthPointerIntent {
    Ignore,
    Hover(Option<String>),
    Select(usize),
    Detail(bool),
    Backdrop,
}
pub(super) fn consume_depth_pointer(
    frame: Option<&gpu_gallery::DepthSnapshot>,
    wallpapers: &[WallpaperItem],
    selected: Option<usize>,
    detail_open: bool,
    visible: bool,
    captured: bool,
    message: &GalleryMessage,
) -> DepthPointerIntent {
    if !visible || captured {
        return DepthPointerIntent::Ignore;
    }
    let (
        Some(frame),
        GalleryMessage::DepthPointer {
            revision,
            item_id,
            right,
            hover,
        },
    ) = (frame, message)
    else {
        return DepthPointerIntent::Ignore;
    };
    if *revision != frame.revision {
        return DepthPointerIntent::Ignore;
    }
    let index = item_id
        .as_ref()
        .and_then(|id| frame.cards.iter().find(|c| c.id == *id))
        .and_then(|c| {
            wallpapers
                .get(c.card.index)
                .filter(|w| w.id == c.id)
                .map(|_| c.card.index)
        });
    if *hover {
        return DepthPointerIntent::Hover(
            index.and_then(|i| wallpapers.get(i).map(|w| w.id.clone())),
        );
    }
    match index {
        Some(i) if selected != Some(i) => DepthPointerIntent::Select(i),
        Some(_) => DepthPointerIntent::Detail(if *right { !detail_open } else { true }),
        None if item_id.is_none() && !*right => DepthPointerIntent::Backdrop,
        _ => DepthPointerIntent::Ignore,
    }
}

impl GpuCandidate {
    pub(super) fn cycle_shape_filter(&mut self) {
        self.filter_rule.orientation = next_shape_filter(self.filter_rule.orientation);
        self.refresh_filter();
        self.schedule_posters();
    }

    pub(super) fn cycle_gallery_variant(&mut self) {
        let count = self.gallery_mode.variant_labels().len();
        if count == 0 {
            return;
        }
        let next = next_gallery_variant_index(self.state.gallery.variant, count);
        self.set_gallery_variant(next);
    }

    fn set_gallery_variant(&mut self, variant: usize) {
        let count = self.gallery_mode.variant_labels().len();
        if count == 0 {
            return;
        }
        let variant = variant % count;
        self.wall_scroll_offset = 0.0;
        activate_gallery_variant(
            &mut self.state.gallery,
            gallery_kind(self.gallery_mode),
            variant,
            self.viewport,
        );
        self.persist_state();
        self.schedule_posters();
    }

    pub(super) fn cycle_gallery_geometry(&mut self) {
        match self.gallery_mode {
            GalleryMode::Slices => {
                let slices = &mut self.state.gallery.slices;
                (
                    slices.width,
                    slices.expanded_width,
                    slices.spacing,
                    slices.skew_distance,
                ) = if slices.skew_distance < 20.0 {
                    (230.0, 390.0, -56.0, 32.0)
                } else if slices.skew_distance < 40.0 {
                    (250.0, 420.0, -72.0, 48.0)
                } else if slices.skew_distance < 60.0 {
                    (270.0, 450.0, -88.0, 72.0)
                } else {
                    (220.0, 380.0, -42.0, 0.0)
                };
            }
            GalleryMode::Geometric => {
                let geometric = &mut self.state.gallery.geometric;
                geometric.curve = match geometric.curve {
                    StateGeometricCurve::Flat => StateGeometricCurve::Arc,
                    StateGeometricCurve::Arc => StateGeometricCurve::Wave,
                    StateGeometricCurve::Wave => StateGeometricCurve::S,
                    StateGeometricCurve::S => StateGeometricCurve::Cylinder,
                    StateGeometricCurve::Cylinder => StateGeometricCurve::Flat,
                };
            }
            GalleryMode::Depth => return,
            GalleryMode::Wall => {
                let wall = &mut self.state.gallery.wall;
                (wall.columns, wall.rows, wall.card_width, wall.card_height) = match wall.columns {
                    0..=4 => (6, 3, 300.0, 169.0),
                    5..=6 => (8, 4, 250.0, 141.0),
                    _ => (4, 2, 360.0, 203.0),
                };
            }
        }
        self.state.gallery.variant = None;
        self.persist_state();
        self.schedule_posters();
    }

    pub(super) fn gallery_settings_changed(&mut self, previous: &candidate_state::GallerySettings) {
        let changed = match self.gallery_mode {
            GalleryMode::Slices => previous.slices != self.state.gallery.slices,
            GalleryMode::Geometric => previous.geometric != self.state.gallery.geometric,
            GalleryMode::Wall => previous.wall != self.state.gallery.wall,
            GalleryMode::Depth => previous.depth != self.state.gallery.depth,
        };
        if !changed {
            return;
        }
        if self.gallery_mode == GalleryMode::Depth {
            self.depth_settings_changed(&previous.depth);
        }
        self.wall_scroll_offset = 0.0;
        self.schedule_posters();
    }
    pub(super) fn publish_gallery_settings(
        &mut self,
        gallery: candidate_state::GallerySettings,
    ) -> bool {
        if let Err(error) = view::settings::controller::finish_geometry_owner_edit(self) {
            self.settings_workbench.message = error;
            return false;
        }
        if gallery == self.state.gallery {
            return false;
        }
        let previous = self.state.gallery.clone();
        let mut proposed = self.state.clone();
        proposed.gallery = gallery;
        if self.commit_local_state_change(proposed).is_err() {
            return false;
        }
        self.gallery_settings_changed(&previous);
        true
    }

    pub(super) fn set_gallery_number(&mut self, field: GalleryNumberField, value: f32) {
        if field == GalleryNumberField::DepthFlipDuration {
            if value.is_finite() {
                self.edit_card_flip(|flip| {
                    flip.duration_ms = value.round().clamp(100.0, 5000.0) as u64
                });
            }
            return;
        }
        let mut proposed = self.state.clone();
        if set_gallery_number_value(&mut proposed, field, value) {
            self.publish_gallery_settings(proposed.gallery);
        }
    }

    pub(super) fn edit_card_flip(
        &mut self,
        edit: impl FnOnce(&mut candidate_state::DepthDetailSettings),
    ) {
        let before = self.state.card_flip().clone();
        let mut proposed = self.state.clone();
        edit(proposed.card_flip_mut());
        if proposed.card_flip() == &before || self.commit_local_state_change(proposed).is_err() {
            return;
        }
        self.card_flip_changed(&before);
    }
    pub(super) fn card_flip_changed(&mut self, before: &candidate_state::DepthDetailSettings) {
        if let Some(mut spring) = self.depth_detail.filter(|_| self.detail_open) {
            if before.effect != self.state.card_flip().effect {
                let start = if !self.state.card_flip().animate_shader
                    && !self.state.card_flip().animate_back
                {
                    1.0
                } else {
                    0.0
                };
                spring = GalleryMotionState::source(
                    start,
                    1.0,
                    self.state.card_flip().duration_ms as f32,
                    1.0,
                );
                if matches!(self.gallery_mode, GalleryMode::Slices | GalleryMode::Depth) {
                    self.detail_progress = start;
                }
            }
            if before.duration_ms != self.state.card_flip().duration_ms {
                let target = spring.target;
                spring = GalleryMotionState::source(
                    spring.value(),
                    target,
                    self.state.card_flip().duration_ms as f32,
                    1.0,
                );
            }
            self.resume_depth_detail(spring, "shared-flip-edit");
        }
        self.schedule_posters();
    }

    pub(super) fn edit_depth(&mut self, edit: impl FnOnce(&mut candidate_state::DepthSettings)) {
        if self.gallery_mode != GalleryMode::Depth {
            return;
        }
        let mut gallery = self.state.gallery.clone();
        edit(&mut gallery.depth);
        if gallery.depth == self.state.gallery.depth {
            return;
        }
        gallery.variant = None;
        self.publish_gallery_settings(gallery);
    }
    pub(super) fn depth_settings_changed(&mut self, previous: &candidate_state::DepthSettings) {
        // Geometry/style/detail edits invalidate retained visual geometry. Keep
        // current live springs/morph/detail, cancel only the bounded cut history.
        self.depth_roll = None;
        if previous.navigation_ms != self.state.gallery.depth.navigation_ms {
            if let Some(camera) = &mut self.depth_camera {
                camera.retime_absolute(self.state.gallery.depth.navigation_ms);
            }
            // Donor shared retime quirk: existing selection springs use Standard.
            for spring in self.depth_selection.values_mut() {
                spring.retime_absolute(250.0);
            }
        }
        if let Some(spring) = self.depth_detail {
            self.resume_depth_detail(spring, "depth-options-edit");
        }
        let hover = self.depth_hover.clone();
        self.start_motion(0.0);
        self.depth_hover = hover;
    }
    pub(super) fn set_geometric_curve(&mut self, curve: StateGeometricCurve) {
        let mut gallery = self.state.gallery.clone();
        if choose_geometric_curve(&mut gallery, curve) {
            self.publish_gallery_settings(gallery);
        }
    }

    pub(super) fn set_geometric_shape(&mut self, shape: StateGeometricShape) {
        let mut gallery = self.state.gallery.clone();
        if choose_geometric_shape(&mut gallery, shape) {
            self.publish_gallery_settings(gallery);
            if self.state.gallery.geometric.shape == shape {
                self.reveal_gallery_layout();
            }
        }
    }

    pub(super) fn cycle_gallery_motion(&mut self) {
        match self.gallery_mode {
            GalleryMode::Slices => {
                let slices = &mut self.state.gallery.slices;
                if slices.wobble {
                    if slices.wobble_strength < 1.5 {
                        slices.wobble_strength = 2.0;
                    } else {
                        slices.wobble = false;
                        slices.wobble_strength = 0.0;
                    }
                } else {
                    slices.wobble = true;
                    slices.wobble_strength = 1.0;
                }
            }
            GalleryMode::Geometric => {
                let geometric = &mut self.state.gallery.geometric;
                (
                    geometric.lens,
                    geometric.orbit,
                    geometric.twist,
                    geometric.scatter,
                ) = if geometric.lens == 0.0 && geometric.orbit == 0.0 {
                    (0.45, 0.0, 0.0, 0.0)
                } else if geometric.orbit == 0.0 {
                    (0.0, 0.35, 0.18, 0.0)
                } else if geometric.scatter == 0.0 {
                    (0.0, 0.0, 0.0, 0.3)
                } else {
                    (0.0, 0.0, 0.0, 0.0)
                };
            }
            GalleryMode::Depth => return,
            GalleryMode::Wall => {
                let wall = &mut self.state.gallery.wall;
                (wall.flow_wave, wall.scatter, wall.scale_variance) =
                    if wall.flow_wave == 0.0 && wall.scatter == 0.0 {
                        (0.35, 0.0, 0.08)
                    } else if wall.scatter == 0.0 {
                        (0.0, 0.25, 0.12)
                    } else {
                        (0.0, 0.0, 0.0)
                    };
            }
        }
        self.persist_state();
        self.schedule_posters();
    }

    pub(super) fn save_gallery_preset(&mut self) -> Result<(), String> {
        if self.state.gallery.mode != gallery_kind(self.gallery_mode) {
            return Err("The current mode changed before profile Save.".into());
        }
        let snapshot = view::settings::profile_projection::capture(self);
        let mut proposed = self.state.clone();
        proposed.mode_profiles.save(snapshot, |number| {
            let mut args = fluent::FluentArgs::new();
            args.set("number", number as i64);
            self.settings_locale
                .format("settings-selector-preset-generated-name", Some(&args))
                .expect("verified preset generator")
        })?;
        self.commit_local_state_change(proposed).map_err(|error| {
            self.save_failure
                .clone()
                .unwrap_or_else(|| error.to_string())
        })?;
        Ok(())
    }

    pub(super) fn apply_gallery_preset(
        &mut self,
        mode: GalleryKind,
        id: u64,
    ) -> Result<(), String> {
        if mode != self.state.gallery.mode || mode != gallery_kind(self.gallery_mode) {
            return Err("Choose a profile from the current mode.".into());
        }
        let snapshot = self
            .state
            .mode_profiles
            .snapshot(mode, id)
            .ok_or("This legacy profile has no available geometry snapshot.")?;
        view::settings::profile_projection::install(self, &snapshot, Some(id))
    }

    pub(super) fn delete_gallery_preset(
        &mut self,
        mode: GalleryKind,
        id: u64,
    ) -> Result<bool, String> {
        if mode != self.state.gallery.mode || mode != gallery_kind(self.gallery_mode) {
            return Ok(false);
        }
        let mut proposed = self.state.clone();
        if !proposed.mode_profiles.delete(mode, id) {
            return Ok(false);
        }
        self.commit_local_state_change(proposed).map_err(|error| {
            self.save_failure
                .clone()
                .unwrap_or_else(|| error.to_string())
        })?;
        Ok(true)
    }

    pub(super) fn cancel_detail(&mut self, reason: &str) {
        if self.detail_open || self.detail_progress > 0.0 {
            apply_trace::event_lazy("ui-detail-transition", || {
                format!(
                    "action=cancelled reason={reason} detail_open={} detail_progress={} detail_generation={} {}",
                    self.detail_open,
                    self.detail_progress,
                    self.detail_generation.load(Ordering::Acquire),
                    self.selected_item_trace_fields()
                )
            });
        }
        self.detail_generation.fetch_add(1, Ordering::AcqRel);
        self.detail_progress = 0.0;
        self.detail_source = None;
        self.depth_detail = None;
        self.depth_detail_id = None;
        self.depth_detail_mode = None;
        self.detail_open = false;
        self.clear_engine_properties();
        self.playlist_panel_open = false;
    }

    pub(super) fn start_detail(&mut self, opening: bool, reason: &str) {
        self.colours.selection_anchor = None;
        self.animate_detail(opening, reason, true);
    }
    pub(super) fn depth_panel_open(&self) -> bool {
        self.gallery_mode == GalleryMode::Depth
            && self.detail_open
            && self.depth_detail_mode == Some(donor_detail::DepthDetailMode::Panel)
    }

    pub(super) fn resolve_depth_detail(&mut self) {
        if self.gallery_mode != GalleryMode::Depth || !self.detail_open {
            if !self.detail_open {
                self.depth_detail_mode = None;
                self.depth_detail_id = None;
            }
            return;
        }
        // Diagnostic requested detail also enters through this shared owner.
        if self.depth_detail_id.is_none() {
            self.depth_detail_id = self
                .selected
                .and_then(|i| self.wallpapers.get(i))
                .map(|w| w.id.clone());
        }
        let readable =
            self.depth_frame
                .as_ref()
                .and_then(|f| {
                    let entry = f.cards.iter().find(|c| {
                        c.card.selected && Some(&c.id) == self.depth_detail_id.as_ref()
                    })?;
                    let wallpaper = self.wallpapers.get(entry.card.index)?;
                    Some(donor_back_layout::depth_inline_eligible(
                        donor_detail::CardGeometry {
                            center: entry.card.center,
                            size: entry.card.size,
                            rotation: entry.card.rotation,
                            skew_distance: entry.card.skew_distance,
                            edge_tilt_distance: entry.card.edge_tilt_distance,
                        },
                        entry.card.corner_radius,
                        donor_back_layout::DETAIL_FACT_COUNT,
                        &detail_actions(self, wallpaper),
                        view::settings::bridge::effective_ui_scale(self),
                        UI_FONT,
                        &wallpaper.title,
                    ))
                })
                .unwrap_or(false);
        let previous = self.depth_detail_mode;
        let mode = previous
            .unwrap_or(donor_detail::DepthDetailMode::Inline)
            .resolve(readable);
        self.depth_detail_mode = Some(mode);
        if previous.is_some() && previous != Some(mode) {
            // Invalidate callbacks from the formerly inline shape, preserving
            // spring progress/target and the selected identity.
            self.depth_revision = self.depth_revision.wrapping_add(1);
            if let Some(frame) = &mut self.depth_frame {
                frame.revision = self.depth_revision;
            }
        }
    }
    fn resume_depth_detail(&mut self, spring: GalleryMotionState, reason: &str) {
        if !self.detail_open {
            return;
        }
        self.depth_detail = Some(spring);
        self.animate_detail(spring.target > 0.5, reason, false);
    }
    fn animate_detail(&mut self, opening: bool, reason: &str, request: bool) {
        {
            let identity = self
                .selected
                .and_then(|i| self.wallpapers.get(i))
                .map(|w| w.id.clone());
            if identity.is_none() {
                self.cancel_detail("depth-no-identity");
                return;
            }
            if self.depth_detail_id.is_some() && self.depth_detail_id != identity {
                self.cancel_detail("depth-identity-replaced");
            }
            self.depth_detail_id = identity;
        }
        let generation = self.detail_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let start = self.detail_progress;
        let target = if opening { 1.0 } else { 0.0 };
        let was_open = self.detail_open;
        apply_trace::event_lazy("ui-detail-transition", || {
            format!(
                "action=requested requested_open={opening} previous_open={was_open} reason={reason} generation={generation} start_progress={start} target_progress={target} {}",
                self.selected_item_trace_fields()
            )
        });
        if opening && self.detail_source.is_none() && self.gallery_mode != GalleryMode::Depth {
            self.detail_source = self
                .ordinary_frame()
                .iter()
                .find(|e| e.card.selected)
                .map(|e| donor_detail::CardGeometry {
                    center: e.card.center,
                    size: e.card.size,
                    rotation: e.card.rotation,
                    skew_distance: e.card.skew_distance,
                    edge_tilt_distance: e.card.edge_tilt_distance,
                });
        }
        self.detail_open = true;
        self.resolve_depth_detail();
        if !opening {
            self.engine_properties_open = false;
        }
        let external = matches!(
            self.gallery_mode,
            GalleryMode::Geometric | GalleryMode::Wall
        );
        let standard_ms = (view::settings::bridge::Source { app: self }).motion_standard_ms();
        let mut spring = self.depth_detail.unwrap_or_else(|| {
            GalleryMotionState::source(
                start,
                target,
                self.state.card_flip().duration_ms as f32,
                1.0,
            )
        });
        // Existing retained legacy springs may use the Absolute policy; both retargets preserve velocity.
        if request {
            spring.target = target;
        }
        if request
            && !external
            && !self.state.card_flip().animate_shader
            && !self.state.card_flip().animate_back
        {
            spring = GalleryMotionState::source(
                target,
                target,
                self.state.card_flip().duration_ms as f32,
                1.0,
            );
            self.depth_detail = Some(spring);
            self.detail_progress = target;
            self.detail_open = opening;
            if !opening {
                self.detail_source = None;
            }
            self.resolve_depth_detail();
            return;
        }
        self.depth_detail = Some(spring);
        let mut current_progress = start;
        let frame = std::time::Duration::from_secs_f32(
            1.0 / (view::settings::bridge::Source { app: self })
                .number("general.maxFps")
                .clamp(1.0, 1000.0) as f32,
        );
        #[cfg(test)]
        if self.deterministic_animation {
            return;
        }
        let generation_clock = self.detail_generation.clone();
        let detail_tx = WAKE_TX.get().and_then(|slot| slot.lock().ok()?.clone());
        let Some(detail_tx) = detail_tx else {
            if !request {
                return;
            }
            self.detail_progress = target;
            self.detail_open = opening;
            if !opening {
                self.detail_source = None;
            }
            self.resolve_depth_detail();
            apply_trace::event_lazy("ui-detail-transition", || {
                format!(
                    "action=completed_without_animation requested_open={opening} reason=message_channel_unavailable generation={generation} {}",
                    self.selected_item_trace_fields()
                )
            });
            return;
        };
        if let Err(error) = std::thread::Builder::new()
            .name(format!("gpu-detail-motion-{generation}"))
            .spawn(move || {
                let frame = frame;
                let started = std::time::Instant::now();
                let mut frame_index = 0u32;
                loop {
                    if generation != generation_clock.load(Ordering::Acquire) {
                        return;
                    }
                    let (value, done) = detail_motion::advance_detail(
                        &mut spring,
                        current_progress,
                        external,
                        standard_ms,
                        frame.as_secs_f32(),
                    );
                    current_progress = value;
                    if detail_tx
                        .unbounded_send(Wake::DetailFrame {
                            generation,
                            value,
                            close_when_done: !opening && done,
                            spring: Some(spring),
                        })
                        .is_err()
                    {
                        return;
                    }
                    if done {
                        return;
                    }
                    frame_index = frame_index.saturating_add(1);
                    let deadline = started + frame * frame_index;
                    std::thread::sleep(
                        deadline.saturating_duration_since(std::time::Instant::now()),
                    );
                }
            })
        {
            log_debug(&format!("Could not start detail animation: {error}"));
            self.detail_progress = target;
            self.detail_open = opening;
            if !opening {
                self.detail_source = None;
            }
            self.resolve_depth_detail();
            apply_trace::event_lazy("ui-detail-transition", || {
                format!(
                    "action=completed_without_animation requested_open={opening} reason=thread_spawn_failed generation={generation} error={error:?} {}",
                    self.selected_item_trace_fields()
                )
            });
        }
    }

    pub(super) fn start_motion(&mut self, direction: f32) {
        let generation = self.motion_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.start_gallery_motion(generation, direction, self.motion_velocity);
    }

    fn start_gallery_motion(&mut self, generation: u64, direction: f32, velocity: f32) {
        self.sync_gallery_presentation();
        let mut wall = (self.gallery_mode == GalleryMode::Wall)
            .then(|| self.wall_presentation.clone())
            .flatten();
        let mut hex = (self.gallery_mode == GalleryMode::Geometric)
            .then(|| self.hex_presentation.clone())
            .flatten();
        let mut ordinary = (self.gallery_mode == GalleryMode::Slices)
            .then(|| self.gallery_presentation.clone())
            .flatten();
        self.scene_motion.generation = generation;
        let mut scene = self.scene_motion;
        let duration_ms = self.state.gallery_motion_speed.duration_ms();
        let initial_motion = self.motion;
        let mut initial_state = if self.gallery_mode == GalleryMode::Depth {
            let target = self
                .selected
                .and_then(|i| self.filtered_indices.iter().position(|j| *j == i))
                .unwrap_or(0) as f32;
            let d = &self.state.gallery.depth;
            let mut state = self.depth_camera.unwrap_or_else(|| {
                GalleryMotionState::absolute_camera(
                    target,
                    target,
                    d.navigation_ms,
                    d.spacing,
                    d.width,
                )
            });
            state.retarget_absolute(target);
            self.depth_camera = Some(state);
            state
        } else {
            GalleryMotionState::with_velocity(initial_motion, velocity, direction, duration_ms)
        };
        self.motion = if self.gallery_mode == GalleryMode::Depth {
            0.0
        } else {
            initial_state.value()
        };
        self.motion_velocity = initial_state.velocity;
        let mut selection = if self.gallery_mode == GalleryMode::Depth {
            let selected_id = self
                .selected
                .and_then(|i| self.wallpapers.get(i))
                .map(|w| w.id.clone());
            let mut values = self.depth_selection.clone();
            values.retain(|id, _| {
                selected_id.as_ref() == Some(id)
                    || self
                        .depth_frame
                        .as_ref()
                        .is_some_and(|f| f.cards.iter().any(|c| c.id == *id))
            });
            for (id, spring) in &mut values {
                let target = f32::from(selected_id.as_ref() == Some(id));
                if spring.target != target {
                    *spring = GalleryMotionState::absolute_camera(
                        spring.value(),
                        target,
                        self.state.gallery.depth.navigation_ms,
                        50.0,
                        50.0,
                    );
                }
            }
            if let Some(id) = selected_id {
                values.entry(id).or_insert_with(|| {
                    GalleryMotionState::absolute_camera(
                        0.0,
                        1.0,
                        self.state.gallery.depth.navigation_ms,
                        50.0,
                        50.0,
                    )
                });
            }
            self.depth_selection = values.clone();
            self.depth_hover = None;
            values
        } else {
            HashMap::new()
        };
        let mut presentation = (self.gallery_mode == GalleryMode::Depth).then(|| {
            self.depth_presentation
                .unwrap_or_else(|| DepthPresentation::target(&self.state))
        });
        let presentation_target = DepthPresentation::target(&self.state);
        let filter_duration = self.filter_swap_ms();
        let mut filter_wave = self
            .depth_roll
            .as_ref()
            .filter(|r| r.active())
            .map(|r| r.wave)
            .or_else(|| self.ordinary_roll.as_ref().map(|r| r.wave));
        // Test event sink owns clock advancement; production preparation and
        // the spring/morph kernels are identical, with no timer races.
        #[cfg(test)]
        if self.deterministic_animation {
            return;
        }
        let frame_interval = std::time::Duration::from_secs_f32(
            1.0 / (view::settings::bridge::Source { app: self })
                .number("general.maxFps")
                .clamp(1.0, 1000.0) as f32,
        );
        let generation_clock = self.motion_generation.clone();
        let motion_tx = WAKE_TX.get().and_then(|slot| slot.lock().ok()?.clone());
        let Some(motion_tx) = motion_tx else {
            self.settle_gallery_motion();
            return;
        };
        if let Err(error) = std::thread::Builder::new()
            .name(format!("gpu-gallery-camera-{generation}"))
            .spawn(move || {
                let frame = frame_interval;
                let mut previous_tick = std::time::Instant::now();
                let mut next_tick = previous_tick + frame;
                loop {
                    if generation != generation_clock.load(Ordering::Acquire) {
                        return;
                    }
                    let dt = motion_tick_elapsed(&mut previous_tick, std::time::Instant::now());
                    if let Some(wave) = &mut filter_wave {
                        *wave =
                            (*wave + gpu_gallery::depth_roll_step(dt, filter_duration)).min(1.6);
                    }
                    let mut moving = advance_gallery_motion(
                        &mut initial_state,
                        &mut selection,
                        &mut presentation,
                        presentation_target,
                        dt,
                    );
                    moving |= scene.tick(dt);
                    if let Some(p) = &mut ordinary {
                        moving |= p.tick(dt);
                    }
                    if let Some(p) = &mut wall {
                        moving |= p.tick(dt);
                    }
                    if let Some(p) = &mut hex {
                        p.inset = scene.inset(p.viewport[0]);
                        moving |= p.tick(dt);
                    }
                    if motion_tx
                        .unbounded_send(Wake::MotionFrame {
                            generation,
                            value: initial_state.value(),
                            velocity: initial_state.velocity,
                            state: initial_state,
                            selection: selection.clone(),
                            presentation,
                            filter_wave,
                            ordinary: ordinary.clone(),
                            hex: hex.clone(),
                            wall: wall.clone(),
                            scene: Some(scene),
                        })
                        .is_err()
                    {
                        return;
                    }
                    if !moving && filter_wave.is_none_or(|wave| wave >= 1.6) {
                        return;
                    }
                    let now = std::time::Instant::now();
                    let deadline = motion_tick_deadline(next_tick, frame, now);
                    std::thread::sleep(deadline.saturating_duration_since(now));
                    next_tick = deadline + frame;
                }
            })
        {
            log_debug(&format!("Could not start Slices camera motion: {error}"));
            self.settle_gallery_motion();
        }
    }

    fn settle_gallery_motion(&mut self) {
        self.scene_motion.settle();
        if let Some(p) = &mut self.hex_presentation {
            p.inset = self.scene_motion.inset(p.viewport[0]);
            p.settle();
        }
        if let Some(p) = &mut self.gallery_presentation {
            p.settle();
        }
        if let Some(p) = &mut self.wall_presentation {
            p.settle();
        }
        self.settle_depth_motion();
        self.motion = 0.0;
        self.motion_velocity = 0.0;
    }

    pub(super) fn engage_gallery(&mut self) {
        self.gallery_engaged = true;
        if let Some(p) = &mut self.wall_presentation {
            p.engaged = true;
        }
    }

    fn navigation_trace_fields(&self) -> String {
        let camera = match self.gallery_mode {
            GalleryMode::Slices => self.gallery_presentation.as_ref().map(|p| &p.camera),
            GalleryMode::Geometric => self.hex_presentation.as_ref().map(|p| &p.camera),
            GalleryMode::Wall => self.wall_presentation.as_ref().map(|p| &p.camera),
            GalleryMode::Depth => self.depth_camera.as_ref(),
        };
        format!(
            "{} mode={:?} camera_current={:?} camera_target={:?} gallery_keyboard={} hex_keyboard={}",
            self.selected_item_trace_fields(),
            self.gallery_mode,
            camera.map(|c| c.value()),
            camera.map(|c| c.target),
            self.gallery_keyboard,
            self.hex_keyboard,
        )
    }

    pub(super) fn navigate_selection(&mut self, offset: isize) {
        self.colours.selection_anchor = None;
        self.gallery_keyboard = true;
        if self.gallery_mode == GalleryMode::Geometric {
            self.hex_keyboard = true;
            if let Some(p) = &mut self.hex_presentation {
                p.keyboard = true;
            }
        }
        if self.filtered_indices.is_empty() {
            return;
        }
        let current = self
            .selected
            .and_then(|selected| {
                self.filtered_indices
                    .iter()
                    .position(|index| *index == selected)
            })
            .unwrap_or(0);
        let next = current
            .saturating_add_signed(offset)
            .min(self.filtered_indices.len() - 1);
        if next != current {
            self.engage_gallery();
        }
        if self.gallery_mode == GalleryMode::Wall {
            self.sync_wall_presentation();
            if let Some(p) = &mut self.wall_presentation {
                p.keyboard = true;
                if next != current {
                    p.follow(next);
                }
            }
        }
        if next != current {
            self.cancel_detail("keyboard-selection-navigation");
            self.wall_scroll_offset = 0.0;
            self.selected = self.filtered_indices.get(next).copied();
            self.start_motion(if next > current { 1.0 } else { -1.0 });
            apply_trace::event_lazy("ui-item-selected", || {
                format!(
                    "reason=keyboard-selection-navigation {}",
                    self.navigation_trace_fields()
                )
            });
            self.schedule_posters();
        }
    }

    pub(super) fn navigate_to_edge(&mut self, last: bool) {
        self.colours.selection_anchor = None;
        self.gallery_keyboard = true;
        if self.gallery_mode == GalleryMode::Geometric {
            self.hex_keyboard = true;
            if let Some(p) = &mut self.hex_presentation {
                p.keyboard = true;
            }
        }
        if self.filtered_indices.is_empty() {
            return;
        }
        let next = if last {
            self.filtered_indices.len() - 1
        } else {
            0
        };
        let current = self.selected.and_then(|selected| {
            self.filtered_indices
                .iter()
                .position(|index| *index == selected)
        });
        self.engage_gallery(); // Source seek accepts same-index Home/End, without repeating grid_follow.
        if self.gallery_mode == GalleryMode::Wall {
            self.sync_wall_presentation();
            if let Some(p) = &mut self.wall_presentation {
                p.keyboard = true;
                if current != Some(next) {
                    p.follow(next);
                }
            }
            self.start_motion(0.0);
        }
        if current != Some(next) {
            self.cancel_detail("keyboard-edge-navigation");
            self.wall_scroll_offset = 0.0;
            self.selected = self.filtered_indices.get(next).copied();
            self.start_motion(if current.is_some_and(|current| next > current) {
                1.0
            } else {
                -1.0
            });
            self.selected = self.filtered_indices.get(next).copied();
            apply_trace::event_lazy("ui-item-selected", || {
                format!(
                    "reason=keyboard-edge-navigation {}",
                    self.navigation_trace_fields()
                )
            });
            self.schedule_posters();
        }
    }

    pub(super) fn set_gallery_mode(&mut self, mode: GalleryMode) {
        if self.gallery_mode == GalleryMode::Depth && mode == GalleryMode::Depth {
            return;
        }
        let changed = self.gallery_mode != mode;
        if changed {
            self.stop_gallery_transients("gallery-mode-changed");
        }
        let studio = self.panel_state.settings_open && self.settings_workbench.layout_studio();
        let mut effects = Vec::with_capacity(4);
        navigate_gallery_mode(
            &mut self.state.gallery,
            gallery_kind(self.gallery_mode),
            gallery_kind(mode),
            |effect| effects.push(effect),
        );
        for effect in effects {
            match effect {
                GalleryNavigationEffect::InvalidatePosterWork => self.invalidate_poster_work(),
                GalleryNavigationEffect::ResetWallScroll => {
                    self.wall_scroll_offset = 0.0;
                    self.wall_presentation = None;
                }
                GalleryNavigationEffect::PersistState => {
                    self.gallery_mode = mode;
                    if mode == GalleryMode::Depth && self.depth_camera.is_none() {
                        self.reset_depth_transients();
                        if !studio {
                            self.restart_gallery_entrance();
                        }
                        self.start_motion(0.0);
                    }
                    self.persist_state();
                }
                GalleryNavigationEffect::SchedulePosters => self.schedule_posters(),
            }
        }
        if changed {
            self.reveal_gallery_layout();
        }
    }

    pub(super) fn invalidate_poster_work(&mut self) {
        self.poster_loading = false;
        let generation = self.poster_generation.fetch_add(1, Ordering::AcqRel) + 1;
        if let Err(error) = self.poster_mailbox.expect(generation, Vec::new()) {
            self.poster_worker = None;
            self.poster_mailbox = Arc::new(poster_worker::PosterMailbox::default());
            let _ = self.poster_mailbox.expect(generation, Vec::new());
            self.record_poster_worker_failure(error);
        }
        if let Some(worker) = &self.poster_worker {
            worker.cancel_posters_pending();
        }
        self.poster_request_key = None;
        self.poster_assignment_key = None;
    }

    pub(super) fn clear_posters(&mut self) {
        self.depth_roll = None;
        self.ordinary_roll = None;
        self.ordinary_transition_issue = None;
        self.invalidate_poster_work();
        self.posters = Arc::new(Vec::new());
        self.poster_slots.clear();
    }

    pub(super) fn record_poster_worker_failure(&mut self, detail: impl std::fmt::Display) {
        self.poster_failure = true;
        let message = format!(
            "Poster preview worker was unavailable; the request was not scheduled: {detail}"
        );
        self.status = Some(self.status_with_persistence_issue(&message));
    }

    pub(super) fn reset_depth_transients(&mut self) {
        self.depth_roll = None;
        if self.gallery_mode != GalleryMode::Depth {
            self.depth_hover = None;
            self.depth_frame = None;
            self.depth_camera = None;
            self.depth_wheel = 0.0;
            self.depth_selection.clear();
            return;
        }
        self.motion_generation.fetch_add(1, Ordering::AcqRel);
        self.depth_hover = None;
        self.depth_wheel = 0.0;
        self.depth_frame = None;
        self.depth_selection.clear();
        let target = self
            .selected
            .and_then(|i| self.filtered_indices.iter().position(|j| *j == i))
            .unwrap_or(0) as f32;
        if self.depth_presentation.is_none() {
            self.depth_presentation = Some(DepthPresentation::target(&self.state));
        }
        let d = &self.state.gallery.depth;
        self.depth_camera = Some(GalleryMotionState::absolute_camera(
            target,
            target,
            d.navigation_ms,
            d.spacing,
            d.width,
        ));
        if let Some(id) = self
            .selected
            .and_then(|i| self.wallpapers.get(i))
            .map(|w| w.id.clone())
        {
            self.depth_selection.insert(
                id,
                GalleryMotionState::absolute_camera(1.0, 1.0, d.navigation_ms, 50.0, 50.0),
            );
        }
        self.cancel_detail("depth-catalog-or-filter-reset");
        if self.visible
            && (self.scene_motion.unsettled()
                || self
                    .depth_presentation
                    .is_some_and(|p| p.unsettled(DepthPresentation::target(&self.state))))
        {
            self.start_motion(0.0);
        }
    }
    fn settle_depth_motion(&mut self) {
        self.depth_roll = None;
        if self.gallery_mode != GalleryMode::Depth {
            return;
        }
        self.depth_presentation = Some(DepthPresentation::target(&self.state));
        if let Some(camera) = &mut self.depth_camera {
            *camera = GalleryMotionState::absolute_camera(
                camera.target,
                camera.target,
                self.state.gallery.depth.navigation_ms,
                self.state.gallery.depth.spacing,
                self.state.gallery.depth.width,
            );
        }
        for spring in self.depth_selection.values_mut() {
            *spring = GalleryMotionState::absolute_camera(
                spring.target,
                spring.target,
                self.state.gallery.depth.navigation_ms,
                50.0,
                50.0,
            );
        }
    }
    pub(super) fn capture_depth_roll(&mut self) -> Option<DepthRoll> {
        if self.gallery_mode != GalleryMode::Depth || !self.visible {
            return None;
        }
        let frame = self.depth_frame.as_ref()?;
        if frame.cards.is_empty() && frame.outgoing.is_empty() {
            return None;
        }
        // Keep only currently presented visual bodies in presented order:
        // current live followed by prior outgoing. Deduplicate cells;
        // neither history nor retained pixels ever exceeds the 32-layer budget.
        let mut old = Vec::new();
        let mut cells = HashSet::new();
        for body in frame.cards.iter().chain(frame.outgoing.iter()) {
            if body.card.size[0] < 1.0 || body.card.opacity <= 0.0001 {
                continue;
            }
            let Some(wallpaper) = self.wallpapers.get(body.card.index) else {
                continue;
            };
            let Some(slot) = self.poster_slots.get(&body.card.index).filter(|slot| {
                slot.catalog_generation == self.catalog_generation.load(Ordering::Acquire)
                    && poster_slot_matches(slot, wallpaper)
                    && slot.generation <= self.poster_generation.load(Ordering::Acquire)
                    && slot.layer < gpu_gallery::MAX_POSTERS as u32
            }) else {
                continue;
            };
            let Some(poster) = self
                .posters
                .iter()
                .find(|poster| poster.layer == slot.layer && poster.source == slot.source)
            else {
                continue;
            };
            let cell = gpu_gallery::depth_roll_cell(body.card.center);
            if !cells.insert(cell) {
                continue;
            }
            if old.len() == gpu_gallery::MAX_POSTERS {
                old.clear();
                self.depth_roll_fallbacks += 1;
                apply_trace::event_lazy("depth-filter-roll-fallback", || {
                    format!(
                        "reason=history-bound capacity={} count={} revision={}",
                        gpu_gallery::MAX_POSTERS,
                        self.depth_roll_fallbacks,
                        frame.revision
                    )
                });
                break;
            }
            let mut body = body.clone();
            body.card.layer = Some(slot.layer);
            body.retained_texture = Some(poster.clone());
            if body.visual_detail_progress.is_none() {
                body.visual_detail_progress = Some(self.detail_progress);
            }
            old.push(DepthGhost {
                phase: gpu_gallery::depth_roll_start(body.card.center[0], frame.viewport[0]),
                index: body.card.index,
                body,
                slot: slot.clone(),
                poster: poster.clone(),
            });
        }
        Some(DepthRoll {
            wave: 0.0,
            old,
            incoming: HashMap::new(),
            completed: HashMap::new(),
        })
    }
    fn apply_depth_roll(&mut self, frame: &mut gpu_gallery::DepthSnapshot) {
        let Some(mut roll) = self.depth_roll.take() else {
            return;
        };
        if !roll.active() {
            return;
        }
        roll.old.retain(|g| {
            self.wallpapers.get(g.index).is_some_and(|w| {
                g.slot.catalog_generation == self.catalog_generation.load(Ordering::Acquire)
                    && poster_slot_matches(&g.slot, w)
                    && self.posters.iter().any(|p| p.same_resource(&g.poster))
            })
        });
        let mut identities: HashSet<usize> = frame.live_poster_indices.iter().copied().collect();
        for ghost in &roll.old {
            identities.insert(ghost.index);
        }
        if identities.len() > gpu_gallery::MAX_POSTERS {
            let union = identities.len();
            roll.old.clear();
            self.depth_roll_fallbacks += 1;
            apply_trace::event_lazy("depth-filter-roll-fallback", || {
                format!(
                    "reason=texture-union-overload union={union} capacity={} count={} revision={}",
                    gpu_gallery::MAX_POSTERS,
                    self.depth_roll_fallbacks,
                    frame.revision
                )
            });
        }
        let mut current_cells = HashSet::new();
        frame.cards.retain_mut(|body| {
            let cell = gpu_gallery::depth_roll_cell(body.card.center);
            current_cells.insert(cell);
            if roll.completed.get(&cell).is_some_and(|completed| {
                completed.catalog_generation == self.catalog_generation.load(Ordering::Acquire)
                    && completed.identity.index == body.card.index
                    && completed.identity.item_id == body.id
                    && self
                        .wallpapers
                        .get(body.card.index)
                        .is_some_and(|wallpaper| {
                            completed.identity.source == poster_source_for_item(wallpaper)
                                && completed.identity.is_video
                                    == (wallpaper.source == WallpaperSource::LocalVideo)
                        })
            }) {
                return true;
            }
            roll.completed.remove(&cell);
            let matching = roll
                .old
                .iter()
                .position(|g| gpu_gallery::depth_roll_cell(g.body.card.center) == cell);
            let phase = if let Some(index) = matching {
                let ghost = &roll.old[index];
                if ghost.index == body.card.index && ghost.body.id == body.id && ghost.phase <= 0.1
                {
                    let ghost = roll.old.remove(index);
                    roll.completed.insert(
                        cell,
                        DepthCompletedCell {
                            catalog_generation: ghost.slot.catalog_generation,
                            identity: PosterRequestIdentity {
                                index: ghost.index,
                                item_id: ghost.slot.item_id,
                                source: ghost.slot.source,
                                is_video: ghost.slot.is_video,
                            },
                        },
                    );
                    roll.incoming.remove(&cell);
                    return true;
                }
                ghost.phase
            } else {
                *roll.incoming.entry(cell).or_insert_with(|| {
                    roll.wave
                        + gpu_gallery::depth_roll_start(body.card.center[0], frame.viewport[0])
                })
            };
            gpu_gallery::depth_roll_cut(body, gpu_gallery::depth_roll_fraction(phase), true)
        });
        roll.incoming
            .retain(|cell, phase| current_cells.contains(cell) && *phase < 1.0);
        roll.completed
            .retain(|cell, _| current_cells.contains(cell));
        frame.outgoing = roll
            .old
            .iter()
            .filter_map(|old| {
                let mut body = old.body.clone();
                gpu_gallery::depth_roll_cut(
                    &mut body,
                    gpu_gallery::depth_roll_fraction(old.phase),
                    false,
                )
                .then_some(body)
            })
            .collect();
        self.depth_roll = Some(roll);
    }

    pub(super) fn rebuild_depth_frame(&mut self) {
        if self.gallery_mode != GalleryMode::Depth || !self.visible {
            self.depth_frame = None;
            return;
        }
        let selected_id = self
            .selected
            .and_then(|i| self.wallpapers.get(i))
            .map(|w| w.id.clone());
        if self.depth_detail_id.is_some() && self.depth_detail_id != selected_id {
            self.cancel_detail("depth-frame-identity-change");
        }
        self.depth_revision = self.depth_revision.wrapping_add(1);
        let selected = self
            .selected
            .and_then(|i| self.wallpapers.get(i))
            .map(|w| w.id.as_str());
        let frame = gpu_gallery::depth_frame_for(
            self.filtered_indices.len(),
            |filtered_index| {
                let i = *self.filtered_indices.get(filtered_index)?;
                let w = self.wallpapers.get(i)?;
                let aspect = self
                    .poster_slots
                    .get(&i)
                    .filter(|s| poster_slot_matches(s, w))
                    .and_then(|s| self.posters.iter().find(|p| p.layer == s.layer))
                    .map(|p| p.aspect_ratio)
                    .or_else(|| {
                        self.catalog
                            .get(i)
                            .filter(|c| c.width > 0 && c.height > 0)
                            .map(|c| c.width as f32 / c.height as f32)
                    });
                Some(gpu_gallery::DepthItem {
                    id: &w.id,
                    master_index: i,
                    source_aspect: aspect,
                })
            },
            self.depth_revision,
            selected,
            self.depth_hover.as_deref(),
            self.depth_camera.map_or(0.0, |c| c.value()),
            self.viewport,
            self.gallery_composition(),
            self.depth_presentation
                .unwrap_or_else(|| DepthPresentation::target(&self.state))
                .params,
            self.depth_presentation.map_or(1.0, |p| p.entrance.value()),
        );
        let mut snapshot = gpu_gallery::DepthSnapshot::from_frame(frame);
        snapshot.entrance = self.depth_presentation.map_or(1.0, |p| p.entrance.value());
        snapshot.launch = self
            .depth_presentation
            .map_or(donor_scene::LaunchAnimation::None, |p| p.launch);
        snapshot.selection_frame = self.state.gallery.depth.selection_frame;
        snapshot.shadows = self.state.gallery.depth.shadows;
        snapshot.detail = donor_scene::DepthDetailPresentation {
            effect: candidate_state::depth_effect_id(&self.state.card_flip().effect).unwrap_or(0),
            animate_shader: self.state.card_flip().animate_shader,
            animate_back: self.state.card_flip().animate_back,
        };
        for entry in &mut snapshot.cards {
            entry.selection_progress = self
                .depth_selection
                .get(&entry.id)
                .map_or(f32::from(entry.card.selected), |spring| spring.value());
        }
        for entry in &mut snapshot.cards {
            donor_scene::launch_card(
                &mut entry.card,
                snapshot.entrance,
                snapshot.launch,
                self.viewport,
            );
        }
        self.apply_depth_roll(&mut snapshot);
        let requests = depth_poster_requests(
            &snapshot,
            &self.filtered_indices,
            &self.wallpapers,
            self.depth_roll.as_ref().map_or(&[], |r| r.old.as_slice()),
        );
        let pins = self
            .depth_roll
            .as_ref()
            .map_or(&[][..], |r| r.old.as_slice());
        self.poster_slots.retain(|index, slot| {
            pins.iter().any(|g| g.index == *index && g.slot == *slot)
                || requests.iter().any(|r| {
                    r.index == *index
                        && r.item_id == slot.item_id
                        && r.source == slot.source
                        && r.is_video == slot.is_video
                        && slot.catalog_generation
                            == self.catalog_generation.load(Ordering::Acquire)
                })
        });
        if self.posters.iter().any(|p| {
            !self
                .poster_slots
                .values()
                .any(|slot| slot.layer == p.layer && slot.source == p.source)
        }) {
            self.posters = Arc::new(
                self.posters
                    .iter()
                    .filter(|p| {
                        self.poster_slots
                            .values()
                            .any(|slot| slot.layer == p.layer && slot.source == p.source)
                    })
                    .cloned()
                    .collect(),
            );
        }
        self.depth_frame = Some(snapshot);
        self.resolve_depth_detail();
    }
    pub(super) fn schedule_posters(&mut self) {
        self.schedule_posters_with_still_selector(library::media_still::MediaStill::select);
    }

    pub(super) fn schedule_posters_with_still_selector(
        &mut self,
        mut select_still: impl FnMut(
            &scanner::WallpaperItem,
            u64,
        ) -> Option<library::media_still::MediaStill>,
    ) {
        self.restore_wall_start_position();
        if self.cache_operation.is_some() {
            return;
        }
        if self.sync_gallery_presentation() {
            self.start_motion(0.0);
        }
        self.rebuild_depth_frame();
        self.reconcile_ordinary_roll();
        if !self.visible || self.wallpapers.is_empty() || self.filtered_indices.is_empty() {
            if self.posters.is_empty()
                && self.poster_slots.is_empty()
                && self.poster_request_key.is_none()
            {
                self.invalidate_poster_work();
            } else {
                self.clear_posters();
            }
            return;
        }

        if let Some(failure) = self
            .poster_worker
            .as_ref()
            .and_then(poster_worker::PosterWorker::unavailable_reason)
        {
            self.poster_worker = None;
            self.poster_request_key = None;
            self.record_poster_worker_failure(failure);
        }

        let requested = if let Some(frame) = &self.depth_frame {
            depth_poster_requests(
                frame,
                &self.filtered_indices,
                &self.wallpapers,
                self.depth_roll.as_ref().map_or(&[], |r| r.old.as_slice()),
            )
        } else {
            let poster_aspects = filtered_poster_aspects(&self.catalog, &self.filtered_indices);
            let indices = if matches!(
                self.gallery_mode,
                GalleryMode::Slices | GalleryMode::Geometric | GalleryMode::Wall
            ) {
                self.ordinary_poster_indices()
            } else {
                poster_indices_for_view(
                    self.gallery_mode,
                    &self.filtered_indices,
                    self.selected,
                    self.viewport,
                    gallery_layout_params_for_app(self),
                    &poster_aspects,
                )
            };
            indices
                .into_iter()
                .filter_map(|index| {
                    let wallpaper = self.wallpapers.get(index)?;
                    Some(PosterRequestIdentity {
                        index,
                        item_id: wallpaper.id.clone(),
                        source: poster_source_for_item(wallpaper),
                        is_video: wallpaper.source == WallpaperSource::LocalVideo,
                    })
                })
                .collect()
        };
        let capacity = if self.gallery_mode == GalleryMode::Depth {
            gpu_gallery::MAX_POSTERS
        } else {
            gpu_gallery::ordinary_poster_limit()
        };
        let pins = self.gallery_pins();
        let mut sources = poster_sources_bounded(
            &requested,
            &self.poster_slots,
            &pins,
            self.catalog_generation.load(Ordering::Acquire),
            capacity,
        );
        let mut assignment_key: Vec<_> = sources.iter().map(|s| (s.index, s.layer)).collect();
        assignment_key.sort_unstable();
        if !poster_request_changed(self.poster_request_key.as_deref(), &requested)
            && self.poster_assignment_key.as_ref() == Some(&assignment_key)
        {
            return;
        }
        // Unchanged demand does not need new captured still descriptors.
        // Live source/slot checks above and worker freshness checks remain.
        for source in &mut sources {
            source.still = self.wallpapers.get(source.index).and_then(|item| {
                select_still(item, self.catalog_generation.load(Ordering::Acquire))
            });
        }
        let generation = self.poster_generation.fetch_add(1, Ordering::AcqRel) + 1;
        let request_key = requested.clone();
        if !self.ensure_preview_worker() {
            return;
        }

        let batch =
            poster_worker::PosterBatch::new(generation, sources, self.poster_cache_dir.clone());
        if let Err(error) =
            self.poster_mailbox
                .expect_bounded(generation, batch.sources.clone(), capacity)
        {
            self.invalidate_poster_work();
            self.poster_worker = None;
            self.record_poster_worker_failure(error);
            return;
        }
        let Some(worker) = self.poster_worker.as_ref() else {
            self.poster_request_key = None;
            self.record_poster_worker_failure("worker ownership was lost");
            return;
        };
        match worker.submit(batch) {
            Ok(()) => {
                self.poster_loading = true;
                self.poster_request_key = Some(request_key);
                self.poster_assignment_key = Some(assignment_key);
            }
            Err(error) => {
                self.poster_worker = None;
                self.poster_request_key = None;
                self.record_poster_worker_failure(error);
            }
        }
    }
}

// Elapsed time includes work and oversleep, with the same bounded spring step.
pub(super) fn motion_tick_elapsed(
    previous: &mut std::time::Instant,
    now: std::time::Instant,
) -> f32 {
    let dt = now
        .saturating_duration_since(*previous)
        .as_secs_f32()
        .min(0.05);
    *previous = now;
    dt
}

// Keep the requested phase without a burst of late frames after a long stall.
pub(super) fn motion_tick_deadline(
    next: std::time::Instant,
    interval: std::time::Duration,
    now: std::time::Instant,
) -> std::time::Instant {
    if next > now {
        return next;
    }
    let skipped = now.duration_since(next).as_nanos() / interval.as_nanos() + 1;
    next + interval.mul_f64(skipped as f64)
}

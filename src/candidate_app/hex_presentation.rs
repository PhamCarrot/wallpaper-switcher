//! beta.24 Hex camera/column springs in the existing application clock owner.
use super::*;
use candidate_state::mode_profiles::{HexProfileSnapshot, Snapshot};
use donor_layout::{HexCurve, HexParams, HexShape, StageParams};
use gpu_gallery::SliceCard;
use view::settings::composed::SettingsSource;

#[derive(Debug, Clone)]
pub(super) struct HexPresentation {
    pub ids: Arc<[String]>,
    positions: Arc<HashMap<String, usize>>,
    pub selected: String,
    pub hover: Option<String>,
    pub camera: GalleryMotionState,
    pub scales: HashMap<usize, GalleryMotionState>,
    pub selection: HashMap<String, GalleryMotionState>,
    pub params: HexParams,
    pub target: HexParams,
    topology_from: Option<Arc<[SliceCard]>>,
    topology_progress: f32,
    target_circle: bool,
    pub viewport: [f32; 2],
    pub duration: f32,
    pub keyboard: bool,
    pub parallax: GalleryMotionState,
    pub circle: bool,
    pub inset: f32,
    pub rtl: bool,
    pub entrance: GalleryMotionState,
    pub launch: donor_scene::LaunchAnimation,
}

fn parameters(p: &HexProfileSnapshot, viewport: [f32; 2]) -> HexParams {
    HexParams {
        r: p.radius.max(donor_layout::MIN_HEX_R),
        rows: (p.rows as usize).max(1),
        cols: (p.columns as usize).max(3),
        scroll_step: (p.scroll_step as usize).max(1),
        curve: match p.curve {
            StateGeometricCurve::Flat => HexCurve::Flat,
            StateGeometricCurve::Arc => HexCurve::Arc,
            StateGeometricCurve::Wave => HexCurve::Wave,
            StateGeometricCurve::S => HexCurve::S,
            StateGeometricCurve::Cylinder => HexCurve::Cylinder,
        },
        shape: match p.shape {
            StateGeometricShape::Triangle => HexShape::Triangle,
            StateGeometricShape::Diamond => HexShape::Diamond,
            StateGeometricShape::Rhombus => HexShape::Rhombus,
            _ => HexShape::Hexagon,
        },
        curve_strength: p.curve_strength,
        curve_frequency: p.curve_frequency,
        gap_x: p.gap_x,
        gap_y: p.gap_y,
        aspect: p.aspect.max(0.05),
        stagger: p.stagger,
        lens: p.lens,
        lens_radius: p.lens_radius.max(1.0),
        orbit: p.orbit,
        orbit_radius: if p.orbit_radius > 0.0 {
            p.orbit_radius
        } else {
            viewport[0].min(viewport[1]) * 0.55
        },
        twist: p.twist,
        scatter: p.scatter,
        stage: StageParams {
            offset_x: p.stage.x,
            offset_y: p.stage.y,
            scale: p.stage.scale,
            rotation: p.stage.rotation_degrees,
            perspective: p.stage.perspective,
            shear_x: p.stage.shear_x,
            shear_y: p.stage.shear_y,
            depth_angle: p.stage.depth_angle_degrees,
        },
    }
}

fn morph_numeric(current: &mut HexParams, target: &HexParams, amount: f32) {
    let topology = (
        current.rows,
        current.cols,
        current.scroll_step,
        current.curve,
        current.shape,
    );
    current.morph_toward(target, amount);
    current.rows = topology.0;
    current.cols = topology.1;
    current.scroll_step = topology.2;
    current.curve = topology.3;
    current.shape = topology.4;
}

fn numeric_settled(current: &HexParams, target: &HexParams) -> bool {
    let mut probe = *current;
    probe.rows = target.rows;
    probe.cols = target.cols;
    probe.scroll_step = target.scroll_step;
    probe.curve = target.curve;
    probe.shape = target.shape;
    probe.settled_to(target)
}

impl HexPresentation {
    pub fn new(
        ids: impl Into<Arc<[String]>>,
        selected: String,
        p: HexProfileSnapshot,
        viewport: [f32; 2],
        duration: f32,
        inset: f32,
        rtl: bool,
    ) -> Self {
        let ids = ids.into();
        let params = parameters(&p, viewport);
        let index = ids.iter().position(|id| *id == selected).unwrap_or(0);
        let center = params.column_x(index / params.rows);
        let mut state = Self {
            positions: Arc::new(
                ids.iter()
                    .enumerate()
                    .map(|(i, id)| (id.clone(), i))
                    .collect(),
            ),
            ids,
            selected,
            hover: None,
            camera: GalleryMotionState::source(center, center, duration, 1.0),
            scales: HashMap::new(),
            selection: HashMap::new(),
            params,
            target: params,
            topology_from: None,
            topology_progress: 1.0,
            target_circle: p.shape == StateGeometricShape::Circle,
            viewport,
            duration,
            keyboard: true,
            parallax: gallery_presentation::parallax_motion(
                f32::from(p.parallax),
                p.parallax,
                duration,
            ),
            circle: p.shape == StateGeometricShape::Circle,
            inset,
            rtl,
            entrance: GalleryMotionState::source(1.0, 1.0, 250.0, 1.0),
            launch: donor_scene::LaunchAnimation::Fade,
        };
        state.retarget(true);
        state
    }

    pub fn sync(
        &mut self,
        ids: impl Into<Arc<[String]>>,
        selected: String,
        p: HexProfileSnapshot,
        viewport: [f32; 2],
        duration: f32,
        inset: f32,
        rtl: bool,
    ) -> bool {
        let ids = ids.into();
        let target = parameters(&p, viewport);
        let target_circle = p.shape == StateGeometricShape::Circle;
        let target_changed =
            !self.target.settled_to(&target) || self.target_circle != target_circle;
        let geometry =
            target_changed || self.viewport != viewport || self.inset != inset || self.rtl != rtl;
        let catalog = !Arc::ptr_eq(&self.ids, &ids) && self.ids != ids;
        let changed = geometry
            || catalog
            || self.selected != selected
            || self.duration != duration
            || self.parallax.target != f32::from(p.parallax)
            || self.target_circle != target_circle;
        if !changed {
            self.ids = ids;
            return false;
        }
        let restart = !catalog
            && (self.target.topology_differs(&target)
                || self.target_circle != target_circle
                || (target_changed && self.topology_from.is_some()));
        if restart {
            let mut source = self.placed_frame();
            source.retain(|card| card.opacity > 0.0);
            self.topology_from = Some(source.into());
            self.topology_progress = 0.0;
            self.scales.clear();
        } else if catalog && self.topology_from.take().is_some() {
            self.params = target;
            self.circle = target_circle;
            self.topology_progress = 1.0;
        } else if catalog {
            // Filtered indices cannot carry the old frame across a catalog change.
            self.params.rows = target.rows;
            self.params.cols = target.cols;
            self.params.scroll_step = target.scroll_step;
            self.params.curve = target.curve;
            self.params.shape = target.shape;
            self.circle = target_circle;
        }
        if self.selected != selected {
            for (id, was) in [(self.selected.clone(), true), (selected.clone(), false)] {
                let current = self
                    .selection
                    .get(&id)
                    .map_or(f32::from(was), |s| s.value());
                self.selection.insert(
                    id,
                    GalleryMotionState::source(current, f32::from(!was), duration, 1.0),
                );
            }
        }
        if catalog {
            self.positions = Arc::new(
                ids.iter()
                    .enumerate()
                    .map(|(i, id)| (id.clone(), i))
                    .collect(),
            );
            self.selection
                .retain(|id, _| self.positions.contains_key(id));
        }
        if catalog {
            self.scales.clear();
        }
        if self.duration != duration {
            for spring in self
                .scales
                .values_mut()
                .chain(self.selection.values_mut())
                .chain(std::iter::once(&mut self.camera))
            {
                *spring = GalleryMotionState::source(spring.value(), spring.target, duration, 1.0);
            }
        }
        if catalog {
            self.ids = ids;
        }
        self.selected = selected;
        self.target = target;
        self.viewport = viewport;
        if self.parallax.target != f32::from(p.parallax) || self.duration != duration {
            self.parallax =
                gallery_presentation::parallax_motion(self.parallax.value(), p.parallax, duration);
        }
        self.duration = duration;
        self.inset = inset;
        self.rtl = rtl;
        self.target_circle = target_circle;
        self.retarget(restart || catalog || (geometry && self.topology_from.is_none()));
        true
    }

    pub fn center_x(&self) -> f32 {
        let inset = self.inset.clamp(0.0, self.viewport[0] * 0.6);
        (if self.rtl { 0.0 } else { inset }) + (self.viewport[0] - inset) * 0.5
    }
    pub fn camera_target(&self) -> f32 {
        let p = self.placement_params();
        let index = self.positions.get(&self.selected).copied().unwrap_or(0);
        let center = p.column_x(index / p.rows.max(1));
        let cx = self.center_x();
        let fade = ((self.viewport[0] - self.inset - p.visible_band()) * 0.5).max(0.0);
        let (lead, trail) = if self.rtl {
            (0.0, self.inset)
        } else {
            (self.inset, 0.0)
        };
        let left = lead + fade + p.step_x();
        let right = self.viewport[0] - trail - fade - p.step_x();
        let target = if self.keyboard || right <= left {
            center
        } else {
            let screen = cx - self.camera.target + center;
            if screen < left {
                self.camera.target + screen - left
            } else if screen > right {
                self.camera.target + screen - right
            } else {
                self.camera.target
            }
        };
        let min = p.r;
        let max = p.column_x(self.ids.len().div_ceil(p.rows.max(1)).saturating_sub(1));
        target.clamp(min.min(max), min.max(max))
    }

    pub fn scale_target(&self, col: usize) -> f32 {
        let p = self.placement_params();
        if p.curve == HexCurve::Flat {
            return 1.0;
        }
        let cx = self.center_x();
        let cy = self.viewport[1] * 0.5;
        let raw = cx - self.camera.value() + p.column_x(col);
        let (x, y) = p.deform(col * p.rows.max(1), raw, cy, (cx, cy));
        let (x, _, _) = p
            .stage
            .transform(x, y, (cx, cy), (self.viewport[0], self.viewport[1]));
        let fade = ((self.viewport[0] - self.inset - p.visible_band()) * 0.5).max(0.0);
        let (lead, trail) = if self.rtl {
            (0.0, self.inset)
        } else {
            (self.inset, 0.0)
        };
        let left = lead + fade;
        let right = self.viewport[0] - trail - fade;
        let half = ((right - left) * 0.5).max(1.0);
        let dn = (x - (left + right) * 0.5).abs() / half;
        let t = ((dn - 0.7) / 0.3).clamp(0.0, 1.0);
        let base = 1.0 - 0.2 * t * t * (3.0 - 2.0 * t);
        let over = (left - x).max(x - right).max(0.0);
        let cut = (1.0 - over / (p.step_x() * 1.2).max(0.001)).clamp(0.0, 1.0);
        base * cut * cut
    }

    fn retarget(&mut self, anchor: bool) {
        let target = self.camera_target();
        if anchor {
            self.camera = GalleryMotionState::source(target, target, self.duration, 1.0);
        } else {
            self.camera.retarget_source(target);
        }
        for (id, spring) in &mut self.selection {
            spring.retarget_source(f32::from(*id == self.selected));
        }
        let params = self.placement_params();
        let count = self.ids.len().div_ceil(params.rows.max(1));
        let cx = self.center_x();
        let margin = self.viewport[0] + params.item_half_w().hypot(params.item_half_h()) * 2.0;
        let active = donor_layout::monotone_window(
            count,
            |col| cx - self.camera.value() + params.column_x(col),
            -margin,
            self.viewport[0] + margin,
        );
        donor_layout::layout_work(2, active.len());
        self.scales.retain(|col, _| active.contains(col));
        for col in active {
            let target = self.scale_target(col);
            self.scales
                .entry(col)
                .or_insert_with(|| GalleryMotionState::source(target, target, self.duration, 1.0))
                .retarget_source(target);
        }
    }

    fn placement_params(&self) -> HexParams {
        if self.topology_from.is_some() {
            self.target
        } else {
            self.params
        }
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        let morphing = self.topology_from.is_none() && !numeric_settled(&self.params, &self.target);
        if morphing {
            let k = 1.0 - (-dt.min(0.05) * 1000.0 / (self.duration / 4.0).max(1.0)).exp();
            morph_numeric(&mut self.params, &self.target, k);
            if numeric_settled(&self.params, &self.target) {
                self.params = self.target;
            }
        }
        let topology_moving = if self.topology_from.is_some() {
            let k = 1.0 - (-dt.min(0.05) * 1000.0 / (self.duration / 4.0).max(1.0)).exp();
            self.topology_progress += (1.0 - self.topology_progress) * k;
            if self.topology_progress >= 0.995 {
                self.topology_progress = 1.0;
                self.params = self.target;
                self.circle = self.target_circle;
                self.topology_from = None;
                false
            } else {
                true
            }
        } else {
            false
        };
        self.retarget(morphing);
        let mut moving = self.camera.tick(dt)
            | self.entrance.tick(dt)
            | self.parallax.tick(dt)
            | morphing
            | topology_moving;
        for spring in self.scales.values_mut().chain(self.selection.values_mut()) {
            moving |= spring.tick(dt);
        }
        self.selection.retain(|_, s| !s.settled());
        moving
    }

    pub fn settle(&mut self) {
        self.params = self.target;
        self.circle = self.target_circle;
        self.parallax = gallery_presentation::parallax_motion(
            self.parallax.target,
            self.parallax.target > 0.0,
            self.duration,
        );
        self.topology_from = None;
        self.topology_progress = 1.0;
        self.selection.clear();
        self.retarget(true);
        for s in self.scales.values_mut() {
            *s = GalleryMotionState::source(s.target, s.target, self.duration, 1.0);
        }
        self.entrance = GalleryMotionState::source(1.0, 1.0, 250.0, 1.0);
    }

    fn placed_cards(
        &self,
        params: HexParams,
        circle: bool,
        scales: &HashMap<usize, f32>,
        selected: usize,
    ) -> Vec<SliceCard> {
        if circle {
            return gpu_gallery::layout_gallery(
                GalleryMode::Geometric,
                self.ids.len(),
                selected,
                self.viewport,
                gpu_gallery::GalleryLayoutParams {
                    geometric: gpu_gallery::GeometricParams {
                        radius: params.r,
                        rows: params.rows,
                        columns: params.cols,
                        shape: gpu_gallery::GeometricShape::Circle,
                        curve: gpu_gallery::GeometricCurve::Flat,
                        curve_strength: params.curve_strength,
                        curve_frequency: params.curve_frequency,
                        gap_x: params.gap_x,
                        gap_y: params.gap_y,
                        aspect: params.aspect,
                        stagger: params.stagger,
                        lens: params.lens,
                        orbit: params.orbit,
                        orbit_radius: params.orbit_radius,
                        twist: params.twist,
                        scatter: params.scatter,
                        motion: (params.column_x(selected / params.rows.max(1))
                            - self.camera.value())
                            / (params.r * 2.0 * params.aspect + params.gap_x).max(1.0),
                        stage: params.stage,
                    },
                    ..Default::default()
                },
            )
            .into_iter()
            .map(|mut card| {
                card.center[0] += self.center_x() - self.viewport[0] * 0.5;
                card
            })
            .collect();
        }
        donor_layout::hex_placements_current(
            self.ids.len(),
            self.viewport,
            &params,
            self.camera.value(),
            self.center_x(),
            scales,
        )
        .into_iter()
        .map(|p| SliceCard {
            triangle_direction: p.triangle_direction,
            index: p.index,
            center: [p.x, p.y],
            size: [p.width, p.height],
            layer: None,
            selected: p.index == selected,
            opacity: p.opacity,
            skew_distance: 0.0,
            edge_tilt_distance: 0.0,
            rotation: 0.0,
            corner_radius: 0.0,
            shape: match p.shape {
                HexShape::Hexagon => 2,
                HexShape::Triangle => 4,
                _ => 3,
            },
            poster_aspect_ratio: 16.0 / 9.0,
        })
        .collect()
    }

    fn placed_frame(&self) -> Vec<SliceCard> {
        let scales = self
            .scales
            .iter()
            .map(|(col, s)| (*col, s.value().clamp(0.0, 1.0)))
            .collect();
        let selected = self.positions.get(&self.selected).copied().unwrap_or(0);
        if let Some(from) = &self.topology_from {
            let target = self.placed_cards(self.target, self.target_circle, &scales, selected);
            gallery_presentation::blend_layout_cards(from, target, selected, self.topology_progress)
        } else {
            self.placed_cards(self.params, self.circle, &scales, selected)
        }
    }

    pub(super) fn prefetch_indices(&self) -> Vec<usize> {
        let params = self.placement_params();
        let circle = if self.topology_from.is_some() {
            self.target_circle
        } else {
            self.circle
        };
        let selected = self.positions.get(&self.selected).copied().unwrap_or(0);
        let scales = self
            .scales
            .iter()
            .map(|(col, s)| (*col, s.value().clamp(0.0, 1.0)))
            .collect();
        let rows = params.rows.max(1);
        let mut columns = self
            .placed_cards(params, circle, &scales, selected)
            .into_iter()
            .map(|card| card.index / rows);
        let Some(first) = columns.next() else {
            return Vec::new();
        };
        let (first, last) =
            columns.fold((first, first), |(lo, hi), col| (lo.min(col), hi.max(col)));
        let before = first.checked_sub(1);
        let after = (last + 1 < self.ids.len().div_ceil(rows)).then_some(last + 1);
        let travel = self.camera.target - self.camera.value();
        let before_first = if !circle && params.step_x() < 0.0 {
            travel > 0.0
        } else {
            travel < 0.0
        };
        let edges = if before_first {
            [before, after]
        } else {
            [after, before]
        };
        edges
            .into_iter()
            .flatten()
            .flat_map(|col| col * rows..((col + 1) * rows).min(self.ids.len()))
            .collect()
    }

    pub fn cards(&self) -> Vec<(SliceCard, f32, bool, [f32; 4], donor_scene::SurfacePayload)> {
        let mut cards: Vec<_> = self
            .placed_frame()
            .into_iter()
            .map(|card| {
                let id = &self.ids[card.index];
                let sel = self
                    .selection
                    .get(id)
                    .map_or(f32::from(card.selected), |s| s.value())
                    .clamp(0.0, 1.0);
                let hover = self.hover.as_ref() == Some(id) && !card.selected;
                (
                    card,
                    sel,
                    hover,
                    [0.0; 4],
                    donor_scene::SurfacePayload::Plain,
                )
            })
            .collect();
        cards.sort_by(|a, b| {
            (a.1 + if a.2 { 0.3 } else { 0.0 }).total_cmp(&(b.1 + if b.2 { 0.3 } else { 0.0 }))
        });
        cards
    }
}

impl GpuCandidate {
    pub(super) fn sync_hex_presentation(&mut self) -> bool {
        let Snapshot::Hex(p) = view::settings::profile_projection::capture(self) else {
            return false;
        };
        let ids = self.presentation_inputs().ids;
        let selected = self
            .selected
            .and_then(|i| self.wallpapers.get(i))
            .map_or_else(
                || ids.first().cloned().unwrap_or_default(),
                |w| w.id.clone(),
            );
        let source = view::settings::bridge::Source { app: self };
        let duration = source.motion_standard_ms().clamp(35.0, 2000.0);
        let rtl = self.settings_locale.language.rtl;
        let inset = self.scene_motion.inset(self.viewport[0]);
        if let Some(state) = &mut self.hex_presentation {
            state.sync(ids, selected, p, self.viewport, duration, inset, rtl)
        } else {
            self.hex_presentation = Some(HexPresentation::new(
                ids,
                selected,
                p,
                self.viewport,
                duration,
                inset,
                rtl,
            ));
            self.restart_gallery_entrance();
            true
        }
    }
}

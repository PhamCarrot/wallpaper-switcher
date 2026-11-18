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
    pub viewport: [f32; 2],
    pub duration: f32,
    pub keyboard: bool,
    pub parallax: bool,
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
            viewport,
            duration,
            keyboard: true,
            parallax: p.parallax,
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
        let geometry = !self.target.settled_to(&target)
            || self.viewport != viewport
            || self.inset != inset
            || self.rtl != rtl;
        let catalog = !Arc::ptr_eq(&self.ids, &ids) && self.ids != ids;
        let changed = geometry
            || catalog
            || self.selected != selected
            || self.duration != duration
            || self.parallax != p.parallax
            || self.circle != (p.shape == StateGeometricShape::Circle);
        if !changed {
            self.ids = ids;
            return false;
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
        if catalog || self.params.topology_differs(&target) {
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
        self.duration = duration;
        self.inset = inset;
        self.rtl = rtl;
        self.parallax = p.parallax;
        self.circle = p.shape == StateGeometricShape::Circle;
        self.retarget(catalog || geometry);
        true
    }

    pub fn center_x(&self) -> f32 {
        let inset = self.inset.clamp(0.0, self.viewport[0] * 0.6);
        (if self.rtl { 0.0 } else { inset }) + (self.viewport[0] - inset) * 0.5
    }
    pub fn camera_target(&self) -> f32 {
        let p = &self.params;
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
        let p = &self.params;
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
        let count = self.ids.len().div_ceil(self.params.rows.max(1));
        let cx = self.center_x();
        let margin =
            self.viewport[0] + self.params.item_half_w().hypot(self.params.item_half_h()) * 2.0;
        let active = donor_layout::monotone_window(
            count,
            |col| cx - self.camera.value() + self.params.column_x(col),
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

    pub fn tick(&mut self, dt: f32) -> bool {
        let morphing = !self.params.settled_to(&self.target);
        if morphing {
            let k = 1.0 - (-dt.min(0.05) * 1000.0 / (self.duration / 4.0).max(1.0)).exp();
            self.params.morph_toward(&self.target, k);
            if self.params.settled_to(&self.target) {
                self.params = self.target;
            }
        }
        self.retarget(morphing);
        let mut moving = self.camera.tick(dt) | self.entrance.tick(dt) | morphing;
        for spring in self.scales.values_mut().chain(self.selection.values_mut()) {
            moving |= spring.tick(dt);
        }
        self.selection.retain(|_, s| !s.settled());
        moving
    }

    pub fn settle(&mut self) {
        self.params = self.target;
        self.selection.clear();
        self.retarget(true);
        for s in self.scales.values_mut() {
            *s = GalleryMotionState::source(s.target, s.target, self.duration, 1.0);
        }
        self.entrance = GalleryMotionState::source(1.0, 1.0, 250.0, 1.0);
    }

    pub fn cards(&self) -> Vec<(SliceCard, f32, bool, [f32; 4], donor_scene::SurfacePayload)> {
        let scales = self
            .scales
            .iter()
            .map(|(col, s)| (*col, s.value().clamp(0.0, 1.0)))
            .collect();
        let selected = self.positions.get(&self.selected).copied().unwrap_or(0);
        let placed: Vec<_> = if self.circle {
            let p = &self.params;
            gpu_gallery::layout_gallery(
                GalleryMode::Geometric,
                self.ids.len(),
                selected,
                self.viewport,
                gpu_gallery::GalleryLayoutParams {
                    geometric: gpu_gallery::GeometricParams {
                        radius: p.r,
                        rows: p.rows,
                        columns: p.cols,
                        shape: gpu_gallery::GeometricShape::Circle,
                        curve: gpu_gallery::GeometricCurve::Flat,
                        curve_strength: p.curve_strength,
                        curve_frequency: p.curve_frequency,
                        gap_x: p.gap_x,
                        gap_y: p.gap_y,
                        aspect: p.aspect,
                        stagger: p.stagger,
                        lens: p.lens,
                        orbit: p.orbit,
                        orbit_radius: p.orbit_radius,
                        twist: p.twist,
                        scatter: p.scatter,
                        motion: (p.column_x(selected / p.rows) - self.camera.value())
                            / (p.r * 2.0 * p.aspect + p.gap_x).max(1.0),
                        stage: p.stage,
                    },
                    ..Default::default()
                },
            )
            .into_iter()
            .map(|mut card| {
                card.center[0] += self.center_x() - self.viewport[0] * 0.5;
                card
            })
            .collect()
        } else {
            donor_layout::hex_placements_current(
                self.ids.len(),
                self.viewport,
                &self.params,
                self.camera.value(),
                self.center_x(),
                &scales,
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
                shape: if self.circle {
                    1
                } else {
                    match p.shape {
                        HexShape::Hexagon => 2,
                        HexShape::Triangle => 4,
                        _ => 3,
                    }
                },
                poster_aspect_ratio: 16.0 / 9.0,
            })
            .collect()
        };
        let mut cards: Vec<_> = placed
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

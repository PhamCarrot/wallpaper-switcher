//! beta.24 Wall camera and hover state, advanced by the existing MotionFrame clock.
use super::*;
use donor_scene::SurfacePayload;
use gpu_gallery::{SliceCard, WallParams};
use view::settings::composed::SettingsSource;

#[derive(Debug, Clone)]
pub(super) struct WallPresentation {
    pub ids: Arc<[String]>,
    pub selected: String,
    pub hover: Option<String>,
    pub fades: HashMap<String, GalleryMotionState>,
    pub keyboard: bool,
    pub engaged: bool,
    pub camera: GalleryMotionState,
    pub params: WallParams,
    pub target: WallParams,
    topology_from: Option<WallTopologySource>,
    topology_progress: f32,
    pub aspects: Arc<[f32]>,
    positions: Arc<HashMap<String, usize>>,
    layouts: std::cell::RefCell<[Option<Arc<gpu_gallery::WallLayoutWindow>>; 2]>,
    pub duration: f32,
    pub morph_ms: f32,
    pub entrance: GalleryMotionState,
    pub launch: donor_scene::LaunchAnimation,
}

#[derive(Debug, Clone)]
struct WallTopologySource {
    cards: Arc<[SliceCard]>,
    clip: [f32; 4],
}

impl WallPresentation {
    pub fn new(
        ids: impl Into<Arc<[String]>>,
        selected: String,
        params: WallParams,
        aspects: impl Into<Arc<[f32]>>,
        duration: f32,
    ) -> Self {
        let ids = ids.into();
        Self {
            positions: Arc::new(
                ids.iter()
                    .enumerate()
                    .map(|(i, id)| (id.clone(), i))
                    .collect(),
            ),
            layouts: Default::default(),
            ids: ids.into(),
            selected,
            hover: None,
            fades: HashMap::new(),
            keyboard: false,
            engaged: false,
            camera: GalleryMotionState::source(0.0, 0.0, duration, 1.0),
            params,
            target: params,
            topology_from: None,
            topology_progress: 1.0,
            aspects: aspects.into(),
            duration,
            morph_ms: 250.0,
            entrance: GalleryMotionState::source(1.0, 1.0, 250.0, 1.0),
            launch: donor_scene::LaunchAnimation::Fade,
        }
    }

    fn layout_for(&self, params: WallParams) -> Arc<gpu_gallery::WallLayoutWindow> {
        let mut layouts = self.layouts.borrow_mut();
        if layouts[1].as_ref().is_some_and(|p| p.matches(params)) {
            layouts.swap(0, 1);
        } else if layouts[0].as_ref().is_none_or(|p| !p.matches(params)) {
            layouts[1] = layouts[0].take();
            layouts[0] = Some(Arc::new(gpu_gallery::WallLayoutWindow::new(
                self.ids.len(),
                params,
                &self.aspects,
            )));
        }
        Arc::clone(layouts[0].as_ref().unwrap())
    }

    fn layout(&self) -> Arc<gpu_gallery::WallLayoutWindow> {
        self.layout_for(if self.topology_from.is_some() {
            self.target
        } else {
            self.params
        })
    }

    fn bounds(&self) -> f32 {
        self.layout().max_scroll
    }

    /// Restoration is an explicit anchor; ordinary selection never recenters the camera.
    pub fn restore(&mut self, index: usize, camera: Option<f32>) {
        let layout = self.layout();
        let p = &layout.donor;
        let anchor = layout
            .placements
            .get(index)
            .map_or(0.0, |c| c.y + c.height * 0.5 - p.total_h() * 0.5);
        let value = camera
            .filter(|v| v.is_finite())
            .unwrap_or(anchor)
            .clamp(0.0, self.bounds());
        self.camera = GalleryMotionState::source(value, value, self.duration, 1.0);
        self.engaged = true;
    }

    pub fn follow(&mut self, index: usize) {
        let layout = self.layout();
        let p = &layout.donor;
        let (top, height) = if self.topology_from.is_some() {
            let Some(placement) = layout.placements.get(index) else {
                return;
            };
            (placement.y, placement.height)
        } else if matches!(self.params.layout, WallLayout::Editorial) {
            let height = p.thumb_h * 2.0 + p.gap_y;
            ((index / 5) as f32 * (height + p.gap_y), height)
        } else {
            ((index / p.cols.max(1)) as f32 * p.cell_h(), p.cell_h())
        };
        let target = if top < self.camera.target {
            top.max(0.0)
        } else if top + height > self.camera.target + p.total_h() {
            top + height - p.total_h()
        } else {
            self.camera.target
        };
        self.camera
            .retarget_source(target.clamp(0.0, layout.max_scroll));
    }

    pub fn wheel(&mut self, delta: f32) {
        self.camera
            .retarget_source((self.camera.target + delta).clamp(0.0, self.bounds()));
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        let amount = 1.0 - (-4.0 * dt.min(0.05) / (self.morph_ms / 1000.0)).exp();
        let mut moving = false;
        macro_rules! morph { ($($field:ident),*) => {$({
            let value = &mut self.params.$field; let target = self.target.$field;
            if (*value - target).abs() > 0.01 { *value += (target - *value) * amount; moving = true; }
            else { *value = target; }
        })*}; }
        morph!(
            card_width,
            card_height,
            gap_x,
            gap_y,
            corner_radius,
            border_width,
            stagger,
            selected_scale,
            flow_wave,
            flow_frequency,
            scatter,
            scale_variance,
            cylinder_bend,
            cylinder_radius
        );
        let topology_moving = if self.topology_from.is_some() {
            self.topology_progress += (1.0 - self.topology_progress)
                * (1.0 - (-dt.min(0.05) * 1000.0 / (self.morph_ms / 4.0).max(1.0)).exp());
            if self.topology_progress >= 0.995 {
                self.topology_progress = 1.0;
                self.params = self.target;
                self.topology_from = None;
                false
            } else {
                true
            }
        } else {
            false
        };
        if !self.params.stage.settled_to(&self.target.stage) {
            self.params.stage.morph_toward(&self.target.stage, amount);
            moving = true;
        }
        self.camera
            .retarget_source(self.camera.target.clamp(0.0, self.bounds()));
        moving |= self.camera.tick(dt) || topology_moving;
        moving |= self.entrance.tick(dt);
        if let Some(id) = &self.hover {
            self.fades
                .entry(id.clone())
                .or_insert_with(|| GalleryMotionState::source(0.0, 1.0, 180.0, 1.0));
        }
        for (id, fade) in &mut self.fades {
            fade.retarget_source(f32::from(self.hover.as_ref() == Some(id)));
            moving |= fade.tick(dt);
        }
        self.fades.retain(|_, f| !f.settled() || f.target > 0.0);
        moving
    }

    pub fn settle(&mut self) {
        self.entrance = GalleryMotionState::source(1.0, 1.0, 250.0, 1.0);
        self.params = self.target;
        self.topology_from = None;
        self.topology_progress = 1.0;
        let target = self.camera.target.clamp(0.0, self.bounds());
        self.camera = GalleryMotionState::source(target, target, self.duration, 1.0);
    }

    pub fn frame_params(&self, composition: WallParams) -> WallParams {
        self.compose(self.params, composition)
    }

    fn compose(&self, mut p: WallParams, composition: WallParams) -> WallParams {
        p.rail_height = composition.rail_height;
        p.rail_width = composition.rail_width;
        p.rail_rtl = composition.rail_rtl;
        p.rail_gap = composition.rail_gap;
        p.scroll_offset = self.camera.value();
        p.motion = 0.0;
        p.user_engaged = self.engaged;
        p
    }

    pub fn clip(&self, viewport: [f32; 2], composition: WallParams) -> [f32; 4] {
        let Some(from) = &self.topology_from else {
            return gpu_gallery::wall_frame(viewport, composition).clip;
        };
        let target = gpu_gallery::wall_frame(viewport, self.compose(self.target, composition));
        std::array::from_fn(|i| {
            from.clip[i] + (target.clip[i] - from.clip[i]) * self.topology_progress
        })
    }

    fn cards_for_params(
        &self,
        params: WallParams,
        viewport: [f32; 2],
        composition: WallParams,
    ) -> Vec<SliceCard> {
        let frame = self.compose(params, composition);
        let layout = self.layout_for(frame);
        let selected = self.positions.get(&self.selected).copied().unwrap_or(0);
        layout.cards(selected, viewport, frame, &self.aspects)
    }

    fn placed_frame(&self, viewport: [f32; 2], composition: WallParams) -> Vec<SliceCard> {
        if let Some(from) = &self.topology_from {
            let destination = self.cards_for_params(self.target, viewport, composition);
            let selected = self.positions.get(&self.selected).copied().unwrap_or(0);
            gallery_presentation::blend_layout_cards(
                &from.cards,
                destination,
                selected,
                self.topology_progress,
            )
        } else {
            self.cards_for_params(self.params, viewport, composition)
        }
    }

    pub(super) fn prefetch_indices(
        &self,
        viewport: [f32; 2],
        composition: WallParams,
    ) -> Vec<usize> {
        let params = if self.topology_from.is_some() {
            self.target
        } else {
            self.params
        };
        let frame = self.compose(params, composition);
        let layout = self.layout_for(frame);
        let selected = self.positions.get(&self.selected).copied().unwrap_or(0);
        let mut step = layout.donor.cell_h().abs().max(1.0);
        if params.layout == WallLayout::Editorial {
            step *= 2.0;
        }
        if self.camera.target < frame.scroll_offset {
            step = -step;
        }
        [step, -step]
            .into_iter()
            .filter_map(|delta| {
                let offset = (frame.scroll_offset + delta).clamp(0.0, layout.max_scroll);
                (offset != frame.scroll_offset).then_some(WallParams {
                    scroll_offset: offset,
                    ..frame
                })
            })
            .flat_map(|frame| layout.cards(selected, viewport, frame, &self.aspects))
            .map(|card| card.index)
            .collect()
    }

    pub fn cards(
        &self,
        viewport: [f32; 2],
        composition: WallParams,
    ) -> Vec<(SliceCard, f32, bool, [f32; 4], SurfacePayload)> {
        self.placed_frame(viewport, composition)
            .into_iter()
            .map(|c| {
                let id = &self.ids[c.index];
                let hover = self
                    .fades
                    .get(id)
                    .map_or(f32::from(self.hover.as_ref() == Some(id)), |f| {
                        f.value().clamp(0.0, 1.0)
                    });
                let border = if c.selected && self.keyboard {
                    hover.max(0.8)
                } else {
                    hover
                };
                (
                    c,
                    border,
                    self.hover.as_ref() == Some(id),
                    [c.corner_radius.min(c.size[0].min(c.size[1]) * 0.5); 4],
                    SurfacePayload::Plain,
                )
            })
            .collect()
    }
}

impl GpuCandidate {
    pub(super) fn sync_wall_presentation(&mut self) -> bool {
        let params = self.configured_wall_params();
        let inputs = self.presentation_inputs();
        let ids = inputs.ids;
        let selected = self
            .selected
            .and_then(|i| self.wallpapers.get(i))
            .map_or_else(
                || ids.first().cloned().unwrap_or_default(),
                |w| w.id.clone(),
            );
        let aspects = inputs.aspects;
        let source = view::settings::bridge::Source { app: self };
        let duration = source.number("motion.slowMs") as f32;
        let morph_ms = source.number("motion.standardMs") as f32;
        let restart = self.wall_presentation.as_ref().is_some_and(|p| {
            p.target.columns != params.columns
                || p.target.rows != params.rows
                || p.target.layout != params.layout
                || (p.topology_from.is_some() && p.target != params)
        });
        let composition = restart.then(|| gallery_layout_params_for_app(self).wall);
        if let Some(p) = &mut self.wall_presentation {
            let catalog = !Arc::ptr_eq(&p.ids, &ids) && p.ids != ids;
            let aspects_changed = !Arc::ptr_eq(&p.aspects, &aspects) && p.aspects != aspects;
            let changed = catalog
                || p.selected != selected
                || p.target != params
                || aspects_changed
                || p.duration != duration
                || p.morph_ms != morph_ms;
            let mut keep_selected_visible = false;
            if catalog {
                p.topology_from = None;
                p.topology_progress = 1.0;
                p.params.columns = params.columns;
                p.params.rows = params.rows;
                p.params.layout = params.layout;
            } else if let Some(composition) = composition {
                let mut source = p.placed_frame(self.viewport, composition);
                source.retain(|card| card.opacity > 0.0);
                keep_selected_visible = p.selected == selected && source.iter().any(|c| c.selected);
                p.topology_from = Some(WallTopologySource {
                    cards: source.into(),
                    clip: p.clip(self.viewport, composition),
                });
                p.topology_progress = 0.0;
            }
            if catalog {
                p.positions = Arc::new(
                    ids.iter()
                        .enumerate()
                        .map(|(i, id)| (id.clone(), i))
                        .collect(),
                );
                p.fades.retain(|id, _| p.positions.contains_key(id));
                if p.hover
                    .as_ref()
                    .is_some_and(|id| !p.positions.contains_key(id))
                {
                    p.hover = None;
                }
            }
            if catalog || aspects_changed {
                *p.layouts.borrow_mut() = Default::default();
            }
            self.gallery_engaged |= p.engaged;
            p.engaged = self.gallery_engaged;
            p.keyboard = self.gallery_keyboard;
            p.ids = ids;
            p.selected = selected;
            p.target = params;
            p.aspects = aspects;
            p.morph_ms = morph_ms;
            if p.duration != duration {
                p.camera =
                    GalleryMotionState::source(p.camera.value(), p.camera.target, duration, 1.0);
                p.duration = duration;
            }
            if p.topology_from.is_some() {
                // Prime the immutable endpoint before the UI and motion producer
                // take separate clones of the presentation.
                let layout = p.layout();
                if let Some(composition) = composition.filter(|_| keep_selected_visible) {
                    let target = p.compose(p.target, composition);
                    if !p
                        .cards_for_params(p.target, self.viewport, composition)
                        .iter()
                        .any(|c| c.selected)
                    {
                        let frame = gpu_gallery::wall_frame(self.viewport, target);
                        if let Some(card) = p
                            .positions
                            .get(&p.selected)
                            .and_then(|&i| layout.placements.get(i))
                        {
                            let center = (frame.clip[1] + frame.clip[3]) * 0.5 - frame.top;
                            let camera =
                                (card.y + card.height * 0.5 - center).clamp(0.0, layout.max_scroll);
                            p.camera = GalleryMotionState::source(camera, camera, duration, 1.0);
                        }
                    }
                }
            }
            changed
        } else {
            let mut p = WallPresentation::new(ids, selected, params, aspects, duration);
            p.morph_ms = morph_ms;
            p.engaged = self.gallery_engaged;
            p.keyboard = self.gallery_keyboard;
            self.wall_presentation = Some(p);
            self.restart_gallery_entrance();
            true
        }
    }
}

impl GpuCandidate {
    fn configured_wall_params(&self) -> WallParams {
        let mut params = gallery::gallery_layout_params(&self.state, 0.0, 0.0).wall;
        params.scroll_offset = 0.0;
        params.user_engaged = false;
        if let candidate_state::mode_profiles::Snapshot::Wall(profile) =
            view::settings::profile_projection::capture(self)
        {
            let s = profile.stage;
            params.stage = donor_layout::StageParams {
                offset_x: s.x,
                offset_y: s.y,
                scale: s.scale,
                rotation: s.rotation_degrees,
                perspective: s.perspective,
                shear_x: s.shear_x,
                shear_y: s.shear_y,
                depth_angle: s.depth_angle_degrees,
            };
        }
        params
    }

    fn wall_position_layout(&self) -> String {
        use std::hash::{Hash, Hasher};
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        // Source hashes the current scene.gp, so a mid-morph save falls back to its anchor.
        let params = self
            .wall_presentation
            .as_ref()
            .map_or_else(|| self.configured_wall_params(), |p| p.params);
        // The existing donor projection contains layout only: no camera, rail, or engagement.
        let (grid, _) = gpu_gallery::wall_placements(0, params, &[]);
        format!("{:?}", grid).hash(&mut hash);
        for &i in &self.filtered_indices {
            if let Some(w) = self.wallpapers.get(i) {
                w.id.hash(&mut hash);
            }
            if let Some(c) = self.catalog.get(i) {
                (c.width, c.height).hash(&mut hash);
            }
        }
        format!("{:016x}", hash.finish())
    }

    pub(super) fn restore_wall_start_position(&mut self) {
        if self.browse_position_restored || self.filtered_indices.is_empty() {
            return;
        }
        self.browse_position_restored = true;
        if self.gallery_mode != GalleryMode::Wall {
            self.restore_retained_start_position();
            return;
        }
        if self.gallery_engaged || self.screenshot_path.is_some() || self.scenario.is_some() {
            return;
        }
        // The donor scene starts at0; a missing record/key must not restore the applied item.
        self.selected = self.filtered_indices.first().copied();
        let source = view::settings::bridge::Source { app: self };
        let (id, camera) = match source
            .text("components.wallpaperSelector.startPosition")
            .as_str()
        {
            "applied" => (
                self.active_wallpaper_index()
                    .and_then(|i| self.wallpapers.get(i))
                    .map(|w| w.id.clone()),
                None,
            ),
            "browsing" => {
                let Some(p) = self
                    .state
                    .frontend_preferences
                    .picker
                    .browse_position
                    .as_ref()
                else {
                    return;
                };
                if p.layout == self.wall_position_layout() {
                    (Some(p.selection.clone()), Some(p.camera))
                } else {
                    (Some(p.anchor.clone()), None)
                }
            }
            _ => {
                self.selected = self.filtered_indices.first().copied();
                return;
            }
        };
        let Some(index) = self.filtered_indices.iter().position(|i| {
            self.wallpapers
                .get(*i)
                .is_some_and(|w| Some(&w.id) == id.as_ref())
        }) else {
            return;
        };
        self.selected = self.filtered_indices.get(index).copied();
        self.sync_wall_presentation();
        if let Some(p) = &mut self.wall_presentation {
            p.restore(index, camera);
            self.gallery_engaged = true;
        }
    }

    pub(super) fn save_wall_browse_position(&mut self) {
        if self.gallery_mode != GalleryMode::Wall {
            self.save_retained_browse_position();
            return;
        }
        if self.gallery_mode != GalleryMode::Wall
            || self.scenario.is_some()
            || self.screenshot_path.is_some()
            || (view::settings::bridge::Source { app: self })
                .text("components.wallpaperSelector.startPosition")
                != "browsing"
        {
            return;
        }
        let Some(p) = self.wall_presentation.as_ref() else {
            return;
        };
        let Some(selection) = self
            .selected
            .and_then(|i| self.wallpapers.get(i))
            .map(|w| w.id.clone())
        else {
            return;
        };
        let anchor = self
            .ordinary_frame()
            .iter()
            .min_by(|a, b| {
                let distance = |e: &donor_scene::CardPresentation| {
                    (e.card.center[0] - self.viewport[0] * 0.5)
                        .hypot(e.card.center[1] - self.viewport[1] * 0.5)
                };
                distance(a).total_cmp(&distance(b))
            })
            .map_or_else(|| selection.clone(), |e| e.id.clone());
        self.state.frontend_preferences.picker.browse_position =
            Some(candidate_state::preferences::BrowsePosition {
                selection,
                anchor,
                layout: self.wall_position_layout(),
                camera: p.camera.target,
            });
        self.persist_state();
    }
}

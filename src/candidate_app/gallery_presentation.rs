//! Ordinary-mode current presentation. Catalog, clock, worker and atlas stay app-owned.
use super::gallery::{
    poster_slot_matches_source, poster_source_for_item, prioritized_poster_indices,
};
use super::*;
use candidate_state::mode_profiles::{SlicesProfileSnapshot, Snapshot};
use donor_scene::{CardPresentation, CardReadiness, LaunchAnimation, SurfacePayload};
use gpu_gallery::SliceCard;
use view::settings::composed::SettingsSource;

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneMotion {
    visibility: GalleryMotionState,
    layout_reveal: GalleryMotionState,
    inset: GalleryMotionState,
    inset_ms: f32,
    pub generation: u64,
}

impl Default for SceneMotion {
    fn default() -> Self {
        Self {
            visibility: GalleryMotionState::absolute_camera(1.0, 1.0, 250.0, 100.0, 1.0),
            layout_reveal: GalleryMotionState::absolute_camera(1.0, 1.0, 250.0, 100.0, 1.0),
            inset: GalleryMotionState::source(0.0, 0.0, 250.0, 1.0),
            inset_ms: 250.0,
            generation: 0,
        }
    }
}

impl SceneMotion {
    pub fn retarget(&mut self, visible: bool, inset: f32, fade_ms: f32, inset_ms: f32) -> bool {
        let target = f32::from(visible);
        let fade_changed =
            self.visibility.target != target || (fade_ms <= 0.0 && !self.visibility.settled());
        if fade_changed {
            let from = if fade_ms <= 0.0 {
                target
            } else {
                self.visibility.value()
            };
            self.visibility =
                GalleryMotionState::absolute_camera(from, target, fade_ms, 100.0, 1.0);
        }
        let inset_changed =
            self.inset.target != inset || (self.inset_ms != inset_ms && !self.inset.settled());
        if inset_changed || self.inset_ms != inset_ms {
            if inset_ms <= 0.0 {
                self.inset = GalleryMotionState::source(inset, inset, inset_ms, 1.0);
            } else if self.inset_ms != inset_ms {
                self.inset = GalleryMotionState::source(self.inset.value(), inset, inset_ms, 1.0);
            } else {
                self.inset.retarget_source(inset);
            }
        }
        self.inset_ms = inset_ms;
        fade_changed || inset_changed
    }

    fn reveal_layout(&mut self, duration: f32) {
        self.layout_reveal = GalleryMotionState::absolute_camera(0.0, 1.0, duration, 100.0, 1.0);
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        let fading = self.visibility.tick(dt);
        let revealing = self.layout_reveal.tick(dt);
        self.inset.tick(dt) || fading || revealing
    }

    pub fn settle(&mut self) {
        self.retarget(self.visibility.target == 1.0, self.inset.target, 0.0, 0.0);
        self.layout_reveal = GalleryMotionState::absolute_camera(1.0, 1.0, 0.0, 100.0, 1.0);
    }

    pub fn unsettled(self) -> bool {
        !self.visibility.settled() || !self.inset.settled() || !self.layout_reveal.settled()
    }

    pub fn opacity(self) -> f32 {
        self.visibility.x.clamp(0.0, 1.0) * self.layout_reveal.x.clamp(0.0, 1.0)
    }

    pub fn inset(self, width: f32) -> f32 {
        self.inset.value().clamp(0.0, width * 0.6)
    }

    pub fn composition(self, width: f32, rtl: bool) -> [f32; 2] {
        let inset = self.inset(width);
        let available = width - inset;
        [available * 0.5 + if rtl { 0.0 } else { inset }, available]
    }
}

#[derive(Debug, Clone)]
pub(super) struct GalleryPresentation {
    pub ids: Arc<[String]>,
    pub selected: String,
    pub hover: Option<String>,
    pub camera: GalleryMotionState,
    pub widths: HashMap<String, GalleryMotionState>,
    pub selection: HashMap<String, GalleryMotionState>,
    pub slices: SlicesProfileSnapshot,
    rounding: f32,
    pub target: SlicesProfileSnapshot,
    pub viewport: [f32; 2],
    pub duration: f32,
    pub entrance: GalleryMotionState,
    pub launch: LaunchAnimation,
}

fn layout_width(width: f32, skew: f32) -> f32 {
    (width - skew.abs()).max(1.0)
}

impl GalleryPresentation {
    pub fn new(
        ids: Vec<String>,
        selected: String,
        mut p: SlicesProfileSnapshot,
        viewport: [f32; 2],
        duration: f32,
    ) -> Self {
        // beta.24 clamps the effective getter, preserving configured legacy values.
        p.wobble_strength = p.wobble_strength.clamp(0.0, 2.0);
        let position = ids.iter().position(|id| *id == selected).unwrap_or(0);
        let center = position as f32 * (layout_width(p.width, p.skew_distance) + p.spacing)
            + layout_width(p.expanded_width, p.skew_distance) * 0.5;
        Self {
            ids: ids.into(),
            selected,
            hover: None,
            camera: GalleryMotionState::source(
                center,
                center,
                duration,
                if p.wobble { 0.45 } else { 1.0 },
            ),
            widths: HashMap::new(),
            selection: HashMap::new(),
            slices: p.clone(),
            rounding: f32::from(p.round_corners),
            target: p,
            viewport,
            duration,
            entrance: GalleryMotionState::source(1.0, 1.0, 250.0, 1.0),
            launch: LaunchAnimation::Fade,
        }
    }

    pub fn sync(
        &mut self,
        ids: Vec<String>,
        selected: String,
        mut p: SlicesProfileSnapshot,
        viewport: [f32; 2],
        duration: f32,
    ) -> bool {
        p.wobble_strength = p.wobble_strength.clamp(0.0, 2.0);
        let geometry = self.target != p || self.viewport != viewport;
        let changed = geometry
            || self.ids.as_ref() != ids.as_slice()
            || self.selected != selected
            || self.duration != duration;
        if !changed {
            return false;
        }
        let anchor = self.ids.as_ref() != ids.as_slice();
        self.widths.retain(|id, _| ids.contains(id));
        self.selection.retain(|id, _| ids.contains(id));
        if self.selected != selected {
            for (id, was_selected) in [(self.selected.clone(), true), (selected.clone(), false)] {
                if !ids.contains(&id) {
                    continue;
                }
                self.widths.entry(id.clone()).or_insert_with(|| {
                    GalleryMotionState::source(
                        if was_selected {
                            self.slices.expanded_width
                        } else {
                            self.slices.width
                        },
                        if was_selected {
                            self.slices.expanded_width
                        } else {
                            self.slices.width
                        },
                        duration,
                        1.0,
                    )
                });
                self.selection.entry(id).or_insert_with(|| {
                    GalleryMotionState::source(
                        f32::from(was_selected),
                        f32::from(was_selected),
                        duration,
                        1.0,
                    )
                });
            }
        }
        if self.duration != duration {
            // Source retiming resets velocity; navigation retargeting does not.
            for spring in self
                .widths
                .values_mut()
                .chain(self.selection.values_mut())
                .chain(std::iter::once(&mut self.camera))
            {
                let target = spring.target;
                *spring = GalleryMotionState::source(spring.value(), target, duration, 1.0);
            }
        }
        if anchor {
            self.ids = ids.into();
        }
        self.selected = selected;
        self.target = p;
        self.viewport = viewport;
        self.duration = duration;
        self.retarget(anchor || geometry);
        true
    }

    fn retarget(&mut self, anchor: bool) {
        let p = &self.slices;
        for (id, spring) in &mut self.widths {
            spring.set_zeta(if p.wobble { 0.5 } else { 1.0 });
            spring.retarget_source(if *id == self.selected {
                p.expanded_width
            } else {
                p.width
            });
        }
        for (id, spring) in &mut self.selection {
            spring.retarget_source(f32::from(*id == self.selected));
        }
        let position = self
            .ids
            .iter()
            .position(|id| *id == self.selected)
            .unwrap_or(0);
        let target = position as f32 * (layout_width(p.width, p.skew_distance) + p.spacing)
            + layout_width(p.expanded_width, p.skew_distance) * 0.5;
        self.camera.set_zeta(if p.wobble { 0.45 } else { 1.0 });
        if anchor {
            self.camera = GalleryMotionState::source(
                target,
                target,
                self.duration,
                if p.wobble { 0.45 } else { 1.0 },
            );
        } else {
            self.camera.retarget_source(target);
        }
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        let morphing = morph_slices(&mut self.slices, &self.target, dt, self.duration);
        self.retarget(morphing);
        let mut moving = self.camera.tick(dt) || morphing;
        let rounding = f32::from(self.target.round_corners);
        if (self.rounding - rounding).abs() > 1e-5 {
            self.rounding = crate::donor_folio::source_ui::motion::approach(
                self.rounding,
                rounding,
                dt,
                self.duration / 4000.0,
            );
            moving = true;
        } else {
            self.rounding = rounding;
        }
        moving |= self.entrance.tick(dt);
        for spring in self.widths.values_mut().chain(self.selection.values_mut()) {
            moving |= spring.tick(dt);
        }
        let current = &self.selected;
        let narrow = self.slices.width;
        self.widths
            .retain(|id, s| *id == *current || !s.settled() || s.target != narrow);
        self.selection.retain(|_, s| !s.settled());
        moving
    }

    pub fn settle(&mut self) {
        self.slices = self.target.clone();
        self.rounding = f32::from(self.target.round_corners);
        self.widths.clear();
        self.selection.clear();
        self.retarget(true);
        self.entrance = GalleryMotionState::source(1.0, 1.0, 250.0, 1.0);
    }

    fn slice_cards(
        &self,
        [center, available]: [f32; 2],
    ) -> Vec<(SliceCard, f32, bool, [f32; 4], SurfacePayload)> {
        let p = &self.slices;
        let stride = layout_width(p.width, p.skew_distance) + p.spacing;
        let strip = (layout_width(p.expanded_width, p.skew_distance)
            + p.visible_count.saturating_sub(1) as f32 * stride)
            .min(available - 40.0)
            .max(1.0);
        let half = strip * 0.5;
        let cx = center + self.viewport[0] * p.offset_x * 0.5;
        // Source TOP_BAR=50, 60px chrome and 15px inset place the body 10px below center.
        let cy = self.viewport[1] * (0.5 + p.offset_y * 0.5) + 10.0;
        let mut acc = 0.0;
        let mut cards = Vec::new();
        for (index, id) in self.ids.iter().enumerate() {
            let selected = *id == self.selected;
            let width = self
                .widths
                .get(id)
                .map_or(if selected { p.expanded_width } else { p.width }, |s| {
                    s.value()
                })
                .max(1.0);
            let layout = layout_width(width, p.skew_distance);
            let x = cx - self.camera.value() + acc + layout * 0.5;
            acc += layout + p.spacing;
            let opacity = crate::upstream_slices_adapter::slice_opacity(
                x,
                cx,
                half,
                layout_width(p.expanded_width, p.skew_distance),
                stride,
            );
            if opacity <= 0.01 {
                continue;
            }
            let corners = donor_layout::slice_clamped_corners(
                p.corners,
                width,
                p.height,
                p.skew_distance,
                p.edge_tilt_distance,
            )
            .map(|radius| radius * self.rounding);
            let card = SliceCard {
                triangle_direction: 0,
                index,
                center: [x, cy],
                size: [width, p.height],
                layer: None,
                selected,
                opacity,
                skew_distance: p.skew_distance,
                edge_tilt_distance: p.edge_tilt_distance,
                rotation: 0.0,
                corner_radius: corners[0],
                shape: 0,
                poster_aspect_ratio: 16.0 / 9.0,
            };
            let selection = self
                .selection
                .get(id)
                .map_or(f32::from(selected), |s| s.value())
                .clamp(0.0, 1.0);
            let payload = if p.wobble {
                SurfacePayload::Wobble {
                    pad: 1.05 + 0.30 * p.wobble_strength.max(1.0),
                    bend: (self.camera.velocity / (p.expanded_width * 2.2).max(1.0))
                        .clamp(-1.35, 1.35)
                        * p.wobble_strength,
                    parallax: ((x - cx) / half.max(1.0)).clamp(-1.0, 1.0) * 0.5,
                }
            } else {
                SurfacePayload::Plain
            };
            cards.push((
                card,
                selection,
                self.hover.as_ref() == Some(id),
                corners,
                payload,
            ));
        }
        let current = self
            .ids
            .iter()
            .position(|id| *id == self.selected)
            .unwrap_or(0);
        cards.sort_by_key(|(card, _, hover, _, _)| {
            if card.selected {
                i64::MAX
            } else if *hover {
                i64::MAX - 1
            } else {
                -(card.index.abs_diff(current) as i64)
            }
        });
        cards
    }
}

fn morph_slices(
    current: &mut SlicesProfileSnapshot,
    target: &SlicesProfileSnapshot,
    dt: f32,
    duration: f32,
) -> bool {
    let k = 1.0 - (-dt.min(0.05) * 1000.0 / (duration / 4.0).max(1.0)).exp();
    let mut moving = false;
    for (v, t) in [
        (&mut current.offset_x, target.offset_x),
        (&mut current.offset_y, target.offset_y),
        (&mut current.width, target.width),
        (&mut current.expanded_width, target.expanded_width),
        (&mut current.height, target.height),
        (&mut current.spacing, target.spacing),
        (&mut current.skew_distance, target.skew_distance),
        (&mut current.edge_tilt_distance, target.edge_tilt_distance),
        (&mut current.wobble_strength, target.wobble_strength),
    ] {
        if (*v - t).abs() > 0.01 {
            *v += (t - *v) * k;
            moving = true;
        } else {
            *v = t;
        }
    }
    for (v, t) in current.corners.iter_mut().zip(target.corners) {
        if (*v - t).abs() > 0.01 {
            *v += (t - *v) * k;
            moving = true;
        } else {
            *v = t;
        }
    }
    current.visible_count = target.visible_count;
    current.wobble = target.wobble;
    current.shadows = target.shadows;
    current.parallax = target.parallax;
    current.round_corners = target.round_corners;
    moving
}

pub(super) fn launch_entrance(
    launch: LaunchAnimation,
    screenshot: bool,
    from: f32,
    duration: f32,
) -> GalleryMotionState {
    let start = if matches!(launch, LaunchAnimation::None) || screenshot || duration <= 0.0 {
        1.0
    } else {
        from
    };
    GalleryMotionState::source(start, 1.0, duration, 1.0)
}

impl GpuCandidate {
    pub(super) fn sync_gallery_presentation(&mut self) -> bool {
        if self.gallery_mode == GalleryMode::Wall {
            return self.sync_wall_presentation();
        }
        if self.gallery_mode == GalleryMode::Geometric {
            return self.sync_hex_presentation();
        }
        if self.gallery_mode != GalleryMode::Slices {
            return false;
        }
        let Snapshot::Slices(p) = view::settings::profile_projection::capture(self) else {
            return false;
        };
        let ids: Vec<_> = self
            .filtered_indices
            .iter()
            .filter_map(|i| self.wallpapers.get(*i).map(|w| w.id.clone()))
            .collect();
        let selected = self
            .selected
            .and_then(|i| self.wallpapers.get(i))
            .map_or_else(
                || ids.first().cloned().unwrap_or_default(),
                |w| w.id.clone(),
            );
        let duration = ((view::settings::bridge::Source { app: self }).number("motion.standardMs")
            as f32)
            .clamp(35.0, 2000.0);
        if let Some(state) = &mut self.gallery_presentation {
            state.sync(ids, selected, p, self.viewport, duration)
        } else {
            self.gallery_presentation = Some(GalleryPresentation::new(
                ids,
                selected,
                p,
                self.viewport,
                duration,
            ));
            self.restart_gallery_entrance();
            true
        }
    }

    fn launch_motion_ms(&self) -> f32 {
        let source = view::settings::bridge::Source { app: self };
        match source.text("motion.launchSpeed").as_str() {
            "fast" => source.motion_fast_ms(),
            "slow" => source.motion_slow_ms(),
            _ => source.motion_standard_ms(),
        }
    }

    pub(super) fn gallery_composition(&self) -> [f32; 2] {
        self.scene_motion
            .composition(self.viewport[0], self.settings_locale.language.rtl)
    }

    pub(super) fn reveal_gallery_layout(&mut self) {
        if self.visible
            && self.screenshot_path.is_none()
            && self.panel_state.settings_open
            && self.settings_workbench.layout_studio()
        {
            // The studio uses the layout reveal even when launch animation is disabled.
            let duration = view::settings::bridge::Source { app: self }.motion_standard_ms();
            if duration > 0.0 {
                self.scene_motion.reveal_layout(duration);
                self.start_motion(0.0);
            }
        }
    }

    pub(super) fn sync_scene_motion(&mut self) {
        let studio = self.panel_state.settings_open && self.settings_workbench.layout_studio();
        let playlist = self.playlist_panel_open && !self.playlist_picker;
        let visible = !playlist && (!self.panel_state.settings_open || studio);
        let source = view::settings::bridge::Source { app: self };
        let scale = view::settings::bridge::effective_ui_scale(self);
        let inset = if studio && !playlist && self.gallery_mode != GalleryMode::Wall {
            crate::donor_folio::source_ui::picker_layout_studio_width(self.viewport[0], scale)
                + 36.0 * scale
        } else {
            0.0
        };
        let (fade_ms, inset_ms) = if self.visible && self.screenshot_path.is_none() {
            (self.launch_motion_ms(), source.motion_standard_ms())
        } else {
            (0.0, 0.0)
        };
        let changed = self
            .scene_motion
            .retarget(visible, inset, fade_ms, inset_ms);
        let interrupted = self.scene_motion.unsettled()
            && self.scene_motion.generation != self.motion_generation.load(Ordering::Acquire);
        if self.visible && (changed || interrupted) {
            self.start_motion(0.0);
            self.rebuild_depth_frame();
        }
    }

    pub(super) fn gallery_entrance_opacity(&self) -> f32 {
        match self.gallery_mode {
            GalleryMode::Slices => self.gallery_presentation.as_ref().map(|p| p.entrance),
            GalleryMode::Geometric => self.hex_presentation.as_ref().map(|p| p.entrance),
            GalleryMode::Wall => self.wall_presentation.as_ref().map(|p| p.entrance),
            GalleryMode::Depth => self.depth_presentation.map(|p| p.entrance),
        }
        .map_or(1.0, |entrance| entrance.value().clamp(0.0, 1.0))
    }

    pub(super) fn restart_gallery_entrance(&mut self) {
        let source = view::settings::bridge::Source { app: self };
        let kind = source.launch_animation();
        let duration = self.launch_motion_ms();
        let from = (source.number("general.openFadeFrom") as f32 / 100.0).clamp(0.0, 1.0);
        let launch = match kind.as_str() {
            "none" => LaunchAnimation::None,
            "rise" => LaunchAnimation::Rise,
            "zoom" => LaunchAnimation::Zoom,
            _ => LaunchAnimation::Fade,
        };
        let entrance = launch_entrance(launch, self.screenshot_path.is_some(), from, duration);
        if let Some(p) = &mut self.gallery_presentation {
            p.launch = launch;
            p.entrance = entrance;
        }
        if let Some(p) = &mut self.hex_presentation {
            p.launch = launch;
            p.entrance = entrance;
        }
        if self.gallery_mode == GalleryMode::Wall {
            if let Some(p) = &mut self.wall_presentation {
                p.launch = launch;
                p.entrance = entrance;
            }
        }
        if self.gallery_mode == GalleryMode::Depth {
            if self.depth_presentation.is_none() {
                self.depth_presentation = Some(DepthPresentation::target(&self.state));
            }
            if let Some(p) = &mut self.depth_presentation {
                p.launch = launch;
                p.entrance = entrance;
            }
        }
    }

    /// One current frame supplies the snapshot and the poster requester.
    pub(super) fn ordinary_base_frame(&self) -> Vec<CardPresentation> {
        self.ordinary_base_frame_with_source_selector(poster_source_for_item)
    }

    pub(super) fn ordinary_base_frame_with_source_selector(
        &self,
        mut select_source: impl FnMut(&WallpaperItem) -> std::path::PathBuf,
    ) -> Vec<CardPresentation> {
        let slices = self.gallery_mode == GalleryMode::Slices;
        let fallback = if slices && self.gallery_presentation.is_none() {
            let Snapshot::Slices(p) = view::settings::profile_projection::capture(self) else {
                unreachable!()
            };
            let ids: Vec<_> = self
                .filtered_indices
                .iter()
                .filter_map(|i| self.wallpapers.get(*i).map(|w| w.id.clone()))
                .collect();
            let selected = self
                .selected
                .and_then(|i| self.wallpapers.get(i))
                .map_or_else(
                    || ids.first().cloned().unwrap_or_default(),
                    |w| w.id.clone(),
                );
            Some(GalleryPresentation::new(
                ids,
                selected,
                p,
                self.viewport,
                ((view::settings::bridge::Source { app: self }).number("motion.standardMs") as f32)
                    .clamp(35.0, 2000.0),
            ))
        } else {
            None
        };
        let slice_state = self.gallery_presentation.as_ref().or(fallback.as_ref());
        let mut entries = if slices {
            slice_state.map_or_else(Vec::new, |p| p.slice_cards(self.gallery_composition()))
        } else if self.gallery_mode == GalleryMode::Geometric && self.hex_presentation.is_some() {
            self.hex_presentation.as_ref().unwrap().cards()
        } else if self.gallery_mode == GalleryMode::Wall && self.wall_presentation.is_some() {
            self.wall_presentation
                .as_ref()
                .unwrap()
                .cards(self.viewport, gallery_layout_params_for_app(self).wall)
        } else {
            gpu_gallery::layout_gallery_with_aspects(
                self.gallery_mode,
                self.filtered_indices.len(),
                view_legacy_selection(self),
                self.viewport,
                gallery_layout_params_for_app(self),
                &filtered_poster_aspects(&self.catalog, &self.filtered_indices),
            )
            .into_iter()
            .map(|c| {
                (
                    c,
                    f32::from(c.selected),
                    false,
                    [c.corner_radius; 4],
                    SurfacePayload::Plain,
                )
            })
            .collect()
        };
        let mut result = Vec::with_capacity(entries.len());
        for (mut card, selection, hovered, mut radii, mut payload) in entries.drain(..) {
            let Some(index) = self.filtered_indices.get(card.index).copied() else {
                continue;
            };
            let Some(wallpaper) = self.wallpapers.get(index) else {
                continue;
            };
            card.index = index;
            if self.gallery_mode == GalleryMode::Wall {
                if let Some(p) = &self.wall_presentation {
                    let k = donor_scene::launch_card(
                        &mut card,
                        p.entrance.value(),
                        p.launch,
                        self.viewport,
                    );
                    radii = radii.map(|v| v * k);
                }
            }
            let source = select_source(wallpaper);
            let poster = self
                .poster_slots
                .get(&index)
                .filter(|s| poster_slot_matches_source(s, wallpaper, &source))
                .and_then(|s| {
                    self.posters
                        .iter()
                        .find(|p| p.layer == s.layer && p.source == s.source)
                });
            card.layer = poster.map(|p| p.layer);
            card.poster_aspect_ratio = poster.map_or_else(
                || {
                    self.catalog
                        .get(index)
                        .map_or(16.0 / 9.0, |c| c.width as f32 / (c.height.max(1) as f32))
                },
                |p| p.aspect_ratio,
            );
            let mut crop = donor_scene::poster_crop_uv(card.poster_aspect_ratio, card.size);
            if slices {
                let p = &slice_state.unwrap().slices;
                if p.parallax {
                    let [center, available] = self.gallery_composition();
                    let cx = center + self.viewport[0] * p.offset_x * 0.5;
                    let half = (layout_width(p.expanded_width, p.skew_distance)
                        + p.visible_count.saturating_sub(1) as f32
                            * (layout_width(p.width, p.skew_distance) + p.spacing))
                        .min(available - 40.0)
                        .max(1.0)
                        * 0.5;
                    crop = position_crop(
                        card.poster_aspect_ratio,
                        crop,
                        (card.center[0] - cx) / (half * 1.2).max(1.0),
                        1.4,
                    );
                }
            }
            if self.gallery_mode == GalleryMode::Geometric {
                if let Some(p) = &self.hex_presentation {
                    if p.parallax {
                        let center = p.center_x();
                        let span = if card.center[0] < center {
                            center
                        } else {
                            self.viewport[0] - center
                        };
                        crop = position_crop(
                            card.poster_aspect_ratio,
                            crop,
                            (card.center[0] - center) / (span + card.size[0] * 0.5).max(1.0),
                            1.4,
                        );
                    } else {
                        crop = position_crop(card.poster_aspect_ratio, crop, 0.0, 1.3);
                    }
                }
            }
            if card.selected && (self.detail_open || self.detail_progress > 0.001) {
                payload = SurfacePayload::Detail;
            }
            let queued = self.poster_request_key.as_ref().is_some_and(|r| {
                r.iter()
                    .any(|r| r.index == index && r.item_id == wallpaper.id && r.source == source)
            });
            let readiness = if card.layer.is_some() {
                CardReadiness::Ready
            } else if queued && self.poster_loading {
                CardReadiness::Loading
            } else {
                CardReadiness::Unavailable
            };
            result.push(CardPresentation {
                card,
                outgoing: false,
                roll: 1.0,
                captured_raw: None,
                retained: None,
                id: wallpaper.id.clone(),
                source,
                catalog_generation: self.catalog_generation.load(Ordering::Acquire),
                selection,
                hovered,
                radii,
                crop,
                payload,
                readiness,
                source_style: slices,
                hex_style: self.gallery_mode == GalleryMode::Geometric
                    && self.hex_presentation.is_some(),
                wall_border: (self.gallery_mode == GalleryMode::Wall).then_some((
                    selection,
                    gallery_layout_params_for_app(self).wall.border_width,
                )),
                shadows: (slices && slice_state.is_some_and(|p| p.slices.shadows))
                    || (self.gallery_mode == GalleryMode::Geometric && selection > 0.01),
                loading_eligible: self.gallery_mode == GalleryMode::Wall
                    && wallpaper.source != WallpaperSource::LocalVideo,
                entrance: if self.gallery_mode == GalleryMode::Geometric {
                    self.hex_presentation
                        .as_ref()
                        .map_or(1.0, |p| p.entrance.value())
                } else if slices {
                    slice_state.map_or(1.0, |p| p.entrance.value())
                } else {
                    self.wall_presentation
                        .as_ref()
                        .map_or(1.0, |p| p.entrance.value())
                },
                launch: if self.gallery_mode == GalleryMode::Geometric {
                    self.hex_presentation
                        .as_ref()
                        .map_or(LaunchAnimation::None, |p| p.launch)
                } else if slices {
                    slice_state.map_or(LaunchAnimation::Fade, |p| p.launch)
                } else {
                    self.wall_presentation
                        .as_ref()
                        .map_or(LaunchAnimation::None, |p| p.launch)
                },
            });
        }
        result
    }

    pub(super) fn ordinary_poster_indices(&self) -> Vec<usize> {
        let mut indices = Vec::new();
        let limit = gpu_gallery::ordinary_poster_limit();
        if let Some(card) = self.external_detail_card() {
            indices.push(card.index);
        }
        for entry in self.ordinary_base_frame().iter().rev() {
            if indices.len() >= limit {
                break;
            }
            if !indices.contains(&entry.card.index) {
                indices.push(entry.card.index);
            }
        }
        for index in prioritized_poster_indices(&self.filtered_indices, self.selected.unwrap_or(0))
        {
            let union: HashSet<_> = indices
                .iter()
                .copied()
                .chain(self.gallery_pins().iter().map(|g| g.index))
                .collect();
            if !union.contains(&index) && union.len() >= limit {
                break;
            }
            gallery::push_unique_poster_index(&mut indices, index);
        }
        indices
    }

    pub(super) fn ordinary_capacity_issue(&self) -> Option<String> {
        if self.gallery_mode == GalleryMode::Depth {
            return None;
        }
        self.ordinary_capacity_issue_for_frame(&self.ordinary_base_frame())
    }

    pub(super) fn ordinary_capacity_issue_for_frame(
        &self,
        frame: &[CardPresentation],
    ) -> Option<String> {
        if !matches!(
            self.gallery_mode,
            GalleryMode::Slices | GalleryMode::Geometric | GalleryMode::Wall
        ) {
            return None;
        }
        let mut required: HashSet<_> = frame
            .iter()
            .filter(|e| !e.outgoing)
            .map(|e| e.card.index)
            .collect();
        if let Some(card) = self.external_detail_card() {
            required.insert(card.index);
        }
        let required = required.len();
        let limit = gpu_gallery::ordinary_poster_limit();
        (required>limit).then(||format!("This layout requires {required} preview images; the current device/memory limit is {limit}. Some images are unavailable. Increase card size or reduce visible cards."))
    }
}

/// Position operates within decoded content, never within square letterboxing.
pub(super) fn position_crop(aspect: f32, crop: [f32; 4], position: f32, zoom: f32) -> [f32; 4] {
    let aspect = if aspect.is_finite() && aspect > 0.0 {
        aspect
    } else {
        16.0 / 9.0
    };
    let content = if aspect >= 1.0 {
        [0.0, (1.0 - 1.0 / aspect) * 0.5, 1.0, 1.0 / aspect]
    } else {
        [(1.0 - aspect) * 0.5, 0.0, aspect, 1.0]
    };
    let w = crop[2] / zoom;
    let h = crop[3] / zoom;
    [
        content[0] + (content[2] - w) * (0.5 + position.clamp(-1.0, 1.0) * 0.5),
        content[1] + (content[3] - h) * 0.5,
        w,
        h,
    ]
}

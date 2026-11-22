//! Ordinary-mode current presentation. Catalog, clock, worker and atlas stay app-owned.
use super::gallery::{poster_media_for_item, poster_slot_matches_source};
use super::*;
use candidate_state::mode_profiles::{SlicesProfileSnapshot, Snapshot};
use donor_scene::{CardPresentation, CardReadiness, LaunchAnimation, SurfacePayload};
use gpu_gallery::SliceCard;
use view::settings::composed::SettingsSource;

/// Rebuilt by the existing catalog/filter owners, never by a motion tick.
#[derive(Debug, Clone)]
pub(super) struct PresentationInputs {
    revision: u64,
    pub ids: Arc<[String]>,
    pub aspects: Arc<[f32]>,
    positions: Arc<HashMap<usize, usize>>,
}

fn id_positions(ids: &[String]) -> Arc<HashMap<String, usize>> {
    Arc::new(
        ids.iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect(),
    )
}

#[derive(Debug, Clone, Copy)]
pub(super) struct SceneMotion {
    visibility: GalleryMotionState,
    layout_reveal: GalleryMotionState,
    inset: GalleryMotionState,
    inset_ms: f32,
    settings_opacity: GalleryMotionState,
    playlist_opacity: GalleryMotionState,
    pub generation: u64,
}

impl Default for SceneMotion {
    fn default() -> Self {
        Self {
            visibility: GalleryMotionState::absolute_camera(1.0, 1.0, 250.0, 100.0, 1.0),
            layout_reveal: GalleryMotionState::absolute_camera(1.0, 1.0, 250.0, 100.0, 1.0),
            inset: GalleryMotionState::source(0.0, 0.0, 250.0, 1.0),
            inset_ms: 250.0,
            settings_opacity: GalleryMotionState::absolute_camera(0.0, 0.0, 0.0, 100.0, 1.0),
            playlist_opacity: GalleryMotionState::absolute_camera(0.0, 0.0, 0.0, 100.0, 1.0),
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

    fn retarget_overlays(&mut self, settings: bool, playlist: bool, duration: f32) -> bool {
        let mut changed = false;
        for (motion, open) in [
            (&mut self.settings_opacity, settings),
            (&mut self.playlist_opacity, playlist),
        ] {
            let target = f32::from(open);
            if motion.target != target || (duration <= 0.0 && !motion.settled()) {
                let current = motion.value().clamp(0.0, 1.0);
                let from = if duration <= 0.0 || (open && current <= 0.0) {
                    target
                } else {
                    current
                };
                *motion = GalleryMotionState::absolute_camera(from, target, duration, 100.0, 1.0);
                changed = true;
            }
        }
        changed
    }

    pub(super) fn settings_opacity(self) -> f32 {
        self.settings_opacity.value().clamp(0.0, 1.0)
    }

    pub(super) fn playlist_opacity(self) -> f32 {
        self.playlist_opacity.value().clamp(0.0, 1.0)
    }

    pub fn tick(&mut self, dt: f32) -> bool {
        let fading = self.visibility.tick(dt);
        let revealing = self.layout_reveal.tick(dt);
        let settings = self.settings_opacity.tick(dt);
        let playlist = self.playlist_opacity.tick(dt);
        self.inset.tick(dt) || fading || revealing || settings || playlist
    }

    pub fn settle(&mut self) {
        self.retarget(self.visibility.target == 1.0, self.inset.target, 0.0, 0.0);
        self.retarget_overlays(
            self.settings_opacity.target == 1.0,
            self.playlist_opacity.target == 1.0,
            0.0,
        );
        self.layout_reveal = GalleryMotionState::absolute_camera(1.0, 1.0, 0.0, 100.0, 1.0);
    }

    pub fn unsettled(self) -> bool {
        !self.visibility.settled()
            || !self.inset.settled()
            || !self.layout_reveal.settled()
            || !self.settings_opacity.settled()
            || !self.playlist_opacity.settled()
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
    positions: Arc<HashMap<String, usize>>,
    layout: std::cell::RefCell<Option<Arc<SliceLayoutWindow>>>,
    pub selected: String,
    pub hover: Option<String>,
    pub camera: GalleryMotionState,
    pub widths: HashMap<String, GalleryMotionState>,
    pub selection: HashMap<String, GalleryMotionState>,
    pub slices: SlicesProfileSnapshot,
    visible_count: f32,
    style_from: Option<(bool, bool, bool, u64)>,
    parallax: GalleryMotionState,
    style_progress: f32,
    rounding: f32,
    pub target: SlicesProfileSnapshot,
    pub viewport: [f32; 2],
    pub duration: f32,
    pub entrance: GalleryMotionState,
    pub launch: LaunchAnimation,
}

#[derive(Debug)]
struct SliceLayoutWindow {
    narrow: f32,
    spacing: f32,
    starts: Vec<f32>,
}

fn layout_width(width: f32, skew: f32) -> f32 {
    (width - skew.abs()).max(1.0)
}

impl GalleryPresentation {
    pub fn new(
        ids: impl Into<Arc<[String]>>,
        selected: String,
        mut p: SlicesProfileSnapshot,
        viewport: [f32; 2],
        duration: f32,
    ) -> Self {
        let ids = ids.into();
        // beta.24 clamps the effective getter, preserving configured legacy values.
        p.wobble_strength = p.wobble_strength.clamp(0.0, 2.0);
        let position = ids.iter().position(|id| *id == selected).unwrap_or(0);
        let center = position as f32 * (layout_width(p.width, p.skew_distance) + p.spacing)
            + layout_width(p.expanded_width, p.skew_distance) * 0.5;
        Self {
            layout: Default::default(),
            positions: id_positions(&ids),
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
            visible_count: p.visible_count.max(1) as f32,
            style_from: None,
            parallax: parallax_motion(f32::from(p.parallax), p.parallax, duration),
            style_progress: 1.0,
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
        ids: impl Into<Arc<[String]>>,
        selected: String,
        mut p: SlicesProfileSnapshot,
        viewport: [f32; 2],
        duration: f32,
    ) -> bool {
        let ids = ids.into();
        p.wobble_strength = p.wobble_strength.clamp(0.0, 2.0);
        let anchor = !Arc::ptr_eq(&self.ids, &ids) && self.ids != ids;
        let mut current_geometry = self.target.clone();
        current_geometry.parallax = p.parallax;
        let geometry = current_geometry != p || self.viewport != viewport;
        let target_style = (p.wobble, p.shadows, p.round_corners, p.visible_count.max(1));
        let current_style = self.style_values();
        let style_changed = current_style != target_style;
        let changed = geometry
            || anchor
            || self.selected != selected
            || self.duration != duration
            || self.parallax.target != f32::from(p.parallax);
        if !changed {
            self.ids = ids;
            return false;
        }
        if anchor {
            self.positions = id_positions(&ids);
            *self.layout.borrow_mut() = None;
            self.widths.retain(|id, _| self.positions.contains_key(id));
            self.selection
                .retain(|id, _| self.positions.contains_key(id));
        }
        if self.selected != selected {
            for (id, was_selected) in [(self.selected.clone(), true), (selected.clone(), false)] {
                if !self.positions.contains_key(&id) {
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
            self.ids = ids;
        }
        self.selected = selected;
        if self.parallax.target != f32::from(p.parallax) || self.duration != duration {
            self.parallax = parallax_motion(self.parallax.value(), p.parallax, duration);
        }
        self.target = p;
        self.viewport = viewport;
        self.duration = duration;
        if style_changed {
            self.style_from = Some(current_style);
            self.style_progress = 0.0;
        }
        self.retarget(anchor || geometry);
        true
    }

    fn style_values(&self) -> (bool, bool, bool, u64) {
        let target = (
            self.target.wobble,
            self.target.shadows,
            self.target.round_corners,
            self.target.visible_count.max(1),
        );
        if let Some(from) = self.style_from {
            if self.style_progress < 0.5 {
                return (from.0, from.1, from.2, self.visible_count.round() as u64);
            }
            return target;
        }
        (
            self.slices.wobble,
            self.slices.shadows,
            self.slices.round_corners,
            self.visible_count.round() as u64,
        )
    }

    fn render_wobble(&self) -> bool {
        self.style_values().0
    }

    pub(super) fn render_shadows(&self) -> bool {
        self.style_values().1
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
        let position = self.positions.get(&self.selected).copied().unwrap_or(0);
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
        let morphing = morph_slices(
            &mut self.slices,
            &mut self.visible_count,
            &self.target,
            dt,
            self.duration,
        );
        let style_moving = if self.style_from.is_some() {
            let k = 1.0 - (-dt.min(0.05) * 1000.0 / (self.duration / 4.0).max(1.0)).exp();
            self.style_progress += (1.0 - self.style_progress) * k;
            if self.style_progress >= 0.995 {
                self.style_progress = 1.0;
                self.slices.wobble = self.target.wobble;
                self.slices.shadows = self.target.shadows;
                self.slices.round_corners = self.target.round_corners;
                self.visible_count = self.target.visible_count.max(1) as f32;
                self.style_from = None;
                false
            } else {
                true
            }
        } else {
            false
        };
        self.retarget(morphing);
        let mut moving = self.camera.tick(dt) || morphing || style_moving;
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
        moving |= self.entrance.tick(dt) | self.parallax.tick(dt);
        if self.parallax.settled() {
            self.slices.parallax = self.target.parallax;
        }
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
        self.parallax = parallax_motion(
            f32::from(self.target.parallax),
            self.target.parallax,
            self.duration,
        );
        self.visible_count = self.target.visible_count.max(1) as f32;
        self.style_from = None;
        self.style_progress = 1.0;
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
        let wobble = self.render_wobble();
        let visible_count = self.visible_count.max(1.0);
        let stride = layout_width(p.width, p.skew_distance) + p.spacing;
        let strip = (layout_width(p.expanded_width, p.skew_distance)
            + (visible_count - 1.0).max(0.0) * stride)
            .min(available - 40.0)
            .max(1.0);
        let half = strip * 0.5;
        let cx = center + self.viewport[0] * p.offset_x * 0.5;
        // Source TOP_BAR=50, 60px chrome and 15px inset place the body 10px below center.
        let cy = self.viewport[1] * (0.5 + p.offset_y * 0.5) + 10.0;
        let current = self.positions.get(&self.selected).copied().unwrap_or(0);
        // All but the sparse selection springs share one stride. Keep dynamic
        // widths separate from the immutable ID index and visit only the window.
        let narrow = layout_width(p.width.max(1.0), p.skew_distance);
        let mut layout = self.layout.borrow_mut();
        if layout.as_ref().is_none_or(|c| {
            c.narrow != narrow || c.spacing != p.spacing || c.starts.len() != self.ids.len()
        }) {
            // Preserve the source's accumulated f32 strip positions once, rather
            // than introducing an index*stride drift on large fractional layouts.
            let mut acc = 0.0;
            let starts = (0..self.ids.len())
                .map(|_| {
                    let x = acc;
                    acc += narrow + p.spacing;
                    x
                })
                .collect();
            donor_layout::layout_work(1, self.ids.len());
            *layout = Some(Arc::new(SliceLayoutWindow {
                narrow,
                spacing: p.spacing,
                starts,
            }));
        }
        let layout = layout.as_ref().unwrap();
        let mut changes: Vec<_> = self
            .widths
            .iter()
            .filter_map(|(id, spring)| {
                self.positions.get(id).map(|&i| {
                    (
                        i,
                        layout_width(spring.value().max(1.0), p.skew_distance) - narrow,
                    )
                })
            })
            .collect();
        if !self.widths.contains_key(&self.selected) && self.positions.contains_key(&self.selected)
        {
            changes.push((
                current,
                layout_width(p.expanded_width.max(1.0), p.skew_distance) - narrow,
            ));
        }
        changes.sort_by_key(|c| c.0);
        let padding = changes.iter().map(|c| c.1.abs()).sum::<f32>() + narrow + 1.0;
        let window = donor_layout::monotone_window(
            self.ids.len(),
            |i| layout.starts[i],
            self.camera.value() - half * 1.2 - padding,
            self.camera.value() + half * 1.2 + padding,
        );
        let mut acc = layout.starts.get(window.start).copied().unwrap_or(0.0)
            + changes
                .iter()
                .filter(|c| c.0 < window.start)
                .map(|c| c.1)
                .sum::<f32>();
        donor_layout::layout_work(2, window.len());
        let mut cards = Vec::new();
        for index in window {
            let id = &self.ids[index];
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
            let payload = if wobble {
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
    visible_count: &mut f32,
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
    if (*visible_count - target.visible_count.max(1) as f32).abs() > 0.01 {
        *visible_count += (target.visible_count.max(1) as f32 - *visible_count) * k;
        moving = true;
    } else {
        *visible_count = target.visible_count.max(1) as f32;
    }
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
    pub(super) fn presentation_inputs(&mut self) -> PresentationInputs {
        let revision = self.catalog_generation.load(Ordering::Acquire);
        if self
            .presentation_inputs
            .as_ref()
            .is_none_or(|p| p.revision != revision)
        {
            donor_layout::layout_work(0, self.filtered_indices.len());
            self.presentation_inputs = Some(PresentationInputs {
                revision,
                ids: self
                    .filtered_indices
                    .iter()
                    .filter_map(|i| self.wallpapers.get(*i).map(|w| w.id.clone()))
                    .collect(),
                aspects: filtered_poster_aspects(&self.catalog, &self.filtered_indices).into(),
                positions: Arc::new(
                    self.filtered_indices
                        .iter()
                        .enumerate()
                        .map(|(p, &i)| (i, p))
                        .collect(),
                ),
            });
        }
        self.presentation_inputs.as_ref().unwrap().clone()
    }

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
        let ids = self.presentation_inputs().ids;
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
        let overlay_changed = self.scene_motion.retarget_overlays(
            self.visible && self.panel_state.settings_open,
            self.visible && self.playlist_panel_open,
            inset_ms,
        );
        let changed = self
            .scene_motion
            .retarget(visible, inset, fade_ms, inset_ms);
        let interrupted = self.scene_motion.unsettled()
            && self.scene_motion.generation != self.motion_generation.load(Ordering::Acquire);
        if self.visible && (changed || overlay_changed || interrupted) {
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
        self.ordinary_base_frame_with_source_selector(poster_media_for_item)
    }

    pub(super) fn ordinary_base_frame_with_source_selector(
        &self,
        mut select_source: impl FnMut(&WallpaperItem) -> (std::path::PathBuf, poster_worker::PosterKind),
    ) -> Vec<CardPresentation> {
        let slices = self.gallery_mode == GalleryMode::Slices;
        let wall_params = (self.gallery_mode == GalleryMode::Wall)
            .then(|| gallery_layout_params_for_app(self).wall);
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
                .cards(self.viewport, wall_params.unwrap())
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
            let (source, kind) = select_source(wallpaper);
            let poster = self
                .poster_slots
                .get(&index)
                .filter(|s| poster_slot_matches_source(s, wallpaper, &source, kind))
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
                let weight = slice_state.unwrap().parallax.value().clamp(0.0, 1.0);
                if weight > 0.0 {
                    let [center, available] = self.gallery_composition();
                    let cx = center + self.viewport[0] * p.offset_x * 0.5;
                    let half = (layout_width(p.expanded_width, p.skew_distance)
                        + p.visible_count.saturating_sub(1) as f32
                            * (layout_width(p.width, p.skew_distance) + p.spacing))
                        .min(available - 40.0)
                        .max(1.0)
                        * 0.5;
                    let enabled = position_crop(
                        card.poster_aspect_ratio,
                        crop,
                        (card.center[0] - cx) / (half * 1.2).max(1.0),
                        1.4,
                    );
                    crop = blend_crop(crop, enabled, weight);
                }
            }
            if self.gallery_mode == GalleryMode::Geometric {
                if let Some(p) = &self.hex_presentation {
                    let disabled = position_crop(card.poster_aspect_ratio, crop, 0.0, 1.3);
                    let weight = p.parallax.value().clamp(0.0, 1.0);
                    crop = if weight > 0.0 {
                        let center = p.center_x();
                        let span = if card.center[0] < center {
                            center
                        } else {
                            self.viewport[0] - center
                        };
                        let enabled = position_crop(
                            card.poster_aspect_ratio,
                            crop,
                            (card.center[0] - center) / (span + card.size[0] * 0.5).max(1.0),
                            1.4,
                        );
                        blend_crop(disabled, enabled, weight)
                    } else {
                        disabled
                    };
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
            } else if queued && self.poster_loading && !self.poster_failed_indices.contains(&index)
            {
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
                wall_border: wall_params.map(|p| (selection, p.border_width)),
                shadows: (slices && slice_state.is_some_and(|p| p.render_shadows()))
                    || (self.gallery_mode == GalleryMode::Geometric && selection > 0.01),
                loading_eligible: wallpaper.source != WallpaperSource::LocalVideo,
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

    #[cfg(test)]
    pub(super) fn ordinary_poster_indices(&self) -> Vec<usize> {
        self.ordinary_poster_demand().0
    }

    pub(super) fn ordinary_poster_demand(&self) -> (Vec<usize>, usize) {
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
        let visible = indices.len();
        let (prefetch, request_limit) = match self.gallery_mode {
            GalleryMode::Wall => (
                self.wall_presentation
                    .as_ref()
                    .map_or_else(Vec::new, |wall| {
                        wall.prefetch_indices(
                            self.viewport,
                            gallery_layout_params_for_app(self).wall,
                        )
                        .into_iter()
                        .filter_map(|slot| self.filtered_indices.get(slot).copied())
                        .collect()
                    }),
                limit,
            ),
            GalleryMode::Geometric => (
                self.hex_presentation
                    .as_ref()
                    .map_or_else(Vec::new, |geometric| {
                        geometric
                            .prefetch_indices()
                            .into_iter()
                            .filter_map(|slot| self.filtered_indices.get(slot).copied())
                            .collect()
                    }),
                limit,
            ),
            _ => {
                let selected_position = self.presentation_inputs.as_ref().map_or_else(
                    || {
                        self.filtered_indices
                            .iter()
                            .position(|i| Some(*i) == self.selected)
                            .unwrap_or(0)
                    },
                    |p| {
                        self.selected
                            .and_then(|i| p.positions.get(&i).copied())
                            .unwrap_or(0)
                    },
                );
                (
                    gallery::prioritized_poster_indices_at(
                        &self.filtered_indices,
                        selected_position,
                    ),
                    limit.min(gpu_gallery::MAX_POSTERS),
                )
            }
        };
        let mut union: HashSet<_> = indices
            .iter()
            .copied()
            .chain(self.gallery_pins().iter().map(|pin| pin.index))
            .collect();
        for index in prefetch {
            if indices.len() >= request_limit {
                break;
            }
            if !union.contains(&index) && union.len() >= limit {
                break;
            }
            if !indices.contains(&index) {
                union.insert(index);
                indices.push(index);
            }
        }
        (indices, visible)
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
/// Rebase an interrupted layout on the visible cards. Indices belong to the
/// unchanged presentation catalog; poster identity is resolved by the frame owner.
pub(super) fn blend_layout_cards(
    source: &[SliceCard],
    destination: Vec<SliceCard>,
    selected: usize,
    amount: f32,
) -> Vec<SliceCard> {
    let mut by_index = std::collections::BTreeMap::new();
    for &card in source {
        by_index.entry(card.index).or_insert((None, None)).0 = Some(card);
    }
    for card in destination {
        by_index.entry(card.index).or_insert((None, None)).1 = Some(card);
    }
    by_index
        .into_iter()
        .filter_map(|(index, pair)| {
            let mut card = match pair {
                (Some(mut old), Some(new)) => {
                    for axis in 0..2 {
                        old.center[axis] += (new.center[axis] - old.center[axis]) * amount;
                        old.size[axis] += (new.size[axis] - old.size[axis]) * amount;
                    }
                    old.opacity += (new.opacity - old.opacity) * amount;
                    old.corner_radius += (new.corner_radius - old.corner_radius) * amount;
                    if amount >= 0.5 {
                        old.shape = new.shape;
                        old.triangle_direction = new.triangle_direction;
                    }
                    old
                }
                (Some(mut old), None) => {
                    old.opacity *= 1.0 - amount;
                    old
                }
                (None, Some(mut new)) => {
                    new.opacity *= amount;
                    new
                }
                (None, None) => return None,
            };
            card.selected = index == selected;
            Some(card)
        })
        .collect()
}

pub(super) fn parallax_motion(value: f32, enabled: bool, duration_ms: f32) -> GalleryMotionState {
    let target = f32::from(enabled);
    let from = if duration_ms > 0.0 {
        value.clamp(0.0, 1.0)
    } else {
        target
    };
    GalleryMotionState::absolute_camera(from, target, duration_ms, 1000.0, 1.0)
}

fn blend_crop(from: [f32; 4], to: [f32; 4], weight: f32) -> [f32; 4] {
    if weight >= 1.0 {
        to
    } else {
        std::array::from_fn(|index| from[index] + (to[index] - from[index]) * weight)
    }
}

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

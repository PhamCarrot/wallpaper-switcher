//! Ordinary filter cuts use the existing frame clock, poster pins, allocator and atlas.
use super::*;
use donor_scene::{CardPresentation, InstanceRaw};
use view::settings::composed::SettingsSource;

#[derive(Debug, Clone)]
pub(super) struct OrdinaryGhost {
    pub pin: gallery::DepthGhost,
    pub entry: CardPresentation,
    pub phase: f32,
}
#[derive(Debug, Clone)]
pub(super) struct OrdinaryRoll {
    pub wave: f32,
    pub old: Vec<OrdinaryGhost>,
    pub incoming: HashMap<(i32, i32), f32>,
    pub poster_wait_deadline: Option<std::time::Instant>,
}
impl OrdinaryRoll {
    pub fn advance_to(&mut self, wave: f32) {
        let step = (wave - self.wave).max(0.0);
        self.wave = wave.min(1.6);
        for old in &mut self.old {
            old.phase += step;
        }
        self.old.retain(|g| g.phase < 1.0);
        for phase in self.incoming.values_mut() {
            *phase += step;
        }
        if self.wave >= 1.6 {
            self.old.clear();
            self.incoming.clear();
        }
    }
}

fn phase(mode: GalleryMode, c: [f32; 2], viewport: [f32; 2], center: f32) -> f32 {
    let p = match mode {
        GalleryMode::Wall => (c[0] / viewport[0].max(1.0) + c[1] / viewport[1].max(1.0)) * 0.5,
        GalleryMode::Geometric => {
            (((c[0] - center) / (viewport[0] * 0.5).max(1.0)).powi(2)
                + ((c[1] - viewport[1] * 0.5) / (viewport[1] * 0.5).max(1.0)).powi(2))
            .sqrt()
                * 0.7
        }
        _ => c[0] / viewport[0].max(1.0),
    };
    -p.clamp(0.0, 1.0) * 0.6
}

impl GpuCandidate {
    pub(super) fn wall_colour_roll_held(&self) -> bool {
        self.ordinary_roll
            .as_ref()
            .is_some_and(|roll| roll.poster_wait_deadline.is_some())
    }

    pub(super) fn release_wall_colour_roll(&mut self, reason: &'static str) {
        if let Some(roll) = &mut self.ordinary_roll {
            if roll.poster_wait_deadline.take().is_some() {
                apply_trace::event_lazy("wall-colour-roll-released", || format!("reason={reason}"));
                self.start_motion(0.0);
            }
        }
    }

    /// User-approved ordinary incoming/outgoing union shares the device/256 MiB atlas bound.
    pub(super) fn ordinary_union_limit(&self) -> usize {
        gpu_gallery::ordinary_poster_limit()
    }

    pub(super) fn gallery_pins(&self) -> Vec<gallery::DepthGhost> {
        if self.gallery_mode == GalleryMode::Depth {
            self.depth_roll
                .as_ref()
                .map_or_else(Vec::new, |r| r.old.clone())
        } else {
            self.ordinary_roll
                .as_ref()
                .map_or_else(Vec::new, |r| r.old.iter().map(|g| g.pin.clone()).collect())
        }
    }

    pub(super) fn capture_ordinary_roll(&mut self) -> Result<Option<OrdinaryRoll>, String> {
        if self.gallery_mode == GalleryMode::Depth || !self.visible {
            return Ok(None);
        }
        let entries = self.ordinary_frame();
        let mut old = self
            .ordinary_roll
            .as_ref()
            .map_or_else(Vec::new, |r| r.old.clone());
        let mut cells: HashSet<_> = old
            .iter()
            .map(|g| gpu_gallery::depth_roll_cell(g.entry.card.center))
            .collect();
        let colors = super::view::gallery_snapshot_colors(self);
        let center = self
            .hex_presentation
            .as_ref()
            .map_or(self.viewport[0] * 0.5, |p| p.center_x());
        let scene = donor_scene::DonorSceneState {
            stage: Default::default(),
            camera: 0.0,
            camera_target: 0.0,
            camera_velocity: 0.0,
            detail_progress: self.detail_progress,
            external_detail: false,
            flip_options: self.flip_presentation(),
            detail_source: None,
            detail_card: None,
            viewport: self.viewport,
            time: self.presentation_started.elapsed().as_secs_f32(),
            visibility: 1.0,
        };
        for mut entry in entries {
            if entry.outgoing
                || entry.roll < 1.0
                || !cells.insert(gpu_gallery::depth_roll_cell(entry.card.center))
            {
                continue;
            }
            let Some(w) = self.wallpapers.get(entry.card.index) else {
                continue;
            };
            let Some(slot) = self.poster_slots.get(&entry.card.index).filter(|s| {
                s.catalog_generation == entry.catalog_generation && poster_slot_matches(s, w)
            }) else {
                continue;
            };
            let Some(poster) = self
                .posters
                .iter()
                .find(|p| p.layer == slot.layer && p.source == slot.source)
            else {
                continue;
            };
            let captured = donor_scene::presented_instances(&[entry.clone()], &colors, 1.0, &scene);
            let body = gpu_gallery::DepthSnapshotCard {
                id: entry.id.clone(),
                filtered_index: 0,
                card: entry.card,
                crop_uv: entry.crop,
                hovered: entry.hovered,
                selection_progress: entry.selection,
                visual_detail_progress: Some(self.detail_progress),
                retained_texture: Some(poster.clone()),
            };
            entry.captured_raw = Some(captured);
            entry.retained = Some(poster.clone());
            entry.outgoing = true;
            let pin = gallery::DepthGhost {
                body,
                phase: 0.0,
                slot: slot.clone(),
                index: entry.card.index,
                poster: poster.clone(),
            };
            let start = phase(self.gallery_mode, entry.card.center, self.viewport, center);
            old.push(OrdinaryGhost {
                pin,
                entry,
                phase: start,
            });
        }
        // Interrupted filters can leave several cells sharing one immutable
        // poster. Match the allocator's unique layer reservation, not body count.
        let resources = old
            .iter()
            .map(|g| g.pin.slot.layer)
            .collect::<HashSet<_>>()
            .len();
        if resources > self.ordinary_union_limit() {
            return Err(format!("Filter transition needs {} retained images; the current atlas limit is {}. Reduce the visible layout and retry.", resources, self.ordinary_union_limit()));
        }
        Ok(Some(OrdinaryRoll {
            wave: 0.0,
            old,
            incoming: HashMap::new(),
            poster_wait_deadline: None,
        }))
    }

    pub(super) fn reconcile_ordinary_roll(&mut self) {
        if self.gallery_mode == GalleryMode::Depth {
            return;
        }
        let Some(mut roll) = self.ordinary_roll.take() else {
            return;
        };
        let base = self.ordinary_base_frame();
        roll.old.retain(|g| {
            g.entry.catalog_generation == self.catalog_generation.load(Ordering::Acquire)
                && self
                    .wallpapers
                    .get(g.pin.index)
                    .is_some_and(|w| poster_slot_matches(&g.pin.slot, w))
        });
        // A reused identity stays fully visible after its old pin is retired.
        let reused: Vec<_> = roll
            .old
            .iter()
            .filter_map(|g| {
                let cell = gpu_gallery::depth_roll_cell(g.entry.card.center);
                base.iter()
                    .any(|e| {
                        gpu_gallery::depth_roll_cell(e.card.center) == cell
                            && e.id == g.entry.id
                            && e.source == g.entry.source
                            && g.phase <= 0.1
                    })
                    .then_some(cell)
            })
            .collect();
        roll.old
            .retain(|g| !reused.contains(&gpu_gallery::depth_roll_cell(g.entry.card.center)));
        for cell in reused {
            roll.incoming.insert(cell, 1.0);
        }
        let identities: HashSet<_> = base
            .iter()
            .map(|e| e.card.index)
            .chain(roll.old.iter().map(|g| g.pin.index))
            .collect();
        if identities.len() > self.ordinary_union_limit() {
            self.ordinary_transition_issue=Some(format!("Filter roll requires {} simultaneous preview images; current atlas limit is {}. Reduce the visible layout and retry.",identities.len(),self.ordinary_union_limit()));
            self.update_catalog_status();
            return;
        }
        let center = self
            .hex_presentation
            .as_ref()
            .map_or(self.viewport[0] * 0.5, |p| p.center_x());
        for e in &base {
            let cell = gpu_gallery::depth_roll_cell(e.card.center);
            if !roll
                .old
                .iter()
                .any(|g| gpu_gallery::depth_roll_cell(g.entry.card.center) == cell)
            {
                roll.incoming.entry(cell).or_insert(
                    roll.wave + phase(self.gallery_mode, e.card.center, self.viewport, center),
                );
            }
        }
        if roll.wave < 1.6 || !roll.old.is_empty() {
            self.ordinary_roll = Some(roll);
        }
    }

    pub(super) fn ordinary_frame(&self) -> Vec<CardPresentation> {
        let mut entries = self.ordinary_base_frame();
        let Some(roll) = &self.ordinary_roll else {
            return entries;
        };
        let center = self
            .hex_presentation
            .as_ref()
            .map_or(self.viewport[0] * 0.5, |p| p.center_x());
        for e in &mut entries {
            let cell = gpu_gallery::depth_roll_cell(e.card.center);
            let t = roll
                .old
                .iter()
                .find(|g| gpu_gallery::depth_roll_cell(g.entry.card.center) == cell)
                .map_or_else(
                    || {
                        *roll.incoming.get(&cell).unwrap_or(
                            &(roll.wave
                                + phase(self.gallery_mode, e.card.center, self.viewport, center)),
                        )
                    },
                    |g| {
                        if g.entry.id == e.id && g.entry.source == e.source && g.phase <= 0.1 {
                            1.0
                        } else {
                            g.phase
                        }
                    },
                );
            e.roll = gpu_gallery::depth_roll_fraction(t);
        }
        for g in &roll.old {
            let mut entry = g.entry.clone();
            entry.roll = gpu_gallery::depth_roll_fraction(g.phase);
            if entry.roll < 1.0 {
                entries.push(entry);
            }
        }
        entries
    }

    pub(super) fn filter_swap_ms(&self) -> f32 {
        let source = view::settings::bridge::Source { app: self };
        let key = match source.text("motion.filterSwapSpeed").as_str() {
            "fast" => "motion.fastMs",
            "standard" => "motion.standardMs",
            _ => "motion.slowMs",
        };
        (source.number(key) as f32).max(50.0)
    }
}

impl GpuCandidate {
    pub(super) fn external_detail_card(&self) -> Option<gpu_gallery::SliceCard> {
        if !self.detail_open
            || !matches!(
                self.gallery_mode,
                GalleryMode::Wall | GalleryMode::Geometric
            )
        {
            return None;
        }
        let index = self.selected?;
        let w = self.wallpapers.get(index)?;
        if self.depth_detail_id.as_deref() != Some(w.id.as_str()) {
            return None;
        }
        let g = self.detail_source?;
        let slot = self.poster_slots.get(&index).filter(|s| {
            s.catalog_generation == self.catalog_generation.load(Ordering::Acquire)
                && poster_slot_matches(s, w)
        });
        let poster = slot.and_then(|s| {
            self.posters
                .iter()
                .find(|p| p.layer == s.layer && p.source == s.source)
        });
        Some(gpu_gallery::SliceCard {
            triangle_direction: 0,
            index,
            center: g.center,
            size: g.size,
            layer: poster.map(|p| p.layer),
            selected: true,
            opacity: 1.0,
            skew_distance: g.skew_distance,
            edge_tilt_distance: g.edge_tilt_distance,
            rotation: g.rotation,
            corner_radius: 0.0,
            shape: 0,
            poster_aspect_ratio: poster.map_or(16.0 / 9.0, |p| p.aspect_ratio),
        })
    }
}

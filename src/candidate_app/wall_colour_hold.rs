//! Keep the displayed Wall frame available while Colour's new order prewarms.
use super::*;
use donor_scene::CardPresentation;
use std::time::Instant;

#[derive(Debug, Clone)]
pub(super) struct WallPosterPin {
    pub index: usize,
    pub slot: PosterSlot,
    pub poster: Poster,
}

pub(super) struct WallColourHold {
    pub entries: Arc<Vec<CardPresentation>>,
    pub clip: [f32; 4],
    pub deadline: Instant,
    pins: Vec<WallPosterPin>,
    viewport: [f32; 2],
    selected: Option<usize>,
    layout: candidate_state::WallSettings,
    rail: (Option<f32>, Option<f32>, f32, bool),
}

impl GpuCandidate {
    fn wall_rail_geometry(&self) -> (Option<f32>, Option<f32>, f32, bool) {
        let wall = gallery_layout_params_for_app(self).wall;
        (
            wall.rail_width,
            wall.rail_height,
            wall.rail_gap,
            wall.rail_rtl,
        )
    }

    pub(super) fn wall_frame_clip(&self) -> [f32; 4] {
        let composition = gallery_layout_params_for_app(self).wall;
        self.wall_presentation.as_ref().map_or_else(
            || gpu_gallery::wall_frame(self.viewport, composition).clip,
            |p| p.clip(self.viewport, composition),
        )
    }

    pub(super) fn capture_wall_colour_hold(&self) -> Option<WallColourHold> {
        let entries = self.ordinary_frame();
        if entries.is_empty() {
            return None;
        }
        let pins = entries
            .iter()
            .filter_map(|entry| {
                let layer = entry.card.layer?;
                let slot = self.poster_slots.get(&entry.card.index)?;
                let poster = self
                    .posters
                    .iter()
                    .find(|p| p.layer == layer && p.source == slot.source)?;
                Some(WallPosterPin {
                    index: entry.card.index,
                    slot: slot.clone(),
                    poster: poster.clone(),
                })
            })
            .collect();
        Some(WallColourHold {
            entries: Arc::new(entries),
            clip: self.wall_frame_clip(),
            deadline: Instant::now(),
            pins,
            viewport: self.viewport,
            selected: self.selected,
            layout: self.state.gallery.wall.clone(),
            rail: self.wall_rail_geometry(),
        })
    }

    pub(super) fn validate_wall_colour_hold(&mut self) {
        let Some(hold) = &self.wall_colour_hold else {
            return;
        };
        let valid = self.gallery_mode == GalleryMode::Wall
            && self.atlas_entry.is_none()
            && self.viewport == hold.viewport
            && self.selected == hold.selected
            && self.state.gallery.wall == hold.layout
            && self.wall_rail_geometry() == hold.rail;
        if !valid {
            self.release_wall_colour_hold("wall-input-changed");
            return;
        }
        // Pins share the ordinary atlas with the incoming visible cards and detail.
        // A hold cannot be allowed to squeeze those identities out of demand.
        let required = self
            .ordinary_frame()
            .iter()
            .map(|entry| entry.card.index)
            .chain(self.external_detail_card().iter().map(|card| card.index))
            .chain(hold.pins.iter().map(|pin| pin.index))
            .collect::<HashSet<_>>();
        if required.len() > gpu_gallery::ordinary_poster_limit() {
            self.release_wall_colour_hold("insufficient-atlas-capacity");
        }
    }

    pub(super) fn wall_colour_pins(&self) -> &[WallPosterPin] {
        self.wall_colour_hold
            .as_ref()
            .map_or(&[], |hold| hold.pins.as_slice())
    }

    pub(super) fn release_wall_colour_hold(&mut self, reason: &'static str) {
        if self.wall_colour_hold.take().is_some() {
            apply_trace::event_lazy("wall-colour-hold-released", || format!("reason={reason}"));
            self.start_motion(0.0);
        }
    }
}

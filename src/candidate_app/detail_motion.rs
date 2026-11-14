//! Pure detail clock kernel used by the existing DetailFrame owner.
use super::*;

/// Source external det_p is a linear Standard ramp, independent of the shared flip spring.
pub(super) fn advance_detail(
    spring: &mut GalleryMotionState,
    progress: f32,
    external: bool,
    standard_ms: f32,
    dt: f32,
) -> (f32, bool) {
    let moving = spring.tick(dt);
    if external {
        let step = dt.min(1.0 / 30.0) * 1000.0 / standard_ms.max(35.0);
        let value = if spring.target > progress {
            (progress + step).min(spring.target)
        } else {
            (progress - step).max(spring.target)
        };
        // External close releases the overlay at det_p0. An open configured
        // spring (including an effect restart) keeps the same clock alive after
        // det_p1, without slowing the Standard visual ramp.
        (
            value,
            value == spring.target && (spring.target == 0.0 || !moving),
        )
    } else {
        (spring.value(), !moving)
    }
}

impl GpuCandidate {
    pub(super) fn flip_presentation(&self) -> donor_scene::DepthDetailPresentation {
        let p = self.state.card_flip();
        donor_scene::DepthDetailPresentation {
            effect: candidate_state::depth_effect_id(&p.effect).unwrap_or(0),
            animate_shader: p.animate_shader,
            animate_back: p.animate_back,
        }
    }
}

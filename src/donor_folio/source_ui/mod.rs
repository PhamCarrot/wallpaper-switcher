#![allow(dead_code, unused_imports)]
macro_rules! flow {
    ($rtl:expr; $($child:expr),* $(,)?) => {{
        crate::donor_folio::source_ui::direction::Flow::new($rtl,vec![$(::iced::Element::from($child)),*])
    }};
}
pub(crate) use flow;
pub mod color;
pub mod direction;
pub mod folio;
pub mod layout;
pub mod motion;
pub mod style;
pub mod typography;
pub mod wheel;
pub use crate::donor_ui::with_alpha_color as with_alpha;
pub use folio::*;
pub use layout::*;
pub use style::*;
pub use typography::*;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn direction_is_explicit_and_visual_reversal_preserves_logical_option_identity() {
        let logical = vec![(0, "First"), (1, "Second"), (2, "Third")];
        assert_eq!(direction::ordered(logical.clone(), false), logical);
        assert_eq!(
            direction::ordered(logical.clone(), true),
            vec![(2, "Third"), (1, "Second"), (0, "First")]
        );
        let ltr = direction::logical_padding_for(false, 1.0, 2.0, 3.0, 4.0);
        let rtl = direction::logical_padding_for(true, 1.0, 2.0, 3.0, 4.0);
        assert_eq!((ltr.left, ltr.right), (4.0, 2.0));
        assert_eq!((rtl.left, rtl.right), (2.0, 4.0));
        assert_eq!(direction::mirror_x_for(true, 10.0, 30.0, 100.0), 60.0);
    }
    #[test]
    fn source_wrapping_keeps_order_and_all_choices_under_narrow_and_long_labels() {
        let rows = wrap_rows(vec![(60.0, 0), (70.0, 1), (90.0, 2), (20.0, 3)], 140.0, 7.0);
        assert_eq!(rows, vec![vec![0, 1], vec![2, 3]]);
        let long = wrap_rows(vec![(400.0, 0), (50.0, 1)], 100.0, 7.0);
        assert_eq!(long, vec![vec![0], vec![1]]);
        assert_eq!(folio_sheet_dims((1920.0, 1080.0), 1.0), (1390.0, 870.0));
        assert_eq!(folio_sheet_dims((2560.0, 1440.0), 1.0), (1390.0, 870.0));
        assert_eq!(typography::legible_type_scale(0.5), 0.95);
    }
    #[test]
    fn tween_retarget_and_duration_changes_keep_presented_state_and_bound_hitches() {
        let profile = motion::MotionProfile::new(180.0, 250.0, 450.0);
        let mut tween = profile.tween(0.0, motion::MotionTier::Fast);
        tween.retarget(1.0);
        tween.tick(0.016);
        let current = tween.x;
        tween.retarget(0.0);
        assert_eq!(tween.x, current);
        tween.set_duration_ms(450.0);
        assert_eq!(tween.x, current);
        tween.tick(20.0);
        assert_eq!(tween.x, 0.0);
        assert!(tween.settled());
        assert_eq!(profile.duration_ms(motion::MotionTier::Standard), 250.0);
    }
}

pub const FOLIO_BUTTON_HEIGHT: f32 = 30.0;

pub mod slider;
pub use slider::folio_slider;

pub mod pane;
pub use pane::{scroll_style, thin_vbar};
pub mod studio;
pub use direction::{end_for, start_for};
pub use studio::picker_layout_studio_width;

pub mod clock;

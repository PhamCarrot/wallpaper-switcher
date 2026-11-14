use iced::widget::canvas::{self, Action};
use iced::{mouse, Border, Color, Element, Event, Rectangle};

use super::with_alpha;

pub fn thin_vbar() -> iced::widget::scrollable::Direction {
    use iced::widget::scrollable::{Direction, Scrollbar};
    Direction::Vertical(Scrollbar::new().width(5.0).scroller_width(5.0).margin(2.0))
}

pub fn thin_hbar() -> iced::widget::scrollable::Direction {
    use iced::widget::scrollable::{Direction, Scrollbar};
    Direction::Horizontal(Scrollbar::new().width(5.0).scroller_width(5.0).margin(2.0))
}

pub fn scroll_style(
    primary: Color,
) -> impl Fn(&iced::Theme, iced::widget::scrollable::Status) -> iced::widget::scrollable::Style {
    use iced::widget::scrollable;
    move |theme, status| {
        let base = scrollable::default(theme, status);
        let hot = matches!(
            status,
            scrollable::Status::Hovered { .. } | scrollable::Status::Dragged { .. }
        );
        let scol = with_alpha(primary, if hot { 0.85 } else { 0.45 });
        let sb = Border {
            radius: 2.0.into(),
            ..Border::default()
        };
        let mk = || scrollable::Rail {
            background: None,
            border: Border::default(),
            scroller: scrollable::Scroller {
                background: scol.into(),
                border: sb,
            },
        };
        scrollable::Style {
            vertical_rail: mk(),
            horizontal_rail: mk(),
            ..base
        }
    }
}

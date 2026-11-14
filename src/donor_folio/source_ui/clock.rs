//! A redraw-driven presentation clock. No thread, provider, or app state owner.
use iced::widget::canvas::{self, Action, Event};
use iced::{mouse, window, Rectangle};
use std::time::{Duration, Instant};

pub struct Clock<Message> {
    pub active: bool,
    pub interval: Duration,
    pub on_frame: fn(Instant) -> Message,
}
#[derive(Default)]
pub struct State {
    last_frame: Option<Instant>,
}

impl<Message> canvas::Program<Message> for Clock<Message> {
    type State = State;
    fn update(
        &self,
        state: &mut State,
        event: &Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        if !self.active {
            state.last_frame = None;
            return None;
        }
        let Event::Window(window::Event::RedrawRequested(now)) = event else {
            return None;
        };
        if let Some(last) = state.last_frame {
            let due = last + self.interval;
            if *now < due {
                return Some(Action::request_redraw_at(due));
            }
        }
        state.last_frame = Some(*now);
        Some(Action::publish((self.on_frame)(*now)))
    }
    fn draw(
        &self,
        _state: &State,
        _renderer: &iced::Renderer,
        _theme: &iced::Theme,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::canvas::Program;
    #[test]
    fn clock_emits_bounded_frames_then_stops_without_capturing_pointer_input() {
        let clock = Clock {
            active: true,
            interval: Duration::from_millis(20),
            on_frame: |now| now,
        };
        let mut state = State::default();
        let now = Instant::now();
        let bounds = Rectangle::default();
        let cursor = mouse::Cursor::Unavailable;
        let (message, _, status) = clock
            .update(
                &mut state,
                &Event::Window(window::Event::RedrawRequested(now)),
                bounds,
                cursor,
            )
            .unwrap()
            .into_inner();
        assert_eq!(message, Some(now));
        assert_eq!(status, iced::event::Status::Ignored);
        let (message, redraw, _) = clock
            .update(
                &mut state,
                &Event::Window(window::Event::RedrawRequested(
                    now + Duration::from_millis(1),
                )),
                bounds,
                cursor,
            )
            .unwrap()
            .into_inner();
        assert!(message.is_none());
        assert_eq!(
            redraw,
            window::RedrawRequest::At(now + Duration::from_millis(20))
        );
        assert!(clock
            .update(
                &mut state,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor
            )
            .is_none());
        let stopped = Clock {
            active: false,
            ..clock
        };
        assert!(stopped
            .update(
                &mut state,
                &Event::Window(window::Event::RedrawRequested(now + Duration::from_secs(1))),
                bounds,
                cursor
            )
            .is_none());
        assert!(state.last_frame.is_none());
    }
}

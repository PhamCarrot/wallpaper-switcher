use iced::widget::canvas::{self, Action, Frame, Path, Stroke};
use iced::{mouse, Element, Event, Point, Rectangle};

use crate::donor_ui::Palette;

use super::with_alpha;

const FOLIO_SLIDER_INSET: f32 = 7.0;

fn slider_fraction(min: f64, max: f64, value: f64) -> f32 {
    if max <= min {
        0.0
    } else {
        (((value - min) / (max - min)) as f32).clamp(0.0, 1.0)
    }
}

fn track_x(rtl: bool, x_local: f32, w: f32) -> f32 {
    if rtl {
        w - x_local
    } else {
        x_local
    }
}

fn marker_snap(raw_x: f32) -> f32 {
    (raw_x * 2.0).round() * 0.5
}

fn slider_value(min: f64, max: f64, step: f64, x_local: f32, w: f32) -> f64 {
    let frac = (x_local / w.max(1.0)).clamp(0.0, 1.0) as f64;
    let raw = min + frac * (max - min);
    let snapped = if step > 0.0 {
        (raw / step).round() * step
    } else {
        raw
    };
    snapped.clamp(min, max)
}

pub struct FolioSlider<Message> {
    rtl: bool,
    enabled: bool,
    min: f64,
    max: f64,
    value: f64,
    step: f64,
    pal: Palette,
    on_change: Box<dyn Fn(f64) -> Message>,
    on_release: Message,
}

impl<Message> FolioSlider<Message> {
    fn value_at(&self, rtl: bool, x: f32, width: f32) -> f64 {
        let track = width - FOLIO_SLIDER_INSET * 2.0;
        slider_value(
            self.min,
            self.max,
            self.step,
            track_x(rtl, x - FOLIO_SLIDER_INSET, track),
            track,
        )
    }
}

impl<Message: Clone> canvas::Program<Message> for FolioSlider<Message> {
    type State = bool;

    fn update(
        &self,
        dragging: &mut bool,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<Message>> {
        if !self.enabled {
            *dragging = false;
            return None;
        }
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if let Some(point) = cursor.position_in(bounds) {
                    *dragging = true;
                    let value = self.value_at(self.rtl, point.x, bounds.width);
                    return Some(Action::publish((self.on_change)(value)).and_capture());
                }
                None
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) if *dragging => {
                let x = cursor.position().map_or(0.0, |point| point.x - bounds.x);
                let value = self.value_at(self.rtl, x, bounds.width);
                Some(Action::publish((self.on_change)(value)).and_capture())
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if *dragging => {
                *dragging = false;
                Some(Action::publish(self.on_release.clone()).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &bool,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let inset = FOLIO_SLIDER_INSET;
        let mirrored = self.rtl;
        let start = Point::new(inset, bounds.height * 0.5);
        let end = Point::new((bounds.width - inset).max(inset), start.y);
        let raw_x = start.x
            + track_x(
                mirrored,
                (end.x - start.x) * slider_fraction(self.min, self.max, self.value),
                end.x - start.x,
            );
        let x = marker_snap(raw_x);

        frame.stroke(
            &Path::line(start, end),
            Stroke::default()
                .with_color(with_alpha(self.pal.outline, 0.5))
                .with_width(1.0),
        );
        let (fill_from, fill_to) = if mirrored {
            (Point::new(x, start.y), end)
        } else {
            (start, Point::new(x, start.y))
        };
        if fill_from.x < fill_to.x {
            frame.stroke(
                &Path::line(fill_from, fill_to),
                Stroke::default()
                    .with_color(self.pal.primary)
                    .with_width(2.0),
            );
        }

        let radius = 5.0;
        let marker = Path::new(|builder| {
            builder.move_to(Point::new(x, start.y - radius));
            builder.line_to(Point::new(x + radius, start.y));
            builder.line_to(Point::new(x, start.y + radius));
            builder.line_to(Point::new(x - radius, start.y));
            builder.close();
        });
        frame.fill(&marker, self.pal.surface);
        frame.stroke(
            &marker,
            Stroke::default()
                .with_color(self.pal.primary)
                .with_width(1.0),
        );
        vec![frame.into_geometry()]
    }
}

pub fn folio_slider<'a, Message: Clone + 'a>(
    rtl: bool,
    enabled: bool,
    min: f64,
    max: f64,
    value: f64,
    step: f64,
    on_change: impl Fn(f64) -> Message + 'static,
    on_release: Message,
    pal: &Palette,
) -> Element<'a, Message> {
    iced::widget::canvas(FolioSlider {
        rtl,
        enabled,
        min,
        max,
        value,
        step,
        pal: if enabled {
            *pal
        } else {
            let mut disabled = *pal;
            disabled.primary = with_alpha(disabled.primary, 0.38);
            disabled
        },
        on_change: Box::new(on_change),
        on_release,
    })
    .width(iced::Length::Fill)
    .height(iced::Length::Fixed(26.0))
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::widget::canvas::Program;
    #[derive(Debug, Clone)]
    enum Intent {
        Change(f64),
        Release,
    }
    fn widget(enabled: bool, rtl: bool) -> FolioSlider<Intent> {
        FolioSlider {
            rtl,
            enabled,
            min: 0.0,
            max: 100.0,
            value: 50.0,
            step: 1.0,
            pal: crate::donor_ui::Palette::from_rgb([0.0; 3], [0.1; 3], [1.0; 3], [0.8, 0.2, 0.4]),
            on_change: Box::new(Intent::Change),
            on_release: Intent::Release,
        }
    }
    #[test]
    fn disabled_display_slider_cannot_publish_pointer_input_or_remain_dragging() {
        let slider = widget(false, false);
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 26.0,
        };
        let cursor = mouse::Cursor::Available(Point::new(100.0, 13.0));
        let mut dragging = true;
        assert!(slider
            .update(
                &mut dragging,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor
            )
            .is_none());
        assert!(!dragging);
        assert!(slider
            .update(
                &mut dragging,
                &Event::Mouse(mouse::Event::CursorMoved {
                    position: Point::new(140.0, 13.0)
                }),
                bounds,
                cursor
            )
            .is_none());
        assert!(slider
            .update(
                &mut dragging,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor
            )
            .is_none());
    }
    #[test]
    fn enabled_slider_retains_capture_release_and_explicit_direction_values() {
        let slider = widget(true, false);
        let bounds = Rectangle {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 26.0,
        };
        let cursor = mouse::Cursor::Available(Point::new(100.0, 13.0));
        let mut dragging = false;
        assert!(slider
            .update(
                &mut dragging,
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                cursor
            )
            .is_some());
        assert!(dragging);
        assert!(slider
            .update(
                &mut dragging,
                &Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                bounds,
                cursor
            )
            .is_some());
        assert!(!dragging);
        assert_eq!(slider.value_at(false, 7.0, 200.0), 0.0);
        assert_eq!(slider.value_at(true, 7.0, 200.0), 100.0);
        assert_eq!(slider.value_at(false, 193.0, 200.0), 100.0);
        assert_eq!(slider.value_at(true, 193.0, 200.0), 0.0);
    }
}

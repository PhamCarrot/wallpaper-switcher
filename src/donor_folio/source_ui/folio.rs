use crate::donor_folio::source_ui::direction::{logical_padding_for, mirror_x_for};
use crate::donor_folio::source_ui::flow;
use std::time::{Duration, Instant};

use iced::widget::canvas::{self, Frame, Path, Stroke};
use iced::widget::{button, column, container, stack, text};
use iced::{
    mouse, window, Alignment, Background, Border, Color, Element, Event, Length, Point, Rectangle,
};

use super::motion::{MotionProfile, MotionTier};
use crate::donor_folio::source_ui::{with_alpha, NERD_FONT, UI_FONT};
use crate::donor_ui::Palette;

use crate::donor_folio::source_ui::typography::legible_type_scale;

const BUTTON_FRAME: Duration = Duration::from_millis(16);
pub const BUTTON_HEIGHT: f32 = 30.0;
pub const EYE_CLOSED: &str = "\u{eae7}";

pub const FOLIO_INDEX_WIDTH: f32 = 318.0;
pub const FOLIO_SCRIM_ALPHA: f32 = 0.68;
pub const FOLIO_INDEX_BG_ALPHA: f32 = 0.9;
pub const FOLIO_INDEX_OUTLINE_ALPHA: f32 = 0.46;

const FOLIO_ACTION_MIN_WIDTH: f32 = 52.0;
const FOLIO_ACTION_MAX_WIDTH: f32 = 190.0;

pub fn folio_action_width_in(action_label: &str, scale: f32, min: f32, max: f32) -> f32 {
    (crate::donor_folio::source_ui::typography::text_width(action_label, 10.0, false) + 28.0)
        .clamp(min, max)
        * scale
}

pub fn folio_action_width(action_label: &str, scale: f32) -> f32 {
    folio_action_width_in(
        action_label,
        scale,
        FOLIO_ACTION_MIN_WIDTH,
        FOLIO_ACTION_MAX_WIDTH,
    )
}

pub fn folio_scrim_style(ease: f32) -> container::Style {
    crate::donor_folio::source_ui::style::scrim_style(FOLIO_SCRIM_ALPHA * ease)
}

pub fn folio_sheet_panel_style(palette: &Palette, ease: f32) -> container::Style {
    let mut style = crate::donor_folio::source_ui::style::box_style(
        with_alpha(palette.surface, 0.99),
        with_alpha(palette.outline, 0.58),
    );
    style.shadow = iced::Shadow {
        color: with_alpha(Color::BLACK, 0.52 * ease),
        offset: iced::Vector::new(0.0, 16.0),
        blur_radius: 48.0,
    };
    style
}

pub fn folio_index_shell<'a, Message: Clone + 'a>(
    rtl: bool,
    title: impl text::IntoFragment<'a>,
    note: impl text::IntoFragment<'a>,
    body: Vec<Element<'a, Message>>,
    spacing: f32,
    scale: f32,
    palette: &Palette,
) -> Element<'a, Message> {
    folio_index_shell_tinted(
        rtl,
        title,
        note,
        body,
        spacing,
        (FOLIO_INDEX_BG_ALPHA, FOLIO_INDEX_OUTLINE_ALPHA),
        scale,
        palette,
    )
}

pub fn folio_index_shell_tinted<'a, Message: Clone + 'a>(
    rtl: bool,
    title: impl text::IntoFragment<'a>,
    note: impl text::IntoFragment<'a>,
    body: Vec<Element<'a, Message>>,
    spacing: f32,
    alphas: (f32, f32),
    scale: f32,
    palette: &Palette,
) -> Element<'a, Message> {
    use crate::donor_folio::source_ui::typography::label;
    let palette = *palette;
    let heading = column![
        label(title, 20.0, scale, palette.surface_text),
        label(
            note,
            crate::donor_folio::source_ui::typography::TYPE_SMALL,
            scale,
            with_alpha(palette.surface_text, 0.56)
        )
        .line_height(iced::widget::text::LineHeight::Relative(1.4)),
    ]
    .spacing(7.0 * scale);
    let mut content = column![heading].spacing(spacing * scale);
    for element in body {
        content = content.push(element);
    }
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(logical_padding_for(
            rtl,
            25.0 * scale,
            20.0 * scale,
            20.0 * scale,
            22.0 * scale,
        ))
        .style(move |_| {
            crate::donor_folio::source_ui::style::box_style(
                with_alpha(palette.background, alphas.0),
                with_alpha(palette.outline, alphas.1),
            )
        })
        .into()
}

pub fn folio_stack_bar<'a, Message: Clone + 'a>(
    rtl: bool,
    content: Element<'a, Message>,
    active: bool,
    expanded: bool,
    reveal: f32,
    on_activate: Message,
    trailing: Option<Element<'a, Message>>,
    options: Element<'a, Message>,
    bar_height: f32,
    options_height: f32,
    scale: f32,
    palette: &'a Palette,
) -> Element<'a, Message> {
    use crate::donor_folio::source_ui::typography::label;

    let disclosure = container(label(
        if expanded { "▴" } else { "▾" },
        9.0,
        scale,
        if expanded {
            palette.primary
        } else {
            with_alpha(palette.surface_text, 0.58)
        },
    ))
    .width(Length::Fixed(20.0 * scale))
    .height(Length::Fixed(32.0 * scale))
    .center(Length::Fill);
    let select = button(
        flow![rtl;content, disclosure]
            .spacing(6.0 * scale)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .padding([9.0 * scale, 10.0 * scale])
    .on_press(on_activate)
    .style(move |_theme, status| {
        crate::donor_folio::source_ui::style::folio_embedded_button_style(
            palette.surface_text,
            !active,
            palette,
            status,
        )
    });
    let marker = container(text(""))
        .width(Length::Fixed(2.0 * scale))
        .height(Length::Fill)
        .style(move |_| {
            crate::donor_folio::source_ui::style::bg_style(Background::Color(if active {
                palette.primary
            } else {
                Color::TRANSPARENT
            }))
        });
    let mut bar_content = flow![rtl;marker, select]
        .spacing(0.0)
        .align_y(Alignment::Center);
    if let Some(trailing) = trailing {
        bar_content = bar_content.push(trailing);
    }
    let bar = container(bar_content)
        .width(Length::Fill)
        .height(Length::Fixed(bar_height * scale))
        .style(move |_| {
            crate::donor_folio::source_ui::style::box_style(
                if active {
                    with_alpha(palette.primary, 0.18)
                } else {
                    with_alpha(palette.surface_container, 0.22)
                },
                with_alpha(
                    if active {
                        palette.primary
                    } else {
                        palette.outline
                    },
                    0.3,
                ),
            )
        });
    let mut layers = column![bar].spacing(0.0);
    let reveal = super::motion::smoothstep(reveal);
    if reveal > 0.0 {
        layers = layers.push(
            container(options)
                .width(Length::Fill)
                .height(Length::Fixed(options_height * scale * reveal))
                .clip(true)
                .padding(logical_padding_for(
                    rtl,
                    7.0 * scale,
                    30.0 * scale,
                    8.0 * scale,
                    12.0 * scale,
                ))
                .style(move |_| {
                    crate::donor_folio::source_ui::style::box_style(
                        with_alpha(palette.surface_container, 0.62),
                        with_alpha(
                            if active {
                                palette.primary
                            } else {
                                palette.outline
                            },
                            0.34,
                        ),
                    )
                }),
        );
    }
    container(layers).width(Length::Fill).into()
}

fn folio_trailing_bar<'a, Message: Clone + 'a>(
    rtl: bool,
    content: Element<'a, Message>,
    focused: bool,
    on_focus: Message,
    action: Element<'a, Message>,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
) -> Element<'a, Message> {
    let marker = container(text(""))
        .width(Length::Fixed(2.0 * scale))
        .height(Length::Fill)
        .style(move |_| {
            crate::donor_folio::source_ui::style::bg_style(Background::Color(if focused {
                with_alpha(palette.primary, fade)
            } else {
                Color::TRANSPARENT
            }))
        });
    let copy = iced::widget::mouse_area(
        container(content)
            .width(Length::Fill)
            .align_y(iced::alignment::Vertical::Center)
            .padding(logical_padding_for(
                rtl,
                4.0 * scale,
                8.0 * scale,
                4.0 * scale,
                10.0 * scale,
            ))
            .clip(true),
    )
    .on_press(on_focus);
    let action = container(action).padding(logical_padding_for(rtl, 0.0, 6.0 * scale, 0.0, 0.0));
    container(
        flow![rtl;iced::widget::Space::new().height(60.0 * scale), marker, copy, action]
            .height(Length::Shrink)
            .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .style(move |_| {
        crate::donor_folio::source_ui::style::box_style(
            with_alpha(palette.surface_container, 0.22 * fade),
            with_alpha(
                if focused {
                    palette.primary
                } else {
                    palette.outline
                },
                0.3 * fade,
            ),
        )
    })
    .into()
}

pub fn folio_action_bar<'a, Message: Clone + 'a>(
    rtl: bool,
    content: Element<'a, Message>,
    focused: bool,
    on_focus: Message,
    action: Element<'a, Message>,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
) -> Element<'a, Message> {
    folio_trailing_bar(
        rtl, content, focused, on_focus, action, scale, palette, fade,
    )
}

pub fn folio_inline_bar<'a, Message: Clone + 'a>(
    rtl: bool,
    content: Element<'a, Message>,
    focused: bool,
    on_focus: Message,
    editor: Element<'a, Message>,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
) -> Element<'a, Message> {
    folio_trailing_bar(
        rtl, content, focused, on_focus, editor, scale, palette, fade,
    )
}

pub fn folio_field<'a, Message: Clone + 'a>(
    rtl: bool,
    number: impl text::IntoFragment<'a>,
    title: impl text::IntoFragment<'a>,
    description: impl text::IntoFragment<'a>,
    body: Element<'a, Message>,
    scale: f32,
    palette: &'a Palette,
) -> Element<'a, Message> {
    use crate::donor_folio::source_ui::typography::label;
    container(
        column![
            crate::donor_folio::source_ui::layout::folio_horizontal_rule(with_alpha(
                palette.outline,
                0.5
            )),
            flow![rtl;
                label(number, 9.0, scale, palette.primary),
                label(title, 13.0, scale, palette.surface_text),
            ]
            .spacing(8.0 * scale)
            .align_y(Alignment::Center),
            label(
                description,
                crate::donor_folio::source_ui::typography::TYPE_SMALL,
                scale,
                with_alpha(palette.surface_text, 0.48)
            )
            .line_height(iced::widget::text::LineHeight::Relative(1.35)),
            body,
        ]
        .spacing(9.0 * scale),
    )
    .width(Length::Fill)
    .padding(iced::Padding {
        top: 0.0,
        right: 8.0 * scale,
        bottom: 6.0 * scale,
        left: 8.0 * scale,
    })
    .into()
}

pub fn folio_ghost_field<'a, Message: Clone + 'a>(
    value: &'a str,
    placeholder: &'a str,
    on_input: impl Fn(String) -> Message + 'a,
    on_submit: Message,
    width: Length,
    scale: f32,
    palette: &Palette,
) -> iced::widget::TextInput<'a, Message> {
    let palette = *palette;
    iced::widget::text_input(placeholder, value)
        .on_input(on_input)
        .on_submit(on_submit)
        .padding(8.0 * scale)
        .size(11.0 * legible_type_scale(scale))
        .font(UI_FONT)
        .width(width)
        .style(move |_theme, _status| {
            crate::donor_folio::source_ui::style::ghost_input_style(&palette)
        })
}

pub fn folio_details<'a, Message: Clone + 'a>(
    rtl: bool,
    title: String,
    summary: String,
    description: String,
    expanded: bool,
    focused: bool,
    on_toggle: Message,
    body: Element<'a, Message>,
    scale: f32,
    palette: &'a Palette,
) -> Element<'a, Message> {
    use crate::donor_folio::source_ui::typography::label;
    let header = button(
        flow![rtl;
            label(
                if expanded { "▾" } else { "▸" },
                11.0,
                scale,
                with_alpha(palette.primary, 0.94),
            ),
            label(title, 13.5, scale, palette.surface_text),
            container(text("")).width(Length::Fill),
            label(
                summary,
                crate::donor_folio::source_ui::typography::TYPE_SMALL,
                scale,
                with_alpha(palette.surface_text, 0.58)
            ),
        ]
        .spacing(9.0 * scale)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([10.0 * scale, 10.0 * scale])
    .on_press(on_toggle)
    .style(move |_theme, status| {
        crate::donor_folio::source_ui::style::folio_line_button_style(
            expanded, focused, palette, 1.0, status,
        )
    });
    let mut details = column![header].spacing(0.0);
    if expanded {
        let content = column![
            label(
                description,
                crate::donor_folio::source_ui::typography::TYPE_SMALL,
                scale,
                with_alpha(palette.surface_text, 0.54)
            )
            .line_height(iced::widget::text::LineHeight::Relative(1.35)),
            crate::donor_folio::source_ui::layout::folio_horizontal_rule(with_alpha(
                palette.outline,
                0.42
            )),
            body,
        ]
        .spacing(12.0 * scale);
        details = details.push(
            container(content)
                .width(Length::Fill)
                .padding(12.0 * scale)
                .style(move |_| {
                    crate::donor_folio::source_ui::style::box_style(
                        with_alpha(palette.surface_container, 0.72),
                        with_alpha(palette.outline, 0.46),
                    )
                }),
        );
    }
    details.into()
}

pub fn folio_action_wrap<'a, Message: Clone + 'a>(
    rtl: bool,
    actions: Vec<(String, bool, Message)>,
    available: f32,
    scale: f32,
    palette: &'a Palette,
) -> Element<'a, Message> {
    let sized = actions
        .into_iter()
        .map(|action| (folio_action_width(&action.0, scale), action))
        .collect();
    let rows =
        crate::donor_folio::source_ui::layout::wrap_rows(sized, available.max(80.0), 6.0 * scale);
    let mut content = column![].spacing(6.0 * scale);
    for line_actions in rows {
        let mut line = flow![rtl;].spacing(6.0 * scale).align_y(Alignment::Center);
        for (action_label, active, message) in line_actions {
            let width = folio_action_width(&action_label, scale);
            line = line.push(folio_action(
                rtl,
                action_label,
                active,
                Some(message),
                Length::Fixed(width),
                scale * 0.9,
                palette,
            ));
        }
        content = content.push(line);
    }
    content.into()
}

#[derive(Debug, Default)]
struct FolioButtonState {
    mix: f32,
    target: bool,
    initialized: bool,
    last_frame: Option<Instant>,
}

impl FolioButtonState {
    fn advance(&mut self, active: bool, now: Instant) -> bool {
        if !self.initialized {
            self.mix = if active { 1.0 } else { 0.0 };
            self.target = active;
            self.initialized = true;
            self.last_frame = Some(now);
            return false;
        }
        if self.target != active {
            self.target = active;
            self.last_frame = Some(now);
        }
        let dt = self.last_frame.replace(now).map_or(0.0, |last| {
            now.saturating_duration_since(last).as_secs_f32().min(0.1)
        });
        let target = if self.target { 1.0 } else { 0.0 };
        if dt > 0.0 {
            self.mix = MotionProfile::default().approach(self.mix, target, dt, MotionTier::Fast);
        }
        if (self.mix - target).abs() < 0.002 {
            self.mix = target;
        }
        self.mix != target
    }
}

pub(crate) fn folio_diagonal_edges(width: f32, height: f32, mix: f32) -> (f32, f32) {
    let mix = mix.clamp(0.0, 1.0);
    if mix <= 0.0 {
        return (0.0, 0.0);
    }
    if mix >= 1.0 {
        return (width, width);
    }
    let slant = height * 0.72;
    let sweep = mix.mul_add(width + slant * 2.0, -slant);
    (
        (sweep - slant).clamp(0.0, width),
        (sweep + slant).clamp(0.0, width),
    )
}

pub(crate) fn folio_diagonal_wipe(rtl: bool, width: f32, height: f32, mix: f32) -> Path {
    let (top, bottom) = folio_diagonal_edges(width, height, mix);
    let origin = mirror_x_for(rtl, 0.0, 0.0, width);
    Path::new(|builder| {
        builder.move_to(Point::new(origin, 0.0));
        builder.line_to(Point::new(mirror_x_for(rtl, top, 0.0, width), 0.0));
        builder.line_to(Point::new(mirror_x_for(rtl, bottom, 0.0, width), height));
        builder.line_to(Point::new(origin, height));
        builder.close();
    })
}

struct FolioButtonFill {
    active: bool,
    destructive: bool,
    enabled: bool,
    rtl: bool,
    palette: Palette,
}

impl<Message> canvas::Program<Message> for FolioButtonFill {
    type State = FolioButtonState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            Event::Window(window::Event::RedrawRequested(now)) => state
                .advance(self.active && self.enabled, *now)
                .then(|| canvas::Action::request_redraw_at(*now + BUTTON_FRAME)),
            _ => None,
        }
    }

    fn draw(
        &self,
        state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let rtl = self.rtl;

        let mut frame = Frame::new(renderer, bounds.size());
        let hovered = self.enabled && cursor.is_over(bounds);
        frame.fill(
            &Path::rectangle(Point::ORIGIN, bounds.size()),
            if hovered {
                with_alpha(self.palette.surface_variant, 0.78)
            } else {
                with_alpha(
                    self.palette.surface_container,
                    if self.enabled { 0.92 } else { 0.42 },
                )
            },
        );
        let mix = super::motion::smoothstep(state.mix.clamp(0.0, 1.0));
        if self.enabled && mix > 0.0 {
            let fill = if self.destructive {
                self.palette.tertiary
            } else {
                self.palette.primary
            };
            frame.fill(
                &folio_diagonal_wipe(rtl, bounds.width, bounds.height, mix),
                with_alpha(fill, 0.96),
            );
            if mix < 1.0 {
                let (top, bottom) = folio_diagonal_edges(bounds.width, bounds.height, mix);
                frame.stroke(
                    &Path::line(
                        Point::new(mirror_x_for(rtl, top, 0.0, bounds.width), 0.0),
                        Point::new(mirror_x_for(rtl, bottom, 0.0, bounds.width), bounds.height),
                    ),
                    Stroke::default()
                        .with_color(with_alpha(self.palette.tertiary, 0.9))
                        .with_width(2.0),
                );
            }
        }
        vec![frame.into_geometry()]
    }
}

fn animated_action<'a, Message: Clone + 'a>(
    rtl: bool,
    label: impl text::IntoFragment<'a>,
    font: iced::Font,
    active: bool,
    destructive: bool,
    message: Option<Message>,
    width: Length,
    scale: f32,
    palette: &Palette,
) -> Element<'a, Message> {
    let palette = *palette;
    let enabled = message.is_some();
    let width = if matches!(width, Length::Shrink) {
        Length::Fixed(BUTTON_HEIGHT * scale.max(1.0))
    } else {
        width
    };
    let backdrop: Element<'a, Message> = iced::widget::canvas(FolioButtonFill {
        active,
        destructive,
        enabled,
        rtl,
        palette,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into();
    let label = container(
        text(label)
            .font(font)
            .size(11.0 * legible_type_scale(scale)),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(iced::alignment::Horizontal::Center)
    .align_y(iced::alignment::Vertical::Center);
    let control = button(label)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(0.0)
        .style(move |_theme, status| {
            let disabled = matches!(status, button::Status::Disabled);
            let hovered = matches!(status, button::Status::Hovered);
            let accent = if destructive {
                palette.tertiary
            } else {
                palette.primary
            };
            button::Style {
                background: Some(Color::TRANSPARENT.into()),
                text_color: if disabled {
                    with_alpha(palette.surface_text, 0.3)
                } else if active {
                    if destructive {
                        palette.background
                    } else {
                        palette.primary_text
                    }
                } else if destructive {
                    palette.tertiary
                } else {
                    palette.surface_text
                },
                border: Border {
                    color: if disabled {
                        with_alpha(palette.outline, 0.18)
                    } else if destructive || hovered || active {
                        with_alpha(accent, 0.86)
                    } else {
                        with_alpha(palette.outline, 0.4)
                    },
                    width: 1.0,
                    radius: 0.0.into(),
                },
                ..Default::default()
            }
        });
    let control = match message {
        Some(message) => control.on_press(message),
        None => control,
    };
    container(stack![backdrop, control])
        .width(width)
        .height(Length::Fixed(BUTTON_HEIGHT * scale.max(1.0)))
        .clip(true)
        .into()
}

pub fn folio_action<'a, Message: Clone + 'a>(
    rtl: bool,
    label: impl text::IntoFragment<'a>,
    active: bool,
    message: Option<Message>,
    width: Length,
    scale: f32,
    palette: &Palette,
) -> Element<'a, Message> {
    animated_action(
        rtl, label, UI_FONT, active, false, message, width, scale, palette,
    )
}

pub fn folio_close<'a, Message: Clone + 'a>(
    rtl: bool,
    message: Message,
    scale: f32,
    palette: &Palette,
) -> Element<'a, Message> {
    container(folio_action(
        rtl,
        "×",
        false,
        Some(message),
        Length::Shrink,
        scale,
        palette,
    ))
    .id("folio-close")
    .into()
}

pub fn folio_icon_action<'a, Message: Clone + 'a>(
    rtl: bool,
    glyph: &'a str,
    message: Option<Message>,
    scale: f32,
    palette: &'a Palette,
) -> Element<'a, Message> {
    animated_action(
        rtl,
        glyph,
        NERD_FONT,
        false,
        false,
        message,
        Length::Shrink,
        scale,
        palette,
    )
}

pub fn folio_destructive_action<'a, Message: Clone + 'a>(
    rtl: bool,
    label: impl text::IntoFragment<'a>,
    active: bool,
    message: Option<Message>,
    width: Length,
    scale: f32,
    palette: &'a Palette,
) -> Element<'a, Message> {
    animated_action(
        rtl, label, UI_FONT, active, true, message, width, scale, palette,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn diagonal_wipe_reaches_both_edges_and_mirrors_without_changing_logical_width() {
        assert_eq!(folio_diagonal_edges(120.0, 30.0, 0.0), (0.0, 0.0));
        assert_eq!(folio_diagonal_edges(120.0, 30.0, 1.0), (120.0, 120.0));
        let (top, bottom) = folio_diagonal_edges(120.0, 30.0, 0.5);
        assert!(top >= 0.0 && top <= 120.0 && bottom >= 0.0 && bottom <= 120.0);
        assert_ne!(
            top, bottom,
            "active intermediate wipe retains the diagonal edge"
        );
        assert_eq!(mirror_x_for(true, top, 0.0, 120.0), 120.0 - top);
        assert_eq!(mirror_x_for(false, bottom, 0.0, 120.0), bottom);
    }
    #[test]
    fn active_fill_reverses_from_presented_mix_and_stops_requesting_frames_when_settled() {
        let now = Instant::now();
        let mut state = FolioButtonState::default();
        assert!(!state.advance(false, now));
        assert!(state.advance(true, now + Duration::from_millis(1)));
        assert!(state.advance(true, now + Duration::from_millis(17)));
        let presented = state.mix;
        assert!(presented > 0.0 && presented < 1.0);
        state.advance(false, now + Duration::from_millis(18));
        assert_eq!(
            state.mix, presented,
            "reversal does not restart from a snapped endpoint"
        );
        for step in 2..200 {
            state.advance(false, now + Duration::from_millis(step * 16));
        }
        assert_eq!(state.mix, 0.0);
        assert!(!state.advance(false, now + Duration::from_secs(5)));
    }
}

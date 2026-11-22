use crate::donor_folio::source_ui::flow;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use iced::widget::canvas::{self, Frame, Path, Stroke};
use iced::widget::{button, column, container, mouse_area, stack, text, text_input};
use iced::{mouse, window, Alignment, Border, Color, Element, Event, Length, Point, Rectangle};

use crate::candidate_app::view::settings::folio::intent::Message;
use crate::donor_folio::source_ui::motion::{MotionProfile, MotionTier};
use crate::donor_folio::source_ui::with_alpha;
use crate::donor_folio::source_ui::{
    folio_diagonal_edges, folio_diagonal_wipe, label, legible_type_scale,
};
use crate::donor_ui::Palette;

use crate::candidate_app::view::settings::composed::{ActionId, Control, PRESET_NAME_KEY};
use crate::candidate_app::view::settings::folio::intent::SettingsMsg;

const BUTTON_FRAME: Duration = Duration::from_millis(16);
const BUTTON_HEIGHT: f32 = 30.0;
const BUTTON_MIN_WIDTH: f32 = 92.0;

fn button_width(label: &str, scale: f32) -> f32 {
    crate::donor_folio::source_ui::folio_action_width_in(
        label,
        scale,
        BUTTON_MIN_WIDTH,
        f32::INFINITY,
    )
}

// Preset names are user text: size and elide using the same shaped font we draw.
fn preset_name_label(name: &str, available_width: f32, scale: f32) -> (f32, String) {
    let font = crate::donor_folio::source_ui::UI_FONT;
    let size = 11.0 * legible_type_scale(scale);
    let inset = 14.0 * scale.max(0.95);
    let width = (crate::donor_back_layout::measured_width(name, size, font) + inset * 2.0)
        .max(BUTTON_MIN_WIDTH * scale)
        .min(available_width.max(0.0));
    let shown = crate::donor_back_layout::measured_lines(
        name,
        size,
        (width - inset * 2.0).max(0.0),
        font,
        1,
    )
    .into_iter()
    .next()
    .unwrap_or_default();
    (width, shown)
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum FillOwner {
    #[default]
    Control,
    Choice(String, String),
}

#[derive(Debug, Default)]
struct ButtonFillState {
    owner: FillOwner,
    mix: f32,
    target: bool,
    initialized: bool,
    last_frame: Option<Instant>,
}

impl ButtonFillState {
    fn mix_for(&self, owner: &FillOwner, active: bool) -> f32 {
        if self.initialized && &self.owner == owner {
            self.mix
        } else if active && *owner == FillOwner::Control {
            1.0
        } else {
            0.0
        }
    }

    fn advance(
        &mut self,
        owner: &FillOwner,
        active: bool,
        now: Instant,
        motion: MotionProfile,
    ) -> bool {
        if !self.initialized || &self.owner != owner {
            self.owner.clone_from(owner);
            self.mix = f32::from(active && *owner == FillOwner::Control);
            self.target = active;
            self.initialized = true;
            self.last_frame = Some(now);
            return active && self.mix == 0.0;
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
            self.mix = motion.approach(self.mix, target, dt, MotionTier::Fast);
        }
        if (self.mix - target).abs() < 0.002 {
            self.mix = target;
        }
        self.mix != target
    }
}

struct ButtonFill {
    owner: FillOwner,
    active: bool,
    destructive: bool,
    enabled: bool,
    rtl: bool,
    palette: Palette,
    fade: f32,
    motion: MotionProfile,
}

impl canvas::Program<Message> for ButtonFill {
    type State = ButtonFillState;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        _bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        match event {
            Event::Window(window::Event::RedrawRequested(now)) => state
                .advance(&self.owner, self.active && self.enabled, *now, self.motion)
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
        let mut frame = Frame::new(renderer, bounds.size());
        let hovered = self.enabled && cursor.is_over(bounds);
        {
            frame.fill(
                &Path::rectangle(Point::ORIGIN, bounds.size()),
                if hovered {
                    with_alpha(self.palette.surface_variant, 0.78 * self.fade)
                } else if !self.enabled {
                    with_alpha(self.palette.surface_container, 0.42 * self.fade)
                } else {
                    with_alpha(self.palette.surface_container, 0.92 * self.fade)
                },
            );
        }

        let mix = if self.enabled {
            crate::donor_folio::source_ui::motion::smoothstep(
                state.mix_for(&self.owner, self.active).clamp(0.0, 1.0),
            )
        } else {
            0.0
        };
        if mix > 0.0 {
            let fill = if self.destructive {
                with_alpha(self.palette.tertiary, 0.96 * self.fade)
            } else {
                with_alpha(self.palette.primary, 0.96 * self.fade)
            };
            frame.fill(
                &folio_diagonal_wipe(self.rtl, bounds.width, bounds.height, mix),
                fill,
            );
            if mix < 1.0 {
                let (top, bottom) = folio_diagonal_edges(bounds.width, bounds.height, mix);
                let top = crate::donor_folio::source_ui::direction::mirror_x_for(
                    self.rtl,
                    top,
                    0.0,
                    bounds.width,
                );
                let bottom = crate::donor_folio::source_ui::direction::mirror_x_for(
                    self.rtl,
                    bottom,
                    0.0,
                    bounds.width,
                );
                frame.stroke(
                    &Path::line(Point::new(top, 0.0), Point::new(bottom, bounds.height)),
                    Stroke::default()
                        .with_color(with_alpha(self.palette.tertiary, 0.9 * self.fade))
                        .with_width(2.0),
                );
            }
        }
        vec![frame.into_geometry()]
    }
}

fn overlay_style(
    active: bool,
    destructive: bool,
    focused: bool,
    fade: f32,
    palette: &Palette,
    status: button::Status,
) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    let disabled = matches!(status, button::Status::Disabled);
    button::Style {
        background: Some(Color::TRANSPARENT.into()),
        text_color: if disabled {
            with_alpha(palette.surface_text, 0.34 * fade)
        } else if active {
            if destructive {
                with_alpha(palette.background, fade)
            } else {
                with_alpha(palette.primary_text, fade)
            }
        } else if destructive {
            with_alpha(palette.tertiary, fade)
        } else {
            with_alpha(palette.surface_text, 0.9 * fade)
        },
        border: Border {
            color: if disabled {
                with_alpha(palette.outline, 0.18 * fade)
            } else if destructive {
                with_alpha(
                    palette.tertiary,
                    if hovered || focused {
                        fade
                    } else {
                        0.62 * fade
                    },
                )
            } else {
                with_alpha(
                    if focused || hovered {
                        palette.primary
                    } else {
                        palette.outline
                    },
                    if focused {
                        fade
                    } else if hovered {
                        0.76 * fade
                    } else {
                        0.4 * fade
                    },
                )
            },
            width: if focused && !disabled { 2.0 } else { 1.0 },
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

fn fixed_button<'a>(
    catalog: &crate::candidate_app::localization::Catalog,
    label: String,
    active: bool,
    destructive: bool,
    message: Option<Message>,
    focused: bool,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    width: f32,
    motion: MotionProfile,
) -> Element<'a, Message> {
    fixed_button_content(
        catalog,
        text(label)
            .font(crate::donor_folio::source_ui::UI_FONT)
            .size(11.0 * legible_type_scale(scale)),
        active,
        destructive,
        message,
        focused,
        scale,
        palette,
        fade,
        width,
        motion,
    )
}

fn fixed_button_content<'a>(
    catalog: &crate::candidate_app::localization::Catalog,
    label: iced::widget::Text<'a>,
    active: bool,
    destructive: bool,
    message: Option<Message>,
    focused: bool,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    width: f32,
    motion: MotionProfile,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let enabled = message.is_some();
    let backdrop: Element<'a, Message> = iced::widget::canvas(ButtonFill {
        owner: match message.as_ref() {
            Some(Message::Settings(SettingsMsg::Pick(path, key))) => {
                FillOwner::Choice(path.clone(), key.clone())
            }
            _ => FillOwner::Control,
        },
        rtl,
        active,
        destructive,
        enabled,
        palette: *palette,
        fade,
        motion,
    })
    .width(Length::Fill)
    .height(Length::Fill)
    .into();
    let label = container(label)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(iced::alignment::Horizontal::Center)
        .align_y(iced::alignment::Vertical::Center);
    let hit_target = button(label)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(0.0)
        .style(move |_theme, status| {
            overlay_style(active, destructive, focused, fade, palette, status)
        });
    let hit_target = if let Some(message) = message {
        hit_target.on_press(message)
    } else {
        hit_target
    };
    container(stack![backdrop, hit_target])
        .width(Length::Fixed(width))
        .height(Length::Fixed(BUTTON_HEIGHT * scale.max(1.0)))
        .clip(true)
        .into()
}

fn option_button<'a>(
    catalog: &crate::candidate_app::localization::Catalog,
    label: String,
    active: bool,
    destructive: bool,
    message: Message,
    focused: bool,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    motion: MotionProfile,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let width = button_width(&label, scale);
    fixed_button(
        catalog,
        label,
        active,
        destructive,
        Some(message),
        focused,
        scale,
        palette,
        fade,
        width,
        motion,
    )
}

#[allow(clippy::too_many_arguments)]
fn choice_buttons<'a>(
    catalog: &crate::candidate_app::localization::Catalog,
    path: &str,
    options: Vec<(String, String)>,
    current: &str,
    disabled: &[String],
    palettes: &[(String, Vec<String>)],
    keyboard_focused: bool,
    focused_choice: Option<usize>,
    available_width: f32,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    motion: MotionProfile,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let items = options
        .into_iter()
        .enumerate()
        .map(|(index, (key, label))| {
            let width = button_width(&label, scale);
            let enabled = !disabled.contains(&key);
            let active = key == current;
            let colors = palettes
                .iter()
                .rev()
                .find(|(name, _)| name == &key)
                .map(|(_, colors)| colors);
            let message =
                enabled.then(|| Message::Settings(SettingsMsg::Pick(path.to_owned(), key)));
            let item = fixed_button(
                catalog,
                label,
                active,
                false,
                message,
                enabled && keyboard_focused && focused_choice == Some(index),
                scale,
                palette,
                fade,
                width,
                motion,
            );
            let item = if let Some(colors) = colors {
                let mut strip = flow![rtl;].width(Length::Fixed(width));
                for color in colors {
                    let color = crate::donor_folio::source_ui::color::parse_hex(color)
                        .unwrap_or(Color::TRANSPARENT);
                    strip = strip.push(
                        container(text(""))
                            .width(Length::Fill)
                            .height(Length::Fixed(10.0 * scale))
                            .style(move |_| {
                                crate::donor_folio::source_ui::bg_style(iced::Background::Color(
                                    color,
                                ))
                            }),
                    );
                }
                column![item, strip].into()
            } else {
                item
            };
            (width, item)
        })
        .collect();
    wrapped_options(catalog, items, available_width, scale)
}

fn wrapped_options<'a>(
    catalog: &crate::candidate_app::localization::Catalog,
    items: Vec<(f32, Element<'a, Message>)>,
    available_width: f32,
    scale: f32,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let lines = crate::donor_folio::source_ui::wrap_rows(items, available_width, 7.0 * scale);
    let mut group = column![].spacing(7.0 * scale);
    for line in lines {
        let mut line_row = flow![rtl;].spacing(7.0 * scale).align_y(Alignment::Center);
        for item in line {
            line_row = line_row.push(item);
        }
        group = group.push(line_row);
    }
    group.into()
}

pub(super) fn widget<'a>(
    catalog: &crate::candidate_app::localization::Catalog,
    control: Control,
    values: &'a HashMap<String, String>,
    has_selected_preset: bool,
    keyboard_focused: bool,
    focused_choice: Option<usize>,
    available_width: f32,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    armed: Option<ActionId>,
    motion: MotionProfile,
    reveals: Option<&HashMap<String, crate::donor_folio::source_ui::motion::Tween>>,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    match control {
        Control::Display(control) => crate::candidate_app::view::settings::folio::display_view(
            catalog, control, scale, palette,
        ),
        Control::AppTheme { app } => label(
            app.name,
            crate::donor_folio::source_ui::TYPE_SMALL,
            scale,
            palette.surface_text,
        )
        .into(),
        Control::Toggle { path, value } => fixed_button(
            catalog,
            if value {
                tr("settings-control-enabled").into()
            } else {
                tr("settings-control-disabled").into()
            },
            value,
            false,
            Some(Message::Settings(SettingsMsg::Toggle(path, !value))),
            keyboard_focused,
            scale,
            palette,
            fade,
            button_width(tr("settings-control-enabled"), scale)
                .max(button_width(tr("settings-control-disabled"), scale)),
            motion,
        ),
        Control::ToggleAction { id, value } => fixed_button(
            catalog,
            if value {
                tr("settings-app-themes-on").into()
            } else {
                tr("settings-app-themes-off").into()
            },
            value,
            false,
            Some(Message::Settings(SettingsMsg::Run(id))),
            keyboard_focused,
            scale,
            palette,
            fade,
            button_width(tr("settings-app-themes-on"), scale)
                .max(button_width(tr("settings-app-themes-off"), scale)),
            motion,
        ),
        Control::Number { key, unit, .. } => {
            let value = values.get(&key).map_or("", String::as_str);
            let automatic =
                key == crate::candidate_app::view::settings::composed::keys::transition::FPS;
            let value = if automatic && value.parse::<f64>() == Ok(0.0) {
                ""
            } else {
                value
            };
            let placeholder = if automatic {
                tr("settings-transitions-rate-auto")
            } else {
                "0"
            };
            let message_key = key.clone();
            flow![rtl;
                text_input(placeholder, value)
                    .id(crate::candidate_app::view::settings::composed::workbench_input_id(&key))
                    .font(crate::donor_folio::source_ui::UI_FONT)
                    .on_input(move |raw| Message::Settings(SettingsMsg::Input(
                        message_key.clone(),
                        raw,
                    )))
                    .on_submit(Message::Settings(SettingsMsg::Commit))
                    .size(16.0 * legible_type_scale(scale))
                    .padding([7.0 * scale, 10.0 * scale])
                    .width(Length::Fill)
                    .style(move |_theme, _status| {
                        crate::donor_folio::source_ui::workbench_input_style(
                            with_alpha(palette.surface_text, fade),
                            with_alpha(palette.primary, fade),
                        )
                    }),
                label(
                    unit,
                    crate::donor_folio::source_ui::TYPE_SMALL,
                    scale,
                    with_alpha(palette.surface_text, 0.56 * fade)
                ),
            ]
            .spacing(8.0 * scale)
            .align_y(Alignment::End)
            .into()
        }
        Control::KeyBinding { path, default, .. } => {
            let value = values.get(&path).map_or("", String::as_str);
            let triggers =
                crate::candidate_app::view::settings::composed::input::parse_binding(value)
                    .or_else(|| {
                        crate::candidate_app::view::settings::composed::input::parse_binding(
                            default,
                        )
                    })
                    .unwrap_or_default();
            let pal = *palette;
            let mut slots = flow![rtl;].spacing(6.0 * scale);
            for slot in 0..crate::candidate_app::view::settings::composed::input::SLOTS {
                let shown = crate::candidate_app::view::settings::composed::input::slot_triggers(
                    &triggers, slot,
                );
                let (display, alpha) = if shown.is_empty() {
                    (tr("settings-keybind-unbound").to_string(), 0.4)
                } else {
                    (
                        crate::candidate_app::view::settings::composed::input::binding_label(shown),
                        1.0,
                    )
                };
                let slot_focused = keyboard_focused && focused_choice == Some(slot);
                slots = slots.push(
                    button(
                        text(display)
                            .font(crate::donor_folio::source_ui::UI_FONT)
                            .size(13.0 * legible_type_scale(scale))
                            .color(with_alpha(palette.surface_text, alpha * fade)),
                    )
                    .padding([8.0 * scale, 10.0 * scale])
                    .width(Length::Fill)
                    .on_press(Message::Settings(SettingsMsg::KeybindCapture(
                        path.clone(),
                        slot,
                    )))
                    .style(move |_theme, status| {
                        crate::donor_folio::source_ui::folio_button_style(
                            slot_focused,
                            false,
                            &pal,
                            status,
                        )
                    }),
                );
            }
            slots.into()
        }
        Control::TextField {
            key, placeholder, ..
        } => {
            let value = values.get(&key).map_or("", String::as_str);
            let enabled = key != PRESET_NAME_KEY || has_selected_preset;
            let hint = if enabled {
                placeholder
            } else {
                tr("settings-selector-preset-rename-hint")
            };
            let mut input = text_input(hint, value)
                .id(crate::candidate_app::view::settings::composed::workbench_input_id(&key))
                .font(crate::donor_folio::source_ui::UI_FONT)
                .size(13.0 * legible_type_scale(scale))
                .padding([8.0 * scale, 10.0 * scale])
                .width(Length::Fill)
                .style(move |_theme, _status| {
                    crate::donor_folio::source_ui::workbench_input_style(
                        with_alpha(palette.surface_text, fade),
                        with_alpha(palette.primary, fade),
                    )
                });
            if enabled {
                if key == PRESET_NAME_KEY {
                    let selected = source_profile_selection(values);
                    if let Some((mode, id)) = selected {
                        input =
                            input.on_input(move |raw| Message::PresetNameInput { mode, id, raw });
                    }
                } else {
                    let message_key = key.clone();
                    input = input
                        .on_input(move |raw| {
                            Message::Settings(SettingsMsg::Input(message_key.clone(), raw))
                        })
                        .on_submit(Message::Settings(SettingsMsg::Commit));
                }
            }
            input.into()
        }
        Control::Resolution {
            key,
            width_placeholder,
            height_placeholder,
            ..
        } => {
            let value = values.get(&key).map_or("", String::as_str);
            let maximum = key.ends_with(".to");
            if maximum && value.is_empty() {
                return flow![rtl;
                    label(
                        tr("settings-filter-resolution-unlimited"),
                        13.0,
                        scale,
                        with_alpha(palette.surface_text, fade)
                    ),
                    option_button(catalog,
                        tr("settings-filter-resolution-set-limit").into(),
                        false,
                        false,
                        Message::Settings(SettingsMsg::ResolutionLimit(key, true)),
                        keyboard_focused,
                        scale,
                        palette,
                        fade,
                        motion,
                    ),
                ]
                .spacing(12.0 * scale)
                .align_y(Alignment::Center)
                .into();
            }
            let (width, height) = value.split_once(['x', 'X', '×']).unwrap_or(("", ""));
            let width_key = key.clone();
            let height_key = key.clone();
            let width_input = text_input(width_placeholder, width)
                .id(
                    crate::candidate_app::view::settings::composed::workbench_input_id(&format!(
                        "{key}.width"
                    )),
                )
                .font(crate::donor_folio::source_ui::UI_FONT)
                .on_input(move |raw| {
                    Message::Settings(SettingsMsg::ResolutionInput(width_key.clone(), true, raw))
                })
                .on_submit(Message::Settings(SettingsMsg::Commit))
                .size(13.0 * legible_type_scale(scale))
                .padding([8.0 * scale, 10.0 * scale])
                .width(Length::Fill)
                .style(move |_theme, _status| {
                    crate::donor_folio::source_ui::workbench_input_style(
                        with_alpha(palette.surface_text, fade),
                        with_alpha(palette.primary, fade),
                    )
                });
            let height_input = text_input(height_placeholder, height)
                .id(
                    crate::candidate_app::view::settings::composed::workbench_input_id(&format!(
                        "{key}.height"
                    )),
                )
                .font(crate::donor_folio::source_ui::UI_FONT)
                .on_input(move |raw| {
                    Message::Settings(SettingsMsg::ResolutionInput(height_key.clone(), false, raw))
                })
                .on_submit(Message::Settings(SettingsMsg::Commit))
                .size(13.0 * legible_type_scale(scale))
                .padding([8.0 * scale, 10.0 * scale])
                .width(Length::Fill)
                .style(move |_theme, _status| {
                    crate::donor_folio::source_ui::workbench_input_style(
                        with_alpha(palette.surface_text, fade),
                        with_alpha(palette.primary, fade),
                    )
                });
            let dimensions = flow![rtl;
                column![
                    label(
                        width_placeholder,
                        11.0,
                        scale,
                        with_alpha(palette.surface_text, 0.68 * fade)
                    ),
                    width_input
                ]
                .spacing(4.0 * scale)
                .width(Length::Fill),
                label("×", 14.0, scale, with_alpha(palette.surface_text, 0.56 * fade)),
                column![
                    label(
                        height_placeholder,
                        11.0,
                        scale,
                        with_alpha(palette.surface_text, 0.68 * fade)
                    ),
                    height_input
                ]
                .spacing(4.0 * scale)
                .width(Length::Fill),
            ]
            .spacing(8.0 * scale)
            .align_y(Alignment::End);
            let mut editor = column![dimensions].spacing(8.0 * scale);
            if maximum {
                editor = editor.push(option_button(
                    catalog,
                    tr("settings-filter-resolution-clear-limit").into(),
                    false,
                    false,
                    Message::Settings(SettingsMsg::ResolutionLimit(key.clone(), false)),
                    false,
                    scale,
                    palette,
                    fade,
                    motion,
                ));
            }
            let base = key.rsplit_once('.').map_or("", |(base, _)| base);
            let from = values
                .get(&format!("{base}.from"))
                .map_or("", String::as_str);
            let to = values.get(&format!("{base}.to")).map_or("", String::as_str);
            if !resolution_bounds_valid(from, to) {
                editor = editor.push(label(
                    tr("settings-filter-resolution-invalid"),
                    11.0,
                    scale,
                    with_alpha(palette.primary, fade),
                ));
            }
            editor.into()
        }
        Control::Dropdown {
            path,
            options,
            current,
            palettes,
        } => choice_buttons(
            catalog,
            &path,
            options,
            &current,
            &[],
            &palettes,
            keyboard_focused,
            focused_choice,
            available_width,
            scale,
            palette,
            fade,
            motion,
        ),
        Control::Chips {
            path,
            options,
            current,
            disabled,
            background,
        } => {
            let choices = choice_buttons(
                catalog,
                &path,
                options,
                &current,
                &disabled,
                &[],
                keyboard_focused,
                focused_choice,
                available_width,
                scale,
                palette,
                fade,
                motion,
            );
            if let Some(background) = background {
                let shown = matches!(current.as_str(), "center" | "fit");
                let reveal = reveals
                    .and_then(|map| map.get(&format!("background:{path}")))
                    .map_or(if shown { 1.0 } else { 0.0 }, |value| value.x);
                let picker = reveals
                    .and_then(|map| map.get(&background.picker_id()))
                    .map_or(0.0, |value| value.x);
                column![
                    choices,
                    crate::candidate_app::view::settings::folio::background_view(
                        catalog,
                        &background,
                        reveal,
                        picker,
                        scale,
                        palette
                    )
                ]
                .into()
            } else {
                choices
            }
        }
        Control::MotionWeights { weights } => {
            let mut group = flow![rtl;].spacing(8.0 * scale);
            for (label, key, reset) in weights {
                let value = values.get(&key).map_or("", String::as_str);
                let message_key = key.clone();
                group = group.push(
                    column![
                        crate::donor_folio::source_ui::label(
                            label,
                            10.0,
                            scale,
                            with_alpha(palette.surface_text, 0.62 * fade),
                        ),
                        flow![rtl;
                            text_input("0", value)
                                .id(crate::candidate_app::view::settings::composed::workbench_input_id(&key))
                                .font(crate::donor_folio::source_ui::UI_FONT)
                                .on_input(move |raw| Message::Settings(SettingsMsg::Input(
                                    message_key.clone(),
                                    raw,
                                )))
                                .on_submit(Message::Settings(SettingsMsg::Commit))
                                .size(13.0 * legible_type_scale(scale))
                                .padding([6.0 * scale, 8.0 * scale])
                                .style(move |_theme, _status| {
                                    crate::donor_folio::source_ui::workbench_input_style(
                                        with_alpha(palette.surface_text, fade),
                                        with_alpha(palette.primary, fade),
                                    )
                                }),
                            fixed_button(catalog,
                                "↺".into(),
                                false,
                                false,
                                Some(Message::Settings(SettingsMsg::Run(reset))),
                                false,
                                scale * 0.85,
                                palette,
                                fade,
                                34.0 * scale,
                                motion,
                            ),
                        ]
                        .spacing(5.0 * scale),
                    ]
                    .spacing(5.0 * scale)
                    .width(Length::FillPortion(1)),
                );
            }
            group.into()
        }
        Control::ActionBtn { id, label } => {
            let confirming = armed == Some(id);
            let confirmation = tr_args!(catalog, "settings-control-confirm", label => &label);
            let width = button_width(&confirmation, scale).max(button_width(&label, scale));
            fixed_button(
                catalog,
                if confirming { confirmation } else { label },
                confirming,
                confirming,
                Some(Message::Settings(SettingsMsg::Run(id))),
                keyboard_focused,
                scale,
                palette,
                fade,
                width,
                motion,
            )
        }
        Control::ActionChips { items } => wrapped_options(
            catalog,
            items
                .into_iter()
                .enumerate()
                .map(|(index, (id, label))| {
                    let width = button_width(&label, scale);
                    (
                        width,
                        fixed_button(
                            catalog,
                            label,
                            false,
                            false,
                            Some(Message::Settings(SettingsMsg::Run(id))),
                            keyboard_focused && focused_choice == Some(index),
                            scale,
                            palette,
                            fade,
                            width,
                            motion,
                        ),
                    )
                })
                .collect(),
            available_width,
            scale,
        ),
        Control::Presets { mode, items } => {
            let mut options = vec![(
                button_width(tr("settings-selector-preset-save-current"), scale),
                option_button(
                    catalog,
                    tr("settings-selector-preset-save-current").into(),
                    false,
                    false,
                    Message::SavePreset(mode),
                    keyboard_focused && focused_choice == Some(0),
                    scale,
                    palette,
                    fade,
                    motion,
                ),
            )];
            let mut choice_index = 1;
            for entry in items {
                let name = entry.name;
                let active = entry.selected;
                let id = entry.id;
                let apply_index = choice_index;
                let rename_index = apply_index + 1;
                let delete_index = apply_index + if entry.available { 1 } else { 2 };
                choice_index = delete_index + 1;
                let gap = 7.0 * scale;
                let delete_width = 34.0 * scale;
                let (width, shown_name) =
                    preset_name_label(&name, available_width - delete_width - gap, scale);
                let name_button = fixed_button_content(
                    catalog,
                    text(shown_name)
                        .font(crate::donor_folio::source_ui::UI_FONT)
                        .size(11.0 * legible_type_scale(scale))
                        .shaping(iced::widget::text::Shaping::Advanced)
                        .wrapping(iced::widget::text::Wrapping::None),
                    active,
                    false,
                    entry.available.then_some(Message::ApplyPreset(mode, id)),
                    keyboard_focused && focused_choice == Some(apply_index),
                    scale,
                    palette,
                    fade,
                    width,
                    motion,
                );
                let delete_button = fixed_button(
                    catalog,
                    "×".into(),
                    false,
                    true,
                    Some(Message::DeletePreset(mode, id)),
                    keyboard_focused && focused_choice == Some(delete_index),
                    scale,
                    palette,
                    fade,
                    delete_width,
                    motion,
                );
                let pair_width = width + gap + delete_width;
                let pair = flow![rtl; name_button, delete_button].spacing(gap);
                if entry.available {
                    options.push((pair_width, pair.into()));
                } else {
                    // Keep the unavailable record's extra action with its name/delete pair.
                    let rename_width = (210.0 * scale).min(available_width.max(0.0));
                    let rename = fixed_button(
                        catalog,
                        "Select for rename · Apply unavailable".into(),
                        active,
                        false,
                        Some(Message::SelectPresetForRename(mode, id)),
                        keyboard_focused && focused_choice == Some(rename_index),
                        scale,
                        palette,
                        fade,
                        rename_width,
                        motion,
                    );
                    options.push((
                        pair_width.max(rename_width),
                        column![pair, rename].spacing(gap).into(),
                    ));
                }
            }
            wrapped_options(catalog, options, available_width, scale)
        }
        Control::Details { summary, .. } | Control::StackBar { summary, .. } => label(
            format!("▸ {summary}"),
            11.0,
            scale,
            with_alpha(palette.surface_text, 0.68 * fade),
        )
        .into(),
        Control::Segment | Control::Static | Control::PlatformForm => text("").into(),
        Control::Code { snippet } => label(
            snippet,
            crate::donor_folio::source_ui::TYPE_SMALL,
            scale,
            with_alpha(palette.surface_text, 0.68 * fade),
        )
        .into(),
    }
}

pub fn resolution_bounds_valid(from: &str, to: &str) -> bool {
    let Some(from) =
        crate::candidate_app::view::settings::composed::catalog::parse_resolution(from)
    else {
        return false;
    };
    if to.trim().is_empty() {
        return true;
    }
    let Some(to) = crate::candidate_app::view::settings::composed::catalog::parse_resolution(to)
    else {
        return false;
    };
    from.0.max(from.1) <= to.0.max(to.1) && from.0.min(from.1) <= to.0.min(to.1)
}

fn source_profile_selection(
    values: &HashMap<String, String>,
) -> Option<(crate::candidate_state::GalleryKind, u64)> {
    let mode = crate::candidate_state::GalleryKind::from_key(values.get("profile-owner-mode")?)?;
    let id = values.get("profile-owner-id")?.parse().ok()?;
    Some((mode, id))
}

#[cfg(test)]
mod fill_tests {
    use super::*;
    #[test]
    fn w5_button_fill_reverses_and_rebinds_by_setting_and_value() {
        let mut state = ButtonFillState::default();
        let motion = MotionProfile::default();
        let origin = Instant::now();
        let at = |ms| origin + Duration::from_millis(ms);
        let launch = FillOwner::Choice("motion.launchSpeed".into(), "fast".into());
        let picker = FillOwner::Choice("motion.filterSwapSpeed".into(), "fast".into());
        let slow = FillOwner::Choice("motion.filterSwapSpeed".into(), "slow".into());

        assert!(!state.advance(&launch, false, at(0), motion));
        assert!(state.advance(&launch, true, at(16), motion));
        assert!(state.advance(&launch, true, at(48), motion));
        let middle = state.mix_for(&launch, true);
        assert!(0.0 < middle && middle < 1.0);

        assert!(state.advance(&launch, false, at(64), motion));
        assert_eq!(state.mix_for(&launch, false), middle);
        assert!(state.advance(&launch, false, at(80), motion));
        let reverse = state.mix_for(&launch, false);
        assert!(0.0 < reverse && reverse < middle);

        assert_eq!(state.mix_for(&picker, true), 0.0);
        assert!(state.advance(&picker, true, at(96), motion));
        assert!(state.advance(&picker, true, at(112), motion));
        assert!((0.0..1.0).contains(&state.mix_for(&picker, true)));
        assert!(!state.advance(&slow, false, at(128), motion));
        assert_eq!(state.mix_for(&slow, false), 0.0);
    }
}

#[cfg(test)]
mod preset_name_tests {
    use super::*;

    fn fonts() {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .unwrap();
        fonts.load_font(std::borrow::Cow::Borrowed(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/RobotoCondensed-Bold-UI.ttf"
        ))));
        fonts.load_font(std::borrow::Cow::Borrowed(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/fonts/RobotoCondensed-Bold-SC.ttf"
        ))));
    }

    #[test]
    fn preset_name_shaped_labels_fit_and_preserve_full_names() {
        fonts();
        let cjk = "夜晚海風森林城市晨光花園星空山水";
        let long =
            "Evening 海風 cafe\u{301} 2026 — a very long preset name for the narrow layout studio "
                .repeat(4);
        for scale in [1.0, 0.5] {
            for available in [310.0, 180.0, 92.0] {
                for name in ["Style 1", cjk, &long] {
                    let original = name.to_owned();
                    let (width, shown) = preset_name_label(name, available, scale);
                    let inset = 14.0 * scale.max(0.95);
                    assert!(width <= available);
                    assert!(!shown.contains('\n'));
                    assert!(
                        crate::donor_back_layout::measured_width(
                            &shown,
                            11.0 * legible_type_scale(scale),
                            crate::donor_folio::source_ui::UI_FONT
                        ) <= width - 2.0 * inset + 0.01
                    );
                    assert_eq!(name, original);
                    if name == "Style 1" || (name == cjk && available == 310.0) {
                        assert_eq!(shown, name, "ordinary and boundedCJK names remain whole");
                    }
                    if name == long {
                        assert!(shown.ends_with('…'), "overflow is explicit");
                    }
                }
            }
        }
    }

    #[test]
    #[ignore = "Owned preset button layout/render/hit contract; explicit invocation"]
    fn preset_name_actual_control_layout_and_identity() {
        use iced::advanced::{
            layout,
            renderer::{Headless, Renderer as _},
            widget::{Operation, Tree},
            Layout,
        };
        #[derive(Default)]
        struct Labels(Vec<(String, Rectangle)>);
        impl Operation for Labels {
            fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
                visit(self);
            }
            fn text(&mut self, _: Option<&iced::widget::Id>, bounds: Rectangle, value: &str) {
                self.0.push((value.into(), bounds));
            }
        }
        fonts();
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            crate::donor_folio::source_ui::UI_FONT,
            iced::Pixels(16.0),
            Some("wgpu"),
        ))
        .unwrap();
        let output =
            std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned output"));
        std::fs::create_dir_all(&output).unwrap();
        for scale in [1.0, 0.5] {
            for width in [396.0, 310.0, 180.0, 92.0] {
                for available in [true, false] {
                    for (case, name) in [
                        "Style 1",
                        "夜晚海風森林城市晨光花園星空山水",
                        "Evening 海風 cafe\u{301} — a very long preset name for the narrow layout studio",
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let catalog = crate::candidate_app::localization::Catalog::default();
                        let palette = Palette::from_rgb([0.1; 3], [0.2; 3], [0.9; 3], [0.3,0.6,0.8]);
                        let values = HashMap::new();
                        let control_scale = scale * 0.94;
                        let mut element = widget(&catalog, Control::Presets { mode: crate::candidate_state::GalleryKind::Slices, items: vec![crate::candidate_state::mode_profiles::ProfileRow {id: 730, name: name.into(), selected:true, available}, crate::candidate_state::mode_profiles::ProfileRow {id:731, name:"Following preset".into(), selected:false, available:true}] }, &values, true, false, None, width, control_scale, &palette, 1.0, None, MotionProfile::default(), None);
                        let size = iced::Size::new(width,360.0);
                        let viewport = Rectangle::with_size(size);
                        let mut tree = Tree::new(&element);
                        let node = element.as_widget_mut().layout(&mut tree, &renderer, &layout::Limits::new(iced::Size::ZERO,size));
                        let mut labels = Labels::default();
                        element.as_widget_mut().operate(&mut tree,Layout::new(&node),&renderer,&mut labels);
                        let (shown, b) = &labels.0[1]; // Save current is first, followed by the first actual preset label.
                        let b = *b;
                        assert!(b.x>=0.0 && b.x+b.width<=width+0.1);
                        assert!(b.height <= 30.0+0.1, "one bounded line must fit button height");
                        assert!(shown == name || shown.ends_with('…') || shown.is_empty());
                        assert!(crate::donor_back_layout::measured_width(shown,11.0*legible_type_scale(control_scale),crate::donor_folio::source_ui::UI_FONT) <= b.width +0.1);
                        let deletes: Vec<_> = labels.0.iter().filter(|(value,_)| value == "×").map(|(_,bounds)| *bounds).collect();
                        assert_eq!(deletes.len(), 2);
                        assert!((b.center().y - deletes[0].center().y).abs() < 0.1, "name and delete stay on the same row");
                        assert!(deletes[0].x > b.x && deletes[0].x + deletes[0].width <= width + 0.1);
                        let following_delete_index = labels.0.iter().enumerate().filter(|(_, (value, _))| value == "×").nth(1).unwrap().0;
                        let following = labels.0[following_delete_index - 1].1;
                        assert!((following.center().y - deletes[1].center().y).abs() < 0.1, "following pair wraps whole");
                        if width <= 180.0 {
                            assert!(deletes[1].y > deletes[0].y, "narrow rows wrap complete pairs");
                        }
                        let mut messages=Vec::new();
                        for event in [iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)] {
                            element.as_widget_mut().update(&mut tree,&Event::Mouse(event),Layout::new(&node),iced::mouse::Cursor::Available(b.center()),&mut renderer,&mut iced::advanced::clipboard::Null,&mut iced::advanced::Shell::new(&mut messages),&viewport);
                        }
                        assert_eq!(messages.iter().any(|m|matches!(m,Message::ApplyPreset(crate::candidate_state::GalleryKind::Slices,730))), available, "visible label keeps exact preset hit ID");
                        for (bounds, id) in deletes.iter().zip([730,731]) {
                            messages.clear();
                            for event in [iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)] {
                                element.as_widget_mut().update(&mut tree,&Event::Mouse(event),Layout::new(&node),iced::mouse::Cursor::Available(bounds.center()),&mut renderer,&mut iced::advanced::clipboard::Null,&mut iced::advanced::Shell::new(&mut messages),&viewport);
                            }
                            assert!(messages.iter().any(|m|matches!(m,Message::DeletePreset(crate::candidate_state::GalleryKind::Slices,target) if *target==id)));
                        }
                        if !available {
                            let rename = labels.0.iter().find(|(value,_)|value=="Select for rename · Apply unavailable").unwrap().1;
                            assert!(rename.y > b.y);
                            messages.clear();
                            for event in [iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)] {
                                element.as_widget_mut().update(&mut tree,&Event::Mouse(event),Layout::new(&node),iced::mouse::Cursor::Available(rename.center()),&mut renderer,&mut iced::advanced::clipboard::Null,&mut iced::advanced::Shell::new(&mut messages),&viewport);
                            }
                            assert!(messages.iter().any(|m|matches!(m,Message::SelectPresetForRename(crate::candidate_state::GalleryKind::Slices,730))));
                        }
                        renderer.reset(viewport);
                        element.as_widget().draw(&tree,&mut renderer,&iced::Theme::Dark,&iced::advanced::renderer::Style {text_color:palette.surface_text},Layout::new(&node),iced::mouse::Cursor::Unavailable,&viewport);
                        let pixels=iced::Size::new(width as u32,360);
                        let bytes=Headless::screenshot(&mut renderer,pixels,1.0,Color::BLACK);
                        ::image::save_buffer_with_format(output.join(format!("preset-scale{scale}-width{width}-available{available}-case{case}.png")),&bytes,pixels.width,pixels.height,::image::ColorType::Rgba8,::image::ImageFormat::Png).unwrap();
                    }
                }
            }
        }
    }
}

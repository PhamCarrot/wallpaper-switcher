use crate::candidate_app::view::settings::composed::background::BackgroundControl;
use crate::donor_folio::source_ui::flow;
use iced::widget::{button, canvas, column, container, text};
use iced::{Color, Element, Length};

use crate::candidate_app::view::settings::composed::{keys, SettingsSource};
use crate::candidate_app::view::settings::folio::intent::Message;
use crate::donor_folio::source_ui::color::{hex_to_hsv, hsv_to_hex};
use crate::donor_folio::source_ui::label;
use crate::donor_ui::Palette;

use crate::candidate_app::view::settings::folio::intent::SettingsMsg;

impl BackgroundControl {
    pub fn button_width<'a>(
        catalog: &crate::candidate_app::localization::Catalog,
        scale: f32,
    ) -> f32 {
        let tr = |key| catalog.text_static(key);
        let rtl = catalog.language.rtl;

        let width = crate::donor_folio::source_ui::folio_action_width(
            tr("settings-background-color"),
            scale,
        ) + 28.0 * scale;
        width
            .max(92.0 * scale)
            .max(crate::donor_folio::source_ui::folio_action_width(
                tr("settings-background-blur"),
                scale,
            ))
            .max(crate::donor_folio::source_ui::folio_action_width(
                tr("settings-background-inherit"),
                scale,
            ))
    }

    pub fn choices_width<'a>(
        catalog: &crate::candidate_app::localization::Catalog,
        scale: f32,
    ) -> f32 {
        let tr = |key| catalog.text_static(key);
        let rtl = catalog.language.rtl;

        72.0 * scale + 3.0 * Self::button_width(catalog, scale) + 30.0 * scale
    }

    pub fn picker_id(&self) -> String {
        format!("background-picker:{}", self.output)
    }

    pub fn view<'a>(
        &self,
        catalog: &crate::candidate_app::localization::Catalog,
        reveal: f32,
        picker: f32,
        scale: f32,
        palette: &'a Palette,
    ) -> Element<'a, Message> {
        let tr = |key| catalog.text_static(key);
        let rtl = catalog.language.rtl;

        let swatch =
            crate::donor_folio::source_ui::color::parse_hex(&self.color).unwrap_or(Color::BLACK);
        let solid = self.mode != "blur";
        let width = Self::button_width(catalog, scale);
        let color = button(
            container(
                flow![rtl;
                    container(text(""))
                        .width(Length::Fixed(20.0 * scale))
                        .height(Length::Fixed(20.0 * scale))
                        .style(move |_| crate::donor_folio::source_ui::box_style(swatch, palette.outline)),
                    label(
                        tr("settings-background-color"),
                        10.0,
                        scale,
                        if solid { palette.primary_text } else { palette.surface_text }
                    ),
                ]
                .spacing(8.0 * scale)
                .align_y(iced::Alignment::Center),
            )
            .center(Length::Fill),
        )
        .width(Length::Fixed(width))
        .height(Length::Fixed(crate::donor_folio::source_ui::FOLIO_BUTTON_HEIGHT * scale.max(1.0)))
        .padding([0.0, 8.0 * scale])
        .on_press(Message::Settings(SettingsMsg::BackgroundPicker(self.output.clone())))
        .style(move |_, status| {
            crate::donor_folio::source_ui::folio_button_style(solid, false, palette, status)
        });
        let mut choices = flow![rtl;
            label(tr("settings-background-label"), 9.0, scale, palette.surface_text)
                .width(Length::Fixed(72.0 * scale)),
            color,
            crate::donor_folio::source_ui::folio_action(rtl,
                tr("settings-background-blur"),
                self.mode == "blur",
                Some(Message::Settings(SettingsMsg::BackgroundMode(
                    self.output.clone(),
                    String::from("blur")
                ))),
                Length::Fixed(width),
                scale,
                palette
            ),
        ]
        .spacing(10.0 * scale)
        .align_y(iced::Alignment::Center);
        if !self.output.is_empty() {
            choices = choices.push(crate::donor_folio::source_ui::folio_action(
                rtl,
                tr("settings-background-inherit"),
                self.inherited,
                Some(Message::Settings(SettingsMsg::BackgroundMode(
                    self.output.clone(),
                    String::from("inherit"),
                ))),
                Length::Fixed(width),
                scale,
                palette,
            ));
        }
        let mut body = column![choices].spacing(8.0 * scale);
        if picker > 0.0 {
            let hsv = hex_to_hsv(&self.color).unwrap_or_default();
            let output = self.output.clone();
            let wheel: Element<'_, crate::donor_folio::source_ui::wheel::WheelAction> =
                canvas(crate::donor_folio::source_ui::wheel::ColorWheel { hsv, binding: None })
                    .width(Length::Fixed(176.0 * scale))
                    .height(Length::Fixed(176.0 * scale))
                    .into();
            let wheel = wheel.map(move |message| {
                let next = match message {
                    crate::donor_folio::source_ui::wheel::WheelAction::Hue(hue) => hsv_to_hex(
                        hue,
                        if hsv.1 == 0.0 { 0.5 } else { hsv.1 },
                        if hsv.2 == 0.0 { 0.5 } else { hsv.2 },
                    ),
                    crate::donor_folio::source_ui::wheel::WheelAction::SV(saturation, value) => {
                        hsv_to_hex(hsv.0, saturation, value)
                    }
                    crate::donor_folio::source_ui::wheel::WheelAction::DragEnd => {
                        return Message::Settings(SettingsMsg::BackgroundCommit(output.clone()));
                    }
                };
                Message::Settings(SettingsMsg::BackgroundColor(output.clone(), next))
            });
            body = body.push(
                container(
                    flow![rtl;wheel, label(self.color.clone(), 12.0, scale, palette.surface_text)]
                        .spacing(14.0 * scale),
                )
                .height(Length::Fixed(
                    184.0 * scale * crate::donor_folio::source_ui::motion::smoothstep(picker),
                ))
                .clip(true),
            );
        }
        container(body)
            .width(Length::Fill)
            .height(Length::Fixed(
                (52.0 + 192.0 * picker)
                    * scale
                    * crate::donor_folio::source_ui::motion::smoothstep(reveal),
            ))
            .clip(true)
            .padding(iced::Padding {
                top: 8.0 * scale,
                ..iced::Padding::ZERO
            })
            .into()
    }
}

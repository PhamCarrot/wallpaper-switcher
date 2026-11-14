use crate::donor_folio::source_ui::flow;
use crate::donor_folio::source_ui::{label, with_alpha};
use crate::{
    candidate_app::localization::Catalog,
    candidate_app::view::settings::composed::display::DisplayControl,
    candidate_app::view::settings::folio::intent::{DisplayIntent, Message},
    donor_ui::Palette,
};
use iced::widget::container;
use iced::{Alignment, Element, Length};
#[derive(Debug, Default, Clone, Copy)]
pub struct Availability {
    pub colours: bool,
    pub audio: bool,
    pub playback: bool,
}
pub fn display_view<'a>(
    catalog: &Catalog,
    availability: Availability,
    control: DisplayControl,
    scale: f32,
    palette: &'a Palette,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    match control {
        DisplayControl::Colours { target, active } => {
            let (source, use_for) = (tr("effects-colours-source"), tr("effects-colours-use"));
            let action_scale = scale * 0.9;
            let width =
                crate::donor_folio::source_ui::folio_action_width(source, action_scale).max(
                    crate::donor_folio::source_ui::folio_action_width(use_for, action_scale),
                );
            flow![rtl;
                    label(
                        tr("effects-colours-label"),
                        9.0,
                        scale,
                        with_alpha(palette.surface_text, 0.46)
                    )
                    .width(Length::Fixed(72.0 * scale)),
                    crate::donor_folio::source_ui::folio_action(rtl,
                        if active { source } else { use_for },
                        active,
                        availability.colours.then_some(Message::Display(DisplayIntent::Colours(target))),
                        Length::Fixed(width),
                        action_scale,
                        palette
                    ),
                ]
                .spacing(6.0 * scale)
                .align_y(Alignment::Center)
                .into()
        }
        DisplayControl::Audio {
            target,
            muted,
            volume,
            source_preview,
        } => audio_row(
            catalog,
            availability,
            target,
            muted,
            volume,
            source_preview,
            scale,
            palette,
        ),
        DisplayControl::Playback {
            target,
            paused,
            manual,
        } => playback_control(
            catalog,
            availability,
            &target,
            paused,
            manual,
            scale,
            palette,
        ),
    }
}
fn audio_row<'a>(
    catalog: &Catalog,
    availability: Availability,
    target: String,
    muted: bool,
    volume: u32,
    source_preview: bool,
    scale: f32,
    palette: &'a Palette,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let mute_message = if source_preview {
        Message::Display(DisplayIntent::PreviewMuted(!muted))
    } else {
        Message::Display(DisplayIntent::Muted(target.clone(), !muted))
    };
    let volume_output = target.clone();
    let release_output = target;
    let gauge = crate::donor_folio::source_ui::folio_slider(
        rtl,
        availability.audio,
        0.0,
        100.0,
        f64::from(volume),
        1.0,
        move |value| {
            if source_preview {
                Message::Display(DisplayIntent::PreviewVolume(value as u32))
            } else {
                Message::Display(DisplayIntent::Volume(volume_output.clone(), value as u32))
            }
        },
        if source_preview {
            Message::Noop
        } else {
            Message::Display(DisplayIntent::VolumeRelease(release_output))
        },
        palette,
    );
    flow![rtl;
        label(tr("effects-audio-label"), 9.0, scale, with_alpha(palette.surface_text, 0.46))
            .width(Length::Fixed(72.0 * scale)),
        crate::donor_folio::source_ui::folio_action(rtl,
            if muted { tr("effects-muted") } else { tr("effects-sound") },
            !muted,
            availability.audio.then_some(mute_message),
            Length::Fixed(74.0 * scale),
            scale * 0.9,
            palette,
        ),
        container(gauge).width(Length::Fill).height(Length::Fixed(27.0 * scale)),
        label(
            format!("{volume}%"),
            10.0,
            scale,
            if muted { with_alpha(palette.surface_text, 0.42) } else { palette.surface_text },
        )
        .width(Length::Fixed(42.0 * scale)),
    ]
    .spacing(7.0 * scale)
    .align_y(Alignment::Center)
    .into()
}
fn playback_control<'a>(
    catalog: &Catalog,
    availability: Availability,
    output: &str,
    paused: bool,
    manual: bool,
    scale: f32,
    palette: &'a Palette,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    flow![rtl;
        label(tr("audio-wallpaper-label"), 9.0, scale, with_alpha(palette.surface_text, 0.46))
            .width(Length::Fixed(72.0 * scale)),
        crate::donor_folio::source_ui::folio_action(rtl,
            tr(if manual { "audio-resume-wallpaper" } else { "audio-pause-wallpaper" }),
            manual,
            availability.playback.then_some(Message::Display(DisplayIntent::Paused(output.to_string(),!manual))),
            Length::Fixed(132.0 * scale),
            scale * 0.9,
            palette,
        ),
        label(
            tr(if manual {
                "audio-wallpaper-paused"
            } else if paused {
                "audio-wallpaper-held"
            } else {
                "audio-wallpaper-playing"
            }),
            9.0,
            scale,
            with_alpha(palette.surface_text, 0.56)
        ),
    ]
    .spacing(7.0 * scale)
    .align_y(Alignment::Center)
    .into()
}

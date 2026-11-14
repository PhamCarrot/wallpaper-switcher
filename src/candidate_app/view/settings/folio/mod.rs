//! Composed Settings presentation adapted from the audited beta.24 reference.
//! Pure view and typed local intents; the application owns all execution.
pub mod intent;
macro_rules! tr_args {
    ($catalog:expr,$key:literal $(,$name:ident=>$value:expr)+ $(,)?)=>{{
        let mut args=fluent::FluentArgs::new();$(args.set(stringify!($name),$value);)+
        $catalog.format($key,Some(&args)).expect("verified source arguments")
    }};
}
mod background;
pub mod control;
use crate::{
    candidate_app::localization::Catalog,
    candidate_app::view::settings::composed::{
        background::BackgroundControl, display::DisplayControl,
    },
    candidate_app::view::settings::folio::intent::Message,
    donor_ui::Palette,
};
use iced::widget::{column, text};
use iced::{Element, Length};
pub fn background_view<'a>(
    catalog: &Catalog,
    value: &BackgroundControl,
    reveal: f32,
    picker: f32,
    scale: f32,
    palette: &'a Palette,
) -> Element<'a, Message> {
    value.view(catalog, reveal, picker, scale, palette)
}

pub mod display;
pub fn display_view<'a>(
    catalog: &Catalog,
    control: DisplayControl,
    scale: f32,
    palette: &'a Palette,
) -> Element<'a, Message> {
    // Availability defaults to unsupported in this owned probe; the production
    // context must supply actual per-display adapter capabilities.
    column![
        display::display_view(
            catalog,
            display::Availability::default(),
            control,
            scale,
            palette
        ),
        text(
            "Per-display operations are unavailable here; global Engine controls remain separate."
        )
    ]
    .into()
}

pub mod app_theme;

pub mod workbench;

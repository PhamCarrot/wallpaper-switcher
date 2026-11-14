use iced::{alignment, Alignment, Color, Font, Point};

pub const NERD_FONT: Font = Font::with_name("Symbols Nerd Font");

pub const UI_FONT_FAMILY: &str = "Roboto Condensed";

pub const UI_FONT: Font = Font {
    family: iced::font::Family::Name(UI_FONT_FAMILY),
    weight: iced::font::Weight::Bold,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};

pub const TYPE_SMALL: f32 = 11.0;

// beta.24 bar controls; panel adoption remains separately scoped.
pub(crate) const CONTROL_HEIGHT: f32 = 24.0;
pub(crate) const CONTROL_LABEL_SIZE: f32 = 10.0;
pub(crate) const CONTROL_ICON_SIZE: f32 = 14.0;
pub(crate) const CONTROL_PADDING: f32 = 24.0;
pub(crate) const CONTROL_SKEW: f32 = 10.0;
pub(crate) const CONTROL_LINE_HEIGHT: f32 = 1.2;

pub fn legible_type_scale(scale: f32) -> f32 {
    scale.max(0.95)
}

pub fn label<'a>(
    content: impl iced::widget::text::IntoFragment<'a>,
    size: f32,
    scale: f32,
    color: Color,
) -> iced::widget::Text<'a> {
    iced::widget::text(content)
        .font(UI_FONT)
        .size(size * legible_type_scale(scale))
        .color(color)
}

pub fn sentence_case(label: &str) -> String {
    let mut value = label.to_ascii_lowercase();
    if let Some(first) = value.get_mut(0..1) {
        first.make_ascii_uppercase();
    }
    value
}

pub(crate) fn glyph_width(size: f32) -> f32 {
    size * 1.1
}

pub(crate) fn text_width(label: &str, size: f32, nerd: bool) -> f32 {
    let count = label.chars().count() as f32;
    if nerd {
        count * glyph_width(size)
    } else {
        count * size * 0.64
    }
}

pub(crate) fn ellipsize_text(label: &str, size: f32, max_width: f32) -> String {
    if text_width(label, size, false) <= max_width {
        return label.to_owned();
    }
    let ellipsis = '…';
    let char_width = size * 0.64;
    let keep = ((max_width / char_width).floor() as usize).saturating_sub(1);
    let mut fitted: String = label.chars().take(keep).collect();
    fitted.push(ellipsis);
    fitted
}

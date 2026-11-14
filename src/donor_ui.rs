//! Small, in-process adaptation of the pinned skwd-wall folio presentation.
//!
//! The donor's `frontend/ui/misc/{folio,layout,style,typography}.rs` owns the
//! visual rules used here.  This module deliberately keeps only pure metrics
//! and Iced style values;  continues to own messages, persistence, catalog
//! identity and provider actions.

use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Padding, Shadow, Vector};

const FOLIO_ACTION_MIN_WIDTH: f32 = 52.0;
const FOLIO_ACTION_MAX_WIDTH: f32 = 190.0;

/// Iced controls share the donor primary role (accent), never the surface.
/// A surface-colored slider handle disappeared against folio reading panes.
pub(crate) fn widget_theme(background: Color, text: Color, accent: Color) -> iced::Theme {
    iced::Theme::custom(
        "Wallpaper Switcher",
        iced::theme::Palette {
            background,
            text,
            primary: accent,
            success: accent,
            warning: Color::from_rgb(0.94, 0.72, 0.37),
            danger: Color::from_rgb(0.90, 0.36, 0.44),
        },
    )
}

pub const FOLIO_INDEX_WIDTH: f32 = 318.0;
pub const FOLIO_SCRIM_ALPHA: f32 = 0.68;
pub const FOLIO_INDEX_BG_ALPHA: f32 = 0.90;
pub const FOLIO_INDEX_OUTLINE_ALPHA: f32 = 0.46;
pub const FOLIO_SHEET_WIDTH: f32 = 1390.0;
pub const FOLIO_SHEET_HEIGHT: f32 = 870.0;
pub const FOLIO_SHEET_MARGIN: f32 = 22.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub primary: Color,
    pub primary_text: Color,
    pub surface: Color,
    pub surface_text: Color,
    pub surface_variant: Color,
    pub surface_container: Color,
    pub background: Color,
    pub outline: Color,
    pub tertiary: Color,
}

impl Palette {
    pub fn from_rgb(
        background: [f32; 3],
        surface: [f32; 3],
        text: [f32; 3],
        accent: [f32; 3],
    ) -> Self {
        let background = rgb(background);
        let surface = rgb(surface);
        let text = rgb(text);
        let accent = rgb(accent);
        Self {
            primary: accent,
            primary_text: background,
            surface,
            surface_text: text,
            surface_variant: mix(surface, text, 0.24),
            surface_container: mix(background, surface, 0.58),
            background,
            outline: mix(text, surface, 0.44),
            tertiary: mix(accent, text, 0.38),
        }
    }
}

fn rgb(value: [f32; 3]) -> Color {
    Color::from_rgb(value[0], value[1], value[2])
}

pub fn rgb_components(value: Color) -> [f32; 3] {
    [value.r, value.g, value.b]
}

fn with_alpha(color: Color, alpha: f32) -> Color {
    Color {
        a: color.a * alpha,
        ..color
    }
}

pub fn with_alpha_color(color: Color, alpha: f32) -> Color {
    with_alpha(color, alpha)
}

fn mix(a: Color, b: Color, t: f32) -> Color {
    let t = t.clamp(0.0, 1.0);
    Color {
        r: a.r + (b.r - a.r) * t,
        g: a.g + (b.g - a.g) * t,
        b: a.b + (b.b - a.b) * t,
        a: a.a + (b.a - a.a) * t,
    }
}

pub fn action_width(label: &str, scale: f32) -> f32 {
    crate::donor_folio::source_ui::folio_action_width_in(
        label,
        scale,
        FOLIO_ACTION_MIN_WIDTH,
        FOLIO_ACTION_MAX_WIDTH,
    )
}

pub fn sheet_dimensions(viewport: (f32, f32), scale: f32) -> (f32, f32) {
    (
        (FOLIO_SHEET_WIDTH * scale)
            .min(viewport.0 - FOLIO_SHEET_MARGIN)
            .max(220.0),
        (FOLIO_SHEET_HEIGHT * scale)
            .min(viewport.1 - FOLIO_SHEET_MARGIN)
            .max(160.0),
    )
}

pub fn sheet_padding(scale: f32) -> Padding {
    Padding {
        top: 25.0 * scale,
        right: 20.0 * scale,
        bottom: 20.0 * scale,
        left: 22.0 * scale,
    }
}

pub fn scrim_style(ease: f32) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba(
            0.0,
            0.0,
            0.0,
            FOLIO_SCRIM_ALPHA * ease.clamp(0.0, 1.0),
        ))),
        ..Default::default()
    }
}

pub fn sheet_style(palette: Palette, ease: f32) -> container::Style {
    container::Style {
        background: Some(with_alpha(palette.surface, 0.99).into()),
        border: Border {
            radius: 0.0.into(),
            width: 1.0,
            color: with_alpha(palette.outline, 0.58),
        },
        shadow: Shadow {
            color: with_alpha(Color::BLACK, 0.52 * ease.clamp(0.0, 1.0)),
            offset: Vector::new(0.0, 16.0),
            blur_radius: 48.0,
        },
        ..Default::default()
    }
}

pub fn detail_sheet_style(palette: Palette, surface_alpha: f32) -> container::Style {
    let alpha = surface_alpha.clamp(0.0, 1.0);
    container::Style {
        background: Some(with_alpha(palette.surface, 0.97 * alpha).into()),
        border: Border {
            radius: 0.0.into(),
            width: 1.0,
            color: with_alpha(palette.outline, 0.72 * alpha),
        },
        shadow: Shadow {
            color: with_alpha(Color::BLACK, 0.56 * alpha),
            offset: Vector::new(0.0, 14.0),
            blur_radius: 34.0,
        },
        ..Default::default()
    }
}

pub fn detail_masthead_style(
    palette: Palette,
    surface_alpha: f32,
    embedded: bool,
) -> container::Style {
    let alpha = surface_alpha.clamp(0.0, 1.0);
    let background = if embedded {
        with_alpha(palette.surface, 0.78 * alpha)
    } else {
        with_alpha(palette.background, 0.90 * alpha)
    };
    box_style(background, with_alpha(palette.outline, 0.48 * alpha))
}

pub fn index_style(palette: Palette) -> container::Style {
    box_style(
        with_alpha(palette.background, FOLIO_INDEX_BG_ALPHA),
        with_alpha(palette.outline, FOLIO_INDEX_OUTLINE_ALPHA),
    )
}

pub fn panel_style(palette: Palette, alpha: f32) -> container::Style {
    box_style(
        with_alpha(palette.surface, alpha.clamp(0.0, 1.0)),
        with_alpha(palette.outline, 0.58),
    )
}

pub fn box_style(fill: Color, border: Color) -> container::Style {
    container::Style {
        background: Some(fill.into()),
        border: Border {
            radius: 0.0.into(),
            width: 1.0,
            color: border,
        },
        ..Default::default()
    }
}

pub fn rail_style(palette: Palette, active: bool, status: button::Status) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    let pressed = matches!(status, button::Status::Pressed);
    let disabled = matches!(status, button::Status::Disabled);
    let accent = if active {
        palette.primary
    } else {
        palette.surface_container
    };
    let alpha = if pressed {
        0.76
    } else if hovered {
        0.66
    } else {
        0.46
    };
    button::Style {
        background: Some(with_alpha(accent, if disabled { alpha * 0.45 } else { alpha }).into()),
        text_color: with_alpha(palette.surface_text, if disabled { 0.42 } else { 1.0 }),
        border: Border {
            color: with_alpha(
                if active {
                    palette.primary
                } else {
                    palette.outline
                },
                if active { 0.86 } else { 0.34 },
            ),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn action_style(
    palette: Palette,
    active: bool,
    destructive: bool,
    status: button::Status,
) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    let disabled = matches!(status, button::Status::Disabled);
    let accent = if destructive {
        palette.tertiary
    } else {
        palette.primary
    };
    button::Style {
        background: if disabled {
            None
        } else if active {
            Some(with_alpha(accent, 0.92).into())
        } else if hovered {
            Some(with_alpha(palette.surface_variant, 0.62).into())
        } else {
            Some(with_alpha(palette.surface_container, 0.92).into())
        },
        text_color: if disabled {
            with_alpha(palette.surface_text, 0.34)
        } else if active {
            palette.primary_text
        } else if destructive {
            palette.tertiary
        } else {
            palette.surface_text
        },
        border: Border {
            color: with_alpha(
                if destructive || hovered || active {
                    accent
                } else {
                    palette.outline
                },
                0.9,
            ),
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn line_style(
    palette: Palette,
    active: bool,
    focused: bool,
    status: button::Status,
) -> button::Style {
    let hovered = matches!(status, button::Status::Hovered);
    button::Style {
        background: if active {
            Some(with_alpha(palette.primary, 0.18).into())
        } else if hovered {
            Some(with_alpha(palette.surface_variant, 0.62).into())
        } else {
            None
        },
        text_color: palette.surface_text,
        border: Border {
            color: if focused {
                palette.primary
            } else {
                Color::TRANSPARENT
            },
            width: if focused { 2.0 } else { 0.0 },
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn embedded_style(palette: Palette, status: button::Status) -> button::Style {
    button::Style {
        background: matches!(status, button::Status::Hovered)
            .then(|| with_alpha(palette.surface_variant, 0.42).into()),
        text_color: palette.surface_text,
        ..Default::default()
    }
}

pub fn ghost_input_style(palette: Palette) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border {
            color: Color::TRANSPARENT,
            width: 0.0,
            radius: 0.0.into(),
        },
        icon: palette.tertiary,
        placeholder: with_alpha(palette.surface_text, 0.42),
        value: palette.surface_text,
        selection: with_alpha(palette.primary, 0.35),
    }
}

pub fn field_input_style(palette: Palette) -> text_input::Style {
    text_input::Style {
        background: with_alpha(palette.surface_container, 0.88).into(),
        border: Border {
            color: with_alpha(palette.outline, 0.48),
            width: 1.0,
            radius: 0.0.into(),
        },
        icon: palette.surface_text,
        placeholder: with_alpha(palette.surface_text, 0.35),
        value: palette.surface_text,
        selection: with_alpha(palette.primary, 0.4),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn folio_controls_use_visible_accent_instead_of_reading_surface() {
        let background = iced::Color::from_rgb8(46, 52, 64);
        let text = iced::Color::from_rgb8(236, 239, 244);
        let accent = iced::Color::from_rgb8(136, 192, 208);
        let theme = super::widget_theme(background, text, accent);
        assert_eq!(theme.palette().primary, accent);
        assert_ne!(theme.palette().primary, iced::Color::from_rgb8(59, 66, 82));
        assert_eq!(theme.palette().background, background);
        assert_eq!(theme.palette().text, text);
    }
    use super::*;

    #[test]
    fn donor_action_widths_are_bounded_and_scale() {
        assert_eq!(action_width("", 1.0), FOLIO_ACTION_MIN_WIDTH);
        assert!(action_width("Apply", 1.0) >= FOLIO_ACTION_MIN_WIDTH);
        assert_eq!(
            action_width("a very long action label that must wrap", 1.0),
            FOLIO_ACTION_MAX_WIDTH
        );
        assert_eq!(action_width("Apply", 1.5), action_width("Apply", 1.0) * 1.5);
    }

    #[test]
    fn donor_sheet_dimensions_obey_viewport_margin_and_minimum() {
        assert_eq!(sheet_dimensions((1920.0, 1080.0), 1.0), (1390.0, 870.0));
        assert_eq!(sheet_dimensions((300.0, 180.0), 1.0), (278.0, 160.0));
    }

    #[test]
    fn donor_palette_preserves_app_colors_as_roles() {
        let palette = Palette::from_rgb(
            [0.1, 0.2, 0.3],
            [0.2, 0.3, 0.4],
            [0.8, 0.7, 0.6],
            [0.9, 0.1, 0.2],
        );
        assert_eq!(palette.background, Color::from_rgb(0.1, 0.2, 0.3));
        assert_eq!(palette.surface, Color::from_rgb(0.2, 0.3, 0.4));
        assert_eq!(palette.surface_text, Color::from_rgb(0.8, 0.7, 0.6));
        assert_eq!(palette.primary, Color::from_rgb(0.9, 0.1, 0.2));
    }
}

//! Search placement adapted from pinned beta.13 app/view/tags.rs.
//! The beta.26 hierarchy follows frontend/tagcloud/tagcloud.rs and app/view/tags.rs.
//!  owns the catalog, query, input/focus and all non-applying messages.
use crate::candidate_state::{GalleryKind, GallerySettings, GeometricShape};
use crate::donor_layout::{HexParams, HexShape};

pub(crate) fn footprint(gallery: &GallerySettings) -> (f32, Option<f32>) {
    match gallery.mode {
        GalleryKind::Slices => (gallery.slices.height + 110.0, None),
        GalleryKind::Depth => (gallery.depth.height + 110.0, None),
        GalleryKind::Wall => {
            let g = &gallery.wall;
            (
                g.rows as f32 * g.card_height + g.rows.saturating_sub(1) as f32 * g.gap_y + 85.0,
                Some(
                    g.columns as f32 * g.card_width + g.columns.saturating_sub(1) as f32 * g.gap_x,
                ),
            )
        }
        GalleryKind::Geometric => {
            let g = &gallery.geometric;
            let shape = match g.shape {
                GeometricShape::Triangle => HexShape::Triangle,
                GeometricShape::Diamond => HexShape::Diamond,
                GeometricShape::Rhombus => HexShape::Rhombus,
                _ => HexShape::Hexagon,
            };
            let p = HexParams {
                r: g.radius.max(crate::donor_layout::MIN_HEX_R),
                rows: g.rows.max(1),
                cols: g.columns.max(1),
                gap_x: g.gap_x,
                gap_y: g.gap_y,
                aspect: g.aspect.max(0.05),
                stagger: g.stagger,
                shape,
                ..HexParams::default()
            };
            if g.shape == GeometricShape::Circle {
                (
                    g.rows as f32 * 2.0 * g.radius
                        + g.rows.saturating_sub(1) as f32 * g.gap_y
                        + 140.0,
                    Some(
                        g.columns as f32 * 2.0 * g.radius
                            + g.columns.saturating_sub(1) as f32 * g.gap_x,
                    ),
                )
            } else {
                (p.content_h() + 140.0, Some(p.visible_band()))
            }
        }
    }
}
pub(crate) fn bounds(
    viewport: [f32; 2],
    gallery: &GallerySettings,
    [width, height]: [f32; 2],
    [offset_x, offset_y]: [f32; 2],
) -> [f32; 4] {
    let (card_h, _) = footprint(gallery);
    let max_top = (viewport[1] - height - 16.0).max(0.0);
    let anchor = (((viewport[1] + card_h) * 0.5 + 14.0).min(max_top)).max(0.0);
    [
        ((viewport[0] - width) * 0.5 + offset_x).clamp(0.0, (viewport[0] - width).max(0.0)),
        (anchor + offset_y).clamp(0.0, max_top),
        width,
        height,
    ]
}

pub(crate) fn wrapped_height(value: &str, size: f32, width: f32, font: iced::Font) -> f32 {
    use iced::advanced::{
        graphics::text::Paragraph,
        text::{self, Paragraph as _},
    };
    Paragraph::with_text(text::Text {
        content: value,
        bounds: iced::Size::new(width, f32::INFINITY),
        size: size.into(),
        line_height: text::LineHeight::Relative(1.25),
        font,
        align_x: text::Alignment::Left,
        align_y: iced::alignment::Vertical::Top,
        shaping: text::Shaping::Advanced,
        wrapping: text::Wrapping::WordOrGlyph,
    })
    .min_height()
    .ceil()
}

pub(crate) fn chip_rows(widths: &[f32], width: f32, gap: f32) -> Vec<usize> {
    let mut rows = Vec::new();
    let mut remaining = 0.0;
    for &chip in widths {
        if rows.is_empty() || gap + chip > remaining {
            rows.push(1);
            remaining = width - chip;
        } else {
            *rows.last_mut().unwrap() += 1;
            remaining -= gap + chip;
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_cloud_anchors_measured_content_and_offsets_inside_viewport() {
        let mut g = GallerySettings::default();
        assert_eq!(
            bounds([1920.0, 1080.0], &g, [760.0, 126.0], [0.0, 0.0]),
            [580.0, 869.0, 760.0, 126.0]
        );
        for viewport in [[800.0, 600.0], [1280.0, 720.0], [1920.0, 1080.0]] {
            for mode in [
                GalleryKind::Slices,
                GalleryKind::Geometric,
                GalleryKind::Wall,
            ] {
                g.mode = mode;
                let size = [viewport[0] - 40.0, 227.0];
                let [x, y, w, h] = bounds(viewport, &g, size, [100.0, -100.0]);
                assert!(x >= 0.0 && y >= 0.0 && x + w <= viewport[0] && y + h <= viewport[1]);
                assert_eq!([w, h], size);
            }
        }
    }

    #[test]
    fn shaped_button_widths_and_spacing_determine_rows() {
        assert!(chip_rows(&[], 180.0, 6.0).is_empty());
        assert_eq!(
            chip_rows(&[49.0, 65.0, 54.0, 135.0, 40.0, 180.0], 180.0, 6.0),
            [3, 1, 1, 1]
        );
        let rows = chip_rows(&[54.0; 50], 180.0, 6.0);
        assert!(rows.len() > 3);
        assert_eq!(rows.into_iter().sum::<usize>(), 50);
    }
}

//! Included BackLayout adaptation from skwd-wall beta.13 (9735be4).
//! Card-back geometry shared by drawing and hit testing.
//! 's explicit Apply is first and remains outside optional content overflow.
use crate::donor_detail::CardGeometry;
use iced::{Font, Point, Rectangle, Size};
#[path = "donor_back_text.rs"]
mod text;
pub(crate) use text::{measured_full_lines, measured_lines, measured_width};

pub(crate) const DETAIL_FACT_COUNT: usize = 4;
pub(crate) const TAG_BUTTON_HEIGHT: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DetailIntent {
    Apply,
    Hide,
    Playlists,
    LocalTags,
    OpenFolder,
    CopyPath,
    CopyImage,
    Properties,
    Favorite,
    Close,
}

#[derive(Debug, Clone)]
pub(crate) struct Action {
    pub intent: DetailIntent,
    pub label: String,
    pub enabled: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct BackLayout {
    /// Readable logical-pixel metrics; all presentations remain at scale 1.
    pub content_scale: f32,
    pub card: Rectangle,
    pub masthead: Rectangle,
    pub sheet: Rectangle,
    pub title: Rectangle,
    pub title_size: f32,
    pub title_lines: Vec<String>,
    pub title_line_height: f32,
    pub label_size: f32,
    pub value_size: f32,
    pub action_size: f32,
    pub source_size: f32,
    pub source_value_offset: f32,
    pub label_offset: f32,
    pub value_inset: f32,
    pub facts: Vec<Rectangle>,
    pub source: Rectangle,
    pub rule_y: f32,
    pub actions_label: Point,
    pub actions: Vec<Rectangle>,
    pub favorite: Rectangle,
    pub close: Rectangle,
    pub wide: bool,
}

impl BackLayout {
    pub(crate) fn source_text_bounds(&self) -> (Rectangle, Rectangle) {
        let s = self.content_scale;
        let y = if self.wide {
            self.source.y
        } else {
            self.rule_y + 10.0 * s
        };
        let label_h = self.label_size * 1.25 * s;
        let value_h = self.source_size * 1.25 * s;
        (
            Rectangle {
                y: y + 3.0 * s - label_h * 0.5,
                height: label_h,
                ..self.source
            },
            Rectangle {
                y: y + self.source_value_offset * s - value_h * 0.5,
                height: value_h,
                ..self.source
            },
        )
    }

    pub(crate) fn fact_text_bounds(&self, r: Rectangle) -> (Rectangle, Rectangle) {
        let s = self.content_scale;
        let label_h = self.label_size * 1.25 * s;
        let value_h = self.value_size * 1.25 * s;
        (
            Rectangle {
                y: r.y + self.label_offset * s - label_h * 0.5,
                height: label_h,
                ..r
            },
            Rectangle {
                y: r.y + r.height - self.value_inset * s - value_h * 0.5,
                height: value_h,
                ..r
            },
        )
    }
}

fn rect(x: f32, y: f32, w: f32, h: f32) -> Rectangle {
    Rectangle::new(Point::new(x, y), Size::new(w, h))
}

/// Shared loaded-font measurement; never use character count as a fit oracle.
pub(crate) fn text_width(text: &str, size: f32) -> f32 {
    measured_width(text, size, Font::default())
}
pub(crate) fn ellipsize(text: &str, size: f32, width: f32) -> String {
    measured_lines(text, size, width, Font::default(), 1)
        .pop()
        .unwrap_or_default()
}

fn action_rows(
    actions: &[Action],
    left: f32,
    y: f32,
    width: f32,
    height: f32,
    gap: f32,
    size: f32,
    font: Font,
) -> (Vec<Rectangle>, f32) {
    let mut x = left;
    let mut cy = y;
    let mut rectangles = Vec::new();
    for action in actions {
        let w = (measured_width(&action.label, size, font) + 22.0)
            .clamp(64.0, 190.0)
            .min(width);
        if x > left && x + w > left + width {
            x = left;
            cy += height + gap;
        }
        rectangles.push(rect(x, cy, w, height));
        x += w + gap;
    }
    (rectangles, cy + height - y)
}

pub(crate) fn layout(
    card: CardGeometry,
    embedded: bool,
    facts: usize,
    actions: &[Action],
    scale: f32,
    font: Font,
    title_text: &str,
) -> BackLayout {
    // Calculate in effective logical units, then return physical rectangles.
    // Saved card geometry and the scene's own transform remain untouched.
    let scale = scale.clamp(0.5, 2.0);
    let physical_card = card;
    let card = CardGeometry {
        size: card.size.map(|v| v / scale),
        center: card.center.map(|v| v / scale),
        skew_distance: card.skew_distance / scale,
        edge_tilt_distance: card.edge_tilt_distance / scale,
        ..card
    };
    let [w, h] = card.size;
    let [cx, cy] = card.center;
    let card_rect = rect(cx - w * 0.5, cy - h * 0.5, w, h);
    let wide = embedded && w >= 620.0 && h >= 320.0;
    let inset = if embedded {
        (w.min(h) * 0.025).clamp(5.0, 12.0)
    } else {
        0.0
    };
    let sw = if embedded {
        w - inset * 2.0
    } else {
        (w * 0.245).clamp(292.0, 410.0).min(w * 0.42)
    };
    let pad = if wide {
        (sw * 0.024).clamp(12.0, 22.0)
    } else if embedded {
        (sw * 0.032).clamp(11.0, 28.0)
    } else {
        (sw * 0.065).clamp(18.0, 28.0)
    };
    let guard = if wide {
        card.skew_distance.abs().min(sw * 0.16)
    } else {
        0.0
    };
    let left = card_rect.x + inset + pad + guard;
    let right = card_rect.x + inset + sw - pad - guard;
    let content_w = (right - left).max(1.0);
    let compact = h < if embedded { 360.0 } else { 460.0 };
    let mh = if wide {
        32.0
    } else if embedded {
        if compact {
            29.0
        } else {
            34.0
        }
    } else {
        (h * 0.052).clamp(42.0, 56.0)
    };
    let masthead = rect(card_rect.x + inset, card_rect.y + inset, sw, mh);
    let gap = if compact { 5.0 } else { 7.0 };
    let label_size = if embedded { 8.5 } else { 11.0 };
    let value_size = if embedded { 12.0 } else { 16.0 };
    let action_size = if embedded { 10.0 } else { 14.0 };
    let row_h = if wide {
        25.0
    } else if compact {
        24.0
    } else if embedded {
        27.0
    } else {
        36.0
    };
    let split = left + content_w * 0.45;
    let col_gap = (content_w * 0.035).clamp(16.0, 28.0);
    let action_left = if wide { split + col_gap * 0.5 } else { left };
    let (mut action_rects, deck_rows_h) = action_rows(
        actions,
        action_left,
        0.0,
        right - action_left,
        row_h,
        gap,
        action_size,
        font,
    );
    let title_size = if wide {
        (w * 0.026).clamp(21.0, 25.0)
    } else if embedded {
        (content_w * 0.042).clamp(if compact { 20.0 } else { 23.0 }, 34.0)
    } else {
        (content_w * 0.09).clamp(25.0, 32.0)
    };
    let title_w = if wide {
        split - col_gap * 0.5 - left
    } else {
        content_w
    };
    let title_line_height = title_size * 1.25;
    let origin = physical_card.point(physical_card.center);
    let axis = physical_card.point([physical_card.center[0] + 1.0, physical_card.center[1]]);
    let vertical = physical_card.point([physical_card.center[0], physical_card.center[1] + 1.0]);
    let horizontal_scale = (axis[0] - origin[0]).hypot(axis[1] - origin[1]);
    let angle = (axis[1] - origin[1]).atan2(axis[0] - origin[0]);
    let normal_scale = ((axis[0] - origin[0]) * (vertical[1] - origin[1])
        - (axis[1] - origin[1]) * (vertical[0] - origin[0]))
        .abs()
        / horizontal_scale.max(0.0001);
    let title_lines = measured_lines(
        title_text,
        title_size * scale,
        title_w * scale * horizontal_scale,
        font,
        if embedded { 1 } else { 2 },
    );
    // Readable glyphs rotate without shearing. Reserve their measured line
    // box plus its vertical rotation footprint before placing facts/deck.
    let mut title_h = if embedded {
        (title_line_height + angle.sin().abs() * title_w * horizontal_scale + 8.0)
            / normal_scale.max(0.1)
    } else {
        title_line_height * title_lines.len().max(1) as f32
    };
    let available = (h - inset * 2.0 - mh - gap).max(1.0);
    if embedded && !wide {
        // Preserve one full readable value line and scroll access to every
        // fact/folder. At short heights the title ellipsizes at a narrower
        // measured width instead of consuming the remaining content region.
        let bottom = if compact { 12.0 } else { 18.0 };
        let top = if compact { 11.0 } else { 14.0 };
        let label_to_deck = label_size * 0.625 + 7.0;
        let max_title = available
            - bottom
            - deck_rows_h
            - label_to_deck
            - 12.0
            - top
            - 4.0
            - (f32::max(value_size * 1.25, TAG_BUTTON_HEIGHT) + 1.0);
        title_h = title_h.min(max_title.max(0.0));
    }
    let wide_rule_offset = (7.0 + title_h).max(43.0) + 7.0;
    let desired = if wide {
        wide_rule_offset + label_size * 1.25 + 16.0 + deck_rows_h + 10.0
    } else {
        14.0 + title_h + 8.0 + facts as f32 * 35.0 + 28.0 + deck_rows_h + 13.0
    };
    let sh = if wide {
        desired.max(112.0).min(available)
    } else if embedded {
        desired.max(h * 0.40).min(available)
    } else {
        h - mh
    };
    let sheet_y = card_rect.y + h - inset - sh;
    let sheet = rect(card_rect.x + inset, sheet_y, sw, sh);
    let deck_y = if wide {
        sheet_y + wide_rule_offset + label_size * 1.25 + 16.0
    } else {
        sheet_y + sh - if compact { 12.0 } else { 18.0 } - deck_rows_h
    };
    for r in &mut action_rects {
        r.y += deck_y;
    }
    let title = rect(
        left,
        sheet_y
            + if wide {
                7.0
            } else if compact {
                11.0
            } else {
                14.0
            },
        title_w,
        title_h,
    );
    let fact_left = if wide { split + col_gap * 0.5 } else { left };
    let fact_top = if wide {
        sheet_y + 7.0
    } else {
        title.y + title.height + if compact { 4.0 } else { 8.0 }
    };
    // Wide donor columns and external donor single/two-column facts. Narrow
    // Slices use two columns to keep supported values legible ( adaptation).
    let columns = if wide {
        facts.max(1)
    } else if compact || embedded {
        2
    } else {
        1
    };
    let fact_gap = if wide { 8.0 } else { 6.0 };
    let fw = ((right - fact_left - fact_gap * (columns - 1) as f32) / columns as f32).max(1.0);
    let fh = if !embedded {
        54.0
    } else if wide {
        36.0
    } else if compact {
        35.0
    } else {
        43.0
    };
    let fact_rects: Vec<_> = (0..facts)
        .map(|i| {
            rect(
                fact_left + (i % columns) as f32 * (fw + fact_gap),
                fact_top + (i / columns) as f32 * fh,
                fw,
                fh,
            )
        })
        .collect();
    let rule_y = if wide {
        sheet_y + wide_rule_offset
    } else {
        fact_rects.last().map_or(fact_top, |r| r.y + r.height)
    };
    let source = if wide {
        rect(left, deck_y, split - col_gap * 0.5 - left, deck_rows_h)
    } else {
        rect(
            left,
            rule_y + 10.0,
            content_w,
            (deck_y - 27.0 - rule_y - 10.0).max(0.0),
        )
    };
    let button_size = mh - 8.0;
    // Donor favorite relationship, reserving a distinct close target in .
    let favorite = rect(
        right - button_size,
        masthead.y + 4.0,
        button_size,
        button_size,
    );
    let close = rect(
        right - button_size * 2.0 - 6.0,
        masthead.y + 4.0,
        button_size,
        button_size,
    );
    let mut result = BackLayout {
        content_scale: scale,
        card: card_rect,
        masthead,
        sheet,
        title,
        title_size,
        title_lines,
        title_line_height,
        label_size,
        value_size,
        action_size,
        source_size: if embedded { 10.0 } else { 14.0 },
        source_value_offset: if embedded { 19.0 } else { 25.0 },
        label_offset: if embedded { 6.0 } else { 9.0 },
        value_inset: if embedded { 7.0 } else { 11.0 },
        facts: fact_rects,
        source,
        rule_y,
        actions_label: Point::new(action_left, deck_y - label_size * 0.625 - 7.0),
        actions: action_rects,
        favorite,
        close,
        wide,
    };
    let physical = |r: &mut Rectangle| {
        r.x *= scale;
        r.y *= scale;
        r.width *= scale;
        r.height *= scale;
    };
    for r in [
        &mut result.card,
        &mut result.masthead,
        &mut result.sheet,
        &mut result.title,
        &mut result.source,
        &mut result.favorite,
        &mut result.close,
    ] {
        physical(r);
    }
    for r in result.facts.iter_mut().chain(result.actions.iter_mut()) {
        physical(r);
    }
    result.rule_y *= scale;
    result.actions_label.x *= scale;
    result.actions_label.y *= scale;
    let clip = title_clip(
        physical_card,
        result.title,
        title_line_height * scale * result.title_lines.len().max(1) as f32,
    );
    let width = title_glyph_width(
        physical_card,
        clip,
        title_line_height * scale * result.title_lines.len().max(1) as f32,
    );
    result.title_lines = measured_lines(
        title_text,
        title_size * scale,
        width,
        font,
        if embedded { 1 } else { 2 },
    );
    result
}

/// Fit a screen-aligned clip around the readable rotated glyph block inside
/// the affine title quad. Solve both inverse-axis inequalities directly: a
/// uniform shrink of the outer AABB loses vertical space on short skewed cards.
pub(crate) fn title_clip(card: CardGeometry, r: Rectangle, glyph_height: f32) -> Rectangle {
    let center = card.point([r.center_x(), r.center_y()]);
    let Some(origin) = card.inverse_point(center) else {
        return Rectangle::default();
    };
    let Some(x) = card.inverse_point([center[0] + 1.0, center[1]]) else {
        return Rectangle::default();
    };
    let Some(y) = card.inverse_point([center[0], center[1] + 1.0]) else {
        return Rectangle::default();
    };
    let base = card.point(card.center);
    let axis = card.point([card.center[0] + 1.0, card.center[1]]);
    let angle = (axis[1] - base[1]).atan2(axis[0] - base[0]);
    let (sin, cos) = angle.sin_cos();
    let (sin, cos) = (sin.abs(), cos.abs());
    let limit = |a: f32, b: f32, extent: f32| {
        let (a, b) = (a.abs(), b.abs());
        let budget = extent - (a * sin + b * cos) * glyph_height;
        let coefficient = a * cos + b * sin;
        if coefficient > 0.000001 {
            budget / coefficient
        } else if budget >= -0.001 {
            f32::INFINITY
        } else {
            0.0
        }
    };
    let width = limit(x[0] - origin[0], y[0] - origin[0], r.width)
        .min(limit(x[1] - origin[1], y[1] - origin[1], r.height))
        .max(0.0);
    if width <= 0.0 {
        return Rectangle::new(Point::new(center[0], center[1]), Size::ZERO);
    }
    let size = Size::new(
        cos * width + sin * glyph_height,
        sin * width + cos * glyph_height,
    );
    Rectangle::new(
        Point::new(center[0] - size.width * 0.5, center[1] - size.height * 0.5),
        size,
    )
}
pub(crate) fn title_glyph_width(card: CardGeometry, clip: Rectangle, height: f32) -> f32 {
    let origin = card.point(card.center);
    let axis = card.point([card.center[0] + 1.0, card.center[1]]);
    let angle = (axis[1] - origin[1]).atan2(axis[0] - origin[0]);
    let (sin, cos) = angle.sin_cos();
    let (sin, cos) = (sin.abs(), cos.abs());
    let width = (clip.width - sin * height) / cos.max(0.0001);
    let vertical = if sin > 0.0001 {
        (clip.height - cos * height) / sin
    } else {
        f32::INFINITY
    };
    width.min(vertical).max(0.0)
}

/// D6-R1: an inline back must fit readable metrics in the *presented* body.
/// Complete control rectangles and one readable facts line are required.
/// The 0.01 logical-pixel tolerance handles boundary float noise only.
pub(crate) fn depth_inline_eligible(
    card: CardGeometry,
    radius: f32,
    facts: usize,
    actions: &[Action],
    scale: f32,
    font: Font,
    title: &str,
) -> bool {
    let l = layout(card, true, facts, actions, scale, font, title);
    let fits = |r: Rectangle| {
        [
            [r.x + 0.01, r.y + 0.01],
            [r.x + r.width - 0.01, r.y + 0.01],
            [r.x + 0.01, r.y + r.height - 0.01],
            [r.x + r.width - 0.01, r.y + r.height - 0.01],
        ]
        .into_iter()
        .all(|p| {
            crate::donor_layout::depth_contains(
                card.center,
                card.size,
                card.skew_distance,
                [radius; 4],
                card.point(p),
            )
        })
    };
    let axis = card.point([card.center[0] + 1.0, card.center[1]]);
    let horizontal_scale = (axis[0] - card.center[0]).hypot(axis[1] - card.center[1]);
    let facts_region = rect(
        l.title.x,
        l.title.y + l.title.height + 4.0 * scale,
        l.title.width,
        (l.actions_label.y - 12.0 * scale - l.title.y - l.title.height - 4.0 * scale).max(0.0),
    );
    l.actions.iter().zip(actions).all(|(r, a)| {
        fits(*r)
            && r.height >= 24.0 * scale
            && r.width * horizontal_scale + 0.01
                >= measured_width(&a.label, l.action_size * scale, font) + 12.0 * scale
    }) && [l.favorite, l.close].into_iter().all(|r| {
        fits(r) && r.width * horizontal_scale + 0.01 >= 21.0 * scale && r.height >= 21.0 * scale
    }) && fits(l.title)
        && !l.title_lines.is_empty()
        && title_glyph_width(
            card,
            title_clip(card, l.title, l.title_line_height * scale),
            l.title_line_height * scale,
        ) >= 100.0 * scale
        && l.title.width * horizontal_scale >= 100.0
        && ((l.wide
            && l.facts
                .iter()
                .all(|r| fits(*r) && r.width * horizontal_scale >= 60.0)
            && fits(l.source)
            && l.source.height >= 24.0)
            || (!l.wide
                && facts_region.height >= TAG_BUTTON_HEIGHT * scale
                && facts_region.width * horizontal_scale >= 100.0
                && fits(facts_region)))
}

/// Only resolved readable inline sessions use this layout. Undersized Depth
/// sessions use the existing external panel layout at ordinary scale instead.
pub(crate) fn depth_layout(
    card: CardGeometry,
    radius: f32,
    facts: usize,
    actions: &[Action],
    scale: f32,
    font: Font,
    title: &str,
) -> BackLayout {
    let _ = radius;
    layout(card, true, facts, actions, scale, font, title)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn actions(n: usize) -> Vec<Action> {
        [
            "Apply",
            "Hide",
            "Playlists",
            "Open Folder",
            "Engine Properties",
            "A retained action",
        ]
        .iter()
        .take(n)
        .map(|s| Action {
            intent: DetailIntent::Apply,
            label: s.to_string(),
            enabled: true,
        })
        .collect()
    }
    fn card(w: f32, h: f32) -> CardGeometry {
        CardGeometry {
            center: [500.0, 400.0],
            size: [w, h],
            rotation: 0.0,
            skew_distance: 48.0,
            edge_tilt_distance: 0.0,
        }
    }
    #[test]
    fn supported_actions_are_contained_and_do_not_overlap_title_or_facts() {
        for (w, h, embedded) in [
            (900.0, 520.0, true),
            (420.0, 420.0, true),
            (280.0, 300.0, true),
            (1747.2, 928.8, false),
            (698.88, 440.32, false),
        ] {
            for n in [4, 5, 6] {
                let l = layout(
                    card(w, h),
                    embedded,
                    3,
                    &actions(n),
                    1.0,
                    Font::default(),
                    "Title",
                );
                for (i, a) in l.actions.iter().enumerate() {
                    assert!(
                        l.sheet.contains(a.position())
                            && l.sheet.contains(Point::new(a.x + a.width, a.y + a.height)),
                        "{w}/{h}/{n}: {a:?} in {:?}",
                        l.sheet
                    );
                    assert!(a.y >= l.title.y + l.title.height || a.x >= l.title.x + l.title.width);
                    for b in &l.actions[..i] {
                        assert!(a.intersection(b).is_none());
                    }
                }
                assert!(l.masthead.contains(l.favorite.center()));
            }
        }
    }
    #[test]
    fn wide_and_external_relationships_match_pinned_formulas() {
        let c = card(900.0, 520.0);
        let l = layout(c, true, 3, &actions(4), 1.0, Font::default(), "Title");
        assert!(l.wide);
        assert_eq!(l.masthead.height, 32.0);
        assert_eq!(l.sheet.width, 876.0);
        assert!(l.title.x + l.title.width < l.facts[0].x);
        assert!(l.actions[0].x > l.title.x);
        let l = layout(
            card(1747.2, 928.8),
            false,
            3,
            &actions(4),
            1.0,
            Font::default(),
            "Title",
        );
        assert!((l.sheet.width - 410.0).abs() < 0.01);
        assert!((l.masthead.height - 48.2976).abs() < 0.01);
        assert_eq!(l.facts[0].x, l.facts[1].x);
        assert!(l.actions[0].y > l.facts[2].y + l.facts[2].height);
    }
    #[test]
    fn donor_ellipsis_is_unicode_safe_and_does_not_wrap() {
        for s in ["long title ".repeat(25), "山の風景と夕暮れ".repeat(10)] {
            let t = ellipsize(&s, 25.0, 180.0);
            assert!(t.ends_with('…'));
            assert!(text_width(&t, 25.0) <= 180.0);
        }
    }
}

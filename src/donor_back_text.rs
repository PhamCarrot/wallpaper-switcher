//! Font measurement and cluster-safe bounded lines for the shared back surface.
use iced::advanced::{
    graphics::text::Paragraph,
    text::{self, Paragraph as _},
};
use iced::{alignment, Font, Size};
use std::{cell::RefCell, collections::VecDeque};

#[derive(PartialEq)]
struct FitKey {
    value: String,
    size: u32,
    width: u32,
    font: Font,
    version: iced::advanced::graphics::text::Version,
    max_lines: usize,
}

thread_local! {
    // Draw, hit testing and updates repeatedly fit the same detail text.
    static FITS: RefCell<VecDeque<(FitKey, Vec<String>)>> = const { RefCell::new(VecDeque::new()) };
}

pub(super) fn paragraph(value: &str, size: f32, font: Font) -> Paragraph {
    Paragraph::with_text(text::Text {
        content: value,
        bounds: Size::new(f32::INFINITY, f32::INFINITY),
        size: size.into(),
        line_height: text::LineHeight::Relative(1.25),
        font,
        align_x: text::Alignment::Left,
        align_y: alignment::Vertical::Top,
        shaping: text::Shaping::Advanced,
        wrapping: text::Wrapping::None,
    })
}

pub(crate) fn measured_width(value: &str, size: f32, font: Font) -> f32 {
    paragraph(value, size, font).min_width()
}

// Shaper clusters retain combining marks and ligatures together. Boundaries
// refer to the normalized single line, so byte offsets never cross UTF8 units.
fn boundaries(value: &str, size: f32, font: Font) -> Vec<usize> {
    let p = paragraph(value, size, font);
    let mut ends: Vec<_> = p
        .buffer()
        .layout_runs()
        .flat_map(|run| run.glyphs.iter().map(|g| g.end))
        .collect();
    ends.extend([0, value.len()]);
    ends.sort_unstable();
    ends.dedup();
    ends.retain(|end| {
        value.is_char_boundary(*end)
            && p.buffer()
                .layout_runs()
                .all(|run| run.glyphs.iter().all(|g| !(g.start < *end && *end < g.end)))
    });
    ends
}

/// Full identity text: preserve line boundaries/whitespace and always advance a
/// positive shaped cluster, even when the available width is smaller than it.
pub(crate) fn measured_full_lines(value: &str, size: f32, width: f32, font: Font) -> Vec<String> {
    let mut lines = Vec::new();
    for line in value.split('\n') {
        if line.is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut remaining = line;
        while !remaining.is_empty() {
            let ends = boundaries(remaining, size, font);
            let positive = ends
                .iter()
                .copied()
                .filter(|end| *end > 0)
                .collect::<Vec<_>>();
            let Some(first) = positive.first().copied() else {
                break;
            };
            let end = positive
                .iter()
                .copied()
                .take_while(|end| measured_width(&remaining[..*end], size, font) <= width)
                .last()
                .unwrap_or(first);
            lines.push(remaining[..end].to_owned());
            remaining = &remaining[end..];
        }
    }
    lines
}

pub(crate) fn measured_lines(
    value: &str,
    size: f32,
    width: f32,
    font: Font,
    max_lines: usize,
) -> Vec<String> {
    let _timing = crate::apply_trace::cpu_span("detail-text-fit", || {
        format!(
            "bytes={} size={size} width={width} lines={max_lines}",
            value.len()
        )
    });
    // Bound retained text as well as entry count; oversized inputs still use
    // the exact same shaper without being retained.
    if value.len() > 4096 || max_lines > 2 || !size.is_finite() || !width.is_finite() {
        return fit_lines(value, size, width, font, max_lines);
    }
    let key = FitKey {
        value: value.to_owned(),
        size: size.to_bits(),
        width: width.to_bits(),
        font,
        version: iced::advanced::graphics::text::font_system()
            .read()
            .unwrap()
            .version(),
        max_lines,
    };
    if let Some(lines) = FITS.with(|fits| {
        let mut fits = fits.borrow_mut();
        let index = fits.iter().position(|(cached, _)| cached == &key)?;
        let entry = fits.remove(index).unwrap();
        let lines = entry.1.clone();
        fits.push_front(entry);
        Some(lines)
    }) {
        return lines;
    }
    let lines = fit_lines(value, size, width, font, max_lines);
    FITS.with(|fits| {
        let mut fits = fits.borrow_mut();
        fits.truncate(31);
        fits.push_front((key, lines.clone()));
    });
    lines
}

fn fit_lines(value: &str, size: f32, width: f32, font: Font, max_lines: usize) -> Vec<String> {
    let value = value.replace(['\n', '\r', '\t'], " ");
    if value.is_empty() || max_lines == 0 || width <= 0.0 {
        return Vec::new();
    }
    let mut remaining = value.as_str();
    let mut lines = Vec::new();
    for line in 0..max_lines {
        if measured_width(remaining, size, font) <= width {
            lines.push(remaining.to_owned());
            break;
        }
        let final_line = line + 1 == max_lines;
        let suffix = if final_line { "…" } else { "" };
        if measured_width(suffix, size, font) > width {
            break;
        }
        let ends = boundaries(remaining, size, font);
        let fit = ends.partition_point(|end| {
            measured_width(&format!("{}{suffix}", &remaining[..*end]), size, font) <= width
        });
        let mut end = ends[fit.saturating_sub(1)];
        if !final_line {
            if let Some(space) = remaining[..end].rfind(' ') {
                if space > 0 && ends.binary_search(&space).is_ok() {
                    end = space;
                }
            }
        }
        lines.push(format!("{}{suffix}", remaining[..end].trim_end()));
        remaining = remaining[end..].trim_start();
        if remaining.is_empty() || final_line {
            break;
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_and_changing_detail_fits_preserve_shaped_lines() {
        FITS.with(|fits| fits.borrow_mut().clear());
        let font = Font::default();
        for value in [
            include_str!("fixtures/e2-workshop-title.txt"),
            "A long office affine title with ligatures and Cafe\u{301} combining marks",
            "日本語と中文の長い見出し\nwith\tspacing and emoji 👩‍💻",
        ] {
            for (size, width, max_lines) in [(23.0, 243.08984, 1), (32.0, 356.7, 2)] {
                let expected = fit_lines(value, size, width, font, max_lines);
                assert_eq!(
                    measured_lines(value, size, width, font, max_lines),
                    expected
                );
                // Nearby animated widths must retain exact fit boundaries.
                let moving_width = f32::from_bits(width.to_bits() + 1);
                assert_eq!(
                    measured_lines(value, size, moving_width, font, max_lines),
                    fit_lines(value, size, moving_width, font, max_lines)
                );
                assert_eq!(
                    measured_lines(value, size, width, font, max_lines),
                    expected
                );
                assert_eq!(
                    measured_lines(value, size + 1.0, width, font, max_lines),
                    fit_lines(value, size + 1.0, width, font, max_lines)
                );
            }
        }
        for n in 0..40 {
            measured_lines(&format!("distinct detail {n}"), 20.0, 120.0, font, 1);
        }
        FITS.with(|fits| assert_eq!(fits.borrow().len(), 32));
        let retained = FITS.with(|fits| fits.borrow().len());
        let oversized = "x".repeat(4097);
        assert_eq!(
            measured_lines(&oversized, 20.0, 120.0, font, 1),
            fit_lines(&oversized, 20.0, 120.0, font, 1)
        );
        FITS.with(|fits| assert_eq!(fits.borrow().len(), retained));
    }

    #[test]
    fn font_load_and_font_choice_invalidate_detail_fits() {
        FITS.with(|fits| fits.borrow_mut().clear());
        let value = "A long font-dependent office title";
        let original = measured_lines(value, 23.0, 120.0, Font::default(), 1);
        let old_version = iced::advanced::graphics::text::font_system()
            .read()
            .unwrap()
            .version();
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Owned(
                include_bytes!("../assets/fonts/RobotoCondensed-Bold-UI.ttf").to_vec(),
            ));
        let font = Font {
            family: iced::font::Family::Name("Roboto Condensed"),
            weight: iced::font::Weight::Bold,
            ..Font::default()
        };
        for font in [Font::default(), font] {
            assert_eq!(
                measured_lines(value, 23.0, 120.0, font, 1),
                fit_lines(value, 23.0, 120.0, font, 1)
            );
        }
        FITS.with(|fits| {
            let fits = fits.borrow();
            assert_eq!(fits.len(), 3);
            assert_ne!(fits.front().unwrap().0.version, old_version);
            assert_eq!(fits.back().unwrap().1, original);
        });
    }
}

//! beta.26 sort analysis, independent of W5 filtering metrics and displayed crops.
//! Adapted from MIT skwd-deck 3e8df6d, infrastructure/media/images.rs.
use image::{imageops::FilterType, DynamicImage};
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, collections::HashMap};

pub const VERSION: u32 = 1;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SortMetrics {
    pub version: u32,
    pub hue: u16,
    /// Dominant chromatic bucket coverage, not HSV saturation.
    pub saturation: u16,
    pub richness: u16,
}
pub type MetadataView = HashMap<String, SortMetrics>;
impl SortMetrics {
    pub fn valid(&self) -> bool {
        self.version == VERSION
            && (self.hue < 12 || self.hue == 99)
            && self.saturation <= 100
            && self.richness <= 1500
            && ((self.hue == 99 && self.saturation == 0)
                || (self.hue < 12 && self.saturation >= 10))
    }
}
fn bucket(hue: u16) -> usize {
    if !(25..355).contains(&hue) {
        0
    } else if hue >= 320 {
        11
    } else if hue >= 295 {
        10
    } else {
        ((hue - 25) / 30 + 1) as usize
    }
}
fn pixel_bucket([r, g, b]: [u8; 3]) -> Option<usize> {
    let [r, g, b] = [r, g, b].map(|v| f64::from(v) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let lightness = f64::midpoint(max, min);
    if !(0.06..=0.94).contains(&lightness) {
        return None;
    }
    let sat = if delta < 1e-6 {
        0.0
    } else {
        delta / (1.0 - 2.0f64.mul_add(lightness, -1.0).abs())
    };
    if sat < 0.18 {
        return Some(12);
    }
    let hue = if (max - r).abs() < 1e-6 {
        60.0 * (((g - b) / delta) % 6.0)
    } else if (max - g).abs() < 1e-6 {
        60.0f64.mul_add((b - r) / delta, 120.0)
    } else {
        60.0f64.mul_add((r - g) / delta, 240.0)
    };
    let hue = if hue < 0.0 { hue + 360.0 } else { hue };
    Some(bucket((hue.round() as u16) % 360))
}
#[cfg(test)]
fn extract(img: &DynamicImage) -> SortMetrics {
    extract_cancellable(img, &|| false).unwrap()
}

fn extract_cancellable(img: &DynamicImage, cancelled: &dyn Fn() -> bool) -> Option<SortMetrics> {
    let mut counts = [0u64; 13];
    for (index, px) in img.to_rgba8().pixels().enumerate() {
        if index % 1024 == 0 && cancelled() {
            return None;
        }
        // The reference recipe ignores alpha; W5 retains its alpha-weighted recipe.
        if let Some(bucket) = pixel_bucket([px[0], px[1], px[2]]) {
            counts[bucket] += 1;
        }
    }
    let meaningful: u64 = counts.iter().sum();
    let chromatic: u64 = counts[..12].iter().sum();
    let mut best = 0;
    for i in 1..12 {
        if counts[i] > counts[best] {
            best = i;
        }
    }
    let sumsq = counts[..12]
        .iter()
        .filter(|&&n| n > 0)
        .map(|&n| {
            let fraction = n as f64 / chromatic as f64;
            fraction * fraction
        })
        .sum::<f64>();
    let richness = if sumsq > 0.0 {
        ((1.0 / sumsq) * 100.0).round().clamp(0.0, 1500.0) as u16
    } else {
        0
    };
    let (hue, saturation) = if meaningful == 0 || chromatic * 100 < meaningful * 5 {
        (99, 0)
    } else {
        (
            best as u16,
            ((counts[best] as f64 / meaningful as f64) * 100.0)
                .round()
                .clamp(10.0, 100.0) as u16,
        )
    };
    Some(SortMetrics {
        version: VERSION,
        hue,
        saturation,
        richness,
    })
}
#[cfg(test)]
pub fn analyse(img: &DynamicImage) -> SortMetrics {
    analyse_cancellable(img, &|| false).unwrap()
}

pub fn analyse_cancellable(
    img: &DynamicImage,
    cancelled: &dyn Fn() -> bool,
) -> Option<SortMetrics> {
    if cancelled() {
        return None;
    }
    let (w, h) = (img.width(), img.height());
    let scale = (1280.0 / f64::from(w)).max(720.0 / f64::from(h));
    let base = if scale < 1.0 {
        Some(img.thumbnail(
            (f64::from(w) * scale).ceil().max(1.0) as u32,
            (f64::from(h) * scale).ceil().max(1.0) as u32,
        ))
    } else {
        None
    };
    if cancelled() {
        return None;
    }
    extract_cancellable(
        &base
            .as_ref()
            .unwrap_or(img)
            .resize_to_fill(640, 360, FilterType::Lanczos3),
        cancelled,
    )
}
pub fn compare(
    a: Option<&SortMetrics>,
    b: Option<&SortMetrics>,
    order: super::SortOrder,
) -> Ordering {
    match (a.filter(|m| m.valid()), b.filter(|m| m.valid())) {
        (Some(a), Some(b)) => match order {
            super::SortOrder::ColourPop => {
                let pop = |m: &SortMetrics| f64::from(m.saturation) - f64::from(m.richness) / 15.0;
                pop(b).total_cmp(&pop(a))
            }
            super::SortOrder::Richness => b
                .richness
                .cmp(&a.richness)
                .then(b.saturation.cmp(&a.saturation)),
            super::SortOrder::Minimalist => a
                .richness
                .cmp(&b.richness)
                .then(b.saturation.cmp(&a.saturation)),
            _ => a.hue.cmp(&b.hue).then(b.saturation.cmp(&a.saturation)),
        },
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        _ => Ordering::Equal,
    }
}
#[cfg(test)]
#[path = "sort_metrics_tests.rs"]
mod tests;

//! Version 1 still metrics. Image luminance is linear light; Theme ordering is encoded RGB.
use sha2::{Digest, Sha256};

pub(crate) fn hash_bytes(bytes: &[u8]) -> String {
    format!("{:X}", Sha256::digest(bytes))
}

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const ALGORITHM_VERSION: u32 = 1;
pub const SAMPLE_EDGE: u32 = 128;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorMetadata {
    pub version: u32,
    /// SHA256 of the bounded encoded still bytes, never the size/mtime stamp.
    pub fingerprint: String,
    pub dominant_rgb: [u8; 3],
    pub hue: Option<f32>,
    pub saturation: f32,
    pub brightness: f32,
    pub richness: f32,
}
impl ColorMetadata {
    pub fn valid(&self) -> bool {
        self.version == ALGORITHM_VERSION
            && self.fingerprint.len() == 64
            && self.fingerprint.bytes().all(|c| c.is_ascii_hexdigit())
            && [self.saturation, self.brightness, self.richness]
                .iter()
                .all(|n| n.is_finite() && (0.0..=1.0).contains(n))
            && self
                .hue
                .is_none_or(|n| n.is_finite() && (0.0..360.0).contains(&n))
    }
}

pub fn linear_luminance(rgb: [u8; 3]) -> f32 {
    let linear = |v: u8| {
        let n = v as f32 / 255.0;
        if n <= 0.04045 {
            n / 12.92
        } else {
            ((n + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * linear(rgb[0]) + 0.7152 * linear(rgb[1]) + 0.0722 * linear(rgb[2])
}
/// Occupied 5-bit RGB bins / min(32768, count of nonzero-alpha sample pixels).
/// A single uniform sample has richness 1; a uniform 128x128 still has 1/16384.
/// Alpha weights metrics/bin winners, while nonzero-alpha occupancy counts samples once.
pub fn occupied_bin_richness(occupied: usize, valid_samples: usize) -> f32 {
    if valid_samples == 0 {
        return 0.0;
    }
    occupied.min(valid_samples.min(32768)) as f32 / valid_samples.min(32768) as f32
}
pub fn analyse_sample(
    sample: &image::RgbaImage,
    fingerprint: String,
) -> Result<ColorMetadata, String> {
    if sample.width() == 0
        || sample.height() == 0
        || sample.width() > SAMPLE_EDGE
        || sample.height() > SAMPLE_EDGE
    {
        return Err("Analysis sample exceeds 128×128 or is empty".into());
    }
    let mut bins = vec![(0u64, [0u64; 3]); 32768];
    let (mut weight, mut saturation, mut luminance, mut valid) = (0.0f64, 0.0f64, 0.0f64, 0usize);
    for pixel in sample.pixels() {
        let [r, g, b, a] = pixel.0;
        if a == 0 {
            continue;
        }
        let rgb = [r, g, b];
        let bin = ((r as usize >> 3) << 10) | ((g as usize >> 3) << 5) | (b as usize >> 3);
        bins[bin].0 += a as u64;
        for (sum, value) in bins[bin].1.iter_mut().zip(rgb) {
            *sum += value as u64 * a as u64;
        }
        let (_, s, _) = crate::donor_folio::source_ui::color::rgb_to_hsv(rgb);
        weight += a as f64;
        saturation += s as f64 * a as f64;
        luminance += linear_luminance(rgb) as f64 * a as f64;
        valid += 1;
    }
    if valid == 0 {
        return Err("Transparent-only sample: colour unavailable".into());
    }
    // First bin wins equal weights, so scan order cannot change the dominant.
    let mut winner = 0;
    for i in 1..bins.len() {
        if bins[i].0 > bins[winner].0 {
            winner = i;
        }
    }
    let (n, sums) = bins[winner];
    let dominant_rgb = sums.map(|v| ((v + n / 2) / n) as u8);
    let (h, s, _) = crate::donor_folio::source_ui::color::rgb_to_hsv(dominant_rgb);
    Ok(ColorMetadata {
        version: ALGORITHM_VERSION,
        fingerprint,
        dominant_rgb,
        hue: (s > 0.05).then_some(h),
        saturation: (saturation / weight) as f32,
        brightness: (luminance / weight) as f32,
        richness: occupied_bin_richness(bins.iter().filter(|b| b.0 > 0).count(), valid),
    })
}

/// Closed quick-filter vocabulary. Unknown metadata never becomes Gray.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorBucket {
    Red,
    Orange,
    Yellow,
    Lime,
    Green,
    Spring,
    Cyan,
    Azure,
    Blue,
    Violet,
    Magenta,
    Rose,
    Gray,
}
impl ColorBucket {
    pub const ALL: [Self; 13] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Lime,
        Self::Green,
        Self::Spring,
        Self::Cyan,
        Self::Azure,
        Self::Blue,
        Self::Violet,
        Self::Magenta,
        Self::Rose,
        Self::Gray,
    ];
    pub fn from_index(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }
    pub fn from_metadata(metadata: &ColorMetadata) -> Option<Self> {
        if !metadata.valid() {
            return None;
        }
        Some(match metadata.hue {
            Some(hue) => Self::ALL[(((f64::from(hue) + 15.0) / 30.0).floor() as usize) % 12],
            None => Self::Gray,
        })
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Red => "Red",
            Self::Orange => "Orange",
            Self::Yellow => "Yellow",
            Self::Lime => "Lime",
            Self::Green => "Green",
            Self::Spring => "Spring",
            Self::Cyan => "Cyan",
            Self::Azure => "Azure",
            Self::Blue => "Blue",
            Self::Violet => "Violet",
            Self::Magenta => "Magenta",
            Self::Rose => "Rose",
            Self::Gray => "Gray",
        }
    }
    pub fn rgb(self) -> [u8; 3] {
        match self {
            Self::Red => [230, 45, 45],
            Self::Orange => [230, 138, 45],
            Self::Yellow => [230, 230, 45],
            Self::Lime => [138, 230, 45],
            Self::Green => [45, 230, 45],
            Self::Spring => [45, 230, 138],
            Self::Cyan => [45, 230, 230],
            Self::Azure => [45, 138, 230],
            Self::Blue => [45, 45, 230],
            Self::Violet => [138, 45, 230],
            Self::Magenta => [230, 45, 230],
            Self::Rose => [230, 45, 138],
            Self::Gray => [150; 3],
        }
    }
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ColorQuery {
    pub bucket: Option<ColorBucket>,
}
pub type MetadataView = HashMap<String, ColorMetadata>;
impl ColorQuery {
    pub fn explicit(&self) -> bool {
        self.bucket.is_some()
    }
    pub fn toggle(&mut self, bucket: ColorBucket) {
        self.bucket = (self.bucket != Some(bucket)).then_some(bucket);
    }
    pub fn matches(&self, value: Option<&ColorMetadata>) -> bool {
        self.bucket
            .is_none_or(|bucket| value.and_then(ColorBucket::from_metadata) == Some(bucket))
    }
}

#[cfg(test)]
#[path = "color_metadata_tests.rs"]
mod tests;

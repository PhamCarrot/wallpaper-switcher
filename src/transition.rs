use std::path::Path;

use image::imageops::FilterType;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionEffect {
    Off,
    Random,
    Crossfade,
    WipeLeft,
    WipeRight,
    WipeUp,
    WipeDown,
    Slices,
    Blinds,
    Checker,
    Pixelate,
    Iris,
    Ring,
    Diamond,
    Circle,
    Mosaic,
    Voronoi,
    Kaleidoscope,
    ChromaticRipple,
    Glitch,
    Warp,
    ZoomBlur,
    RadialBlur,
    Swirl,
    Displacement,
    Wave,
    NoiseDissolve,
    Ink,
    Smoke,
    Burn,
    Luma,
    Film,
}

pub const TRANSITION_CATALOG: [TransitionEffect; 30] = [
    TransitionEffect::Crossfade,
    TransitionEffect::WipeLeft,
    TransitionEffect::WipeRight,
    TransitionEffect::WipeUp,
    TransitionEffect::WipeDown,
    TransitionEffect::Slices,
    TransitionEffect::Blinds,
    TransitionEffect::Checker,
    TransitionEffect::Pixelate,
    TransitionEffect::Iris,
    TransitionEffect::Ring,
    TransitionEffect::Diamond,
    TransitionEffect::Circle,
    TransitionEffect::Mosaic,
    TransitionEffect::Voronoi,
    TransitionEffect::Kaleidoscope,
    TransitionEffect::ChromaticRipple,
    TransitionEffect::Glitch,
    TransitionEffect::Warp,
    TransitionEffect::ZoomBlur,
    TransitionEffect::RadialBlur,
    TransitionEffect::Swirl,
    TransitionEffect::Displacement,
    TransitionEffect::Wave,
    TransitionEffect::NoiseDissolve,
    TransitionEffect::Ink,
    TransitionEffect::Smoke,
    TransitionEffect::Burn,
    TransitionEffect::Luma,
    TransitionEffect::Film,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionMigration {
    RemovedSand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParsedTransitionEffect {
    pub effect: TransitionEffect,
    pub migration: Option<TransitionMigration>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionEffectParseError {
    value: String,
}

impl std::fmt::Display for TransitionEffectParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "unknown transition effect {:?}; expected off, random, or one of the 30 catalog effects",
            self.value
        )
    }
}

impl std::error::Error for TransitionEffectParseError {}

impl TransitionEffect {
    pub const fn key(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Random => "random",
            Self::Crossfade => "crossfade",
            Self::WipeLeft => "wipe-left",
            Self::WipeRight => "wipe-right",
            Self::WipeUp => "wipe-up",
            Self::WipeDown => "wipe-down",
            Self::Slices => "slices",
            Self::Blinds => "blinds",
            Self::Checker => "checker",
            Self::Pixelate => "pixelate",
            Self::Iris => "iris",
            Self::Ring => "ring",
            Self::Diamond => "diamond",
            Self::Circle => "circle",
            Self::Mosaic => "mosaic",
            Self::Voronoi => "voronoi",
            Self::Kaleidoscope => "kaleidoscope",
            Self::ChromaticRipple => "chromatic-ripple",
            Self::Glitch => "glitch",
            Self::Warp => "warp",
            Self::ZoomBlur => "zoom-blur",
            Self::RadialBlur => "radial-blur",
            Self::Swirl => "swirl",
            Self::Displacement => "displacement",
            Self::Wave => "wave",
            Self::NoiseDissolve => "noise-dissolve",
            Self::Ink => "ink",
            Self::Smoke => "smoke",
            Self::Burn => "burn",
            Self::Luma => "luma",
            Self::Film => "film",
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Random => "Random",
            Self::Crossfade => "Crossfade",
            Self::WipeLeft => "Wipe left",
            Self::WipeRight => "Wipe right",
            Self::WipeUp => "Wipe up",
            Self::WipeDown => "Wipe down",
            Self::Slices => "Slices",
            Self::Blinds => "Blinds",
            Self::Checker => "Checker",
            Self::Pixelate => "Pixelate dissolve",
            Self::Iris => "Iris",
            Self::Ring => "Ring",
            Self::Diamond => "Diamond",
            Self::Circle => "Circle",
            Self::Mosaic => "Mosaic",
            Self::Voronoi => "Voronoi",
            Self::Kaleidoscope => "Kaleidoscope",
            Self::ChromaticRipple => "Chromatic ripple",
            Self::Glitch => "Glitch",
            Self::Warp => "Warp",
            Self::ZoomBlur => "Zoom blur",
            Self::RadialBlur => "Radial blur",
            Self::Swirl => "Swirl",
            Self::Displacement => "Displacement",
            Self::Wave => "Wave",
            Self::NoiseDissolve => "Noise dissolve",
            Self::Ink => "Ink",
            Self::Smoke => "Smoke",
            Self::Burn => "Burn",
            Self::Luma => "Luma",
            Self::Film => "Film",
        }
    }

    pub fn parse_saved(value: &str) -> Result<ParsedTransitionEffect, TransitionEffectParseError> {
        let normalized = normalize_effect_name(value);
        for effect in std::iter::once(Self::Off)
            .chain(std::iter::once(Self::Random))
            .chain(TRANSITION_CATALOG)
        {
            if normalized == normalize_effect_name(effect.key())
                || normalized == normalize_effect_name(effect.display_name())
            {
                return Ok(ParsedTransitionEffect {
                    effect,
                    migration: None,
                });
            }
        }

        let legacy = match normalized.as_str() {
            "none" => Some(Self::Off),
            "wipe" => Some(Self::WipeLeft),
            "ripple" => Some(Self::ChromaticRipple),
            _ => None,
        };
        if let Some(effect) = legacy {
            return Ok(ParsedTransitionEffect {
                effect,
                migration: None,
            });
        }

        if matches!(normalized.as_str(), "sand" | "sandy" | "sanddissolve") {
            return Ok(ParsedTransitionEffect {
                effect: Self::Crossfade,
                migration: Some(TransitionMigration::RemovedSand),
            });
        }

        Err(TransitionEffectParseError {
            value: value.to_string(),
        })
    }

    pub fn resolve(self, sample: u64) -> Self {
        if self == Self::Random {
            TRANSITION_CATALOG[sample as usize % TRANSITION_CATALOG.len()]
        } else {
            self
        }
    }
}

fn normalize_effect_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

impl Serialize for TransitionEffect {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.key())
    }
}

impl<'de> Deserialize<'de> for TransitionEffect {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::parse_saved(&String::deserialize(deserializer)?)
            .map(|parsed| parsed.effect)
            .map_err(D::Error::custom)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionConfig {
    #[serde(default = "default_effect")]
    pub effect: TransitionEffect,
    #[serde(default = "default_duration_ms")]
    pub duration_ms: u64,
}

fn default_effect() -> TransitionEffect {
    TransitionEffect::Crossfade
}

fn default_duration_ms() -> u64 {
    650
}

impl Default for TransitionConfig {
    fn default() -> Self {
        Self {
            effect: default_effect(),
            duration_ms: default_duration_ms(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrozenFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl FrozenFrame {
    pub fn from_path_cover(path: &Path, width: u32, height: u32) -> Result<Self, String> {
        let source = image::open(path)
            .map_err(|error| {
                format!(
                    "Could not decode transition frame {}: {error}",
                    path.display()
                )
            })?
            .to_rgba8();
        let scale =
            (width as f64 / source.width() as f64).max(height as f64 / source.height() as f64);
        let scaled_width = (source.width() as f64 * scale).round().max(1.0) as u32;
        let scaled_height = (source.height() as f64 * scale).round().max(1.0) as u32;
        let scaled =
            image::imageops::resize(&source, scaled_width, scaled_height, FilterType::Triangle);
        let left = (scaled_width.saturating_sub(width)) / 2;
        let top = (scaled_height.saturating_sub(height)) / 2;
        let cropped = image::imageops::crop_imm(&scaled, left, top, width, height).to_image();
        Ok(Self {
            width,
            height,
            rgba: cropped.into_raw(),
        })
    }

    fn compatible_with(&self, other: &Self) -> bool {
        self.width == other.width
            && self.height == other.height
            && self.rgba.len() == other.rgba.len()
            && self.rgba.len() == (self.width * self.height * 4) as usize
    }
}

pub fn render_effect(
    effect: TransitionEffect,
    outgoing: &FrozenFrame,
    incoming: &FrozenFrame,
    progress: f32,
) -> Result<FrozenFrame, String> {
    if !outgoing.compatible_with(incoming) {
        return Err("Transition frames must have identical valid dimensions".to_string());
    }
    let effect = effect.resolve(0);
    let progress = progress.clamp(0.0, 1.0);
    if progress <= 0.0 || effect == TransitionEffect::Off {
        return Ok(outgoing.clone());
    }
    if progress >= 1.0 {
        return Ok(incoming.clone());
    }

    let mut rgba = vec![0; outgoing.rgba.len()];
    match effect {
        TransitionEffect::Off => unreachable!(),
        TransitionEffect::Random => unreachable!(),
        TransitionEffect::Crossfade => crossfade(&mut rgba, outgoing, incoming, progress),
        TransitionEffect::WipeLeft => wipe_direction(&mut rgba, outgoing, incoming, progress, 0),
        TransitionEffect::WipeRight => wipe_direction(&mut rgba, outgoing, incoming, progress, 1),
        TransitionEffect::WipeUp => wipe_direction(&mut rgba, outgoing, incoming, progress, 2),
        TransitionEffect::WipeDown => wipe_direction(&mut rgba, outgoing, incoming, progress, 3),
        TransitionEffect::Slices => patterned_reveal(&mut rgba, outgoing, incoming, progress, 0),
        TransitionEffect::Blinds => patterned_reveal(&mut rgba, outgoing, incoming, progress, 1),
        TransitionEffect::Checker => patterned_reveal(&mut rgba, outgoing, incoming, progress, 2),
        TransitionEffect::Pixelate => pixelate(&mut rgba, outgoing, incoming, progress),
        TransitionEffect::Iris => radial_reveal(&mut rgba, outgoing, incoming, progress, 0),
        TransitionEffect::Ring => radial_reveal(&mut rgba, outgoing, incoming, progress, 1),
        TransitionEffect::Diamond => radial_reveal(&mut rgba, outgoing, incoming, progress, 2),
        TransitionEffect::Circle => radial_reveal(&mut rgba, outgoing, incoming, progress, 3),
        TransitionEffect::Mosaic => patterned_reveal(&mut rgba, outgoing, incoming, progress, 3),
        TransitionEffect::Voronoi => patterned_reveal(&mut rgba, outgoing, incoming, progress, 4),
        TransitionEffect::Kaleidoscope => {
            distorted_blend(&mut rgba, outgoing, incoming, progress, 0)
        }
        TransitionEffect::ChromaticRipple => ripple(&mut rgba, outgoing, incoming, progress),
        TransitionEffect::Glitch => distorted_blend(&mut rgba, outgoing, incoming, progress, 1),
        TransitionEffect::Warp => distorted_blend(&mut rgba, outgoing, incoming, progress, 2),
        TransitionEffect::ZoomBlur => distorted_blend(&mut rgba, outgoing, incoming, progress, 3),
        TransitionEffect::RadialBlur => distorted_blend(&mut rgba, outgoing, incoming, progress, 4),
        TransitionEffect::Swirl => distorted_blend(&mut rgba, outgoing, incoming, progress, 5),
        TransitionEffect::Displacement => {
            distorted_blend(&mut rgba, outgoing, incoming, progress, 6)
        }
        TransitionEffect::Wave => distorted_blend(&mut rgba, outgoing, incoming, progress, 7),
        TransitionEffect::NoiseDissolve => {
            organic_reveal(&mut rgba, outgoing, incoming, progress, 0)
        }
        TransitionEffect::Ink => organic_reveal(&mut rgba, outgoing, incoming, progress, 1),
        TransitionEffect::Smoke => organic_reveal(&mut rgba, outgoing, incoming, progress, 2),
        TransitionEffect::Burn => organic_reveal(&mut rgba, outgoing, incoming, progress, 3),
        TransitionEffect::Luma => organic_reveal(&mut rgba, outgoing, incoming, progress, 4),
        TransitionEffect::Film => organic_reveal(&mut rgba, outgoing, incoming, progress, 5),
    }
    Ok(FrozenFrame {
        width: outgoing.width,
        height: outgoing.height,
        rgba,
    })
}

fn crossfade(output: &mut [u8], from: &FrozenFrame, to: &FrozenFrame, progress: f32) {
    for ((out, a), b) in output.iter_mut().zip(&from.rgba).zip(&to.rgba) {
        *out = (*a as f32 + (*b as f32 - *a as f32) * progress).round() as u8;
    }
}

fn wipe_direction(
    output: &mut [u8],
    from: &FrozenFrame,
    to: &FrozenFrame,
    progress: f32,
    direction: u32,
) {
    let span = if direction < 2 {
        from.width
    } else {
        from.height
    } as f32;
    let feather = (span * 0.025).clamp(8.0, 48.0);
    let edge = (span + feather) * progress - feather;
    for y in 0..from.height {
        for x in 0..from.width {
            let coordinate = match direction {
                0 => x as f32,
                1 => (from.width - 1 - x) as f32,
                2 => y as f32,
                _ => (from.height - 1 - y) as f32,
            };
            let blend = ((edge - coordinate) / feather + 0.5).clamp(0.0, 1.0);
            blend_pixel(output, from, to, x, y, x, y, blend);
        }
    }
}

fn patterned_reveal(
    output: &mut [u8],
    from: &FrozenFrame,
    to: &FrozenFrame,
    progress: f32,
    style: u32,
) {
    let eased = smoothstep(progress);
    for y in 0..from.height {
        for x in 0..from.width {
            let threshold = match style {
                0 => ((x / 28) as f32 / (from.width / 28).max(1) as f32 + (y % 28) as f32 / 84.0)
                    .fract(),
                1 => (y % 48) as f32 / 48.0,
                2 => (((x / 48 + y / 48) % 2) as f32 * 0.45) + (x % 48) as f32 / 110.0,
                3 => hash01(x / 20, y / 20, 0x92f1_42a7),
                _ => hash01(x / 12, y / 12, 0x5c21_a0de),
            };
            let blend = ((eased - threshold) * 12.0 + 0.5).clamp(0.0, 1.0);
            blend_pixel(output, from, to, x, y, x, y, blend);
        }
    }
}

fn radial_reveal(
    output: &mut [u8],
    from: &FrozenFrame,
    to: &FrozenFrame,
    progress: f32,
    style: u32,
) {
    let cx = (from.width - 1) as f32 * 0.5;
    let cy = (from.height - 1) as f32 * 0.5;
    let max_radius = cx.hypot(cy).max(1.0);
    for y in 0..from.height {
        for x in 0..from.width {
            let nx = (x as f32 - cx) / max_radius;
            let ny = (y as f32 - cy) / max_radius;
            let distance = match style {
                2 => nx.abs() + ny.abs(),
                3 => {
                    ((x as f32 / from.width as f32) - 0.5)
                        .hypot((y as f32 / from.height as f32) - 0.5)
                        * 1.4
                }
                _ => nx.hypot(ny),
            };
            let threshold = if style == 1 {
                (distance - progress).abs() - 0.12 * progress
            } else {
                distance - progress
            };
            let blend = (0.5 - threshold * 20.0).clamp(0.0, 1.0);
            blend_pixel(output, from, to, x, y, x, y, blend);
        }
    }
}

fn ripple(output: &mut [u8], from: &FrozenFrame, to: &FrozenFrame, progress: f32) {
    let decay = (1.0 - progress) * progress;
    let amplitude = 24.0 * decay;
    for y in 0..from.height {
        for x in 0..from.width {
            let wave = ((y as f32 * 0.035) + progress * std::f32::consts::TAU * 2.0).sin();
            let source_x = (x as f32 + wave * amplitude)
                .round()
                .clamp(0.0, (from.width - 1) as f32) as u32;
            blend_pixel(output, from, to, x, y, source_x, y, smoothstep(progress));
        }
    }
}

fn pixelate(output: &mut [u8], from: &FrozenFrame, to: &FrozenFrame, progress: f32) {
    let block = 18u32;
    for y in 0..from.height {
        for x in 0..from.width {
            let bx = x / block;
            let by = y / block;
            let threshold = hash01(bx, by, 0x51ed_270b);
            let choose_incoming = threshold <= smoothstep(progress);
            copy_pixel(output, if choose_incoming { to } else { from }, x, y, x, y);
        }
    }
}

fn distorted_blend(
    output: &mut [u8],
    from: &FrozenFrame,
    to: &FrozenFrame,
    progress: f32,
    style: u32,
) {
    let cx = from.width as f32 * 0.5;
    let cy = from.height as f32 * 0.5;
    let amplitude = (progress * (1.0 - progress) * 4.0) * 28.0;
    for y in 0..from.height {
        for x in 0..from.width {
            let dx = x as f32 - cx;
            let dy = y as f32 - cy;
            let radius = dx.hypot(dy).max(1.0);
            let angle = dy.atan2(dx);
            let (offset_x, offset_y) = match style {
                0 => ((angle * 6.0 + progress * 9.0).sin() * amplitude, 0.0),
                1 => {
                    let jump = if (y / 18 + (progress * 13.0) as u32).is_multiple_of(3) {
                        amplitude
                    } else {
                        -amplitude * 0.35
                    };
                    (jump, 0.0)
                }
                2 => (dx / radius * amplitude, dy / radius * amplitude),
                3 | 4 => (-dx / radius * amplitude, -dy / radius * amplitude),
                5 => (-dy / radius * amplitude, dx / radius * amplitude),
                6 => (
                    (hash01(x / 24, y / 24, 0x71ab_02c3) - 0.5) * amplitude,
                    (hash01(x / 24, y / 24, 0x991d_43a1) - 0.5) * amplitude,
                ),
                _ => ((y as f32 * 0.035 + progress * 12.0).sin() * amplitude, 0.0),
            };
            let source_x = (x as f32 + offset_x)
                .round()
                .clamp(0.0, (from.width - 1) as f32) as u32;
            let source_y = (y as f32 + offset_y)
                .round()
                .clamp(0.0, (from.height - 1) as f32) as u32;
            blend_pixel(
                output,
                from,
                to,
                x,
                y,
                source_x,
                source_y,
                smoothstep(progress),
            );
        }
    }
}

fn organic_reveal(
    output: &mut [u8],
    from: &FrozenFrame,
    to: &FrozenFrame,
    progress: f32,
    style: u32,
) {
    let eased = smoothstep(progress);
    for y in 0..from.height {
        for x in 0..from.width {
            let coarse = hash01(
                x / (5 + style * 3),
                y / (5 + style * 2),
                0xa511_e9b3 ^ style,
            );
            let fine = hash01(x, y, 0x63d8_35a7 ^ style.wrapping_mul(193));
            let luma = {
                let index = ((y * to.width + x) * 4) as usize;
                (to.rgba[index] as f32 * 0.2126
                    + to.rgba[index + 1] as f32 * 0.7152
                    + to.rgba[index + 2] as f32 * 0.0722)
                    / 255.0
            };
            let threshold = match style {
                1 => (coarse * 0.65 + (y as f32 / from.height as f32) * 0.35).fract(),
                2 => coarse * 0.8 + fine * 0.2,
                3 => coarse * 0.55 + (1.0 - y as f32 / from.height as f32) * 0.45,
                4 => luma * 0.5 + (x as f32 / from.width.max(1) as f32) * 0.5,
                5 => ((x as f32 / from.width as f32) * 0.7 + fine * 0.3).fract(),
                _ => coarse * 0.7 + fine * 0.3,
            };
            let blend = ((eased - threshold) * 10.0 + 0.5).clamp(0.0, 1.0);
            blend_pixel(output, from, to, x, y, x, y, blend);
        }
    }
}

fn smoothstep(value: f32) -> f32 {
    value * value * (3.0 - 2.0 * value)
}

fn hash01(x: u32, y: u32, seed: u32) -> f32 {
    let mut value = x
        .wrapping_mul(0x9e37_79b1)
        .wrapping_add(y.wrapping_mul(0x85eb_ca6b))
        .wrapping_add(seed);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    (value & 0xffff) as f32 / 65_535.0
}

fn copy_pixel(
    output: &mut [u8],
    source: &FrozenFrame,
    destination_x: u32,
    destination_y: u32,
    source_x: u32,
    source_y: u32,
) {
    let destination = ((destination_y * source.width + destination_x) * 4) as usize;
    let source_index = ((source_y * source.width + source_x) * 4) as usize;
    output[destination..destination + 4]
        .copy_from_slice(&source.rgba[source_index..source_index + 4]);
}

fn blend_pixel(
    output: &mut [u8],
    from: &FrozenFrame,
    to: &FrozenFrame,
    destination_x: u32,
    destination_y: u32,
    to_x: u32,
    to_y: u32,
    blend: f32,
) {
    let destination = ((destination_y * from.width + destination_x) * 4) as usize;
    let from_index = destination;
    let to_index = ((to_y * to.width + to_x) * 4) as usize;
    for channel in 0..4 {
        let a = from.rgba[from_index + channel] as f32;
        let b = to.rgba[to_index + channel] as f32;
        output[destination + channel] = (a + (b - a) * blend).round() as u8;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(width: u32, height: u32, color: [u8; 4]) -> FrozenFrame {
        FrozenFrame {
            width,
            height,
            rgba: color.repeat((width * height) as usize),
        }
    }

    #[test]
    fn every_effect_has_exact_endpoints() {
        let from = frame(23, 11, [8, 16, 24, 255]);
        let to = frame(23, 11, [220, 180, 140, 255]);
        for effect in TRANSITION_CATALOG {
            assert_eq!(render_effect(effect, &from, &to, 0.0).unwrap(), from);
            assert_eq!(render_effect(effect, &from, &to, 1.0).unwrap(), to);
        }
    }

    #[test]
    fn all_thirty_effects_produce_a_real_intermediate_frame() {
        let from = frame(64, 32, [0, 0, 0, 255]);
        let to = frame(64, 32, [255, 255, 255, 255]);
        for effect in TRANSITION_CATALOG {
            let middle = render_effect(effect, &from, &to, 0.5).unwrap();
            assert_ne!(middle, from, "{effect:?} stayed on the outgoing frame");
            assert_ne!(middle, to, "{effect:?} jumped to the incoming frame");
        }
    }

    #[test]
    fn invalid_frame_dimensions_are_rejected() {
        let from = frame(2, 2, [0, 0, 0, 255]);
        let to = frame(3, 2, [255, 255, 255, 255]);
        assert!(render_effect(TransitionEffect::Crossfade, &from, &to, 0.5).is_err());
    }

    #[test]
    fn catalog_contains_exactly_thirty_unique_non_sand_effects() {
        let keys = TRANSITION_CATALOG
            .iter()
            .map(|effect| effect.key())
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(TRANSITION_CATALOG.len(), 30);
        assert_eq!(keys.len(), 30);
        assert!(keys.iter().all(|key| !key.contains("sand")));
    }

    #[test]
    fn every_catalog_key_and_ui_name_round_trips_to_the_same_effect() {
        for effect in TRANSITION_CATALOG {
            for saved in [effect.key(), effect.display_name()] {
                let parsed = TransitionEffect::parse_saved(saved).unwrap();
                assert_eq!(parsed.effect, effect, "saved name {saved:?}");
                assert_eq!(parsed.migration, None, "saved name {saved:?}");
            }
        }

        let pixelate = TransitionEffect::parse_saved("Pixelate dissolve").unwrap();
        assert_eq!(pixelate.effect, TransitionEffect::Pixelate);
    }

    #[test]
    fn removed_and_legacy_saved_values_migrate_deterministically() {
        for removed in ["sand", "sandy", "sand-dissolve"] {
            let encoded = format!("effect = \"{removed}\"\nduration_ms = 650");
            let config: TransitionConfig = toml::from_str(&encoded).unwrap();
            assert_eq!(config.effect, TransitionEffect::Crossfade);
            assert_eq!(
                TransitionEffect::parse_saved(removed).unwrap().migration,
                Some(TransitionMigration::RemovedSand)
            );
        }
        for (saved, expected) in [
            ("none", TransitionEffect::Off),
            ("wipe", TransitionEffect::WipeLeft),
            ("ripple", TransitionEffect::ChromaticRipple),
        ] {
            let parsed = TransitionEffect::parse_saved(saved).unwrap();
            assert_eq!(parsed.effect, expected);
            assert_eq!(parsed.migration, None);
        }
    }

    #[test]
    fn unknown_saved_effect_is_an_error_instead_of_crossfade() {
        let error = toml::from_str::<TransitionConfig>(
            "effect = \"definitely-not-an-effect\"\nduration_ms = 650",
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("unknown transition effect"), "{error}");
        assert!(error.contains("definitely-not-an-effect"), "{error}");
    }

    #[test]
    fn random_maps_equal_sized_sample_buckets_over_the_catalog() {
        let resolved = (0..300)
            .map(|sample| TransitionEffect::Random.resolve(sample))
            .fold(std::collections::BTreeMap::new(), |mut counts, effect| {
                *counts.entry(effect.key()).or_insert(0usize) += 1;
                counts
            });

        assert_eq!(resolved.len(), 30);
        assert!(resolved.values().all(|count| *count == 10));
    }
}

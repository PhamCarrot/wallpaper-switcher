use iced::Color;
pub fn parse_hex(raw: &str) -> Option<Color> {
    hex(raw)
}

fn hex(raw: &str) -> Option<Color> {
    let digits = raw.trim().trim_start_matches('#');
    let parse = |idx: usize| u8::from_str_radix(digits.get(idx..idx + 2)?, 16).ok();
    match digits.len() {
        6 => Some(Color::from_rgb8(parse(0)?, parse(2)?, parse(4)?)),
        8 => Some(Color::from_rgba8(
            parse(2)?,
            parse(4)?,
            parse(6)?,
            u8::from_str_radix(&digits[0..2], 16).ok()? as f32 / 255.0,
        )),
        _ => None,
    }
}

pub fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> (f32, f32, f32) {
    let hue = hue.rem_euclid(360.0);
    let chroma = value * saturation;
    let intermediate = chroma * (1.0 - ((hue / 60.0).rem_euclid(2.0) - 1.0).abs());
    let offset = value - chroma;
    let (red, green, blue) = match hue as u32 {
        0..=59 => (chroma, intermediate, 0.0),
        60..=119 => (intermediate, chroma, 0.0),
        120..=179 => (0.0, chroma, intermediate),
        180..=239 => (0.0, intermediate, chroma),
        240..=299 => (intermediate, 0.0, chroma),
        _ => (chroma, 0.0, intermediate),
    };
    (red + offset, green + offset, blue + offset)
}

pub fn hsv_to_hex(hue: f32, saturation: f32, value: f32) -> String {
    let (red, green, blue) = hsv_to_rgb(hue, saturation, value);
    format!(
        "#{:02x}{:02x}{:02x}",
        (red * 255.0).round() as u8,
        (green * 255.0).round() as u8,
        (blue * 255.0).round() as u8
    )
}

pub fn hex_to_hsv(hex: &str) -> Option<(f32, f32, f32)> {
    let (red, green, blue) = parse_rgb(hex)?;
    Some(rgb_to_hsv([red, green, blue]))
}

pub fn rgb_to_hsv([red, green, blue]: [u8; 3]) -> (f32, f32, f32) {
    let (red, green, blue) = (
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
    );
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;
    let hue = if delta == 0.0 {
        0.0
    } else if max == red {
        60.0 * (((green - blue) / delta).rem_euclid(6.0))
    } else if max == green {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    (hue, saturation, max)
}

fn parse_rgb(raw: &str) -> Option<(u8, u8, u8)> {
    let digits = raw.trim().trim_start_matches('#');
    let parse = |start: usize| u8::from_str_radix(digits.get(start..start + 2)?, 16).ok();
    match digits.len() {
        6 => Some((parse(0)?, parse(2)?, parse(4)?)),
        8 => {
            parse(0)?;
            Some((parse(2)?, parse(4)?, parse(6)?))
        }
        _ => None,
    }
}

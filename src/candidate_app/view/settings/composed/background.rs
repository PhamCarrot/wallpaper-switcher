//! Narrow source presentation data; no config IO, daemon, mutation, or worker.
use super::{keys, SettingsSource};
#[derive(Debug, Clone, Default)]
pub struct BackgroundControl {
    pub output: String,
    pub mode: String,
    pub color: String,
    pub inherited: bool,
}
impl BackgroundControl {
    pub fn new(cfg: &dyn SettingsSource, output: &str) -> Self {
        let mode = cfg.text(&format!("{}.{output}", keys::display::BACKGROUND_MODES));
        let inherited = !output.is_empty() && !matches!(mode.as_str(), "color" | "blur");
        let mode = if output.is_empty() || inherited {
            cfg.text(keys::display::BACKGROUND_MODE)
        } else {
            mode
        };
        let color = if output.is_empty() || inherited {
            String::new()
        } else {
            cfg.text(&format!("{}.{output}", keys::display::BACKGROUND_COLORS))
        };
        let color = if color.is_empty() {
            cfg.text(keys::display::FILL_COLOR)
        } else {
            color
        };
        let hex = color.trim_start_matches('#');
        let color =
            if matches!(hex.len(), 6 | 8) && hex.bytes().all(|value| value.is_ascii_hexdigit()) {
                format!("#{}", &hex[..6])
            } else {
                String::from("#000000")
            };
        Self {
            output: output.to_string(),
            mode,
            color,
            inherited,
        }
    }
}

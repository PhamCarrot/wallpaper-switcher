//! Narrow source presentation data; no config IO, daemon, mutation, or worker.
#[derive(Debug, Clone)]
pub enum DisplayControl {
    Colours {
        target: String,
        active: bool,
    },
    Audio {
        target: String,
        muted: bool,
        volume: u32,
        source_preview: bool,
    },
    Playback {
        target: String,
        paused: bool,
        manual: bool,
    },
}
impl DisplayControl {
    pub fn label(&self, catalog: &crate::candidate_app::localization::Catalog) -> &'static str {
        catalog.text_static(match self {
            Self::Colours { .. } => "effects-colours-label",
            Self::Audio { .. } => "effects-audio-label",
            Self::Playback { .. } => "audio-wallpaper-label",
        })
    }
}

impl DisplayControl {
    pub fn summary(&self, catalog: &crate::candidate_app::localization::Catalog) -> String {
        let tr = |key| catalog.text_static(key);
        match self {
            Self::Colours { active, .. } => tr(if *active {
                "effects-colours-source"
            } else {
                "effects-colours-use"
            })
            .into(),
            Self::Audio { muted, volume, .. } => format!(
                "{} {volume}%",
                tr(if *muted {
                    "effects-muted"
                } else {
                    "effects-sound"
                })
            ),
            Self::Playback { paused, manual, .. } => tr(if *manual {
                "audio-wallpaper-paused"
            } else if *paused {
                "audio-wallpaper-held"
            } else {
                "audio-wallpaper-playing"
            })
            .into(),
        }
    }
}

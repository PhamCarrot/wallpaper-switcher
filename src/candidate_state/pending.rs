//! Closed source-form schema. Saving pending intent never grants execution.
use super::frontend::DeferredCapability;
use serde_json::Value;
#[path = "pending/forms.rs"]
pub mod forms;
#[path = "pending/rules.rs"]
pub mod rules;
#[derive(Debug, Clone, Copy)]
pub enum Kind {
    Boolean,
    Number,
    Text,
}
#[derive(Debug, Clone, Copy)]
pub struct Spec {
    pub capability: DeferredCapability,
    pub path: &'static str,
    pub kind: Kind,
    pub bounds: Option<(f64, f64)>,
    pub choices: &'static [&'static str],
}
pub const SPECS: &[Spec] = &[
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "display.backgroundMode",
        kind: Kind::Text,
        bounds: None,
        choices: &["color", "blur"],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "display.fillColor",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "effects.autoRecolor",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "effects.autoTheme",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "features.matugen",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "theme.policy",
        kind: Kind::Text,
        bounds: None,
        choices: &["wallpaper", "fixed", "off"],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "theme.authority",
        kind: Kind::Text,
        bounds: None,
        choices: &["skwd", "caelestia", "noctalia", "dms", "end4"],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "theme.engine",
        kind: Kind::Text,
        bounds: None,
        choices: &[
            "skwd-iris",
            "matugen",
            "wallust",
            "pywal",
            "iris",
            "native",
            "skwd-pywal",
            "skwd-wallust",
        ],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "theme.mode",
        kind: Kind::Text,
        bounds: None,
        choices: &["dark", "light", "auto", "smart"],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "theme.scheme",
        kind: Kind::Text,
        bounds: None,
        choices: &[
            "tonal-spot",
            "vibrant",
            "expressive",
            "neutral",
            "monochrome",
            "fidelity",
            "content",
            "rainbow",
            "fruit-salad",
        ],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "theme.style",
        kind: Kind::Text,
        bounds: None,
        choices: &["natural", "pastel", "muted", "vibrant"],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "theme.customColors",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "theme.wallustPalette",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "matugen.schemeType",
        kind: Kind::Text,
        bounds: None,
        choices: &[
            "scheme-content",
            "scheme-expressive",
            "scheme-fidelity",
            "scheme-fruit-salad",
            "scheme-monochrome",
            "scheme-neutral",
            "scheme-rainbow",
            "scheme-tonal-spot",
            "scheme-vibrant",
            "scheme-smart",
        ],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "matugen.colorIndex",
        kind: Kind::Number,
        bounds: Some((0.0, 3.0)),
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "matugen.contrast",
        kind: Kind::Number,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "defaultMatugenConfig",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ThemeGeneration,
        path: "externalMatugenCommand",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ExpandedScheduling,
        path: "schedule.enabled",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ExpandedScheduling,
        path: "schedule.applyOnStart",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ExpandedScheduling,
        path: "schedule.latitude",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ExpandedScheduling,
        path: "general.weatherMatch",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ExpandedScheduling,
        path: "general.locale",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ExpandedScheduling,
        path: "pickOnlyMode",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::ExpandedScheduling,
        path: "postProcessOnRestore",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "playback.processEnabled",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "playback.processes",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "playback.fullscreen",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "playback.maximized",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "playback.fullscreenScope",
        kind: Kind::Text,
        bounds: None,
        choices: &["all", "display"],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "playback.resumeDelay",
        kind: Kind::Number,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "playback.muteOnOtherAudio",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "paper.idlePauseSeconds",
        kind: Kind::Number,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "selector.livePreview",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "videoPreview.enabled",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "videoPreview.delayMs",
        kind: Kind::Number,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "niri.overviewOnlyPlayback",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "weRender.fps",
        kind: Kind::Number,
        bounds: Some((1.0, 240.0)),
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "weRender.disableParticles",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::MediaPlaybackPolicy,
        path: "weRender.scaling",
        kind: Kind::Text,
        bounds: None,
        choices: &[
            "default", "fill", "fit", "stretch", "center", "tile", "span",
        ],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.engine",
        kind: Kind::Text,
        bounds: None,
        choices: &["skwd-paper", "awww"],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.loadTimeoutSeconds",
        kind: Kind::Number,
        bounds: Some((3.0, 60.0)),
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.videoEngine",
        kind: Kind::Text,
        bounds: None,
        choices: &["vulkan"],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.wallpaperLayer",
        kind: Kind::Text,
        bounds: None,
        choices: &["bottom", "background", "top", "overlay"],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.videoMultiProcess",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.performanceMode",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.awww.transitionType",
        kind: Kind::Text,
        bounds: None,
        choices: &[
            "none", "simple", "fade", "wipe", "wave", "grow", "center", "outer", "left", "right",
            "top", "bottom", "any", "random",
        ],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.awww.transitionDurationMs",
        kind: Kind::Number,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.awww.transitionFps",
        kind: Kind::Number,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.awww.transitionStep",
        kind: Kind::Number,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.awww.transitionAngle",
        kind: Kind::Number,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.awww.transitionWaveWidth",
        kind: Kind::Number,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.awww.transitionWaveHeight",
        kind: Kind::Number,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.awww.transitionPos",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.awww.invertY",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.awww.transitionBezier",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paper.awww.filter",
        kind: Kind::Text,
        bounds: None,
        choices: &["Nearest", "Bilinear", "CatmullRom", "Mitchell", "Lanczos3"],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "plasma.lockScreen.mode",
        kind: Kind::Text,
        bounds: None,
        choices: &["off", "static", "follow"],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "plasma.lockScreen.image",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "plasma.lockScreen.dynamic",
        kind: Kind::Text,
        bounds: None,
        choices: &["poster", "live"],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "noctalia.themeMode",
        kind: Kind::Text,
        bounds: None,
        choices: &["follow", "keep", "dark", "light", "auto"],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "noctalia.hoverPreview",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "paths.noctaliaBin",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "dms.hoverPreview",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "transition.sandPrimary",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "transition.sandQuality",
        kind: Kind::Text,
        bounds: None,
        choices: &["auto", "full", "low"],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "transition.sandSharp",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::Optimization,
        path: "performance.gpuDevice",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::Optimization,
        path: "performance.gpuPreference",
        kind: Kind::Text,
        bounds: None,
        choices: &["auto", "low", "high", "none"],
    },
    Spec {
        capability: DeferredCapability::Optimization,
        path: "performance.batterySaver",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::Optimization,
        path: "performance.batteryFps",
        kind: Kind::Number,
        bounds: Some((0.0, 360.0)),
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::Optimization,
        path: "performance.batteryVideoIdleSeconds",
        kind: Kind::Number,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::Optimization,
        path: "performance.batteryWallpaperPerformance",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::Optimization,
        path: "performance.autoOptimizeImages",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::Optimization,
        path: "performance.imageOptimizePreset",
        kind: Kind::Text,
        bounds: None,
        choices: &["light", "balanced", "quality"],
    },
    Spec {
        capability: DeferredCapability::Optimization,
        path: "performance.imageTrashDays",
        kind: Kind::Number,
        bounds: Some((0.0, 3650.0)),
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::Optimization,
        path: "performance.autoDeleteImageTrash",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::Optimization,
        path: "performance.maxThumbJobs",
        kind: Kind::Number,
        bounds: Some((1.0, 32.0)),
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "features.wallhaven",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.wallhaven.showApplyButton",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.steam.showApplyButton",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.unsplash.showApplyButton",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.pexels.showApplyButton",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.youtube.showApplyButton",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.unsplash.enabled",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.unsplash.accessKey",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.pexels.enabled",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.pexels.apiKey",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.bing.enabled",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.bing.market",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.youtube.enabled",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.youtube.maxHeight",
        kind: Kind::Number,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "sources.youtube.maxMinutes",
        kind: Kind::Number,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "wallhaven.apiKey",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "wallhaven.username",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "wallhaven.defaults.sort",
        kind: Kind::Text,
        bounds: None,
        choices: &[
            "toplist",
            "hot",
            "date_added",
            "relevance",
            "views",
            "favorites",
            "random",
        ],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "wallhaven.defaults.atleast",
        kind: Kind::Text,
        bounds: None,
        choices: &[
            "",
            "1920x1080",
            "2560x1440",
            "3840x2160",
            "5120x2880",
            "7680x4320",
        ],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "wallhaven.defaults.atmost",
        kind: Kind::Text,
        bounds: None,
        choices: &[
            "",
            "1920x1080",
            "2560x1440",
            "3840x2160",
            "5120x2880",
            "7680x4320",
        ],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "wallhaven.defaults.ratios",
        kind: Kind::Text,
        bounds: None,
        choices: &["", "16x9", "16x10", "21x9", "32x9", "4x3"],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "wallhaven.defaults.general",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "wallhaven.defaults.anime",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "wallhaven.defaults.people",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "wallhaven.defaults.sfw",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "wallhaven.defaults.sketchy",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "wallhaven.defaults.nsfw",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "steam.backend",
        kind: Kind::Text,
        bounds: None,
        choices: &["steam", "steamcmd"],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "steam.username",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "steam.apiKey",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "steam.defaults.type",
        kind: Kind::Text,
        bounds: None,
        choices: &["", "Scene", "Video"],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "steam.defaults.resolution",
        kind: Kind::Text,
        bounds: None,
        choices: &[
            "",
            "1920 x 1080",
            "2560 x 1440",
            "3840 x 2160",
            "2560 x 1080",
            "3440 x 1440",
            "3840 x 1080",
        ],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "steam.defaults.category",
        kind: Kind::Text,
        bounds: None,
        choices: &[
            "",
            "Abstract",
            "Animal",
            "Anime",
            "CGI",
            "Cyberpunk",
            "Fantasy",
            "Game",
            "Girls",
            "Guys",
            "Landscape",
            "Medieval",
            "Music",
            "Nature",
            "Pixel art",
            "Relaxing",
            "Retro",
            "Sci-Fi",
            "Technology",
            "Vehicle",
        ],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "steam.defaults.nsfw",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "paths.steam",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "paths.steamWorkshop",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "paths.steamWeAssets",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SemanticSearch,
        path: "semantic.enabled",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SemanticSearch,
        path: "semantic.indexProfile",
        kind: Kind::Text,
        bounds: None,
        choices: &["full", "multiview"],
    },
    Spec {
        capability: DeferredCapability::SemanticSearch,
        path: "semantic.manifest",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::SemanticSearch,
        path: "tagging.defaultSearchMode",
        kind: Kind::Text,
        bounds: None,
        choices: &["describe", "tags"],
    },
    Spec {
        capability: DeferredCapability::ExpandedScheduling,
        path: "schedule.longitude",
        kind: Kind::Text,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::Optimization,
        path: "performance.imageOptimizeResolution",
        kind: Kind::Text,
        bounds: None,
        choices: &["1080p", "2k", "4k"],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "wallhaven.defaults.topRange",
        kind: Kind::Text,
        bounds: None,
        choices: &["1d", "3d", "1w", "1M", "3M", "6M", "1y"],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "steam.defaults.sort",
        kind: Kind::Text,
        bounds: None,
        choices: &["3", "0", "1", "21", "9"],
    },
    Spec {
        capability: DeferredCapability::SourceBrowsing,
        path: "steam.defaults.trendDays",
        kind: Kind::Text,
        bounds: None,
        choices: &["1", "3", "7"],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "library.pollingFallback",
        kind: Kind::Boolean,
        bounds: None,
        choices: &[],
    },
    Spec {
        capability: DeferredCapability::PlatformIntegration,
        path: "library.pollingIntervalSeconds",
        kind: Kind::Number,
        bounds: Some((15.0, 3600.0)),
        choices: &[],
    },
];
pub fn spec(path: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|spec| spec.path == path)
}
pub fn normalize(
    capability: DeferredCapability,
    path: &str,
    value: Value,
) -> Result<Value, String> {
    let spec = spec(path).ok_or("This source field has no pending adapter.")?;
    if spec.capability != capability {
        return Err("Pending capability does not own this source field.".into());
    }
    let result = match spec.kind {
        Kind::Boolean => Value::Bool(value.as_bool().ok_or("Enter a boolean value.")?),
        Kind::Text => match value {
            Value::String(text) => Value::String(text),
            Value::Number(number) => Value::String(number.to_string()),
            _ => return Err("Enter a text value.".into()),
        },
        Kind::Number => {
            let number = value
                .as_f64()
                .or_else(|| value.as_str().and_then(|text| text.trim().parse().ok()))
                .filter(|number| number.is_finite())
                .ok_or("Enter a finite number.")?;
            let normalized = spec
                .bounds
                .map_or(number, |(min, max)| number.clamp(min, max));
            if path == "matugen.colorIndex" && normalized.fract() != 0.0 {
                return Err("Choose a color index from 0 to 3.".into());
            }
            if normalized.fract() == 0.0
                && normalized >= i64::MIN as f64
                && normalized <= i64::MAX as f64
            {
                Value::from(normalized as i64)
            } else {
                Value::from(normalized)
            }
        }
    };
    if !spec.choices.is_empty()
        && !result
            .as_str()
            .is_some_and(|value| spec.choices.contains(&value))
    {
        return Err("Choose an available source value.".into());
    }
    if path == "display.fillColor" && !valid_background_color(&result) {
        return Err("Enter a #RRGGBB colour.".into());
    }
    Ok(result)
}

#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Command {
    #[serde(rename = "type")]
    pub kind: String,
    pub command: String,
    #[serde(flatten)]
    pub retained: std::collections::BTreeMap<String, Value>,
}
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Integration {
    pub name: String,
    pub template: String,
    pub output: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reload: Option<String>,
    #[serde(
        default,
        rename = "livePreview",
        skip_serializing_if = "Option::is_none"
    )]
    pub live_preview: Option<bool>,
    #[serde(flatten)]
    pub retained: std::collections::BTreeMap<String, Value>,
}
#[derive(Debug, Default, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SemanticModel {
    pub name: String,
    pub manifest: String,
    #[serde(flatten)]
    pub retained: std::collections::BTreeMap<String, Value>,
}

pub fn root_owner(path: &str) -> Option<DeferredCapability> {
    Some(match path {
        "postProcessing" => DeferredCapability::ExpandedScheduling,
        "integrations" | "transition.shaderScopes" => DeferredCapability::PlatformIntegration,
        "semantic.models" => DeferredCapability::SemanticSearch,
        "theme.targets" => DeferredCapability::ThemeGeneration,
        "effects.stack" => DeferredCapability::ExtraImageEffects,
        "schedule.rules" => DeferredCapability::ExpandedScheduling,
        "theme.wallpaperProfiles" => DeferredCapability::WallpaperBoundProfiles,
        "browser.forms" => DeferredCapability::SourceBrowsing,
        "display.backgroundModes" | "display.backgroundColors" => {
            DeferredCapability::PlatformIntegration
        }
        _ => return None,
    })
}

pub fn normalize_root(
    capability: DeferredCapability,
    path: &str,
    value: Value,
) -> Result<Value, String> {
    if root_owner(path) != Some(capability) {
        return Err("This pending root has no matching owner.".into());
    }
    match path {
        "display.backgroundModes" | "display.backgroundColors" => {
            let entries = value
                .as_object()
                .ok_or("Invalid per-display background map; original data retained.")?;
            if entries.iter().any(|(key, value)| {
                key.is_empty()
                    || key.len() > 1024
                    || if path == "display.backgroundModes" {
                        !matches!(value.as_str(), Some("color" | "blur" | "inherit"))
                    } else {
                        !valid_background_color(value)
                    }
            }) {
                return Err("Invalid per-display background value; original data retained.".into());
            }
        }
        "schedule.rules" => {
            rules::validate_records(&value)?;
        }
        "theme.wallpaperProfiles" => {
            forms::validate_profiles(&value)?;
        }
        "browser.forms" => {
            forms::validate_browsers(&value)?;
        }
        "effects.stack" => {
            super::image_effects::validate_stack(value.clone())?;
        }
        "postProcessing" => {
            let entries: Vec<Command> = serde_json::from_value(value.clone())
                .map_err(|_| "Invalid post-processing records.")?;
            if entries
                .iter()
                .any(|entry| !["all", "static", "video", "we"].contains(&entry.kind.as_str()))
            {
                return Err("Unknown command media kind.".into());
            }
        }
        "integrations" => {
            let entries = value.as_array().ok_or("Invalid integration records.")?;
            for entry in entries {
                let object = entry.as_object().ok_or("Invalid integration record.")?;
                // serde Option alone conflates absent and explicit null. An
                // incompatible stored member must block editing, retaining the
                // original whole root rather than erasing it on the next save.
                for field in ["enabled", "livePreview"] {
                    if object.get(field).is_some_and(|value| !value.is_boolean()) {
                        return Err(
                            "Invalid optional integration boolean; stored data was retained."
                                .into(),
                        );
                    }
                }
                if object.get("reload").is_some_and(|value| !value.is_string()) {
                    return Err(
                        "Invalid optional integration text; stored data was retained.".into(),
                    );
                }
            }
            let _: Vec<Integration> = serde_json::from_value(value.clone())
                .map_err(|_| "Invalid integration records.")?;
        }
        "semantic.models" => {
            let _: Vec<SemanticModel> = serde_json::from_value(value.clone())
                .map_err(|_| "Invalid semantic model records.")?;
        }
        "theme.targets" => {
            let entries = value.as_array().ok_or("Theme targets must be an array.")?;
            if entries.iter().any(|entry| !entry.is_string()) {
                return Err("Theme target identities must be strings.".into());
            }
        }
        "transition.shaderScopes" => {
            let entries = value
                .as_object()
                .ok_or("Shader scopes must be an object.")?;
            // Loaded unknown keys stay inert. Public source-transition authoring
            // is retired; this validator remains for compatible round-trip.
            if entries.values().any(|entry| {
                !entry
                    .as_str()
                    .is_some_and(|scope| ["all", "primary"].contains(&scope))
            }) {
                return Err("Shader scope must be all or primary.".into());
            }
        }
        _ => unreachable!(),
    }
    Ok(value)
}
fn valid_background_color(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 7 && s.starts_with('#') && s[1..].bytes().all(|c| c.is_ascii_hexdigit())
    })
}

pub fn accepted(capability: DeferredCapability, path: &str, value: Value) -> Result<Value, String> {
    if root_owner(path).is_some() {
        normalize_root(capability, path, value)
    } else {
        normalize(capability, path, value)
    }
}

/// Invalid/unknown stored records are preserved, but never become live inputs.
pub fn diagnostic(capability: DeferredCapability, path: &str, value: &str) -> Option<String> {
    let parsed: Value = match serde_json::from_str(value) {
        Ok(value) => value,
        Err(_) => {
            return Some(format!(
                "Pending field {path} retained inert: invalid JSON."
            ))
        }
    };
    match accepted(capability, path, parsed.clone()) {
        Err(error) => Some(format!("Pending field {path} retained inert: {error}")),
        Ok(normalized) if normalized != parsed => Some(format!("Pending field {path} retained inert: the saved representation requires explicit correction.")),
        Ok(_) => {
            let known: &[&str] = match path {
                "postProcessing" => &["type", "command"],
                "integrations" => &["name", "template", "output", "enabled", "reload", "livePreview"],
                "semantic.models" => &["name", "manifest"],
                _ => return None,
            };
            parsed.as_array().is_some_and(|entries| entries.iter().any(|entry| entry.as_object().is_some_and(|object| object.keys().any(|key| !known.contains(&key.as_str())))))
                .then(|| format!("Pending field {path} retained inert: unrecognized record members were preserved."))
        }
    }
}

//! Inert source browser and wallpaper-profile forms; never execution settings.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
pub const SOURCES: [&str; 6] = [
    "wallhaven",
    "steam",
    "unsplash",
    "pexels",
    "youtube",
    "bing",
];
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserForm {
    pub source: String,
    pub query: String,
    pub sort: String,
    pub range: String,
    pub orientation: String,
    pub color: String,
    pub safety: String,
    pub size: String,
    pub duration: i64,
    pub exact: bool,
    pub minimum: String,
    pub maximum: String,
    pub resolutions: Vec<String>,
    pub ratios: Vec<String>,
    pub categories: Vec<String>,
    pub purity: Vec<String>,
    #[serde(default)]
    pub steam_kind: String,
    #[serde(default)]
    pub steam_resolution: String,
    #[serde(default)]
    pub steam_category: String,
    #[serde(flatten)]
    pub retained: BTreeMap<String, Value>,
}
impl BrowserForm {
    pub fn new(source: &str) -> Self {
        Self {
            source: source.into(),
            query: String::new(),
            sort: match source {
                "wallhaven" => "toplist",
                "steam" => "3",
                "unsplash" => "relevant",
                _ => "",
            }
            .into(),
            range: match source {
                "steam" => "7",
                "wallhaven" => "1M",
                _ => "",
            }
            .into(),
            orientation: String::new(),
            color: if source == "wallhaven" { "-1" } else { "" }.into(),
            safety: if source == "unsplash" { "high" } else { "" }.into(),
            size: String::new(),
            duration: 0,
            exact: false,
            minimum: String::new(),
            maximum: String::new(),
            resolutions: Vec::new(),
            ratios: Vec::new(),
            categories: if source == "wallhaven" {
                vec!["general".into(), "anime".into(), "people".into()]
            } else {
                Vec::new()
            },
            purity: if source == "wallhaven" {
                vec!["sfw".into()]
            } else {
                Vec::new()
            },
            steam_kind: String::new(),
            steam_resolution: String::new(),
            steam_category: String::new(),
            retained: BTreeMap::new(),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if !SOURCES.contains(&self.source.as_str()) {
            return Err("Unknown browser source; original data retained.".into());
        }
        if self.query.len() > 4096 {
            return Err("Query is too long.".into());
        }
        if (["unsplash", "pexels"].contains(&self.source.as_str())
            && !["", "landscape", "portrait", "square"].contains(&self.orientation.as_str()))
            || (self.source == "pexels"
                && !["", "small", "medium", "large"].contains(&self.size.as_str()))
            || ![0, 300, 600, 1800, 3600].contains(&self.duration)
        {
            return Err("Choose an available source filter.".into());
        }
        let sort = match self.source.as_str() {
            "wallhaven" => [
                "toplist",
                "hot",
                "date_added",
                "relevance",
                "views",
                "favorites",
                "random",
            ]
            .as_slice(),
            "unsplash" => ["relevant", "latest"].as_slice(),
            "steam" => ["3", "0", "1", "21", "9"].as_slice(),
            _ => [].as_slice(),
        };
        if !sort.is_empty() && !sort.contains(&self.sort.as_str()) {
            return Err("Choose an available source order.".into());
        }
        if self.source == "unsplash" && !["low", "high"].contains(&self.safety.as_str()) {
            return Err("Choose low or high content safety.".into());
        }
        if self.source == "steam"
            && (!["1", "3", "7"].contains(&self.range.as_str())
                || !["", "Scene", "Video"].contains(&self.steam_kind.as_str())
                || !STEAM_RESOLUTIONS.contains(&self.steam_resolution.as_str())
                || !STEAM_CATEGORIES.contains(&self.steam_category.as_str()))
        {
            return Err("Choose an available Workshop filter.".into());
        }
        if self.source == "wallhaven"
            && (!["1d", "3d", "1w", "1M", "3M", "6M", "1y"].contains(&self.range.as_str())
                || !self
                    .ratios
                    .iter()
                    .all(|v| ["16x9", "16x10", "21x9", "32x9", "4x3"].contains(&v.as_str())))
        {
            return Err("Choose an available Wallhaven range or ratio.".into());
        }
        let colors = match self.source.as_str() {
            "unsplash" => [
                "",
                "black_and_white",
                "black",
                "white",
                "yellow",
                "orange",
                "red",
                "purple",
                "magenta",
                "green",
                "teal",
                "blue",
            ]
            .as_slice(),
            "pexels" => [
                "",
                "red",
                "orange",
                "yellow",
                "green",
                "turquoise",
                "blue",
                "violet",
                "pink",
                "brown",
                "black",
                "gray",
                "white",
                "#808080",
            ]
            .as_slice(),
            _ => [].as_slice(),
        };
        if !colors.is_empty() && !colors.contains(&self.color.as_str()) {
            return Err("Choose an available source colour.".into());
        }
        for resolution in self
            .resolutions
            .iter()
            .chain([&self.minimum, &self.maximum])
        {
            if !resolution.is_empty()
                && !resolution.split_once('x').is_some_and(|(w, h)| {
                    w.parse::<u32>().is_ok_and(|n| n > 0) && h.parse::<u32>().is_ok_and(|n| n > 0)
                })
            {
                return Err("Enter a resolution such as1920x1080.".into());
            }
        }
        if self.source == "wallhaven"
            && (!self
                .categories
                .iter()
                .all(|v| ["general", "anime", "people"].contains(&v.as_str()))
                || self.categories.is_empty()
                || !self
                    .purity
                    .iter()
                    .all(|v| ["sfw", "sketchy", "nsfw"].contains(&v.as_str()))
                || self.purity.is_empty())
        {
            return Err("Select at least one valid category and purity.".into());
        }
        if self.source == "wallhaven"
            && !self
                .color
                .parse::<i64>()
                .is_ok_and(|n| (-1..=12).contains(&n))
        {
            return Err("Choose an available colour bucket.".into());
        }
        Ok(())
    }
}
pub const STEAM_RESOLUTIONS: [&str; 7] = [
    "",
    "1920 x 1080",
    "2560 x 1440",
    "3840 x 2160",
    "2560 x 1080",
    "3440 x 1440",
    "3840 x 1080",
];
pub const STEAM_CATEGORIES: [&str; 20] = [
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
];
pub fn validate_browsers(value: &Value) -> Result<(), String> {
    let records: Vec<BrowserForm> = serde_json::from_value(value.clone())
        .map_err(|_| "Saved browser forms are incompatible; original data retained.")?;
    for record in records {
        record.validate()?;
    }
    Ok(())
}
pub fn validate_profiles(value: &Value) -> Result<(), String> {
    for profile in value
        .as_array()
        .ok_or("Wallpaper profiles must be an array.")?
    {
        let object = profile.as_object().ok_or("Invalid wallpaper profile.")?;
        if !object
            .get("key")
            .and_then(Value::as_str)
            .is_some_and(|s| !s.is_empty())
        {
            return Err("Wallpaper profile requires its source identity.".into());
        }
        for key in ["enabled", "settingsPinned"] {
            if object.get(key).is_some_and(|v| !v.is_boolean()) {
                return Err("Invalid profile switch; original data retained.".into());
            }
        }
        for key in ["dark", "light"] {
            if let Some(document) = object.get(key) {
                let scheme = document.get("_scheme").unwrap_or(document);
                let _: crate::theme_roles::ThemeDocument =
                    serde_json::from_value(scheme.clone())
                        .map_err(|_| "Profile document is incompatible; original data retained.")?;
            }
        }
    }
    Ok(())
}

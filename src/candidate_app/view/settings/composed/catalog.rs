//! Source-derived pure control projection; explicit locale and -owned consumers.
pub const SORT_KEYS: [&str; 7] = [
    "toplist",
    "hot",
    "date_added",
    "relevance",
    "views",
    "favorites",
    "random",
];

pub const STEAM_SORTS: [&str; 5] = ["3", "0", "1", "21", "9"];

pub const STEAM_TREND_DAYS: [&str; 3] = ["1", "3", "7"];

pub const STEAM_TYPES: [&str; 3] = ["", "Scene", "Video"];

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

pub const TOP_RANGES: [(&str, &str); 7] = [
    ("1d", "Day"),
    ("3d", "3 Days"),
    ("1w", "Week"),
    ("1M", "Month"),
    ("3M", "3 Months"),
    ("6M", "6 Months"),
    ("1y", "Year"),
];

pub const RESOLUTIONS: [(&str, &str); 6] = [
    ("", "Any"),
    ("1920x1080", "1080p"),
    ("2560x1440", "2K"),
    ("3840x2160", "4K"),
    ("5120x2880", "5K"),
    ("7680x4320", "8K"),
];

pub const DURATIONS: [(&str, &str); 5] = [
    ("", "Any"),
    ("300", "\u{2264}5m"),
    ("600", "\u{2264}10m"),
    ("1800", "\u{2264}30m"),
    ("3600", "\u{2264}1h"),
];

pub const PHOTO_ORIENTATIONS: [(&str, &str); 4] = [
    ("", "Any"),
    ("landscape", "Landscape"),
    ("portrait", "Portrait"),
    ("square", "Square"),
];

pub const UNSPLASH_ORDERS: [(&str, &str); 2] = [("relevant", "Relevant"), ("latest", "Latest")];

pub const UNSPLASH_SAFETY: [(&str, &str); 2] = [("low", "Standard"), ("high", "Strict")];

pub const UNSPLASH_COLORS: [(&str, &str); 12] = [
    ("", "Any colour"),
    ("black_and_white", "B&W"),
    ("black", "Black"),
    ("white", "White"),
    ("yellow", "Yellow"),
    ("orange", "Orange"),
    ("red", "Red"),
    ("purple", "Purple"),
    ("magenta", "Magenta"),
    ("green", "Green"),
    ("teal", "Teal"),
    ("blue", "Blue"),
];

pub const PEXELS_SIZES: [(&str, &str); 4] = [
    ("", "Any size"),
    ("small", "4 MP+"),
    ("medium", "12 MP+"),
    ("large", "24 MP+"),
];

pub const PEXELS_COLORS: [(&str, &str); 14] = [
    ("", "Any colour"),
    ("red", "Red"),
    ("orange", "Orange"),
    ("yellow", "Yellow"),
    ("green", "Green"),
    ("turquoise", "Turquoise"),
    ("blue", "Blue"),
    ("violet", "Violet"),
    ("pink", "Pink"),
    ("brown", "Brown"),
    ("black", "Black"),
    ("gray", "Gray"),
    ("white", "White"),
    ("#808080", "Neutral"),
];

pub const RATIOS: [(&str, &str); 6] = [
    ("", "Any"),
    ("16x9", "16:9"),
    ("16x10", "16:10"),
    ("21x9", "21:9"),
    ("32x9", "32:9"),
    ("4x3", "4:3"),
];

pub const TYPES: [&str; 4] = ["", "static", "video", "we"];

pub const SORTS: [(&str, &str); 7] = [
    ("date", "\u{f00f0}"),
    ("recent", "\u{f02da}"),
    ("color", "\u{f03d8}"),
    ("pop", "\u{f0238}"),
    ("richness", "\u{f0b74}"),
    ("minimalist", "\u{f0764}"),
    ("res", "\u{f0a24}"),
];

pub const ORIENTS: [&str; 3] = ["", "landscape", "portrait"];

pub fn type_label_key(key: &str) -> &'static str {
    match key {
        "static" => "filter-bar-type-pic",
        "video" => "filter-bar-type-vid",
        "we" => "filter-bar-type-we",
        _ => "filter-bar-type-all",
    }
}

pub fn sort_label_key(mode: &str) -> &'static str {
    match mode {
        "date" => "filter-bar-sort-date",
        "recent" => "filter-bar-sort-recent",
        "color" => "filter-bar-sort-color",
        "pop" => "filter-bar-sort-pop",
        "richness" => "filter-bar-sort-richness",
        "minimalist" => "filter-bar-sort-minimalist",
        _ => "filter-bar-sort-res",
    }
}

pub(crate) fn steam_sort_label_key(key: &str) -> &'static str {
    match key {
        "0" => "browser-top",
        "1" => "browser-new",
        "21" => "browser-updated",
        "9" => "browser-subscribers",
        _ => "browser-trend",
    }
}
pub(crate) fn steam_trend_days_label_key(key: &str) -> &'static str {
    match key {
        "1" => "browser-1-day",
        "3" => "browser-3-days",
        _ => "browser-7-days",
    }
}
pub(crate) fn steam_type_label_key(key: &str) -> &'static str {
    match key {
        "Scene" => "browser-scene",
        "Video" => "browser-video",
        _ => "browser-scene-video",
    }
}
pub(crate) fn steam_resolution_label(
    catalog: &crate::candidate_app::localization::Catalog,
    key: &str,
) -> &'static str {
    let tr = |key| catalog.text_static(key);

    match key {
        "1920 x 1080" => "1080p",
        "2560 x 1440" => "2K",
        "3840 x 2160" => "4K",
        "2560 x 1080" => "UW",
        "3440 x 1440" => "UWQHD",
        "3840 x 1080" => tr("browser-dual"),
        _ => tr("browser-any"),
    }
}
pub(crate) fn steam_category_label_key(key: &str) -> &'static str {
    match key {
        "Abstract" => "browser-abstract",
        "Animal" => "browser-animal",
        "Anime" => "browser-anime",
        "CGI" => "browser-cgi",
        "Cyberpunk" => "browser-cyberpunk",
        "Fantasy" => "browser-fantasy",
        "Game" => "browser-game",
        "Girls" => "browser-girls",
        "Guys" => "browser-guys",
        "Landscape" => "browser-landscape",
        "Medieval" => "browser-medieval",
        "Music" => "browser-music",
        "Nature" => "browser-nature",
        "Pixel art" => "browser-pixel-art",
        "Relaxing" => "browser-relaxing",
        "Retro" => "browser-retro",
        "Sci-Fi" => "browser-sci-fi",
        "Technology" => "browser-technology",
        "Vehicle" => "browser-vehicle",
        _ => "browser-all",
    }
}
pub(crate) fn wallhaven_sort_label_key(key: &str) -> &'static str {
    match key {
        "toplist" => "browser-top",
        "hot" => "browser-hot",
        "date_added" => "browser-new",
        "relevance" => "browser-relevant",
        "views" => "browser-views",
        "favorites" => "browser-favourites",
        _ => "browser-random",
    }
}
pub(crate) fn wallhaven_range_label_key(key: &str) -> &'static str {
    match key {
        "1d" => "browser-day",
        "3d" => "browser-3-days",
        "1w" => "browser-week",
        "1M" => "browser-month",
        "3M" => "browser-3-months",
        "6M" => "browser-6-months",
        _ => "browser-year",
    }
}
pub fn type_key(kind: &str) -> &'static str {
    match kind {
        "static" => "static",
        "video" => "video",
        "we" => "we",
        _ => "all",
    }
}
pub fn parse_resolution(value: &str) -> Option<(i64, i64)> {
    let (width, height) = value.split_once(['x', 'X', '×'])?;
    let width = width.trim().parse::<i64>().ok()?;
    let height = height.trim().parse::<i64>().ok()?;
    (width > 0 && height > 0).then_some((width, height))
}

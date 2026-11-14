use crate::candidate_state::{OrientationFilter, Playlist, PlaylistMembership, SmartRule};
use crate::scanner::WallpaperItem;
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

#[path = "library/color_metadata.rs"]
pub mod color_metadata;
#[path = "library/media_still.rs"]
pub mod media_still;
#[path = "library/sort_metrics.rs"]
pub mod sort_metrics;

const DIMENSION_CACHE_CAPACITY: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DimensionCacheKey {
    path: PathBuf,
    size: u64,
    modified_nanos: Option<u128>,
}

#[derive(Default)]
struct DimensionCache {
    entries: HashMap<DimensionCacheKey, (u32, u32)>,
    insertion_order: VecDeque<DimensionCacheKey>,
}

impl DimensionCache {
    fn get(&self, key: &DimensionCacheKey) -> Option<(u32, u32)> {
        self.entries.get(key).copied()
    }

    fn insert(&mut self, key: DimensionCacheKey, dimensions: (u32, u32)) {
        if self.entries.contains_key(&key) {
            return;
        }
        while self.entries.len() >= DIMENSION_CACHE_CAPACITY {
            let Some(oldest) = self.insertion_order.pop_front() else {
                break;
            };
            self.entries.remove(&oldest);
        }
        self.insertion_order.push_back(key.clone());
        self.entries.insert(key, dimensions);
    }
}

fn dimension_cache_key(path: &Path) -> Option<DimensionCacheKey> {
    let metadata = path.metadata().ok()?;
    let modified_nanos = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos());
    Some(DimensionCacheKey {
        path: path.to_path_buf(),
        size: metadata.len(),
        modified_nanos,
    })
}

fn cached_image_dimensions(path: &Path) -> (u32, u32) {
    static CACHE: OnceLock<Mutex<DimensionCache>> = OnceLock::new();

    let Some(key) = dimension_cache_key(path) else {
        return image::image_dimensions(path).unwrap_or((0, 0));
    };
    let cache = CACHE.get_or_init(|| Mutex::new(DimensionCache::default()));
    if let Some(dimensions) = cache.lock().ok().and_then(|cache| cache.get(&key)) {
        return dimensions;
    }

    // CatalogEntry construction is performed by the candidate's catalog scanner threads.
    let dimensions = image::image_dimensions(path).unwrap_or((0, 0));
    if let Ok(mut cache) = cache.lock() {
        cache.insert(key, dimensions);
    }
    dimensions
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub folder: PathBuf,
    pub width: u32,
    pub height: u32,
    pub modified_unix: u64,
    /// Immutable source tags; local projections always rebuild from these.
    pub source_tags: BTreeSet<String>,
    /// Effective tags consumed by queries and playlists.
    pub tags: BTreeSet<String>,
}

impl CatalogEntry {
    pub fn from_wallpaper(item: &WallpaperItem) -> Self {
        let source_tags = read_project_tags(&item.project_json_path);
        let dimensions = item
            .preview_path
            .as_deref()
            .or(Some(item.file_target.as_path()))
            .map(cached_image_dimensions)
            .unwrap_or((0, 0));
        Self {
            id: item.id.clone(),
            title: item.title.clone(),
            kind: item.wallpaper_type.clone(),
            folder: item.folder_path.clone(),
            width: dimensions.0,
            height: dimensions.1,
            modified_unix: source_modified_unix(item),
            tags: source_tags.clone(),
            source_tags,
        }
    }

    pub fn orientation(&self) -> OrientationFilter {
        match self.width.cmp(&self.height) {
            std::cmp::Ordering::Greater => OrientationFilter::Landscape,
            std::cmp::Ordering::Less => OrientationFilter::Portrait,
            std::cmp::Ordering::Equal if self.width > 0 => OrientationFilter::Square,
            std::cmp::Ordering::Equal => OrientationFilter::Any,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SortOrder {
    Newest,
    Recent,
    #[default]
    Colour,
    ColourPop,
    Richness,
    Minimalist,
    ResolutionDescending,
}
impl SortOrder {
    pub const ALL: [Self; 7] = [
        Self::Newest,
        Self::Recent,
        Self::Colour,
        Self::ColourPop,
        Self::Richness,
        Self::Minimalist,
        Self::ResolutionDescending,
    ];
    pub fn visible(self, v: &crate::candidate_state::preferences::VisibilityPreferences) -> bool {
        match self {
            Self::Newest => v.sort_date,
            Self::Recent => v.sort_recent,
            Self::Colour => v.sort_color,
            Self::ColourPop => v.sort_pop,
            Self::Richness => v.sort_richness,
            Self::Minimalist => v.sort_minimalist,
            Self::ResolutionDescending => v.sort_res,
        }
        .unwrap_or(true)
    }
}
fn source_modified_unix(item: &WallpaperItem) -> u64 {
    let path = if item.source == crate::scanner::WallpaperSource::WallpaperEngine {
        &item.project_json_path
    } else {
        &item.file_target
    };
    path.metadata()
        .and_then(|m| m.modified())
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_secs())
}

fn read_project_tags(path: &Path) -> BTreeSet<String> {
    use std::io::Read;
    if path
        .extension()
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("json"))
    {
        return BTreeSet::new();
    }
    let Ok(file) = std::fs::File::open(path) else {
        return BTreeSet::new();
    };
    let mut bytes = Vec::new();
    // Bound metadata reads; poster ownership and identities remain unchanged.
    if file.take(1_048_577).read_to_end(&mut bytes).is_err() || bytes.len() > 1_048_576 {
        return BTreeSet::new();
    }
    project_tags(&bytes)
}
fn project_tags(bytes: &[u8]) -> BTreeSet<String> {
    let Ok(value) = serde_json::from_slice::<serde_json::Value>(bytes) else {
        return BTreeSet::new();
    };
    value
        .get("tags")
        .and_then(|tags| tags.as_array())
        .into_iter()
        .flatten()
        .filter_map(|tag| tag.as_str())
        .map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(str::to_lowercase)
        .collect()
}

// Folder query spelling only; this does not establish filesystem/profile ownership.
fn folder_query_path(path: &Path) -> PathBuf {
    let mut spelling = path.to_string_lossy().replace('/', "\\");
    spelling.make_ascii_lowercase();
    if let Some(unc) = spelling.strip_prefix(r"\\?\unc\") {
        let mut components = unc.split('\\').filter(|part| !part.is_empty());
        if components.next().is_some() && components.next().is_some() {
            return PathBuf::from(format!(r"\\{unc}"));
        }
    }
    if let Some(disk) = spelling.strip_prefix(r"\\?\") {
        let bytes = disk.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
        {
            return PathBuf::from(disk);
        }
    }
    PathBuf::from(spelling)
}

fn folder_query_contains(entry: &Path, folder: &Path) -> bool {
    entry.starts_with(folder) || folder_query_path(entry).starts_with(folder_query_path(folder))
}

pub fn matches_rule(entry: &CatalogEntry, rule: &SmartRule, favorites: &BTreeSet<String>) -> bool {
    let query = rule.text.trim().to_lowercase();
    if !query.is_empty()
        && !query.split_whitespace().all(|word| {
            entry.title.to_lowercase().contains(word)
                || entry.tags.iter().any(|tag| tag.contains(word))
                || entry.id.to_lowercase().contains(word)
                || entry.folder.to_string_lossy().to_lowercase().contains(word)
        })
    {
        return false;
    }
    let has_tag = |tag: &String| entry.tags.contains(&tag.trim().to_lowercase());
    if rule.excluded_tags.iter().any(has_tag) {
        return false;
    }
    if !rule.tags.is_empty()
        && !(if rule.tags_match_any {
            rule.tags.iter().any(has_tag)
        } else {
            rule.tags.iter().all(has_tag)
        })
    {
        return false;
    }
    if !rule.kinds.is_empty()
        && !rule
            .kinds
            .iter()
            .any(|kind| kind.eq_ignore_ascii_case(&entry.kind))
    {
        return false;
    }
    if !rule.folders.is_empty()
        && !rule
            .folders
            .iter()
            .any(|folder| folder_query_contains(&entry.folder, folder))
    {
        return false;
    }
    if entry.width < rule.minimum_width || entry.height < rule.minimum_height {
        return false;
    }
    if rule
        .resolution_preset
        .as_ref()
        .is_some_and(|preset| !preset.matches_dimensions(entry.width, entry.height))
    {
        return false;
    }
    if rule.orientation != OrientationFilter::Any && entry.orientation() != rule.orientation {
        return false;
    }
    !rule.favorites_only || favorites.contains(&entry.id)
}

pub fn query_catalog<'a>(
    catalog: &'a [CatalogEntry],
    rule: &SmartRule,
    favorites: &BTreeSet<String>,
    order: SortOrder,
    colour: &color_metadata::ColorQuery,
    metadata: &color_metadata::MetadataView,
    sorting: &sort_metrics::MetadataView,
    usage: &crate::candidate_state::local_library::UsageHistory,
) -> Vec<&'a CatalogEntry> {
    let mut results: Vec<_> = catalog
        .iter()
        .filter(|entry| {
            matches_rule(entry, rule, favorites) && colour.matches(metadata.get(&entry.id))
        })
        .collect();
    results.sort_by(|left, right| {
        (match order {
            SortOrder::Newest => right.modified_unix.cmp(&left.modified_unix),
            SortOrder::Recent => {
                let rank = |id: &str| {
                    usage
                        .items
                        .get(id)
                        .filter(|u| u.confirmed_count > 0)
                        .map_or(0, |u| u.last_confirmed_unix)
                };
                rank(&right.id)
                    .cmp(&rank(&left.id))
                    .then(right.modified_unix.cmp(&left.modified_unix))
            }
            SortOrder::Colour
            | SortOrder::ColourPop
            | SortOrder::Richness
            | SortOrder::Minimalist => {
                sort_metrics::compare(sorting.get(&left.id), sorting.get(&right.id), order)
            }
            SortOrder::ResolutionDescending => (right.width as u64 * right.height as u64)
                .cmp(&(left.width as u64 * left.height as u64))
                .then(right.width.cmp(&left.width))
                .then(right.modified_unix.cmp(&left.modified_unix)),
        })
        .then_with(|| left.id.cmp(&right.id))
    });
    results
}

pub fn evaluate_playlist<'a>(
    playlist: &Playlist,
    catalog: &'a [CatalogEntry],
    favorites: &BTreeSet<String>,
) -> Vec<&'a CatalogEntry> {
    match &playlist.membership {
        PlaylistMembership::Explicit { wallpaper_ids } => wallpaper_ids
            .iter()
            .filter_map(|id| catalog.iter().find(|entry| &entry.id == id))
            .collect(),
        PlaylistMembership::Smart { rule } => {
            // Playlist sequence remains independent of the public browse sort selection.
            let mut entries: Vec<_> = catalog
                .iter()
                .filter(|e| matches_rule(e, rule, favorites))
                .collect();
            entries.sort_by(|a, b| {
                natural_key(&a.title)
                    .cmp(&natural_key(&b.title))
                    .then(a.id.cmp(&b.id))
            });
            entries
        }
    }
}

pub fn unmatched_explicit_ids(playlist: &Playlist, catalog: &[CatalogEntry]) -> Vec<String> {
    let PlaylistMembership::Explicit { wallpaper_ids } = &playlist.membership else {
        return Vec::new();
    };
    wallpaper_ids
        .iter()
        .filter(|id| !catalog.iter().any(|entry| &entry.id == *id))
        .cloned()
        .collect()
}

fn natural_key(value: &str) -> Vec<NaturalPart> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut digit = None;
    for character in value.to_lowercase().chars() {
        let is_digit = character.is_ascii_digit();
        if digit.is_some_and(|previous| previous != is_digit) {
            parts.push(NaturalPart::from_text(&current, digit.unwrap_or(false)));
            current.clear();
        }
        digit = Some(is_digit);
        current.push(character);
    }
    if !current.is_empty() {
        parts.push(NaturalPart::from_text(&current, digit.unwrap_or(false)));
    }
    parts
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum NaturalPart {
    Text(String),
    Number(u64),
}

impl NaturalPart {
    fn from_text(value: &str, digit: bool) -> Self {
        if digit {
            Self::Number(value.parse().unwrap_or(u64::MAX))
        } else {
            Self::Text(value.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate_state::{PlaybackOrder, PlaylistMembership};
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn read_only_project_tags_are_normalized_without_inventing_metadata() {
        assert_eq!(
            project_tags(br#"{"tags":[" Landscape ","LANDSCAPE","Relaxing",1,""]}"#),
            BTreeSet::from(["landscape".into(), "relaxing".into()])
        );
        assert!(project_tags(br#"{"title":"Landscape"}"#).is_empty());
        assert!(project_tags(b"invalid JSON").is_empty());
    }
    #[test]
    fn catalog_text_still_matches_title_id_and_folder_and_composes_with_tags() {
        let mut wallpaper = entry("workshop-123", "Forest dusk", "scene", 1920, 1080);
        wallpaper.folder = PathBuf::from("C:/library/quiet");
        wallpaper.tags = BTreeSet::from(["landscape".into(), "relaxing".into()]);
        let mut rule = SmartRule {
            text: "FOREST 123 QUIET".into(),
            ..Default::default()
        };
        let favorites = BTreeSet::new();
        assert!(matches_rule(&wallpaper, &rule, &favorites));
        rule.tags = BTreeSet::from(["Landscape".into(), "absent".into()]);
        assert!(!matches_rule(&wallpaper, &rule, &favorites));
        rule.tags_match_any = true;
        assert!(matches_rule(&wallpaper, &rule, &favorites));
        rule.excluded_tags.insert("RELAXING".into());
        assert!(!matches_rule(&wallpaper, &rule, &favorites));
        rule.excluded_tags.clear();
        rule.minimum_width = 3840;
        assert!(!matches_rule(&wallpaper, &rule, &favorites));
        rule.minimum_width = 0;
        rule.text = "absent folder".into();
        assert!(!matches_rule(&wallpaper, &rule, &favorites));
    }
    #[test]
    fn legacy_smart_rules_default_to_no_tag_filter_and_new_rules_round_trip() {
        let mut rule: SmartRule = toml::from_str("text='quiet'\nfavorites_only=true").unwrap();
        assert!(rule.tags.is_empty() && rule.excluded_tags.is_empty() && !rule.tags_match_any);
        rule.tags.insert("landscape".into());
        rule.excluded_tags.insert("anime".into());
        rule.tags_match_any = true;
        let copy: SmartRule = toml::from_str(&toml::to_string(&rule).unwrap()).unwrap();
        assert_eq!(copy, rule);
    }

    fn entry(id: &str, title: &str, kind: &str, width: u32, height: u32) -> CatalogEntry {
        CatalogEntry {
            id: id.into(),
            title: title.into(),
            kind: kind.into(),
            folder: PathBuf::from(if id.starts_with('a') { "A" } else { "B" }),
            width,
            height,
            modified_unix: 0,
            source_tags: Default::default(),
            tags: BTreeSet::new(),
        }
    }

    fn cached_dimensions_with<F>(
        cache: &mut DimensionCache,
        key: DimensionCacheKey,
        load: F,
    ) -> (u32, u32)
    where
        F: FnOnce() -> (u32, u32),
    {
        if let Some(dimensions) = cache.get(&key) {
            return dimensions;
        }
        let dimensions = load();
        cache.insert(key, dimensions);
        dimensions
    }

    #[test]
    fn dimension_cache_reuses_matching_path_size_and_mtime_only() {
        let loads = AtomicUsize::new(0);
        let mut cache = DimensionCache::default();
        let key = DimensionCacheKey {
            path: PathBuf::from("poster.png"),
            size: 123,
            modified_nanos: Some(456),
        };
        let load = || {
            loads.fetch_add(1, Ordering::Relaxed);
            (1920, 1080)
        };

        assert_eq!(
            cached_dimensions_with(&mut cache, key.clone(), load),
            (1920, 1080)
        );
        assert_eq!(
            cached_dimensions_with(&mut cache, key.clone(), load),
            (1920, 1080)
        );
        assert_eq!(loads.load(Ordering::Relaxed), 1);

        let changed = DimensionCacheKey {
            size: 124,
            modified_nanos: Some(457),
            ..key
        };
        assert_eq!(
            cached_dimensions_with(&mut cache, changed, load),
            (1920, 1080)
        );
        assert_eq!(loads.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn search_filter_and_unknown_sort_identity_compose() {
        let catalog = vec![
            entry("a10", "Forest 10", "scene", 3840, 2160),
            entry("a2", "Forest 2", "scene", 1920, 1080),
            entry("b1", "City 1", "video", 1080, 1920),
        ];
        let rule = SmartRule {
            text: "forest".into(),
            kinds: BTreeSet::from(["scene".into()]),
            orientation: OrientationFilter::Landscape,
            ..Default::default()
        };
        let results = query_catalog(
            &catalog,
            &rule,
            &BTreeSet::new(),
            SortOrder::Colour,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &Default::default(),
        );

        assert_eq!(
            results
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["a10", "a2"]
        );
    }

    #[test]
    fn smart_playlist_refreshes_against_current_catalog() {
        let playlist = Playlist {
            id: "wide".into(),
            name: "Wide".into(),
            order: PlaybackOrder::Sequential,
            membership: PlaylistMembership::Smart {
                rule: SmartRule {
                    minimum_width: 2560,
                    ..Default::default()
                },
            },
        };
        let mut catalog = vec![entry("a", "A", "image", 1920, 1080)];
        assert!(evaluate_playlist(&playlist, &catalog, &BTreeSet::new()).is_empty());
        catalog.push(entry("b", "B", "image", 3840, 2160));
        assert_eq!(
            evaluate_playlist(&playlist, &catalog, &BTreeSet::new())[0].id,
            "b"
        );
    }

    #[test]
    fn explicit_playlist_preserves_order_and_reports_missing_items() {
        let playlist = Playlist {
            id: "chosen".into(),
            name: "Chosen".into(),
            order: PlaybackOrder::Sequential,
            membership: PlaylistMembership::Explicit {
                wallpaper_ids: vec!["b".into(), "missing".into(), "a".into()],
            },
        };
        let catalog = vec![
            entry("a", "A", "image", 1, 1),
            entry("b", "B", "image", 1, 1),
        ];

        assert_eq!(
            evaluate_playlist(&playlist, &catalog, &BTreeSet::new())
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert_eq!(unmatched_explicit_ids(&playlist, &catalog), ["missing"]);
    }
}

#[cfg(test)]
mod folder_query_tests {
    use super::*;
    #[test]
    fn w3_folder_query_compares_supported_windows_spellings_by_component() {
        for (ordinary, verbatim) in [
            (r"J:\Library\project", r"\\?\J:\Library\project"),
            (r"\\server\share\project", r"\\?\UNC\server\share\project"),
        ] {
            assert!(folder_query_contains(
                Path::new(ordinary),
                Path::new(verbatim)
            ));
            assert!(folder_query_contains(
                Path::new(verbatim),
                Path::new(ordinary)
            ));
        }
        assert!(folder_query_contains(
            Path::new(r"\\?\J:\LIBRARY\project"),
            Path::new("j:/library")
        ));
        assert!(!folder_query_contains(
            Path::new(r"\\?\J:\LibrarySibling\project"),
            Path::new(r"J:\Library")
        ));
        assert!(!folder_query_contains(
            Path::new(r"\\?\Volume{unknown}\Library"),
            Path::new(r"J:\Library")
        ));
    }
}

#[cfg(test)]
mod pk02_resolution_tests {
    use super::*;
    use crate::candidate_state::preferences::{Orientation, ResolutionPreset};
    #[test]
    fn pk02_typed_range_boundaries_unknown_dimensions_and_orientation() {
        let wide = ResolutionPreset {
            retained: Default::default(),
            label: "band".into(),
            orientation: Orientation::Wide,
            from: (1080, 1920),
            to: Some((1439, 2559)),
        };
        assert!(wide.valid());
        let mut rule = SmartRule {
            resolution_preset: Some(wide.clone()),
            ..Default::default()
        };
        let catalog = |w, h| CatalogEntry {
            source_tags: BTreeSet::new(),
            id: "id".into(),
            title: "".into(),
            kind: "image".into(),
            folder: PathBuf::new(),
            tags: BTreeSet::new(),
            width: w,
            height: h,
            modified_unix: 0,
        };
        for (w, h, expected) in [
            (1920, 1080, true),
            (2559, 1439, true),
            (1919, 1080, false),
            (1920, 1079, false),
            (2560, 1439, false),
            (2559, 1440, false),
            (0, 1080, false),
            (1920, 0, false),
            (1080, 1920, false),
        ] {
            assert_eq!(
                matches_rule(&catalog(w, h), &rule, &BTreeSet::new()),
                expected,
                "{w}x{h}"
            );
        }
        rule.resolution_preset.as_mut().unwrap().to = None;
        assert!(matches_rule(
            &catalog(u32::MAX, u32::MAX - 1),
            &rule,
            &BTreeSet::new()
        ));
        rule.resolution_preset.as_mut().unwrap().orientation = Orientation::Tall;
        assert!(matches_rule(&catalog(1080, 1920), &rule, &BTreeSet::new()));
        assert!(!matches_rule(&catalog(1920, 1080), &rule, &BTreeSet::new()));
        rule.orientation = OrientationFilter::Landscape;
        assert!(
            !matches_rule(&catalog(1080, 1920), &rule, &BTreeSet::new()),
            "independent orientation AND snapshot"
        );
        let square = ResolutionPreset {
            from: (10, 10),
            to: Some((10, 10)),
            ..wide.clone()
        };
        assert!(square.matches_dimensions(10, 10));
        assert!(!ResolutionPreset {
            orientation: Orientation::Tall,
            ..square
        }
        .matches_dimensions(10, 10));
        assert!(wide.matches_orientation(OrientationFilter::Square));
        assert!(wide.matches_orientation(OrientationFilter::Any));
        assert!(!wide.matches_orientation(OrientationFilter::Portrait));
        assert!(!ResolutionPreset {
            from: (0, 1),
            ..wide.clone()
        }
        .valid());
        assert!(!ResolutionPreset {
            to: Some((1919, 1080)),
            ..wide.clone()
        }
        .valid());
        assert!(wide.same_range(&ResolutionPreset {
            label: "different".into(),
            from: (1920, 1080),
            to: Some((2559, 1439)),
            ..wide.clone()
        }));
        let roundtrip: SmartRule = toml::from_str(&toml::to_string(&rule).unwrap()).unwrap();
        assert_eq!(
            roundtrip, rule,
            "explicit saved smart-query snapshots round-trip"
        );
        let legacy: SmartRule = toml::from_str("minimum_width=1920").unwrap();
        assert!(legacy.resolution_preset.is_none());
        assert!(!toml::to_string(&legacy)
            .unwrap()
            .contains("resolution_preset"));
    }
}

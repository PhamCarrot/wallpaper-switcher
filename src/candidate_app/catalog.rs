use super::gallery::poster_media_for_item;
use super::*;
use crate::{apply_trace, engine_ipc, library, native_wallpaper, scanner};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SourceFilter {
    All,
    Pictures,
    Videos,
    WallpaperEngine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CatalogPresentation {
    Loading,
    EmptyCatalog,
    EmptyFilter,
    Ready,
    PosterFailure,
}

impl CatalogPresentation {
    pub(super) fn title(self) -> &'static str {
        match self {
            Self::Loading => "Loading wallpapers…",
            Self::EmptyCatalog => "No wallpapers found",
            Self::EmptyFilter => "No wallpapers match this filter",
            Self::Ready => "",
            Self::PosterFailure => "Wallpapers found; previews unavailable",
        }
    }

    pub(super) fn kind(self) -> &'static str {
        match self {
            Self::Loading => "CATALOG",
            Self::EmptyCatalog => "EMPTY",
            Self::EmptyFilter => "FILTER",
            Self::Ready => "CATALOG",
            Self::PosterFailure => "PREVIEW ERROR",
        }
    }
}

pub(super) fn catalog_presentation(
    catalog_loaded: bool,
    catalog_items: usize,
    filtered_items: usize,
    poster_failure: bool,
) -> CatalogPresentation {
    if !catalog_loaded {
        CatalogPresentation::Loading
    } else if catalog_items == 0 {
        CatalogPresentation::EmptyCatalog
    } else if filtered_items == 0 {
        CatalogPresentation::EmptyFilter
    } else if poster_failure {
        CatalogPresentation::PosterFailure
    } else {
        CatalogPresentation::Ready
    }
}

impl SourceFilter {
    pub(super) fn from_screenshot_arg(value: &str) -> Option<Self> {
        match value {
            "pic" | "picture" | "pictures" => Some(Self::Pictures),
            _ => None,
        }
    }

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::All => "ALL",
            Self::Pictures => "PIC",
            Self::Videos => "VID",
            Self::WallpaperEngine => "WE",
        }
    }

    pub(super) fn accepts(self, source: WallpaperSource) -> bool {
        matches!(
            (self, source),
            (Self::All, _)
                | (Self::Pictures, WallpaperSource::LocalImage)
                | (Self::Videos, WallpaperSource::LocalVideo)
                | (Self::WallpaperEngine, WallpaperSource::WallpaperEngine)
        )
    }
}

pub(super) fn sort_label(order: SortOrder) -> &'static str {
    match order {
        SortOrder::Newest => "Newest",
        SortOrder::Recent => "Recently applied · local confirmed",
        SortOrder::Colour => "Colour",
        SortOrder::ColourPop => "Colour pop",
        SortOrder::Richness => "Richness ↓",
        SortOrder::Minimalist => "Minimalist ↑",
        SortOrder::ResolutionDescending => "Resolution ↓",
    }
}

pub(super) fn orientation_label(orientation: OrientationFilter) -> &'static str {
    match orientation {
        OrientationFilter::Any => "Any orientation",
        OrientationFilter::Landscape => "Landscape",
        OrientationFilter::Portrait => "Portrait",
        OrientationFilter::Square => "Square",
    }
}

pub(super) fn canonical_paths_equal(left: &std::path::Path, right: &std::path::Path) -> bool {
    let canonical_left = std::fs::canonicalize(left).unwrap_or_else(|_| left.to_path_buf());
    let canonical_right = std::fs::canonicalize(right).unwrap_or_else(|_| right.to_path_buf());
    canonical_left
        .to_string_lossy()
        .eq_ignore_ascii_case(&canonical_right.to_string_lossy())
}

pub(super) fn wallpaper_index_by_id(wallpapers: &[WallpaperItem], id: &str) -> Option<usize> {
    wallpapers.iter().position(|wallpaper| wallpaper.id == id)
}

pub(super) fn wallpaper_index_for_active_files(
    wallpapers: &[WallpaperItem],
    active_files: impl IntoIterator<Item = String>,
    native_cache_dir: &std::path::Path,
) -> Option<usize> {
    active_files.into_iter().find_map(|active| {
        scanner::find_active_wallpaper_index(wallpapers, &active, native_cache_dir)
    })
}

pub(super) fn wallpaper_allowed_on_monitor(
    config: &Config,
    monitor_key: &str,
    show_excluded: bool,
    wallpaper_id: &str,
) -> bool {
    !config.is_disabled(monitor_key, wallpaper_id)
        && (show_excluded || !config.is_excluded(monitor_key, wallpaper_id))
}

pub(super) fn folder_filter_path(path: std::path::PathBuf) -> std::path::PathBuf {
    path.canonicalize().unwrap_or(path)
}

impl GpuCandidate {
    pub(super) fn install_cached_catalog(&mut self, snapshot: catalog_snapshot::Snapshot) {
        (self.wallpapers, self.catalog) = snapshot.into_catalog();
        self.project_local_tags();
        self.catalog_loaded = true;
        self.catalog_fresh = false;
        self.reset_default_folder();
        self.selected = self.active_wallpaper_index();
        self.refresh_filter();
        self.start_colours(false);
        if self.visible {
            self.schedule_posters();
            self.refresh_backdrop();
        }
        apply_trace::event_lazy("catalog-snapshot-installed", || {
            format!(
                "items={} filtered={} fresh={} status={:?}",
                self.wallpapers.len(),
                self.filtered_indices.len(),
                self.catalog_fresh,
                self.status_for_display(),
            )
        });
    }

    pub(super) fn accept_fresh_catalog(&mut self, generation: u64) {
        self.catalog_fresh = true;
        if let Some(owner) = &self.catalog_watch {
            owner.accept_snapshot(generation);
        }
        self.prepare_startup_reapply();
        apply_trace::event_lazy("catalog-fresh-accepted", || {
            format!(
                "scan={generation} revision={} items={} fresh={} status={:?}",
                self.catalog_generation.load(Ordering::Acquire),
                self.wallpapers.len(),
                self.catalog_fresh,
                self.status_for_display(),
            )
        });
    }

    pub(super) fn available_folders(&self) -> Vec<std::path::PathBuf> {
        donor_layout::layout_work(0, self.catalog.len());
        self.catalog
            .iter()
            .map(|entry| entry.folder.clone())
            .chain(
                self.config
                    .catalog
                    .local_folders
                    .iter()
                    .cloned()
                    .map(folder_filter_path),
            )
            .chain(self.filter_rule.folders.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect()
    }

    pub(super) fn refresh_catalog(&mut self) {
        let generation = self.catalog_scan_generation.fetch_add(1, Ordering::AcqRel) + 1;
        if let Some(owner) = &self.catalog_watch {
            owner.request(
                self.config.catalog.local_folders.clone(),
                self.config.behavior.include_default_projects,
                self.state.frontend_preferences.library.clone(),
                generation,
            );
            self.status = Some("Refreshing wallpaper catalog…".into());
        } else {
            self.status =
                Some("Catalog watch owner is unavailable; refresh was not scheduled.".into());
        }
    }

    /// Retain only exact healthy resources, remapping stable IDs before replacing the catalog.
    pub(super) fn retain_catalog_posters(&mut self, incoming: &[WallpaperItem], revision: u64) {
        self.depth_roll = None;
        self.ordinary_roll = None;
        self.ordinary_transition_issue = None;
        self.depth_frame = None;
        self.invalidate_poster_work();
        let mut slots = HashMap::new();
        let mut posters = Vec::new();
        let mut ids = HashSet::new();
        let mut duplicates = HashSet::new();
        for item in incoming {
            if !ids.insert(item.id.as_str()) {
                duplicates.insert(item.id.as_str());
            }
        }
        for (index, item) in incoming.iter().enumerate() {
            if duplicates.contains(item.id.as_str()) {
                continue;
            }
            let mut matching = self.poster_slots.values().filter(|s| s.item_id == item.id);
            let Some(slot) = matching.next() else {
                continue;
            };
            let (path, kind) = poster_media_for_item(item);
            if matching.next().is_some() || slot.source != path || slot.kind != kind {
                continue;
            }
            use crate::poster_worker::{PosterKind, PosterStamp};
            let stamp = match kind {
                PosterKind::Still => {
                    library::media_still::Fingerprint::capture(&path).map(PosterStamp::Still)
                }
                PosterKind::Video => item.video_stamp().map(PosterStamp::Video),
                PosterKind::Placeholder => None,
            };
            let Some(poster) = self.posters.iter().find(|p| {
                p.layer == slot.layer
                    && p.source == slot.source
                    && p.matches_source(kind, stamp.clone())
            }) else {
                continue;
            };
            let mut slot = slot.clone();
            slot.catalog_generation = revision;
            slots.insert(index, slot);
            posters.push(poster.clone());
        }
        self.poster_slots = slots;
        self.posters = Arc::new(posters);
    }

    pub(super) fn resolution_active(&self) -> bool {
        self.filter_rule.resolution_preset.is_some()
            || self.filter_rule.minimum_width != 0
            || self.filter_rule.minimum_height != 0
    }

    pub(super) fn resolution_label(&self) -> String {
        if let Some(preset) = &self.filter_rule.resolution_preset {
            preset.summary()
        } else if self.resolution_active() {
            format!(
                "≥{}×{}",
                self.filter_rule.minimum_width, self.filter_rule.minimum_height
            )
        } else {
            "SIZE".into()
        }
    }

    pub(super) fn refresh_filter(&mut self) {
        self.refresh_filter_with_poster_hold(false);
    }

    pub(super) fn refresh_filter_with_poster_hold(&mut self, hold_wall_posters: bool) {
        let hold_wall_posters = hold_wall_posters && self.atlas_entry.is_none();
        let defer_wall_colour = self.defer_wall_colour_sort();
        if !defer_wall_colour {
            self.prune_colour_stamps();
        }
        self.ordinary_transition_issue = None;
        let previous_filtered = self.filtered_indices.clone();
        let outgoing = self.capture_depth_roll();
        let (ordinary_outgoing, ordinary_error) = match self.capture_ordinary_roll() {
            Ok(roll) => (roll, None),
            Err(error) => (None, Some(error)),
        };
        let monitor_key = self.active_monitor_key();
        let index_by_id: HashMap<&str, usize> = self
            .catalog
            .iter()
            .enumerate()
            .map(|(index, entry)| (entry.id.as_str(), index))
            .collect();
        let browse_ids = self.browse_playlist_id.as_ref().and_then(|id| {
            self.state
                .playlists
                .iter()
                .find(|p| &p.id == id)
                .map(|playlist| {
                    library::evaluate_playlist(playlist, &self.catalog, &self.state.favorites)
                        .into_iter()
                        .map(|entry| entry.id.clone())
                        .collect::<std::collections::HashSet<_>>()
                })
        });
        if self.browse_playlist_id.is_some() && browse_ids.is_none() {
            self.browse_playlist_id = None;
            self.browse_playlist_unavailable = true;
        }
        // Until analysis settles, Colour's unknown-metric ID order stays stable
        // even when an unrelated folder, search or source filter is refreshed.
        let no_metrics = library::sort_metrics::MetadataView::default();
        let sorting = if defer_wall_colour {
            &no_metrics
        } else {
            &self.colours.sorting
        };
        self.filtered_indices = library::query_catalog(
            &self.catalog,
            &self.filter_rule,
            &self.state.favorites,
            self.sort_order,
            &self.colours.query,
            &self.colours.metadata,
            sorting,
            &self.state.local_library.usage,
        )
        .into_iter()
        .filter(|entry| {
            browse_ids
                .as_ref()
                .is_none_or(|ids| ids.contains(&entry.id))
        })
        .filter(|entry| {
            monitor_key.as_ref().is_none_or(|monitor_key| {
                wallpaper_allowed_on_monitor(
                    &self.config,
                    monitor_key,
                    self.show_excluded,
                    &entry.id,
                )
            })
        })
        .filter_map(|entry| index_by_id.get(entry.id.as_str()).copied())
        .filter(|index| {
            self.wallpapers
                .get(*index)
                .is_some_and(|wallpaper| self.source_filter.accepts(wallpaper.source))
        })
        .collect();
        if previous_filtered != self.filtered_indices {
            self.presentation_inputs = None;
        }
        if self.colours.passive_requery {
            if let Some(index) = self
                .colours
                .selection_anchor
                .as_ref()
                .and_then(|id| self.catalog.iter().position(|e| &e.id == id))
                .filter(|index| self.filtered_indices.contains(index))
            {
                self.selected = Some(index);
                self.colours.selection_anchor = None;
            }
        } else {
            self.colours.selection_anchor = None;
        }
        if self
            .selected
            .is_none_or(|selected| !self.filtered_indices.contains(&selected))
        {
            self.cancel_detail("filter-invalidated-selection");
            self.selected = self.filtered_indices.first().copied();
            apply_trace::event_lazy("ui-item-selected", || {
                format!(
                    "reason=filter-invalidated-selection {}",
                    self.selected_item_trace_fields()
                )
            });
        }
        let replacement = previous_filtered != self.filtered_indices;
        if !replacement {
            self.update_catalog_status();
            return;
        }
        self.reset_depth_transients();
        self.invalidate_poster_work();
        if replacement
            && self.visible
            && self.gallery_mode == GalleryMode::Depth
            && !self.filtered_indices.is_empty()
            && outgoing.is_some()
        {
            self.depth_roll = outgoing;
            self.start_motion(0.0);
        }
        if replacement
            && self.visible
            && self.gallery_mode != GalleryMode::Depth
            && !self.filtered_indices.is_empty()
        {
            self.ordinary_roll = ordinary_outgoing.map(|mut roll| {
                if hold_wall_posters && !roll.old.is_empty() {
                    roll.poster_wait_deadline =
                        Some(std::time::Instant::now() + std::time::Duration::from_secs(4));
                }
                roll
            });
            self.start_motion(0.0);
        }
        self.poster_failure = false;
        if replacement {
            self.ordinary_transition_issue = ordinary_error;
        }
        self.update_catalog_status();
    }

    pub(super) fn catalog_presentation(&self) -> CatalogPresentation {
        catalog_presentation(
            self.catalog_loaded,
            self.wallpapers.len(),
            self.filtered_indices.len(),
            self.poster_failure,
        )
    }

    pub(super) fn update_catalog_status(&mut self) {
        let catalog_status = match self.catalog_presentation() {
            CatalogPresentation::EmptyCatalog => {
                Some("No wallpapers found in the configured folders.".into())
            }
            CatalogPresentation::EmptyFilter => {
                Some("No wallpapers match the current filter.".into())
            }
            CatalogPresentation::PosterFailure => {
                Some("Wallpapers found, but their previews could not be loaded.".into())
            }
            CatalogPresentation::Loading => Some("Loading wallpapers…".into()),
            CatalogPresentation::Ready => None,
        };
        let status = self
            .engine_properties_diagnostic
            .clone()
            .or_else(|| self.ordinary_transition_issue.clone())
            .or(catalog_status);
        self.status = status
            .as_deref()
            .map(|message| self.status_with_persistence_issue(message))
            .or_else(|| self.persistence_status());
    }

    pub(super) fn active_wallpaper_files(&self) -> Vec<String> {
        let Some(active_id) = self.active_monitor_id.as_ref() else {
            return Vec::new();
        };
        let Some(monitor_index) = self
            .monitor_ids
            .iter()
            .position(|device_id| device_id.eq_ignore_ascii_case(active_id))
        else {
            return Vec::new();
        };
        let mut files = Vec::with_capacity(2);
        if engine_ipc::is_running() {
            if let Some(file) = scanner::get_active_wallpaper_file(monitor_index) {
                files.push(file);
            }
        }
        if let Ok(path) = native_wallpaper::get_wallpaper_for_device(active_id) {
            let path = path.to_string_lossy().into_owned();
            if !files
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(&path))
            {
                files.push(path);
            }
        }
        files
    }

    pub(super) fn active_wallpaper_index(&self) -> Option<usize> {
        let native_cache_dir = Config::cache_dir().join("native-images");
        wallpaper_index_for_active_files(
            &self.wallpapers,
            self.active_wallpaper_files(),
            &native_cache_dir,
        )
    }
}

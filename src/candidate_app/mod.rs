#[cfg(test)]
use crate::donor_ui;
#[path = "automation.rs"]
mod automation_integration;
mod catalog;
mod catalog_snapshot;
mod catalog_watch;
mod colours;
mod detail_files;
mod detail_motion;
mod diagnostics;
mod events;
mod gallery;
mod gallery_presentation;
mod hex_presentation;
mod images;
mod lifecycle;
mod local_library;
mod localization;
mod motion_mailbox;
mod native_caption;
mod operations;
mod wall_colour_hold;
use wall_colour_hold::WallPosterPin;
mod persistence;
mod playlists;
mod runtime;
mod startup_loading;
#[cfg(test)]
mod tests;
mod theme_workflow;
mod usage;
mod view;
mod wall_presentation;

#[cfg(test)]
use gallery::{
    activate_gallery_variant, advance_gallery_motion, apply_saved_gallery_preset,
    choose_geometric_curve, choose_geometric_shape, configure_wall_variant_geometry,
    depth_poster_requests, gallery_layout_params, gallery_variant_for_viewport,
    navigate_gallery_mode, next_gallery_variant_index, next_shape_filter, poster_indices_for_view,
    poster_mode_requires_invalidation, poster_request_changed, poster_source_for_item,
    prioritized_poster_indices, wall_visible_poster_indices, GalleryNavigationEffect,
};
use gallery::{
    apply_gallery_viewport_policy, card_click_action, consume_depth_pointer, depth_wheel_steps,
    filtered_poster_aspects, gallery_kind, gallery_layout_params_for_app, gallery_mode,
    gallery_number_value, geometric_curve_choices, geometric_shape_choices,
    poster_completion_is_current, poster_completion_matches, poster_slot_matches,
    set_gallery_number_value, shape_chip_label, trace_gallery_mouse_consumption,
    view_legacy_selection, wall_layout_choices, CardClickAction, DepthPointerIntent,
    DepthPresentation, GalleryNumberField, PosterRequestIdentity, PosterSlot,
    DEPTH_POSITION_FIELDS,
};
#[cfg(test)]
use images::backdrop_dimensions;

#[cfg(test)]
use automation_integration::{
    automation_submission_block, consume_automation_tick, AutomationSubmissionBlock,
    AutomationTickContext,
};
use automation_integration::{
    parse_schedule_time, unique_playlist_id, unique_schedule_id, AutomationTraceContext,
};
#[cfg(test)]
use catalog::wallpaper_index_for_active_files;
use catalog::{
    canonical_paths_equal, catalog_presentation, orientation_label, sort_label,
    wallpaper_allowed_on_monitor, wallpaper_index_by_id, CatalogPresentation, SourceFilter,
};
use persistence::successful_engine_persistence_status;
#[cfg(test)]
use persistence::{diagnostic_status, state_save_failure_message};

use events::{update, update_with_startup};
#[cfg(test)]
use lifecycle::{
    close_owned_sheet, exit_action_for_ui, matched_selection_navigation, matches_candidate_key,
    top_owned_sheet, visibility_after_tray_open, EscapeAction, OwnedSheet, PanelEffect,
    SelectionNavigation,
};
use lifecycle::{
    initial_panels, sheet_owns_keyboard, should_handle_key_event, NavigationKeyEvent, PanelState,
};
use operations::{
    cursor_monitor_id, engine_property_defaults, engine_property_schema_status_message,
    provider_mutation_allowed, provider_recovery_action_available,
};
#[cfg(test)]
use operations::{
    monitor_id_at_point, plan_apply_request, provider_recovery_action_available_with,
    replace_engine_property_controls, update_provider_recovery_state, ApplyPlanError,
};

#[cfg(test)]
use view::{
    application_style, backdrop_background_rgb, detail_chrome_visibility, donor_palette,
    gallery_background_layers, gallery_snapshot_colors, motion_settings_content, position_content,
    rail_chip_style, rail_palette, settings_category_matches, settings_search_hits, theme,
    theme_role_editor_content, theme_settings_content, view,
};
use view::{backdrop_click_dismisses, detail_actions, detail_controls_active, gallery_rail_model};

use crate::log_debug;
#[cfg(test)]
use crate::monitor;
use crate::{
    appearance_theme, apply_trace, automation, candidate_state, coordinator, donor_back_layout,
    donor_detail, donor_layout, donor_rail, donor_scene, engine_ipc, engine_properties,
    gpu_gallery, library, native_wallpaper, poster_worker, scanner, theme_roles, transition, tray,
};
use diagnostics::{
    build_catalog_playlist_snapshot, initial_screenshot_source_filter, BenchmarkState,
    ScenarioState, PLAYLIST_CATALOG_SNAPSHOT_STAGE,
};
#[cfg(test)]
use diagnostics::{explicit_playlist_catalog_observation, scenario_window_identity_matches};
pub(super) use runtime::run;
use runtime::{local_schedule_time, unix_now, window_settings};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use futures_channel::mpsc::{unbounded, UnboundedReceiver, UnboundedSender};
use iced::widget::{
    button, checkbox, column, container, image as iced_image, pick_list, row, scrollable, shader,
    slider, stack, text, text_input, Space,
};
use iced::{
    event, keyboard, window, Background, Color, ContentFit, Element, Event, Fill, Subscription,
    Task, Theme,
};

use crate::candidate_state::{
    commit_saved_state, persist_candidate_state, BackgroundPresentation, CandidateState,
    GalleryKind, GeometricCurve as StateGeometricCurve, GeometricShape as StateGeometricShape,
    OrientationFilter, PlaybackOrder, Playlist, PlaylistMembership, SaveError, SaveOutcome,
    SmartRule, WallLayout as StateWallLayout,
};
use crate::config::{Config, KeybindsConfig};
use crate::coordinator::{
    ApplyTarget, CoordinatorResult, EngineCommand, OperationId, ProviderRecovery,
    WallpaperCoordinator,
};
use crate::donor_scene::DonorSceneState;
use crate::gpu_gallery::{
    GalleryColors, GalleryLayoutParams, GalleryMessage, GalleryMode, GalleryProgram,
    GallerySnapshot, GeometricCurve, GeometricParams, GeometricShape, Poster, SliceParams,
    WallLayout, WallParams,
};
use crate::hotkey::GlobalHotkeyListener;
use crate::library::SortOrder;
use crate::scanner::{WallpaperItem, WallpaperSource};
use crate::tray::TrayAction;
use crate::upstream_slices_adapter::{
    browse_intent, resolve_item, resolve_master_index, BrowseIntent, GalleryMotionState,
};

static WAKE_TX: OnceLock<Mutex<Option<UnboundedSender<Wake>>>> = OnceLock::new();
static WAKE_RX: OnceLock<Mutex<Option<UnboundedReceiver<Wake>>>> = OnceLock::new();

const UI_FONT: iced::Font = iced::Font {
    family: iced::font::Family::Name("Roboto Condensed"),
    weight: iced::font::Weight::Bold,
    stretch: iced::font::Stretch::Normal,
    style: iced::font::Style::Normal,
};
const NERD_FONT: iced::Font = iced::Font::with_name("Symbols Nerd Font");
const PLAYLIST_CHOOSER_QUERY_ID: &str = "playlist-chooser-query";

#[derive(Debug, Clone)]
enum Wake {
    Toggle,
    RailFrame {
        generation: u64,
        value: f32,
    },
    Tray(TrayAction),
    CatalogDirty {
        generation: u64,
    },
    CacheAcknowledged(poster_worker::cache::Report),
    CatalogLoaded {
        generation: u64,
        wallpapers: Vec<WallpaperItem>,
        entries: Vec<library::CatalogEntry>,
    },
    ColourReady(poster_worker::colour::Completion),
    CatalogSnapshotSaved(Result<(), String>),
    PostersReady,
    StartupLoadingCheck,
    RetryPosters(u64),
    PosterWorkerExited(poster_worker::WorkerExit),
    PosterGpu(gpu_gallery::PosterFeedback),
    BackdropLoaded {
        generation: u64,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
    CoordinatorFinished(CoordinatorResult),
    MotionReady,
    MotionFrame {
        generation: u64,
        value: f32,
        velocity: f32,
        state: GalleryMotionState,
        selection: HashMap<String, GalleryMotionState>,
        presentation: Option<DepthPresentation>,
        ordinary: Option<gallery_presentation::GalleryPresentation>,
        hex: Option<hex_presentation::HexPresentation>,
        wall: Option<wall_presentation::WallPresentation>,
        scene: Option<gallery_presentation::SceneMotion>,
    },
    DetailFrame {
        generation: u64,
        value: f32,
        close_when_done: bool,
        spring: Option<GalleryMotionState>,
    },
    AutomationTick {
        generation: u64,
        now_unix: u64,
        weekday: u8,
        minute_of_day: u16,
    },
    BenchmarkVisibility(bool),
    BenchmarkNavigate(usize),
    BenchmarkFinished,
    CloseHiddenWindow(u64),
    CaptureScreenshot,
    ScenarioAdvance,
    ScenarioStepReady {
        step: usize,
        window_mode: Option<window::Mode>,
    },
    ScenarioBeginWindowSettlement {
        step: usize,
        requested: window::Mode,
    },
    ScenarioWindowPoll {
        step: usize,
        generation: u64,
    },
    ScenarioWindowObserved {
        step: usize,
        generation: u64,
        window_mode: Option<window::Mode>,
    },
    ScenarioAbort,
    ScenarioTimeout,
}

#[derive(Debug, Clone)]
enum Message {
    ExitCommitted {
        decision: u64,
    },
    Wake(Wake),
    ExpireRotationNotice(std::time::Instant),
    DetailImageCopied(u64, Result<(), String>),
    WindowOpened {
        id: window::Id,
        width: f32,
        height: f32,
    },
    WindowResized {
        id: window::Id,
        width: f32,
        height: f32,
    },
    WindowClosed(window::Id),
    CloseRequested(window::Id),
    WindowFocused(window::Id),
    WindowUnfocused(window::Id),
    ScreenshotCaptured(window::Screenshot),
    CardClicked {
        index: usize,
        mouse_sequence: Option<u64>,
    },
    ApplySelected,
    SheetBackdrop,
    RefreshCatalog,
    RefreshThumbnails,
    ClearPosterCache,
    Colour(colours::Action),
    SetLibraryWatching(bool),
    SetLibraryPolling(u32),
    Gallery(GalleryMessage),
    GalleryInput {
        window: Option<window::Id>,
        visibility_generation: u64,
        motion_generation: u64,
        catalog_generation: u64,
        mode: GalleryMode,
        selected: Option<String>,
        settings_generation: u64,
        viewport: [f32; 2],
        target_source: Option<std::path::PathBuf>,
        message: GalleryMessage,
    },
    SetSourceFilter(SourceFilter),
    DonorRail(donor_rail::RailIntent),
    DonorDetail(String, donor_back_layout::DetailIntent),
    DepthDonorDetail(u64, String, donor_back_layout::DetailIntent),
    SetGalleryMode(GalleryMode),
    CyclePickerMode(GalleryMode),
    DepthDefault,
    OpenSearch,
    ToggleSettingsPanel,
    SettingsIntent {
        window: Option<window::Id>,
        generation: u64,
        intent: view::settings::folio::intent::Message,
    },
    SettingsPreviewReady {
        generation: u64,
        image: Option<iced_image::Handle>,
    },
    SettingsPageAction {
        window: Option<window::Id>,
        generation: u64,
        message: Box<Message>,
    },
    SettingsOpenPage {
        window: Option<window::Id>,
        generation: u64,
        category: SettingsCategory,
    },
    SettingsThemeClose {
        window: Option<window::Id>,
        generation: u64,
    },
    SettingsReloadSavedState {
        window: Option<window::Id>,
        generation: u64,
    },
    SettingsRecheckProvider {
        window: Option<window::Id>,
        generation: u64,
        recovery: ProviderRecovery,
    },
    SettingsCapturedKey {
        window: window::Id,
        key: keyboard::Key,
        modifiers: keyboard::Modifiers,
    },
    SettingsModifiers {
        window: window::Id,
        modifiers: keyboard::Modifiers,
    },
    SetSettingsCategory(SettingsCategory),
    SettingsSearchChanged(String),
    CloseSearchPanel,
    CycleShapeFilter,
    CycleGalleryVariant,
    CycleGalleryGeometry,
    SetGeometricCurve(StateGeometricCurve),
    SetGeometricShape(StateGeometricShape),
    SetGalleryNumber(GalleryNumberField, f32),
    SetDepthFrame(bool),
    SetDepthShadows(bool),
    SetDepthEffect(String),
    SetDepthShader(bool),
    SetDepthBack(bool),
    SetSliceWobble(bool),
    SetWallLayout(StateWallLayout),
    CycleGalleryMotionSpeed,
    CycleGalleryVariation,
    SaveGalleryPreset,
    ApplyGalleryPreset(GalleryKind, u64),
    DeleteGalleryPreset(GalleryKind, u64),
    ToggleFavorite,
    ToggleExcluded,
    ToggleShowExcluded,
    OpenFolder,
    SourceFolderError(detail_files::RequestOwner, String),
    ToggleEngineProperties,
    EnginePropertyValueChanged(String, serde_json::Value),
    EnginePropertyTextChanged(String, String),
    ApplyEngineProperties,
    SearchChanged(String),
    ToggleSearchTags,
    OpenLocalTags(String),
    BackToSearchFilters,
    LocalTag {
        owner: local_library::TagOwner,
        edit: local_library::TagEdit,
    },
    FolderPathChanged(String),
    AddLocalRoot,
    UnlistLocalRoot(std::path::PathBuf),
    SetInstalledDiscovery(bool),
    ToggleTagSort,
    SearchTag(String, bool),
    SetTagMatchAny(bool),
    ClearSearchTags,
    CycleSort,
    SetSort(SortOrder),
    CycleSortBack,
    ToggleFolderMenu,
    DismissFolderMenu,
    SetFolderFilter(Option<std::path::PathBuf>),
    CycleResolution,
    ClearResolution,
    CycleOrientation,
    ToggleFavoritesFilter,
    TogglePlaylistPanel,
    TogglePlaylistChooser,
    PlaylistBrowse(playlists::Owner, playlists::Browse),
    OpenPlaylistPicker,
    PlaylistEdit(playlists::Owner, playlists::Edit),
    PlaylistNameChanged(String),
    CreatePlaylist {
        smart: bool,
    },
    TogglePlaylistMembership(String),
    TogglePlaylistOrder(String),
    SetActivePlaylist(String, u64),
    BrowsePlaylist(String),
    HideGallery,
    PendingForms {
        window: Option<window::Id>,
        generation: u64,
        action: view::settings::pending_forms::Action,
    },

    CycleAppearancePreset,
    AppearanceColorChanged(AppearanceField, String),
    CycleAppearanceOpacity,
    SetBackgroundPresentation(BackgroundPresentation),
    ThemeDesigner(theme_workflow::Action),
    ThemeExitProbe,
    ThemeExitProbeObserved {
        window: window::Id,
        generation: u64,
        attempt: u8,
        mode: window::Mode,
    },
    ThemeExitProbeTimeout {
        window: window::Id,
        generation: u64,
    },
    ThemeExitProbePollFailed {
        window: window::Id,
        generation: u64,
    },
    ThemeWheel {
        editor_instance: u64,
        selection_generation: u64,
        role: theme_roles::ThemeRole,
        variant: theme_roles::ColorVariant,
        action: crate::donor_folio::source_ui::wheel::WheelAction,
    },
    ThemeRecent {
        editor_instance: u64,
        selection_generation: u64,
        role: theme_roles::ThemeRole,
        variant: theme_roles::ColorVariant,
        color: String,
    },
    ThemeRoleSelected {
        editor_instance: u64,
        role: theme_roles::ThemeRole,
    },
    ThemeVariantSelected {
        editor_instance: u64,
        variant: theme_roles::ColorVariant,
    },
    ThemeRoleFilterChanged {
        editor_instance: u64,
        text: String,
    },
    ThemeRoleTextChanged {
        editor_instance: u64,
        selection_generation: u64,
        role: theme_roles::ThemeRole,
        variant: theme_roles::ColorVariant,
        text: String,
    },
    ThemeRoleSetColor {
        editor_instance: u64,
        selection_generation: u64,
        role: theme_roles::ThemeRole,
        variant: theme_roles::ColorVariant,
    },
    ThemeRoleResetColor {
        editor_instance: u64,
        selection_generation: u64,
        role: theme_roles::ThemeRole,
        variant: theme_roles::ColorVariant,
    },
    ToggleEnginePaused,
    ToggleEngineMuted,
    CycleEngineVolume,
    ToggleRandomRotation,
    StopPlaylist {
        generation: u64,
    },
    ResumeRandomRotation,
    CycleSlideshowInterval,
    ScheduleNameChanged(String),
    ScheduleTimeChanged(String),
    ToggleScheduleDay(u8),
    ToggleScheduleMonitor(String),
    ToggleMonitorLock(String),
    CreateSchedule,
    ToggleSchedule(String),
    CycleTargetMonitor,
    KeyPressed {
        key: keyboard::Key,
        modifiers: keyboard::Modifiers,
        event: NavigationKeyEvent,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AppearanceField {
    Background,
    Surface,
    Text,
    Muted,
    Accent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum SettingsCategory {
    Picker,
    #[default]
    CatalogFilters,
    Position,
    WallpapersDisplays,
    Motion,
    Playback,
    Library,
    Sources,
    Automation,
    Theme,
}

impl SettingsCategory {
    const VISIBLE: [Self; 8] = [
        Self::Picker,
        Self::CatalogFilters,
        Self::Position,
        Self::Motion,
        Self::Playback,
        Self::Library,
        Self::Automation,
        Self::Theme,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Picker => "Picker",
            Self::CatalogFilters => "Catalog filters",
            Self::Position => "Position",
            Self::WallpapersDisplays => "Wallpapers & displays",
            Self::Motion => "Motion",
            Self::Playback => "Playback",
            Self::Library => "Library",
            Self::Sources => "Sources",
            Self::Automation => "Automation",
            Self::Theme => "Theme",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::Picker => "Choose the gallery and its supported arrangements.",
            Self::CatalogFilters => "Search the wallpaper catalog and refine the visible results.",
            Self::Position => "Adjust the active gallery arrangement and presets.",
            Self::WallpapersDisplays => "Choose the target display and automatic-change locks.",
            Self::Motion => "Choose the gallery motion behavior.",
            Self::Playback => "Control slideshow and supported Engine playback commands.",
            Self::Library => "Refresh the catalog and organize saved favorites and playlists.",
            Self::Sources => "Choose the catalog source and local folder scope.",
            Self::Automation => "Configure supported schedules and their targets.",
            Self::Theme => "Adjust the candidate appearance and overlay colors.",
        }
    }

    fn search_terms(self) -> &'static str {
        match self {
            Self::Picker => "gallery slices geometric wall arrangement",
            Self::CatalogFilters => "catalog search pictures videos engine folders resolution orientation favorites sort",
            Self::Position => "arrangement layout rows columns width height radius spacing gaps skew tilt wobble curve tile presets stagger cylinder",
            Self::WallpapersDisplays => "wallpaper display monitor target locks",
            Self::Motion => "motion animation speed fast standard slow",
            Self::Playback => "slideshow playback wallpaper engine pause mute volume playlists",
            Self::Library => "library favorites playlists refresh",
            Self::Sources => "source folders pictures videos wallpaper engine workshop",
            Self::Automation => "automation schedules days time monitor",
            Self::Theme => "theme appearance nord colors opacity background surface accent text",
        }
    }
}

// These labels were observed in the packaged reference. The candidate's
// existing render modes provide the closest source-supported mapping:
// Plane->Flat, Bow->Arc, Ribbon->Wave, S sweep->S, and Cylinder->Cylinder.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum PosterRetry {
    #[default]
    Available,
    SawLock,
    Waiting,
    Spent,
}

struct GpuCandidate {
    launch_policy: runtime::LaunchPolicy,
    config: Config,
    state: CandidateState,
    state_path: std::path::PathBuf,
    wallpapers: Vec<WallpaperItem>,
    catalog: Vec<library::CatalogEntry>,
    filtered_indices: Vec<usize>,
    presentation_inputs: Option<gallery_presentation::PresentationInputs>,
    source_filter: SourceFilter,
    filter_rule: SmartRule,
    sort_order: SortOrder,
    colours: colours::State,
    show_excluded: bool,
    playlist_panel_open: bool,
    playlist_picker: bool,
    playlist_chooser: bool,
    playlist_query: String,
    browse_playlist_id: Option<String>,
    browse_playlist_unavailable: bool,
    playlist_session: u64,
    playlist_context: Option<playlists::Owner>,
    retiring_playlist: Option<playlists::RetiringPlaylist>,
    playlist_delete: Option<playlists::Deletion>,
    playlist_browsing_id: Option<String>,
    engine_properties_open: bool,
    engine_properties: Vec<engine_properties::EngineProperty>,
    engine_property_values: serde_json::Map<String, serde_json::Value>,
    engine_properties_project: Option<(String, std::path::PathBuf)>,
    engine_properties_diagnostic: Option<String>,
    playlist_name: String,
    panel_state: PanelState,
    theme_session: appearance_theme::ThemeSession,
    theme_workflow: theme_workflow::Workflow,
    settings_search: String,
    settings_locale: localization::Catalog,
    settings_workbench: view::settings::bridge::Session,
    controls_panel_open: bool,
    folder_menu_open: bool,
    schedule_name: String,
    schedule_time: String,
    schedule_days: u8,
    schedule_monitors: BTreeSet<String>,
    monitor_ids: Vec<String>,
    active_monitor_id: Option<String>,
    selected: Option<usize>,
    catalog_loaded: bool,
    catalog_fresh: bool,
    catalog_snapshot_issue: Option<String>,
    poster_failure: bool,
    detail_open: bool,
    status: Option<String>,
    detail_copy_generation: Arc<AtomicU64>,
    rotation_notice: Option<(std::time::Instant, candidate_state::RandomRotationState)>,
    load_diagnostic: Option<String>,
    config_load_diagnostic: Option<String>,
    save_failure: Option<String>,
    backup_warning: Option<String>,
    config_save_failure: Option<String>,
    provider_state_unknown: bool,
    provider_recoveries: Vec<ProviderRecovery>,
    apply_pending: bool,
    active_operation: Option<OperationId>,
    wallpaper_change: Option<operations::CapturedWallpaperChange>,
    startup_checked: bool,
    startup_status: Option<String>,
    startup_queue:
        std::collections::VecDeque<(String, candidate_state::local_library::ConfirmedAssignment)>,
    hide_after_apply: bool,
    apply_enabled: bool,
    exit_requested: bool,
    visible: bool,
    visibility_generation: u64,
    native_visibility: native_caption::Visibility,
    window: Option<window::Id>,
    window_focused: Option<bool>,
    viewport: [f32; 2],
    presentation_started: std::time::Instant,
    search_input_id: iced::widget::Id,
    search_tags_open: bool,
    local_tag_input: String,
    tag_drawer_item: Option<String>,
    local_tag_draft_owner: Option<local_library::TagOwner>,
    tag_editor_context: Option<local_library::TagOwner>,
    tag_edit_session: u64,
    local_tag_input_id: iced::widget::Id,
    folder_path_input: String,
    library_config_reconcile: bool,
    search_tags_sort_az: bool,
    poster_cache_dir: std::path::PathBuf,
    catalog_generation: Arc<AtomicU64>,
    catalog_scan_generation: Arc<AtomicU64>,
    catalog_watch: Option<catalog_watch::CatalogWatch>,
    cache_operation: Option<u64>,
    cache_status: String,
    posters: Arc<Vec<Poster>>,
    poster_slots: HashMap<usize, PosterSlot>,
    poster_failed_indices: HashSet<usize>,
    poster_request_key: Option<Vec<PosterRequestIdentity>>,
    poster_retry: PosterRetry,
    poster_assignment_key: Option<(Vec<(usize, u32)>, Vec<usize>)>,
    poster_generation: Arc<AtomicU64>,
    poster_worker: Option<poster_worker::PosterWorker>,
    poster_mailbox: Arc<poster_worker::PosterMailbox>,
    motion_mailbox: Arc<motion_mailbox::MotionMailbox>,
    backdrop: Option<iced_image::Handle>,
    backdrop_source: Option<std::path::PathBuf>,
    backdrop_generation: Arc<AtomicU64>,
    motion_generation: Arc<AtomicU64>,
    motion: f32,
    motion_velocity: f32,
    gallery_presentation: Option<gallery_presentation::GalleryPresentation>,
    hex_presentation: Option<hex_presentation::HexPresentation>,
    wall_presentation: Option<wall_presentation::WallPresentation>,
    scene_motion: gallery_presentation::SceneMotion,
    browse_position_restored: bool,
    gallery_engaged: bool,
    gallery_keyboard: bool,
    hex_keyboard: bool,
    poster_loading: bool,
    depth_camera: Option<GalleryMotionState>,
    depth_selection: HashMap<String, GalleryMotionState>,
    depth_hover: Option<String>,
    depth_wheel: f32,
    preset_wheel: Option<(events::PresetWheelBinding, f32)>,
    depth_revision: u64,
    depth_frame: Option<gpu_gallery::DepthSnapshot>,
    wall_colour_hold: Option<wall_colour_hold::WallColourHold>,
    depth_presentation: Option<DepthPresentation>,
    #[cfg(test)]
    deterministic_animation: bool,
    #[cfg(test)]
    view_legacy_lookups: std::sync::atomic::AtomicUsize,
    detail_generation: Arc<AtomicU64>,
    rail_generation: Arc<AtomicU64>,
    rail_reveal: f32,
    rail_dismissed: bool,
    detail_progress: f32,
    detail_source: Option<donor_detail::CardGeometry>,
    depth_detail: Option<GalleryMotionState>,
    depth_detail_id: Option<String>,
    depth_detail_mode: Option<donor_detail::DepthDetailMode>,
    wall_scroll_offset: f32,
    automation_generation: Arc<AtomicU64>,
    last_automation_minute: Option<(u8, u16)>,
    next_slideshow_unix: u64,
    automation_suspended: bool,
    gallery_mode: GalleryMode,
    atlas_entry: Option<GalleryMode>,
    startup_loading: Option<startup_loading::StartupLoading>,
    benchmark: Option<BenchmarkState>,
    scenario: Option<ScenarioState>,
    screenshot_path: Option<std::path::PathBuf>,
    screenshot_requested: bool,
    screenshot_detail: bool,
    screenshot_detail_progress: f32,
    screenshot_index: usize,
    screenshot_panel: Option<String>,
    screenshot_engine_schema: Option<std::path::PathBuf>,
    _hotkey: Option<GlobalHotkeyListener>,
    _tray: Option<tray::TrayState>,
    coordinator: WallpaperCoordinator,
}

// The Settings > Theme message family is handled by explicit arms in
// the `update` match below (after `CycleAppearanceOpacity`), each one
// owner-guarded through `theme_owner_active` and delegating to the pure
// `ThemeSession` reducer. There is deliberately no pre-dispatch
// interception and no wildcard arm: the match must stay exhaustive so
// a future Message variant cannot silently bypass the theme guards.

// The queued-event consumer is shared with deterministic tests. Native enumeration
// and playlist submission remain owned by the candidate through this small seam.
impl GpuCandidate {
    // One app-owned cancellation boundary for hide, actual mode exit and
    // matching window close. Queued callbacks become harmless in every path.
    // Runs before consumption and after every update, including early-return paths.
}

// beta.24 stage/controls adapted to 's folio and CPU image consumer.
// No donor daemon; gallery shaders remain owned by the existing renderer.
#[cfg(test)]
pub(super) mod f7_support {
    use super::*;

    pub(crate) fn gallery_layout_params(
        state: &CandidateState,
        motion: f32,
        wall_scroll_offset: f32,
    ) -> GalleryLayoutParams {
        super::gallery_layout_params(state, motion, wall_scroll_offset)
    }

    pub(crate) fn filtered_poster_aspects(
        catalog: &[library::CatalogEntry],
        filtered_indices: &[usize],
    ) -> Vec<f32> {
        super::filtered_poster_aspects(catalog, filtered_indices)
    }

    pub(crate) fn poster_indices_for_view(
        mode: GalleryMode,
        filtered_indices: &[usize],
        selected: Option<usize>,
        viewport: [f32; 2],
        layout_params: GalleryLayoutParams,
        poster_aspects: &[f32],
    ) -> Vec<usize> {
        super::poster_indices_for_view(
            mode,
            filtered_indices,
            selected,
            viewport,
            layout_params,
            poster_aspects,
        )
    }
}

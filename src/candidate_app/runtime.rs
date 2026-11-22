use super::view::{application_style, theme, view};
use super::*;
use crate::{
    appearance_theme, apply_trace, candidate_instance, candidate_scenario, config,
    diagnostic_apply, engine_properties, library, native_wallpaper, poster_worker, scanner, tray,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum LaunchMode {
    #[default]
    Normal,
    Developer,
    Preview,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct LaunchPolicy {
    mode: LaunchMode,
}

impl LaunchPolicy {
    pub(super) fn is_preview(self) -> bool {
        self.mode == LaunchMode::Preview
    }

    pub(super) fn allows_automatic_provider(self) -> bool {
        self.mode == LaunchMode::Normal
    }

    pub(super) fn allows_provider(self) -> bool {
        !self.is_preview()
    }

    pub(super) fn requests_reopen(self) -> bool {
        self.mode == LaunchMode::Normal
    }

    pub(super) fn shell_hooks_enabled(self, requested: bool) -> bool {
        !self.is_preview() && (self.mode == LaunchMode::Normal || requested)
    }

    pub(super) fn show_at_start(self, requested: bool) -> bool {
        self.mode != LaunchMode::Developer || requested
    }

    pub(super) fn apply_enabled(self, requested: bool) -> bool {
        self.allows_provider() && (self.mode == LaunchMode::Normal || requested)
    }
}

pub(super) fn parse_launch_policy(arguments: &[String]) -> Result<LaunchPolicy, String> {
    let mut preview = false;
    let mut developer = false;
    let mut theme_exit_probe = false;
    for argument in arguments.iter().skip(1) {
        let flag = argument.to_ascii_lowercase();
        if flag == "--preview" {
            preview = true;
        } else if flag == "--preview-theme-exit-probe" {
            theme_exit_probe = true;
        } else if flag == "--preview-theme-export-probe" {
            return Err("Effects export probe is retired.".into());
        } else if flag.starts_with("--preview") {
            return Err(format!("Unknown preview option: {argument}"));
        }
        developer |= flag.starts_with("--diagnostic-")
            || flag == "--scenario"
            || flag.starts_with("--scenario-")
            || flag == "--screenshot"
            || flag.starts_with("--screenshot-")
            || flag == "--benchmark"
            || flag.starts_with("--benchmark-");
    }
    if preview && developer {
        return Err("--preview cannot be combined with diagnostic, scenario, screenshot or benchmark modes.".into());
    }
    if theme_exit_probe
        && (!preview
            || !arguments
                .iter()
                .any(|arg| arg.eq_ignore_ascii_case(apply_trace::FLAG)))
    {
        return Err("Theme probes require --preview and --apply-trace.".into());
    }
    Ok(LaunchPolicy {
        mode: if preview {
            LaunchMode::Preview
        } else if developer {
            LaunchMode::Developer
        } else {
            LaunchMode::Normal
        },
    })
}

const LAUNCH_MARKER: &str = "candidate-launch-mode.toml";

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct LaunchMarker {
    version: u32,
    mode: LaunchMode,
}

pub(super) fn validate_launch_profile(
    identity: &candidate_instance::ProfileIdentity,
    policy: LaunchPolicy,
) -> Result<(), String> {
    use std::io::Read;
    let profile = identity.canonical_path();
    let marker_path = profile.join(LAUNCH_MARKER);
    let file = match std::fs::File::open(&marker_path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound && !policy.is_preview() => {
            return Ok(());
        }
        Err(error) => return Err(format!("Profile launch marker is unavailable: {error}")),
    };
    let mut bytes = Vec::new();
    file.take(1025)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read profile launch marker: {error}"))?;
    if bytes.len() > 1024 {
        return Err("Profile launch marker is too large.".into());
    }
    let text = std::str::from_utf8(&bytes)
        .map_err(|_| "Profile launch marker is not UTF-8.".to_string())?;
    let marker: LaunchMarker = toml::from_str(text)
        .map_err(|error| format!("Profile launch marker is invalid: {error}"))?;
    if marker.version != 1 || marker.mode == LaunchMode::Developer {
        return Err("Profile launch marker has an unsupported version or mode.".into());
    }
    if (marker.mode == LaunchMode::Preview) != policy.is_preview() {
        return Err(
            "Preview and normal instances cannot share or reopen each other's profiles.".into(),
        );
    }
    Ok(())
}

pub(super) fn resolve_launch_profile(
    profile: &std::path::Path,
    policy: LaunchPolicy,
) -> Result<candidate_instance::ProfileIdentity, String> {
    // Only normal/developer first use may create a directory. Preview reuse must
    // name an existing, explicitly marked owned profile. Parse policy before here.
    if policy.is_preview() {
        if !profile.is_absolute() {
            return Err("Preview profile reuse requires an absolute owned profile path.".into());
        }
    } else {
        std::fs::create_dir_all(profile)
            .map_err(|error| format!("Could not prepare profile directory: {error}"))?;
    }
    let identity = candidate_instance::ProfileIdentity::open(profile)?;
    validate_launch_profile(&identity, policy)?;
    Ok(identity)
}

pub(super) fn fresh_preview_profile(root: &std::path::Path) -> Result<std::path::PathBuf, String> {
    use std::io::Write;
    if !root.is_absolute() {
        return Err("Preview profile root must be absolute.".into());
    }
    std::fs::create_dir_all(root)
        .map_err(|error| format!("Could not prepare owned preview root: {error}"))?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    for attempt in 0..32 {
        let profile = root.join(format!("{}-{nonce}-{attempt}", std::process::id()));
        match std::fs::create_dir(&profile) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(format!("Could not create owned preview profile: {error}")),
        }
        let marker = toml::to_string(&LaunchMarker {
            version: 1,
            mode: LaunchMode::Preview,
        })
        .map_err(|error| error.to_string())?;
        let marker_path = profile.join(LAUNCH_MARKER);
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&marker_path)
            .map_err(|error| format!("Could not mark owned preview profile: {error}"))?;
        file.write_all(marker.as_bytes())
            .and_then(|()| file.sync_all())
            .map_err(|error| format!("Could not finish preview marker: {error}"))?;
        return Ok(profile);
    }
    Err("Could not allocate a unique owned preview profile.".into())
}

fn install_runtime_channels() -> UnboundedSender<Wake> {
    let tx_slot = WAKE_TX.get_or_init(|| Mutex::new(None));
    if let Some(existing) = tx_slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .as_ref()
        .cloned()
    {
        return existing;
    }
    let (tx, rx) = unbounded();
    *tx_slot
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(tx.clone());
    *WAKE_RX
        .get_or_init(|| Mutex::new(None))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(rx);
    tx
}

fn take_wake_receiver() -> Option<UnboundedReceiver<Wake>> {
    WAKE_RX.get_or_init(|| Mutex::new(None)).lock().ok()?.take()
}

fn wake_stream() -> impl iced::futures::Stream<Item = Message> {
    use iced::futures::StreamExt;

    match take_wake_receiver() {
        Some(receiver) => receiver.map(Message::Wake).boxed(),
        None => iced::futures::stream::pending().boxed(),
    }
}

pub(super) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

pub(super) fn local_schedule_time() -> (u8, u16) {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;

    let mut local: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut local) };
    (
        local.wDayOfWeek as u8,
        (local.wHour as u16 * 60 + local.wMinute as u16).min(1439),
    )
}

fn boot(
    launch_policy: LaunchPolicy,
    loaded_state: candidate_state::StateLoadOutcome,
) -> (GpuCandidate, Task<Message>) {
    let boot_started = std::time::Instant::now();
    let tx = install_runtime_channels();
    let arguments: Vec<_> = std::env::args().collect();
    let scenario_request = match candidate_scenario::parse_arguments(&arguments) {
        candidate_scenario::ScenarioInvocation::Ready(request) => Some(request),
        _ => None,
    };
    let benchmark_quick = arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case("--benchmark-quick"));
    let benchmark_enabled = benchmark_quick
        || arguments
            .iter()
            .any(|argument| argument.eq_ignore_ascii_case("--benchmark"));
    let benchmark_warm_cycles = if benchmark_quick { 2 } else { 30 };
    let benchmark_navigation_steps = if benchmark_quick { 4 } else { 120 };
    let screenshot_path = arguments
        .windows(2)
        .find(|pair| pair[0].eq_ignore_ascii_case("--screenshot"))
        .map(|pair| std::path::PathBuf::from(&pair[1]));
    let screenshot_mode = arguments
        .windows(2)
        .find(|pair| pair[0].eq_ignore_ascii_case("--screenshot-mode"))
        .map(|pair| pair[1].to_ascii_lowercase());
    let screenshot_panel = arguments
        .windows(2)
        .find(|pair| pair[0].eq_ignore_ascii_case("--screenshot-panel"))
        .map(|pair| pair[1].to_ascii_lowercase());
    let screenshot_source = arguments
        .windows(2)
        .find(|pair| pair[0].eq_ignore_ascii_case("--screenshot-source"))
        .map(|pair| pair[1].to_ascii_lowercase());
    let screenshot_detail = arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case("--screenshot-detail"));
    // Static diagnostic controls never affect normal daily presentation.
    let screenshot_detail_progress = arguments
        .windows(2)
        .find(|pair| pair[0].eq_ignore_ascii_case("--screenshot-detail-progress"))
        .and_then(|pair| pair[1].parse::<f32>().ok())
        .filter(|p| p.is_finite())
        .unwrap_or(1.0)
        .clamp(0.0, 1.0);
    let screenshot_index = arguments
        .windows(2)
        .find(|pair| pair[0].eq_ignore_ascii_case("--screenshot-index"))
        .and_then(|pair| pair[1].parse::<usize>().ok())
        .unwrap_or(0);
    let diagnostic_engine_properties =
        if screenshot_path.is_some() && screenshot_panel.as_deref() == Some("engine") {
            arguments
                .windows(2)
                .find(|p| p[0] == "--screenshot-engine-schema")
                .map(|p| engine_properties::read_supported(std::path::Path::new(&p[1])))
        } else {
            None
        };
    let diagnostic_settings_query = if screenshot_path.is_some() {
        arguments
            .windows(2)
            .find(|p| p[0] == "--screenshot-settings-query")
            .map(|p| p[1].clone())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let diagnostic_arg = |flag: &str| {
        if screenshot_path.is_some() {
            arguments
                .windows(2)
                .find(|p| p[0] == flag)
                .map(|p| p[1].clone())
                .unwrap_or_default()
        } else {
            String::new()
        }
    };
    let diagnostic_search_rule = SmartRule {
        text: diagnostic_arg("--screenshot-query"),
        tags: diagnostic_arg("--screenshot-tags")
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_lowercase)
            .collect(),
        excluded_tags: diagnostic_arg("--screenshot-excluded-tags")
            .split(',')
            .map(str::trim)
            .filter(|tag| !tag.is_empty())
            .map(str::to_lowercase)
            .collect(),
        ..Default::default()
    };
    let candidate_runtime = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .is_some_and(|stem| stem.contains("gpu-candidate"));
    let shell_hooks = launch_policy.shell_hooks_enabled(
        !candidate_runtime
            || arguments
                .iter()
                .any(|argument| argument.eq_ignore_ascii_case("--shell-hooks")),
    );
    let loaded_config = if candidate_runtime {
        Config::load_read_only_with_diagnostics()
    } else {
        Config::load_with_diagnostics()
    };
    let config_load_diagnostic = loaded_config.diagnostic.clone().or_else(|| {
        matches!(
            loaded_config.source,
            config::ConfigLoadSource::FailureDefaults
        )
        .then(|| "Configuration fell back to defaults after a load failure.".to_string())
    });
    let config = loaded_config.config;
    let state_path = Config::config_dir().join(if candidate_runtime {
        "candidate-state.toml"
    } else {
        "state.toml"
    });
    let poster_cache_dir = Config::cache_dir().join("posters");
    // Diagnostic catalog setup retains its existing fresh-scan semantics.
    let snapshot_path =
        (!benchmark_enabled && screenshot_path.is_none() && scenario_request.is_none())
            .then(|| state_path.with_file_name("catalog-snapshot-v1.json"));
    let cached_catalog = snapshot_path.as_ref().and_then(|path| {
        match catalog_snapshot::load(
            path,
            &config.catalog.local_folders,
            config.behavior.include_default_projects,
        ) {
            Ok(snapshot) => Some(snapshot),
            Err(error) => {
                apply_trace::event_lazy("catalog-snapshot-load", || {
                    format!("accepted=false reason={error}")
                });
                None
            }
        }
    });
    let load_diagnostic = loaded_state.diagnostic.clone();
    let mut state = loaded_state.state;
    if screenshot_path.is_some() {
        state.gallery.mode = match screenshot_mode.as_deref() {
            Some("geometric") => GalleryKind::Geometric,
            Some("wall") => GalleryKind::Wall,
            Some("depth") => GalleryKind::Depth,
            _ => GalleryKind::Slices,
        };
        if arguments
            .iter()
            .any(|arg| arg.eq_ignore_ascii_case("--screenshot-custom-gallery"))
        {
            // Read explicit geometry from the isolated diagnostic profile.
            state.gallery.variant = None;
        }
    }
    if benchmark_enabled {
        state.slideshow_enabled = false;
        state.automation_intent = Some(Default::default());
        for schedule in &mut state.schedules {
            schedule.enabled = false;
        }
    }
    let next_slideshow_unix = unix_now().saturating_add(state.slideshow_interval_seconds);
    let show_excluded = config.behavior.show_excluded;
    let native_monitors = native_wallpaper::monitors().unwrap_or_default();
    let monitor_ids: Vec<_> = native_monitors
        .iter()
        .map(|monitor| monitor.device_id.clone())
        .collect();
    let active_monitor_id = config
        .behavior
        .follow_cursor
        .then(|| cursor_monitor_id(&native_monitors))
        .flatten()
        .or_else(|| {
            state
                .frontend_preferences
                .picker
                .desired_monitor_id
                .as_ref()
                .and_then(|desired| {
                    monitor_ids
                        .iter()
                        .find(|id| id.eq_ignore_ascii_case(desired))
                        .cloned()
                })
        })
        .or_else(|| monitor_ids.first().cloned());
    let active_monitor_origin = active_monitor_id.as_ref().and_then(|active| {
        native_monitors
            .iter()
            .find(|monitor| monitor.device_id.eq_ignore_ascii_case(active))
            .map(|monitor| [monitor.rect.left, monitor.rect.top])
    });

    let hotkey = shell_hooks.then(|| {
        let hotkey_tx = tx.clone();
        GlobalHotkeyListener::spawn(config.keybinds.toggle_gallery.clone(), move || {
            let _ = hotkey_tx.unbounded_send(Wake::Toggle);
        })
    });

    let tray = if shell_hooks {
        let tray_tx = tx.clone();
        tray::set_event_handler(move |action| {
            let _ = tray_tx.unbounded_send(Wake::Tray(action));
        });
        let result = if candidate_runtime {
            tray::create_gpu_candidate(state.automation_intent().random_enabled)
        } else {
            tray::create(state.automation_intent().random_enabled)
        };
        result.map_err(|error| log_debug(&error)).ok()
    } else {
        if launch_policy.is_preview() {
            log_debug("Preview does not install global tray or hotkey hooks");
        } else {
            log_debug("GPU candidate shell hooks are disabled; pass --shell-hooks to opt in");
        }
        None
    };

    let coordinator_tx = tx.clone();
    let coordinator = WallpaperCoordinator::spawn(move |result| {
        let _ = coordinator_tx.unbounded_send(Wake::CoordinatorFinished(result));
    });

    let catalog_generation = Arc::new(AtomicU64::new(1));
    let catalog_scan_generation = Arc::new(AtomicU64::new(1));
    let catalog_tx = tx.clone();
    let scan_threads = scanner::configured_catalog_scan_threads(config.catalog_scan_threads);
    log_debug(&format!(
        "Catalog scan policy: io_threads={scan_threads} configured={:?}",
        config.catalog_scan_threads
    ));
    let catalog_watch = catalog_watch::CatalogWatch::spawn(
        scan_threads,
        config.catalog.local_folders.clone(),
        config.behavior.include_default_projects,
        state.frontend_preferences.library.clone(),
        catalog_scan_generation.clone(),
        move |wake| catalog_tx.unbounded_send(wake).is_ok(),
    )
    .expect("GPU candidate catalog owner");
    if let Some(path) = snapshot_path {
        catalog_watch.enable_snapshots(path);
    }

    let visible = launch_policy.show_at_start(
        benchmark_enabled
            || screenshot_path.is_some()
            || arguments
                .iter()
                .any(|argument| argument.eq_ignore_ascii_case("--show")),
    );
    let apply_enabled = launch_policy.apply_enabled(
        arguments
            .iter()
            .any(|argument| argument.eq_ignore_ascii_case("--allow-apply")),
    );
    log_debug(&format!(
        "Launch policy: mode={:?} visible={visible} apply_enabled={apply_enabled} shell_hooks={shell_hooks}",
        launch_policy.mode
    ));
    let (panel_state, controls_panel_open) = initial_panels(screenshot_panel.as_deref());
    let screenshot_source_filter =
        initial_screenshot_source_filter(screenshot_path.is_some(), screenshot_source.as_deref());
    let (diagnostic_properties, diagnostic_property_values) = diagnostic_engine_properties
        .as_ref()
        .map(engine_property_defaults)
        .unwrap_or_default();
    let (window, open) = if visible {
        let (id, open) = window::open(window_settings(active_monitor_origin));
        (Some(id), open.discard())
    } else {
        (None, Task::none())
    };
    let settings_locale = localization::Catalog::new(
        state
            .frontend_preferences
            .interface
            .language
            .as_deref()
            .unwrap_or("auto"),
        localization::automatic_tag(),
    )
    .expect("validated saved interface language");
    let settings_workbench = view::settings::bridge::Session::from_state(&state);
    let mut theme_workflow = theme_workflow::Workflow::default();
    theme_workflow.probe_armed = launch_policy.is_preview()
        && arguments
            .iter()
            .any(|arg| arg.eq_ignore_ascii_case("--preview-theme-exit-probe"));
    let mut app = GpuCandidate {
        launch_policy,
        config,
        gallery_mode: gallery_mode(state.gallery.mode),
        atlas_entry: None,
        state,
        state_path,
        wallpapers: Vec::new(),
        catalog: Vec::new(),
        filtered_indices: Vec::new(),
        presentation_inputs: None,
        source_filter: screenshot_source_filter,
        filter_rule: diagnostic_search_rule,
        sort_order: SortOrder::Colour,
        colours: colours::State::default(),
        show_excluded,
        playlist_browsing_id: None,
        playlist_chooser: false,
        playlist_query: String::new(),
        browse_playlist_id: None,
        browse_playlist_unavailable: false,
        playlist_panel_open: screenshot_path.is_some()
            && screenshot_panel.as_deref() == Some("playlist"),
        engine_properties_open: screenshot_path.is_some()
            && screenshot_panel.as_deref() == Some("engine"),
        engine_property_values: diagnostic_property_values,
        engine_properties: diagnostic_properties,
        engine_properties_project: None,
        engine_properties_diagnostic: diagnostic_engine_properties
            .as_ref()
            .and_then(engine_property_schema_status_message),
        playlist_name: String::new(),
        panel_state,
        theme_session: appearance_theme::ThemeSession::new(),
        theme_workflow,
        settings_search: diagnostic_settings_query,
        settings_locale,
        settings_workbench,
        controls_panel_open,
        folder_menu_open: screenshot_path.is_some()
            && screenshot_panel.as_deref() == Some("folders"),
        schedule_name: String::new(),
        schedule_time: "08:00".into(),
        schedule_days: 0x7f,
        schedule_monitors: BTreeSet::new(),
        monitor_ids,
        active_monitor_id,
        selected: None,
        catalog_loaded: false,
        catalog_fresh: false,
        catalog_snapshot_issue: None,
        poster_failure: false,
        detail_open: false,
        status: load_diagnostic
            .clone()
            .or_else(|| config_load_diagnostic.clone())
            .or_else(|| {
                diagnostic_engine_properties
                    .as_ref()
                    .and_then(engine_property_schema_status_message)
            })
            .or_else(|| Some("Loading wallpapers…".into())),
        load_diagnostic,
        config_load_diagnostic,
        save_failure: None,
        backup_warning: None,
        config_save_failure: None,
        provider_state_unknown: false,
        provider_recoveries: Vec::new(),
        apply_pending: false,
        active_operation: None,
        wallpaper_change: None,
        startup_checked: false,
        startup_status: None,
        rotation_notice: None,
        detail_copy_generation: Arc::new(AtomicU64::new(0)),
        startup_queue: Default::default(),
        hide_after_apply: false,
        apply_enabled,
        exit_requested: false,
        visible,
        visibility_generation: 0,
        native_visibility: Default::default(),
        window,
        window_focused: None,
        viewport: [1920.0, 1080.0],
        presentation_started: boot_started,
        search_input_id: iced::widget::Id::unique(),
        local_tag_input: String::new(),
        tag_drawer_item: None,
        local_tag_draft_owner: None,
        playlist_picker: false,
        playlist_session: 0,
        playlist_context: None,
        retiring_playlist: None,
        playlist_delete: None,
        tag_editor_context: None,
        tag_edit_session: 0,
        local_tag_input_id: iced::widget::Id::unique(),
        folder_path_input: String::new(),
        library_config_reconcile: false,
        search_tags_open: screenshot_path.is_some()
            && arguments.iter().any(|arg| arg == "--screenshot-tags-open"),
        search_tags_sort_az: false,
        poster_cache_dir,
        catalog_generation,
        catalog_scan_generation,
        catalog_watch: Some(catalog_watch),
        cache_operation: None,
        cache_status: String::new(),
        posters: Arc::new(Vec::new()),
        poster_slots: HashMap::new(),
        poster_failed_indices: Default::default(),
        poster_retry: Default::default(),
        poster_request_key: None,
        poster_assignment_key: None,
        poster_generation: Arc::new(AtomicU64::new(0)),
        poster_worker: None,
        poster_mailbox: Arc::new(poster_worker::PosterMailbox::default()),
        motion_mailbox: Arc::new(motion_mailbox::MotionMailbox::default()),
        backdrop: None,
        backdrop_source: None,
        backdrop_generation: Arc::new(AtomicU64::new(0)),
        motion_generation: Arc::new(AtomicU64::new(0)),
        motion: 0.0,
        motion_velocity: 0.0,
        gallery_presentation: None,
        hex_presentation: None,
        wall_presentation: None,
        scene_motion: gallery_presentation::SceneMotion::default(),
        browse_position_restored: false,
        gallery_engaged: false,
        gallery_keyboard: false,
        hex_keyboard: true,
        poster_loading: false,
        depth_camera: None,
        depth_selection: HashMap::new(),
        depth_hover: None,
        depth_wheel: 0.0,
        preset_wheel: None,
        depth_revision: 0,
        depth_frame: None,
        wall_colour_hold: None,
        depth_presentation: None,
        #[cfg(test)]
        deterministic_animation: false,
        #[cfg(test)]
        view_legacy_lookups: Default::default(),
        detail_generation: Arc::new(AtomicU64::new(0)),
        rail_generation: Arc::new(AtomicU64::new(0)),
        rail_dismissed: false,
        rail_reveal: if screenshot_path.is_some() { 1.0 } else { 0.0 },
        detail_progress: 0.0,
        detail_source: None,
        depth_detail: None,
        depth_detail_id: None,
        depth_detail_mode: None,
        wall_scroll_offset: 0.0,
        automation_generation: Arc::new(AtomicU64::new(0)),
        last_automation_minute: None,
        next_slideshow_unix,
        automation_suspended: false,
        benchmark: benchmark_enabled.then(|| BenchmarkState {
            started: boot_started,
            cold_open_ms: None,
            source_catalog_items: 0,
            warm_cycles_requested: benchmark_warm_cycles,
            animation_seconds_requested: benchmark_navigation_steps as u64 / 2,
            last_motion_frame: None,
            frame_intervals_ms: Vec::new(),
            open_requested: None,
            warm_open_ms: Vec::new(),
            report_path: std::env::current_dir()
                .unwrap_or_default()
                .join("target")
                .join("performance")
                .join("gpu-candidate-benchmark.json"),
        }),
        scenario: scenario_request.map(|request| ScenarioState {
            report: candidate_scenario::ScenarioReport::for_request(&request),
            request,
            item_index: None,
            next_step: 0,
            ready: false,
            started: std::time::Instant::now(),
            window_settlement: None,
            next_window_settlement_generation: 0,
        }),
        screenshot_path,
        screenshot_requested: false,
        screenshot_detail,
        screenshot_detail_progress,
        screenshot_index,
        screenshot_panel,
        screenshot_engine_schema: arguments
            .windows(2)
            .find(|p| p[0] == "--screenshot-engine-schema")
            .map(|p| std::path::PathBuf::from(&p[1])),
        _hotkey: hotkey,
        _tray: tray,
        coordinator,
    };
    if let Some(snapshot) = cached_catalog {
        app.install_cached_catalog(snapshot);
    }
    if app.visible && app.gallery_mode == GalleryMode::Depth {
        app.restart_gallery_entrance();
    }
    app.refresh_automation_timer();
    if benchmark_enabled {
        let benchmark_tx = tx.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("gpu-candidate-benchmark".into())
            .spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(2));
                for _ in 0..benchmark_warm_cycles {
                    if benchmark_tx
                        .unbounded_send(Wake::BenchmarkVisibility(false))
                        .is_err()
                    {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(150));
                    if benchmark_tx
                        .unbounded_send(Wake::BenchmarkVisibility(true))
                        .is_err()
                    {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(850));
                }
                for step in 0..benchmark_navigation_steps {
                    if benchmark_tx
                        .unbounded_send(Wake::BenchmarkNavigate(step))
                        .is_err()
                    {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(500));
                }
                let _ = benchmark_tx.unbounded_send(Wake::BenchmarkFinished);
            })
        {
            log_debug(&format!("Could not start GPU candidate benchmark: {error}"));
        }
    }
    if let Some(scenario) = &app.scenario {
        let scenario_tx = tx.clone();
        let timeout = std::time::Duration::from_millis(scenario.request.timeout_ms);
        if let Err(error) = std::thread::Builder::new()
            .name("gpu-candidate-scenario-timeout".into())
            .spawn(move || {
                std::thread::sleep(timeout);
                let _ = scenario_tx.unbounded_send(Wake::ScenarioTimeout);
            })
        {
            log_debug(&format!(
                "Could not start GPU candidate scenario timeout: {error}"
            ));
        }
    }
    view::settings::controller::reconcile(&mut app);
    (app, open)
}

fn subscription(_app: &GpuCandidate) -> Subscription<Message> {
    Subscription::batch([Subscription::run(wake_stream), event::listen_with(on_event)])
}

fn on_event(event: Event, status: event::Status, id: window::Id) -> Option<Message> {
    match event {
        Event::Window(window::Event::Opened { size, .. }) => Some(Message::WindowOpened {
            id,
            width: size.width,
            height: size.height,
        }),
        Event::Window(window::Event::Resized(size)) => Some(Message::WindowResized {
            id,
            width: size.width,
            height: size.height,
        }),
        Event::Window(window::Event::Closed) => Some(Message::WindowClosed(id)),
        Event::Window(window::Event::CloseRequested) => Some(Message::CloseRequested(id)),
        Event::Window(window::Event::Focused) => Some(Message::WindowFocused(id)),
        Event::Window(window::Event::Unfocused) => Some(Message::WindowUnfocused(id)),
        Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => {
            Some(Message::SettingsModifiers {
                window: id,
                modifiers,
            })
        }
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            physical_key,
            location,
            modifiers,
            repeat,
            ..
        }) if should_handle_key_event(status, &key, modifiers) => Some(Message::KeyPressed {
            key,
            modifiers,
            event: NavigationKeyEvent {
                event_window: id,
                physical_key,
                location,
                repeat,
            },
        }),
        Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. })
            if status == event::Status::Captured =>
        {
            Some(Message::SettingsCapturedKey {
                window: id,
                key,
                modifiers,
            })
        }
        _ => None,
    }
}

pub(super) fn window_settings(origin: Option<[i32; 2]>) -> window::Settings {
    window::Settings {
        fullscreen: true,
        decorations: false,
        transparent: true,
        level: window::Level::AlwaysOnTop,
        exit_on_close_request: false,
        visible: false,
        position: origin.map_or(window::Position::Default, |[x, y]| {
            window::Position::Specific(iced::Point::new(x as f32, y as f32))
        }),
        platform_specific: window::settings::PlatformSpecific {
            skip_taskbar: true,
            ..Default::default()
        },
        ..Default::default()
    }
}

fn candidate_runtime_directory() -> std::path::PathBuf {
    std::env::var_os("WALLPAPER_SWITCHER_CONFIG_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| std::path::PathBuf::from("."))
                .join(".config")
                .join("wallpaper-switcher-gpu-candidate")
        })
}

fn configure_candidate_runtime(candidate: &std::path::Path) {
    if std::env::var_os("WALLPAPER_SWITCHER_CONFIG_DIR").is_none() {
        let original = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".config")
            .join("wallpaper-switcher")
            .join("config.toml");
        let candidate_config = candidate.join("config.toml");
        if !candidate_config.exists() && original.exists() {
            let _ = std::fs::copy(original, &candidate_config);
        }
    }
    // Every consumer uses the same resolved directory, including explicit aliases.
    std::env::set_var("WALLPAPER_SWITCHER_CONFIG_DIR", candidate);
}

fn load_startup_state() -> candidate_state::StateLoadOutcome {
    let candidate_runtime = std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .is_some_and(|stem| stem.contains("gpu-candidate"));
    CandidateState::load_with_diagnostics(&Config::config_dir().join(if candidate_runtime {
        "candidate-state.toml"
    } else {
        "state.toml"
    }))
}

pub(crate) fn run() -> iced::Result {
    let arguments = std::env::args().collect::<Vec<_>>();
    // This boundary precedes even diagnostic dispatch and profile/guard side effects.
    let launch_policy = match parse_launch_policy(&arguments) {
        Ok(policy) => policy,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let diagnostic = diagnostic_apply::parse_arguments(&arguments);
    if let diagnostic_apply::DiagnosticInvocation::Invalid(output) = &diagnostic {
        output.emit_json();
        return Ok(());
    }
    let scenario = candidate_scenario::parse_arguments(&arguments);
    if let candidate_scenario::ScenarioInvocation::Invalid(report) = &scenario {
        report.emit_json();
        return Ok(());
    }
    if let candidate_scenario::ScenarioInvocation::Ready(request) = &scenario {
        if let Err(error) = candidate_scenario::validate_isolated_environment() {
            let mut report = candidate_scenario::ScenarioReport::for_request(request);
            report.finish_failure(error);
            report.emit_json();
            return Ok(());
        }
    }

    let profile_path = if launch_policy.is_preview()
        && std::env::var_os("WALLPAPER_SWITCHER_CONFIG_DIR").is_none()
    {
        fresh_preview_profile(&std::env::temp_dir().join("wallpaper-switcher-preview"))
    } else {
        Ok(candidate_runtime_directory())
    };
    let identity = match profile_path.and_then(|path| resolve_launch_profile(&path, launch_policy))
    {
        Ok(profile) => profile,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    let request_reopen = launch_policy.requests_reopen()
        && !matches!(
            &diagnostic,
            diagnostic_apply::DiagnosticInvocation::Ready(_)
        )
        && !matches!(&scenario, candidate_scenario::ScenarioInvocation::Ready(_));
    let mut instance_guard = match candidate_instance::CandidateInstanceGuard::acquire(
        identity,
        request_reopen,
    ) {
        Ok(guard) => guard,
        Err(error) => {
            if matches!(
                error,
                candidate_instance::CandidateInstanceError::AlreadyRunning
            ) {
                if let diagnostic_apply::DiagnosticInvocation::Ready(request) = &diagnostic {
                    diagnostic_apply::DiagnosticApplyOutput::before_submission(
                        request,
                        "another candidate instance already owns this profile".to_string(),
                    )
                    .emit_json();
                    return Ok(());
                }
                if let candidate_scenario::ScenarioInvocation::Ready(request) = &scenario {
                    let mut report = candidate_scenario::ScenarioReport::for_request(request);
                    report.finish_failure(
                        "another candidate instance already owns this profile; no scenario steps ran",
                    );
                    report.emit_json();
                    return Ok(());
                }
                // Only normal launches signal a resident; preview/developer duplicates
                // report ownership without reopening or changing its launch policy.
                std::process::exit(73);
            }
            if let diagnostic_apply::DiagnosticInvocation::Ready(request) = &diagnostic {
                diagnostic_apply::DiagnosticApplyOutput::before_submission(
                    request,
                    error.to_string(),
                )
                .emit_json();
                return Ok(());
            }
            candidate_instance::show_rejection_diagnostic(&error);
            return Ok(());
        }
    };
    let candidate_profile = instance_guard.profile().canonical_path();

    if launch_policy.is_preview() {
        std::env::set_var("WALLPAPER_SWITCHER_CONFIG_DIR", &candidate_profile);
        eprintln!("Owned preview profile: {}", candidate_profile.display());
    } else {
        configure_candidate_runtime(&candidate_profile);
    }
    let trace_enabled = arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case(apply_trace::FLAG));
    apply_trace::set_enabled(trace_enabled);
    log_debug(&format!(
        "Physical profile identity: {:?}",
        instance_guard.profile()
    ));
    if let diagnostic_apply::DiagnosticInvocation::Ready(request) = diagnostic {
        diagnostic_apply::run(request).emit_json();
        return Ok(());
    }

    let scenario_active = matches!(scenario, candidate_scenario::ScenarioInvocation::Ready(_));
    let screenshot_active = arguments
        .iter()
        .any(|argument| argument.eq_ignore_ascii_case("--screenshot"));
    if !launch_policy.is_preview() && !scenario_active && !screenshot_active {
        let wake_tx = install_runtime_channels();
        match instance_guard.start_reopen_listener(move || {
            let _ = wake_tx.unbounded_send(Wake::Tray(TrayAction::Open));
        }) {
            Ok(()) => {}
            Err(error) => {
                log_debug(&format!(
                    "Resident reopen listener could not start: {error}"
                ));
            }
        }
    }

    let startup_state = load_startup_state();
    let startup_locale = localization::Catalog::new(
        startup_state
            .state
            .frontend_preferences
            .interface
            .language
            .as_deref()
            .unwrap_or("auto"),
        localization::automatic_tag(),
    )
    .expect("validated saved startup language");
    iced::daemon(
        move || boot(launch_policy, startup_state.clone()),
        update,
        view,
    )
    .title("Wallpaper Switcher GPU Candidate")
    .theme(theme)
    .style(application_style)
    .subscription(subscription)
    .default_font(UI_FONT)
    .font(startup_locale.font_bytes())
    .font(include_bytes!("../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice())
    .antialiasing(true)
    .run()
}

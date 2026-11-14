use super::*;
use crate::{apply_trace, coordinator, donor_rail, native_wallpaper};

/// The saved local slots and Config fallback have one projection for UI and execution.
pub(super) fn effective_input_bindings(
    app: &GpuCandidate,
) -> view::settings::composed::input::InputMap {
    use view::settings::{
        composed::{
            self,
            draft::Value,
            input::{parse_binding, InputAction as A, InputMap},
        },
        preference_projection,
    };
    let config = &app.config.keybinds;
    let mut map = InputMap::from_bindings(A::ALL.into_iter().map(|action| {
        let raw = if !action.local_available() {
            "none"
        } else {
            match action {
                A::Apply => &config.apply_wallpaper,
                A::NavLeft => &config.left,
                A::NavRight => &config.right,
                A::PageBack => &config.page_left,
                A::PageForward => &config.page_right,
                A::JumpFirst => &config.first,
                A::JumpLast => &config.last,
                _ => action.default_binding(),
            }
        };
        (action, raw.to_string())
    }));
    map.remember_fallbacks();
    map.override_bindings(
        composed::KEY_BINDINGS
            .into_iter()
            .filter(|entry| entry.action.local_available())
            .filter_map(|entry| {
                let local = app
                    .settings_workbench
                    .draft
                    .values
                    .get(entry.path)
                    .cloned()
                    .or_else(|| {
                        preference_projection::read(&app.state.frontend_preferences, entry.path)
                    });
                match local {
                    Some(Value::Text(raw)) => Some((
                        entry.action,
                        if raw.is_empty() {
                            entry.action.default_binding().into()
                        } else {
                            raw
                        },
                    )),
                    _ => None,
                }
            }),
    );
    // Preserve the existing Config-backed local action order before rail/source defaults.
    map.prioritize(&[
        A::Apply,
        A::NavLeft,
        A::NavRight,
        A::PageBack,
        A::PageForward,
        A::JumpFirst,
        A::JumpLast,
    ]);
    for (label, raw) in [
        ("Config: close", config.close.as_str()),
        ("Config: apply to all", &config.apply_to_all),
        ("Config: monitor", &config.switch_monitor),
        ("Config: exclude", &config.toggle_exclude),
        ("Config: hidden", &config.toggle_hidden),
        ("Config: Explorer", &config.open_in_explorer),
        ("Config: reload", &config.reload_config),
        ("Config: global gallery toggle", &config.toggle_gallery),
        ("Literal Escape", "escape"),
        ("Gallery search", "ctrl+f"),
    ] {
        match parse_binding(raw) {
            Some(keys) => map.reserved.push((label.into(), keys)),
            None => map
                .native_diagnostics
                .push(format!("Invalid {label} binding: {raw:?}")),
        }
    }
    map
}

pub(super) fn input_modifiers(
    modifiers: keyboard::Modifiers,
) -> view::settings::composed::input::Mods {
    view::settings::composed::input::Mods {
        ctrl: modifiers.control(),
        alt: modifiers.alt(),
        shift: modifiers.shift(),
        logo: modifiers.logo(),
    }
}

pub(super) fn input_key(key: &keyboard::Key) -> Option<view::settings::composed::input::KeyId> {
    use keyboard::{key::Named, Key};
    use view::settings::composed::input::KeyId;
    match key {
        Key::Character(text) => Some(KeyId::Char(text.to_ascii_lowercase())),
        Key::Named(named) => match named {
            Named::ArrowLeft => Some(KeyId::Left),
            Named::ArrowRight => Some(KeyId::Right),
            Named::ArrowUp => Some(KeyId::Up),
            Named::ArrowDown => Some(KeyId::Down),
            Named::Enter => Some(KeyId::Enter),
            Named::Tab => Some(KeyId::Tab),
            Named::Space => Some(KeyId::Char(" ".into())),
            Named::Home => Some(KeyId::Home),
            Named::End => Some(KeyId::End),
            Named::PageUp => Some(KeyId::PageUp),
            Named::PageDown => Some(KeyId::PageDown),
            Named::Escape => Some(KeyId::Escape),
            Named::Delete => Some(KeyId::Delete),
            _ => {
                let name = format!("{named:?}");
                name.strip_prefix('F')
                    .and_then(|n| n.parse::<u8>().ok())
                    .filter(|n| (1..=12).contains(n))
                    .map(KeyId::Function)
            }
        },
        _ => None,
    }
}

/// Capture the current view lifetime and stable item source before publication.
pub(super) fn bind_gallery_input(app: &GpuCandidate, message: GalleryMessage) -> Message {
    let target_source = match &message {
        GalleryMessage::Pointer {
            index: Some(index),
            id: Some(id),
            ..
        } => app
            .wallpapers
            .get(*index)
            .filter(|w| &w.id == id)
            .map(gallery::poster_source_for_item),
        _ => None,
    };
    Message::GalleryInput {
        window: app.window,
        visibility_generation: app.visibility_generation,
        motion_generation: app.motion_generation.load(Ordering::Acquire),
        catalog_generation: app.catalog_generation.load(Ordering::Acquire),
        mode: app.gallery_mode,
        selected: app
            .selected
            .and_then(|i| app.wallpapers.get(i))
            .map(|w| w.id.clone()),
        settings_generation: app.settings_workbench.generation,
        viewport: app.viewport,
        target_source,
        message,
    }
}

#[derive(Debug, Clone)]
pub(super) struct NavigationKeyEvent {
    pub(super) event_window: window::Id,
    pub(super) physical_key: keyboard::key::Physical,
    pub(super) location: keyboard::Location,
    pub(super) repeat: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SelectionNavigation {
    Relative(isize),
    Edge { last: bool },
}

impl SelectionNavigation {
    fn trace_action(self) -> &'static str {
        match self {
            Self::Relative(-1) => "previous",
            Self::Relative(1) => "next",
            Self::Relative(-5) => "page-previous",
            Self::Relative(5) => "page-next",
            Self::Edge { last: false } => "first",
            Self::Edge { last: true } => "last",
            Self::Relative(_) => "relative",
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct PanelState {
    pub(super) search_open: bool,
    pub(super) settings_open: bool,
    pub(super) settings_category: SettingsCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PanelEffect {
    FocusSearchInput,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EscapeAction {
    CloseSearch,
    CloseSettings,
    CloseDetail,
    HideGallery,
}

impl PanelState {
    pub(super) fn open_search(&mut self) -> PanelEffect {
        self.search_open = true;
        self.settings_open = false;
        PanelEffect::FocusSearchInput
    }

    pub(super) fn open_settings(&mut self, category: SettingsCategory) {
        self.search_open = false;
        self.settings_open = true;
        self.settings_category = category;
    }

    pub(super) fn toggle_settings(&mut self) -> bool {
        self.settings_open = !self.settings_open;
        if self.settings_open {
            self.search_open = false;
        }
        self.settings_open
    }

    pub(super) fn close_search_panel(&mut self) {
        if self.settings_open && self.settings_category == SettingsCategory::CatalogFilters {
            self.settings_open = false;
        } else {
            self.search_open = false;
        }
    }

    pub(super) fn escape(&mut self, detail_open: bool) -> EscapeAction {
        if self.search_open {
            self.search_open = false;
            EscapeAction::CloseSearch
        } else if self.settings_open {
            self.settings_open = false;
            EscapeAction::CloseSettings
        } else if detail_open {
            EscapeAction::CloseDetail
        } else {
            EscapeAction::HideGallery
        }
    }
}

pub(super) fn should_handle_key_event(
    status: event::Status,
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
) -> bool {
    status == event::Status::Ignored
        || matches_candidate_key(key, modifiers, "ctrl+f")
        || matches_candidate_key(key, modifiers, "escape")
}

pub(super) fn exit_action_for_ui(
    apply_pending: bool,
    provider_unknown: bool,
    pending: &[ProviderRecovery],
    owner_action: coordinator::ShutdownAction,
) -> coordinator::ShutdownAction {
    if apply_pending {
        return coordinator::ShutdownAction::WaitForActive;
    }
    if matches!(owner_action, coordinator::ShutdownAction::Ready)
        && (provider_unknown || !pending.is_empty())
    {
        return coordinator::ShutdownAction::Blocked(
            "UI still has unresolved ownership; no matching owner completion has been received"
                .into(),
        );
    }
    owner_action
}

pub(super) fn gallery_close_action(detail_open: bool) -> EscapeAction {
    if detail_open {
        EscapeAction::CloseDetail
    } else {
        EscapeAction::HideGallery
    }
}

pub(super) fn visibility_after_tray_open(_was_visible: bool) -> bool {
    true
}

pub(super) fn initial_panels(screenshot_panel: Option<&str>) -> (PanelState, bool) {
    let mut panels = PanelState::default();
    let shape_panel_open = match screenshot_panel.map(str::to_ascii_lowercase).as_deref() {
        Some("playlist" | "engine" | "folders") => false,
        Some("search") => {
            panels.search_open = true;
            false
        }
        Some("settings") => {
            panels.settings_open = true;
            false
        }
        Some("appearance" | "theme") => {
            panels.settings_open = true;
            panels.settings_category = SettingsCategory::Theme;
            false
        }
        Some("automation") => {
            panels.settings_open = true;
            panels.settings_category = SettingsCategory::Automation;
            false
        }
        Some("library") => {
            panels.settings_open = true;
            panels.settings_category = SettingsCategory::Library;
            false
        }
        Some("picker") => {
            panels.settings_open = true;
            panels.settings_category = SettingsCategory::Picker;
            false
        }
        Some("position") => {
            panels.settings_open = true;
            panels.settings_category = SettingsCategory::Position;
            false
        }
        Some("motion") => {
            panels.settings_open = true;
            panels.settings_category = SettingsCategory::Motion;
            false
        }
        Some("playback") => {
            panels.settings_open = true;
            panels.settings_category = SettingsCategory::Playback;
            false
        }
        Some("sources") => {
            panels.settings_open = true;
            panels.settings_category = SettingsCategory::Sources;
            false
        }
        Some("displays" | "wallpapers-displays") => {
            panels.settings_open = true;
            panels.settings_category = SettingsCategory::WallpapersDisplays;
            false
        }
        Some("filter" | "filter-search") => {
            panels.settings_open = true;
            panels.settings_category = SettingsCategory::CatalogFilters;
            false
        }
        Some(_) => true,
        None => false,
    };
    (panels, shape_panel_open)
}

pub(super) fn matches_candidate_key(
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
    binding: &str,
) -> bool {
    let mut need_control = false;
    let mut need_alt = false;
    let mut need_shift = false;
    let mut need_logo = false;
    let mut base = "";
    for part in binding.split('+').map(str::trim) {
        match part.to_ascii_lowercase().as_str() {
            "hyper" => {
                need_control = true;
                need_alt = true;
                need_shift = true;
                need_logo = true;
            }
            "ctrl" | "control" => need_control = true,
            "alt" => need_alt = true,
            "shift" => need_shift = true,
            "win" | "super" | "windows" => need_logo = true,
            _ => base = part,
        }
    }
    if modifiers.control() != need_control
        || modifiers.alt() != need_alt
        || modifiers.shift() != need_shift
        || modifiers.logo() != need_logo
    {
        return false;
    }
    let target = base.to_ascii_lowercase();
    match key {
        keyboard::Key::Character(value) => value.eq_ignore_ascii_case(&target),
        keyboard::Key::Named(named) => matches!(
            (named, target.as_str()),
            (keyboard::key::Named::Escape, "escape" | "esc")
                | (keyboard::key::Named::Enter, "return" | "enter")
                | (keyboard::key::Named::ArrowLeft, "left")
                | (keyboard::key::Named::ArrowRight, "right")
                | (keyboard::key::Named::ArrowUp, "up")
                | (keyboard::key::Named::ArrowDown, "down")
                | (keyboard::key::Named::PageUp, "pageup" | "page_up" | "pgup")
                | (
                    keyboard::key::Named::PageDown,
                    "pagedown" | "page_down" | "pgdn"
                )
                | (keyboard::key::Named::Home, "home")
                | (keyboard::key::Named::End, "end")
                | (keyboard::key::Named::Tab, "tab")
                | (keyboard::key::Named::Delete, "delete" | "del")
                | (keyboard::key::Named::F1, "f1")
                | (keyboard::key::Named::F2, "f2")
                | (keyboard::key::Named::F3, "f3")
                | (keyboard::key::Named::F4, "f4")
                | (keyboard::key::Named::F5, "f5")
                | (keyboard::key::Named::F6, "f6")
                | (keyboard::key::Named::F7, "f7")
                | (keyboard::key::Named::F8, "f8")
                | (keyboard::key::Named::F9, "f9")
                | (keyboard::key::Named::F10, "f10")
                | (keyboard::key::Named::F11, "f11")
                | (keyboard::key::Named::F12, "f12")
        ),
        keyboard::Key::Unidentified => false,
    }
}

#[cfg(test)]
pub(super) fn matched_selection_navigation(
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
    keybinds: &KeybindsConfig,
) -> Option<SelectionNavigation> {
    if matches_candidate_key(key, modifiers, &keybinds.left) {
        Some(SelectionNavigation::Relative(-1))
    } else if matches_candidate_key(key, modifiers, &keybinds.right) {
        Some(SelectionNavigation::Relative(1))
    } else if matches_candidate_key(key, modifiers, &keybinds.page_left) {
        Some(SelectionNavigation::Relative(-5))
    } else if matches_candidate_key(key, modifiers, &keybinds.page_right) {
        Some(SelectionNavigation::Relative(5))
    } else if matches_candidate_key(key, modifiers, &keybinds.first) {
        Some(SelectionNavigation::Edge { last: false })
    } else if matches_candidate_key(key, modifiers, &keybinds.last) {
        Some(SelectionNavigation::Edge { last: true })
    } else {
        None
    }
}

/// The Settings > Theme page is the active theme owner exactly while
/// Settings is open on the Theme category. Theme messages are accepted
/// only under this owner, and transient audition state is cancelled
/// when the owner goes away (see `cancel_theme_transient_if_departed`).
pub(super) fn theme_owner_active(app: &GpuCandidate) -> bool {
    app.visible
        && app.panel_state.settings_open
        && app.panel_state.settings_category == SettingsCategory::Theme
        && (!app.settings_workbench.active
            || app.settings_workbench.windows_panel == Some(SettingsCategory::Theme))
}

/// Invalidate the Theme session after a lifecycle transition if the
/// Theme owner was active before it and is not active now: transient
/// state is cleared and the session token advances unconditionally
/// (even for an already-clean session), while the committed selection
/// is preserved by the session reducer.
pub(super) fn cancel_theme_transient_if_departed(app: &mut GpuCandidate, was_theme_owner: bool) {
    if was_theme_owner && !theme_owner_active(app) {
        app.theme_session.on_departure();
    }
}

pub(super) fn sheet_owns_keyboard(
    panels: PanelState,
    playlist: bool,
    engine: bool,
    controls: bool,
    folders: bool,
) -> bool {
    panels.search_open || panels.settings_open || playlist || engine || controls || folders
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OwnedSheet {
    Search,
    Settings,
    Playlist,
    EngineProperties,
    FolderMenu,
    Controls,
}

pub(super) fn top_owned_sheet(
    panels: PanelState,
    playlist: bool,
    engine: bool,
    controls: bool,
    folders: bool,
) -> Option<OwnedSheet> {
    if engine {
        Some(OwnedSheet::EngineProperties)
    } else if playlist {
        Some(OwnedSheet::Playlist)
    } else if folders {
        Some(OwnedSheet::FolderMenu)
    } else if controls {
        Some(OwnedSheet::Controls)
    } else if panels.settings_open {
        Some(OwnedSheet::Settings)
    } else if panels.search_open {
        Some(OwnedSheet::Search)
    } else {
        None
    }
}

pub(super) fn close_owned_sheet(app: &mut GpuCandidate, sheet: OwnedSheet) {
    if sheet == OwnedSheet::Settings
        && theme_owner_active(app)
        && app.theme_session.designer_dirty()
    {
        let _ = app.request_theme_departure(super::theme_workflow::Departure::Message(Box::new(
            Message::ToggleSettingsPanel,
        )));
        return;
    }
    let was_theme_owner = theme_owner_active(app);
    match sheet {
        OwnedSheet::Search => app.panel_state.search_open = false,
        OwnedSheet::Settings => app.panel_state.settings_open = false,
        OwnedSheet::Playlist => app.playlist_panel_open = false,
        OwnedSheet::EngineProperties => app.toggle_engine_properties(),
        OwnedSheet::FolderMenu => app.folder_menu_open = false,
        OwnedSheet::Controls => app.controls_panel_open = false,
    }
    cancel_theme_transient_if_departed(app, was_theme_owner);
}

impl GpuCandidate {
    pub(super) fn request_exit(&mut self) -> Task<Message> {
        self.request_theme_exit()
    }

    fn trace_visibility_transition(
        &self,
        previous_visible: bool,
        previous_window: Option<window::Id>,
        requested_visible: bool,
        reason: &str,
    ) {
        apply_trace::event_lazy("ui-visibility-transition", || {
            format!(
                "reason={reason} requested_visible={requested_visible} previous_visible={previous_visible} visible={} changed={} visibility_generation={} previous_window_id={previous_window:?} window_id={:?} {}",
                self.visible,
                previous_visible != self.visible,
                self.visibility_generation,
                self.window,
                self.selected_item_trace_fields()
            )
        });
    }

    pub(super) fn trace_navigation_key_event(
        &self,
        key: &keyboard::Key,
        modifiers: keyboard::Modifiers,
        event: &NavigationKeyEvent,
        navigation: SelectionNavigation,
    ) {
        let key_identity = match key {
            keyboard::Key::Named(named) => format!("named:{named:?}"),
            // Do not record character content. Character bindings still have a
            // physical key identity below, which is sufficient for diagnosis.
            keyboard::Key::Character(_) => "character".to_string(),
            keyboard::Key::Unidentified => "unidentified".to_string(),
        };
        apply_trace::event_lazy("ui-navigation-key-event", || {
            format!(
                "action={} key_identity={} physical_key={:?} location={:?} repeat={} modifiers_control={} modifiers_alt={} modifiers_shift={} modifiers_logo={} event_window_id={:?} current_window_id={:?} event_window_matches_current={} visible={} focused={:?}",
                navigation.trace_action(),
                key_identity,
                event.physical_key,
                event.location,
                event.repeat,
                modifiers.control(),
                modifiers.alt(),
                modifiers.shift(),
                modifiers.logo(),
                event.event_window,
                self.window,
                self.window == Some(event.event_window),
                self.visible,
                self.window_focused,
            )
        });
    }

    /// Winit restores the saved placement while leaving fullscreen. Queue the
    /// windowed transition before the one hidden transition so that placement
    /// restoration cannot re-show a window after the hide request.
    pub(super) fn hidden_window_mode_sequence() -> [window::Mode; 2] {
        [window::Mode::Windowed, window::Mode::Hidden]
    }

    pub(super) fn hide_window_task(id: window::Id) -> Task<Message> {
        let modes = Self::hidden_window_mode_sequence();
        window::set_mode(id, modes[0]).chain(window::set_mode(id, modes[1]))
    }

    pub(super) fn start_rail_reveal(&mut self, opening: bool) {
        let generation = self.rail_generation.fetch_add(1, Ordering::AcqRel) + 1;
        if self.screenshot_path.is_some() {
            self.rail_reveal = 1.0;
            return;
        }
        let mut value = self.rail_reveal;
        let target = if opening { 1.0 } else { 0.0 };
        let clock = self.rail_generation.clone();
        let Some(tx) = WAKE_TX.get().and_then(|s| s.lock().ok()?.clone()) else {
            self.rail_reveal = target;
            return;
        };
        if let Err(error) = std::thread::Builder::new()
            .name(format!("rail-reveal-{generation}"))
            .spawn(move || {
                let mut last = std::time::Instant::now();
                loop {
                    if clock.load(Ordering::Acquire) != generation {
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(16));
                    let now = std::time::Instant::now();
                    value = donor_rail::reveal_step(value, target, (now - last).as_secs_f32());
                    last = now;
                    if tx
                        .unbounded_send(Wake::RailFrame { generation, value })
                        .is_err()
                        || value == target
                    {
                        return;
                    }
                }
            })
        {
            log_debug(&format!("Rail reveal worker unavailable: {error}"));
            self.rail_reveal = target;
        }
    }
    pub(super) fn set_visible(&mut self, visible: bool, reason: &str) -> Task<Message> {
        if !visible && self.launch_policy.is_preview() {
            apply_trace::event_lazy("ui-preview-close-request", || {
                format!(
                    "reason={reason} visible={} window={:?}",
                    self.visible, self.window
                )
            });
            return self.request_exit();
        }
        self.set_visible_owned(visible, reason)
    }

    /// Shared visibility mechanics. Ordinary preview hide remains guarded above;
    /// the explicit one-shot diagnostics probe is the sole additional caller.
    pub(super) fn set_visible_owned(&mut self, visible: bool, reason: &str) -> Task<Message> {
        if !visible {
            self.rotation_notice = None;
            self.save_wall_browse_position();
            // Retain named drafts and revoke callbacks from hidden widgets.
            self.theme_session.on_departure();
        }
        let previous_visible = self.visible;
        let previous_window = self.window;
        if visible && !previous_visible {
            self.reset_default_folder();
            self.browse_position_restored = false;
            self.gallery_engaged = false;
            self.restart_gallery_entrance();
        }
        self.start_rail_reveal(visible);
        self.visibility_generation = self.visibility_generation.wrapping_add(1);
        if self.scenario.is_some() {
            log_debug(&format!(
                "[SCENARIO-VISIBILITY] set-visible requested={} generation={} current_visible={} window_id={:?}",
                visible, self.visibility_generation, self.visible, self.window
            ));
        }
        if visible {
            let previous_monitor = self.active_monitor_id.clone();
            self.refresh_monitors(self.config.behavior.follow_cursor);
            if self.active_monitor_id != previous_monitor {
                self.selected = self.active_wallpaper_index();
                apply_trace::event_lazy("ui-item-selected", || {
                    format!(
                        "reason=active-monitor-changed {}",
                        self.selected_item_trace_fields()
                    )
                });
            }
            self.refresh_filter();
            self.refresh_backdrop();
            if let Some(id) = self.window {
                self.visible = true;
                if !previous_visible
                    && matches!(
                        self.gallery_mode,
                        GalleryMode::Slices
                            | GalleryMode::Geometric
                            | GalleryMode::Wall
                            | GalleryMode::Depth
                    )
                {
                    self.start_motion(0.0);
                }
                self.trace_visibility_transition(
                    previous_visible,
                    previous_window,
                    visible,
                    reason,
                );
                self.schedule_posters();
                let origin = self.active_monitor_id.as_ref().and_then(|active| {
                    native_wallpaper::monitors()
                        .ok()?
                        .into_iter()
                        .find_map(|monitor| {
                            monitor.device_id.eq_ignore_ascii_case(active).then_some(
                                iced::Point::new(monitor.rect.left as f32, monitor.rect.top as f32),
                            )
                        })
                });
                let mut tasks = vec![
                    window::set_mode(id, window::Mode::Fullscreen),
                    window::gain_focus(id),
                ];
                if let Some(origin) = origin {
                    tasks.push(window::move_to(id, origin));
                }
                return Task::batch(tasks);
            }

            let origin = self.active_monitor_id.as_ref().and_then(|active| {
                native_wallpaper::monitors()
                    .ok()?
                    .into_iter()
                    .find_map(|monitor| {
                        monitor
                            .device_id
                            .eq_ignore_ascii_case(active)
                            .then_some([monitor.rect.left, monitor.rect.top])
                    })
            });
            let (id, open) = window::open(window_settings(origin));
            self.window = Some(id);
            self.visible = true;
            if !previous_visible
                && matches!(
                    self.gallery_mode,
                    GalleryMode::Slices
                        | GalleryMode::Geometric
                        | GalleryMode::Wall
                        | GalleryMode::Depth
                )
            {
                self.start_motion(0.0);
            }
            self.trace_visibility_transition(previous_visible, previous_window, visible, reason);
            self.schedule_posters();
            open.discard()
        } else if let Some(id) = self.window {
            self.visible = false;
            self.stop_gallery_transients("visibility-hidden");
            self.clear_posters();
            let generation = self.visibility_generation;
            if let Some(close_tx) = WAKE_TX.get().and_then(|slot| slot.lock().ok()?.clone()) {
                let _ = std::thread::Builder::new()
                    .name(format!("gpu-hidden-cleanup-{generation}"))
                    .spawn(move || {
                        std::thread::sleep(std::time::Duration::from_secs(2));
                        let _ = close_tx.unbounded_send(Wake::CloseHiddenWindow(generation));
                    });
            }
            if self.scenario.is_some() {
                log_debug(&format!(
                    "[SCENARIO-VISIBILITY] hide-sequence window_id={id:?} modes=Windowed->Hidden generation={generation}"
                ));
            }
            self.trace_visibility_transition(previous_visible, previous_window, visible, reason);
            Self::hide_window_task(id)
        } else {
            self.visible = false;
            self.stop_gallery_transients("visibility-hidden");
            self.clear_posters();
            self.trace_visibility_transition(previous_visible, previous_window, visible, reason);
            Task::none()
        }
    }

    pub(super) fn stop_gallery_transients(&mut self, reason: &str) {
        if let Some(p) = &mut self.hex_presentation {
            p.settle();
            p.hover = None;
        }
        if let Some(p) = &mut self.gallery_presentation {
            p.settle();
            p.hover = None;
        }
        if let Some(p) = &mut self.wall_presentation {
            p.settle();
            p.hover = None;
            p.fades.clear();
        }
        self.motion_generation.fetch_add(1, Ordering::AcqRel);
        self.motion = 0.0;
        self.motion_velocity = 0.0;
        if self.depth_roll.is_some() || self.ordinary_roll.is_some() {
            self.posters = Arc::new(
                self.posters
                    .iter()
                    .filter(|p| {
                        self.poster_slots.iter().any(|(index, slot)| {
                            self.filtered_indices.contains(index) && slot.layer == p.layer
                        })
                    })
                    .cloned()
                    .collect(),
            );
            self.poster_slots
                .retain(|index, _| self.filtered_indices.contains(index));
        }
        self.depth_roll = None;
        self.ordinary_roll = None;
        self.ordinary_transition_issue = None;
        self.depth_frame = None;
        self.depth_camera = None;
        self.depth_presentation = None;
        self.depth_hover = None;
        self.depth_wheel = 0.0;
        self.depth_selection.clear();
        self.cancel_detail(reason);
    }
}

/// A drawable detail (including reversal/closing) or higher sheet owns hover.
pub(super) fn detail_owns_gallery_hover(app: &GpuCandidate) -> bool {
    app.detail_open
        || app.detail_progress > 0.0
        || app
            .depth_detail
            .is_some_and(|spring| spring.value() > 0.0 || spring.target > 0.0)
        || app.theme_workflow.prompt.is_some()
        || sheet_owns_keyboard(
            app.panel_state,
            app.playlist_panel_open,
            app.engine_properties_open,
            app.controls_panel_open,
            app.folder_menu_open,
        )
}

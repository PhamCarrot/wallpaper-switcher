//! One window and generation boundary for Settings and its platform forms.
use super::{bridge, controller, folio::intent};
use crate::candidate_app::*;

pub(in crate::candidate_app) fn folder_owns_input(app: &GpuCandidate) -> bool {
    app.folder_menu_open
        && (!app.panel_state.settings_open || app.settings_workbench.tab == "library")
}

pub(in crate::candidate_app) fn controls_owns_input(app: &GpuCandidate) -> bool {
    app.controls_panel_open && !app.panel_state.settings_open
}

pub(in crate::candidate_app) fn frame_valid(
    app: &GpuCandidate,
    window: Option<window::Id>,
    generation: u64,
) -> bool {
    app.visible
        && !app.exit_requested
        && app.settings_workbench.active
        && app.panel_state.settings_open
        && window.is_some()
        && app.window == window
        && app.settings_workbench.owner_window == window
        && app.settings_workbench.draft.is_current(generation)
}

fn bound(app: &GpuCandidate, window: Option<window::Id>, generation: u64) -> bool {
    frame_valid(app, window, generation)
        && !app.engine_properties_open
        && !app.playlist_panel_open
        && !controls_owns_input(app)
}

pub(in crate::candidate_app) fn valid(
    app: &GpuCandidate,
    window: Option<window::Id>,
    generation: u64,
) -> bool {
    bound(app, window, generation) && !folder_owns_input(app)
}
pub(in crate::candidate_app) fn page_valid(
    app: &GpuCandidate,
    window: Option<window::Id>,
    generation: u64,
    message: &Message,
) -> bool {
    if !bound(app, window, generation) {
        return false;
    }
    if app.launch_policy.is_preview()
        && matches!(message, Message::Wake(Wake::Tray(TrayAction::OpenConfig)))
    {
        return false;
    }
    if folder_owns_input(app) {
        return app.settings_workbench.tab == "library"
            && matches!(
                message,
                Message::SetFolderFilter(_) | Message::ToggleFolderMenu
            );
    }
    if app.settings_workbench.windows_panel == Some(SettingsCategory::Theme) {
        return true;
    }
    match app.settings_workbench.tab.as_str() {
        "displays" => matches!(
            message,
            Message::CycleTargetMonitor
                | Message::ToggleMonitorLock(_)
                | Message::Wake(Wake::Tray(TrayAction::OpenConfig))
                | Message::SettingsRecheckProvider { .. }
        ),
        "library" => matches!(
            message,
            Message::FolderPathChanged(_)
                | Message::AddLocalRoot
                | Message::UnlistLocalRoot(_)
                | Message::SetInstalledDiscovery(_)
                | Message::RefreshCatalog
                | Message::SearchChanged(_)
                | Message::SetSourceFilter(_)
                | Message::ToggleFolderMenu
                | Message::ToggleFavoritesFilter
                | Message::SetLibraryWatching(_)
                | Message::SetLibraryPolling(_)
                | Message::RefreshThumbnails
                | Message::ClearPosterCache
        ),
        "automation" => matches!(
            message,
            Message::ScheduleNameChanged(_)
                | Message::ScheduleTimeChanged(_)
                | Message::ToggleScheduleDay(_)
                | Message::ToggleScheduleMonitor(_)
                | Message::CreateSchedule
                | Message::ToggleSchedule(_)
        ),
        "playback" => matches!(
            message,
            Message::ToggleRandomRotation
                | Message::CycleSlideshowInterval
                | Message::TogglePlaylistPanel
                | Message::ToggleEnginePaused
                | Message::ToggleEngineMuted
                | Message::CycleEngineVolume
                | Message::StopPlaylist { .. }
                | Message::ResumeRandomRotation
        ),
        _ => false,
    }
}

pub(super) fn platform_form(app: &GpuCandidate) -> Option<Element<'_, Message>> {
    let scale = bridge::effective_ui_scale(app);
    let palette = crate::candidate_app::view::donor_palette(app);
    Some(match app.settings_workbench.tab.as_str() {
        "displays" => {
            let mut content =
                iced::widget::column![super::windows_display_content(app)].spacing(12.0 * scale);
            for recovery in app
                .provider_recoveries
                .iter()
                .filter(|recovery| coordinator::provider_recheck_available(recovery))
            {
                content = content.push(
                    iced::widget::column![
                        super::super::sheets::panel_text(
                            format!(
                                "{} · {}",
                                recovery.target_id,
                                app.settings_locale
                                    .text_static("settings-windows-provider-needs-recheck")
                            ),
                            scale,
                        ),
                        super::super::sheets::panel_button(
                            super::super::sheets::panel_text(
                                app.settings_locale
                                    .text_static("settings-windows-recheck-provider"),
                                scale
                            ),
                            palette,
                            scale,
                        )
                        .on_press_maybe(
                            (app.launch_policy.allows_provider() && !app.apply_pending).then(
                                || Message::SettingsRecheckProvider {
                                    window: app.window,
                                    generation: app.settings_workbench.generation,
                                    recovery: recovery.clone(),
                                }
                            )
                        ),
                    ]
                    .spacing(6.0 * scale),
                );
            }
            content.into()
        }
        "library" => super::library_platform_form(app),
        "playback" => super::playback_settings_content(app),
        "automation" => super::automation_settings_content_with_palette(app, palette),
        _ => return None,
    })
}

pub(in crate::candidate_app) fn view(app: &GpuCandidate) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    let session = &app.settings_workbench;
    let generation = session.generation;
    let window = app.window;
    let opacity = app.scene_motion.settings_opacity();
    let opacity = if app.panel_state.settings_open && opacity == 0.0 {
        1.0
    } else {
        opacity
    };
    let palette = crate::candidate_app::view::donor_palette(app).opacity(opacity);
    if session.windows_panel == Some(SettingsCategory::Theme) {
        let bind = move |message| Message::SettingsPageAction {
            window,
            generation,
            message: Box::new(message),
        };
        return crate::donor_folio::sheet_with_scroll_capture(
            app.viewport,
            Some(super::theme_designer::palette_index(app).map(bind)),
            super::theme_settings_content(app).map(bind),
            palette,
            Message::SettingsThemeClose { window, generation },
            Message::SheetBackdrop,
            None,
            scale,
        );
    }
    let content = bridge::view(app).map(move |intent| match intent {
        intent::Message::PageAction(message) => match *message {
            message @ Message::SettingsRecheckProvider { .. } => message,
            message => Message::SettingsPageAction {
                window,
                generation,
                message: Box::new(message),
            },
        },
        intent => Message::SettingsIntent {
            window,
            generation,
            intent,
        },
    });
    let mut footer = iced::widget::row![]
        .spacing(10.0 * scale)
        .align_y(iced::Alignment::Center);
    let recover = app.local_state_recovery_available();
    let studio_status = session.layout_studio() && session.save_notice.is_some();
    if !recover && !studio_status {
        return iced::widget::stack![content].into();
    }
    if recover {
        footer = footer.push(
            super::super::sheets::panel_button(
                super::super::sheets::panel_text(
                    app.settings_locale.text_static("r5-reload-settings"),
                    scale,
                ),
                palette,
                scale,
            )
            .on_press(Message::SettingsReloadSavedState { window, generation }),
        );
    }
    if let Some(palette) = session.palette.as_ref().filter(|_| studio_status) {
        footer = footer.push(
            super::folio::workbench::status_footer(bridge::SaveNotice::status(app), scale, palette)
                .map(move |intent| Message::SettingsIntent {
                    window,
                    generation,
                    intent,
                }),
        );
    }
    let footer = iced::widget::container(footer)
        .padding(10.0 * scale)
        .width(Fill)
        .style(move |_| crate::donor_ui::box_style(palette.surface, palette.outline));
    let bottom = iced::widget::column![footer];
    if session.layout_studio() {
        return iced::widget::column![iced::widget::container(content).height(Fill), bottom,]
            .height(Fill)
            .into();
    }
    iced::widget::stack![
        content,
        iced::widget::container(bottom)
            .height(Fill)
            .align_y(iced::alignment::Vertical::Bottom)
    ]
    .into()
}

pub(in crate::candidate_app) fn open(
    app: &mut GpuCandidate,
    category: SettingsCategory,
) -> Task<Message> {
    let task = match controller::finish_pointer_edit(app) {
        Ok(task) => task,
        Err(task) => return task,
    };
    let tab = match category {
        SettingsCategory::Theme => {
            app.ensure_theme_designer();
            controller::select_tab(app, "theme".into());
            app.settings_workbench.windows_panel = Some(SettingsCategory::Theme);
            return task;
        }
        SettingsCategory::WallpapersDisplays => "displays",
        SettingsCategory::Library => "library",
        SettingsCategory::Automation => "automation",
        SettingsCategory::Playback => "playback",
        _ => return task,
    };
    controller::select_tab(app, tab.into());
    if let Some(section) = controller::cards(app).iter().position(|(_, rows)| {
        rows.iter()
            .any(|row| matches!(row.control, super::composed::Control::PlatformForm))
    }) {
        app.settings_workbench.navigation.section = section;
    }
    task
}
pub(in crate::candidate_app) fn close(app: &mut GpuCandidate) {
    if let Err(error) = super::pending_forms::finish_rules(app) {
        app.settings_workbench.pending_forms.message = error;
        return;
    }
    if app.settings_workbench.windows_panel == Some(SettingsCategory::Theme)
        && app.theme_session.designer_dirty()
    {
        let _ =
            app.request_theme_departure(crate::candidate_app::theme_workflow::Departure::Message(
                Box::new(Message::SettingsThemeClose {
                    window: app.window,
                    generation: app.settings_workbench.generation,
                }),
            ));
        return;
    }
    if app.settings_workbench.windows_panel.take() == Some(SettingsCategory::Theme) {
        app.theme_session.on_departure();
    }
    controller::rotate(app);
}

/// An exposed studio click dismisses one owner before gallery hit processing.
pub(in crate::candidate_app) fn outside_click(app: &mut GpuCandidate) -> Option<Task<Message>> {
    if !app.panel_state.settings_open {
        return None;
    }
    if !app.visible || app.exit_requested {
        return Some(Task::none());
    }
    if let Some(sheet) = lifecycle::top_owned_sheet(
        app.panel_state,
        app.playlist_panel_open,
        app.engine_properties_open,
        controls_owns_input(app),
        folder_owns_input(app),
    ) {
        if sheet != lifecycle::OwnedSheet::Settings {
            lifecycle::close_owned_sheet(app, sheet);
        } else if app.settings_workbench.windows_panel.is_some() {
            close(app);
        } else {
            return Some(controller::dispatch(
                app,
                app.settings_workbench.generation,
                intent::Message::ToggleSettings,
            ));
        }
    }
    Some(Task::none())
}

pub(in crate::candidate_app) fn route_key(
    app: &mut GpuCandidate,
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
    id: window::Id,
    captured: bool,
) -> Option<Task<Message>> {
    if !valid(app, Some(id), app.settings_workbench.generation) {
        return None;
    }
    if app.settings_workbench.windows_panel.is_some() {
        if crate::candidate_app::lifecycle::matches_candidate_key(key, modifiers, "escape")
            || (!captured
                && crate::candidate_app::lifecycle::matches_candidate_key(
                    key,
                    modifiers,
                    &app.config.keybinds.close,
                ))
        {
            close(app);
        }
        return Some(Task::none());
    }
    controller::route_key(app, key, modifiers, id, captured)
}

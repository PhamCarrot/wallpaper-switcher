//! One window/generation boundary for composed Settings and retained Windows
//! controls. All resulting actions still enter the existing app updater.
use super::{bridge, controller, folio::intent};
use crate::candidate_app::*;

pub(in crate::candidate_app) fn folder_owns_input(app: &GpuCandidate) -> bool {
    app.folder_menu_open
        && (!app.panel_state.settings_open
            || app.settings_workbench.windows_panel == Some(SettingsCategory::Library))
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
pub(in crate::candidate_app) fn legacy_valid(
    app: &GpuCandidate,
    window: Option<window::Id>,
    generation: u64,
    message: &Message,
) -> bool {
    bound(app, window, generation)
        && (!folder_owns_input(app)
            || (app.settings_workbench.windows_panel == Some(SettingsCategory::Library)
                && matches!(
                    message,
                    Message::SetFolderFilter(_) | Message::ToggleFolderMenu
                )))
}

pub(in crate::candidate_app) fn view(app: &GpuCandidate) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    let session = &app.settings_workbench;
    let generation = session.generation;
    let window = app.window;
    let palette = crate::candidate_app::view::donor_palette(app);
    if let Some(category) = session.windows_panel {
        // Theme owns a source-style index and one reading scroller. Mount it
        // directly, retaining the same window/generation and departure gateway.
        if category == SettingsCategory::Theme {
            let bind = move |message| Message::SettingsLegacy {
                window,
                generation,
                message: Box::new(message),
            };
            return crate::donor_folio::sheet_with_scroll_capture(
                app.viewport,
                Some(super::theme_designer::palette_index(app).map(bind)),
                super::theme_settings_content(app).map(bind),
                palette,
                Message::SettingsWindowsClose { window, generation },
                Message::SheetBackdrop,
                None,
                scale,
            );
        }
        let mut content = super::windows_settings_content(app, category).map(move |message| {
            Message::SettingsLegacy {
                window,
                generation,
                message: Box::new(message),
            }
        });
        if app.local_state_recovery_available() {
            content = iced::widget::column![
                super::super::sheets::panel_button(
                    super::super::sheets::panel_text(
                        app.settings_locale.text_static("r5-reload-settings"),
                        scale
                    ),
                    palette,
                    scale
                )
                .on_press(Message::SettingsReloadSavedState { window, generation }),
                iced::widget::scrollable(
                    iced::widget::text(&session.message)
                        .size(12.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                        .width(Fill)
                )
                .height(60.0 * scale),
                content,
            ]
            .spacing(10.0 * scale)
            .into();
        }
        let close = Message::SettingsWindowsClose { window, generation };
        let tr = |key| app.settings_locale.text_static(key);
        let index = crate::donor_folio::index(
            tr("r5-windows-controls"),
            tr("r5-windows-note"),
            iced::widget::column![
                super::super::sheets::panel_button(
                    super::super::sheets::panel_text(tr("r5-back-settings"), scale),
                    palette,
                    scale
                )
                .on_press(close.clone()),
                super::super::sheets::panel_text(category.label(), scale)
                    .size(13.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
                super::super::sheets::panel_text(category.description(), scale),
            ]
            .spacing(12.0 * scale)
            .into(),
            palette,
            scale,
        );
        let page = crate::donor_folio::page(
            category.label(),
            if category == SettingsCategory::Theme {
                "Theme designer"
            } else {
                "Windows controls"
            },
            content,
            Some(close.clone()),
            None,
            palette,
            crate::donor_folio::BlueprintKind::Settings,
            scale,
        );
        return crate::donor_folio::sheet_with_scroll_capture(
            app.viewport,
            Some(index),
            page,
            palette,
            close,
            Message::SheetBackdrop,
            None,
            scale,
        );
    }
    let content = bridge::view(app).map(move |intent| Message::SettingsIntent {
        window,
        generation,
        intent,
    });
    let mut footer = iced::widget::row![]
        .spacing(10.0 * scale)
        .align_y(iced::Alignment::Center);
    let windows_category = category_for_tab(&session.tab)
        .filter(|_| !matches!(session.tab.as_str(), "picker" | "library" | "automation"));
    let recover = app.local_state_recovery_available();
    let studio_status = session.layout_studio() && !session.message.is_empty();
    if windows_category.is_none() && session.tab != "library" && !recover && !studio_status {
        return content.into();
    }
    if let Some(category) = windows_category {
        footer = footer.push(
            super::super::sheets::panel_button(
                super::super::sheets::panel_text(
                    app.settings_locale.text_static("r5-windows-controls"),
                    scale,
                ),
                palette,
                scale,
            )
            .on_press(Message::SettingsWindowsOpen {
                window,
                generation,
                category,
            })
            .style(move |_, status| crate::donor_ui::action_style(palette, false, false, status)),
        );
    }
    if session.tab == "library" {
        footer = footer.push(
            iced::widget::text(format!("{} wallpapers", app.wallpapers.len()))
                .size(12.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
        );
        footer = footer.push(
            super::super::sheets::panel_button(
                super::super::sheets::panel_text(
                    app.settings_locale.text_static("r5-catalog-refresh"),
                    scale,
                ),
                palette,
                scale,
            )
            .on_press(Message::SettingsIntent {
                window,
                generation,
                intent: intent::Message::RefreshCatalog,
            }),
        );
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
    // The reading panel and process modal own the scrollable status text.
    // Studio uses its own renderer, so retain its existing gateway status slot.
    if studio_status {
        footer = footer.push(
            iced::widget::scrollable(
                iced::widget::text(&session.message)
                    .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                    .width(Fill),
            )
            .height(60.0 * scale)
            .width(Fill),
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

fn category_for_tab(tab: &str) -> Option<SettingsCategory> {
    Some(match tab {
        "picker" => SettingsCategory::Picker,
        "filter" => SettingsCategory::CatalogFilters,
        "position" => SettingsCategory::Position,
        "motion" => SettingsCategory::Motion,
        "playback" => SettingsCategory::Playback,
        "library" => SettingsCategory::Library,
        "automation" => SettingsCategory::Automation,
        "theme" => SettingsCategory::Theme,
        _ => return None,
    })
}

pub(in crate::candidate_app) fn open(
    app: &mut GpuCandidate,
    category: SettingsCategory,
) -> Task<Message> {
    let task = match controller::finish_pointer_edit(app) {
        Ok(task) => task,
        Err(task) => return task,
    };
    if category == SettingsCategory::Theme {
        app.ensure_theme_designer();
    }
    app.panel_state.settings_category = category;
    app.settings_workbench.legacy_category = Some(category);
    app.settings_workbench.windows_panel = Some(category);
    controller::rotate(app);
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
                Box::new(Message::SettingsWindowsClose {
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

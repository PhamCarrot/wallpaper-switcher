use super::lifecycle::{
    cancel_theme_transient_if_departed, close_owned_sheet, gallery_close_action,
    matches_candidate_key, sheet_owns_keyboard, theme_owner_active, top_owned_sheet,
    visibility_after_tray_open, EscapeAction, PanelEffect, SelectionNavigation,
};
use super::operations::{
    provider_recovery_action_available, replace_engine_property_controls,
    update_provider_recovery_state,
};
use super::*;
use crate::{
    apply_trace, candidate_state, coordinator, donor_back_layout, donor_rail, engine_properties,
    gpu_gallery, poster_worker,
};

fn vertical_selection_offset(app: &GpuCandidate, down: bool) -> Option<isize> {
    let count = app.filtered_indices.len();
    if count == 0 {
        return None;
    }
    let current = app
        .selected
        .and_then(|selected| {
            app.filtered_indices
                .iter()
                .position(|index| *index == selected)
        })
        .unwrap_or(0);
    match app.gallery_mode {
        GalleryMode::Slices | GalleryMode::Depth => None,
        GalleryMode::Geometric => {
            let rows = app.state.gallery.geometric.rows.max(1);
            let row = current % rows;
            if down {
                (row + 1 < rows && current + 1 < count).then_some(1)
            } else {
                (row > 0).then_some(-1)
            }
        }
        GalleryMode::Wall => {
            let columns = app.state.gallery.wall.columns.max(1);
            if down {
                (current.checked_add(columns)? < count).then_some(columns as isize)
            } else {
                (current >= columns).then_some(-(columns as isize))
            }
        }
    }
}

fn dispatch_local_input(
    app: &mut GpuCandidate,
    action: view::settings::composed::input::InputAction,
    key_event: Option<(&keyboard::Key, keyboard::Modifiers, &NavigationKeyEvent)>,
    sequence: Option<u64>,
) -> Task<Message> {
    use view::settings::composed::input::InputAction as A;
    let navigation = match action {
        A::NavLeft => Some(SelectionNavigation::Relative(-1)),
        A::NavRight => Some(SelectionNavigation::Relative(1)),
        A::NavUp | A::NavDown => {
            let Some(offset) = vertical_selection_offset(app, action == A::NavDown) else {
                return Task::none();
            };
            Some(SelectionNavigation::Relative(offset))
        }
        A::PageBack => Some(SelectionNavigation::Relative(-5)),
        A::PageForward => Some(SelectionNavigation::Relative(5)),
        A::JumpFirst => Some(SelectionNavigation::Edge { last: false }),
        A::JumpLast => Some(SelectionNavigation::Edge { last: true }),
        _ => None,
    };
    if let Some(navigation) = navigation {
        if let Some((key, modifiers, event)) = key_event {
            app.trace_navigation_key_event(key, modifiers, event, navigation);
        }
        match navigation {
            SelectionNavigation::Relative(offset) => {
                let offset = if app.gallery_mode == GalleryMode::Depth && offset.abs() == 5 {
                    offset.signum()
                        * app.depth_frame.as_ref().map_or(1, |f| f.cards.len().max(1)) as isize
                } else {
                    offset
                };
                app.navigate_selection(offset);
            }
            SelectionNavigation::Edge { last } => app.navigate_to_edge(last),
        }
        app.resolve_depth_detail();
        return Task::none();
    }
    let message = match action {
        A::Select => app.selected.map(|index| Message::CardClicked {
            index,
            mouse_sequence: sequence,
        }),
        A::Apply => Some(Message::ApplySelected),
        A::Favourite => Some(Message::ToggleFavorite),
        A::Effects => {
            let task = update(
                app,
                Message::SetSettingsCategory(SettingsCategory::WallpapersDisplays),
            );
            if app.theme_workflow.prompt.is_some() || !app.panel_state.settings_open {
                return task;
            }
            let child = update(
                app,
                Message::SettingsWindowsOpen {
                    window: app.window,
                    generation: app.settings_workbench.generation,
                    category: SettingsCategory::WallpapersDisplays,
                },
            );
            return Task::batch([task, child]);
        }
        A::SceneProperties => Some(Message::ToggleEngineProperties),
        A::RandomRotate => Some(Message::ToggleRandomRotation),
        A::Playlists => Some(Message::TogglePlaylistPanel),
        A::Settings => Some(Message::ToggleSettingsPanel),
        A::Help => {
            let task = update(app, Message::SetSettingsCategory(SettingsCategory::Picker));
            view::settings::controller::reconcile(app);
            if app.theme_workflow.prompt.is_some() || app.settings_workbench.tab != "picker" {
                return task;
            }
            let section = update(
                app,
                Message::SettingsIntent {
                    window: app.window,
                    generation: app.settings_workbench.generation,
                    intent: view::settings::folio::intent::Message::Settings(
                        view::settings::folio::intent::SettingsMsg::SelectSection(
                            view::settings::composed::PICKER_CONTROLS_SECTION,
                        ),
                    ),
                },
            );
            return Task::batch([task, section]);
        }
        A::Flip => {
            let open = app
                .depth_detail
                .map_or(app.detail_open, |spring| spring.target > 0.0);
            app.start_detail(!open, "local-binding-flip");
            None
        }
        A::FilterBar => {
            app.rail_dismissed = !app.rail_dismissed;
            app.start_rail_reveal(!app.rail_dismissed);
            None
        }
        A::TagCloud => {
            if !app.panel_state.search_open {
                let task = update(app, Message::OpenSearch);
                if app.panel_state.search_open {
                    app.search_tags_open = true;
                }
                return task;
            }
            Some(Message::ToggleSearchTags)
        }
        A::FolderToggle => Some(Message::ToggleFolderMenu),
        A::SortNext => Some(Message::CycleSort),
        A::SortPrev => Some(Message::CycleSortBack),
        A::ModePrev | A::ModeNext => {
            let modes = [
                GalleryMode::Slices,
                GalleryMode::Geometric,
                GalleryMode::Wall,
                GalleryMode::Depth,
            ];
            let index = modes
                .iter()
                .position(|m| *m == app.gallery_mode)
                .unwrap_or(0);
            Some(Message::SetGalleryMode(
                modes[(index + if action == A::ModePrev { 3 } else { 1 }) % 4],
            ))
        }
        A::TypePrev | A::TypeNext => {
            let modes = [
                SourceFilter::All,
                SourceFilter::Pictures,
                SourceFilter::Videos,
                SourceFilter::WallpaperEngine,
            ];
            let index = modes
                .iter()
                .position(|m| *m == app.source_filter)
                .unwrap_or(0);
            Some(Message::SetSourceFilter(
                modes[(index + if action == A::TypePrev { 3 } else { 1 }) % 4],
            ))
        }
        A::FolderPrev | A::FolderNext => {
            let mut folders = vec![None];
            folders.extend(app.available_folders().into_iter().map(Some));
            let current = app.filter_rule.folders.iter().next();
            let index = folders
                .iter()
                .position(|f| f.as_ref() == current)
                .unwrap_or(0);
            let step = if action == A::FolderPrev {
                folders.len() - 1
            } else {
                1
            };
            Some(Message::SetFolderFilter(
                folders[(index + step) % folders.len()].clone(),
            ))
        }
        _ => None,
    };
    let task = message.map_or_else(Task::none, |message| update(app, message));
    app.resolve_depth_detail();
    task
}

fn encode_detail_bitmap(path: &std::path::Path) -> Result<Vec<u8>, String> {
    use image::{ImageDecoder, ImageEncoder, ImageReader};

    let mut reader = ImageReader::open(path)
        .and_then(ImageReader::with_guessed_format)
        .map_err(|error| error.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoder = reader.into_decoder().map_err(|error| error.to_string())?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > 32 * 1024 * 1024 {
        return Err("Image exceeds the clipboard size limit.".into());
    }
    let pixels = image::DynamicImage::from_decoder(decoder)
        .map_err(|error| error.to_string())?
        .to_rgb8();
    let mut bmp = Vec::new();
    image::codecs::bmp::BmpEncoder::new(&mut bmp)
        .write_image(
            &pixels,
            pixels.width(),
            pixels.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|error| error.to_string())?;
    Ok(bmp)
}

pub(super) fn update(app: &mut GpuCandidate, message: Message) -> Task<Message> {
    app.reconcile_tag_editor();
    app.reconcile_playlist_editor();
    let delivered_exit = match &message {
        Message::ExitCommitted { decision } => Some(*decision),
        _ => None,
    };
    view::settings::controller::reconcile(app);
    let geometry_edit = matches!(
        &message,
        Message::DepthDefault
            | Message::SetGalleryMode(_)
            | Message::CycleGalleryVariant
            | Message::CycleGalleryGeometry
            | Message::SetGeometricCurve(_)
            | Message::SetGeometricShape(_)
            | Message::SetGalleryNumber(_, _)
            | Message::SetDepthFrame(_)
            | Message::SetDepthShadows(_)
            | Message::SetSliceWobble(_)
            | Message::SetWallLayout(_)
            | Message::CycleGalleryVariation
            | Message::SaveGalleryPreset
            | Message::ApplyGalleryPreset(_, _)
            | Message::DeleteGalleryPreset(_, _)
    );
    let pending_edit = if geometry_edit
        && (!app.settings_workbench.draft.raw_inputs.is_empty()
            || app.settings_workbench.geometry_preview_baseline.is_some())
    {
        match view::settings::controller::finish_pointer_edit(app) {
            Ok(task) => task,
            Err(task) => return Task::batch([task]),
        }
    } else {
        Task::none()
    };
    let selection_before = app.selected;
    let search_under_settings = app.panel_state.settings_open && app.panel_state.search_open;
    let animation_frame = matches!(
        &message,
        Message::Wake(
            Wake::RailFrame { .. }
                | Wake::MotionReady
                | Wake::MotionFrame { .. }
                | Wake::DetailFrame { .. }
        ) | Message::SettingsIntent {
            intent: view::settings::folio::intent::Message::Frame(_),
            ..
        }
    );
    let task = update_inner(app, message);
    app.reconcile_tag_editor();
    app.reconcile_playlist_editor();
    app.reconcile_theme_prompt();
    view::settings::controller::reconcile(app);
    let exit = app.advance_exit_if_current(delivered_exit);
    if !animation_frame {
        app.sync_scene_motion();
    }
    let focus = if search_under_settings
        && app.visible
        && top_owned_sheet(
            app.panel_state,
            app.playlist_panel_open,
            app.engine_properties_open,
            app.controls_panel_open,
            app.folder_menu_open,
        ) == Some(super::lifecycle::OwnedSheet::Search)
    {
        let id = if app.tag_drawer_item.is_some() {
            &app.local_tag_input_id
        } else {
            &app.search_input_id
        };
        iced::widget::operation::focus(id.clone())
    } else {
        Task::none()
    };
    let preview = if animation_frame && selection_before == app.selected {
        Task::none()
    } else {
        view::settings::bridge::sync_preview(app)
    };
    Task::batch([pending_edit, task, exit, preview, focus])
}
pub(super) fn update_inner(app: &mut GpuCandidate, message: Message) -> Task<Message> {
    if app.playlist_delete.is_some()
        && matches!(
            &message,
            Message::PlaylistNameChanged(_)
                | Message::CreatePlaylist { .. }
                | Message::TogglePlaylistMembership(_)
                | Message::TogglePlaylistOrder(_)
                | Message::SetActivePlaylist(..)
                | Message::BrowsePlaylist(_)
        )
    {
        return Task::none();
    }
    if let Some(action) = app.theme_prompt_escape(&message) {
        return app.theme_action(action);
    }
    if app.theme_departure_needed(&message) {
        return app.request_theme_departure(theme_workflow::Departure::Message(Box::new(message)));
    }
    match message {
        Message::ThemeExitProbe => return app.begin_theme_exit_probe(),
        Message::ThemeExitProbeObserved {
            window,
            generation,
            attempt,
            mode,
        } => {
            return app.observe_theme_exit_probe(window, generation, attempt, mode);
        }
        Message::ThemeExitProbeTimeout { window, generation } => {
            return app.finish_theme_exit_probe_failure(window, generation);
        }
        Message::ThemeExitProbePollFailed { window, generation } => {
            return app.finish_theme_exit_probe_poll_failure(window, generation);
        }
        Message::ExitCommitted { .. } => {}
        Message::SettingsReloadSavedState { window, generation } => {
            if view::settings::gateway::valid(app, window, generation)
                && app.local_state_recovery_available()
            {
                if let Err(error) = app.reload_local_state() {
                    app.settings_workbench.message = error;
                }
            }
        }
        Message::SettingsIntent {
            window,
            generation,
            intent,
        } => {
            let allowed = if matches!(&intent, view::settings::folio::intent::Message::Frame(_)) {
                view::settings::gateway::frame_valid(app, window, generation)
            } else {
                view::settings::gateway::valid(app, window, generation)
                    && app.settings_workbench.windows_panel.is_none()
            };
            if allowed {
                return view::settings::controller::dispatch(app, generation, intent);
            }
        }
        Message::SettingsPreviewReady { generation, image } => {
            app.settings_workbench.finish_preview(generation, image);
        }
        Message::SettingsLegacy {
            window,
            generation,
            message,
        } => {
            if view::settings::gateway::legacy_valid(app, window, generation, &message)
                && app.settings_workbench.windows_panel.is_some()
            {
                return update_inner(app, *message);
            }
        }
        Message::SettingsWindowsOpen {
            window,
            generation,
            category,
        } => {
            if view::settings::gateway::valid(app, window, generation)
                && app.settings_workbench.windows_panel.is_none()
            {
                return view::settings::gateway::open(app, category);
            }
        }
        Message::SettingsWindowsClose { window, generation } => {
            if view::settings::gateway::valid(app, window, generation)
                && app.settings_workbench.windows_panel.is_some()
            {
                view::settings::gateway::close(app);
            }
        }
        Message::SettingsCapturedKey {
            window,
            key,
            modifiers,
        } => {
            if !app.engine_properties_open
                && !app.playlist_panel_open
                && !view::settings::gateway::controls_owns_input(app)
                && !view::settings::gateway::folder_owns_input(app)
                && (!app.panel_state.search_open || app.panel_state.settings_open)
            {
                if let Some(task) =
                    view::settings::gateway::route_key(app, &key, modifiers, window, true)
                {
                    return task;
                }
            }
        }
        Message::SettingsModifiers { window, modifiers } => {
            if app.window == Some(window) && app.visible {
                view::settings::controller::observe_modifiers(app, modifiers);
            }
        }
        Message::Wake(Wake::Toggle) => {
            if app.scenario.is_some() {
                log_debug(&format!(
                    "[SCENARIO-VISIBILITY] toggle requested current_visible={} window_id={:?}",
                    app.visible, app.window
                ));
            }
            return app.set_visible(!app.visible, "visibility-toggle");
        }
        Message::Wake(Wake::Tray(TrayAction::Open)) => {
            if app.scenario.is_some() {
                log_debug(&format!(
                    "[SCENARIO-VISIBILITY] tray-open requested current_visible={} window_id={:?}",
                    app.visible, app.window
                ));
            }
            return app.set_visible(visibility_after_tray_open(app.visible), "tray-open");
        }
        Message::Wake(Wake::Tray(TrayAction::Exit)) => {
            return app.request_exit();
        }
        Message::Wake(Wake::Tray(TrayAction::OpenConfig)) => {
            if let Err(error) = std::process::Command::new("notepad.exe")
                .arg(Config::config_path())
                .spawn()
            {
                app.status = Some(format!("Could not open config: {error}"));
            }
        }
        Message::Wake(Wake::Tray(TrayAction::Random)) => {
            app.run_playlist(
                None,
                None,
                AutomationTraceContext {
                    generation: None,
                    source: "manual-ui",
                    schedule_id: None,
                },
            );
        }
        Message::Wake(Wake::Tray(TrayAction::ToggleRandomRotation)) => {
            return update(app, Message::ToggleRandomRotation);
        }
        Message::Wake(Wake::Tray(TrayAction::StopPlaylist)) => {
            return update(
                app,
                Message::StopPlaylist {
                    generation: app.automation_generation.load(Ordering::Acquire),
                },
            )
        }
        Message::Wake(Wake::Tray(TrayAction::ResumeRandomRotation)) => {
            return update(app, Message::ResumeRandomRotation)
        }
        Message::Wake(Wake::CatalogDirty { generation }) => {
            if !app.exit_requested
                && generation == app.catalog_scan_generation.load(Ordering::Acquire)
            {
                app.refresh_catalog();
            }
        }
        Message::Wake(Wake::ColourReady(c)) => app.install_colour(c),
        Message::Wake(Wake::CatalogSnapshotSaved(result)) => {
            apply_trace::event_lazy("catalog-snapshot-saved", || format!("result={result:?}"));
            app.catalog_snapshot_issue = result.err();
        }
        Message::Colour(action) => app.colour_action(action),
        Message::Wake(Wake::CacheAcknowledged(report)) => {
            apply_trace::event_lazy("ui-cache-operation-ack", || {
                format!("epoch={} {}", report.epoch, report.summary())
            });
            if app.cache_operation == Some(report.epoch) {
                app.cache_operation = None;
                app.clear_posters();
                app.cache_status = report.summary();
                app.status = Some(app.cache_status.clone());
                app.schedule_posters();
                app.start_colours(false);
            }
        }
        Message::Wake(Wake::CatalogLoaded {
            generation,
            mut wallpapers,
            mut entries,
        }) => {
            if app.exit_requested
                || generation != app.catalog_scan_generation.load(Ordering::Acquire)
            {
                if let Some(owner) = &app.catalog_watch {
                    owner.acknowledge();
                }
                return Task::none();
            }
            if let Some(benchmark) = &mut app.benchmark {
                benchmark.source_catalog_items = wallpapers.len();
                let source_count = wallpapers.len().min(entries.len());
                if source_count > 0 && source_count < 1_000 {
                    for index in source_count..1_000 {
                        let base = index % source_count;
                        let mut wallpaper = wallpapers[base].clone();
                        wallpaper.id = format!("benchmark-{index}:{}", wallpaper.id);
                        wallpaper.title = format!("{} · benchmark {index}", wallpaper.title);
                        let mut entry = entries[base].clone();
                        entry.id = wallpaper.id.clone();
                        entry.title = wallpaper.title.clone();
                        wallpapers.push(wallpaper);
                        entries.push(entry);
                    }
                }
            }
            for entry in &mut entries {
                entry.tags = app
                    .state
                    .local_library
                    .effective_tags(&entry.id, &entry.source_tags);
            }
            if app.catalog_loaded && wallpapers == app.wallpapers && entries == app.catalog {
                let first_fresh = !app.catalog_fresh;
                app.accept_fresh_catalog(generation);
                app.revoke_catalog_tag_callbacks();
                app.update_catalog_status();
                if first_fresh {
                    app.schedule_poster_pregeneration();
                }
                apply_trace::event_lazy("catalog-unchanged", || {
                    format!(
                        "scan={generation} revision={}",
                        app.catalog_generation.load(Ordering::Acquire)
                    )
                });
                return Task::none();
            }
            // Capture before resource/catalog invalidation. A new catalog
            // revision invalidates outgoing ownership even for reused ID/path;
            // omit those old bodies truthfully, retain incoming roll presentation.
            let catalog_roll = app.capture_depth_roll().is_some();
            let revision = app.catalog_generation.fetch_add(1, Ordering::AcqRel) + 1;
            app.retain_catalog_posters(&wallpapers, revision);
            app.clear_engine_properties();
            let selected_id = app
                .selected
                .and_then(|index| app.wallpapers.get(index))
                .map(|wallpaper| wallpaper.id.clone());
            app.invalidate_tag_editor();
            // Never capture outgoing geometry using old indices against the new vectors.
            app.filtered_indices.clear();
            app.presentation_inputs = None;
            app.wallpapers = wallpapers;
            app.catalog = entries;
            app.project_local_tags();
            if !app.catalog_loaded {
                app.reset_default_folder();
            }
            app.colours.selection_anchor = None;
            app.selected = selected_id
                .as_deref()
                .and_then(|id| wallpaper_index_by_id(&app.wallpapers, id))
                .or_else(|| app.active_wallpaper_index())
                .or_else(|| {
                    app.screenshot_path
                        .is_some()
                        .then_some(app.wallpapers.len() / 2)
                });
            apply_trace::event_lazy("ui-item-selected", || {
                format!(
                    "reason=catalog-loaded previous_selected_id={selected_id:?} {}",
                    app.selected_item_trace_fields()
                )
            });
            app.catalog_loaded = true;
            app.accept_fresh_catalog(generation);
            app.start_colours(false);
            app.poster_failure = false;
            app.refresh_filter();
            if app.visible
                && app.gallery_mode == GalleryMode::Depth
                && app
                    .depth_presentation
                    .is_some_and(|p| !p.entrance.settled())
            {
                app.start_motion(0.0);
            }
            if catalog_roll
                && app.visible
                && app.gallery_mode == GalleryMode::Depth
                && !app.filtered_indices.is_empty()
            {
                app.depth_roll = Some(DepthRoll {
                    wave: 0.0,
                    old: Vec::new(),
                    incoming: HashMap::new(),
                    completed: HashMap::new(),
                });
                app.start_motion(0.0);
            }
            apply_trace::event_lazy(PLAYLIST_CATALOG_SNAPSHOT_STAGE, || {
                let snapshot = build_catalog_playlist_snapshot(
                    &app.state,
                    &app.catalog,
                    &app.wallpapers,
                    Config::config_dir(),
                    app.catalog_loaded,
                    revision,
                );
                let snapshot_json = serde_json::to_string(&snapshot)
                    .unwrap_or_else(|_| "{\"snapshot_serialization\":\"failed\"}".into());
                format!("snapshot_json={snapshot_json}")
            });
            if app.scenario.is_some() {
                let requested_item = app
                    .scenario
                    .as_ref()
                    .map(|scenario| scenario.request.item.clone());
                let item_index = requested_item
                    .as_deref()
                    .and_then(|path| app.scenario_item_index(path));
                if let Some(scenario) = app.scenario.as_mut() {
                    scenario.item_index = item_index;
                    scenario.report.set_monitor_ids(app.monitor_ids.clone());
                    if let Some(index) = item_index {
                        scenario.report.set_resolved_item_path(
                            app.wallpapers[index]
                                .file_target
                                .to_string_lossy()
                                .into_owned(),
                        );
                    }
                }
                if item_index.is_none() {
                    return app.scenario_finish_failure(
                        "the explicit --scenario-item was not present in the scanned catalog",
                    );
                }
                let target = app
                    .scenario
                    .as_ref()
                    .map(|scenario| scenario.request.target.clone())
                    .unwrap_or_default();
                let target_valid = if target.eq_ignore_ascii_case("all") {
                    !app.monitor_ids.is_empty()
                } else {
                    app.monitor_ids
                        .iter()
                        .any(|device_id| device_id.eq_ignore_ascii_case(&target))
                };
                if !target_valid {
                    return app.scenario_finish_failure(format!(
                        "explicit --scenario-target does not match an available monitor: {target}"
                    ));
                }
            }
            if app.screenshot_path.is_some() {
                if let Some(index) = app.filtered_indices.get(app.screenshot_index).copied() {
                    app.colours.selection_anchor = None;
                    app.selected = Some(index);
                    apply_trace::event_lazy("ui-item-selected", || {
                        format!(
                            "reason=screenshot-selection {}",
                            app.selected_item_trace_fields()
                        )
                    });
                }
                if app.gallery_mode == GalleryMode::Depth {
                    // Diagnostic selection is resolved after filter settlement.
                    // Anchor its frame before installing requested static detail.
                    app.reset_depth_transients();
                }
                // Initial refresh legitimately resets detail/panel state when
                // selection changes. Set diagnostic sheets only after that
                // catalog settlement; normal daily entry remains unchanged.
                if app.screenshot_panel.as_deref() == Some("playlist") {
                    app.playlist_panel_open = true;
                }
                if app.screenshot_panel.as_deref() == Some("engine") {
                    app.engine_properties_open = true;
                    if let Some(path) = &app.screenshot_engine_schema {
                        let schema = engine_properties::read_supported(path);
                        replace_engine_property_controls(
                            &mut app.engine_properties,
                            &mut app.engine_property_values,
                            &mut app.engine_properties_diagnostic,
                            &schema,
                        );
                    }
                }
            }
            if app.screenshot_path.is_some() && app.screenshot_detail && app.selected.is_some() {
                app.detail_open = true;
                app.detail_progress = app.screenshot_detail_progress;
            }
            app.update_catalog_status();
            if app.visible {
                app.schedule_posters();
                app.refresh_backdrop();
            }
            app.schedule_poster_pregeneration();
        }
        Message::PendingForms {
            window,
            generation,
            action,
        } => {
            if view::settings::gateway::valid(app, window, generation) {
                view::settings::pending_forms::dispatch(app, action);
            }
        }
        Message::Wake(Wake::PosterGpu(gpu_gallery::PosterFeedback::Restore(mut posters))) => {
            if app.visible && app.cache_operation.is_none() {
                posters.retain(|(poster, _)| app.posters.iter().any(|p| p.same_resource(poster)));
                if !posters.is_empty() && app.ensure_preview_worker() {
                    if let Some(worker) = &app.poster_worker {
                        worker.restore(&app.posters, posters, || {
                            super::images::publish_poster_feedback(
                                gpu_gallery::PosterFeedback::Redraw,
                            );
                        });
                    }
                }
            }
            return Task::none();
        }
        Message::Wake(Wake::PosterGpu(gpu_gallery::PosterFeedback::Redraw)) => return Task::none(),
        Message::Wake(Wake::PosterGpu(gpu_gallery::PosterFeedback::Prewarmed { key, posters })) => {
            if app.visible
                && !app.poster_loading
                && app.cache_operation.is_none()
                && app.atlas_entry == Some(app.gallery_mode)
                && key == app.atlas_prewarm_key()
                && Arc::ptr_eq(&posters, &app.posters)
            {
                app.atlas_entry = None;
                apply_trace::event_lazy("atlas-prewarm-released", || {
                    format!(
                        "mode={:?} generation={} resources={}",
                        app.gallery_mode,
                        key.generation,
                        posters.len()
                    )
                });
                app.reveal_gallery_layout();
            }
            return Task::none();
        }
        Message::Wake(Wake::PosterGpu(gpu_gallery::PosterFeedback::MaintenanceAfter(delay))) => {
            let (send, receive) = futures_channel::oneshot::channel();
            std::thread::spawn(move || {
                std::thread::sleep(delay);
                let _ = send.send(());
            });
            return Task::perform(receive, |_| {
                Message::Wake(Wake::PosterGpu(gpu_gallery::PosterFeedback::Redraw))
            });
        }
        Message::Wake(Wake::PosterWorkerExited(exit)) => {
            if let Some(worker) = app
                .poster_worker
                .as_ref()
                .filter(|worker| worker.id() == exit.worker_id)
            {
                debug_assert!(worker.can_replace());
                let generation = app.poster_generation.load(Ordering::Acquire);
                let was_loading = app.poster_loading;
                let request_key = app.poster_request_key.take();
                let assignment_key = app.poster_assignment_key.take();
                app.poster_worker = None;
                app.invalidate_poster_work();
                // Motion ticks may repeat the failed demand. A new request or
                // explicit invalidation can retry through the normal scheduler.
                app.poster_request_key = request_key;
                app.poster_assignment_key = assignment_key;
                if app.cache_operation.take().is_some() {
                    app.cache_status =
                        "Poster worker stopped before the cache operation was acknowledged.".into();
                }
                app.poster_failure = true;
                let message = format!("Poster preview worker stopped: {}", exit.failure);
                app.status = Some(app.status_with_persistence_issue(&message));
                apply_trace::event_lazy("poster-worker-stopped", || {
                    format!(
                        "worker={} generation={generation} loading={was_loading}->{} failure={:?} status={message:?}",
                        exit.worker_id, app.poster_loading, exit.failure
                    )
                });
            }
        }
        Message::Wake(Wake::PostersReady) => {
            let completion = match app.poster_mailbox.take() {
                Ok(value) => value,
                Err(error) => {
                    app.invalidate_poster_work();
                    app.poster_worker = None;
                    app.record_poster_worker_failure(error);
                    return Task::none();
                }
            };
            let Some(poster_worker::PosterCompletion {
                generation,
                cursor,
                finished,
                posters,
                failures,
            }) = completion
            else {
                return Task::none();
            };
            if poster_completion_is_current(
                generation,
                app.poster_generation.load(Ordering::Acquire),
                app.visible,
            ) {
                let pins = app.gallery_pins();
                let valid_posters: Vec<_> = posters
                    .into_iter()
                    .filter(|decoded| {
                        app.wallpapers
                            .get(decoded.source.index)
                            .is_some_and(|wallpaper| poster_completion_matches(decoded, wallpaper))
                            && !pins.iter().any(|pin| {
                                pin.index == decoded.source.index
                                    || pin.slot.layer == decoded.source.layer
                            })
                    })
                    .collect();
                if !valid_posters.is_empty() {
                    app.poster_failure = false;
                    if let Some(benchmark) = &mut app.benchmark {
                        if benchmark.cold_open_ms.is_none() {
                            benchmark.cold_open_ms =
                                Some(benchmark.started.elapsed().as_secs_f64() * 1_000.0);
                        } else if let Some(requested) = benchmark.open_requested.take() {
                            benchmark
                                .warm_open_ms
                                .push(requested.elapsed().as_secs_f64() * 1_000.0);
                        }
                    }
                }
                let catalog_generation = app.catalog_generation.load(Ordering::Acquire);
                let mut failed = 0;
                for failure in failures {
                    let source = &failure.source;
                    if app.wallpapers.get(source.index).is_some_and(|wallpaper| {
                        gallery::poster_source_matches(source, wallpaper)
                            && source
                                .still
                                .as_ref()
                                .is_none_or(|still| still.catalog_revision == catalog_generation)
                    }) && !pins
                        .iter()
                        .any(|pin| pin.index == source.index || pin.slot.layer == source.layer)
                    {
                        app.poster_failed_indices.insert(source.index);
                        failed += 1;
                        apply_trace::event_lazy("poster-item-failed", || {
                            let [far, near] = failure.tiers;
                            format!(
                                "generation={generation} index={} layer={} tiers={far:?},{near:?} item={:?} source={:?} error={}",
                                source.index, source.layer, source.item_id, source.path, failure.error,
                            )
                        });
                    }
                }
                let incoming_indices: HashSet<_> = valid_posters
                    .iter()
                    .map(|decoded| decoded.source.index)
                    .collect();
                let incoming_layers: HashSet<_> = valid_posters
                    .iter()
                    .map(|decoded| decoded.source.layer)
                    .collect();
                app.poster_failed_indices
                    .retain(|index| !incoming_indices.contains(index));
                // Older slots can still draw while their replacements arrive.
                // Reclaim a slot on either kind of collision and retire leftovers at EOF.
                app.poster_slots.retain(|index, slot| {
                    slot.catalog_generation == catalog_generation
                        && (!finished || slot.generation == generation)
                        && !incoming_indices.contains(index)
                        && !app.poster_failed_indices.contains(index)
                        && !incoming_layers.contains(&slot.layer)
                        && !pins
                            .iter()
                            .any(|pin| pin.index == *index || pin.slot.layer == slot.layer)
                });
                let retained_layers: HashSet<_> =
                    app.poster_slots.values().map(|slot| slot.layer).collect();
                let resident = Arc::make_mut(&mut app.posters);
                resident.retain(|poster| retained_layers.contains(&poster.layer));
                let installed = valid_posters.len();
                for decoded in valid_posters {
                    let source = decoded.source;
                    app.poster_slots.insert(
                        source.index,
                        PosterSlot {
                            layer: source.layer,
                            kind: source.kind,
                            generation,
                            catalog_generation,
                            item_id: source.item_id,
                            source: source.path,
                        },
                    );
                    resident.push(decoded.poster);
                }
                if finished {
                    let live_pin = pins.iter().any(|pin| {
                        (app.gallery_mode != GalleryMode::Depth
                            && app.filtered_indices.contains(&pin.index))
                            || app
                                .depth_frame
                                .as_ref()
                                .is_some_and(|frame| frame.live_poster_indices.contains(&pin.index))
                    });
                    app.poster_failure =
                        resident.is_empty() && !live_pin && !app.filtered_indices.is_empty();
                    app.poster_loading = false;
                }
                for pin in pins {
                    app.poster_slots.insert(pin.index, pin.slot);
                    if !resident
                        .iter()
                        .any(|p: &Poster| p.layer == pin.poster.layer)
                    {
                        resident.push(pin.poster);
                    }
                }
                apply_trace::event_lazy("poster-gallery-merge", || {
                    format!(
                        "generation={generation} cursor={cursor} finished={finished} installed={installed} slots={} failed={failed}",
                        resident.len()
                    )
                });
                app.rebuild_depth_frame();
                app.update_catalog_status();
                if finished {
                    app.maybe_schedule_screenshot();
                    app.scenario_ready_after_posters();
                }
            }
        }
        Message::Wake(Wake::BackdropLoaded {
            generation,
            width,
            height,
            rgba,
        }) => {
            if app.state.appearance.presentation == BackgroundPresentation::WallpaperBackdrop
                && generation == app.backdrop_generation.load(Ordering::Acquire)
            {
                if width > 0 && height > 0 && rgba.len() == width as usize * height as usize * 4 {
                    app.backdrop = Some(iced_image::Handle::from_rgba(width, height, rgba));
                } else {
                    app.backdrop = None;
                    app.backdrop_source = None;
                }
                app.maybe_schedule_screenshot();
            }
        }
        Message::Wake(Wake::CaptureScreenshot) => {
            if let Some(id) = app.window {
                return window::screenshot(id).map(Message::ScreenshotCaptured);
            }
        }
        Message::Wake(Wake::ScenarioAdvance) => return app.run_scenario_step(),
        Message::Wake(Wake::ScenarioStepReady { step, window_mode }) => {
            return app.complete_scenario_step(step, window_mode, None);
        }
        Message::Wake(Wake::ScenarioBeginWindowSettlement { step, requested }) => {
            return app.begin_scenario_window_settlement(step, requested);
        }
        Message::Wake(Wake::ScenarioWindowPoll { step, generation }) => {
            return app.poll_scenario_window(step, generation);
        }
        Message::Wake(Wake::ScenarioWindowObserved {
            step,
            generation,
            window_mode,
        }) => {
            return app.observe_scenario_window(step, generation, window_mode);
        }
        Message::Wake(Wake::ScenarioAbort) => {
            app.catalog_watch = None;
            std::process::exit(2)
        }
        Message::Wake(Wake::ScenarioTimeout) => {
            return app.scenario_finish_failure("scenario timed out before completing");
        }
        Message::ScreenshotCaptured(screenshot) => {
            let Some(path) = app.screenshot_path.as_ref() else {
                return Task::none();
            };
            if let Some(parent) = path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
            {
                if let Err(error) = std::fs::create_dir_all(parent) {
                    log_debug(&format!("Could not create screenshot directory: {error}"));
                    return Task::none();
                }
            }
            match image::save_buffer_with_format(
                path,
                screenshot.as_ref(),
                screenshot.size.width,
                screenshot.size.height,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            ) {
                Ok(()) => {
                    log_debug(&format!(
                        "GPU candidate screenshot saved to {}",
                        path.display()
                    ));
                    return app.request_exit();
                }
                Err(error) => {
                    app.status = Some(format!("Could not save candidate screenshot: {error}"));
                    log_debug(app.status.as_deref().unwrap_or_default());
                }
            }
        }
        Message::Wake(Wake::CoordinatorFinished(CoordinatorResult::Apply(result))) => {
            let operation_matches = app.active_operation == Some(result.operation_id);
            apply_trace::event_for_lazy(result.operation_id.get(), "ui-apply-result", || {
                let provider_unknown = result
                    .outcomes
                    .iter()
                    .any(|outcome| outcome.state == coordinator::ApplyOperationState::Unknown);
                format!(
                    "active_operation_match={operation_matches} success={} provider_unknown={provider_unknown} hide_after_apply={} error={:?} {}",
                    result.is_success(),
                    app.hide_after_apply,
                    result.error_message(),
                    app.selected_item_trace_fields()
                )
            });
            if !operation_matches {
                log_debug(&format!(
                    "Ignoring stale apply completion for operation {}",
                    result.operation_id.get()
                ));
                return Task::none();
            }
            if app
                .wallpaper_change
                .as_ref()
                .is_some_and(|captured| !captured.accepts(&result))
            {
                app.status = Some(
                    "Mismatched wallpaper completion ignored; captured operation remains owned."
                        .into(),
                );
                return Task::none();
            }
            // The active physical operation owns its result even after timer/control edits.
            let automatic_failed = app.wallpaper_change.as_ref().is_some_and(|captured| {
                captured.automation_generation.is_some()
                    && (!result.is_success()
                        || result.outcomes.is_empty()
                        || captured.target_ids.iter().any(|target| {
                            !result.outcomes.iter().any(|outcome| {
                                target.eq_ignore_ascii_case(&outcome.target_id)
                                    && outcome.state
                                        == coordinator::ApplyOperationState::ProviderConfirmed
                                    && outcome.error.is_none()
                            })
                        })
                        || result.outcomes.iter().any(|outcome| {
                            outcome.state != coordinator::ApplyOperationState::ProviderConfirmed
                                || outcome.error.is_some()
                        }))
            });
            app.record_confirmed_completion(&result, unix_now());
            if automatic_failed {
                app.safely_stop_automation();
            }
            if !result.is_success()
                || result
                    .outcomes
                    .iter()
                    .any(|o| o.state != coordinator::ApplyOperationState::ProviderConfirmed)
            {
                app.startup_queue.clear();
            }
            app.active_operation = None;
            app.apply_pending = false;
            let close_after = std::mem::take(&mut app.hide_after_apply);
            let provider_unknown = result
                .outcomes
                .iter()
                .any(|outcome| outcome.state == coordinator::ApplyOperationState::Unknown);
            let recovery_pending = update_provider_recovery_state(
                &mut app.provider_recoveries,
                None,
                false,
                provider_unknown,
                &result.recoveries,
            );
            app.provider_state_unknown = provider_unknown || recovery_pending;
            if app.provider_state_unknown && !app.automation_suspended {
                app.safely_stop_automation();
            }
            if app.exit_requested {
                return Task::none();
            }
            if provider_unknown || recovery_pending {
                app.hide_after_apply = false;
                let exit_requested = app.exit_requested;
                let message = if exit_requested {
                    if !provider_recovery_action_available(&app.provider_recoveries, app.selected) {
                        "Wallpaper provider ownership remains unresolved; Exit remains held because no automatic reconciliation capability is available."
                    } else {
                        "Wallpaper provider ownership remains unresolved; Exit remains held while retained provider ownership is unresolved."
                    }
                } else {
                    if !provider_recovery_action_available(&app.provider_recoveries, app.selected) {
                        "Wallpaper provider state remains unresolved; independent recovery capability is unavailable."
                    } else {
                        "Wallpaper provider state remains unresolved; further provider changes remain blocked."
                    }
                };
                app.status = Some(app.status_with_persistence_issue(message));
                return Task::none();
            }
            if result.is_success() {
                let message = match result.transition.warning_message() {
                    Some(warning) => format!(
                        "Automatic wallpaper change completed. Transition warning: {warning}"
                    ),
                    None => "Automatic wallpaper change completed.".to_string(),
                };
                app.status = Some(app.status_with_persistence_issue(&message));
                if close_after
                    && !result.outcomes.is_empty()
                    && result.outcomes.iter().all(|o| {
                        o.state == coordinator::ApplyOperationState::ProviderConfirmed
                            && o.error.is_none()
                    })
                {
                    app.hide_after_apply = false;
                    return app.set_visible(false, "apply-confirmed-hide");
                }
            } else {
                app.status = result
                    .error_message()
                    .map(|message| app.status_with_persistence_issue(&message));
            }
            if !app.startup_queue.is_empty() {
                app.advance_startup_reapply();
            }
        }
        Message::Wake(Wake::CoordinatorFinished(CoordinatorResult::Engine(result))) => {
            if app.wallpaper_change.is_some() {
                app.status =
                    Some("Control completion cannot settle a captured wallpaper operation.".into());
                return Task::none();
            }
            if app.active_operation != Some(result.operation_id) {
                log_debug(&format!(
                    "Ignoring stale Engine completion for operation {}",
                    result.operation_id.get()
                ));
                return Task::none();
            }
            app.wallpaper_change = None;
            app.active_operation = None;
            app.apply_pending = false;
            let provider_unknown = result
                .outcomes
                .iter()
                .any(|outcome| outcome.state == coordinator::ApplyOperationState::Unknown);
            let reconciliation = matches!(
                &result.command,
                EngineCommand::ReconcileProvider { .. } | EngineCommand::CancelPreflight { .. }
            );
            let recovery_resolved = result.recovery_resolved();
            let recovery_pending = update_provider_recovery_state(
                &mut app.provider_recoveries,
                Some(&result.command),
                result.is_success() || recovery_resolved,
                provider_unknown,
                &result.recoveries,
            );
            app.provider_state_unknown = provider_unknown || recovery_pending;
            if app.provider_state_unknown && !app.automation_suspended {
                app.safely_stop_automation();
            }
            if app.exit_requested {
                return Task::none();
            }
            if provider_unknown || recovery_pending {
                let exit_requested = app.exit_requested;
                let message = if exit_requested {
                    if !provider_recovery_action_available(&app.provider_recoveries, app.selected) {
                        "Wallpaper Engine ownership remains unresolved; Exit remains held because no automatic reconciliation capability is available."
                    } else {
                        "Wallpaper Engine ownership remains unresolved; Exit remains held while retained provider ownership is unresolved."
                    }
                } else {
                    if !provider_recovery_action_available(&app.provider_recoveries, app.selected) {
                        "Wallpaper Engine provider state remains unresolved; independent recovery capability is unavailable."
                    } else {
                        "Wallpaper Engine provider state remains unresolved; further provider changes remain blocked."
                    }
                };
                app.status = Some(app.status_with_persistence_issue(message));
                return Task::none();
            }
            if recovery_resolved {
                let message = if result
                    .outcomes
                    .iter()
                    .all(|outcome| outcome.state == coordinator::ApplyOperationState::Cancelled)
                {
                    "Unstarted request released. No wallpaper was applied; the next Apply requires fresh preflight."
                } else {
                    "Provider postcondition reconciled using the supported observation source; rendered scene is not independently observed."
                };
                app.status = Some(app.status_with_persistence_issue(message));
                return Task::none();
            }
            if result.is_success() {
                match result.command {
                    EngineCommand::SetPaused(value) => app.state.global_paused = value,
                    EngineCommand::SetMuted(value) => app.state.global_muted = value,
                    EngineCommand::SetVolume(value) => app.state.global_volume = value.min(100),
                    EngineCommand::ApplyProperties { .. } => {}
                    EngineCommand::ReconcileProvider { .. }
                    | EngineCommand::CancelPreflight { .. } => {}
                }
                let message = if reconciliation {
                    "Wallpaper Engine provider state reconciled."
                } else if result.outcomes.iter().all(|outcome| {
                    outcome.state == coordinator::ApplyOperationState::ProviderConfirmed
                }) {
                    "Wallpaper Engine control confirmed by independent provider readback."
                } else {
                    "Wallpaper Engine command accepted; current control/property values were not read back."
                };
                let persistence = app.persist_state_outcome();
                app.status = Some(match persistence {
                    Ok(outcome) => successful_engine_persistence_status(
                        message,
                        &outcome.backup_description(&app.state_path.with_extension("toml.bak")),
                        app.save_failure.as_deref(),
                        app.config_save_failure.as_deref(),
                        app.load_diagnostic.as_deref(),
                        app.config_load_diagnostic.as_deref(),
                    ),
                    Err(_) => app.status_with_persistence_issue(message),
                });
            } else {
                app.status = result
                    .error_message()
                    .map(|message| app.status_with_persistence_issue(&message));
            }
        }
        Message::Wake(Wake::MotionReady) => {
            if let Some(frame) = app.motion_mailbox.take() {
                return update_inner(app, Message::Wake(frame));
            }
        }
        Message::Wake(Wake::MotionFrame {
            generation,
            value,
            velocity,
            state,
            selection,
            presentation,
            filter_wave,
            ordinary,
            hex,
            wall,
            scene,
        }) => {
            if generation == app.motion_generation.load(Ordering::Acquire) && app.visible {
                if let Some(scene) = scene {
                    app.scene_motion = scene;
                }
                if let Some(benchmark) = &mut app.benchmark {
                    let now = std::time::Instant::now();
                    if let Some(previous) = benchmark.last_motion_frame.replace(now) {
                        let interval_ms = now.duration_since(previous).as_secs_f64() * 1_000.0;
                        if interval_ms < 100.0 {
                            benchmark.frame_intervals_ms.push(interval_ms);
                        }
                    }
                }
                if app.gallery_mode == GalleryMode::Depth {
                    if let (Some(roll), Some(wave)) = (&mut app.depth_roll, filter_wave) {
                        roll.advance_to(wave);
                    }
                    app.depth_camera = Some(state);
                    app.depth_selection = selection;
                    if presentation.is_some() {
                        app.depth_presentation = presentation;
                    }
                }
                if let (Some(roll), Some(wave)) = (&mut app.ordinary_roll, filter_wave) {
                    roll.advance_to(wave);
                }
                app.motion = value;
                app.motion_velocity = velocity;
                if let Some(mut ordinary) = ordinary {
                    ordinary.hover = app
                        .gallery_presentation
                        .as_ref()
                        .and_then(|p| p.hover.clone());
                    app.gallery_presentation = Some(ordinary);
                }
                if let Some(mut p) = hex {
                    p.hover = app.hex_presentation.as_ref().and_then(|h| h.hover.clone());
                    p.keyboard = app.hex_keyboard;
                    app.hex_presentation = Some(p);
                }
                if let Some(mut p) = wall {
                    if let Some(current) = &app.wall_presentation {
                        p.hover = current.hover.clone();
                        p.keyboard = current.keyboard;
                        p.engaged = current.engaged;
                    }
                    app.wall_presentation = Some(p);
                }
                if matches!(
                    app.gallery_mode,
                    GalleryMode::Wall
                        | GalleryMode::Depth
                        | GalleryMode::Slices
                        | GalleryMode::Geometric
                ) {
                    app.schedule_posters();
                }
                app.schedule_colour();
            }
        }
        Message::Wake(Wake::RailFrame { generation, value }) => {
            if generation == app.rail_generation.load(Ordering::Acquire) {
                app.rail_reveal = value;
            }
        }
        Message::Wake(Wake::DetailFrame {
            generation,
            value,
            close_when_done,
            spring,
        }) => {
            if generation == app.detail_generation.load(Ordering::Acquire) && app.visible {
                let completed_open = !close_when_done && value >= 1.0 && app.detail_progress < 1.0;
                app.detail_progress = value;
                app.depth_detail = spring;
                if close_when_done && value <= 0.0 {
                    apply_trace::event_lazy("ui-detail-transition", || {
                        format!(
                            "action=closed reason=close-animation-complete generation={generation} {}",
                            app.selected_item_trace_fields()
                        )
                    });
                    app.detail_open = false;
                    app.depth_detail = None;
                    app.detail_source = None;
                    app.depth_detail_mode = None;
                    app.depth_detail_id = None;
                } else if completed_open {
                    apply_trace::event_lazy("ui-detail-transition", || {
                        format!(
                            "action=opened reason=open-animation-complete generation={generation} {}",
                            app.selected_item_trace_fields()
                        )
                    });
                }
            }
        }
        Message::Wake(Wake::AutomationTick {
            generation,
            now_unix,
            weekday,
            minute_of_day,
        }) => {
            apply_trace::event_lazy("automation-tick-received", || {
                format!(
                    "generation={generation} now_unix={now_unix} weekday={weekday} minute_of_day={minute_of_day}"
                )
            });
            app.handle_automation_tick(generation, now_unix, weekday, minute_of_day);
        }
        Message::Wake(Wake::BenchmarkVisibility(visible)) => {
            if visible {
                if let Some(benchmark) = &mut app.benchmark {
                    benchmark.open_requested = Some(std::time::Instant::now());
                }
            }
            return app.set_visible(visible, "benchmark-visibility");
        }
        Message::Wake(Wake::BenchmarkNavigate(step)) => {
            if !app.filtered_indices.is_empty() {
                let current = app
                    .selected
                    .and_then(|selected| {
                        app.filtered_indices
                            .iter()
                            .position(|index| *index == selected)
                    })
                    .unwrap_or(0);
                let next = (current + 1) % app.filtered_indices.len();
                app.colours.selection_anchor = None;
                app.selected = app.filtered_indices.get(next).copied();
                let mode = match (step / 20) % 4 {
                    0 => GalleryMode::Slices,
                    1 => GalleryMode::Geometric,
                    2 => GalleryMode::Wall,
                    _ => GalleryMode::Depth,
                };
                if app.gallery_mode != mode {
                    // Benchmarks change only transient presentation, never the
                    // saved mode/profile, but must honor normal resource ownership.
                    app.stop_gallery_transients("benchmark-gallery-mode-changed");
                    app.invalidate_poster_work();
                    app.wall_scroll_offset = 0.0;
                    app.gallery_mode = mode;
                    if mode == GalleryMode::Depth {
                        app.reset_depth_transients();
                    }
                }
                app.start_motion(1.0);
                app.schedule_posters();
            }
        }
        Message::Wake(Wake::BenchmarkFinished) => {
            app.write_benchmark_report();
            app.status = Some("Benchmark finished; candidate is settling hidden.".into());
            return app.set_visible(false, "benchmark-complete");
        }
        Message::Wake(Wake::CloseHiddenWindow(generation)) => {
            if !app.visible && app.visibility_generation == generation {
                if let Some(id) = app.window.take() {
                    apply_trace::event_lazy("ui-visibility-transition", || {
                        format!(
                            "action=window-close-requested reason=hidden-delay-elapsed generation={generation} window_id={id:?} visible={} {}",
                            app.visible,
                            app.selected_item_trace_fields()
                        )
                    });
                    return window::close(id);
                }
            }
            apply_trace::event_lazy("ui-visibility-transition", || {
                format!(
                    "action=window-close-suppressed reason=visibility-request-changed requested_generation={generation} current_generation={} visible={} window_id={:?} {}",
                    app.visibility_generation,
                    app.visible,
                    app.window,
                    app.selected_item_trace_fields()
                )
            });
        }
        Message::WindowOpened { id, width, height } => {
            if app.window != Some(id) {
                return Task::none();
            }
            let caption_task = super::native_caption::install(id);
            if !app.visible {
                // A hide can overtake the asynchronous open request. The late
                // completion must honor that intent after native creation too.
                return caption_task.chain(GpuCandidate::hide_window_task(id));
            }
            if app.viewport != [width, height] {
                app.depth_roll = None;
            }
            let previous_visible = app.visible;
            let previous_window = app.window;
            app.start_rail_reveal(true);
            log_debug(&format!(
                "GPU candidate window opened {width:.0}x{height:.0} id={id:?} scenario={} ",
                app.scenario.is_some()
            ));
            app.window = Some(id);
            app.visible = true;
            app.viewport = [width, height];
            apply_trace::event_lazy("ui-visibility-transition", || {
                format!(
                    "action=window-opened reason=window-open-event previous_visible={previous_visible} previous_window_id={previous_window:?} visible={} window_id={id:?} viewport={width}x{height} visibility_generation={} {}",
                    app.visible,
                    app.visibility_generation,
                    app.selected_item_trace_fields()
                )
            });
            apply_gallery_viewport_policy(
                &mut app.state.gallery,
                gallery_kind(app.gallery_mode),
                app.viewport,
            );
            app.schedule_posters();
            app.resolve_depth_detail();
            return caption_task;
        }
        Message::WindowResized { id, width, height } => {
            if app.window == Some(id) {
                app.depth_roll = None;
                app.viewport = [width, height];
                apply_gallery_viewport_policy(
                    &mut app.state.gallery,
                    gallery_kind(app.gallery_mode),
                    app.viewport,
                );
                app.schedule_posters();
            }
        }
        Message::WindowClosed(id) => {
            if app.scenario.is_some() {
                log_debug(&format!(
                    "[SCENARIO-VISIBILITY] window-closed event id={id:?} current_window={:?} generation={}",
                    app.window, app.visibility_generation
                ));
            }
            let was_theme_owner = theme_owner_active(app);
            if app.window == Some(id) {
                app.window = None;
                app.window_focused = None;
                app.visible = false;
                app.stop_gallery_transients("window-closed");
                app.clear_posters();
                // The owned window closing ends Theme ownership just
                // like a hide: invalidate the session (committed
                // selection preserved) even when no hide ran first.
                // When the app was already hidden/invalidated,
                // was_theme_owner is false and this is a no-op.
                cancel_theme_transient_if_departed(app, was_theme_owner);
                apply_trace::event_lazy("ui-visibility-transition", || {
                    format!(
                        "action=window-closed reason=window-closed-event closed_window_id={id:?} visible={} visibility_generation={} {}",
                        app.visible,
                        app.visibility_generation,
                        app.selected_item_trace_fields()
                    )
                });
            } else {
                apply_trace::event_lazy("ui-visibility-transition", || {
                    format!(
                        "action=window-close-ignored reason=window_id_mismatch event_window_id={id:?} current_window_id={:?} visible={} visibility_generation={} {}",
                        app.window,
                        app.visible,
                        app.visibility_generation,
                        app.selected_item_trace_fields()
                    )
                });
            }
        }
        Message::WindowFocused(id) => {
            if app.window == Some(id) {
                app.window_focused = Some(true);
            }
            if app.scenario.is_some() {
                log_debug(&format!(
                    "[SCENARIO-VISIBILITY] focused event id={id:?} current_window={:?} visible={} generation={}",
                    app.window, app.visible, app.visibility_generation
                ));
            }
        }
        Message::CloseRequested(id) => {
            if app.scenario.is_some() {
                log_debug(&format!(
                    "[SCENARIO-VISIBILITY] close-requested event id={id:?} current_window={:?} visible={} generation={}",
                    app.window, app.visible, app.visibility_generation
                ));
            }
            if app.window == Some(id) {
                return app.set_visible(false, "window-close-requested");
            }
        }
        Message::WindowUnfocused(id) => {
            if app.window == Some(id) {
                app.window_focused = Some(false);
            }
            if app.scenario.is_some() {
                log_debug(&format!(
                    "[SCENARIO-VISIBILITY] unfocused event id={id:?} current_window={:?} visible={} close_on_focus_loss={} generation={}",
                    app.window,
                    app.visible,
                    app.config.ui.close_on_focus_loss,
                    app.visibility_generation
                ));
            }
            if app.window == Some(id)
                && app.visible
                && !app.launch_policy.is_preview()
                && app.theme_workflow.prompt.is_none()
                && app.theme_workflow.exit.is_none()
                && app.screenshot_path.is_none()
                && app.config.ui.close_on_focus_loss
            {
                return app.set_visible(false, "focus-loss");
            }
        }
        Message::CardClicked {
            index,
            mouse_sequence,
        } => {
            if let Some(task) = view::settings::gateway::outside_click(app) {
                return task;
            }
            if index < app.wallpapers.len() {
                app.gallery_keyboard = false;
                if app.gallery_mode == GalleryMode::Wall {
                    app.sync_wall_presentation();
                    if let Some(p) = &mut app.wall_presentation {
                        p.keyboard = false;
                    }
                }
                match card_click_action(app.selected, index) {
                    CardClickAction::Select => {
                        app.engage_gallery();
                        app.cancel_detail("card-selection-changed");
                        let previous = app.selected;
                        app.colours.selection_anchor = None;
                        app.selected = Some(index);
                        app.start_motion(if index > previous.unwrap_or(index) {
                            1.0
                        } else {
                            -1.0
                        });
                        app.colours.selection_anchor = None;
                        app.selected = Some(index);
                        apply_trace::event_lazy("ui-item-selected", || {
                            format!("reason=card-selection {}", app.selected_item_trace_fields())
                        });
                        app.wall_scroll_offset = 0.0;
                        app.schedule_posters();
                        trace_gallery_mouse_consumption(
                            mouse_sequence,
                            "clicked",
                            "select",
                            index,
                            app.selected_item_trace_fields(),
                        );
                    }
                    CardClickAction::OpenDetail => {
                        app.start_detail(true, "card-double-click");
                        trace_gallery_mouse_consumption(
                            mouse_sequence,
                            "clicked",
                            "open-detail",
                            index,
                            app.selected_item_trace_fields(),
                        );
                    }
                }
            } else {
                trace_gallery_mouse_consumption(
                    mouse_sequence,
                    "clicked",
                    "ignored-index-out-of-range",
                    index,
                    app.selected_item_trace_fields(),
                );
            }
        }
        Message::DepthDonorDetail(revision, identity, intent) => {
            if !app.depth_frame.as_ref().is_some_and(|f| {
                f.revision == revision
                    && f.cards.iter().any(|c| c.card.selected && c.id == identity)
            }) {
                return Task::none();
            }
            return update(app, Message::DonorDetail(identity, intent));
        }
        Message::DonorDetail(identity, intent) => {
            use donor_back_layout::DetailIntent;
            let controls_active = detail_controls_active(app);
            let selected_identity_matches = controls_active
                && app
                    .selected
                    .and_then(|i| app.wallpapers.get(i))
                    .is_some_and(|wallpaper| wallpaper.id == identity);
            if matches!(&intent, DetailIntent::Apply) {
                let outcome = if selected_identity_matches {
                    "dispatched"
                } else {
                    "rejected"
                };
                let reason = if !controls_active {
                    "detail_controls_inactive"
                } else if !selected_identity_matches {
                    "detail_identity_mismatch"
                } else {
                    "selected_identity_verified"
                };
                apply_trace::event_lazy("ui-apply-intent", || {
                    format!(
                        "source=detail outcome={outcome} reason={reason} detail_identity={identity:?} {}",
                        app.selected_item_trace_fields()
                    )
                });
            }
            if !controls_active || !selected_identity_matches {
                return Task::none();
            }
            let message = match intent {
                DetailIntent::Apply => Message::ApplySelected,
                DetailIntent::Hide => Message::ToggleExcluded,
                DetailIntent::Playlists => Message::OpenPlaylistPicker,
                DetailIntent::LocalTags => Message::OpenLocalTags(identity),
                DetailIntent::OpenFolder => Message::OpenFolder,
                DetailIntent::CopyImage => {
                    let Some(item) = app.selected.and_then(|index| app.wallpapers.get(index))
                    else {
                        return Task::none();
                    };
                    if item.source != WallpaperSource::LocalImage {
                        return Task::none();
                    }
                    let Some(window) = app.window else {
                        return Task::none();
                    };
                    let path = item.file_target.clone();
                    let current_copy = app.detail_copy_generation.clone();
                    let generation = current_copy.fetch_add(1, Ordering::AcqRel) + 1;
                    return Task::future(async move { encode_detail_bitmap(&path) }).then(
                        move |bmp| {
                            let current_copy = current_copy.clone();
                            let bmp = match bmp {
                                Ok(bmp) => bmp,
                                Err(error) => {
                                    return Task::done(Message::DetailImageCopied(
                                        generation,
                                        Err(error),
                                    ))
                                }
                            };
                            window::run(window, move |window| {
                                use iced::window::raw_window_handle::RawWindowHandle;
                                if current_copy.load(Ordering::Acquire) != generation {
                                    return Ok(());
                                }
                                let Ok(handle) = window.window_handle() else {
                                    return Err("Picker window is unavailable.".into());
                                };
                                let RawWindowHandle::Win32(handle) = handle.as_raw() else {
                                    return Err("Picker window is unavailable.".into());
                                };
                                let hwnd = handle.hwnd.get() as clipboard_win::types::HWND;
                                let _clipboard =
                                    clipboard_win::Clipboard::new_attempts_for(hwnd, 10)
                                        .map_err(|error| error.to_string())?;
                                clipboard_win::raw::set_bitmap_with(
                                    &bmp,
                                    clipboard_win::options::DoClear,
                                )
                                .map_err(|error| error.to_string())
                            })
                            .map(move |result| Message::DetailImageCopied(generation, result))
                        },
                    );
                }
                DetailIntent::CopyPath => {
                    return app
                        .selected
                        .and_then(|index| app.wallpapers.get(index))
                        .filter(|item| item.file_target.is_absolute())
                        .map_or_else(Task::none, |item| {
                            app.detail_copy_generation.fetch_add(1, Ordering::AcqRel);
                            iced::clipboard::write(item.file_target.display().to_string())
                        });
                }
                DetailIntent::Properties => Message::ToggleEngineProperties,
                DetailIntent::Favorite => Message::ToggleFavorite,
                DetailIntent::Close => {
                    app.start_detail(false, "detail-close-control");
                    return Task::none();
                }
            };
            return update(app, message);
        }
        Message::DetailImageCopied(generation, result) => {
            if app.detail_copy_generation.load(Ordering::Acquire) == generation {
                app.status = Some(match result {
                    Ok(()) => "Image copied.".into(),
                    Err(error) => format!("Could not copy image: {error}"),
                });
            }
        }
        Message::SheetBackdrop => {}
        Message::ApplySelected => app.apply_selected(false),
        Message::RefreshCatalog => app.refresh_catalog(),
        Message::RefreshThumbnails => app.request_poster_cache(false),
        Message::ClearPosterCache => app.request_poster_cache(true),
        Message::SetLibraryWatching(enabled) => {
            let mut proposed = app.state.clone();
            proposed.frontend_preferences.library.automatic_watch = Some(enabled);
            if app.commit_local_state_change(proposed).is_ok() {
                app.refresh_catalog();
            }
        }
        Message::SetLibraryPolling(seconds) => {
            let mut proposed = app.state.clone();
            proposed.frontend_preferences.library.polling_seconds = Some(seconds);
            if proposed.frontend_preferences.library.validate().is_ok()
                && app.commit_local_state_change(proposed).is_ok()
            {
                app.refresh_catalog();
            }
        }
        Message::GalleryInput {
            window,
            visibility_generation,
            motion_generation,
            catalog_generation,
            mode,
            selected,
            settings_generation,
            viewport,
            target_source,
            message,
        } => {
            if !app.visible
                || app.window != window
                || window.is_none()
                || app.visibility_generation != visibility_generation
                || app.motion_generation.load(Ordering::Acquire) != motion_generation
                || app.gallery_mode != mode
                || app.catalog_generation.load(Ordering::Acquire) != catalog_generation
                || app.viewport != viewport
                || app.settings_workbench.generation != settings_generation
                || app
                    .selected
                    .and_then(|i| app.wallpapers.get(i))
                    .map(|w| w.id.clone())
                    != selected
            {
                return Task::none();
            }
            if let GalleryMessage::Pointer {
                index,
                id,
                source,
                revision,
                presented_catalog_generation,
                button,
                modifiers,
                sequence,
            } = message
            {
                if app.theme_workflow.prompt.is_some() {
                    return Task::none();
                }
                if sheet_owns_keyboard(
                    app.panel_state,
                    app.playlist_panel_open,
                    app.engine_properties_open,
                    app.controls_panel_open,
                    app.folder_menu_open,
                ) {
                    if button == candidate_state::binding_codec::MouseButton::Left {
                        return view::settings::gateway::outside_click(app)
                            .unwrap_or_else(Task::none);
                    }
                    return Task::none();
                }
                if revision
                    .is_some_and(|r| app.depth_frame.as_ref().is_none_or(|f| f.revision != r))
                    || presented_catalog_generation.is_some_and(|g| g != catalog_generation)
                {
                    return Task::none();
                }
                let target = match (index, id) {
                    (Some(index), Some(id))
                        if app.filtered_indices.contains(&index)
                            && app.wallpapers.get(index).is_some_and(|w| {
                                w.id == id
                                    && Some(gallery::poster_source_for_item(w))
                                        == source.or(target_source)
                            }) =>
                    {
                        Some(index)
                    }
                    (None, None) => None,
                    _ => return Task::none(),
                };
                let map = super::lifecycle::effective_input_bindings(app);
                let action = map
                    .lookup_mouse(
                        candidate_state::binding_codec::MouseSpec {
                            mods: modifiers,
                            button,
                        },
                        view::settings::composed::input::ActiveScopes::default(),
                    )
                    .filter(|action| !action.targets_card() || target.is_some());
                if let Some(action) = action {
                    if app.gallery_mode == GalleryMode::Geometric {
                        app.hex_keyboard = false;
                        if let Some(p) = &mut app.hex_presentation {
                            p.keyboard = false;
                        }
                    }
                    if action == view::settings::composed::input::InputAction::Select {
                        if let Some(index) = target {
                            return update(
                                app,
                                Message::CardClicked {
                                    index,
                                    mouse_sequence: sequence,
                                },
                            );
                        }
                    } else {
                        if action.targets_card() {
                            if let Some(index) = target.filter(|i| app.selected != Some(*i)) {
                                if app.gallery_mode == GalleryMode::Depth
                                    && action == view::settings::composed::input::InputAction::Flip
                                {
                                    return update(
                                        app,
                                        Message::CardClicked {
                                            index,
                                            mouse_sequence: sequence,
                                        },
                                    );
                                }
                                let _ = update(
                                    app,
                                    Message::CardClicked {
                                        index,
                                        mouse_sequence: sequence,
                                    },
                                );
                            }
                        }
                        return dispatch_local_input(app, action, None, sequence);
                    }
                }
                if target.is_none() && button == candidate_state::binding_codec::MouseButton::Left {
                    return update(
                        app,
                        Message::Gallery(GalleryMessage::BackdropClicked(sequence)),
                    );
                }
                return Task::none();
            }
            return update(app, Message::Gallery(message));
        }
        Message::Gallery(GalleryMessage::Pointer { .. }) => {
            return Task::none();
        }
        Message::Gallery(message @ GalleryMessage::DepthPointer { .. }) => {
            if matches!(&message, GalleryMessage::DepthPointer { hover: true, .. })
                && super::lifecycle::detail_owns_gallery_hover(app)
            {
                return Task::none();
            }
            if app.gallery_mode == GalleryMode::Depth
                && matches!(&message, GalleryMessage::DepthPointer{revision,right:false,hover:false,..}
                    if app.depth_frame.as_ref().is_some_and(|frame| frame.revision == *revision))
            {
                if let Some(task) = view::settings::gateway::outside_click(app) {
                    return task;
                }
            }
            let captured = app.depth_panel_open()
                || sheet_owns_keyboard(
                    app.panel_state,
                    app.playlist_panel_open,
                    app.engine_properties_open,
                    app.controls_panel_open,
                    app.folder_menu_open,
                );
            // During reversal the card remains drawable until close settles;
            // toggle the latest requested spring target, not that visibility.
            let detail_target_open = app
                .depth_detail
                .map_or(app.detail_open, |spring| spring.target > 0.0);
            let intent = consume_depth_pointer(
                app.depth_frame.as_ref(),
                &app.wallpapers,
                app.selected,
                detail_target_open,
                app.visible && app.gallery_mode == GalleryMode::Depth,
                captured,
                &message,
            );
            match intent {
                DepthPointerIntent::Hover(id) => {
                    app.depth_hover = id;
                    app.rebuild_depth_frame();
                }
                DepthPointerIntent::Select(index) => {
                    return update(
                        app,
                        Message::CardClicked {
                            index,
                            mouse_sequence: None,
                        },
                    )
                }
                DepthPointerIntent::Detail(open) => app.start_detail(open, "depth-card-detail"),
                DepthPointerIntent::Backdrop if backdrop_click_dismisses(app) => {
                    return app.set_visible(false, "depth-backdrop")
                }
                _ => {}
            }
        }
        Message::Gallery(GalleryMessage::Hovered(index)) => {
            use view::settings::composed::SettingsSource;
            if super::lifecycle::detail_owns_gallery_hover(app) {
                return Task::none();
            }
            let hovered = index
                .and_then(|i| app.wallpapers.get(i))
                .map(|w| w.id.clone());
            app.gallery_keyboard = false;
            if let Some(p) = &mut app.gallery_presentation {
                p.hover = hovered.clone();
            }
            if app.gallery_mode == GalleryMode::Geometric {
                app.hex_keyboard = false;
                if let Some(p) = &mut app.hex_presentation {
                    p.hover = hovered.clone();
                    p.keyboard = false;
                }
            }
            if app.gallery_mode == GalleryMode::Wall {
                if let Some(p) = &mut app.wall_presentation {
                    p.hover = hovered;
                    p.keyboard = false;
                }
                app.start_motion(0.0);
            }
            let hover_selects =
                (view::settings::bridge::Source { app }).flag("general.hoverSelects");
            let hover_selects = hover_selects
                && matches!(
                    app.gallery_mode,
                    GalleryMode::Slices | GalleryMode::Geometric
                );
            if hover_selects && index.is_some_and(|i| app.filtered_indices.contains(&i)) {
                app.engage_gallery();
            }
            if hover_selects
                && index.is_some()
                && index != app.selected
                && index.is_some_and(|i| app.filtered_indices.contains(&i))
            {
                app.cancel_detail("hover-selection");
                app.colours.selection_anchor = None;
                app.selected = index;
                app.start_motion(0.0);
                app.schedule_posters();
            }
        }
        #[cfg(test)]
        Message::Gallery(GalleryMessage::PresentedClick {
            index,
            id,
            source,
            catalog_generation,
            sequence,
        }) => {
            if app.gallery_mode == GalleryMode::Geometric {
                app.hex_keyboard = false;
                if let Some(p) = &mut app.hex_presentation {
                    p.keyboard = false;
                }
            }
            if catalog_generation == app.catalog_generation.load(Ordering::Acquire)
                && app.filtered_indices.contains(&index)
                && app
                    .wallpapers
                    .get(index)
                    .is_some_and(|w| w.id == id && gallery::poster_source_for_item(w) == source)
            {
                return update(
                    app,
                    Message::CardClicked {
                        index,
                        mouse_sequence: sequence,
                    },
                );
            }
        }
        #[cfg(test)]
        Message::Gallery(GalleryMessage::Clicked(index, mouse_sequence)) => {
            return update(
                app,
                Message::CardClicked {
                    index,
                    mouse_sequence,
                },
            );
        }
        Message::Gallery(GalleryMessage::BackdropClicked(mouse_sequence)) => {
            if let Some(task) = view::settings::gateway::outside_click(app) {
                return task;
            }
            if backdrop_click_dismisses(app) {
                trace_gallery_mouse_consumption(
                    mouse_sequence,
                    "backdrop-clicked",
                    "hide-gallery",
                    0,
                    app.selected_item_trace_fields(),
                );
                return app.set_visible(false, "backdrop-click");
            }
            trace_gallery_mouse_consumption(
                mouse_sequence,
                "backdrop-clicked",
                "ignored-close-on-backdrop-click-disabled",
                0,
                app.selected_item_trace_fields(),
            );
        }
        Message::SetSourceFilter(source_filter) => {
            app.reset_nonsticky_folder();
            app.source_filter = source_filter;
            app.refresh_filter();
            app.schedule_posters();
        }
        Message::DonorRail(intent) => {
            if app.panel_state.settings_open || app.playlist_panel_open {
                return Task::none();
            }
            let message = match intent {
                donor_rail::RailIntent::Source(0) => {
                    Some(Message::SetSourceFilter(SourceFilter::All))
                }
                donor_rail::RailIntent::Source(1) => {
                    Some(Message::SetSourceFilter(SourceFilter::Pictures))
                }
                donor_rail::RailIntent::Source(2) => {
                    Some(Message::SetSourceFilter(SourceFilter::Videos))
                }
                donor_rail::RailIntent::Source(3) => {
                    Some(Message::SetSourceFilter(SourceFilter::WallpaperEngine))
                }
                donor_rail::RailIntent::Source(_) => None,
                donor_rail::RailIntent::CycleShape => Some(Message::CycleShapeFilter),
                donor_rail::RailIntent::CycleResolution => Some(Message::CycleResolution),
                donor_rail::RailIntent::SetSort(order) => Some(Message::SetSort(order)),
                donor_rail::RailIntent::ToggleFolder => Some(Message::ToggleFolderMenu),
                donor_rail::RailIntent::ToggleFavorites => Some(Message::ToggleFavoritesFilter),
                donor_rail::RailIntent::ToggleRandomRotation => Some(Message::ToggleRandomRotation),
                donor_rail::RailIntent::ToggleEngineMute if app.apply_pending => None,
                donor_rail::RailIntent::ToggleEngineMute => Some(Message::ToggleEngineMuted),
                donor_rail::RailIntent::TogglePlaylists => Some(Message::TogglePlaylistChooser),
                donor_rail::RailIntent::OpenSearch if app.panel_state.search_open => {
                    Some(Message::CloseSearchPanel)
                }
                donor_rail::RailIntent::OpenSearch => Some(Message::OpenSearch),
                donor_rail::RailIntent::ToggleSettings => Some(Message::ToggleSettingsPanel),
                donor_rail::RailIntent::HuePreset(index) => {
                    library::color_metadata::ColorBucket::from_index(index)
                        .map(|bucket| Message::Colour(colours::Action::Bucket(bucket)))
                }
                donor_rail::RailIntent::Close => Some(Message::HideGallery),
            };
            if let Some(message) = message {
                return update(app, message);
            }
        }
        Message::Gallery(GalleryMessage::Wheel(amount, mouse_sequence)) => {
            if !app.visible
                || app.detail_open
                || sheet_owns_keyboard(
                    app.panel_state,
                    app.playlist_panel_open,
                    app.engine_properties_open,
                    app.controls_panel_open,
                    app.folder_menu_open,
                )
            {
                return Task::none();
            }
            if app.gallery_mode == GalleryMode::Depth {
                let steps = depth_wheel_steps(&mut app.depth_wheel, amount);
                if steps != 0 {
                    app.engage_gallery();
                    app.navigate_selection(-steps);
                }
                return Task::none();
            }
            if !app.filtered_indices.is_empty() {
                if app.gallery_mode == GalleryMode::Wall {
                    let step = app.state.gallery.wall.card_height + app.state.gallery.wall.gap_y;
                    app.sync_wall_presentation();
                    if let Some(p) = &mut app.wall_presentation {
                        p.wheel(-amount * step);
                    }
                    app.start_motion(0.0);
                    app.schedule_posters();
                    trace_gallery_mouse_consumption(
                        mouse_sequence,
                        "wheel",
                        "wall-scroll",
                        app.selected.unwrap_or(0),
                        app.selected_item_trace_fields(),
                    );
                    return Task::none();
                }
                let current = app
                    .selected
                    .and_then(|selected| {
                        app.filtered_indices
                            .iter()
                            .position(|index| *index == selected)
                    })
                    .unwrap_or(0);
                app.engage_gallery();
                let next_position = if app.gallery_mode == GalleryMode::Geometric {
                    let rows = app.state.gallery.geometric.rows.max(1);
                    let step = app
                        .state
                        .gallery
                        .geometric
                        .scroll_step
                        .map_or(1, |s| s.effective() as usize)
                        .max(1);
                    let col = current / rows;
                    let next = col
                        .saturating_add_signed(if amount < 0.0 {
                            step as isize
                        } else {
                            -(step as isize)
                        })
                        .min(app.filtered_indices.len().div_ceil(rows).saturating_sub(1));
                    (next * rows + current % rows).min(app.filtered_indices.len() - 1)
                } else if amount < 0.0 {
                    (current + 1).min(app.filtered_indices.len() - 1)
                } else {
                    current.saturating_sub(1)
                };
                if next_position != current {
                    app.cancel_detail("wheel-selection-navigation");
                    app.start_motion(if next_position > current { 1.0 } else { -1.0 });
                }
                app.colours.selection_anchor = None;
                app.selected = app.filtered_indices.get(next_position).copied();
                if next_position != current {
                    apply_trace::event_lazy("ui-item-selected", || {
                        format!(
                            "reason=wheel-selection-navigation {}",
                            app.selected_item_trace_fields()
                        )
                    });
                }
                app.schedule_posters();
                trace_gallery_mouse_consumption(
                    mouse_sequence,
                    "wheel",
                    if next_position == current {
                        "selection-edge-noop"
                    } else {
                        "select"
                    },
                    app.selected.unwrap_or(0),
                    app.selected_item_trace_fields(),
                );
            } else {
                trace_gallery_mouse_consumption(
                    mouse_sequence,
                    "wheel",
                    "ignored-empty-filter",
                    0,
                    app.selected_item_trace_fields(),
                );
            }
        }
        Message::DepthDefault => {
            if app.gallery_mode != GalleryMode::Depth {
                return Task::none();
            }
            let previous = app.state.gallery.depth.clone();
            let legacy_detail = app.state.gallery.depth.detail.clone();
            app.state.gallery.depth = candidate_state::DepthSettings::default();
            app.state.gallery.depth.detail = legacy_detail;
            app.state.gallery.variant = None;
            app.depth_settings_changed(&previous);
            app.persist_state();
            app.schedule_posters();
        }
        Message::SetGalleryMode(mode) => {
            app.set_gallery_mode(mode);
        }
        Message::OpenSearch => {
            if sheet_owns_keyboard(
                app.panel_state,
                app.playlist_panel_open,
                app.engine_properties_open,
                app.controls_panel_open,
                app.folder_menu_open,
            ) {
                return Task::none();
            }
            let was_theme_owner = theme_owner_active(app);
            app.playlist_panel_open = false;
            app.engine_properties_open = false;
            app.folder_menu_open = false;
            let PanelEffect::FocusSearchInput = app.panel_state.open_search();
            app.controls_panel_open = false;
            cancel_theme_transient_if_departed(app, was_theme_owner);
            return iced::widget::operation::focus(app.search_input_id.clone());
        }
        Message::ToggleSettingsPanel => {
            let was_theme_owner = theme_owner_active(app);
            // Settings hides gallery search while preserving its query/open state.
            // Other sheets keep their own lifetime and drawing priority.
            app.panel_state.settings_open = !app.panel_state.settings_open;
            cancel_theme_transient_if_departed(app, was_theme_owner);
        }
        Message::SetSettingsCategory(category) => {
            if category == SettingsCategory::Theme {
                app.ensure_theme_designer();
            }
            let was_theme_owner = theme_owner_active(app);
            app.panel_state.open_settings(category);
            app.controls_panel_open = false;
            app.folder_menu_open = false;
            cancel_theme_transient_if_departed(app, was_theme_owner);
        }
        Message::SettingsSearchChanged(query) => app.settings_search = query,
        Message::CloseSearchPanel => {
            let was_theme_owner = theme_owner_active(app);
            app.panel_state.close_search_panel();
            cancel_theme_transient_if_departed(app, was_theme_owner);
        }
        Message::CycleShapeFilter => app.cycle_shape_filter(),
        Message::CycleGalleryVariant => app.cycle_gallery_variant(),
        Message::CycleGalleryGeometry => app.cycle_gallery_geometry(),
        Message::SetGeometricCurve(curve) => app.set_geometric_curve(curve),
        Message::SetGeometricShape(shape) => app.set_geometric_shape(shape),
        Message::SetGalleryNumber(field, value) => app.set_gallery_number(field, value),
        Message::SetDepthFrame(value) => app.edit_depth(|d| d.selection_frame = value),
        Message::SetDepthShadows(value) => app.edit_depth(|d| d.shadows = value),
        Message::SetDepthEffect(value) => {
            if candidate_state::depth_effect_id(&value).is_some() {
                app.edit_card_flip(|d| d.effect = value);
            }
        }
        Message::SetDepthShader(value) => app.edit_card_flip(|d| d.animate_shader = value),
        Message::SetDepthBack(value) => app.edit_card_flip(|d| d.animate_back = value),
        Message::SetSliceWobble(wobble) => {
            app.state.gallery.slices.wobble = wobble;
            app.state.gallery.variant = None;
            app.persist_state();
            app.schedule_posters();
        }
        Message::SetWallLayout(layout) => {
            let changed = app.state.gallery.wall.layout != layout;
            app.state.gallery.wall.layout = layout;
            app.state.gallery.variant = None;
            app.wall_scroll_offset = 0.0;
            app.persist_state();
            app.schedule_posters();
            if changed {
                app.reveal_gallery_layout();
            }
        }
        Message::CycleGalleryMotionSpeed => {
            app.state.gallery_motion_speed = app.state.gallery_motion_speed.next();
            if app.gallery_mode == GalleryMode::Depth && app.depth_roll.is_some() {
                app.start_motion(0.0);
            }
            app.persist_state();
        }
        Message::CycleGalleryVariation => app.cycle_gallery_motion(),
        Message::SaveGalleryPreset => match app.save_gallery_preset() {
            Ok(()) => {
                app.settings_workbench.preset_name_raw = None;
                app.settings_workbench.message = "Mode profile saved.".into();
                view::settings::controller::rotate(app);
            }
            Err(error) => app.settings_workbench.message = error,
        },
        Message::ApplyGalleryPreset(mode, id) => match app.apply_gallery_preset(mode, id) {
            Ok(()) => {
                app.settings_workbench.preset_name_raw = None;
                app.settings_workbench.message = "Current-mode profile saved and restored.".into();
                view::settings::controller::rotate(app);
            }
            Err(error) => app.settings_workbench.message = error,
        },
        Message::DeleteGalleryPreset(mode, id) => match app.delete_gallery_preset(mode, id) {
            Ok(true) => {
                app.settings_workbench.preset_name_raw = None;
                app.settings_workbench.message =
                    "Mode profile deleted; current geometry is unchanged.".into();
                view::settings::controller::rotate(app);
            }
            Ok(false) => {}
            Err(error) => app.settings_workbench.message = error,
        },

        Message::ToggleFavorite => {
            if let Some(wallpaper) = app.selected.and_then(|index| app.wallpapers.get(index)) {
                let favorite = app.state.toggle_favorite(&wallpaper.id);
                app.status = Some(if favorite {
                    "Added to favorites".into()
                } else {
                    "Removed from favorites".into()
                });
                app.persist_state();
                if app.filter_rule.favorites_only || app.browse_playlist_id.is_some() {
                    app.refresh_filter();
                    app.schedule_posters();
                }
            }
        }
        Message::ToggleExcluded => {
            if app.library_config_reconcile {
                app.status = Some("Config publication needs reconciliation. Reload saved Config or close/reopen before changing exclusions; live roots, exclusions and recovery files were retained.".into());
                return Task::none();
            }
            let wallpaper_id = app
                .selected
                .and_then(|index| app.wallpapers.get(index))
                .map(|wallpaper| wallpaper.id.clone());
            if let (Some(wallpaper_id), Some(monitor_key)) =
                (wallpaper_id, app.active_monitor_key())
            {
                let excluded = app.config.toggle_excluded(&monitor_key, &wallpaper_id);
                let message: String = if excluded {
                    "Hidden on the active display".into()
                } else {
                    "Visible on the active display".into()
                };
                match app.config.save_with_diagnostics() {
                    Ok(outcome) => {
                        app.config_save_failure = (!outcome.backup.is_available()).then(|| {
                            format!(
                                "Configuration changed and was saved, but {}",
                                outcome.backup_description(
                                    &Config::config_path().with_extension("toml.bak")
                                )
                            )
                        });
                        app.status = Some(app.status_with_persistence_issue(&message));
                    }
                    Err(error) => {
                        let failure = format!(
                            "Configuration changed in memory but was not fully saved; it may not survive restart: {error}. {}",
                            error.backup_description(&Config::config_path().with_extension("toml.bak"))
                        );
                        app.config_save_failure = Some(failure.clone());
                        app.status = Some(format!("{message}; {failure}"));
                    }
                }
                app.refresh_filter();
                app.schedule_posters();
            }
        }
        Message::ToggleShowExcluded => {
            app.reset_nonsticky_folder();
            app.show_excluded = !app.show_excluded;
            app.refresh_filter();
            app.schedule_posters();
        }
        Message::OpenFolder => {
            if let Some(wallpaper) = app.selected.and_then(|index| app.wallpapers.get(index)) {
                let target = if wallpaper.file_target.exists() {
                    &wallpaper.file_target
                } else {
                    &wallpaper.folder_path
                };
                if let Err(error) = std::process::Command::new("explorer.exe")
                    .arg("/select,")
                    .arg(target)
                    .spawn()
                {
                    app.status = Some(format!("Could not open source folder: {error}"));
                }
            }
        }
        Message::ToggleEngineProperties => {
            let was_theme_owner = theme_owner_active(app);
            app.toggle_engine_properties();
            cancel_theme_transient_if_departed(app, was_theme_owner);
        }
        Message::EnginePropertyValueChanged(key, value) => {
            if app.engine_properties_open
                && app.engine_properties.iter().any(|property| {
                    property.key == key && engine_properties::accepts_value(property, &value)
                })
            {
                app.engine_property_values.insert(key, value);
            }
        }
        Message::EnginePropertyTextChanged(key, value) => {
            if app.engine_properties_open
                && app.engine_properties.iter().any(|property| {
                    property.key == key
                        && matches!(property.kind, engine_properties::EnginePropertyKind::Color)
                })
            {
                app.engine_property_values
                    .insert(key, serde_json::Value::String(value));
            }
        }
        Message::ApplyEngineProperties => app.apply_engine_properties(),
        Message::OpenLocalTags(item) => {
            if !app.visible
                || app.exit_requested
                || app.window.is_none()
                || app.panel_state.settings_open
                || app.playlist_panel_open
                || app.engine_properties_open
                || app.controls_panel_open
                || app.folder_menu_open
                || !app
                    .selected
                    .and_then(|i| app.wallpapers.get(i))
                    .is_some_and(|w| w.id == item)
            {
                return Task::none();
            }
            app.invalidate_tag_editor();
            app.tag_drawer_item = Some(item);
            app.panel_state.search_open = true;
            app.reconcile_tag_editor();
            return iced::widget::operation::focus(app.local_tag_input_id.clone());
        }
        Message::BackToSearchFilters => {
            app.tag_drawer_item = None;
            app.invalidate_tag_editor();
            return iced::widget::operation::focus(app.search_input_id.clone());
        }
        Message::LocalTag { owner, edit } => {
            app.edit_local_tag(owner, edit);
        }
        Message::FolderPathChanged(path) => app.folder_path_input = path,
        Message::AddLocalRoot => app.add_local_root(),
        Message::UnlistLocalRoot(path) => {
            let mut config = app.config.clone();
            config.catalog.local_folders.retain(|p| p != &path);
            app.publish_library_config(config);
        }
        Message::SetInstalledDiscovery(enabled) => {
            let mut config = app.config.clone();
            config.behavior.include_default_projects = enabled;
            app.publish_library_config(config);
        }
        Message::SearchChanged(query) => {
            app.reset_nonsticky_folder();
            app.filter_rule.text = query;
            app.refresh_filter();
            app.schedule_posters();
        }
        Message::ToggleSearchTags => {
            app.search_tags_open = !app.search_tags_open;
            return iced::widget::operation::focus(app.search_input_id.clone());
        }
        Message::ToggleTagSort => {
            app.search_tags_sort_az = !app.search_tags_sort_az;
            return iced::widget::operation::focus(app.search_input_id.clone());
        }
        Message::SearchTag(tag, exclude) => {
            app.reset_nonsticky_folder();
            let (selected, opposite) = if exclude {
                (
                    &mut app.filter_rule.excluded_tags,
                    &mut app.filter_rule.tags,
                )
            } else {
                (
                    &mut app.filter_rule.tags,
                    &mut app.filter_rule.excluded_tags,
                )
            };
            opposite.remove(&tag);
            if !selected.remove(&tag) {
                selected.insert(tag);
            }
            app.refresh_filter();
            app.schedule_posters();
            return iced::widget::operation::focus(app.search_input_id.clone());
        }
        Message::SetTagMatchAny(any) => {
            app.reset_nonsticky_folder();
            app.filter_rule.tags_match_any = any;
            app.refresh_filter();
            app.schedule_posters();
            return iced::widget::operation::focus(app.search_input_id.clone());
        }
        Message::ClearSearchTags => {
            app.filter_rule.tags.clear();
            app.filter_rule.excluded_tags.clear();
            app.refresh_filter();
            app.schedule_posters();
            return iced::widget::operation::focus(app.search_input_id.clone());
        }
        Message::SetSort(order) => {
            if app.sort_order != order {
                app.sort_order = order;
                app.refresh_filter();
                app.schedule_posters();
            }
        }
        message @ (Message::CycleSort | Message::CycleSortBack) => {
            let reverse = matches!(message, Message::CycleSortBack);
            let order = SortOrder::ALL
                .into_iter()
                .filter(|o| o.visible(&app.state.frontend_preferences.visibility))
                .collect::<Vec<_>>();
            if order.is_empty() {
                return Task::none();
            }
            let index = order.iter().position(|o| *o == app.sort_order).unwrap_or(0);
            app.sort_order =
                order[(index + if reverse { order.len() - 1 } else { 1 }) % order.len()];

            app.refresh_filter();
            app.schedule_posters();
        }
        Message::ToggleFolderMenu => app.folder_menu_open = !app.folder_menu_open,
        Message::DismissFolderMenu => {
            if !app.panel_state.settings_open {
                app.folder_menu_open = false;
            }
        }
        Message::SetFolderFilter(folder) => {
            app.browse_playlist_id = None;
            app.browse_playlist_unavailable = false;
            app.filter_rule.folders.clear();
            if let Some(folder) = folder {
                app.filter_rule
                    .folders
                    .insert(catalog::folder_filter_path(folder));
            }
            app.folder_menu_open = false;
            app.refresh_filter();
            app.schedule_posters();
        }
        Message::CycleResolution | Message::ClearResolution => {
            app.reset_nonsticky_folder();
            let next = if matches!(message, Message::ClearResolution)
                || app.filter_rule.minimum_width != 0
                || app.filter_rule.minimum_height != 0
            {
                None
            } else {
                let mut seen = std::collections::BTreeSet::new();
                let applicable: Vec<_> = app
                    .state
                    .frontend_preferences
                    .resolution_presets
                    .iter()
                    .filter(|preset| preset.matches_orientation(app.filter_rule.orientation))
                    // Keep every editable record, but cycle each normalized predicate once.
                    .filter(|preset| seen.insert(preset.range_key()))
                    .collect();
                let next = match &app.filter_rule.resolution_preset {
                    None => applicable.first().copied(),
                    Some(active) => applicable
                        .iter()
                        .position(|preset| preset.same_range(active))
                        .and_then(|index| applicable.get(index + 1))
                        .copied(),
                };
                next.cloned()
            };
            app.filter_rule.resolution_preset = next;
            app.filter_rule.minimum_width = 0;
            app.filter_rule.minimum_height = 0;
            app.refresh_filter();
            app.schedule_posters();
        }
        Message::CycleOrientation => {
            app.reset_nonsticky_folder();
            app.filter_rule.orientation = match app.filter_rule.orientation {
                OrientationFilter::Any => OrientationFilter::Landscape,
                OrientationFilter::Landscape => OrientationFilter::Portrait,
                OrientationFilter::Portrait => OrientationFilter::Square,
                OrientationFilter::Square => OrientationFilter::Any,
            };
            app.refresh_filter();
            app.schedule_posters();
        }
        Message::ToggleFavoritesFilter => {
            app.reset_nonsticky_folder();
            app.filter_rule.favorites_only = !app.filter_rule.favorites_only;
            app.refresh_filter();
            app.schedule_posters();
        }
        Message::TogglePlaylistChooser => {
            if !app.visible || app.exit_requested || app.window.is_none() {
                return Task::none();
            }
            let opening = !app.playlist_panel_open || !app.playlist_chooser;
            if app.playlist_panel_open {
                app.playlist_panel_open = false;
                app.reconcile_playlist_editor();
            }
            if opening {
                let _ = update_inner(app, Message::TogglePlaylistPanel);
                app.playlist_chooser = true;
                app.playlist_query.clear();
                app.reconcile_playlist_editor();
                return iced::widget::operation::focus(iced::widget::Id::new(
                    PLAYLIST_CHOOSER_QUERY_ID,
                ));
            }
        }
        Message::PlaylistBrowse(owner, action) => app.browse_playlist(owner, action),
        Message::TogglePlaylistPanel => {
            let was_theme_owner = theme_owner_active(app);
            app.playlist_picker = false;
            app.playlist_chooser = false;
            app.playlist_panel_open = !app.playlist_panel_open;
            if app.playlist_panel_open {
                app.engine_properties_open = false;
                app.folder_menu_open = false;
                app.panel_state.search_open = false;
                app.panel_state.settings_open = false;
                app.controls_panel_open = false;
            }
            cancel_theme_transient_if_departed(app, was_theme_owner);
        }
        Message::OpenPlaylistPicker => {
            if !detail_controls_active(app) {
                return Task::none();
            }
            let task = update_inner(app, Message::TogglePlaylistPanel);
            app.playlist_picker = app.playlist_panel_open;
            return task;
        }
        Message::PlaylistEdit(owner, edit) => app.edit_playlist(owner, edit),
        Message::HideGallery => return app.set_visible(false, "hide-gallery"),
        Message::PlaylistNameChanged(name) => app.playlist_name = name,
        Message::CreatePlaylist { smart } => app.create_playlist(smart),
        Message::TogglePlaylistMembership(playlist_id) => {
            if let Some(item) = app
                .selected
                .filter(|i| app.filtered_indices.contains(i))
                .and_then(|i| app.wallpapers.get(i))
                .map(|w| w.id.clone())
            {
                let included = app.state.playlists.iter().find(|p| p.id == playlist_id)
                    .is_some_and(|p| matches!(&p.membership, PlaylistMembership::Explicit { wallpaper_ids } if wallpaper_ids.contains(&item)));
                app.set_playlist_membership(&playlist_id, &item, !included);
            }
        }
        Message::TogglePlaylistOrder(playlist_id) => {
            let mut proposed = app.state.clone();
            if let Some(playlist) = proposed
                .playlists
                .iter_mut()
                .find(|playlist| playlist.id == playlist_id)
            {
                playlist.order = match playlist.order {
                    PlaybackOrder::Sequential => PlaybackOrder::Shuffle,
                    PlaybackOrder::Shuffle => PlaybackOrder::Sequential,
                };
                let _ = app.commit_local_state_change(proposed);
            }
        }
        Message::BrowsePlaylist(playlist_id) => {
            if app.state.playlists.iter().any(|p| p.id == playlist_id) {
                app.playlist_browsing_id = Some(playlist_id);
            }
        }
        Message::SetActivePlaylist(playlist_id, generation) => {
            if generation != app.automation_generation.load(Ordering::Acquire) {
                return Task::none();
            }
            app.start_playlist(playlist_id, unix_now());
        }
        Message::CycleAppearancePreset if app.state.applied_theme.is_some() => {}
        Message::CycleAppearancePreset => {
            let next = match app.state.appearance.preset.as_str() {
                "catppuccin-mocha" => (
                    "rose-pine-moon",
                    "#232136",
                    "#2a273f",
                    "#e0def4",
                    "#6e6a86",
                    "#eb6f92",
                ),
                "rose-pine-moon" => (
                    "nord", "#2e3440", "#3b4252", "#eceff4", "#7b88a1", "#88c0d0",
                ),
                _ => (
                    "catppuccin-mocha",
                    "#1e1e2e",
                    "#313244",
                    "#cdd6f4",
                    "#6c7086",
                    "#f5c2e7",
                ),
            };
            app.state.appearance.preset = next.0.into();
            app.state.appearance.background = next.1.into();
            app.state.appearance.surface = next.2.into();
            app.state.appearance.text = next.3.into();
            app.state.appearance.muted = next.4.into();
            app.state.appearance.accent = next.5.into();
            app.persist_state();
        }
        Message::AppearanceColorChanged(..) if app.state.applied_theme.is_some() => {}
        Message::AppearanceColorChanged(field, value) => {
            match field {
                AppearanceField::Background => app.state.appearance.background = value,
                AppearanceField::Surface => app.state.appearance.surface = value,
                AppearanceField::Text => app.state.appearance.text = value,
                AppearanceField::Muted => app.state.appearance.muted = value,
                AppearanceField::Accent => app.state.appearance.accent = value,
            }
            app.state.appearance.preset = "custom".into();
            app.persist_state();
        }
        Message::CycleAppearanceOpacity => {
            app.state.appearance.overlay_opacity = match app.state.appearance.overlay_opacity {
                value if value < 0.63 => 0.70,
                value if value < 0.76 => 0.82,
                value if value < 0.89 => 0.95,
                _ => 0.55,
            };
            app.persist_state();
        }
        Message::SetBackgroundPresentation(presentation) => {
            if app.state.appearance.presentation != presentation {
                app.state.appearance.presentation = presentation;
                app.refresh_backdrop();
                app.persist_state();
            }
        }
        // Settings > Theme session family. Every arm is owner-guarded
        // (visible + Settings open on the Theme category) and the
        // reducer re-checks draft revision / session token, so stale
        // or off-page callbacks change nothing. Theme transitions
        // never persist state, call providers/coordinators, or touch
        // the saved legacy appearance.
        Message::ThemeDesigner(action) => return app.theme_action(action),
        Message::ThemeWheel {
            editor_instance,
            selection_generation,
            role,
            variant,
            action,
        } => {
            if theme_owner_active(app) {
                app.theme_session.designer_wheel(
                    editor_instance,
                    selection_generation,
                    role,
                    variant,
                    action,
                );
            }
        }
        Message::ThemeRecent {
            editor_instance,
            selection_generation,
            role,
            variant,
            color,
        } => {
            if theme_owner_active(app) {
                app.theme_session.designer_color(
                    editor_instance,
                    selection_generation,
                    role,
                    variant,
                    &color,
                );
            }
        }
        Message::ThemeRoleSelected {
            editor_instance,
            role,
        } => {
            if theme_owner_active(app) {
                app.theme_session.editor_select_role(editor_instance, role);
            }
        }
        Message::ThemeVariantSelected {
            editor_instance,
            variant,
        } => {
            if theme_owner_active(app) {
                app.theme_session
                    .editor_select_variant(editor_instance, variant);
            }
        }
        Message::ThemeRoleFilterChanged {
            editor_instance,
            text,
        } => {
            if theme_owner_active(app) {
                app.theme_session.editor_set_filter(editor_instance, text);
            }
        }
        Message::ThemeRoleTextChanged {
            editor_instance,
            selection_generation,
            role,
            variant,
            text,
        } => {
            if theme_owner_active(app) {
                app.theme_session.editor_type_text(
                    editor_instance,
                    selection_generation,
                    role,
                    variant,
                    text,
                );
            }
        }
        Message::ThemeRoleSetColor {
            editor_instance,
            selection_generation,
            role,
            variant,
        } => {
            if theme_owner_active(app) {
                if app.theme_session.designer_active() {
                    app.theme_session.designer_set_hex(
                        editor_instance,
                        selection_generation,
                        role,
                        variant,
                    );
                } else {
                    app.theme_session.editor_set_color(
                        editor_instance,
                        selection_generation,
                        role,
                        variant,
                    );
                }
            }
        }
        Message::ThemeRoleResetColor {
            editor_instance,
            selection_generation,
            role,
            variant,
        } => {
            if theme_owner_active(app) {
                app.theme_session.editor_reset_color(
                    editor_instance,
                    selection_generation,
                    role,
                    variant,
                );
            }
        }
        Message::ToggleEnginePaused => {
            app.submit_engine_command(EngineCommand::SetPaused(!app.state.global_paused));
        }
        Message::ToggleEngineMuted => {
            app.submit_engine_command(EngineCommand::SetMuted(!app.state.global_muted));
        }
        Message::CycleEngineVolume => {
            let volume = match app.state.global_volume {
                76..=100 => 75,
                51..=75 => 50,
                26..=50 => 25,
                1..=25 => 0,
                _ => 100,
            };
            app.submit_engine_command(EngineCommand::SetVolume(volume));
        }
        Message::ToggleRandomRotation => {
            let enabled = !app.state.automation_intent().random_enabled;
            if app.publish_random_rotation(enabled, unix_now()) {
                let started = std::time::Instant::now();
                app.rotation_notice = Some((started, app.random_rotation_state()));
                let (send, receive) = futures_channel::oneshot::channel();
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_secs(2));
                    let _ = send.send(());
                });
                return Task::perform(receive, move |_| Message::ExpireRotationNotice(started));
            }
        }
        Message::ExpireRotationNotice(started) => {
            if app.rotation_notice.is_some_and(|(id, _)| id == started) {
                app.rotation_notice = None;
            }
        }
        Message::ResumeRandomRotation => {
            app.set_random_rotation(true, unix_now());
        }
        Message::StopPlaylist { generation } => {
            if generation == app.automation_generation.load(Ordering::Acquire) {
                app.stop_playlist(unix_now());
            }
        }
        Message::CycleSlideshowInterval => {
            let interval = match app.state.slideshow_interval_seconds {
                0..=599 => 10 * 60,
                600..=1799 => 30 * 60,
                1800..=3599 => 60 * 60,
                3600..=21_599 => 6 * 60 * 60,
                _ => 10 * 60,
            };
            app.set_automation_interval(interval, unix_now());
        }
        Message::ScheduleNameChanged(name) => app.schedule_name = name,
        Message::ScheduleTimeChanged(time) => app.schedule_time = time,
        Message::ToggleScheduleDay(day) if day < 7 => app.schedule_days ^= 1 << day,
        Message::ToggleScheduleDay(_) => {}
        Message::ToggleScheduleMonitor(device_id) => {
            if !app.schedule_monitors.remove(&device_id) {
                app.schedule_monitors.insert(device_id);
            }
        }
        Message::ToggleMonitorLock(device_id) => {
            if !app.state.locked_monitors.remove(&device_id) {
                app.state.locked_monitors.insert(device_id);
            }
            app.persist_state();
        }
        Message::CreateSchedule => {
            let name = app.schedule_name.trim().to_string();
            let playlist_id = app.state.active_playlist_id.clone();
            let minute = parse_schedule_time(&app.schedule_time);
            match (name.is_empty(), playlist_id, minute, app.schedule_days) {
                (false, Some(playlist_id), Some(minute_of_day), days) if days != 0 => {
                    let id = unique_schedule_id(&name, &app.state.schedules);
                    let creation_order = app
                        .state
                        .schedules
                        .iter()
                        .map(|schedule| schedule.creation_order)
                        .max()
                        .unwrap_or(0)
                        .saturating_add(1);
                    app.state.schedules.push(candidate_state::Schedule {
                        id,
                        name,
                        enabled: true,
                        days,
                        minute_of_day,
                        priority: 100,
                        creation_order,
                        playlist_id,
                        monitor_ids: app.schedule_monitors.iter().cloned().collect(),
                    });
                    app.schedule_name.clear();
                    app.persist_state();
                    app.refresh_automation_timer();
                }
                _ => {
                    app.status = Some(
                        "A schedule needs a name, valid HH:MM time, at least one day, and an active playlist."
                            .into(),
                    );
                }
            }
        }
        Message::ToggleSchedule(schedule_id) => {
            if let Some(schedule) = app
                .state
                .schedules
                .iter_mut()
                .find(|schedule| schedule.id == schedule_id)
            {
                schedule.enabled = !schedule.enabled;
                app.persist_state();
                app.refresh_automation_timer();
            }
        }
        Message::CycleTargetMonitor => app.cycle_target_monitor(),
        Message::KeyPressed {
            key,
            modifiers,
            event,
        } => {
            if !app.visible || app.window != Some(event.event_window) {
                return Task::none();
            }

            if !app.engine_properties_open
                && !app.playlist_panel_open
                && !view::settings::gateway::controls_owns_input(app)
                && !view::settings::gateway::folder_owns_input(app)
                && (!app.panel_state.search_open || app.panel_state.settings_open)
            {
                if let Some(task) = view::settings::gateway::route_key(
                    app,
                    &key,
                    modifiers,
                    event.event_window,
                    false,
                ) {
                    return task;
                }
            }
            if matches_candidate_key(&key, modifiers, "ctrl+f") {
                return update(app, Message::OpenSearch);
            }
            let is_escape = matches_candidate_key(&key, modifiers, "escape");
            if is_escape {
                if app.engine_properties_open {
                    let was_theme_owner = theme_owner_active(app);
                    app.toggle_engine_properties();
                    cancel_theme_transient_if_departed(app, was_theme_owner);
                    return Task::none();
                }
                if app.playlist_panel_open {
                    if app.playlist_delete.take().is_some() {
                        app.playlist_session = app.playlist_session.wrapping_add(1);
                        return Task::none();
                    }
                    app.engine_properties_open = false;
                    app.folder_menu_open = false;
                    app.playlist_panel_open = false;
                    return Task::none();
                }
                // Search draws above these retained lower panels. Custom close
                // keeps its existing, separate top_owned_sheet action order.
                if app.panel_state.search_open && !app.panel_state.settings_open {
                    let action = app.panel_state.escape(app.detail_open);
                    debug_assert_eq!(action, EscapeAction::CloseSearch);
                    return Task::none();
                }
                if view::settings::gateway::folder_owns_input(app) {
                    app.folder_menu_open = false;
                    return Task::none();
                }
                if view::settings::gateway::controls_owns_input(app) {
                    app.controls_panel_open = false;
                    return Task::none();
                }
            }
            let keybinds = &app.config.keybinds;
            if matches_candidate_key(&key, modifiers, &keybinds.close) {
                if !is_escape {
                    if let Some(sheet) = top_owned_sheet(
                        app.panel_state,
                        app.playlist_panel_open,
                        app.engine_properties_open,
                        view::settings::gateway::controls_owns_input(app),
                        view::settings::gateway::folder_owns_input(app),
                    ) {
                        close_owned_sheet(app, sheet);
                        return Task::none();
                    }
                }
                let was_theme_owner = theme_owner_active(app);
                match if is_escape {
                    app.panel_state.escape(app.detail_open)
                } else {
                    gallery_close_action(app.detail_open)
                } {
                    EscapeAction::CloseSearch => {}
                    EscapeAction::CloseSettings => {
                        cancel_theme_transient_if_departed(app, was_theme_owner)
                    }
                    EscapeAction::CloseDetail => app.start_detail(false, "escape-close-detail"),
                    EscapeAction::HideGallery => {
                        return app.set_visible(false, "escape-hide-gallery")
                    }
                }
            } else if sheet_owns_keyboard(
                app.panel_state,
                app.playlist_panel_open,
                app.engine_properties_open,
                app.controls_panel_open,
                app.folder_menu_open,
            ) {
                return Task::none();
            } else if matches_candidate_key(&key, modifiers, &keybinds.apply_to_all) {
                app.apply_selected(true);
            } else if matches_candidate_key(&key, modifiers, &keybinds.switch_monitor) {
                app.cycle_target_monitor();
            } else if matches_candidate_key(&key, modifiers, &keybinds.toggle_exclude) {
                return update(app, Message::ToggleExcluded);
            } else if matches_candidate_key(&key, modifiers, &keybinds.toggle_hidden) {
                return update(app, Message::ToggleShowExcluded);
            } else if matches_candidate_key(&key, modifiers, &keybinds.open_in_explorer) {
                return update(app, Message::OpenFolder);
            } else if matches_candidate_key(&key, modifiers, &keybinds.reload_config) {
                let loaded_config = Config::load_read_only_with_diagnostics();
                app.config = loaded_config.config;
                app.config_load_diagnostic = loaded_config.diagnostic;
                app.library_config_reconcile = false;
                if let Some(diagnostic) = &app.config_load_diagnostic {
                    app.status = Some(diagnostic.clone());
                }
                app.show_excluded = app.config.behavior.show_excluded;
                app.refresh_catalog();
            } else if let Some(action) = super::lifecycle::input_key(&key).and_then(|id| {
                super::lifecycle::effective_input_bindings(app).lookup_key(
                    &id,
                    super::lifecycle::input_modifiers(modifiers),
                    view::settings::composed::input::ActiveScopes::default(),
                )
            }) {
                return dispatch_local_input(app, action, Some((&key, modifiers, &event)), None);
            } else if matches_candidate_key(&key, modifiers, "1") {
                app.set_gallery_mode(GalleryMode::Slices);
            } else if matches_candidate_key(&key, modifiers, "2") {
                app.set_gallery_mode(GalleryMode::Geometric);
            } else if matches_candidate_key(&key, modifiers, "3") {
                app.set_gallery_mode(GalleryMode::Wall);
            } else if matches_candidate_key(&key, modifiers, "4") {
                app.set_gallery_mode(GalleryMode::Depth);
            }
        }
    }
    app.resolve_depth_detail();
    Task::none()
}

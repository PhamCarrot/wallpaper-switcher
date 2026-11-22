use super::super::*;
use super::donor_palette;
use super::sheets::{folder_popup_model, folder_popup_view, search_filter_content};
use crate::{appearance_theme, candidate_state, donor_folio, donor_ui, gpu_gallery, theme_roles};
use iced::widget::column;
pub(in crate::candidate_app) mod bridge;
pub(in crate::candidate_app) mod composed;
pub(in crate::candidate_app) mod controller;
pub(in crate::candidate_app) mod folio;
pub(in crate::candidate_app) mod gateway;
pub(in crate::candidate_app) mod geometry_projection;
pub(in crate::candidate_app) mod pending_forms;
pub(in crate::candidate_app) mod pending_projection;
pub(in crate::candidate_app) mod preference_projection;
pub(in crate::candidate_app) mod profile_projection;
pub(in crate::candidate_app) mod theme_designer;

pub(super) fn gallery_picker_content(app: &GpuCandidate) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    use super::sheets::{panel_button, panel_text};
    let palette = donor_palette(app);
    column![
        panel_text("Gallery", scale),
        row![
            panel_button(panel_text("Slices", scale), palette, scale)
                .on_press(Message::SetGalleryMode(GalleryMode::Slices))
                .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
            panel_button(panel_text("Geometric", scale), palette, scale)
                .on_press(Message::SetGalleryMode(GalleryMode::Geometric))
                .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
            panel_button(panel_text("Depth", scale), palette, scale)
                .on_press(Message::SetGalleryMode(GalleryMode::Depth))
                .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
            panel_button(panel_text("Wall", scale), palette, scale)
                .on_press(Message::SetGalleryMode(GalleryMode::Wall))
                .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
        ]
        .spacing(8.0 * scale),
        panel_text(
            format!("Current gallery · {}", app.gallery_mode.label()),
            scale
        ),
    ]
    .spacing(8.0 * scale)
    .into()
}

fn gallery_number_control(app: &GpuCandidate, field: GalleryNumberField) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    use super::sheets::{panel_button, panel_text};
    let palette = donor_palette(app);
    let value = gallery_number_value(&app.state, field);
    let step = field.step();
    let suffix = match field {
        GalleryNumberField::DepthHeight => " px (100–1600)",
        GalleryNumberField::DepthWidth => " px (40–2400)",
        GalleryNumberField::DepthSpacing => " px (0–4000)",
        GalleryNumberField::DepthCount => " items (3–21)",
        GalleryNumberField::DepthFalloff => "  (0–1)",
        GalleryNumberField::DepthCorners => " px (0–100)",
        GalleryNumberField::DepthSkew => " px (-200–200)",
        GalleryNumberField::DepthNavigation => " ms (35–8000)",
        GalleryNumberField::DepthFlipDuration => " ms (100–5000)",
        _ => "",
    };
    let shown = if value.fract().abs() < f32::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    };
    row![
        panel_text(field.label(), scale).width(Fill),
        panel_button(panel_text("−", scale), palette, scale)
            .on_press(Message::SetGalleryNumber(field, value - step))
            .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
        panel_text(format!("{shown}{suffix}"), scale).width(if suffix.is_empty() {
            64
        } else {
            180
        }),
        panel_button(panel_text("+", scale), palette, scale)
            .on_press(Message::SetGalleryNumber(field, value + step))
            .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
    ]
    .spacing(6.0 * scale)
    .align_y(iced::Alignment::Center)
    .into()
}

pub(in crate::candidate_app) fn position_content(app: &GpuCandidate) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    use super::sheets::{panel_button, panel_text};
    let palette = donor_palette(app);
    let mut presets = row![panel_text("Presets:", scale)].spacing(6.0 * scale);
    for preset in app.state.mode_profiles.rows(app.state.gallery.mode) {
        let mode = app.state.gallery.mode;
        let id = preset.id;
        presets = presets
            .push(
                panel_button(
                    panel_text(
                        if preset.available {
                            preset.name.to_owned()
                        } else {
                            format!("{} · Legacy Depth snapshot unavailable", preset.name)
                        },
                        scale,
                    ),
                    palette,
                    scale,
                )
                .on_press_maybe(
                    preset
                        .available
                        .then_some(Message::ApplyGalleryPreset(mode, id)),
                )
                .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
            )
            .push(
                panel_button(panel_text("×", scale), palette, scale)
                    .on_press(Message::DeleteGalleryPreset(mode, id))
                    .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
            );
    }
    let mut content = column![
        panel_button(
            panel_text(
                format!(
                    "Arrangement · {}",
                    app.state.gallery.variant.map_or_else(
                        || "CUSTOM".to_string(),
                        |variant| app.gallery_mode.variant_label(variant as usize).to_string(),
                    )
                ),
                scale
            ),
            palette,
            scale
        )
        .on_press(Message::CycleGalleryVariant)
        .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
        panel_button(panel_text("Save preset", scale), palette, scale)
            .on_press(Message::SaveGalleryPreset)
            .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
    ]
    .spacing(8.0 * scale);

    match app.gallery_mode {
        GalleryMode::Depth => {
            content = column![
                panel_button(panel_text("Depth Default", scale), palette, scale)
                    .on_press(Message::DepthDefault),
                panel_button(panel_text("Save preset", scale), palette, scale)
                    .on_press(Message::SaveGalleryPreset)
            ]
            .spacing(8.0 * scale);
            for field in DEPTH_POSITION_FIELDS {
                content = content.push(gallery_number_control(app, field));
            }
            content = content.push(row![
                panel_text("Selection frame", scale).width(Fill),
                checkbox(app.state.gallery.depth.selection_frame).on_toggle(Message::SetDepthFrame)
            ]);
            content = content.push(row![
                panel_text("Shadows", scale).width(Fill),
                checkbox(app.state.gallery.depth.shadows).on_toggle(Message::SetDepthShadows)
            ]);
        }
        GalleryMode::Slices => {
            content = content.push(panel_text("Slice geometry", scale));
            for field in [
                GalleryNumberField::SliceVisibleCount,
                GalleryNumberField::SliceWidth,
                GalleryNumberField::SliceExpandedWidth,
                GalleryNumberField::SliceHeight,
                GalleryNumberField::SliceSpacing,
                GalleryNumberField::SliceSkewDistance,
                GalleryNumberField::SliceEdgeTilt,
                GalleryNumberField::SliceCornerRadius,
            ] {
                content = content.push(gallery_number_control(app, field));
            }
            content = content.push(
                row![
                    panel_text("Wobble", scale).width(Fill),
                    checkbox(app.state.gallery.slices.wobble).on_toggle(Message::SetSliceWobble),
                ]
                .align_y(iced::Alignment::Center),
            );
            if app.state.gallery.slices.wobble {
                content = content.push(gallery_number_control(
                    app,
                    GalleryNumberField::SliceWobbleStrength,
                ));
            }
        }
        GalleryMode::Geometric => {
            content = content.push(panel_text("Tile geometry", scale));
            for field in [
                GalleryNumberField::GeometricRadius,
                GalleryNumberField::GeometricRows,
                GalleryNumberField::GeometricColumns,
                GalleryNumberField::GeometricGapX,
                GalleryNumberField::GeometricGapY,
            ] {
                content = content.push(gallery_number_control(app, field));
            }
            if app.state.gallery.geometric.curve != StateGeometricCurve::Flat {
                content = content.push(gallery_number_control(
                    app,
                    GalleryNumberField::GeometricCurveStrength,
                ));
            }
            if app.state.gallery.geometric.curve == StateGeometricCurve::Wave {
                content = content.push(gallery_number_control(
                    app,
                    GalleryNumberField::GeometricCurveFrequency,
                ));
            }
            if app.state.gallery.geometric.curve == StateGeometricCurve::Cylinder {
                content = content.push(gallery_number_control(
                    app,
                    GalleryNumberField::GeometricOrbitRadius,
                ));
            }
            content = content.push(
                panel_button(panel_text("Cycle curve", scale), palette, scale)
                    .on_press(Message::CycleGalleryGeometry)
                    .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
            );
        }
        GalleryMode::Wall => {
            content = content.push(panel_text("Wall layout", scale));
            let mut layouts = row![].spacing(5.0 * scale);
            for (layout, label) in wall_layout_choices() {
                let label = if app.state.gallery.wall.layout == layout {
                    format!("✓ {label}")
                } else {
                    label.to_string()
                };
                layouts = layouts.push(
                    panel_button(panel_text(label, scale), palette, scale)
                        .on_press(Message::SetWallLayout(layout))
                        .style(move |_, status| {
                            donor_ui::action_style(palette, false, false, status)
                        }),
                );
            }
            content = content.push(layouts);
            for field in [
                GalleryNumberField::WallColumns,
                GalleryNumberField::WallRows,
                GalleryNumberField::WallCardWidth,
                GalleryNumberField::WallCardHeight,
                GalleryNumberField::WallGapX,
                GalleryNumberField::WallGapY,
                GalleryNumberField::WallCornerRadius,
            ] {
                content = content.push(gallery_number_control(app, field));
            }
            if matches!(
                app.state.gallery.wall.layout,
                StateWallLayout::Brick | StateWallLayout::Editorial
            ) {
                content =
                    content.push(gallery_number_control(app, GalleryNumberField::WallStagger));
            }
            if app.state.gallery.wall.layout == StateWallLayout::Cylinder {
                content = content
                    .push(gallery_number_control(
                        app,
                        GalleryNumberField::WallCylinderBend,
                    ))
                    .push(gallery_number_control(
                        app,
                        GalleryNumberField::WallCylinderRadius,
                    ));
            }
        }
    }

    content = content.push(
        panel_button(
            panel_text("Cycle arrangement variation", scale),
            palette,
            scale,
        )
        .on_press(Message::CycleGalleryVariation)
        .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
    );

    if app.gallery_mode == GalleryMode::Geometric {
        let mut curves = row![panel_text("Curve", scale)].spacing(6.0 * scale);
        for (curve, label) in geometric_curve_choices() {
            let label = if app.state.gallery.geometric.curve == curve {
                format!("✓ {label}")
            } else {
                label.to_string()
            };
            curves = curves.push(
                panel_button(panel_text(label, scale), palette, scale)
                    .on_press(Message::SetGeometricCurve(curve))
                    .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
            );
        }

        let mut shapes = row![panel_text("Tiles", scale)].spacing(6.0 * scale);
        for (shape, label) in geometric_shape_choices() {
            let label = if app.state.gallery.geometric.shape == shape {
                format!("✓ {label}")
            } else {
                label.to_string()
            };
            shapes = shapes.push(
                panel_button(panel_text(label, scale), palette, scale)
                    .on_press(Message::SetGeometricShape(shape))
                    .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
            );
        }
        content = content.push(curves).push(shapes);
    }

    content = content.push(
        if app
            .state
            .mode_profiles
            .rows(app.state.gallery.mode)
            .is_empty()
        {
            row![panel_text("No saved presets", scale)]
        } else {
            presets
        },
    );
    content.into()
}

pub(super) fn windows_display_content(app: &GpuCandidate) -> Element<'_, Message> {
    use super::sheets::{panel_button, panel_text};
    use composed::{SettingsSource, KEY_BINDINGS};
    let scale = bridge::effective_ui_scale(app);
    let palette = donor_palette(app);
    let tr = |key| app.settings_locale.text_static(key);
    let map = bridge::Source { app }.bindings();
    let mut native = column![panel_button(
        panel_text(tr("settings-windows-open-config"), scale),
        palette,
        scale
    )
    .on_press_maybe(
        (!app.launch_policy.is_preview())
            .then_some(Message::Wake(Wake::Tray(TrayAction::OpenConfig)))
    )
    .style(move |_, status| donor_ui::action_style(palette, false, false, status))]
    .spacing(8.0 * scale);
    for diagnostic in &map.native_diagnostics {
        native = native.push(panel_text(diagnostic.clone(), scale));
    }
    for descriptor in KEY_BINDINGS
        .iter()
        .filter(|entry| entry.action.local_available())
    {
        for reserved in map.reserved_conflicts(map.triggers(descriptor.action)) {
            let mut args = fluent::FluentArgs::new();
            args.set("action", tr(descriptor.title_key));
            args.set("binding", reserved);
            native = native.push(panel_text(
                app.settings_locale
                    .format("settings-windows-reserved-conflict", Some(&args))
                    .expect("verified Windows translation arguments"),
                scale,
            ));
        }
    }
    column![
        donor_folio::section(
            tr("r5-windows-target-title"),
            tr("r5-windows-target-help"),
            display_settings_content(app),
            palette,
            scale
        ),
        donor_folio::section(
            tr("settings-windows-native-config"),
            tr("settings-windows-native-config-desc"),
            native.into(),
            palette,
            scale
        ),
    ]
    .spacing(24.0 * scale)
    .into()
}

fn display_settings_content(app: &GpuCandidate) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    use super::sheets::{panel_button, panel_text};
    let palette = donor_palette(app);
    let active = app
        .active_monitor_id
        .as_deref()
        .unwrap_or("No display selected");
    let mut locks = column![panel_text(
        app.settings_locale.text_static("r5-panel-copy-11"),
        scale
    )]
    .spacing(6.0 * scale);
    for device_id in &app.monitor_ids {
        let is_locked = app.state.locked_monitors.contains(device_id);
        locks = locks.push(
            panel_button(
                panel_text(
                    format!("{} {device_id}", if is_locked { "Unlock" } else { "Lock" }),
                    scale,
                ),
                palette,
                scale,
            )
            .on_press(Message::ToggleMonitorLock(device_id.clone()))
            .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
        );
    }
    column![
        panel_text(format!("Active display · {active}"), scale),
        panel_button(
            panel_text(app.settings_locale.text_static("r5-panel-copy-12"), scale),
            palette,
            scale
        )
        .on_press(Message::CycleTargetMonitor)
        .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
        locks,
    ]
    .spacing(8.0 * scale)
    .into()
}

pub(in crate::candidate_app) fn motion_settings_content(
    app: &GpuCandidate,
) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    let palette = donor_palette(app);
    use super::sheets::{panel_button, panel_text};
    {
        let d = app.state.card_flip();
        let mut effects = column![panel_text("Flip effect", scale)].spacing(4.0 * scale);
        for name in candidate_state::DEPTH_EFFECTS {
            effects = effects.push(
                panel_button(panel_text(name, scale), palette, scale)
                    .on_press(Message::SetDepthEffect(name.into()))
                    .style(move |_, status| {
                        donor_ui::action_style(donor_palette(app), d.effect == name, false, status)
                    }),
            );
        }
        let mut content=column![
            panel_text("Shared gallery speed", scale),
            panel_button(panel_text(app.state.gallery_motion_speed.label(), scale), palette, scale).on_press(Message::CycleGalleryMotionSpeed),
            panel_text("Shared flip settings apply to Slices, Hex, Wall and Depth. External travel uses Standard motion.", scale),
            effects,
            row![panel_text("Animate shader", scale).width(Fill), checkbox(d.animate_shader).on_toggle(Message::SetDepthShader)],
            row![panel_text("Animate back reveal", scale).width(Fill), checkbox(d.animate_back).on_toggle(Message::SetDepthBack)],
            gallery_number_control(app, GalleryNumberField::DepthFlipDuration),
        ].spacing(8.0 * scale);
        if app.gallery_mode == GalleryMode::Depth {
            content = content.push(gallery_number_control(
                app,
                GalleryNumberField::DepthNavigation,
            ));
        }
        return content.into();
    }
}

pub(super) fn playback_settings_content(app: &GpuCandidate) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    use super::sheets::{panel_button, panel_text};
    let palette = donor_palette(app);
    column![
        panel_text(app.settings_locale.text_static("r5-panel-copy-13"), scale)
            .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
        row![
            panel_button(
                panel_text(app.random_rotation_state().label(), scale),
                palette,
                scale
            )
            .on_press(Message::ToggleRandomRotation)
            .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
            panel_button(
                panel_text(
                    format!("Every {} sec", app.state.slideshow_interval_seconds),
                    scale
                ),
                palette,
                scale
            )
            .on_press(Message::CycleSlideshowInterval)
            .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
            panel_button(
                panel_text(app.settings_locale.text_static("r5-playlists"), scale),
                palette,
                scale
            )
            .on_press(Message::TogglePlaylistPanel)
            .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
        ]
        .spacing(8.0 * scale),
        row![
            panel_button(panel_text("Stop playlist", scale), palette, scale).on_press_maybe(
                app.playlist_playing().then_some(Message::StopPlaylist {
                    generation: app.automation_generation.load(Ordering::Acquire)
                })
            ),
            panel_button(panel_text("Resume random rotation", scale), palette, scale)
                .on_press_maybe(
                    (app.random_rotation_state()
                        == candidate_state::RandomRotationState::SafelyStopped)
                        .then_some(Message::ResumeRandomRotation)
                ),
        ]
        .spacing(8.0 * scale)
        .wrap(),
        panel_text(app.automation_policy_status(), scale),
        panel_text("Wallpaper Engine", scale),
        panel_text(app.settings_locale.text_static("r5-panel-copy-17"), scale)
            .size(12.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
        row![
            panel_button(
                panel_text(
                    if app.state.global_paused {
                        app.settings_locale.text_static("r5-panel-copy-18")
                    } else {
                        app.settings_locale.text_static("r5-panel-copy-19")
                    },
                    scale
                ),
                palette,
                scale
            )
            .on_press_maybe((!app.apply_pending).then_some(Message::ToggleEnginePaused))
            .style(move |_, status| donor_ui::action_style(
                palette,
                app.state.global_paused,
                false,
                status
            )),
            panel_button(
                panel_text(
                    if app.state.global_muted {
                        app.settings_locale.text_static("r5-panel-copy-20")
                    } else {
                        app.settings_locale.text_static("r5-panel-copy-21")
                    },
                    scale
                ),
                palette,
                scale
            )
            .on_press_maybe((!app.apply_pending).then_some(Message::ToggleEngineMuted))
            .style(move |_, status| donor_ui::action_style(
                palette,
                app.state.global_muted,
                false,
                status
            )),
            panel_button(
                panel_text(format!("Volume {}%", app.state.global_volume), scale),
                palette,
                scale
            )
            .on_press_maybe((!app.apply_pending).then_some(Message::CycleEngineVolume))
            .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
        ]
        .spacing(8.0 * scale),
    ]
    .spacing(8.0 * scale)
    .into()
}

fn library_settings_content(app: &GpuCandidate) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    use super::sheets::{panel_button, panel_text};
    let palette = donor_palette(app);
    let prefs = &app.state.frontend_preferences.library;
    let (watch_status, fallback) = app
        .catalog_watch
        .as_ref()
        .map(|o| o.status())
        .unwrap_or(("Watch owner unavailable".into(), false));
    let catalog = row![
        panel_text(format!("{} wallpapers", app.wallpapers.len()), scale)
            .size(12.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
            .width(Fill),
        panel_button(
            panel_text(app.settings_locale.text_static("r5-catalog-refresh"), scale),
            palette,
            scale
        )
        .on_press(Message::RefreshCatalog)
    ]
    .spacing(8.0 * scale);
    let folder_label = app
        .filter_rule
        .folders
        .iter()
        .next()
        .and_then(|folder| folder.file_name())
        .map_or_else(
            || {
                app.settings_locale
                    .text_static("r5-all-folders")
                    .to_string()
            },
            |name| name.to_string_lossy().into_owned(),
        );
    let mut source_filters = row![].spacing(6.0 * scale);
    for source_filter in [
        SourceFilter::All,
        SourceFilter::Pictures,
        SourceFilter::Videos,
        SourceFilter::WallpaperEngine,
    ] {
        source_filters = source_filters.push(
            panel_button(panel_text(source_filter.label(), scale), palette, scale)
                .on_press(Message::SetSourceFilter(source_filter))
                .style(move |_, status| {
                    donor_ui::action_style(
                        palette,
                        app.source_filter == source_filter,
                        false,
                        status,
                    )
                }),
        );
    }
    let library_toolbar = column![
        text_input("Search wallpapers", &app.filter_rule.text)
            .font(UI_FONT)
            .size(12.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
            .padding([8.0 * scale, 10.0 * scale])
            .style(move |_, _| donor_ui::ghost_input_style(palette))
            .id(app.search_input_id.clone())
            .on_input(Message::SearchChanged)
            .width(Fill),
        row![
            source_filters,
            panel_button(
                panel_text(format!("Folder · {folder_label} ⌄"), scale),
                palette,
                scale
            )
            .on_press(Message::ToggleFolderMenu)
            .style(move |_, status| donor_ui::action_style(
                palette,
                !app.filter_rule.folders.is_empty(),
                false,
                status
            )),
            panel_button(
                panel_text(
                    if app.filter_rule.favorites_only {
                        "✓ Favorites"
                    } else {
                        "Favorites"
                    },
                    scale
                ),
                palette,
                scale
            )
            .on_press(Message::ToggleFavoritesFilter)
            .style(move |_, status| {
                donor_ui::action_style(palette, app.filter_rule.favorites_only, false, status)
            })
        ]
        .spacing(8.0 * scale)
        .wrap(),
    ]
    .spacing(8.0 * scale);
    let mut catalog_tools = column![catalog, library_toolbar].spacing(10.0 * scale);
    if app.folder_menu_open {
        catalog_tools = catalog_tools.push(folder_popup_view(folder_popup_model(app)));
    }
    let mut content = column![
        panel_text(watch_status, scale)
            .size(12.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
        row![
            panel_text(
                if prefs.watching() {
                    app.settings_locale.text_static("r5-panel-copy-22")
                } else {
                    app.settings_locale.text_static("r5-panel-copy-23")
                },
                scale
            )
            .width(Fill),
            panel_button(
                panel_text(
                    if prefs.watching() {
                        app.settings_locale.text_static("r5-panel-copy-24")
                    } else {
                        app.settings_locale.text_static("r5-panel-copy-25")
                    },
                    scale
                ),
                palette,
                scale
            )
            .on_press(Message::SetLibraryWatching(!prefs.watching()))
        ]
        .spacing(8.0 * scale),
        panel_text(
            if prefs.automatic_watch.is_none() {
                app.settings_locale.text_static("r5-panel-copy-26")
            } else {
                app.settings_locale.text_static("r5-panel-copy-27")
            },
            scale
        )
        .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
    ]
    .spacing(8.0 * scale);
    if fallback {
        content = content.push(
            row![
                panel_text(
                    format!("Fallback interval: {} s (advanced)", prefs.interval()),
                    scale
                )
                .width(Fill),
                panel_button(panel_text("−15 s", scale), palette, scale).on_press(
                    Message::SetLibraryPolling(prefs.interval().saturating_sub(15).max(1))
                ),
                panel_button(panel_text("+15 s", scale), palette, scale).on_press(
                    Message::SetLibraryPolling((prefs.interval() + 15).min(3600))
                )
            ]
            .spacing(8.0 * scale),
        );
    }
    let mut refresh = panel_button(
        panel_text(app.settings_locale.text_static("r5-panel-copy-29"), scale),
        palette,
        scale,
    );
    let mut clear = panel_button(
        panel_text(app.settings_locale.text_static("r5-panel-copy-30"), scale),
        palette,
        scale,
    );
    if app.cache_operation.is_none() {
        refresh = refresh.on_press(Message::RefreshThumbnails);
        clear = clear.on_press(Message::ClearPosterCache);
    }
    column![
        donor_folio::section(
            app.settings_locale.text_static("feedback-catalog"),
            "",
            catalog_tools.into(),
            palette,
            scale
        ),
        donor_folio::section(
            app.settings_locale.text_static("repair-library-watching"),
            "",
            content.into(),
            palette,
            scale
        ),
        donor_folio::section(
            app.settings_locale.text_static("repair-library-cache"),
            app.settings_locale.text_static("r5-panel-copy-31"),
            column![
                row![refresh, clear].spacing(8.0 * scale).wrap(),
                panel_text(app.cache_status.clone(), scale)
                    .size(12.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
            ]
            .spacing(12.0 * scale)
            .into(),
            palette,
            scale
        ),
        donor_folio::section(
            app.settings_locale.text_static("repair-library-colour"),
            "",
            panel_text(
                if app.colours.status.is_empty() {
                    format!(
                        "Colour information: {} known · {} unavailable{}",
                        app.colours.metadata.len(),
                        app.colours.unavailable,
                        if app.colours.active {
                            " · analysing"
                        } else {
                            ""
                        }
                    )
                } else {
                    app.colours.status.clone()
                },
                scale
            )
            .into(),
            palette,
            scale
        )
    ]
    .spacing(28.0 * scale)
    .into()
}

pub(super) fn library_platform_form(app: &GpuCandidate) -> Element<'_, Message> {
    column![folder_ledger(app), library_settings_content(app)]
        .spacing(28.0 * bridge::effective_ui_scale(app))
        .into()
}

fn folder_ledger(app: &GpuCandidate) -> Element<'_, Message> {
    use super::sheets::{panel_button, panel_text};
    let scale = bridge::effective_ui_scale(app);
    let palette = donor_palette(app);
    let mut content = column![
        panel_text(
            format!("Local folders ({})", app.config.catalog.local_folders.len()),
            scale
        ),
        iced::widget::rule::horizontal(1)
    ]
    .spacing(6.0 * scale);
    for path in &app.config.catalog.local_folders {
        content = content
            .push(
                row![
                    panel_text(
                        format!(
                            "{}{}",
                            path.display(),
                            if path.is_dir() { "" } else { " · unavailable" }
                        ),
                        scale
                    )
                    .width(Fill),
                    panel_button(
                        panel_text(app.settings_locale.text_static("r5-panel-copy-37"), scale),
                        palette,
                        scale
                    )
                    .on_press(Message::UnlistLocalRoot(path.clone()))
                ]
                .align_y(iced::Alignment::Center)
                .padding([8.0 * scale, 0.0])
                .spacing(8.0 * scale),
            )
            .push(iced::widget::rule::horizontal(1));
    }
    content
        .push(
            column![
                row![
                    text_input(
                        app.settings_locale.text_static("r5-panel-copy-34"),
                        &app.folder_path_input
                    )
                    .font(UI_FONT)
                    .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                    .padding([7.0 * scale, 10.0 * scale])
                    .style(move |_, _| donor_ui::ghost_input_style(palette))
                    .on_input(Message::FolderPathChanged)
                    .on_submit(Message::AddLocalRoot)
                    .width(Fill),
                    panel_button(
                        panel_text(app.settings_locale.text_static("r5-panel-copy-35"), scale),
                        palette,
                        scale
                    )
                    .on_press(Message::AddLocalRoot)
                ]
                .spacing(8.0 * scale),
                checkbox(app.config.behavior.include_default_projects)
                    .text_size(11)
                    .size(14.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                    .label(app.settings_locale.text_static("r5-panel-copy-36"))
                    .on_toggle(Message::SetInstalledDiscovery),
            ]
            .spacing(10.0 * scale),
        )
        .into()
}

fn source_settings_content(app: &GpuCandidate) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    use super::sheets::{panel_button, panel_text};
    let palette = donor_palette(app);
    let local_paths = app
        .config
        .catalog
        .local_folders
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join("; ");
    let mut sources = row![].spacing(6.0 * scale);
    for source_filter in [
        SourceFilter::All,
        SourceFilter::Pictures,
        SourceFilter::Videos,
        SourceFilter::WallpaperEngine,
    ] {
        sources = sources.push(
            panel_button(panel_text(source_filter.label(), scale), palette, scale)
                .on_press(Message::SetSourceFilter(source_filter))
                .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
        );
    }
    let folder_label = app
        .filter_rule
        .folders
        .iter()
        .next()
        .and_then(|folder| folder.file_name())
        .map_or_else(
            || {
                app.settings_locale
                    .text_static("r5-all-folders")
                    .to_string()
            },
            |name| name.to_string_lossy().into_owned(),
        );
    let mut content = column![
        panel_text(
            format!("Config: {}", Config::config_path().display()),
            scale
        ),
        panel_text(format!("Local folders: {local_paths}"), scale),
        panel_text(
            format!(
                "Installed projects: {}",
                if app.config.behavior.include_default_projects {
                    "default installed Workshop discovery enabled"
                } else {
                    "default discovery disabled"
                }
            ),
            scale
        ),
        panel_text(app.settings_locale.text_static("r5-panel-copy-33"), scale),
        folder_ledger(app),
        sources,
        panel_button(
            panel_text(format!("Folder · {folder_label} ⌄"), scale),
            palette,
            scale
        )
        .on_press(Message::ToggleFolderMenu)
        .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
    ]
    .spacing(8.0 * scale);
    if app.folder_menu_open {
        content = content.push(folder_popup_view(folder_popup_model(app)));
    }
    content.into()
}

fn automation_settings_content(app: &GpuCandidate) -> Element<'_, Message> {
    automation_settings_content_with_palette(app, donor_palette(app))
}

pub(super) fn automation_settings_content_with_palette(
    app: &GpuCandidate,
    palette: donor_ui::Palette,
) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    use super::sheets::{panel_button, panel_text};
    let active_playlist = app.active_schedule_playlist();
    let active_name = active_playlist.map_or("No active playlist", |p| p.name.as_str());
    let mut days = row![].spacing(5.0 * scale);
    for (day, label) in ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"]
        .into_iter()
        .enumerate()
    {
        let active = app.schedule_days & (1 << day) != 0;
        days = days.push(
            panel_button(
                panel_text(label, scale)
                    .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
                palette,
                scale,
            )
            .on_press(Message::ToggleScheduleDay(day as u8))
            .padding([6.0 * scale, 9.0 * scale])
            .style(move |_, status| donor_ui::action_style(palette, active, false, status)),
        );
    }
    let mut targets =
        column![
            panel_text(app.settings_locale.text_static("r5-panel-copy-38"), scale)
                .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
        ]
        .spacing(9.0 * scale);
    for device_id in &app.monitor_ids {
        let active = app.schedule_monitors.contains(device_id);
        targets = targets.push(
            panel_button(
                panel_text(
                    format!("{} {}", if active { "✓" } else { "○" }, device_id),
                    scale,
                )
                .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                .width(Fill),
                palette,
                scale,
            )
            .width(Fill)
            .padding([7.0 * scale, 10.0 * scale])
            .on_press(Message::ToggleScheduleMonitor(device_id.clone()))
            .style(move |_, status| donor_ui::line_style(palette, active, false, status)),
        );
    }
    let form = column![
        panel_text(format!("Playlist / {active_name}"), scale)
            .size(14.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
        row![
            text_input(
                app.settings_locale.text_static("r5-panel-copy-39"),
                &app.schedule_name
            )
            .font(UI_FONT)
            .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
            .padding([7.0 * scale, 10.0 * scale])
            .style(move |_, _| donor_ui::ghost_input_style(palette))
            .on_input(Message::ScheduleNameChanged)
            .width(Fill)
            .padding([7.0 * scale, 10.0 * scale])
            .style(move |_, _| donor_ui::field_input_style(palette)),
            text_input("HH:MM", &app.schedule_time)
                .font(UI_FONT)
                .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                .padding([7.0 * scale, 10.0 * scale])
                .style(move |_, _| donor_ui::ghost_input_style(palette))
                .on_input(Message::ScheduleTimeChanged)
                .width(100.0 * scale)
                .padding([7.0 * scale, 10.0 * scale])
                .style(move |_, _| donor_ui::field_input_style(palette))
        ]
        .spacing(14.0 * scale),
        days,
        targets,
        panel_button(
            panel_text(app.settings_locale.text_static("r5-panel-copy-40"), scale),
            palette,
            scale
        )
        .on_press_maybe(
            (!app.schedule_name.trim().is_empty()
                && parse_schedule_time(&app.schedule_time).is_some()
                && active_playlist.is_some()
                && app.schedule_days != 0)
                .then_some(Message::CreateSchedule)
        )
        .style(move |_, status| donor_ui::action_style(palette, true, false, status))
    ]
    .spacing(14.0 * scale);
    let mut schedules = column![].spacing(14.0 * scale);
    if app.state.schedules.is_empty() {
        schedules = schedules.push(
            panel_text(app.settings_locale.text_static("r5-panel-copy-41"), scale)
                .size(13.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
        );
    }
    for schedule in &app.state.schedules {
        let enabled = schedule.enabled;
        schedules = schedules.push(
            row![
                column![
                    panel_text(&schedule.name, scale)
                        .size(14.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
                    panel_text(
                        format!(
                            "{:02}:{:02} / priority {} / {}",
                            schedule.minute_of_day / 60,
                            schedule.minute_of_day % 60,
                            schedule.priority,
                            schedule.playlist_id
                        ),
                        scale
                    )
                    .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                    .color(donor_ui::with_alpha_color(palette.surface_text, 0.56))
                ]
                .spacing(5.0 * scale)
                .width(Fill),
                panel_button(
                    panel_text(
                        if enabled {
                            app.settings_locale.text_static("r5-panel-copy-42")
                        } else {
                            app.settings_locale.text_static("r5-panel-copy-43")
                        },
                        scale
                    ),
                    palette,
                    scale
                )
                .on_press(Message::ToggleSchedule(schedule.id.clone()))
                .style(move |_, status| donor_ui::action_style(palette, enabled, false, status))
            ]
            .spacing(14.0 * scale),
        );
    }
    column![
        donor_folio::section(
            app.settings_locale.text_static("r5-panel-copy-44"),
            app.settings_locale.text_static("r5-panel-copy-45"),
            form.into(),
            palette,
            scale
        ),
        donor_folio::section(
            app.settings_locale.text_static("r5-panel-copy-46"),
            "",
            schedules.into(),
            palette,
            scale
        ),
    ]
    .spacing(28.0 * scale)
    .into()
}

/// The Settings > Theme role-editor section, rendered only while
/// the session is in role-edit mode. All callbacks are constructed
/// with the session's current editor instance / selection generation
/// / typed role / variant bindings, and the reducer re-validates
/// them, so queued stale events change nothing.
pub(in crate::candidate_app) fn theme_role_editor_content(
    app: &GpuCandidate,
) -> Element<'_, Message> {
    theme_designer::wheel_editor(app)
}

pub(in crate::candidate_app) fn theme_settings_content(app: &GpuCandidate) -> Element<'_, Message> {
    theme_designer::content(app)
}

fn settings_category_content(app: &GpuCandidate) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    match app.panel_state.settings_category {
        SettingsCategory::Picker => gallery_picker_content(app),
        SettingsCategory::CatalogFilters => search_filter_content(app),
        SettingsCategory::Position => position_content(app),
        SettingsCategory::WallpapersDisplays => display_settings_content(app),
        SettingsCategory::Motion => motion_settings_content(app),
        SettingsCategory::Playback => playback_settings_content(app),
        SettingsCategory::Library => column![
            donor_folio::section(
                app.settings_locale.text_static("r5-library-paths"),
                "",
                folder_ledger(app),
                donor_palette(app),
                scale
            ),
            library_settings_content(app)
        ]
        .spacing(28.0 * scale)
        .into(),
        SettingsCategory::Sources => source_settings_content(app),
        SettingsCategory::Automation => automation_settings_content(app),
        SettingsCategory::Theme => theme_settings_content(app),
    }
}

pub(in crate::candidate_app) fn settings_category_matches(
    category: SettingsCategory,
    query: &str,
) -> bool {
    let query = query.trim().to_ascii_lowercase();
    query.is_empty()
        || format!(
            "{} {} {}",
            category.label(),
            category.description(),
            category.search_terms()
        )
        .to_ascii_lowercase()
        .contains(&query)
}

fn supported_settings_rows(category: SettingsCategory) -> &'static [(&'static str, &'static str)] {
    match category {
        SettingsCategory::Picker => &[
            ("Gallery", "1 Slices, 2 Geometric, 3 Wall, 4 Depth"),
            ("Arrangement", "Select a supported gallery arrangement"),
            (
                "Blurred wallpaper / Live desktop-through",
                "Picker background appearance and overlay opacity",
            ),
        ],
        SettingsCategory::CatalogFilters => &[
            ("Search wallpapers", "Search the local catalog"),
            ("Pictures", "Show local image items"),
            ("Videos", "Show supported local video items"),
            ("Wallpaper Engine", "Show installed Engine items"),
            ("Folders", "Limit the catalog to selected folders"),
            ("Favorites", "Show only favorite items"),
            ("Sort", "Change catalog ordering"),
        ],
        SettingsCategory::Position => &[
            ("Rows and columns", "Control gallery density"),
            ("Card width and height", "Control poster dimensions"),
            ("Gaps and spacing", "Control card spacing"),
            ("Corner radius", "Control poster corners"),
            ("Orbit radius", "Control Geometric Cylinder radius"),
            ("Presets", "Save and restore supported arrangements"),
        ],
        SettingsCategory::WallpapersDisplays => &[
            ("Display", "Select the active display"),
            ("Monitor locks", "Lock displays out of automation"),
            ("Apply target", "Choose the explicit target display"),
        ],
        SettingsCategory::Motion => &[
            ("Gallery motion", "Enable supported gallery motion"),
            ("Motion speed", "Choose the filter and detail speed"),
            ("Wobble", "Control the Slices wobble"),
        ],
        SettingsCategory::Playback => &[
            ("Slideshow", "Configure the optional slideshow"),
            ("Wallpaper Engine pause", "Pause or resume Engine playback"),
            ("Wallpaper Engine mute", "Mute or unmute Engine playback"),
            ("Wallpaper Engine volume", "Set the global Engine volume"),
        ],
        SettingsCategory::Library => &[
            ("Refresh catalog", "Scan configured sources"),
            ("Favorites and playlists", "Organize local catalog items"),
        ],
        SettingsCategory::Sources => &[
            ("Local folders", "Choose local catalog roots"),
            ("Pictures and videos", "Choose supported local sources"),
            ("Wallpaper Engine", "Discover installed Engine projects"),
        ],
        SettingsCategory::Automation => &[
            ("Schedules", "Configure supported schedules"),
            ("Monitor locks", "Lock displays out of automation"),
            ("Schedule targets", "Choose schedule display targets"),
        ],
        SettingsCategory::Theme => &[("Theme designer", "Edit and save named Theme documents")],
    }
}

pub(in crate::candidate_app) fn settings_search_hits(
    query: &str,
) -> Vec<(SettingsCategory, &'static str, &'static str)> {
    let query = query.trim().to_ascii_lowercase();
    if query.is_empty() {
        return Vec::new();
    }
    SettingsCategory::VISIBLE
        .into_iter()
        .flat_map(|category| {
            let query = query.clone();
            supported_settings_rows(category)
                .iter()
                .copied()
                .filter_map(move |(label, description)| {
                    let haystack = format!(
                        "{} {} {} {}",
                        category.label(),
                        category.description(),
                        label,
                        description
                    )
                    .to_ascii_lowercase();
                    haystack
                        .contains(&query)
                        .then_some((category, label, description))
                })
        })
        .collect()
}

pub(super) fn settings_panel_view(
    app: &GpuCandidate,
    palette: donor_ui::Palette,
) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    let mut tree = column![text_input("Search settings", &app.settings_search)
        .on_input(Message::SettingsSearchChanged)
        .padding([7.0 * scale, 6.0 * scale])
        .size(13.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
        .style(move |_, _| donor_ui::ghost_input_style(palette))]
    .spacing(17.0 * scale);
    let mut entries = column![].spacing(1.0 * scale);
    if app.settings_search.trim().is_empty() {
        for category in SettingsCategory::VISIBLE {
            let selected = category == app.panel_state.settings_category;
            entries = entries.push(
                button(
                    row![
                        text(if selected { "▾" } else { "▸" })
                            .size(9.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
                        text(category.label()).size(12.5 * scale)
                    ]
                    .spacing(9.0 * scale)
                    .align_y(iced::Alignment::Center),
                )
                .width(Fill)
                .padding([8.0 * scale, 8.0 * scale])
                .on_press(Message::SetSettingsCategory(category))
                .style(move |_, status| donor_ui::line_style(palette, selected, false, status)),
            );
            if selected {
                entries = entries.push(
                    container(
                        text(category.description())
                            .size(10.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                            .color(donor_ui::with_alpha_color(palette.surface_text, 0.56)),
                    )
                    .padding([8.0 * scale, 18.0 * scale]),
                );
            }
        }
    } else {
        let hits = settings_search_hits(app.settings_search.trim());
        if hits.is_empty() {
            entries = entries.push(
                text("No supported settings match this search.")
                    .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
            );
        }
        for (category, label, description) in hits.into_iter().take(12) {
            entries = entries.push(
                button(
                    column![
                        text(label)
                            .size(12.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
                        text(format!("{} / {description}", category.label()))
                            .size(9.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                            .color(palette.primary)
                    ]
                    .spacing(3.0 * scale),
                )
                .width(Fill)
                .padding([9.0 * scale, 8.0 * scale])
                .on_press(Message::SetSettingsCategory(category))
                .style(move |_, status| donor_ui::line_style(palette, false, false, status)),
            );
        }
    }
    let tracked: Element<'_, Message> = if app.settings_search.trim().is_empty() {
        let active = SettingsCategory::VISIBLE
            .iter()
            .position(|category| *category == app.panel_state.settings_category)
            .unwrap_or(0);
        row![
            scrollable(entries).height(Fill).width(Fill),
            donor_folio::index_rail(active, SettingsCategory::VISIBLE.len(), palette, scale)
        ]
        .spacing(9.0 * scale)
        .height(Fill)
        .into()
    } else {
        scrollable(entries).height(Fill).into()
    };
    tree = tree.push(tracked);
    let index = donor_folio::index(
        "Settings",
        app.panel_state.settings_category.description(),
        tree.into(),
        palette,
        scale,
    );
    let category = app.panel_state.settings_category;
    let page = donor_folio::page(
        category.label(),
        category.description(),
        settings_category_content(app),
        Some(Message::ToggleSettingsPanel),
        None,
        palette,
        donor_folio::BlueprintKind::Settings,
        scale,
    );
    donor_folio::sheet(
        app.viewport,
        Some(index),
        page,
        palette,
        Message::SheetBackdrop,
        None,
        scale,
    )
}

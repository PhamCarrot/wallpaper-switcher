use iced::widget::column;
pub(super) mod settings;
mod sheets;
#[cfg(test)]
pub(super) use sheets::{
    folder_popup_model, folder_popup_size, folder_popup_view as composed_folder_popup_view,
};

use super::*;
use crate::{
    coordinator, donor_back_layout, donor_back_view, donor_detail, donor_layout, donor_rail,
    donor_scene, donor_ui, gpu_gallery,
};
use settings::composed::SettingsSource;
use settings::{gallery_picker_content, settings_panel_view};
pub(super) use settings::{motion_settings_content, position_content};
#[cfg(test)]
pub(super) use settings::{
    settings_category_matches, settings_search_hits, theme_role_editor_content,
    theme_settings_content,
};
use sheets::{engine_property_sheet, folder_popup_view, playlist_panel_view, search_panel_view};

pub(super) fn detail_chrome_visibility(mode: GalleryMode, detail_open: bool) -> (bool, bool) {
    // The external BackPanel is a modal source-to-target presentation. 's
    // top rail intersects its pinned masthead at normal viewport sizes, so
    // suspend that rail until close; never move or shrink the donor body.
    // The pinned composition also suppresses library hints during detail.
    (
        !(detail_open && !matches!(mode, GalleryMode::Slices | GalleryMode::Depth)),
        false,
    )
}

/// Application appearance comes only from a successfully published full
/// document snapshot. A designer audition never becomes application authority.
/// Legacy states with no snapshot retain their existing appearance fallback.
fn applied_theme_colors(
    app: &GpuCandidate,
) -> Option<crate::appearance_theme::EffectiveThemeColors> {
    app.state.applied_theme.as_ref().map(|snapshot| {
        crate::appearance_theme::EffectiveThemeColors::from_document(
            &snapshot.document,
            snapshot.variant,
        )
    })
}

pub(super) fn donor_palette(app: &GpuCandidate) -> donor_ui::Palette {
    if let Some(colors) = applied_theme_colors(app) {
        return colors.palette();
    }
    let defaults = GalleryColors::default();
    let background = gpu_gallery::parse_hex_rgb(
        &app.state.appearance.background,
        [
            0x1e as f32 / 255.0,
            0x1e as f32 / 255.0,
            0x2e as f32 / 255.0,
        ],
    );
    let surface = gpu_gallery::parse_hex_rgb(&app.state.appearance.surface, defaults.surface);
    let text = gpu_gallery::parse_hex_rgb(
        &app.state.appearance.text,
        [
            0xcd as f32 / 255.0,
            0xd6 as f32 / 255.0,
            0xf4 as f32 / 255.0,
        ],
    );
    let accent = gpu_gallery::parse_hex_rgb(&app.state.appearance.accent, defaults.accent);
    donor_ui::Palette::from_rgb(background, surface, text, accent)
}

/// Production gallery snapshot colors used by `view`'s actual
/// `GallerySnapshot` assembly. With a persisted applied snapshot the
/// surface/accent come from the document's surface/primary roles; all
/// other fields are the existing config-driven values, unchanged.
pub(super) fn gallery_snapshot_colors(app: &GpuCandidate) -> GalleryColors {
    let effective = applied_theme_colors(app);
    GalleryColors {
        surface: match effective {
            Some(colors) => colors.surface,
            None => gpu_gallery::parse_hex_rgb(
                &app.state.appearance.surface,
                GalleryColors::default().surface,
            ),
        },
        accent: match effective {
            Some(colors) => colors.primary,
            None => gpu_gallery::parse_hex_rgb(
                &app.state.appearance.accent,
                GalleryColors::default().accent,
            ),
        },
        unselected_brightness: if app.config.ui.dim_unselected {
            (1.0 - app.config.style.dim_unselected).clamp(0.0, 1.0)
        } else {
            1.0
        },
        selection_border_width: app.config.style.border_width,
        show_selection_border: app.config.ui.show_selection_border,
        border_glow: app.config.style.border_glow,
        border_glow_radius: app.config.style.glow_radius,
    }
}

/// Production rail palette conversion used by `view`'s `RailModel`
/// assembly: every rail color comes from the (possibly
/// applied-document) folio palette, faded by the smoothstep reveal
/// factor `reveal * reveal * (3 - 2 * reveal)`. Retain the source's
/// container/variant and on-primary roles independently.
pub(super) fn rail_palette(
    palette: donor_ui::Palette,
    rail_reveal: f32,
) -> donor_rail::RailPalette {
    let reveal_alpha = rail_reveal * rail_reveal * (3.0 - 2.0 * rail_reveal);
    donor_rail::RailPalette {
        background: donor_ui::with_alpha_color(palette.background, reveal_alpha),
        surface: donor_ui::with_alpha_color(palette.surface, reveal_alpha),
        container: donor_ui::with_alpha_color(palette.surface_container, reveal_alpha),
        hover: donor_ui::with_alpha_color(palette.surface_variant, reveal_alpha),
        active_text: donor_ui::with_alpha_color(palette.primary_text, reveal_alpha),
        text: donor_ui::with_alpha_color(palette.surface_text, reveal_alpha),
        accent: donor_ui::with_alpha_color(palette.primary, reveal_alpha),
        outline: donor_ui::with_alpha_color(palette.outline, reveal_alpha),
    }
}

/// Production backdrop tint background used by `view`: the effective
/// document background when a persisted snapshot exists, otherwise the
/// saved legacy background with its existing render fallback. The
/// legacy overlay opacity is applied by the caller, unchanged.
pub(super) fn backdrop_background_rgb(app: &GpuCandidate) -> [f32; 3] {
    if let Some(colors) = applied_theme_colors(app) {
        return colors.background;
    }
    gpu_gallery::parse_hex_rgb(
        &app.state.appearance.background,
        [
            0x1e as f32 / 255.0,
            0x1e as f32 / 255.0,
            0x2e as f32 / 255.0,
        ],
    )
}

pub(super) fn backdrop_click_dismisses(app: &GpuCandidate) -> bool {
    app.state.appearance.presentation == BackgroundPresentation::WallpaperBackdrop
        && app.config.ui.close_on_backdrop_click
}

pub(super) fn application_style(app: &GpuCandidate, theme: &Theme) -> iced::theme::Style {
    let mut style = iced::theme::default(theme);
    if app.state.appearance.presentation == BackgroundPresentation::LiveTransparentOverlay {
        style.background_color = Color::TRANSPARENT;
    }
    style
}

pub(super) fn rail_chip_style(
    mode: GalleryMode,
    status: button::Status,
    selected: bool,
    surface_rgb: [f32; 3],
    accent_rgb: [f32; 3],
    text_rgb: [f32; 3],
) -> button::Style {
    let palette = donor_ui::Palette::from_rgb(
        GalleryColors::default().surface,
        surface_rgb,
        text_rgb,
        accent_rgb,
    );
    let mut style = donor_ui::rail_style(palette, selected, status);
    // Iced's stock button exposes per-corner radii, so keep each gallery's
    // rail silhouette distinct without replacing its accessible button.
    style.border.radius = match mode {
        GalleryMode::Slices | GalleryMode::Depth => iced::border::Radius {
            top_left: 0.0,
            top_right: 12.0,
            bottom_right: 0.0,
            bottom_left: 12.0,
        },
        GalleryMode::Geometric => iced::border::Radius::new(3.0),
        GalleryMode::Wall => iced::border::Radius::default(),
    };
    style
}

pub(super) fn gallery_rail_model(app: &GpuCandidate) -> donor_rail::RailModel {
    let folio_palette = donor_palette(app);
    let rail_mode = match app.gallery_mode {
        GalleryMode::Slices | GalleryMode::Depth => donor_rail::RailMode::Slices,
        GalleryMode::Geometric => donor_rail::RailMode::Geometric,
        GalleryMode::Wall => donor_rail::RailMode::Wall,
    };
    let source = settings::bridge::Source { app };
    let rail_mode = match source.text("filterBar.visualStyle").as_str() {
        "slices" => donor_rail::RailMode::Slices,
        "hex" => donor_rail::RailMode::Geometric,
        "wall" => donor_rail::RailMode::Wall,
        _ => rail_mode,
    };
    let visibility = donor_rail::RailVisibility {
        sources: ["all", "static", "video", "we"]
            .map(|kind| source.flag_default_true(&format!("filterBar.show.type.{kind}"))),
        orientation: source.flag_default_true("filterBar.show.orient"),
        resolution: app.resolution_active()
            || (source.flag_default_true("filterBar.show.resolution")
                && app
                    .state
                    .frontend_preferences
                    .resolution_presets
                    .iter()
                    .any(|preset| preset.matches_orientation(app.filter_rule.orientation))),
        folder: source.flag_default_true("filterBar.show.folder")
            && (!app.catalog.is_empty()
                || !app.config.catalog.local_folders.is_empty()
                || !app.filter_rule.folders.is_empty()),
        sort: source.flag_default_true("filterBar.show.sort"),
        favorites: source.flag_default_true("filterBar.show.favourites"),
        random: source.flag_default_true("filterBar.show.random"),
        playlists: source.flag_default_true("filterBar.show.playlists"),
        colors: source.flag_default_true("filterBar.show.colors"),
        tagcloud: source.flag_default_true("filterBar.show.tagcloud"),
        mute: source.flag_default_true("filterBar.show.mute"),
        search: source.flag_default_true("filterBar.show.search"),
        settings: source.flag_default_true("filterBar.show.settings"),
    };
    let reveal = if app.rail_dismissed {
        0.0
    } else if source.flag(settings::composed::keys::general::FILTER_BAR_ALWAYS_VISIBLE) {
        1.0
    } else {
        app.rail_reveal
    };
    let source_selected = match app.source_filter {
        SourceFilter::All => 0,
        SourceFilter::Pictures => 1,
        SourceFilter::Videos => 2,
        SourceFilter::WallpaperEngine => 3,
    };
    let folder_label = match app.filter_rule.folders.len() {
        0 => "ALL WALLPAPERS".to_owned(),
        1 => {
            let folder = app.filter_rule.folders.iter().next().unwrap();
            folder.file_name().map_or_else(
                || folder.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            )
        }
        count => format!("{count} FOLDERS"),
    };
    let vertical = (settings::bridge::Source { app }).text("filterBar.orientation") == "vertical";
    donor_rail::RailModel {
        visibility,
        reveal,
        viewport: app.viewport,
        max_width: (app.viewport[0] - if vertical { 32.0 } else { 24.0 }).max(1.0),
        vertical,
        max_height: (app.viewport[1] - 32.0).max(1.0),
        scale: settings::bridge::effective_ui_scale(app),
        rtl: app.settings_locale.language.rtl,
        resolution_label: app.resolution_label(),
        sort_order: app.sort_order,
        sort_visibility: SortOrder::ALL
            .map(|order| order.visible(&app.state.frontend_preferences.visibility)),
        folder_active: !app.filter_rule.folders.is_empty(),
        folder_open: app.folder_menu_open,
        mode: rail_mode,
        title: None,
        source_labels: ["ALL".into(), "PIC".into(), "VID".into(), "WE".into()],
        source_selected,
        shape_label: shape_chip_label(app.filter_rule.orientation).into(),
        shape_active: app.filter_rule.orientation != OrientationFilter::Any,
        folder_label,
        favorites_active: app.filter_rule.favorites_only,
        random_state: app.random_rotation_state(),
        muted: app.state.global_muted,
        playlists_active: app.playlist_panel_open,
        search_active: app.panel_state.search_open
            || !app.filter_rule.text.is_empty()
            || !app.filter_rule.tags.is_empty()
            || !app.filter_rule.excluded_tags.is_empty(),
        settings_active: app.panel_state.settings_open,
        bucket: app.colours.query.bucket,
        show_close: app.config.ui.show_close_button
            && source.flag_default_true("filterBar.show.close"),
        palette: rail_palette(folio_palette, reveal),
        font: UI_FONT,
        icon_font: NERD_FONT,
    }
}

/// Source chrome.rs orientation branches, using the same current Wall frame as draw/hits.
pub(super) fn gallery_rail_anchor(
    app: &GpuCandidate,
    model: &donor_rail::RailModel,
    gallery_height: f32,
) -> [f32; 2] {
    let gallery_height = match app.gallery_mode {
        GalleryMode::Slices => app
            .gallery_presentation
            .as_ref()
            .map_or(gallery_height, |p| p.slices.height),
        GalleryMode::Geometric => app.hex_presentation.as_ref().map_or(gallery_height, |p| {
            if p.circle {
                2.0 * p.params.r * p.params.rows as f32
                    + p.params.gap_y * p.params.rows.saturating_sub(1) as f32
            } else {
                p.params.content_h()
            }
        }),
        GalleryMode::Depth | GalleryMode::Wall => gallery_height,
    };
    let size = donor_rail::footprint(model);
    let rtl = app.settings_locale.language.rtl;
    // Source scene composition uses motion scale1; chrome uses UI scale separately.
    let layout_gap = 8.0 * model.scale;
    let mut at = if app.gallery_mode == GalleryMode::Wall {
        let p = gallery_layout_params_for_app(app).wall;
        let frame = gpu_gallery::wall_frame(app.viewport, p);
        if model.vertical {
            let width =
                p.card_width * p.columns as f32 + p.gap_x * p.columns.saturating_sub(1) as f32;
            [
                if rtl {
                    frame.center[0] + width * 0.5 + ((p.rail_gap + layout_gap) * 0.5)
                } else {
                    frame.left - ((p.rail_gap + layout_gap) * 0.5) - size[0]
                },
                frame.top,
            ]
        } else {
            [
                (app.viewport[0] - size[0]) * 0.5,
                frame.top - ((p.rail_gap + layout_gap) * 0.5) - size[1],
            ]
        }
    } else if model.vertical {
        [
            if rtl {
                app.viewport[0] - 16.0 - size[0]
            } else {
                16.0
            },
            (app.viewport[1] - size[1]) * 0.5,
        ]
    } else {
        donor_rail::anchor(model.mode, app.viewport, size, gallery_height)
    };
    // A tall retained vertical bar can exceed the gallery's own height. Keep
    // the source anchor inside its available viewport before saved offsets.
    if model.vertical {
        at[0] = at[0].clamp(0.0, (app.viewport[0] - size[0]).max(0.0));
        at[1] = at[1].clamp(0.0, (app.viewport[1] - size[1]).max(0.0));
    }
    let source = settings::bridge::Source { app };
    at[0] = (at[0] + source.number("filterBar.offsetX") as f32 * if rtl { -1.0 } else { 1.0 })
        .clamp(0.0, (app.viewport[0] - size[0]).max(0.0));
    at[1] = (at[1] + source.number("filterBar.offsetY") as f32)
        .clamp(0.0, (app.viewport[1] - size[1]).max(0.0));
    at
}

pub(super) fn gallery_background_layers(app: &GpuCandidate) -> iced::widget::Stack<'_, Message> {
    if app.state.appearance.presentation == BackgroundPresentation::LiveTransparentOverlay {
        return stack![].width(Fill).height(Fill);
    }
    let background_rgb = backdrop_background_rgb(app);
    let overlay_opacity = app.state.appearance.overlay_opacity.clamp(0.0, 1.0);
    let backdrop_art: Element<'_, Message> = app.backdrop.as_ref().map_or_else(
        || container(text("")).width(Fill).height(Fill).into(),
        |handle| {
            iced_image(handle.clone())
                .width(Fill)
                .height(Fill)
                .content_fit(ContentFit::Cover)
                .into()
        },
    );
    let backdrop_tint = container(text(""))
        .width(Fill)
        .height(Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(Color::from_rgba(
                background_rgb[0],
                background_rgb[1],
                background_rgb[2],
                overlay_opacity,
            ))),
            ..Default::default()
        });
    stack![backdrop_art, backdrop_tint]
}

pub(super) fn view(app: &GpuCandidate, _id: window::Id) -> Element<'_, Message> {
    let _timing = crate::apply_trace::cpu_span("detail-root-view", || {
        format!(
            "mode={:?} detail={} {}",
            app.gallery_mode,
            app.detail_open,
            app.selected_item_trace_fields()
        )
    });
    // A decision owns the whole widget tree. Retaining the editor underneath
    // would let its focused text input consume keyboard events through a Stack.
    if let Some(prompt) = settings::theme_designer::prompt(app) {
        return prompt;
    }
    let ordinary = (app.gallery_mode != GalleryMode::Depth).then(|| app.ordinary_frame());
    let mut cards = if let Some(entries) = &ordinary {
        entries
            .iter()
            .filter(|e| !e.outgoing)
            .filter_map(donor_scene::presented_hit_card)
            .collect()
    } else if let Some(frame) = &app.depth_frame {
        frame.cards.iter().map(|e| e.card).collect()
    } else {
        let selected = view_legacy_selection(app);
        let poster_aspects = filtered_poster_aspects(&app.catalog, &app.filtered_indices);
        gpu_gallery::layout_gallery_with_aspects(
            app.gallery_mode,
            app.filtered_indices.len(),
            selected,
            app.viewport,
            gallery_layout_params_for_app(app),
            &poster_aspects,
        )
    };
    let loaded_layers: HashSet<_> = app.posters.iter().map(|poster| poster.layer).collect();
    for card in &mut cards {
        let Some(master_index) = (if app.gallery_mode == GalleryMode::Depth || ordinary.is_some() {
            Some(card.index)
        } else {
            resolve_master_index(&app.wallpapers, &app.filtered_indices, card.index)
        }) else {
            card.layer = None;
            continue;
        };
        card.index = master_index;
        let layer = app
            .poster_slots
            .get(&master_index)
            .filter(|slot| {
                app.wallpapers
                    .get(master_index)
                    .is_some_and(|wallpaper| poster_slot_matches(slot, wallpaper))
            })
            .map(|slot| slot.layer)
            .filter(|layer| loaded_layers.contains(layer));
        card.layer = layer;
        if let Some(poster) = layer.and_then(|layer| app.posters.iter().find(|p| p.layer == layer))
        {
            card.poster_aspect_ratio = poster.aspect_ratio;
        }
    }
    let selected_card_geometry = cards
        .iter()
        .find(|card| card.selected)
        .map(|card| donor_detail::CardGeometry {
            center: card.center,
            size: card.size,
            rotation: card.rotation,
            skew_distance: card.skew_distance,
            edge_tilt_distance: card.edge_tilt_distance,
        })
        .or(app.detail_source);
    let snapshot = Arc::new(GallerySnapshot {
        ordinary,
        wall_clip: (app.gallery_mode == GalleryMode::Wall).then(|| {
            gpu_gallery::wall_frame(app.viewport, gallery_layout_params_for_app(app).wall).clip
        }),
        depth: app.depth_frame.clone().map(|mut frame| {
            for e in &mut frame.cards {
                if let Some(card) = cards.iter().find(|c| c.index == e.card.index) {
                    e.card = *card;
                }
            }
            frame
        }),
        cards,
        posters: app.posters.clone(),
        colors: gallery_snapshot_colors(app),
        scene: DonorSceneState {
            stage: donor_layout::StageParams::default(),
            camera: app.motion,
            camera_target: 0.0,
            camera_velocity: 0.0,
            detail_progress: app.detail_progress,
            flip_options: app.flip_presentation(),
            detail_source: app.detail_source,
            detail_card: app.external_detail_card(),
            external_detail: app.depth_panel_open()
                || !matches!(app.gallery_mode, GalleryMode::Slices | GalleryMode::Depth),
            viewport: app.viewport,
            time: app.presentation_started.elapsed().as_secs_f32(),
            visibility: if app.atlas_entry.is_some() {
                0.0
            } else {
                app.scene_motion.opacity()
            },
        },
    });
    let scene: Element<'_, GalleryMessage> = shader(GalleryProgram {
        prewarm: app.atlas_entry.map(|_| {
            (
                app.atlas_prewarm_key(),
                !app.poster_loading && app.poster_request_key.is_some(),
            )
        }),
        snapshot: snapshot.clone(),
        poster_feedback: Some(super::images::publish_poster_feedback),
    })
    .width(Fill)
    .height(Fill)
    .into();
    let scene = scene.map(move |message| super::lifecycle::bind_gallery_input(app, message));

    let presentation = app.catalog_presentation();
    let text_rgb = match applied_theme_colors(app) {
        Some(colors) => colors.surface_text,
        None => gpu_gallery::parse_hex_rgb(
            &app.state.appearance.text,
            [
                0xcd as f32 / 255.0,
                0xd6 as f32 / 255.0,
                0xf4 as f32 / 255.0,
            ],
        ),
    };
    let folio_palette = donor_palette(app);
    let rail_model = gallery_rail_model(app);
    let gallery_height = match app.gallery_mode {
        GalleryMode::Slices => app.state.gallery.slices.height,
        GalleryMode::Depth => app.state.gallery.depth.height,
        GalleryMode::Geometric => {
            let g = &app.state.gallery.geometric;
            let shape = match g.shape {
                StateGeometricShape::Triangle => donor_layout::HexShape::Triangle,
                StateGeometricShape::Diamond => donor_layout::HexShape::Diamond,
                StateGeometricShape::Rhombus => donor_layout::HexShape::Rhombus,
                _ => donor_layout::HexShape::Hexagon,
            };
            if g.shape == StateGeometricShape::Circle {
                g.radius * 2.0 * g.rows as f32 + g.gap_y * g.rows.saturating_sub(1) as f32
            } else {
                donor_rail::geometric_height(g.radius, g.rows, g.gap_y, g.stagger, shape)
            }
        }
        GalleryMode::Wall => {
            app.state.gallery.wall.card_height * app.state.gallery.wall.rows as f32
                + app.state.gallery.wall.gap_y
                    * app.state.gallery.wall.rows.saturating_sub(1) as f32
        }
    };
    let rail_at = gallery_rail_anchor(app, &rail_model, gallery_height);
    let folder_popup = (app.folder_menu_open && !app.panel_state.settings_open).then(|| {
        let model = sheets::folder_popup_model(app);
        let anchor = donor_rail::folder_popup_anchor(
            &rail_model,
            rail_at,
            app.viewport,
            [model.width, model.height],
        );
        (model, anchor)
    });
    let rail_layer: Element<'_, Message> = container(
        donor_rail::view_with_opacity(
            rail_model,
            app.scene_motion.opacity() * app.gallery_entrance_opacity(),
            !app.playlist_panel_open,
        )
        .map(Message::DonorRail),
    )
    .padding(iced::Padding {
        top: rail_at[1],
        left: rail_at[0],
        ..iced::Padding::ZERO
    })
    .into();
    let mut content = column![].spacing(8).padding(18);
    if let Some(issue) =
        app.ordinary_capacity_issue_for_frame(snapshot.ordinary.as_deref().unwrap_or(&[]))
    {
        content = content.push(text(issue).size(14));
    }
    let mut detail_overlay: Option<Element<'_, Message>> = None;
    let (mut rail_visible, _) = detail_chrome_visibility(app.gallery_mode, app.detail_open);
    rail_visible &= !app.rail_dismissed;
    rail_visible &= !app.depth_panel_open();
    rail_visible &= !app.panel_state.settings_open;
    if presentation != CatalogPresentation::Ready {
        content = content.push(text(presentation.title()).size(14));
    }
    if app.controls_panel_open && !app.panel_state.settings_open {
        let panel_content = column![
            gallery_picker_content(app),
            position_content(app),
            motion_settings_content(app),
        ]
        .spacing(10);
        let controls_panel = container(scrollable(panel_content).height(iced::Shrink))
            .padding(donor_ui::sheet_padding(0.72))
            .width(Fill)
            .max_width(1100)
            .max_height(420)
            .style(move |_| donor_ui::panel_style(folio_palette, 0.96));
        content = content.push(container(controls_panel).width(Fill).center_x(Fill));
    }
    if app.detail_open {
        if let Some((index, wallpaper, source_card)) = app
            .selected
            .and_then(|index| Some((index, app.wallpapers.get(index)?, selected_card_geometry?)))
        {
            let embedded = matches!(app.gallery_mode, GalleryMode::Slices | GalleryMode::Depth)
                && !app.depth_panel_open();
            let card = if embedded {
                source_card
            } else {
                donor_detail::external_target(app.viewport)
            };
            let resolution = app
                .catalog
                .get(index)
                .filter(|entry| entry.width > 0 && entry.height > 0)
                .map_or_else(
                    || "Unknown".to_string(),
                    |entry| format!("{} × {}", entry.width, entry.height),
                );
            let file_size = wallpaper
                .file_target
                .metadata()
                .map(|m| format_file_size(m.len()))
                .unwrap_or_else(|_| "Unavailable".into());
            let actions = detail_actions(app, wallpaper);
            let radius = if app.gallery_mode == GalleryMode::Depth && embedded {
                app.depth_frame
                    .as_ref()
                    .and_then(|f| f.cards.iter().find(|c| c.card.selected))
                    .map_or(0.0, |c| c.card.corner_radius)
            } else if embedded {
                snapshot
                    .cards
                    .iter()
                    .find(|card| card.selected)
                    .map_or(0.0, |card| card.corner_radius)
            } else {
                0.0
            };
            let detail_identity = wallpaper.id.clone();
            detail_overlay = Some(
                donor_back_view::view(donor_back_view::BackModel {
                    identity: wallpaper.id.clone(),
                    input_active: detail_controls_active(app),
                    card,
                    embedded,
                    scale: settings::bridge::effective_ui_scale(app),
                    progress: app.detail_progress,
                    depth: app.depth_frame.as_ref().map(|f| f.detail),
                    flip_options: app.flip_presentation(),
                    depth_revision: app.depth_frame.as_ref().map(|f| f.revision),
                    opacity: if app.depth_panel_open() {
                        1.0
                    } else {
                        app.depth_frame
                            .as_ref()
                            .and_then(|f| f.cards.iter().find(|c| c.card.selected))
                            .map_or(1.0, |c| c.card.opacity)
                    } * app.scene_motion.opacity(),
                    radius,
                    title: wallpaper.title.clone(),
                    kind: wallpaper.wallpaper_type.to_uppercase(),
                    fields: vec![
                        ("Resolution".into(), resolution),
                        ("Size".into(), file_size),
                        (
                            "Modified".into(),
                            app.catalog
                                .get(index)
                                .and_then(|entry| detail_modified_date(entry.modified_unix))
                                .unwrap_or_else(|| "Unknown".into()),
                        ),
                        (
                            "Applied".into(),
                            app.state
                                .local_library
                                .usage
                                .items
                                .get(&wallpaper.id)
                                .map_or_else(
                                    || "0 times".into(),
                                    |usage| {
                                        let count = usage.confirmed_count;
                                        format!(
                                            "{count} {}",
                                            if count == 1 { "time" } else { "times" }
                                        )
                                    },
                                ),
                        ),
                    ],
                    tags: app.catalog.get(index).map_or_else(String::new, |entry| {
                        entry
                            .tags
                            .iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .join(" · ")
                    }),
                    favorite: app.state.favorites.contains(&wallpaper.id),
                    actions,
                    palette: folio_palette,
                    font: UI_FONT,
                    icon_font: NERD_FONT,
                })
                .map(move |intent| {
                    if let Some(frame) = &app.depth_frame {
                        Message::DepthDonorDetail(frame.revision, detail_identity.clone(), intent)
                    } else {
                        Message::DonorDetail(detail_identity.clone(), intent)
                    }
                }),
            );
        }
    }
    content = content.push(Space::new().height(Fill));
    if app.browse_playlist_unavailable {
        content = content.push(text(
            app.settings_locale
                .text_static("playlist-browse-unavailable"),
        ));
    }
    if let Some(status) = app.status_for_display() {
        // Settings already owns this exact diagnostic in its scrollable panel.
        // Do not repeat it behind the gateway's bottom action row.
        let settings_owns_status = app.panel_state.settings_open
            && app.settings_workbench.active
            && status == app.settings_workbench.message;
        if !settings_owns_status {
            content = content.push(
                container(
                    iced::widget::scrollable(text(status).width(Fill)).height(iced::Length::Shrink),
                )
                .max_height(80.0),
            );
        }
    }
    if !app.apply_pending {
        for recovery in &app.provider_recoveries {
            content = content.push(text(coordinator::recovery_capabilities(recovery).reason));
        }
    }
    let chrome = container(content)
        .width(Fill)
        .height(Fill)
        .style(move |_| container::Style {
            text_color: Some(Color::from_rgb(text_rgb[0], text_rgb[1], text_rgb[2])),
            ..Default::default()
        });

    let mut layers = gallery_background_layers(app).push(scene);
    if app.gallery_mode == GalleryMode::Depth {
        if let Some(entry) = app
            .depth_frame
            .as_ref()
            .and_then(|f| f.cards.iter().find(|c| c.card.selected))
        {
            if app
                .wallpapers
                .get(entry.card.index)
                .is_some_and(|w| w.id == entry.id)
            {
                let alpha = (1.0 - app.detail_progress).clamp(0.0, 1.0)
                    * entry.card.opacity
                    * app.scene_motion.opacity();
                let label = text(if app.state.favorites.contains(&entry.id) {
                    "★"
                } else {
                    "☆"
                })
                .size(12)
                .color(donor_ui::with_alpha_color(folio_palette.primary, alpha));
                layers = layers.push(container(label).padding(iced::Padding {
                    top: (entry.card.center[1] - entry.card.size[1] * 0.5 + 12.0).max(0.0),
                    left: (entry.card.center[0] + entry.card.size[0] * 0.5 - 24.0).max(0.0),
                    ..iced::Padding::ZERO
                }));
            }
        }
    }
    if let Some(detail_overlay) = detail_overlay {
        layers = layers.push(detail_overlay);
    }
    layers = layers.push(chrome);
    if folder_popup.is_some() {
        layers = layers.push(
            iced::widget::mouse_area(Space::new().width(Fill).height(Fill))
                .on_press(Message::DismissFolderMenu),
        );
    }
    // Keep the later chooser subtree at the same Stack index through the fade.
    if app.config.ui.show_top_bar && rail_visible {
        layers = layers.push(rail_layer);
    }
    if let Some((model, folder_at)) = folder_popup {
        layers = layers.push(container(folder_popup_view(model)).padding(iced::Padding {
            left: folder_at[0],
            top: folder_at[1],
            ..iced::Padding::ZERO
        }));
    }
    if app.panel_state.settings_open {
        layers = layers.push(settings::gateway::view(app));
    }
    if app.panel_state.search_open && !app.panel_state.settings_open {
        layers = layers.push(search_panel_view(app, folio_palette));
    }
    if app.playlist_panel_open {
        let selected = app
            .selected
            .filter(|i| app.filtered_indices.contains(i))
            .and_then(|i| app.wallpapers.get(i).map(|w| (i, w)));
        layers = layers.push(if app.playlist_chooser {
            sheets::playlist_chooser_view(app)
        } else if app.playlist_picker {
            sheets::playlist_picker_view(app)
        } else {
            playlist_panel_view(app, selected)
        });
    }
    if app.engine_properties_open {
        layers = layers.push(
            container(engine_property_sheet(app, folio_palette))
                .center_x(Fill)
                .center_y(Fill),
        );
    }
    if let Some((_, notice)) = app.rotation_notice {
        use candidate_state::RandomRotationState;
        let label = match notice {
            RandomRotationState::PausedForPlaylist => {
                "Random rotation is paused for playlist playback."
            }
            RandomRotationState::SafelyStopped => "Random rotation is safely stopped.",
            RandomRotationState::On => app.settings_locale.text_static("status-random-rotation-on"),
            RandomRotationState::Off => app
                .settings_locale
                .text_static("status-random-rotation-off"),
        };
        layers = layers.push(
            container(
                container(text(label).size(14).font(UI_FONT))
                    .padding([7, 14])
                    .style(move |_| {
                        donor_ui::box_style(folio_palette.surface, folio_palette.outline)
                    }),
            )
            .width(Fill)
            .height(Fill)
            .align_x(iced::alignment::Horizontal::Center)
            .align_y(iced::alignment::Vertical::Bottom)
            .padding(28),
        );
    }
    layers.width(Fill).height(Fill).into()
}

fn detail_modified_date(seconds: u64) -> Option<String> {
    use windows_sys::Win32::Foundation::{FILETIME, SYSTEMTIME};
    use windows_sys::Win32::System::Time::FileTimeToSystemTime;
    if seconds == 0 {
        return None;
    }
    let ticks = seconds
        .checked_add(11_644_473_600)?
        .checked_mul(10_000_000)?;
    let file = FILETIME {
        dwLowDateTime: ticks as u32,
        dwHighDateTime: (ticks >> 32) as u32,
    };
    // SAFETY: the API receives a valid FILETIME and writable SYSTEMTIME.
    let mut date: SYSTEMTIME = unsafe { std::mem::zeroed() };
    (unsafe { FileTimeToSystemTime(&file, &mut date) } != 0)
        .then(|| format!("{:04}-{:02}-{:02}", date.wYear, date.wMonth, date.wDay))
}

pub(super) fn detail_actions(
    app: &GpuCandidate,
    wallpaper: &WallpaperItem,
) -> Vec<donor_back_layout::Action> {
    use donor_back_layout::{Action, DetailIntent};
    let mut actions = vec![
        Action {
            intent: DetailIntent::Apply,
            label: if app.apply_pending {
                "Applying…"
            } else {
                "Apply"
            }
            .into(),
            enabled: app.apply_enabled && !app.apply_pending && !app.provider_state_unknown,
        },
        Action {
            intent: DetailIntent::Playlists,
            label: "Playlists".into(),
            enabled: true,
        },
        Action {
            intent: DetailIntent::OpenFolder,
            label: "Show in folder".into(),
            enabled: true,
        },
    ];
    if wallpaper.source == WallpaperSource::LocalImage && wallpaper.file_target.is_absolute() {
        actions.push(Action {
            intent: DetailIntent::CopyImage,
            label: "Copy image".into(),
            enabled: true,
        });
    }
    if wallpaper.file_target.is_absolute() {
        actions.push(Action {
            intent: DetailIntent::CopyPath,
            label: "Copy absolute path".into(),
            enabled: true,
        });
    }
    if wallpaper.source == WallpaperSource::WallpaperEngine {
        actions.push(Action {
            intent: DetailIntent::Properties,
            label: "Engine Properties".into(),
            enabled: true,
        });
    }
    actions
}

pub(super) fn detail_controls_active(app: &GpuCandidate) -> bool {
    app.detail_open
        && (!matches!(app.gallery_mode, GalleryMode::Slices | GalleryMode::Depth)
            || donor_scene::card_flip_phases(
                app.detail_progress,
                app.flip_presentation().animate_shader,
                app.flip_presentation().animate_back,
            )
            .content
                > 0.01)
        && (app.gallery_mode != GalleryMode::Depth
            || app.depth_frame.as_ref().is_some_and(|f| {
                let p = donor_scene::card_flip_phases(
                    app.detail_progress,
                    f.detail.animate_shader,
                    f.detail.animate_back,
                );
                p.content > 0.01
                    && app
                        .depth_detail_id
                        .as_ref()
                        .is_some_and(|id| f.cards.iter().any(|c| c.card.selected && &c.id == id))
            }))
        && !app.panel_state.search_open
        && !app.panel_state.settings_open
        && !app.playlist_panel_open
        && !app.controls_panel_open
        && !app.folder_menu_open
        && !app.engine_properties_open
}

fn format_file_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut amount = bytes as f64;
    let mut unit = 0;
    while amount >= 1024.0 && unit + 1 < UNITS.len() {
        amount /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{amount:.1} {}", UNITS[unit])
    }
}

pub(super) fn theme(app: &GpuCandidate, _id: window::Id) -> Theme {
    if let Some(colors) = applied_theme_colors(app) {
        return donor_ui::widget_theme(
            Color::from_rgb(
                colors.background[0],
                colors.background[1],
                colors.background[2],
            ),
            Color::from_rgb(
                colors.surface_text[0],
                colors.surface_text[1],
                colors.surface_text[2],
            ),
            Color::from_rgb(colors.primary[0], colors.primary[1], colors.primary[2]),
        );
    }
    let defaults = GalleryColors::default();
    let background = gpu_gallery::parse_hex_rgb(
        &app.state.appearance.background,
        [
            0x1e as f32 / 255.0,
            0x1e as f32 / 255.0,
            0x2e as f32 / 255.0,
        ],
    );

    let accent = gpu_gallery::parse_hex_rgb(&app.state.appearance.accent, defaults.accent);
    let text = gpu_gallery::parse_hex_rgb(
        &app.state.appearance.text,
        [
            0xcd as f32 / 255.0,
            0xd6 as f32 / 255.0,
            0xf4 as f32 / 255.0,
        ],
    );

    donor_ui::widget_theme(
        Color::from_rgb(background[0], background[1], background[2]),
        Color::from_rgb(text[0], text[1], text[2]),
        Color::from_rgb(accent[0], accent[1], accent[2]),
    )
}

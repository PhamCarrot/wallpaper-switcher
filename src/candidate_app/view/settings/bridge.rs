//! Read-only projection of current  values and local draft intent.
use super::super::super::*;
use super::composed::{self, draft::Value, input, keys, schema, SettingsSource};
use crate::candidate_state::GalleryMotionSpeed;

#[derive(Default)]
pub(in crate::candidate_app) struct Session {
    /// Supplied only by compatibility-owner tests; never a public composition route.
    #[cfg(test)]
    pub owner_test_model:
        Option<fn(&GpuCandidate, &str) -> Vec<(composed::Card, Vec<composed::Row>)>>,
    pub pending_forms: super::pending_forms::Session,
    pub draft: composed::draft::Draft,
    /// Only an active numeric preview baseline; never serialized as another model.
    pub geometry_preview_baseline: Option<candidate_state::GallerySettings>,
    pub lists: composed::collections::Collections,
    pub theme_targets_draft: Option<Vec<String>>,
    pub navigation: composed::navigation::Navigation,
    pub capture: Option<composed::binding_capture::Capture>,
    pub capture_modifiers: composed::input::Mods,
    pub generation: u64,
    pub message: String,
    pub active: bool,
    pub windows_panel: Option<SettingsCategory>,
    pub legacy_category: Option<SettingsCategory>,
    pub owner_window: Option<iced::window::Id>,
    pub tab: String,
    pub preset_name_raw: Option<(crate::candidate_state::GalleryKind, u64, String)>,
    pub render_values: std::collections::HashMap<String, String>,
    pub output_statuses: Vec<composed::status::OutputStatus>,
    pub folders: Vec<String>,
    pub output_previews: std::collections::HashMap<String, String>,
    pub results: Vec<composed::search::SettingsSearchResult>,
    pub palette: Option<crate::donor_ui::Palette>,
    pub bar_reveals:
        std::collections::HashMap<String, crate::donor_folio::source_ui::motion::Tween>,
    pub armed: Option<composed::ActionId>,
    pub process_picker: Option<super::folio::workbench::ProcessPickerView>,
    pub last_frame: Option<std::time::Instant>,
    pub folio_entrance: Option<crate::donor_folio::source_ui::motion::Tween>,
    pub tab_reveal: Option<crate::donor_folio::source_ui::motion::Tween>,
    preview_still: Option<library::media_still::MediaStill>,
    preview_image: Option<iced_image::Handle>,
    preview_generation: u64,
    preview_loading: Option<u64>,
}

impl Session {
    pub(in crate::candidate_app) fn layout_studio(&self) -> bool {
        self.windows_panel.is_none()
            && composed::is_picker_layout_section(&self.tab, self.navigation.section)
    }

    pub(super) fn clear_preview(&mut self) {
        if self.preview_still.take().is_some() {
            self.preview_generation = self.preview_generation.wrapping_add(1);
        }
        self.preview_image = None;
    }

    pub(in crate::candidate_app) fn finish_preview(
        &mut self,
        generation: u64,
        image: Option<iced_image::Handle>,
    ) {
        if self.preview_loading == Some(generation) {
            self.preview_loading = None;
            if self.preview_generation == generation {
                self.preview_image = image;
            }
        }
    }

    pub(in crate::candidate_app) fn from_state(state: &CandidateState) -> Self {
        let mut session = Self::default();
        session.pending_forms = super::pending_forms::Session::from_state(state);
        session.lists.resolutions = state.frontend_preferences.resolution_presets.clone();
        super::pending_projection::hydrate_collections(state, &mut session.lists);
        if !state.pending_frontend.settings.is_empty() || !state.pending_frontend.presets.is_empty()
        {
            session.message =
                "Saved pending configuration restored. Unavailable operations remain inactive."
                    .into();
        }
        for root in super::pending_projection::COLLECTION_ROOTS {
            if let Err(error) = super::pending_projection::collection_editable(state, root) {
                session.message = format!("{root}: {error}");
                break;
            }
        }
        for path in ["theme.targets", "transition.shaderScopes"] {
            match super::pending_projection::checked_root(state, path) {
                Err(error) => session.message = format!("{path}: {error}"),
                Ok(Some(value)) => {
                    let unknown = if path == "theme.targets" {
                        value.as_array().is_some_and(|entries| {
                            entries
                                .iter()
                                .any(|entry| !matches!(entry.as_str(), Some("noctalia" | "dms")))
                        })
                    } else {
                        value.as_object().is_some_and(|entries| {
                            entries
                                .keys()
                                .any(|key| !composed::SHADERS.contains(&key.as_str()))
                        })
                    };
                    if unknown {
                        session.message = format!("Saved pending {path} contains unfamiliar identities. They were retained and remain inactive.");
                    }
                }
                Ok(None) => {}
            }
        }
        session
    }
}

pub(in crate::candidate_app) fn build(
    app: &GpuCandidate,
    tab: &str,
) -> Vec<(composed::Card, Vec<composed::Row>)> {
    let session = &app.settings_workbench;
    #[cfg(test)]
    if let Some(model) = session.owner_test_model {
        return model(app, tab);
    }
    composed::build_tab_with_runtime_status(
        tab,
        &Source { app },
        &[],
        &session.folders,
        "",
        &[],
        &app.monitor_ids,
        &session.output_statuses,
        &session.output_previews,
        None,
        None,
        None,
    )
}

/// Facts from the existing read-only Windows monitor enumeration only. Current
/// media/audio/playback readback remains unobserved and gains no capability.
pub(in crate::candidate_app) fn observe_monitors(
    session: &mut Session,
    monitors: &[crate::native_wallpaper::NativeMonitor],
) {
    session.output_statuses = monitors
        .iter()
        .map(|monitor| composed::status::OutputStatus {
            name: monitor.device_id.clone(),
            target: monitor.device_id.clone(),
            connected: true,
            width: monitor.rect.width() as i32,
            height: monitor.rect.height() as i32,
            kind: composed::media::MediaKind::Other("unobserved".into()),
            ..Default::default()
        })
        .collect();
}

/// Use the gallery's bounded thumbnail format without reserving an atlas layer.
pub(in crate::candidate_app) fn sync_preview(app: &mut GpuCandidate) -> Task<Message> {
    let session = &app.settings_workbench;
    if !session.active || session.windows_panel.is_some() || session.layout_studio() {
        app.settings_workbench.clear_preview();
        return Task::none();
    }
    let selected = app
        .selected
        .filter(|index| app.filtered_indices.contains(index));
    let still = selected
        .and_then(|index| app.wallpapers.get(index))
        .and_then(|item| {
            library::media_still::MediaStill::select(
                item,
                app.catalog_generation.load(Ordering::Acquire),
            )
        });
    if still != session.preview_still {
        app.settings_workbench.clear_preview();
    }
    let session = &mut app.settings_workbench;
    if session.preview_loading.is_some() || still == session.preview_still {
        return Task::none();
    }
    let Some(still) = still else {
        return Task::none();
    };
    let poster = selected
        .and_then(|index| app.poster_slots.get(&index))
        .filter(|slot| {
            !slot.is_video
                && slot.item_id == still.item_id
                && slot.catalog_generation == still.catalog_revision
                && slot.source == still.path
        })
        .and_then(|slot| {
            app.posters
                .iter()
                .find(|p| p.layer == slot.layer && p.source == slot.source)
        })
        .cloned();
    session.preview_generation = session.preview_generation.wrapping_add(1);
    let generation = session.preview_generation;
    session.preview_still = Some(still.clone());
    session.preview_loading = Some(generation);
    Task::perform(
        async move { load_preview(&still, poster.as_ref()) },
        move |image| Message::SettingsPreviewReady { generation, image },
    )
}

fn load_preview(
    still: &library::media_still::MediaStill,
    poster: Option<&Poster>,
) -> Option<iced_image::Handle> {
    if !still.is_current() {
        return None;
    }
    let image = gpu_gallery::still_thumbnail(&still.path, poster)?;
    still.is_current().then_some(image)
}

#[cfg(test)]
impl Session {
    pub(in crate::candidate_app) fn preview_completion(&self) -> Option<Message> {
        let generation = self.preview_loading?;
        let still = self.preview_still.as_ref()?;
        Some(Message::SettingsPreviewReady {
            generation,
            image: load_preview(still, None),
        })
    }

    pub(in crate::candidate_app) fn preview_image(&self) -> Option<&iced_image::Handle> {
        self.preview_image.as_ref()
    }
}

/// Frame demand shares the Settings lifetime; higher input owners may coexist.
pub(in crate::candidate_app) fn wants_frames(app: &GpuCandidate) -> bool {
    let session = &app.settings_workbench;
    session.active
        && app.visible
        && !app.exit_requested
        && app.panel_state.settings_open
        && (session.bar_reveals.values().any(|tween| !tween.settled())
            || session
                .folio_entrance
                .as_ref()
                .is_some_and(|tween| !tween.settled())
            || session
                .tab_reveal
                .as_ref()
                .is_some_and(|tween| !tween.settled()))
}

/// The same draft-first scale used by composed Settings, independent of scene/DPI scale.
pub(in crate::candidate_app) fn effective_ui_scale(app: &GpuCandidate) -> f32 {
    Source { app }.number(keys::general::UI_SCALE) as f32
}

/// Pure renderer input; only app-owned data outlives the temporary Source adapter.
pub(in crate::candidate_app) fn view(
    app: &GpuCandidate,
) -> Element<'_, super::folio::intent::Message> {
    use super::folio::workbench::*;
    let session = &app.settings_workbench;
    let Some(palette) = session.palette.as_ref() else {
        return iced::widget::text("Preparing Settings…").into();
    };
    let config = Source { app };
    let source = SourceCtx {
        config: &config,
        values: &session.render_values,
        themes: &[],
        folders: &session.folders,
        backends: &[],
        outputs: &app.monitor_ids,
        output_statuses: &session.output_statuses,
        output_previews: &session.output_previews,
        library_watch: None,
        app_themes: None,
        playback: None,
        thumbnail_task: None,
        analysis: String::new(),
    };
    let focus = FocusCtx {
        keyboard_focus: session.navigation.focus,
        focused_control: session.navigation.row,
        focused_choice: session.navigation.choice,
        expanded_details: &session.navigation.expanded,
        bar_reveals: &session.bar_reveals,
        active_input: session.navigation.input_edit.as_deref(),
        armed: session.armed,
    };
    let chrome = ChromeCtx {
        viewport: app.viewport.into(),
        scale: effective_ui_scale(app),
        entrance: session.folio_entrance.as_ref().map_or(1.0, |tween| tween.x),
        palette,
    };
    let content = if session.layout_studio() {
        picker_layout_workbench(&source, focus, chrome)
    } else {
        settings_workbench(WorkbenchInput {
            source,
            focus,
            chrome,
            preview_image: session.preview_image.as_ref(),
            tab_transition: session.tab_reveal.as_ref().map_or(1.0, |tween| tween.x),
            tab: &session.tab,
            selected_section: session.navigation.section,
            search_open: session.navigation.search_open,
            search_query: &app.settings_search,
            search_results: &session.results,
            keybind_capture: session.capture.as_ref().map(|capture| {
                let map = config.bindings();
                let mut conflicts: Vec<String> = capture
                    .conflicts(&map)
                    .iter()
                    .map(|a| format!("{a:?}"))
                    .collect();
                conflicts.extend(map.reserved_conflicts(&capture.triggers));
                KeybindCaptureView {
                    title: app.settings_locale.text(capture.title_key),
                    binding: composed::input::binding_label(&capture.triggers),
                    edited: capture.edited,
                    conflict: (!conflicts.is_empty()).then(|| format!("{conflicts:?}")),
                }
            }),
            process_picker: session.process_picker.clone(),
            status: &session.message,
        })
    };
    let clock = crate::donor_folio::source_ui::clock::Clock {
        active: wants_frames(app),
        interval: std::time::Duration::from_secs_f64(
            1.0 / config.number(keys::general::MAX_FPS).clamp(1.0, 1_000.0),
        ),
        on_frame: super::folio::intent::Message::Frame,
    };
    iced::widget::stack![
        content,
        iced::widget::canvas(clock).width(Fill).height(Fill)
    ]
    .into()
}

pub(in crate::candidate_app) struct Source<'a> {
    pub app: &'a GpuCandidate,
}

pub(in crate::candidate_app) fn mode_key(mode: GalleryMode) -> &'static str {
    match mode {
        GalleryMode::Slices => "slices",
        GalleryMode::Depth => "depth",
        GalleryMode::Geometric => "hex",
        GalleryMode::Wall => "wall",
    }
}

pub(in crate::candidate_app) fn gallery_field(path: &str) -> Option<(GalleryNumberField, f32)> {
    use keys::selector as K;
    use GalleryNumberField as F;
    Some(match path {
        K::DEPTH_HEIGHT => (F::DepthHeight, 1.0),
        K::DEPTH_WIDTH_PX => (F::DepthWidth, 1.0),
        K::DEPTH_SPACING_PX => (F::DepthSpacing, 1.0),
        K::DEPTH_COUNT => (F::DepthCount, 1.0),
        K::DEPTH_FALLOFF_FACTOR => (F::DepthFalloff, 1.0),
        K::DEPTH_CORNERS => (F::DepthCorners, 1.0),
        K::DEPTH_SKEW => (F::DepthSkew, 1.0),
        K::DEPTH_NAVIGATION_MS => (F::DepthNavigation, 1.0),
        K::FLIP_DURATION_MS => (F::DepthFlipDuration, 1.0),
        K::VISIBLE_COUNT => (F::SliceVisibleCount, 1.0),
        K::SLICE_WIDTH => (F::SliceWidth, 1.0),
        K::EXPANDED_WIDTH => (F::SliceExpandedWidth, 1.0),
        K::SLICE_HEIGHT => (F::SliceHeight, 1.0),
        K::SLICE_SPACING => (F::SliceSpacing, 1.0),
        K::SKEW_OFFSET => (F::SliceSkewDistance, 1.0),
        K::SLICE_EDGE_TILT => (F::SliceEdgeTilt, 1.0),
        K::SLICE_WOBBLE_STRENGTH => (F::SliceWobbleStrength, 100.0),
        K::HEX_RADIUS => (F::GeometricRadius, 1.0),
        K::HEX_ROWS => (F::GeometricRows, 1.0),
        K::HEX_COLS => (F::GeometricColumns, 1.0),
        K::HEX_GAP_X => (F::GeometricGapX, 1.0),
        K::HEX_GAP_Y => (F::GeometricGapY, 1.0),
        K::HEX_ARC_INTENSITY_X10 => (F::GeometricCurveStrength, 10.0),
        K::HEX_CURVE_FREQUENCY => (F::GeometricCurveFrequency, 1.0),
        K::HEX_ORBIT_RADIUS => (F::GeometricOrbitRadius, 1.0),
        K::GRID_COLUMNS => (F::WallColumns, 1.0),
        K::GRID_ROWS => (F::WallRows, 1.0),
        K::GRID_THUMB_WIDTH => (F::WallCardWidth, 1.0),
        K::GRID_THUMB_HEIGHT => (F::WallCardHeight, 1.0),
        K::GRID_GAP_X => (F::WallGapX, 1.0),
        K::GRID_GAP_Y => (F::WallGapY, 1.0),
        K::GRID_STAGGER => (F::WallStagger, 100.0),
        K::GRID_CORNER_RADIUS => (F::WallCornerRadius, 1.0),
        K::GRID_CYLINDER_BEND => (F::WallCylinderBend, 100.0),
        K::GRID_CYLINDER_RADIUS => (F::WallCylinderRadius, 1.0),
        _ => return None,
    })
}

impl Source<'_> {
    fn actual_number(&self, path: &str) -> Option<f64> {
        use keys::selector as K;
        let gallery = &self.app.state.gallery;
        Some(f64::from(match path {
            K::SLICE_STAGE_X => gallery.slices.offset_x * 100.0,
            K::SLICE_STAGE_Y => gallery.slices.offset_y * 100.0,
            K::HEX_ARC_INTENSITY => gallery.geometric.curve_strength,
            K::HEX_ASPECT => gallery.geometric.aspect * 100.0,
            K::HEX_STAGGER => gallery.geometric.stagger * 100.0,
            K::HEX_LENS => gallery.geometric.lens * 100.0,
            K::HEX_ORBIT => gallery.geometric.orbit * 100.0,
            K::HEX_TWIST => gallery.geometric.twist,
            K::HEX_SCATTER => gallery.geometric.scatter,
            K::GRID_SELECTED_SCALE => gallery.wall.selected_scale * 100.0,
            K::GRID_FLOW_WAVE => gallery.wall.flow_wave,
            K::GRID_FLOW_FREQUENCY => gallery.wall.flow_frequency,
            K::GRID_SCATTER => gallery.wall.scatter,
            K::GRID_SCALE_VARIANCE => gallery.wall.scale_variance * 100.0,
            _ => return None,
        }))
    }
    fn local(&self, path: &str) -> Option<Value> {
        self.app
            .settings_workbench
            .draft
            .values
            .get(path)
            .cloned()
            .or_else(|| {
                let policy = self.app.rotation_edit_preferences();
                Some(Value::Boolean(match path {
                    "restoreOnStartup" => self.app.state.local_library.startup.reapply_enabled,
                    "general.randomIncludeStatic" => policy.images,
                    "general.randomIncludeVideo" => policy.videos,
                    "general.randomIncludeWE" => policy.we,
                    "general.randomIncludeFavourites" => policy.favourites_only,
                    _ => return None,
                }))
            })
            .or_else(|| super::geometry_projection::read(&self.app.state, path))
            .or_else(|| {
                super::preference_projection::read(&self.app.state.frontend_preferences, path)
            })
            .or_else(|| {
                super::pending_projection::collection_value(
                    &self.app.settings_workbench.lists,
                    path,
                )
            })
            .or_else(|| super::pending_projection::read(&self.app.state, path))
            .or_else(|| super::pending_projection::scope(&self.app.state, path))
            .or_else(|| super::pending_projection::background(&self.app.state, path))
    }

    fn effective_text(&self, path: &str) -> Option<String> {
        let app = self.app;
        Some(match path {
            keys::selector::DISPLAY_MODE => mode_key(app.gallery_mode).into(),
            keys::selector::FLIP_EFFECT => app.state.card_flip().effect.clone(),
            keys::selector::HEX_CURVE => match app.state.gallery.geometric.curve {
                StateGeometricCurve::Flat => "flat",
                StateGeometricCurve::Arc => "arc",
                StateGeometricCurve::Wave => "wave",
                StateGeometricCurve::S => "s",
                StateGeometricCurve::Cylinder => "cylinder",
            }
            .into(),
            keys::selector::HEX_SHAPE => match app.state.gallery.geometric.shape {
                StateGeometricShape::Triangle => "triangle",
                StateGeometricShape::Hexagon => "hexagon",
                StateGeometricShape::Diamond => "diamond",
                StateGeometricShape::Rhombus => "rhombus",
                StateGeometricShape::Circle => "circle",
            }
            .into(),
            keys::selector::GRID_LAYOUT => match app.state.gallery.wall.layout {
                StateWallLayout::Uniform => "uniform",
                StateWallLayout::Brick => "brick",
                StateWallLayout::Masonry => "masonry",
                StateWallLayout::Justified => "justified",
                StateWallLayout::Editorial => "editorial",
                StateWallLayout::Cylinder => "cylinder",
            }
            .into(),
            keys::motion::FILTER_SWAP_SPEED => match app.state.gallery_motion_speed {
                GalleryMotionSpeed::Fast => "fast",
                GalleryMotionSpeed::Standard => "standard",
                GalleryMotionSpeed::Slow => "slow",
            }
            .into(),
            keys::system::MONITOR => app
                .state
                .frontend_preferences
                .picker
                .desired_monitor_id
                .clone()
                .unwrap_or_else(|| app.active_monitor_id.clone().unwrap_or_default()),
            _ => return None,
        })
    }
}

impl SettingsSource for Source<'_> {
    fn language_persistence_note(&self) -> Option<&'static str> {
        Some("The interface language is saved for the next launch after successful publication.")
    }
    fn stash_available(&self) -> bool {
        !self.app.launch_policy.is_preview()
    }
    fn catalog(&self) -> &localization::Catalog {
        &self.app.settings_locale
    }
    fn flag(&self, path: &str) -> bool {
        let app = self.app;
        if let Some(target) = path.strip_prefix(&format!("{}.", keys::display::OUTPUT_LOCKS)) {
            return app.state.locked_monitors.contains(target);
        }
        match path {
            keys::selector::DEPTH_SHADOWS => app.state.gallery.depth.shadows,
            keys::selector::DEPTH_SELECTION_FRAME => app.state.gallery.depth.selection_frame,
            keys::selector::FLIP_SHADER => app.state.card_flip().animate_shader,
            keys::selector::FLIP_BACK_REVEAL => app.state.card_flip().animate_back,
            keys::selector::SLICE_WOBBLE => app.state.gallery.slices.wobble,
            keys::selector::ROUND_CORNERS => match self.local(path) {
                Some(Value::Boolean(value)) => value,
                _ => app.state.gallery.slices.corner_radius > 0.0,
            },
            keys::selector::GRID_ROUND_CORNERS => match self.local(path) {
                Some(Value::Boolean(value)) => value,
                _ => app.state.gallery.wall.corner_radius > 0.0,
            },
            "pk03.canResumeRandom" => {
                app.random_rotation_state() == candidate_state::RandomRotationState::SafelyStopped
            }
            "pk03.playlistPlaying" => app.playlist_playing(),
            keys::general::RANDOM_ROTATE => app.state.automation_intent().random_enabled,
            keys::wallpaper::MUTE => app.state.global_muted,
            _ => match self.local(path) {
                Some(Value::Boolean(value)) => value,
                _ => schema::boolean_default(path).unwrap_or(false),
            },
        }
    }
    fn flag_default_true(&self, path: &str) -> bool {
        if schema::boolean_default(path).is_some() || self.local(path).is_some() {
            self.flag(path)
        } else {
            true
        }
    }
    fn text(&self, path: &str) -> String {
        if path == "filterBar.activeResolution" {
            return self.app.resolution_label();
        }
        if path == "w6.policyStatus" {
            return self.app.automation_policy_status();
        }
        if path == "appearance.presentation" {
            return match self.app.state.appearance.presentation {
                BackgroundPresentation::WallpaperBackdrop => "wallpaper",
                BackgroundPresentation::LiveTransparentOverlay => "live",
            }
            .into();
        }
        if let Some(index) = path
            .strip_prefix("theme.targets.")
            .and_then(|index| index.parse::<usize>().ok())
        {
            let targets = self
                .app
                .settings_workbench
                .theme_targets_draft
                .clone()
                .or_else(|| super::pending_projection::target_names(&self.app.state).ok())
                .unwrap_or_default();
            return targets.get(index).cloned().unwrap_or_default();
        }
        if path == composed::PRESET_NAME_KEY {
            let session = &self.app.settings_workbench;
            let mode = self.app.state.gallery.mode;
            let selected = self.app.state.mode_profiles.selected_id(mode);
            return session
                .preset_name_raw
                .as_ref()
                .filter(|(owner_mode, id, _)| *owner_mode == mode && selected == Some(*id))
                .map(|(_, _, raw)| raw.clone())
                .unwrap_or_else(|| {
                    self.app
                        .state
                        .mode_profiles
                        .selected_name(mode)
                        .unwrap_or("")
                        .to_owned()
                });
        }
        if let Some(value) = self.effective_text(path) {
            return value;
        }
        if gallery_field(path).is_some() || self.actual_number(path).is_some() {
            // These existing geometry values are f32; display their shortest
            // decimal without exposing the f64 promotion's binary tail.
            return (self.number(path) as f32).to_string();
        }
        if matches!(
            path,
            keys::general::RANDOM_INTERVAL | keys::wallpaper::VOLUME
        ) {
            return composed::format_config_number(self.number(path));
        }
        if let Some(value) = self.app.settings_workbench.lists.text(path) {
            return value;
        }
        match self.local(path) {
            Some(Value::Text(value)) => value.clone(),
            Some(Value::Number(value)) => composed::format_config_number(value),
            Some(Value::Boolean(value)) => value.to_string(),
            None => {
                if gallery_field(path).is_some()
                    || matches!(
                        path,
                        keys::general::RANDOM_INTERVAL
                            | keys::wallpaper::VOLUME
                            | keys::selector::CORNER_TL
                            | keys::selector::CORNER_TR
                            | keys::selector::CORNER_BR
                            | keys::selector::CORNER_BL
                    )
                {
                    return composed::format_config_number(self.number(path));
                }
                schema::text_default(path)
                    .map(str::to_owned)
                    .or_else(|| schema::number_default(path).map(composed::format_config_number))
                    .unwrap_or_default()
            }
        }
    }
    fn number(&self, path: &str) -> f64 {
        if path == "appearance.overlayOpacity" {
            return f64::from(self.app.state.appearance.overlay_opacity);
        }
        if let Some(value) = self.actual_number(path) {
            return value;
        }
        if matches!(
            path,
            keys::selector::CORNER_TL
                | keys::selector::CORNER_TR
                | keys::selector::CORNER_BR
                | keys::selector::CORNER_BL
        ) {
            return match self.local(path) {
                Some(Value::Number(value)) => value,
                _ => f64::from(self.app.state.gallery.slices.corner_radius),
            };
        }
        if let Some((field, scale)) = gallery_field(path) {
            return f64::from(gallery_number_value(&self.app.state, field) * scale);
        }
        match path {
            keys::general::RANDOM_INTERVAL => self.app.state.slideshow_interval_seconds as f64,
            keys::wallpaper::VOLUME => f64::from(self.app.state.global_volume),
            _ => match self.local(path) {
                Some(Value::Number(value)) => value,
                _ => schema::number_default(path).unwrap_or(0.0),
            },
        }
    }
    fn array_len(&self, path: &str) -> usize {
        if path == "theme.targets" {
            return self
                .app
                .settings_workbench
                .theme_targets_draft
                .as_ref()
                .map(Vec::len)
                .or_else(|| {
                    super::pending_projection::target_names(&self.app.state)
                        .ok()
                        .map(|targets| targets.len())
                })
                .unwrap_or_default();
        }
        self.app.settings_workbench.lists.len(path)
    }
    fn default_semantic_model_available(&self) -> bool {
        false
    }
    fn display_mode(&self) -> String {
        mode_key(self.app.gallery_mode).into()
    }
    fn selector_mode(&self) -> String {
        if self.app.gallery_mode == GalleryMode::Wall {
            "grid".into()
        } else {
            self.display_mode()
        }
    }
    fn mode_profile_rows(
        &self,
        mode: crate::candidate_state::GalleryKind,
    ) -> Vec<crate::candidate_state::mode_profiles::ProfileRow> {
        self.app.state.mode_profiles.rows(mode)
    }
    fn launch_animation(&self) -> String {
        match self.local(keys::launch::ANIMATION) {
            Some(Value::Text(value)) => value.clone(),
            _ => "fade".into(),
        }
    }
    fn theme_backend(&self) -> String {
        match self.text(keys::theme::POLICY).as_str() {
            "fixed" => "static".into(),
            "off" => "off".into(),
            _ => {
                let authority = self.text(keys::theme::AUTHORITY);
                if matches!(
                    authority.as_str(),
                    "caelestia" | "noctalia" | "dms" | "end4"
                ) {
                    authority
                } else {
                    self.text(keys::theme::ENGINE)
                }
            }
        }
    }
    fn motion_fast_ms(&self) -> f32 {
        self.number(keys::motion::FAST_MS) as f32
    }
    fn motion_standard_ms(&self) -> f32 {
        self.number(keys::motion::STANDARD_MS) as f32
    }
    fn motion_slow_ms(&self) -> f32 {
        self.number(keys::motion::SLOW_MS) as f32
    }
    fn saved_theme_names(&self) -> Vec<String> {
        Vec::new()
    }
    fn palette_colors(&self) -> Vec<(String, Vec<String>)> {
        Vec::new()
    }
    fn palette_presets(&self) -> Vec<(String, String)> {
        vec![("nord".into(), "Nord".into())]
    }
    fn bindings(&self) -> input::InputMap {
        crate::candidate_app::lifecycle::effective_input_bindings(self.app)
    }
    fn graphics_card(&self) -> composed::GraphicsCard {
        composed::GraphicsCard {
            name: "Renderer device observation unavailable".into(),
            tier: composed::GraphicsTier::Other,
        }
    }
    fn graphics_devices(&self) -> Vec<composed::GraphicsDevice> {
        Vec::new()
    }
    fn is_niri(&self) -> bool {
        false
    }
    fn on_battery_power(&self) -> Option<bool> {
        None
    }
}

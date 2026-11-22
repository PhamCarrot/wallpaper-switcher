//! Application-owned intents for the composed Settings view.
use super::{
    bridge::{gallery_field, Source},
    composed::{self, draft::Value, ActionId, Control, SettingsKey, SettingsSource},
    folio::intent::{self, SettingsMsg},
};
use crate::candidate_app::*;
use crate::donor_folio::source_ui::motion::{MotionProfile, MotionTier, Tween};
use std::collections::HashMap;

pub(in crate::candidate_app) fn category_key(category: SettingsCategory) -> &'static str {
    match category {
        SettingsCategory::Picker => "picker",
        SettingsCategory::CatalogFilters => "filter",
        SettingsCategory::Position => "position",
        SettingsCategory::WallpapersDisplays => "displays",
        SettingsCategory::Motion => "motion",
        SettingsCategory::Playback => "playback",
        SettingsCategory::Library => "library",
        SettingsCategory::Sources => "library",
        SettingsCategory::Automation => "automation",
        SettingsCategory::Theme => "theme",
    }
}
pub(in crate::candidate_app) fn cards(
    app: &GpuCandidate,
) -> Vec<(composed::Card, Vec<composed::Row>)> {
    super::bridge::build(app, &app.settings_workbench.tab)
}
pub(in crate::candidate_app) fn rotate(app: &mut GpuCandidate) {
    let session = &mut app.settings_workbench;
    session.generation = session.draft.bind();
    session.capture = None;
    session.armed = None;
    session.navigation.input_edit = None;
}
pub(in crate::candidate_app) fn reconcile(app: &mut GpuCandidate) {
    let active =
        app.visible && !app.exit_requested && app.panel_state.settings_open && app.window.is_some();
    let entering = active
        && (!app.settings_workbench.active || app.settings_workbench.owner_window != app.window);
    if active != app.settings_workbench.active
        || (active && app.settings_workbench.owner_window != app.window)
        || (active
            && app.settings_workbench.legacy_category != Some(app.panel_state.settings_category))
    {
        app.settings_workbench.active = active;
        app.settings_workbench.owner_window = if active { app.window } else { None };
        if active {
            if entering {
                app.settings_workbench.clear_preview();
                if app.scene_motion.settings_opacity() == 0.0 {
                    let mut tween =
                        Tween::for_duration_ms(0.0, Source { app }.motion_standard_ms());
                    tween.retarget(1.0);
                    app.settings_workbench.folio_entrance = Some(tween);
                }
                app.settings_workbench.tab_reveal =
                    Some(Tween::for_duration_ms(1.0, Source { app }.motion_fast_ms()));
                app.settings_workbench.last_frame = None;
            }
            let changed =
                app.settings_workbench.legacy_category != Some(app.panel_state.settings_category);
            if changed && !entering {
                restart_tab_reveal(app);
            }
            if changed || app.settings_workbench.tab.is_empty() {
                app.settings_workbench.tab = category_key(app.panel_state.settings_category).into();
                app.settings_workbench.navigation.section = 0;
                app.settings_workbench.navigation.row = 0;
                app.settings_workbench.navigation.choice = None;
                app.settings_workbench.windows_panel = (app.panel_state.settings_category
                    == SettingsCategory::Theme)
                    .then_some(SettingsCategory::Theme);
            }
            app.settings_workbench.legacy_category = Some(app.panel_state.settings_category);
            // Binding can originate from a retained Windows category or a new
            // window. Keyboard navigation must start at the effective source tab.
            app.settings_workbench.navigation.category = composed::visible_tabs(&Source { app })
                .iter()
                .position(|(key, _)| *key == app.settings_workbench.tab)
                .unwrap_or(0);
            rotate(app);
            // Retained invalid buffers must remain reachable after a window departure.
            app.settings_workbench.navigation.input_edit = app
                .settings_workbench
                .draft
                .raw_inputs
                .keys()
                .min()
                .cloned();
        } else {
            let session = &mut app.settings_workbench;
            if app.visible && app.scene_motion.settings_opacity() > 0.0 {
                session.keep_outgoing_visuals();
            }
            session.clear_preview();
            session.save_notice = None;
            session.save_details_open = false;
            session.draft.depart();
            session.generation = session.generation.wrapping_add(1);
            session.capture = None;
            session.armed = None;
            session.navigation.input_edit = None;
            session.navigation.search_open = false;
        }
    }
    if !active {
        app.settings_workbench.visible_layout_options.clear();
        return;
    }
    app.settings_workbench.folders = app
        .config
        .catalog
        .local_folders
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    let model = cards(app);
    if app.settings_workbench.layout_studio() {
        let options: HashSet<_> = model
            .get(composed::PICKER_LAYOUT_SECTION)
            .into_iter()
            .flat_map(|(_, rows)| rows)
            .filter_map(|row| super::folio::workbench::layout_option_reveal_key(&row.control))
            .collect();
        if options != app.settings_workbench.visible_layout_options {
            let duration = Source { app }.motion_fast_ms();
            let session = &mut app.settings_workbench;
            for id in options.difference(&session.visible_layout_options) {
                let mut tween = Tween::for_duration_ms(0.0, duration);
                tween.retarget(1.0);
                session.bar_reveals.insert(id.clone(), tween);
            }
            session.visible_layout_options = options;
            session.last_frame = None;
        }
    } else {
        app.settings_workbench.visible_layout_options.clear();
    }
    let mut values = HashMap::new();
    fn collect(rows: &[composed::Row], source: &Source<'_>, values: &mut HashMap<String, String>) {
        for row in rows {
            match &row.control {
                Control::Number { key, path, .. }
                | Control::TextField { key, path, .. }
                | Control::KeyBinding { key, path, .. }
                | Control::Resolution { key, path, .. } => {
                    values.insert(key.clone(), source.text(path));
                }
                Control::MotionWeights { weights } => {
                    for (_, key, _) in weights {
                        values.insert(key.clone(), source.text(key));
                    }
                }
                Control::Details { rows, .. } | Control::StackBar { rows, .. } => {
                    collect(rows, source, values)
                }
                _ => {}
            }
        }
    }
    for (_, rows) in &model {
        collect(rows, &Source { app }, &mut values);
    }
    values.extend(app.settings_workbench.draft.raw_inputs.clone());
    let mode = app.state.gallery.mode;
    if let Some(id) = app.state.mode_profiles.selected_id(mode) {
        values.insert("profile-owner-mode".into(), mode.key().into());
        values.insert("profile-owner-id".into(), id.to_string());
    }
    app.settings_workbench.render_values = values;
    app.settings_workbench.palette = Some(crate::candidate_app::view::donor_palette(app));
    app.settings_workbench.results =
        composed::search::search_composed(&app.settings_search, &Source { app }, |tab| {
            super::bridge::build(app, tab)
        });
}
/// Retired source authoring is rejected before draft mutation or publication.
fn retired_source_transition_path(path: &str) -> bool {
    use composed::keys::transition as T;
    matches!(
        path,
        T::ENABLED
            | T::SHADER
            | T::DURATION_MS
            | T::FPS
            | T::PREVIEW
            | T::PREVIEW_FPS
            | T::FAMILY
            | T::SAND_FPS
            | T::SAND_PRIMARY
            | T::SAND_QUALITY
            | T::SAND_SCOPE
            | T::SAND_SHARP
            | T::SHADER_SCOPES
    ) || path.starts_with("transition.shaderScopes.")
}

fn persist_wallpaper_transition(
    app: &mut GpuCandidate,
    path: &str,
    value: &Value,
) -> Result<bool, String> {
    use crate::transition::TransitionEffect;
    use composed::keys::wallpaper_transition as T;
    use composed::transitions::{native_default_effect_for_family, Family};

    if !matches!(path, T::ENABLED | T::EFFECT | T::FAMILY | T::DURATION_MS) {
        return Ok(false);
    }

    let mut proposed = app.config.clone();
    match (path, value) {
        (T::ENABLED, Value::Boolean(enabled)) => {
            proposed.transition.effect = if *enabled {
                (proposed.transition.effect == TransitionEffect::Off)
                    .then_some(TransitionEffect::Crossfade)
                    .unwrap_or(proposed.transition.effect)
            } else {
                TransitionEffect::Off
            };
        }
        (T::EFFECT, Value::Text(effect)) => {
            proposed.transition.effect = TransitionEffect::parse_saved(effect)
                .map_err(|error| error.to_string())?
                .effect;
        }
        (T::FAMILY, Value::Text(family)) => {
            let family = match family.as_str() {
                "random" => Family::Random,
                "fade" => Family::Fade,
                "wipe" => Family::Wipe,
                "warp" => Family::Warp,
                "break" => Family::Break,
                "sand" => {
                    return Err("Sand transitions are unavailable in this Windows build.".into())
                }
                _ => return Err("Choose a listed transition family.".into()),
            };
            proposed.transition.effect = if family == Family::Random {
                TransitionEffect::Random
            } else {
                native_default_effect_for_family(family).ok_or_else(|| {
                    "The selected transition family has no native effects.".to_string()
                })?
            };
        }
        (T::DURATION_MS, Value::Number(duration_ms)) if duration_ms.is_finite() => {
            proposed.transition.duration_ms = duration_ms.round().clamp(100.0, 10_000.0) as u64;
        }
        (T::DURATION_MS, Value::Number(_)) => {
            return Err("Enter a finite transition duration.".into());
        }
        _ => return Err("The wallpaper transition value has the wrong type.".into()),
    }

    let backup_path = crate::config::Config::config_path().with_extension("toml.bak");
    let result = proposed.save_with_diagnostics();
    app.settings_workbench
        .announce_save(super::bridge::SaveNotice::config_result(
            "Wallpaper transition",
            result.as_ref(),
        ));
    match result {
        Ok(outcome) => {
            app.config = proposed;
            let effect = app.config.transition.effect;
            app.settings_workbench.draft.values.insert(
                T::ENABLED.to_string(),
                Value::Boolean(effect != TransitionEffect::Off),
            );
            app.settings_workbench
                .draft
                .values
                .insert(T::EFFECT.to_string(), Value::Text(effect.key().to_string()));
            app.settings_workbench.draft.values.insert(
                T::FAMILY.to_string(),
                Value::Text(
                    composed::transitions::native_family_of(effect)
                        .key()
                        .to_string(),
                ),
            );
            app.settings_workbench.draft.values.insert(
                T::DURATION_MS.to_string(),
                Value::Number(app.config.transition.duration_ms as f64),
            );
            app.config_save_failure = (!outcome.backup.is_available()).then(|| {
                format!(
                    "Wallpaper transition saved, but {}",
                    outcome.backup_description(&backup_path)
                )
            });
            app.settings_workbench.message = format!(
                "Wallpaper transition saved. {}",
                outcome.backup_description(&backup_path)
            );
            Ok(true)
        }
        Err(error) => {
            let failure = format!(
                "Wallpaper transition was not fully saved: {error}. {}",
                error.backup_description(&backup_path)
            );
            app.config_save_failure = Some(failure.clone());
            Err(failure)
        }
    }
}

fn edit_config_keybind(
    config: &mut crate::config::Config,
    path: &str,
    value: &Value,
) -> Result<Option<bool>, String> {
    use composed::keys::keybind as K;
    let slot = match path {
        K::CONFIG_APPLY_TO_ALL => &mut config.keybinds.apply_to_all,
        K::CONFIG_CLOSE => &mut config.keybinds.close,
        K::CONFIG_SWITCH_MONITOR => &mut config.keybinds.switch_monitor,
        K::CONFIG_TOGGLE_EXCLUDE => &mut config.keybinds.toggle_exclude,
        K::CONFIG_TOGGLE_HIDDEN => &mut config.keybinds.toggle_hidden,
        K::CONFIG_OPEN_EXPLORER => &mut config.keybinds.open_in_explorer,
        K::CONFIG_RELOAD => &mut config.keybinds.reload_config,
        _ => return Ok(None),
    };
    let Value::Text(binding) = value else {
        return Err("Application shortcuts require a text binding.".into());
    };
    let changed = *slot != *binding;
    slot.clone_from(binding);
    Ok(Some(changed))
}

fn publish_config_keybinds(
    app: &mut GpuCandidate,
    proposed: crate::config::Config,
) -> Result<super::bridge::SaveNotice, String> {
    use super::bridge::SaveNotice;
    if app.launch_policy.is_preview() {
        app.settings_workbench.announce_save(SaveNotice::Rejected);
        return Err("Application shortcuts cannot be saved in Preview.".into());
    }
    let backup_path = crate::config::Config::config_path().with_extension("toml.bak");
    let result = proposed.save_with_diagnostics();
    let notice = SaveNotice::config_result("Config", result.as_ref());
    app.settings_workbench.announce_save(notice);
    match result {
        Ok(outcome) => {
            app.config = proposed;
            app.config_save_failure = (!outcome.backup.is_available()).then(|| {
                format!(
                    "Application shortcuts saved, but {}",
                    outcome.backup_description(&backup_path)
                )
            });
            Ok(notice)
        }
        Err(error) => {
            let failure = format!(
                "Application shortcut publication needs attention. {error}. {}",
                error.backup_description(&backup_path)
            );
            app.config_save_failure = Some(failure.clone());
            if error.primary_committed {
                app.config = proposed;
                Ok(notice)
            } else {
                Err(failure)
            }
        }
    }
}

fn persist_config_keybind(
    app: &mut GpuCandidate,
    path: &str,
    value: &Value,
) -> Result<bool, String> {
    let mut proposed = app.config.clone();
    let Some(changed) = edit_config_keybind(&mut proposed, path, value)? else {
        return Ok(false);
    };
    if changed {
        publish_config_keybinds(app, proposed)?;
    }
    app.settings_workbench
        .draft
        .values
        .insert(path.into(), value.clone());
    Ok(true)
}

fn save_captured_bindings(
    app: &mut GpuCandidate,
    changes: Vec<composed::binding_capture::Change>,
) -> Result<(), String> {
    use super::bridge::SaveNotice;
    let mut proposed_config = app.config.clone();
    let mut proposed = app.state.clone();
    let mut config_changed = false;
    for change in &changes {
        let value = Value::Text(change.binding.clone());
        if let Some(changed) = edit_config_keybind(&mut proposed_config, change.path, &value)? {
            config_changed |= changed;
        } else {
            super::preference_projection::write(
                &mut proposed.frontend_preferences,
                change.path,
                &value,
            )?;
        }
    }
    proposed.frontend_preferences.validate()?;
    let local_changed = proposed.frontend_preferences != app.state.frontend_preferences;
    let config_notice = if config_changed {
        let notice = publish_config_keybinds(app, proposed_config)?;
        for change in changes
            .iter()
            .filter(|change| change.action.is_config_global())
        {
            app.settings_workbench
                .draft
                .values
                .insert(change.path.into(), Value::Text(change.binding.clone()));
        }
        Some(notice)
    } else {
        None
    };
    if local_changed {
        if let Err(error) = app.commit_local_state_change(proposed) {
            if config_changed || error.primary_committed {
                app.settings_workbench
                    .announce_save(if error.primary_committed {
                        SaveNotice::RecoveryWarning("Shortcuts")
                    } else {
                        SaveNotice::Partial
                    });
                rotate(app);
            }
            return Err(app
                .save_failure
                .clone()
                .unwrap_or_else(|| error.to_string()));
        }
    }
    if matches!(config_notice, Some(SaveNotice::RecoveryWarning(_))) {
        app.settings_workbench
            .announce_save(SaveNotice::RecoveryWarning("Shortcuts"));
    } else if (local_changed && app.backup_warning.is_some())
        || (config_changed && app.config_save_failure.is_some())
    {
        app.settings_workbench
            .announce_save(SaveNotice::BackupWarning("Shortcuts"));
    } else if config_changed || local_changed {
        app.settings_workbench
            .announce_save(SaveNotice::Saved("Shortcuts"));
    }
    for change in changes {
        app.settings_workbench
            .draft
            .values
            .insert(change.path.into(), Value::Text(change.binding));
    }
    rotate(app);
    Ok(())
}

/// A single narrow adapter into CandidateState's existing publication owner.
fn persist_preference(app: &mut GpuCandidate, path: &str, value: &Value) -> Result<bool, String> {
    if persist_config_keybind(app, path, value)? {
        return Ok(true);
    }
    if persist_wallpaper_transition(app, path, value)? {
        return Ok(true);
    }
    let mut proposed = app.state.clone();
    if matches!(
        path,
        "restoreOnStartup"
            | "general.randomIncludeStatic"
            | "general.randomIncludeVideo"
            | "general.randomIncludeWE"
            | "general.randomIncludeFavourites"
    ) {
        let Value::Boolean(enabled) = value else {
            return Err("Execution policy requires an explicit boolean save.".into());
        };
        if path == "restoreOnStartup" {
            proposed.local_library.startup.reapply_enabled = *enabled;
        } else {
            let mut rotation = app.rotation_edit_preferences();
            match path {
                "general.randomIncludeStatic" => rotation.images = *enabled,
                "general.randomIncludeVideo" => rotation.videos = *enabled,
                "general.randomIncludeWE" => rotation.we = *enabled,
                "general.randomIncludeFavourites" => rotation.favourites_only = *enabled,
                _ => unreachable!(),
            }
            proposed.local_library.rotation = Some(rotation);
        }
        app.commit_local_state_change(proposed).map_err(|error| {
            app.save_failure
                .clone()
                .unwrap_or_else(|| error.to_string())
        })?;
        app.settings_workbench.message = app.automation_policy_status();
        return Ok(true);
    }
    let resolution = path.starts_with(&format!(
        "{}.",
        composed::keys::filter_bar::RESOLUTION_PRESETS
    ));
    let owned = if resolution {
        proposed.frontend_preferences.resolution_presets =
            app.settings_workbench.lists.resolutions.clone();
        true
    } else {
        super::preference_projection::write(&mut proposed.frontend_preferences, path, value)?
    };
    if !owned {
        return Ok(false);
    }
    if proposed.frontend_preferences != app.state.frontend_preferences {
        app.commit_local_state_change(proposed).map_err(|error| {
            app.save_failure
                .clone()
                .unwrap_or_else(|| error.to_string())
        })?;
    }
    app.settings_workbench.message = "Local preference saved.".into();
    if path.starts_with("filterBar.show.sort.")
        && !app
            .sort_order
            .visible(&app.state.frontend_preferences.visibility)
    {
        app.sort_order = SortOrder::ALL
            .into_iter()
            .find(|o| o.visible(&app.state.frontend_preferences.visibility))
            .unwrap_or_default();
        app.refresh_filter();
    }
    if path.starts_with("motion.") || path == "launch.animation" || path == "general.openFadeFrom" {
        app.schedule_posters();
    }
    if path == composed::keys::system::MONITOR {
        if let Some(desired) = &app.state.frontend_preferences.picker.desired_monitor_id {
            if let Some(current) = app
                .monitor_ids
                .iter()
                .find(|id| id.eq_ignore_ascii_case(desired))
            {
                app.active_monitor_id = Some(current.clone());
            }
        }
    }
    Ok(true)
}
/// Project only a source control's explicit current geometry owner.
fn project_geometry_value(
    state: &mut CandidateState,
    path: &str,
    value: &Value,
) -> Result<bool, String> {
    use composed::keys::selector as K;
    if let (Some((field, factor)), Value::Number(number)) = (gallery_field(path), value) {
        if !number.is_finite() {
            return Err("Enter a finite geometry value.".into());
        }
        if field == GalleryNumberField::DepthFlipDuration {
            state.card_flip_mut().duration_ms = number.round().clamp(100.0, 5000.0) as u64;
        } else {
            if state.gallery.mode != field.mode() {
                return Err("This geometry control no longer owns the current mode.".into());
            }
            let narrowed = (*number / f64::from(factor)) as f32;
            if !narrowed.is_finite() {
                return Err("This geometry value cannot be represented.".into());
            }
            set_gallery_number_value(state, field, narrowed);
        }
        return Ok(true);
    }
    if super::geometry_projection::write(state, path, value)? {
        return Ok(true);
    }
    match (path, value) {
        (K::DEPTH_SHADOWS, Value::Boolean(value)) => state.gallery.depth.shadows = *value,
        (K::DEPTH_SELECTION_FRAME, Value::Boolean(value)) => {
            state.gallery.depth.selection_frame = *value
        }
        (K::SLICE_WOBBLE, Value::Boolean(value)) => state.gallery.slices.wobble = *value,
        _ => return Ok(false),
    }
    state.gallery.variant = None;
    Ok(true)
}

fn refresh_geometry(app: &mut GpuCandidate, previous: &candidate_state::GallerySettings) {
    app.gallery_settings_changed(previous);
}

/// Preview is transient. The existing app persistence facade excludes this
/// baseline from unrelated saves until this field's explicit Commit succeeds.
fn preview_geometry(app: &mut GpuCandidate, path: &str, value: &Value) -> Result<bool, String> {
    if path == composed::keys::selector::FLIP_DURATION_MS {
        return Ok(false);
    }
    if app.state.write_restriction.is_some() || app.state.reconciliation_required.is_some() {
        return Ok(false);
    }
    let mut proposed = app.state.clone();
    if !project_geometry_value(&mut proposed, path, value)? {
        return Ok(false);
    }
    if proposed.gallery != app.state.gallery {
        let previous = app.state.gallery.clone();
        app.settings_workbench
            .geometry_preview_baseline
            .get_or_insert_with(|| previous.clone());
        app.state.gallery = proposed.gallery;
        refresh_geometry(app, &previous);
    }
    Ok(true)
}

fn cancel_geometry_preview(app: &mut GpuCandidate) {
    if let Some(baseline) = app.settings_workbench.geometry_preview_baseline.take() {
        let previous = std::mem::replace(&mut app.state.gallery, baseline);
        refresh_geometry(app, &previous);
    }
}

fn commit_geometry(app: &mut GpuCandidate, path: &str, value: &Value) -> Result<bool, String> {
    let render_previous = app.state.gallery.clone();
    let mut proposed = app.state.clone();
    if !project_geometry_value(&mut proposed, path, value)? {
        return Ok(false);
    }
    let baseline = app.settings_workbench.geometry_preview_baseline.take();
    let previous = baseline
        .clone()
        .unwrap_or_else(|| app.state.gallery.clone());
    let before_flip = app.state.card_flip().clone();
    // Editing away and back must not clear the original curated identity.
    // Compare canonical geometry independently of the transient CUSTOM marker.
    let mut comparable = proposed.gallery.clone();
    comparable.variant = previous.variant;
    if comparable == previous {
        proposed.gallery.variant = previous.variant;
    }
    // A preview may equal the proposal while disk still holds the baseline.
    let changed = proposed.gallery != previous || proposed.card_flip() != &before_flip;
    if changed {
        if let Err(error) = app.commit_local_state_change(proposed) {
            let failed_preview = std::mem::replace(&mut app.state.gallery, previous);
            app.settings_workbench.geometry_preview_baseline = baseline;
            refresh_geometry(app, &failed_preview);
            return Err(app
                .save_failure
                .clone()
                .unwrap_or_else(|| error.to_string()));
        }
        refresh_geometry(app, &render_previous);
        if before_flip != *app.state.card_flip() {
            app.card_flip_changed(&before_flip);
        }
    } else {
        app.state.gallery = proposed.gallery;
    }
    app.settings_workbench.message = if changed {
        "Mode-local configuration saved."
    } else {
        "No geometry changes to save."
    }
    .into();
    Ok(true)
}

fn persist_collection_root(app: &mut GpuCandidate, root: &str) -> Result<(), String> {
    let mut proposed = app.state.clone();
    super::pending_projection::write_collection(
        &mut proposed,
        &app.settings_workbench.lists,
        root,
    )?;
    if proposed.pending_frontend != app.state.pending_frontend {
        app.commit_local_state_change(proposed).map_err(|error| {
            app.save_failure
                .clone()
                .unwrap_or_else(|| error.to_string())
        })?;
    }
    app.settings_workbench.message = match root {
        "postProcessing" => "Saved pending. Post-processing command execution is unavailable; no command was run.",
        "integrations" => "Saved pending. External file generation and reload commands are unavailable; no output was written.",
        "semantic.models" => "Saved pending. Semantic model import and indexing are unavailable; no model was loaded.",
        _ => return Err("Unknown collection owner.".into()),
    }.into();
    Ok(())
}
fn persist_collection(app: &mut GpuCandidate, path: &str, value: &Value) -> Result<bool, String> {
    let Some(root) = super::pending_projection::collection_owner(path) else {
        return Ok(false);
    };
    super::pending_projection::collection_editable(&app.state, root)?;
    let value = match value {
        Value::Text(text) => text.clone(),
        Value::Boolean(value) => value.to_string(),
        _ => return Err("This collection field requires text or a boolean.".into()),
    };
    app.settings_workbench.lists.edit(path, &value)?;
    persist_collection_root(app, root)?;
    Ok(true)
}

fn persist_pending(app: &mut GpuCandidate, path: &str, value: &Value) -> Result<bool, String> {
    let mut proposed = app.state.clone();
    if !super::pending_projection::write(&mut proposed, path, value)? {
        return Ok(false);
    }
    let changed = proposed.pending_frontend != app.state.pending_frontend;
    if changed {
        app.commit_local_state_change(proposed).map_err(|error| {
            app.save_failure
                .clone()
                .unwrap_or_else(|| error.to_string())
        })?;
    }
    let spec = candidate_state::pending::spec(path).expect("matched exact pending field");
    let reason = match spec.capability {
        candidate_state::frontend::DeferredCapability::ThemeGeneration => {
            "Theme generation is unavailable in this build."
        }
        candidate_state::frontend::DeferredCapability::ExpandedScheduling => {
            "Advanced scheduling and post-processing are unavailable in this build."
        }
        candidate_state::frontend::DeferredCapability::MediaPlaybackPolicy => {
            "This playback setting is unavailable on the current provider."
        }
        candidate_state::frontend::DeferredCapability::SourceBrowsing => {
            "Online browsing and downloads are unavailable in this build."
        }
        candidate_state::frontend::DeferredCapability::SemanticSearch => {
            "Semantic indexing and search are unavailable in this build."
        }
        candidate_state::frontend::DeferredCapability::PlatformIntegration => {
            "This platform feature is unavailable in this Windows build."
        }
        candidate_state::frontend::DeferredCapability::Optimization => {
            "Automatic optimization and trash removal are unavailable in this build."
        }
        _ => "This operation is unavailable in this build.",
    };
    let reason = if matches!(
        path,
        "library.pollingFallback" | "library.pollingIntervalSeconds"
    ) {
        "Automatic library watching is unavailable; Refresh catalog still works."
    } else {
        reason
    };
    app.settings_workbench.message = format!("Saved pending. {reason}");
    if changed
        && matches!(
            app.settings_workbench.save_notice,
            Some(super::bridge::SaveNotice::Saved(_))
        )
    {
        app.settings_workbench
            .announce_save(super::bridge::SaveNotice::Pending("Settings"));
    }
    Ok(true)
}

fn set_theme_target(app: &mut GpuCandidate, provider: &str, enabled: bool) -> Result<(), String> {
    if !["noctalia", "dms"].contains(&provider) {
        return Err("Choose a listed Theme output target.".into());
    }
    let mut targets = match &app.settings_workbench.theme_targets_draft {
        Some(targets) => targets.clone(),
        None => super::pending_projection::target_names(&app.state)?,
    };
    targets.retain(|target| target != provider);
    if enabled {
        targets.push(provider.into());
    }
    app.settings_workbench.theme_targets_draft = Some(targets.clone());
    let mut proposed = app.state.clone();
    super::pending_projection::put(
        &mut proposed,
        candidate_state::frontend::DeferredCapability::ThemeGeneration,
        "theme.targets",
        serde_json::json!(targets),
    )?;
    app.commit_local_state_change(proposed).map_err(|error| {
        app.save_failure
            .clone()
            .unwrap_or_else(|| error.to_string())
    })?;
    app.settings_workbench.theme_targets_draft = None;
    app.settings_workbench.message = "Saved pending Theme output targets. External theme generation and retheme execution are unavailable.".into();
    Ok(())
}
fn commit_path(app: &mut GpuCandidate, path: &str) -> Result<Task<Message>, String> {
    if let Some(error) = app.settings_workbench.draft.errors.get(path) {
        return Err(error.clone());
    }
    let session = &mut app.settings_workbench;
    if let Some(Value::Text(value)) = session.draft.values.get(path) {
        if session.lists.text(path).is_some() {
            session.lists.edit(path, value)?;
        }
    }
    if let Some(value) = session.draft.values.get(path).cloned() {
        if persist_preference(app, path, &value)?
            || persist_collection(app, path, &value)?
            || persist_pending(app, path, &value)?
            || commit_geometry(app, path, &value)?
        {
            app.settings_workbench.draft.commit_input(path)?;
            return Ok(Task::none());
        }
    }
    let value = app.settings_workbench.draft.commit_input(path)?;
    Ok(value.map_or_else(Task::none, |value| apply_value(app, path, value)))
}
fn commit(app: &mut GpuCandidate) -> Result<Task<Message>, (Task<Message>, String)> {
    let errors = &app.settings_workbench.draft.errors;
    if let Some(error) = app
        .settings_workbench
        .navigation
        .input_edit
        .as_ref()
        .and_then(|path| errors.get(path))
        .or_else(|| {
            errors
                .iter()
                .min_by_key(|(path, _)| *path)
                .map(|(_, error)| error)
        })
    {
        return Err((Task::none(), error.clone()));
    }
    let mut paths: Vec<_> = app
        .settings_workbench
        .draft
        .raw_inputs
        .keys()
        .cloned()
        .collect();
    paths.sort_unstable();
    let mut tasks = Vec::new();
    for path in paths {
        match commit_path(app, &path) {
            Ok(task) => tasks.push(task),
            Err(error) => return Err((Task::batch(tasks), error)),
        }
    }
    Ok(Task::batch(tasks))
}
fn input(
    app: &mut GpuCandidate,
    generation: u64,
    model: &[(composed::Card, Vec<composed::Row>)],
    path: String,
    raw: String,
) -> Task<Message> {
    let control = find_control(model, &path);
    let Some(control) = control else {
        return Task::none();
    };
    let prior_task = if let Some(active) = app
        .settings_workbench
        .navigation
        .input_edit
        .clone()
        .filter(|active| active != &path)
    {
        match commit_path(app, &active) {
            Ok(task) => task,
            Err(error) => return blocked_edit_task(app, error),
        }
    } else {
        Task::none()
    };
    let effective = match control {
        Control::Number { .. } | Control::MotionWeights { .. } => {
            Value::Number(Source { app }.number(&path))
        }
        _ => Value::Text(Source { app }.text(&path)),
    };
    let session = &mut app.settings_workbench;
    session.draft.begin_input(&path, effective);
    session.navigation.input_edit = Some(path.clone());
    let previous = session.draft.values.get(&path).cloned();
    let value = session.draft.input(generation, model, &path, raw);
    if let Some(Value::Text(text)) = &value {
        if session.lists.text(&path).is_some() {
            if let Err(error) = session.lists.edit(&path, text) {
                match previous {
                    Some(value) => {
                        session.draft.values.insert(path.clone(), value);
                    }
                    None => {
                        session.draft.values.remove(&path);
                    }
                }
                session.draft.errors.insert(path, error);
                return prior_task;
            }
        }
    }
    if let Some(value) = value {
        if let Some(spec) = candidate_state::pending::spec(&path) {
            let json = match &value {
                Value::Boolean(v) => (*v).into(),
                Value::Number(v) => (*v).into(),
                Value::Text(v) => v.clone().into(),
            };
            if let Err(error) = candidate_state::pending::normalize(spec.capability, &path, json) {
                app.settings_workbench
                    .draft
                    .errors
                    .insert(path, error.clone());
                app.settings_workbench.message = error;
                return prior_task;
            }
        }
        if let Err(error) = preview_geometry(app, &path, &value) {
            app.settings_workbench
                .draft
                .errors
                .insert(path, error.clone());
            app.settings_workbench.message = error;
        }
    }
    prior_task
}
fn find_control<'a>(
    model: &'a [(composed::Card, Vec<composed::Row>)],
    path: &str,
) -> Option<&'a Control> {
    fn rows<'a>(items: &'a [composed::Row], path: &str) -> Option<&'a Control> {
        for row in items {
            match &row.control {
                Control::Number { path: key, .. }
                | Control::TextField { path: key, .. }
                | Control::Resolution { path: key, .. }
                | Control::KeyBinding { path: key, .. }
                | Control::Toggle { path: key, .. }
                | Control::Dropdown { path: key, .. }
                | Control::Chips { path: key, .. }
                    if key == path =>
                {
                    return Some(&row.control)
                }
                Control::MotionWeights { weights }
                    if weights.iter().any(|(_, key, _)| key == path) =>
                {
                    return Some(&row.control)
                }
                Control::Details { rows: children, .. }
                | Control::StackBar { rows: children, .. } => {
                    if let Some(found) = rows(children, path) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    model.iter().find_map(|(_, items)| rows(items, path))
}
fn apply_value(app: &mut GpuCandidate, path: &str, value: Value) -> Task<Message> {
    use composed::keys::{general as G, selector as S, wallpaper as W};
    if retired_source_transition_path(path) {
        return Task::none();
    }
    if path == "appearance.presentation" {
        if let Value::Text(value) = value {
            let presentation = match value.as_str() {
                "wallpaper" => Some(BackgroundPresentation::WallpaperBackdrop),
                "live" => Some(BackgroundPresentation::LiveTransparentOverlay),
                _ => None,
            };
            if let Some(presentation) = presentation {
                let task = crate::candidate_app::events::update(
                    app,
                    Message::SetBackgroundPresentation(presentation),
                );
                app.settings_workbench.message = app
                    .save_failure
                    .clone()
                    .unwrap_or_else(|| "Background presentation updated.".into());
                return task;
            }
        }
        return Task::none();
    }

    match persist_collection(app, path, &value) {
        Ok(true) => return Task::none(),
        Err(error) => {
            app.settings_workbench.message = error;
            return Task::none();
        }
        Ok(false) => {}
    }
    match persist_pending(app, path, &value) {
        Ok(true) => return Task::none(),
        Err(error) => {
            app.settings_workbench.message = error;
            return Task::none();
        }
        Ok(false) => {}
    }

    match commit_geometry(app, path, &value) {
        Ok(true) => return Task::none(),
        Err(error) => {
            app.settings_workbench.message = error;
            return Task::none();
        }
        Ok(false) => {}
    }

    match persist_preference(app, path, &value) {
        Ok(true) => return Task::none(),
        Err(error) => {
            app.settings_workbench.message = error;
            return Task::none();
        }
        Ok(false) => {}
    }

    if let Some(target) = path.strip_prefix(&format!("{}.", composed::keys::display::OUTPUT_LOCKS))
    {
        if let Value::Boolean(locked) = value {
            if app.monitor_ids.iter().any(|id| id == target)
                && app.state.locked_monitors.contains(target) != locked
            {
                return crate::candidate_app::events::update(
                    app,
                    Message::ToggleMonitorLock(target.into()),
                );
            }
        }
        return Task::none();
    }
    if let Value::Number(number) = value {
        if let Some((field, factor)) = gallery_field(path) {
            return crate::candidate_app::events::update(
                app,
                Message::SetGalleryNumber(field, (number / f64::from(factor)) as f32),
            );
        }
        if path == G::RANDOM_INTERVAL {
            app.set_automation_interval(number.max(10.0) as u64, unix_now());
        } else if path == W::VOLUME {
            app.submit_engine_command(EngineCommand::SetVolume(number.clamp(0.0, 100.0) as u8));
        } else {
            app.settings_workbench.message =
                "Local configuration updated; execution remains pending where unavailable.".into();
        }
        return Task::none();
    }
    let next = match (path, value) {
        (S::DISPLAY_MODE, Value::Text(mode)) => {
            Some(Message::SetGalleryMode(match mode.as_str() {
                "depth" => GalleryMode::Depth,
                "hex" => GalleryMode::Geometric,
                "wall" => GalleryMode::Wall,
                _ => GalleryMode::Slices,
            }))
        }
        (S::HEX_CURVE, Value::Text(value)) => {
            Some(Message::SetGeometricCurve(match value.as_str() {
                "flat" => StateGeometricCurve::Flat,
                "arc" => StateGeometricCurve::Arc,
                "wave" => StateGeometricCurve::Wave,
                "s" => StateGeometricCurve::S,
                "cylinder" => StateGeometricCurve::Cylinder,
                _ => return Task::none(),
            }))
        }
        (S::HEX_SHAPE, Value::Text(value)) => {
            Some(Message::SetGeometricShape(match value.as_str() {
                "triangle" => StateGeometricShape::Triangle,
                "hexagon" => StateGeometricShape::Hexagon,
                "diamond" => StateGeometricShape::Diamond,
                "rhombus" => StateGeometricShape::Rhombus,
                _ => return Task::none(),
            }))
        }
        (S::GRID_LAYOUT, Value::Text(value)) => {
            Some(Message::SetWallLayout(match value.as_str() {
                "uniform" => StateWallLayout::Uniform,
                "brick" => StateWallLayout::Brick,
                "masonry" => StateWallLayout::Masonry,
                "justified" => StateWallLayout::Justified,
                "editorial" => StateWallLayout::Editorial,
                "cylinder" => StateWallLayout::Cylinder,
                _ => return Task::none(),
            }))
        }
        (composed::keys::motion::FILTER_SWAP_SPEED, Value::Text(value)) => {
            let speed = match value.as_str() {
                "fast" => crate::candidate_state::GalleryMotionSpeed::Fast,
                "standard" => crate::candidate_state::GalleryMotionSpeed::Standard,
                "slow" => crate::candidate_state::GalleryMotionSpeed::Slow,
                _ => return Task::none(),
            };
            if app.state.gallery_motion_speed != speed {
                app.state.gallery_motion_speed = speed;
                if app.gallery_mode == GalleryMode::Depth && app.depth_roll.is_some() {
                    app.start_motion(0.0);
                }
                app.persist_state();
            }
            None
        }
        (S::DEPTH_SHADOWS, Value::Boolean(value)) => Some(Message::SetDepthShadows(value)),
        (S::DEPTH_SELECTION_FRAME, Value::Boolean(value)) => Some(Message::SetDepthFrame(value)),
        (S::SLICE_WOBBLE, Value::Boolean(value)) => Some(Message::SetSliceWobble(value)),
        (S::FLIP_EFFECT, Value::Text(value)) => Some(Message::SetDepthEffect(value)),
        (S::FLIP_SHADER, Value::Boolean(value)) => Some(Message::SetDepthShader(value)),
        (S::FLIP_BACK_REVEAL, Value::Boolean(value)) => Some(Message::SetDepthBack(value)),
        (G::RANDOM_ROTATE, Value::Boolean(value))
            if value != app.state.automation_intent().random_enabled =>
        {
            Some(Message::ToggleRandomRotation)
        }
        (W::MUTE, Value::Boolean(value)) => {
            app.submit_engine_command(EngineCommand::SetMuted(value));
            None
        }
        _ => {
            app.settings_workbench.message =
                "Local configuration updated; execution remains pending where unavailable.".into();
            None
        }
    };
    next.map_or_else(Task::none, |message| {
        crate::candidate_app::events::update(app, message)
    })
}
fn restart_tab_reveal(app: &mut GpuCandidate) {
    let mut tween = Tween::for_duration_ms(0.0, Source { app }.motion_fast_ms());
    tween.retarget(1.0);
    app.settings_workbench.tab_reveal = Some(tween);
    app.settings_workbench.last_frame = None;
}
pub(super) fn select_tab(app: &mut GpuCandidate, tab: String) {
    let tabs = composed::visible_tabs(&Source { app });
    let Some(index) = tabs.iter().position(|(key, _)| *key == tab) else {
        return;
    };
    let was_theme = crate::candidate_app::lifecycle::theme_owner_active(app);
    app.panel_state.settings_category = match tab.as_str() {
        "filter" => SettingsCategory::CatalogFilters,
        "position" => SettingsCategory::Position,
        "displays" => SettingsCategory::WallpapersDisplays,
        "motion" => SettingsCategory::Motion,
        "playback" => SettingsCategory::Playback,
        "library" => SettingsCategory::Library,
        "sources" => SettingsCategory::Sources,
        "automation" => SettingsCategory::Automation,
        "theme" => SettingsCategory::Theme,
        _ => SettingsCategory::Picker,
    };
    if app.settings_workbench.tab != tab {
        restart_tab_reveal(app);
    }
    if app.settings_workbench.tab == "library" && tab != "library" {
        app.folder_menu_open = false;
    }
    app.settings_workbench.save_notice = None;
    app.settings_workbench.save_details_open = false;
    app.settings_workbench.tab = tab;
    app.settings_workbench.legacy_category = Some(app.panel_state.settings_category);
    app.settings_workbench.windows_panel = None;
    crate::candidate_app::lifecycle::cancel_theme_transient_if_departed(app, was_theme);
    app.settings_workbench.navigation.category = index;
    app.settings_workbench.navigation.section = 0;
    app.settings_workbench.navigation.row = 0;
    app.settings_workbench.navigation.choice = None;
    app.settings_workbench.navigation.input_edit = None;
    app.settings_workbench.navigation.search_open = false;
    app.settings_search.clear();
    rotate(app);
}
fn action_visible(model: &[(composed::Card, Vec<composed::Row>)], action: ActionId) -> bool {
    fn rows(items: &[composed::Row], action: ActionId) -> bool {
        items.iter().any(|row| match &row.control {
            Control::ActionBtn { id, .. } | Control::ToggleAction { id, .. } => *id == action,
            Control::ActionChips { items } => items.iter().any(|(id, _)| *id == action),
            Control::MotionWeights { weights } => weights.iter().any(|(_, _, id)| *id == action),
            Control::Details { rows: children, .. } | Control::StackBar { rows: children, .. } => {
                rows(children, action)
            }
            _ => false,
        })
    }
    model.iter().any(|(_, items)| rows(items, action))
}
fn background_visible(model: &[(composed::Card, Vec<composed::Row>)], output: &str) -> bool {
    fn rows(items: &[composed::Row], output: &str) -> bool {
        items.iter().any(|row| match &row.control {
            Control::Chips {
                background: Some(background),
                ..
            } => background.output == output,
            Control::Details { rows: children, .. } | Control::StackBar { rows: children, .. } => {
                rows(children, output)
            }
            _ => false,
        })
    }
    model.iter().any(|(_, items)| rows(items, output))
}
fn background_paths(output: &str) -> (String, String) {
    use composed::keys::display as D;
    if output.is_empty() {
        (D::BACKGROUND_MODE.into(), D::FILL_COLOR.into())
    } else {
        (
            format!("{}.{output}", D::BACKGROUND_MODES),
            format!("{}.{output}", D::BACKGROUND_COLORS),
        )
    }
}
fn persist_background(app: &mut GpuCandidate, output: &str) -> Result<(), String> {
    let (mode_path, color_path) = background_paths(output);
    let control = composed::background::BackgroundControl::new(&Source { app }, output);
    let mode = app
        .settings_workbench
        .draft
        .values
        .get(&mode_path)
        .cloned()
        .unwrap_or(Value::Text(if control.inherited {
            "inherit".into()
        } else {
            control.mode
        }));
    let color = app
        .settings_workbench
        .draft
        .values
        .get(&color_path)
        .cloned()
        .unwrap_or(Value::Text(control.color));
    let mut proposed = app.state.clone();
    if output.is_empty() {
        super::pending_projection::write(&mut proposed, &mode_path, &mode)?;
        super::pending_projection::write(&mut proposed, &color_path, &color)?;
    } else {
        for (root, value) in [
            (composed::keys::display::BACKGROUND_MODES, mode),
            (composed::keys::display::BACKGROUND_COLORS, color),
        ] {
            let mut map = super::pending_projection::checked_root(&app.state, root)?
                .unwrap_or_else(|| serde_json::json!({}));
            let Value::Text(value) = value else {
                return Err("Background requires a source text value.".into());
            };
            map.as_object_mut()
                .ok_or("Invalid saved background map; original data retained.")?
                .insert(output.into(), value.into());
            super::pending_projection::put(
                &mut proposed,
                candidate_state::frontend::DeferredCapability::PlatformIntegration,
                root,
                map,
            )?;
        }
    }
    app.commit_local_state_change(proposed)
        .map_err(|e| app.save_failure.clone().unwrap_or_else(|| e.to_string()))?;
    app.settings_workbench.message="Saved pending background. Per-display blur/colour execution is unavailable; the desktop is unchanged.".into();
    Ok(())
}
fn processes(app: &GpuCandidate) -> Vec<String> {
    Source { app }
        .text(composed::keys::playback::PROCESSES)
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect()
}
fn edit_process(app: &mut GpuCandidate, name: &str, add: bool) -> bool {
    let name = name.trim();
    if name.is_empty() || name.contains([',', '\n', '\r']) {
        app.settings_workbench.message = "Enter a single process name.".into();
        if let Some(picker) = &mut app.settings_workbench.process_picker {
            picker.manual_error = Some(app.settings_workbench.message.clone());
        }
        app.settings_workbench
            .announce_save(super::bridge::SaveNotice::Rejected);
        return false;
    }
    let mut names = processes(app);
    if add {
        if !names.iter().any(|existing| existing == name) {
            names.push(name.into());
        }
    } else {
        names.retain(|existing| existing != name);
    }
    let value = Value::Text(names.join(", "));
    match persist_pending(app, composed::keys::playback::PROCESSES, &value) {
        Ok(true) => {
            app.settings_workbench
                .draft
                .values
                .insert(composed::keys::playback::PROCESSES.into(), value);
            if let Some(picker) = &mut app.settings_workbench.process_picker {
                picker.targets = names;
                picker.manual_error = None;
            }
            true
        }
        Ok(false) => {
            app.settings_workbench.message = "This process form has no saved pending owner.".into();
            false
        }
        Err(error) => {
            app.settings_workbench
                .draft
                .values
                .insert(composed::keys::playback::PROCESSES.into(), value);
            app.settings_workbench.message = error;
            false
        }
    }
}
fn retire_indexed_buffers(app: &mut GpuCandidate) {
    let roots = [
        composed::keys::filter_bar::RESOLUTION_PRESETS,
        composed::keys::post_processing::LIST,
        composed::keys::integrations::LIST,
        composed::keys::semantic::MODELS,
    ];
    let paths: Vec<_> = app
        .settings_workbench
        .draft
        .raw_inputs
        .keys()
        .filter(|path| {
            roots
                .iter()
                .any(|root| path.starts_with(&format!("{root}.")))
        })
        .cloned()
        .collect();
    for path in paths {
        app.settings_workbench.draft.cancel_input(&path);
    }
    app.settings_workbench.draft.values.retain(|path, _| {
        !roots
            .iter()
            .any(|root| path.starts_with(&format!("{root}.")))
    });
    app.settings_workbench.navigation.input_edit = None;
}
fn blocked_edit_task(app: &mut GpuCandidate, error: String) -> Task<Message> {
    app.settings_workbench.message = error;
    if !app.settings_workbench.draft.errors.is_empty() {
        app.settings_workbench
            .announce_save(super::bridge::SaveNotice::Rejected);
    }
    let Some(path) = &app.settings_workbench.navigation.input_edit else {
        return Task::none();
    };
    let key = if path.ends_with(".from") || path.ends_with(".to") {
        format!(
            "{path}.{}",
            if app.settings_workbench.navigation.resolution_height {
                "height"
            } else {
                "width"
            }
        )
    } else {
        path.clone()
    };
    iced::widget::operation::focus(composed::workbench_input_id(&key))
}
pub(in crate::candidate_app) fn finish_geometry_owner_edit(
    app: &mut GpuCandidate,
) -> Result<Task<Message>, Task<Message>> {
    if app.settings_workbench.geometry_preview_baseline.is_some()
        || !app.settings_workbench.draft.raw_inputs.is_empty()
    {
        return finish_pointer_edit(app);
    }
    Ok(Task::none())
}

pub(in crate::candidate_app) fn finish_pointer_edit(
    app: &mut GpuCandidate,
) -> Result<Task<Message>, Task<Message>> {
    match commit(app) {
        Ok(task) => {
            app.settings_workbench.navigation.input_edit = None;
            Ok(task)
        }
        Err((task, error)) => Err(Task::batch([task, blocked_edit_task(app, error)])),
    }
}
fn row_has_id(control: &Control, id: &str, details: bool) -> bool {
    if details {
        if let Control::AppTheme { app } = control {
            if id == format!("app-theme:{}", app.id) {
                return true;
            }
        }
        if let Control::Details { id: current, .. } = control {
            if current == id {
                return true;
            }
        }
    } else if control.compact_bar_id().as_deref() == Some(id) {
        return true;
    }
    match control {
        Control::Details { rows, .. } | Control::StackBar { rows, .. } => {
            rows.iter().any(|row| row_has_id(&row.control, id, details))
        }
        _ => false,
    }
}
pub(in crate::candidate_app) fn dispatch(
    app: &mut GpuCandidate,
    generation: u64,
    message: intent::Message,
) -> Task<Message> {
    if !app.settings_workbench.active
        || app.settings_workbench.owner_window != app.window
        || !app.settings_workbench.draft.is_current(generation)
    {
        return Task::none();
    }
    if let intent::Message::Settings(
        SettingsMsg::Input(path, _)
        | SettingsMsg::Toggle(path, _)
        | SettingsMsg::Pick(path, _)
        | SettingsMsg::ResolutionInput(path, _, _)
        | SettingsMsg::ResolutionLimit(path, _),
    ) = &message
    {
        if retired_source_transition_path(path) {
            return Task::none();
        }
    }
    let model = cards(app);
    match message {
        intent::Message::RefreshCatalog => {
            if app.settings_workbench.tab == "library" {
                return crate::candidate_app::events::update(app, Message::RefreshCatalog);
            }
        }
        intent::Message::SavePreset(mode) => {
            if app.settings_workbench.tab != "picker" || mode != app.state.gallery.mode {
                return Task::none();
            }
            let task = match finish_pointer_edit(app) {
                Ok(task) => task,
                Err(task) => return task,
            };
            let result = app.save_gallery_preset();
            match result {
                Ok(_) => {
                    app.settings_workbench.preset_name_raw = None;
                    app.settings_workbench.message = "Mode profile saved.".into();
                    rotate(app);
                }
                Err(error) => app.settings_workbench.message = error,
            }
            return task;
        }
        intent::Message::PresetNameInput { mode, id, raw } => {
            if app.settings_workbench.tab != "picker"
                || mode != app.state.gallery.mode
                || app.state.mode_profiles.selected_id(mode) != Some(id)
            {
                return Task::none();
            }
            let mut proposed = app.state.clone();
            if proposed.mode_profiles.rename_selected(mode, id, &raw) {
                if let Err(error) = app.commit_local_state_change(proposed) {
                    app.settings_workbench.message = app
                        .save_failure
                        .clone()
                        .unwrap_or_else(|| error.to_string());
                }
            }
            app.settings_workbench.preset_name_raw = Some((mode, id, raw));
        }
        intent::Message::ApplyPreset(mode, id) => {
            if app.settings_workbench.tab != "picker" || mode != app.state.gallery.mode {
                return Task::none();
            }
            let Some(_) = app.state.mode_profiles.snapshot(mode, id) else {
                app.settings_workbench.message =
                    "This legacy profile has no available geometry snapshot.".into();
                return Task::none();
            };
            let (task, result) = app.apply_gallery_preset(mode, id);
            match result {
                Ok(()) => {
                    app.settings_workbench.preset_name_raw = None;
                    app.settings_workbench.message = "Current-mode layout restored.".into();
                    rotate(app);
                }
                Err(error) => app.settings_workbench.message = error,
            }
            return task;
        }
        intent::Message::DeletePreset(mode, id) => {
            if app.settings_workbench.tab != "picker" || mode != app.state.gallery.mode {
                return Task::none();
            }
            let task = match finish_pointer_edit(app) {
                Ok(task) => task,
                Err(task) => return task,
            };
            if app.delete_gallery_preset(mode, id).unwrap_or_else(|error| {
                app.settings_workbench.message = error;
                false
            }) {
                app.settings_workbench.preset_name_raw = None;
                app.settings_workbench.message =
                    "Mode profile deleted; current geometry is unchanged.".into();
                rotate(app);
            }
            return task;
        }
        intent::Message::SelectPresetForRename(mode, id) => {
            if app.settings_workbench.tab != "picker" || mode != app.state.gallery.mode {
                return Task::none();
            }
            let task = match finish_pointer_edit(app) {
                Ok(task) => task,
                Err(task) => return task,
            };
            let mut proposed = app.state.clone();
            if proposed.mode_profiles.select_for_rename(mode, id)
                && (proposed.mode_profiles == app.state.mode_profiles
                    || app
                        .commit_local_state_change(proposed)
                        .map(|_| true)
                        .unwrap_or_else(|error| {
                            app.settings_workbench.message = app
                                .save_failure
                                .clone()
                                .unwrap_or_else(|| error.to_string());
                            false
                        }))
            {
                app.settings_workbench.preset_name_raw = None;
                app.settings_workbench.message =
                    "Record selected for naming only; no layout was applied.".into();
                rotate(app);
            }
            return task;
        }

        intent::Message::Frame(now) => {
            let duration = Source { app }.motion_fast_ms();
            let standard = Source { app }.motion_standard_ms();
            let session = &mut app.settings_workbench;
            let dt = session.last_frame.replace(now).map_or(0.0, |last| {
                now.saturating_duration_since(last).as_secs_f32().min(0.1)
            });
            if let Some(tween) = &mut session.folio_entrance {
                tween.set_duration_ms(standard);
                tween.tick(dt);
            }
            if let Some(tween) = &mut session.tab_reveal {
                tween.set_duration_ms(duration);
                tween.tick(dt);
            }
            for tween in session.bar_reveals.values_mut() {
                tween.set_duration_ms(duration);
                tween.tick(dt);
            }
        }
        intent::Message::Noop => {}
        intent::Message::Stash => {
            if !(Source { app }).stash_available() {
                app.settings_workbench.message = "Hide unavailable in preview".into();
                return Task::none();
            }
            return app.set_visible(false, "settings-stash");
        }
        intent::Message::ToggleSettings | intent::Message::Backdrop => {
            if app.settings_workbench.draft.errors.is_empty() {
                let task = match commit(app) {
                    Ok(task) => task,
                    Err((task, _)) => return task,
                };
                app.panel_state.settings_open = false;
                return task;
            }
        }
        intent::Message::Settings(message) => match message {
            SettingsMsg::Input(path, raw) => return input(app, generation, &model, path, raw),
            SettingsMsg::ResolutionInput(path, width, raw) => {
                if !matches!(
                    find_control(&model, &path),
                    Some(Control::Resolution { .. })
                ) {
                    return Task::none();
                }
                app.settings_workbench.navigation.resolution_height = !width;
                let current = app
                    .settings_workbench
                    .draft
                    .raw_inputs
                    .get(&path)
                    .cloned()
                    .unwrap_or_else(|| Source { app }.text(&path));
                let (w, h) = current.split_once(['x', 'X', '×']).unwrap_or(("", ""));
                let next = if width {
                    format!("{raw}×{h}")
                } else {
                    format!("{w}×{raw}")
                };
                return input(app, generation, &model, path, next);
            }
            SettingsMsg::ResolutionLimit(path, limited) => {
                if !path.ends_with(".to")
                    || !matches!(
                        find_control(&model, &path),
                        Some(Control::Resolution { .. })
                    )
                {
                    return Task::none();
                }
                let value = if limited {
                    let from = format!("{}.from", path.trim_end_matches(".to"));
                    Source { app }.text(&from)
                } else {
                    String::new()
                };
                return input(app, generation, &model, path, value);
            }
            SettingsMsg::KeybindCapture(path, slot) => {
                if matches!(
                    find_control(&model, &path),
                    Some(Control::KeyBinding { .. })
                ) {
                    let task = match finish_pointer_edit(app) {
                        Ok(task) => task,
                        Err(task) => return task,
                    };
                    let map = Source { app }.bindings();
                    rotate(app);
                    let generation = app.settings_workbench.generation;
                    app.settings_workbench.capture =
                        composed::binding_capture::Capture::begin(generation, &path, slot, &map);
                    return task;
                }
            }
            SettingsMsg::KeybindCaptureCancel => {
                app.settings_workbench.capture = None;
                rotate(app);
            }
            SettingsMsg::KeybindCaptureDefault => {
                if let Some(capture) = &mut app.settings_workbench.capture {
                    capture.restore_default();
                }
            }
            SettingsMsg::KeybindCaptureUnbind => {
                if let Some(capture) = &mut app.settings_workbench.capture {
                    capture.unbind();
                }
            }
            SettingsMsg::KeybindCaptureClick(button) => {
                if let Some(capture) = &mut app.settings_workbench.capture {
                    capture.replace(composed::input::Trigger::Mouse(
                        composed::input::MouseSpec {
                            mods: app.settings_workbench.capture_modifiers,
                            button,
                        },
                    ));
                }
            }
            SettingsMsg::KeybindCaptureWheel(direction) => {
                if let Some(capture) = app
                    .settings_workbench
                    .capture
                    .as_mut()
                    .filter(|capture| capture.action.accepts_preset_wheel())
                {
                    capture.replace(composed::input::Trigger::Wheel(
                        composed::input::WheelSpec {
                            mods: app.settings_workbench.capture_modifiers,
                            direction,
                        },
                    ));
                }
            }
            SettingsMsg::ToggleSaveDetails => {
                app.settings_workbench.save_details_open =
                    !app.settings_workbench.save_details_open;
            }
            SettingsMsg::KeybindCaptureApply => {
                let map = Source { app }.bindings();
                let changes = app
                    .settings_workbench
                    .capture
                    .as_ref()
                    .and_then(|capture| capture.proposal(generation, &map));
                if changes.is_none()
                    && app.settings_workbench.capture.as_ref().is_some_and(|c| {
                        !map.reserved_conflicts_except(c.action, &c.triggers)
                            .is_empty()
                    })
                {
                    app.settings_workbench.message = "Reserved shortcut. Edit application shortcuts in Controls or open config.toml from Displays.".into();
                }
                if let Some(changes) = changes {
                    if let Err(error) = save_captured_bindings(app, changes) {
                        app.settings_workbench.message = error;
                    }
                } else if app
                    .settings_workbench
                    .capture
                    .as_ref()
                    .is_some_and(|capture| capture.edited)
                {
                    app.settings_workbench
                        .announce_save(super::bridge::SaveNotice::Rejected);
                }
            }
            SettingsMsg::BackgroundPicker(output) => {
                if !background_visible(&model, &output) {
                    return Task::none();
                }
                let task = match finish_pointer_edit(app) {
                    Ok(task) => task,
                    Err(task) => return task,
                };
                let (mode, _) = background_paths(&output);
                app.settings_workbench
                    .draft
                    .values
                    .insert(mode, Value::Text("color".into()));
                let id = format!("background-picker:{output}");
                let duration = Source { app }.motion_fast_ms();
                let session = &mut app.settings_workbench;
                let tween = session.bar_reveals.entry(id).or_insert_with(|| {
                    MotionProfile::new(duration, 250.0, 450.0).tween(0.0, MotionTier::Fast)
                });
                tween.retarget(if tween.target > 0.5 { 0.0 } else { 1.0 });
                session.last_frame = None;
                return task;
            }
            SettingsMsg::BackgroundMode(output, value) => {
                if !background_visible(&model, &output)
                    || !matches!(value.as_str(), "color" | "blur" | "inherit")
                    || (output.is_empty() && value == "inherit")
                {
                    return Task::none();
                }
                let task = match finish_pointer_edit(app) {
                    Ok(task) => task,
                    Err(task) => return task,
                };
                let previous =
                    composed::background::BackgroundControl::new(&Source { app }, &output);
                let (mode, color) = background_paths(&output);
                let session = &mut app.settings_workbench;
                if !output.is_empty()
                    && value != "inherit"
                    && !session.draft.values.contains_key(&color)
                {
                    session
                        .draft
                        .values
                        .insert(color, Value::Text(previous.color));
                }
                session
                    .draft
                    .values
                    .insert(mode, Value::Text(value.clone()));
                if value != "color" {
                    if let Some(tween) = session
                        .bar_reveals
                        .get_mut(&format!("background-picker:{output}"))
                    {
                        tween.retarget(0.0);
                        session.last_frame = None;
                    }
                }
                if let Err(error) = persist_background(app, &output) {
                    app.settings_workbench.message = error;
                }
                rotate(app);
                return task;
            }
            SettingsMsg::BackgroundColor(output, color) => {
                if !background_visible(&model, &output)
                    || !app
                        .settings_workbench
                        .bar_reveals
                        .get(&format!("background-picker:{output}"))
                        .is_some_and(|reveal| reveal.target > 0.5)
                    || crate::donor_folio::source_ui::color::hex_to_hsv(&color).is_none()
                {
                    return Task::none();
                }
                let (mode, path) = background_paths(&output);
                let session = &mut app.settings_workbench;
                session
                    .draft
                    .values
                    .insert(mode, Value::Text("color".into()));
                session.draft.values.insert(path, Value::Text(color));
            }
            SettingsMsg::BackgroundCommit(output) => {
                if background_visible(&model, &output) {
                    if let Err(error) = persist_background(app, &output) {
                        app.settings_workbench.message = error;
                    }
                }
            }
            SettingsMsg::ProcessPickerClose => {
                app.settings_workbench.process_picker = None;
                rotate(app);
            }
            SettingsMsg::ProcessPickerSearch(query) => {
                if let Some(picker) = &mut app.settings_workbench.process_picker {
                    picker.query = query;
                }
            }
            SettingsMsg::ProcessPickerManualInput(raw) => {
                if let Some(picker) = &mut app.settings_workbench.process_picker {
                    picker.manual = raw;
                    picker.manual_error = None;
                }
            }
            SettingsMsg::ProcessPickerManualAdd => {
                if let Some(name) = app
                    .settings_workbench
                    .process_picker
                    .as_ref()
                    .map(|picker| picker.manual.clone())
                {
                    if edit_process(app, &name, true) {
                        if let Some(picker) = &mut app.settings_workbench.process_picker {
                            picker.manual.clear();
                        }
                    }
                }
            }
            SettingsMsg::ProcessPickerScroll(value) => {
                if value.is_finite() {
                    if let Some(picker) = &mut app.settings_workbench.process_picker {
                        picker.scroll = value.clamp(0.0, 1.0);
                    }
                }
            }
            SettingsMsg::ProcessPickerAdd(name) => {
                if app
                    .settings_workbench
                    .process_picker
                    .as_ref()
                    .is_some_and(|picker| picker.running.contains(&name))
                {
                    edit_process(app, &name, true);
                }
            }
            SettingsMsg::ProcessPickerRemove(name) => {
                if app.settings_workbench.process_picker.is_some() {
                    edit_process(app, &name, false);
                }
            }
            SettingsMsg::Commit => match commit(app) {
                Ok(task) => {
                    app.settings_workbench.navigation.input_edit = None;
                    return task;
                }
                Err((task, error)) => {
                    app.settings_workbench.message = error;
                    return task;
                }
            },
            SettingsMsg::Toggle(path, value) => {
                let commit_task = match commit(app) {
                    Ok(task) => task,
                    Err((task, _)) => return task,
                };
                if let Some(value) = app.settings_workbench.draft.choose(
                    generation,
                    &model,
                    &path,
                    Value::Boolean(value),
                ) {
                    rotate(app);
                    return Task::batch([commit_task, apply_value(app, &path, value)]);
                }
                return commit_task;
            }
            SettingsMsg::Pick(path, text) => {
                let commit_task = match commit(app) {
                    Ok(task) => task,
                    Err((task, _)) => return task,
                };
                let previous = app.settings_workbench.draft.values.get(&path).cloned();
                if let Some(value) = app.settings_workbench.draft.choose(
                    generation,
                    &model,
                    &path,
                    Value::Text(text.clone()),
                ) {
                    if path == composed::keys::general::LANGUAGE {
                        let next = localization::Catalog::new(&text, localization::automatic_tag());
                        let result = next.and_then(|next| {
                            let previous_script = app.settings_locale.language.script;
                            localization::publish_locale_change(previous_script, &next, || {
                                persist_preference(app, &path, &value).map(|_| ())
                            })?;
                            app.settings_locale = next;
                            app.settings_workbench.results.clear();
                            Ok(())
                        });
                        if let Err(error) = result {
                            // Keep the failed language choice as a local edit;
                            // application Catalog/fonts still use the committed value.
                            app.settings_workbench.message = error;
                            return commit_task;
                        }
                        rotate(app);
                        return commit_task;
                    }
                    if app.settings_workbench.lists.text(&path).is_some() {
                        if let Err(error) = app.settings_workbench.lists.edit(&path, &text) {
                            let draft = &mut app.settings_workbench.draft;
                            match previous {
                                Some(value) => {
                                    draft.values.insert(path.clone(), value);
                                }
                                None => {
                                    draft.values.remove(&path);
                                }
                            }
                            draft.raw_inputs.insert(path.clone(), text);
                            draft.errors.insert(path, error);
                            return commit_task;
                        }
                    }
                    rotate(app);
                    return Task::batch([commit_task, apply_value(app, &path, value)]);
                }
                return commit_task;
            }
            SettingsMsg::SelectTab(tab) => {
                if app.settings_workbench.draft.errors.is_empty() {
                    let task = match commit(app) {
                        Ok(task) => task,
                        Err((task, _)) => return task,
                    };
                    select_tab(app, tab);
                    return task;
                }
            }
            SettingsMsg::SelectSection(index) => {
                if model.get(index).is_none() {
                    return Task::none();
                }
                if app.settings_workbench.draft.errors.is_empty() {
                    let task = match commit(app) {
                        Ok(task) => task,
                        Err((task, _)) => return task,
                    };
                    app.settings_workbench
                        .navigation
                        .select_section(index, &model);
                    rotate(app);
                    return task;
                }
            }
            SettingsMsg::FocusControl(row) => {
                if !model
                    .get(app.settings_workbench.navigation.section)
                    .and_then(|(_, rows)| rows.get(row))
                    .is_some_and(|row| row.control.is_focusable())
                {
                    return Task::none();
                }
                let task = match finish_pointer_edit(app) {
                    Ok(task) => task,
                    Err(task) => return task,
                };
                app.settings_workbench.navigation.row = row;
                app.settings_workbench.navigation.focus = composed::SettingsFocus::Controls;
                return task;
            }
            SettingsMsg::ToggleDetails(id, row) => {
                if !model
                    .get(app.settings_workbench.navigation.section)
                    .and_then(|(_, rows)| rows.get(row))
                    .is_some_and(|row| row_has_id(&row.control, &id, true))
                {
                    return Task::none();
                }
                let task = match finish_pointer_edit(app) {
                    Ok(task) => task,
                    Err(task) => return task,
                };
                let nav = &mut app.settings_workbench.navigation;
                if !nav.expanded.remove(&id) {
                    nav.expanded.insert(id);
                }
                nav.row = row;
                rotate(app);
                return task;
            }
            SettingsMsg::ToggleBar(id, row) => {
                if !model
                    .get(app.settings_workbench.navigation.section)
                    .and_then(|(_, rows)| rows.get(row))
                    .is_some_and(|row| row_has_id(&row.control, &id, false))
                {
                    return Task::none();
                }
                let task = match finish_pointer_edit(app) {
                    Ok(task) => task,
                    Err(task) => return task,
                };
                let duration = Source { app }.motion_fast_ms();
                let session = &mut app.settings_workbench;
                let open = !session.navigation.open_bars.remove(&id);
                if open {
                    session.navigation.open_bars.insert(id.clone());
                }
                session.navigation.row = row;
                let tween = session.bar_reveals.entry(id).or_insert_with(|| {
                    MotionProfile::new(duration, 250.0, 450.0).tween(0.0, MotionTier::Fast)
                });
                tween.retarget(if open { 1.0 } else { 0.0 });
                session.last_frame = None;
                rotate(app);
                return task;
            }
            SettingsMsg::SearchOpen => {
                let task = match finish_pointer_edit(app) {
                    Ok(task) => task,
                    Err(task) => return task,
                };
                app.settings_workbench.navigation.search_open = true;
                return Task::batch([
                    task,
                    iced::widget::operation::focus(composed::settings_search_input_id()),
                ]);
            }
            SettingsMsg::SearchClose => {
                app.settings_workbench.navigation.search_open = false;
                app.settings_search.clear();
                app.settings_workbench.results.clear();
            }
            SettingsMsg::SearchInput(query) => app.settings_search = query,
            SettingsMsg::OpenSearchResult(tab, section, row) => {
                if tab == "motion"
                    && super::bridge::build(app, &tab)
                        .get(section)
                        .and_then(|(_, rows)| rows.get(row))
                        .is_none()
                {
                    return Task::none();
                }
                if app.settings_workbench.draft.errors.is_empty() {
                    let task = match commit(app) {
                        Ok(task) => task,
                        Err((task, _)) => return task,
                    };
                    select_tab(app, tab);
                    let model = cards(app);
                    app.settings_workbench
                        .navigation
                        .select_search_result(section, row, &model);
                    return task;
                }
            }
            SettingsMsg::LeaveLayoutStudio => {
                let task = match finish_pointer_edit(app) {
                    Ok(task) => task,
                    Err(task) => return task,
                };
                app.settings_workbench.navigation.section = 0;
                rotate(app);
                return task;
            }
            SettingsMsg::Key(key) => return navigate(app, key, &model),
            SettingsMsg::Run(action) => {
                if !action_visible(&model, action) {
                    return Task::none();
                }
                if matches!(
                    action,
                    ActionId::ResumeRandomRotation | ActionId::StopPlaylist
                ) {
                    let message = if action == ActionId::StopPlaylist {
                        Message::StopPlaylist {
                            generation: app.automation_generation.load(Ordering::Acquire),
                        }
                    } else {
                        Message::ResumeRandomRotation
                    };
                    let next = crate::candidate_app::events::update_inner(app, message);
                    rotate(app);
                    return next;
                }
                if matches!(
                    action,
                    ActionId::CycleResolution | ActionId::ClearResolution
                ) {
                    let message = if action == ActionId::ClearResolution {
                        Message::ClearResolution
                    } else {
                        Message::CycleResolution
                    };
                    let next = crate::candidate_app::events::update_inner(app, message);
                    rotate(app);
                    return next;
                }
                let task = match commit(app) {
                    Ok(task) => task,
                    Err((task, _)) => return task,
                };
                if let ActionId::SetThemeTarget(provider, enabled) = action {
                    if let Err(error) = set_theme_target(app, provider, enabled) {
                        app.settings_workbench.message = error;
                    }
                    rotate(app);
                    return task;
                }
                if action.is_destructive() && app.settings_workbench.armed != Some(action) {
                    app.settings_workbench.armed = Some(action);
                    return task;
                }
                app.settings_workbench.armed = None;
                if let Some(root) = super::pending_projection::collection_action(action) {
                    if let Err(error) =
                        super::pending_projection::collection_editable(&app.state, root)
                    {
                        app.settings_workbench.message = error;
                        return task;
                    }
                }
                if app
                    .settings_workbench
                    .lists
                    .action(action, &app.settings_locale)
                {
                    if matches!(
                        action,
                        ActionId::AddResolutionPreset
                            | ActionId::RemoveResolutionPreset(_)
                            | ActionId::CreateResolutionPresetBand(_)
                    ) {
                        let mut proposed = app.state.clone();
                        proposed.frontend_preferences.resolution_presets =
                            app.settings_workbench.lists.resolutions.clone();
                        if let Err(error) = app.commit_local_state_change(proposed) {
                            app.settings_workbench.message = app
                                .save_failure
                                .clone()
                                .unwrap_or_else(|| error.to_string());
                        }
                    }
                    if let Some(root) = super::pending_projection::collection_action(action) {
                        if let Err(error) = persist_collection_root(app, root) {
                            app.settings_workbench.message = error;
                        }
                    }
                    retire_indexed_buffers(app);
                    rotate(app);
                } else {
                    match action {
                    ActionId::ResetMotionFast|ActionId::ResetMotionStandard|ActionId::ResetMotionSlow=>{
                        let path=match action{ActionId::ResetMotionFast=>composed::keys::motion::FAST_MS,ActionId::ResetMotionStandard=>composed::keys::motion::STANDARD_MS,_=>composed::keys::motion::SLOW_MS};
                        if let Some(value)=composed::schema::number_default(path){
                            let value = Value::Number(value);
                            app.settings_workbench.draft.values.insert(path.into(), value.clone());
                            if let Err(error) = persist_preference(app, path, &value) { app.settings_workbench.message = error; }
                        }
                    },
                    ActionId::ResetKeybinds => {
                        let mut proposed = app.state.clone();
                        let mut changes = Vec::new();
                        for descriptor in composed::KEY_BINDINGS.into_iter().filter(|entry| entry.action.local_available()) {
                            let value = Value::Text(String::new());
                            if let Err(error) = super::preference_projection::write(&mut proposed.frontend_preferences, descriptor.path, &value) {
                                app.settings_workbench.message = error;
                                return task;
                            }
                            changes.push((descriptor.path, value));
                        }
                        match app.commit_local_state_change(proposed) {
                            Ok(_) => {
                                app.settings_workbench.draft.values.extend(changes.into_iter().map(|(path, value)| (path.into(), value)));
                                rotate(app);
                            }
                            Err(error) => app.settings_workbench.message = app.save_failure.clone().unwrap_or_else(|| error.to_string()),
                        }
                    },
                    ActionId::SetNavKeys(keys) => {
                        let current = Source { app }.bindings();
                        let mut proposed = app.state.clone();
                        let mut changes = Vec::new();
                        for (action, second) in keys.second_keys() {
                            let descriptor = composed::KEY_BINDINGS.iter().find(|entry| entry.action == action).expect("source navigation descriptor");
                            let mut map = current.clone();
                            map.override_bindings(composed::KEY_BINDINGS.iter().filter(|entry| entry.action.local_available()).map(|entry| {
                                let binding = match super::preference_projection::read(&proposed.frontend_preferences, entry.path) {
                                    Some(Value::Text(value)) if !value.is_empty() => value,
                                    Some(Value::Text(_)) => entry.action.default_binding().into(),
                                    _ => composed::input::binding_config(current.fallback_triggers(entry.action)),
                                };
                                (entry.action, binding)
                            }));
                            let mut capture = composed::binding_capture::Capture::begin(generation, descriptor.path, 1, &map).expect("source secondary capture");
                            capture.replace(composed::input::Trigger::parse(second).expect("source secondary key"));
                            let Some(proposal)=capture.proposal(generation,&map) else {
                                app.settings_workbench.message="Navigation preset conflicts with a reserved Config shortcut; edit Config or choose another preset.".into();
                                return task;
                            };
                            for change in proposal {
                                if let Err(error) = super::preference_projection::write(&mut proposed.frontend_preferences, change.path, &Value::Text(change.binding.clone())) {
                                    app.settings_workbench.message = error;
                                    return task;
                                }
                                changes.push((change.path, change.binding));
                            }
                        }
                        match app.commit_local_state_change(proposed) {
                            Ok(_) => {
                                for (path, binding) in changes { app.settings_workbench.draft.values.insert(path.into(), Value::Text(binding)); }
                                rotate(app);
                            }
                            Err(error) => app.settings_workbench.message = app.save_failure.clone().unwrap_or_else(|| error.to_string()),
                        }
                    },
                    ActionId::ChooseRunningProcess=>{
                        let targets=processes(app);
                        app.settings_workbench.process_picker=Some(super::folio::workbench::ProcessPickerView{targets,..Default::default()});
                        app.settings_workbench.message="Running-process discovery and automatic pause execution are unavailable; enter process names manually.".into();
                        rotate(app);
                    },
                    ActionId::OpenDisplays=>return Task::batch([task,super::gateway::open(app,SettingsCategory::WallpapersDisplays)]),
                    ActionId::ShowExcluded=>return Task::batch([task,crate::candidate_app::events::update(app,Message::ToggleShowExcluded)]),
                    ActionId::OpenThemeDesigner=>return Task::batch([task,super::gateway::open(app,SettingsCategory::Theme)]),
                    ActionId::CycleBackgroundOpacity=>{
                        let background_task=crate::candidate_app::events::update(app,Message::CycleAppearanceOpacity);
                        app.settings_workbench.message=app.save_failure.clone().unwrap_or_else(||"Overlay opacity updated.".into());
                        return Task::batch([task,background_task]);
                    },
                    ActionId::RefreshBackdrop=>app.refresh_backdrop(),
                    _=>app.settings_workbench.message="This action is unavailable; configuration remains local and execution is pending.".into(),
                }
                }
                return task;
            }
        },
        _ => {
            app.settings_workbench.message =
                "This action is unavailable until its local adapter is connected.".into();
        }
    }
    Task::none()
}
fn navigate(
    app: &mut GpuCandidate,
    key: SettingsKey,
    model: &[(composed::Card, Vec<composed::Row>)],
) -> Task<Message> {
    use composed::navigation::Effect;
    let count = composed::visible_tabs(&Source { app }).len();
    let mut proposed = app.settings_workbench.navigation.clone();
    let effects = proposed.key(key, model, count);
    let mut tasks = Vec::new();
    // Validate before publishing any proposed focus/category change.
    for effect in &effects {
        if let Effect::CommitInput(path) = effect {
            match commit_path(app, path) {
                Ok(task) => tasks.push(task),
                Err(error) => {
                    app.settings_workbench.message = error;
                    return Task::none();
                }
            }
        }
    }
    app.settings_workbench.navigation = proposed;
    for effect in effects {
        match effect {
            Effect::CommitInput(_) => {}
            Effect::CloseSearch => {
                app.settings_search.clear();
                app.settings_workbench.results.clear();
            }
            Effect::CancelInput(path) => {
                if let Some(value) = app.settings_workbench.draft.cancel_input(&path) {
                    if let Value::Text(text) = &value {
                        if app.settings_workbench.lists.text(&path).is_some() {
                            let _ = app.settings_workbench.lists.edit(&path, text);
                        }
                    }
                    cancel_geometry_preview(app);
                }
            }
            Effect::CloseSettings => {
                app.panel_state.settings_open = false;
            }
            Effect::FocusInput(path) => tasks.push(iced::widget::operation::focus(
                composed::workbench_input_id(&path),
            )),
            Effect::FocusResolution { path, height } => tasks.push(iced::widget::operation::focus(
                composed::workbench_input_id(&format!(
                    "{path}.{}",
                    if height { "height" } else { "width" }
                )),
            )),
            Effect::Category(index) => {
                let tab = composed::visible_tabs(&Source { app })[index].0;
                select_tab(app, tab.into());
            }
            Effect::Activate { row, choice } => {
                tasks.push(activate_control(app, row, choice, model))
            }
            Effect::ActivateSearchResult => {
                if let Some(result) = app.settings_workbench.results.first().cloned() {
                    let generation = app.settings_workbench.generation;
                    tasks.push(dispatch(
                        app,
                        generation,
                        intent::Message::Settings(SettingsMsg::OpenSearchResult(
                            result.tab,
                            result.section,
                            result.row,
                        )),
                    ));
                }
            }
            _ => {}
        }
    }
    if matches!(key, SettingsKey::Search) {
        tasks.push(iced::widget::operation::focus(
            composed::settings_search_input_id(),
        ));
    }
    Task::batch(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn details_validator_accepts_source_app_theme_id_and_rejects_other_record_or_kind() {
        let app = composed::status::AppThemeStatus {
            id: "example".into(),
            ..Default::default()
        };
        let control = Control::AppTheme { app };
        assert!(row_has_id(&control, "app-theme:example", true));
        assert!(!row_has_id(&control, "app-theme:other", true));
        assert!(!row_has_id(&control, "example", true));
        assert!(!row_has_id(&control, "app-theme:example", false));
    }
}

fn activate_control(
    app: &mut GpuCandidate,
    row: usize,
    choice: Option<usize>,
    model: &[(composed::Card, Vec<composed::Row>)],
) -> Task<Message> {
    let Some(control) = model
        .get(app.settings_workbench.navigation.section)
        .and_then(|(_, rows)| rows.get(row))
        .map(|row| row.control.clone())
    else {
        return Task::none();
    };
    let generation = app.settings_workbench.generation;
    let mut intent = None;
    match control {
        Control::Toggle { path, value } => intent = Some(SettingsMsg::Toggle(path, !value)),
        Control::Number { key, .. } | Control::TextField { key, .. } => {
            app.settings_workbench.navigation.input_edit = Some(key.clone());
            return iced::widget::operation::focus(composed::workbench_input_id(&key));
        }
        Control::Resolution { key, .. } => {
            if key.ends_with(".to") && (Source { app }).text(&key).is_empty() {
                intent = Some(SettingsMsg::ResolutionLimit(key, true));
            } else {
                app.settings_workbench.navigation.input_edit = Some(key.clone());
                app.settings_workbench.navigation.resolution_height = false;
                return iced::widget::operation::focus(composed::workbench_input_id(&format!(
                    "{key}.width"
                )));
            }
        }
        Control::MotionWeights { weights } => {
            if let Some((_, key, _)) = weights.first() {
                let id = format!("setting-group:{key}");
                let duration = Source { app }.motion_fast_ms();
                let session = &mut app.settings_workbench;
                session.navigation.open_bars.insert(id.clone());
                session
                    .bar_reveals
                    .entry(id)
                    .or_insert_with(|| {
                        MotionProfile::new(duration, 250.0, 450.0).tween(0.0, MotionTier::Fast)
                    })
                    .retarget(1.0);
                session.last_frame = None;
                session.navigation.input_edit = Some(key.clone());
                return iced::widget::operation::focus(composed::workbench_input_id(key));
            }
        }
        Control::KeyBinding { path, .. } => match choice {
            Some(slot) => intent = Some(SettingsMsg::KeybindCapture(path, slot)),
            None => app.settings_workbench.navigation.choice = Some(0),
        },
        Control::Dropdown {
            path,
            options,
            current,
            ..
        } => match choice {
            Some(choice) => {
                if let Some((value, _)) = options.get(choice) {
                    intent = Some(SettingsMsg::Pick(path, value.clone()));
                }
            }
            None => {
                app.settings_workbench.navigation.choice = Some(
                    options
                        .iter()
                        .position(|(key, _)| key == &current)
                        .unwrap_or(0),
                )
            }
        },
        Control::Chips {
            path,
            options,
            current,
            disabled,
            ..
        } => match choice {
            Some(choice) => {
                if let Some((value, _)) = options
                    .get(choice)
                    .filter(|(key, _)| !disabled.contains(key))
                {
                    intent = Some(SettingsMsg::Pick(path, value.clone()));
                }
            }
            None => {
                app.settings_workbench.navigation.choice = options
                    .iter()
                    .position(|(key, _)| key == &current && !disabled.contains(key))
                    .or_else(|| options.iter().position(|(key, _)| !disabled.contains(key)))
            }
        },
        Control::ActionBtn { id, .. } | Control::ToggleAction { id, .. } => {
            intent = Some(SettingsMsg::Run(id))
        }
        Control::ActionChips { items } => match choice {
            Some(choice) => {
                if let Some((id, _)) = items.get(choice) {
                    intent = Some(SettingsMsg::Run(*id));
                }
            }
            None => app.settings_workbench.navigation.choice = Some(0),
        },
        Control::Details { id, .. } => intent = Some(SettingsMsg::ToggleDetails(id, row)),
        Control::StackBar { id, .. } => intent = Some(SettingsMsg::ToggleBar(id, row)),
        Control::AppTheme { app: entry } => {
            intent = Some(SettingsMsg::ToggleDetails(
                format!("app-theme:{}", entry.id),
                row,
            ))
        }
        Control::Presets { mode, items } => {
            use composed::ProfileChoice as P;
            let choices = composed::profile_choices(&items);
            let Some(choice) = choice else {
                let selected = items.iter().find(|entry| entry.selected);
                app.settings_workbench.navigation.choice = Some(
                    selected
                        .and_then(|entry| {
                            choices.iter().position(|choice| {
                                *choice
                                    == if entry.available {
                                        P::Apply(entry.id)
                                    } else {
                                        P::SelectForRename(entry.id)
                                    }
                            })
                        })
                        .unwrap_or(0),
                );
                return Task::none();
            };
            let message = match choices.get(choice) {
                Some(P::Save) => intent::Message::SavePreset(mode),
                Some(P::Apply(id))
                    if items.iter().any(|entry| entry.id == *id && entry.available) =>
                {
                    intent::Message::ApplyPreset(mode, *id)
                }
                Some(P::SelectForRename(id)) => intent::Message::SelectPresetForRename(mode, *id),
                Some(P::Delete(id)) => intent::Message::DeletePreset(mode, *id),
                _ => return Task::none(),
            };
            app.settings_workbench.navigation.choice = None;
            return dispatch(app, generation, message);
        }
        Control::Display(_) => {
            app.settings_workbench.message =
                "Per-display execution is unavailable; global Engine controls remain separate."
                    .into();
        }
        Control::Static | Control::Segment | Control::PlatformForm | Control::Code { .. } => {}
    }
    if let Some(intent) = intent {
        app.settings_workbench.navigation.choice = None;
        dispatch(app, generation, intent::Message::Settings(intent))
    } else {
        Task::none()
    }
}

pub(in crate::candidate_app) fn route_key(
    app: &mut GpuCandidate,
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
    event_window: window::Id,
    captured: bool,
) -> Option<Task<Message>> {
    use composed::input::{KeySpec, Mods, Trigger};
    use keyboard::{key::Named, Key};
    if !app.visible
        || !app.panel_state.settings_open
        || !app.settings_workbench.active
        || app.window != Some(event_window)
        || app.settings_workbench.owner_window != Some(event_window)
    {
        return None;
    }
    observe_modifiers(app, modifiers);
    let escape = matches!(key, Key::Named(Named::Escape));
    let search = crate::candidate_app::lifecycle::matches_candidate_key(key, modifiers, "ctrl+f")
        || (!captured
            && modifiers.is_empty()
            && matches!(key, Key::Character(text) if text.as_str()=="/"));
    if app.settings_workbench.capture.is_some() {
        if escape {
            app.settings_workbench.capture = None;
            rotate(app);
            return Some(Task::none());
        }
        if matches!(key, Key::Named(Named::Enter)) {
            let generation = app.settings_workbench.generation;
            return Some(dispatch(
                app,
                generation,
                intent::Message::Settings(SettingsMsg::KeybindCaptureApply),
            ));
        }
        let id = crate::candidate_app::lifecycle::input_key(key);
        if let Some(id) = id {
            app.settings_workbench
                .capture
                .as_mut()
                .unwrap()
                .replace(Trigger::Key(KeySpec {
                    id,
                    mods: Mods {
                        ctrl: modifiers.control(),
                        alt: modifiers.alt(),
                        shift: modifiers.shift(),
                        logo: modifiers.logo(),
                    },
                }));
        }
        return Some(Task::none());
    }
    if app.settings_workbench.process_picker.is_some() {
        if escape {
            app.settings_workbench.process_picker = None;
        }
        return Some(Task::none());
    }
    let generation = app.settings_workbench.generation;
    if !captured
        && !escape
        && !search
        && crate::candidate_app::lifecycle::matches_candidate_key(
            key,
            modifiers,
            &app.config.keybinds.close,
        )
    {
        return Some(dispatch(app, generation, intent::Message::ToggleSettings));
    }
    let nav = if escape {
        Some(SettingsKey::Cancel)
    } else if search {
        Some(SettingsKey::Search)
    } else {
        match key {
            Key::Named(Named::Tab) if modifiers.control() => Some(SettingsKey::CategoryNext {
                backwards: modifiers.shift(),
            }),
            Key::Named(Named::Tab) => Some(SettingsKey::FocusNext {
                backwards: modifiers.shift(),
            }),
            Key::Named(Named::ArrowLeft) => Some(SettingsKey::Previous),
            Key::Named(Named::ArrowRight) => Some(SettingsKey::Next),
            Key::Named(Named::ArrowUp) => Some(SettingsKey::Up),
            Key::Named(Named::ArrowDown) => Some(SettingsKey::Down),
            Key::Named(Named::Enter) => Some(SettingsKey::Activate),
            Key::Named(Named::Space) if modifiers.is_empty() => Some(SettingsKey::Activate),
            _ => None,
        }
    };
    // Text widgets own captured typing/Enter/arrows. Literal Escape and Ctrl+F
    // remain Settings commands; Tab explicitly moves the composed focus path.
    if captured && !escape && !search && !matches!(key, Key::Named(Named::Tab)) {
        return Some(Task::none());
    }
    if let Some(nav) = nav {
        return Some(dispatch(
            app,
            generation,
            intent::Message::Settings(SettingsMsg::Key(nav)),
        ));
    }
    if crate::candidate_app::lifecycle::matches_candidate_key(
        key,
        modifiers,
        &app.config.keybinds.close,
    ) {
        return Some(dispatch(app, generation, intent::Message::ToggleSettings));
    }
    Some(Task::none())
}

pub(in crate::candidate_app) fn observe_modifiers(
    app: &mut GpuCandidate,
    modifiers: keyboard::Modifiers,
) {
    app.settings_workbench.capture_modifiers = composed::input::Mods {
        ctrl: modifiers.control(),
        alt: modifiers.alt(),
        shift: modifiers.shift(),
        logo: modifiers.logo(),
    };
}

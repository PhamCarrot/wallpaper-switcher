//! Consequential P6 regression journeys, using real form and persistence owners.
use super::*;
use view::settings::pending_forms::Action as Form;
fn form(app: &mut GpuCandidate, action: Form) {
    let _ = update(
        app,
        Message::PendingForms {
            window: app.window,
            generation: app.settings_workbench.generation,
            action,
        },
    );
}
fn pending_app(category: SettingsCategory) -> GpuCandidate {
    let mut app = p3_composed_editor_app(category);
    let _ = view::settings::gateway::open(&mut app, category);
    view::settings::controller::reconcile(&mut app);
    app
}
#[test]
fn p6_source_effect_forms_validate_defaults_without_granting_execution() {
    use candidate_state::image_effects::{self, PendingEffect};
    assert_eq!(image_effects::definitions().len(), 34);
    for definition in image_effects::definitions() {
        let mut form = PendingEffect::new(&definition.id).unwrap();
        form.validate().unwrap();
        for parameter in &definition.params {
            let invalid = match parameter.kind.as_str() {
                "number" | "integer" => serde_json::json!(f64::MAX),
                "dropdown" => serde_json::json!("unknown-choice"),
                _ => serde_json::json!("#xyz"),
            };
            assert!(
                form.set(&parameter.id, invalid).is_err(),
                "{}:{}",
                definition.id,
                parameter.id
            );
        }
    }
}

#[test]
fn p6_actual_pending_rules_save_restart_validate_and_never_enter_active_schedules() {
    let mut app = pending_app(SettingsCategory::Automation);
    let active = app.state.schedules.clone();
    let timer = app.next_slideshow_unix;
    let config = app.config.slideshow.enabled;
    form(&mut app, Form::AddRule);
    form(&mut app, Form::RuleText(0, "name".into(), "Evening".into()));
    form(&mut app, Form::NewKind("Time window".into()));
    form(&mut app, Form::AddNode(0, Vec::new(), None));
    form(
        &mut app,
        Form::NodeText(0, vec![0], "a".into(), "sunrise+721".into()),
    );
    form(&mut app, Form::SaveRules);
    assert!(app
        .settings_workbench
        .pending_forms
        .message
        .contains("Invalid"));
    assert_eq!(
        app.settings_workbench
            .pending_forms
            .node_inputs
            .get(&(0, vec![0], "a".into()))
            .unwrap(),
        "sunrise+721"
    );
    form(
        &mut app,
        Form::NodeText(0, vec![0], "a".into(), "sunrise+30".into()),
    );
    form(&mut app, Form::SaveRules);
    let saved = CandidateState::load(&app.state_path);
    let restored = view::settings::pending_forms::Session::from_state(&saved);
    assert_eq!(restored.rules[0].name, "Evening");
    assert_eq!(restored.rules[0].condition.version, 2);
    assert_eq!(
        restored.rules[0].condition.root.children.as_ref().unwrap()[0]
            .value
            .as_deref(),
        Some("time:sunrise+30..sunset")
    );
    assert_eq!(app.state.schedules, active);
    assert_eq!(app.next_slideshow_unix, timer);
    assert_eq!(app.config.slideshow.enabled, config);
    let old_generation = app.settings_workbench.generation;
    let window = app.window;
    let _ = update(
        &mut app,
        Message::PendingForms {
            window,
            generation: old_generation.wrapping_sub(1),
            action: Form::AddRule,
        },
    );
    assert_eq!(app.settings_workbench.pending_forms.rules.len(), 1);
    // Capability presence changes presentation only; persisted pending data is
    // never used to construct an active rule or worker request.
    let before = app.state.clone();
    app.engine_properties_open = true;
    view::settings::controller::reconcile(&mut app);
    assert_eq!(app.state, before);
}
#[test]
fn p6_actual_browser_forms_validate_save_and_restore_without_search_dispatch() {
    let mut app = pending_app(SettingsCategory::Sources);
    let catalog = app.catalog_generation.load(Ordering::Acquire);
    form(&mut app, Form::BrowserSource("wallhaven".into()));
    form(&mut app, Form::BrowserText("minimum".into(), "bad".into()));
    form(&mut app, Form::SaveBrowser);
    assert!(app
        .settings_workbench
        .pending_forms
        .message
        .contains("resolution"));
    form(
        &mut app,
        Form::BrowserText("minimum".into(), "2560x1440".into()),
    );
    form(
        &mut app,
        Form::BrowserText("query".into(), "owned pending query".into()),
    );
    form(&mut app, Form::SaveBrowser);
    let saved = CandidateState::load(&app.state_path);
    let restored = view::settings::pending_forms::Session::from_state(&saved);
    assert_eq!(
        restored
            .browsers
            .iter()
            .find(|f| f.source == "wallhaven")
            .unwrap()
            .minimum,
        "2560x1440"
    );
    assert_eq!(app.catalog_generation.load(Ordering::Acquire), catalog);
    assert!(app.poster_worker.is_none());
    assert!(app
        .settings_workbench
        .pending_forms
        .message
        .starts_with("Saved pending"));
}
#[test]
fn p6_actual_wallpaper_profile_saves_draft_without_applying() {
    let mut app = super::theme_workflow::designer_app();
    let path = app.state_path.parent().unwrap().join("profile-source.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([1, 2, 3, 255]))
        .save(&path)
        .unwrap();
    app.wallpapers
        .push(scanner::local_image_item(&path).unwrap());
    app.selected = Some(0);
    view::settings::controller::reconcile(&mut app);
    app.settings_workbench.pending_forms =
        view::settings::pending_forms::Session::from_state(&app.state);
    let applied = app.state.applied_theme.clone();
    form(&mut app, Form::SaveWallpaperProfile);
    let saved = CandidateState::load(&app.state_path);
    assert_eq!(saved.applied_theme, applied);
    let restored = view::settings::pending_forms::Session::from_state(&saved);
    assert_eq!(restored.profiles.len(), 1);
}

#[test]
fn p6_pending_rule_metadata_and_newer_reload_are_preserved() {
    let mut app = pending_app(SettingsCategory::Automation);
    form(&mut app, Form::AddRule);
    let mut saved = CandidateState::load(&app.state_path);
    let record = saved
        .pending_frontend
        .settings
        .iter_mut()
        .find(|e| e.source_path == "schedule.rules")
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&record.value_json).unwrap();
    value[0]["condition"]["future-metadata"] = serde_json::json!({"fraction":1.125,"null":null});
    record.value_json = value.to_string();
    saved.save(&app.state_path).unwrap();
    app.reload_local_state().unwrap();
    form(&mut app, Form::RuleEnabled(0, false));
    let saved = CandidateState::load(&app.state_path);
    let value = view::settings::pending_projection::checked_root(&saved, "schedule.rules")
        .unwrap()
        .unwrap();
    assert_eq!(
        value[0]["condition"]["future-metadata"],
        serde_json::json!({"fraction":1.125,"null":null})
    );
    form(
        &mut app,
        Form::RuleText(0, "name".into(), "Retained local draft".into()),
    );
    let mut newer = saved.clone();
    let record = newer
        .pending_frontend
        .settings
        .iter_mut()
        .find(|e| e.source_path == "schedule.rules")
        .unwrap();
    let mut value: serde_json::Value = serde_json::from_str(&record.value_json).unwrap();
    value[0]["name"] = "Newer saved name".into();
    record.value_json = value.to_string();
    newer.save(&app.state_path).unwrap();
    let bytes = std::fs::read(&app.state_path).unwrap();
    assert!(app
        .reload_local_state()
        .unwrap_err()
        .contains("newer records"));
    assert_eq!(
        app.settings_workbench.pending_forms.rules[0].name,
        "Retained local draft"
    );
    assert_eq!(std::fs::read(&app.state_path).unwrap(), bytes);
    // A clean form can install newer saved records and rebase its widget data.
    app.settings_workbench.pending_forms =
        view::settings::pending_forms::Session::from_state(&app.state);
    app.reload_local_state().unwrap();
    assert_eq!(
        app.settings_workbench.pending_forms.rules[0].name,
        "Newer saved name"
    );
    form(&mut app, Form::RuleEnabled(0, true));
    let saved = CandidateState::load(&app.state_path);
    let restored = view::settings::pending_forms::Session::from_state(&saved);
    assert_eq!(restored.rules[0].name, "Newer saved name");
    // Disk may contain a prior proposed form while a newer raw condition is
    // still being typed. Exact serialized-rule equality must not erase it.
    app.settings_workbench.pending_forms.rules[0].name = "Published proposal".into();
    app.settings_workbench.pending_forms.rules_dirty = true;
    app.settings_workbench
        .pending_forms
        .node_inputs
        .insert((0, vec![0], "a".into()), "new raw text".into());
    let mut published = saved.clone();
    let record = published
        .pending_frontend
        .settings
        .iter_mut()
        .find(|r| r.source_path == "schedule.rules")
        .unwrap();
    record.value_json = serde_json::to_string(&app.settings_workbench.pending_forms.rules).unwrap();
    published.save(&app.state_path).unwrap();
    assert!(app
        .reload_local_state()
        .unwrap_err()
        .contains("raw condition"));
    assert_eq!(
        app.settings_workbench.pending_forms.node_inputs[&(0, vec![0], "a".into())],
        "new raw text"
    );
}

#[test]
fn retirement_legacy_stack_survives_unrelated_theme_save_reload_inertly() {
    use candidate_state::frontend::{DeferredCapability, PendingOnly, PendingSetting};
    let mut app = super::theme_workflow::designer_app();
    let raw = r#"[ {"effect":"brightness","params":{"factor":1.3},"future":{"source":"receipt"}}, {"effect":"future-kernel","params":{"unknown":42}} ]"#;
    app.state.pending_frontend.settings.push(PendingSetting {
        capability: DeferredCapability::ExtraImageEffects,
        source_path: "effects.stack".into(),
        value_json: raw.into(),
        status: PendingOnly::Pending,
    });
    app.state.save(&app.state_path).unwrap();
    let bytes = std::fs::read(&app.state_path).unwrap();
    let loaded = CandidateState::load(&app.state_path);
    assert_eq!(
        std::fs::read(&app.state_path).unwrap(),
        bytes,
        "loading is read-only"
    );
    assert_eq!(loaded.pending_frontend, app.state.pending_frontend);
    let before = app.state.pending_frontend.clone();
    super::theme_workflow::designer_name(&mut app, "Retained Theme save");
    super::theme_workflow::designer_save(&mut app, false);
    assert_eq!(app.state.pending_frontend, before);
    app.reload_local_state().unwrap();
    assert_eq!(app.state.pending_frontend, before);
    assert_eq!(app.state.pending_frontend.settings[0].value_json, raw);
    assert!(app.poster_worker.is_none());
    assert!(!app.apply_pending && app.active_operation.is_none());
    assert!(!app.status.as_deref().unwrap_or("").contains("export"));
}

#[test]
fn settings_cleanup_saved_device_and_unknown_pending_data_read_without_migration() {
    use candidate_state::frontend::{DeferredCapability, PendingOnly, PendingSetting};
    use view::settings::{
        composed::{self, draft::Value, SettingsSource},
        pending_projection,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Picker);
    pending_projection::write(
        &mut app.state,
        composed::keys::performance::GPU_DEVICE,
        &Value::Text("saved-adapter-42".into()),
    )
    .unwrap();
    pending_projection::write(
        &mut app.state,
        composed::keys::performance::BATTERY_SAVER,
        &Value::Boolean(true),
    )
    .unwrap();
    app.state.pending_frontend.settings.push(PendingSetting {
        capability: DeferredCapability::Optimization,
        source_path: "performance.futureAdapterPolicy".into(),
        value_json: r#"{"opaque":[17,"preserve"]}"#.into(),
        status: PendingOnly::Pending,
    });
    app.persist_state_outcome().unwrap();
    let bytes = std::fs::read(&app.state_path).unwrap();
    let expected = app.state.pending_frontend.clone();
    let saved = CandidateState::load(&app.state_path);
    assert_eq!(
        std::fs::read(&app.state_path).unwrap(),
        bytes,
        "load is read only"
    );
    assert_eq!(saved.pending_frontend, expected);
    assert_eq!(
        pending_projection::read(&saved, composed::keys::performance::GPU_DEVICE),
        Some(Value::Text("saved-adapter-42".into()))
    );
    app.state = saved;
    assert_eq!(
        (view::settings::bridge::Source { app: &app })
            .text(composed::keys::performance::GPU_DEVICE),
        "saved-adapter-42"
    );
    let _ = composed::build_tab(
        "performance",
        &(view::settings::bridge::Source { app: &app }),
        &[],
        &[],
        "",
        &[],
    );
    assert_eq!(
        std::fs::read(&app.state_path).unwrap(),
        bytes,
        "presentation cannot migrate saved data"
    );
    app.persist_state_outcome().unwrap();
    assert_eq!(
        CandidateState::load(&app.state_path).pending_frontend,
        expected
    );
}

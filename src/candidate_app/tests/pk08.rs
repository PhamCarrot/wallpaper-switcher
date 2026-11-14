use super::pk07::{renderer, Scene};
use super::*;
use std::path::PathBuf;
use view::settings::{
    composed::{self, keys::selector as tag, ActionId, Control, PICKER_CONTROLS_SECTION},
    folio::intent::{Message as Intent, SettingsMsg},
};

fn fixture() -> (
    GpuCandidate,
    Arc<std::sync::atomic::AtomicUsize>,
    [PathBuf; 2],
) {
    let (mut app, submissions) = d3_application_fixture(2);
    app.launch_policy = p1_preview_policy();
    app.panel_state.settings_category = SettingsCategory::Picker;
    let paths = ["still-a.png", "still-b.png"].map(|name| app.state_path.with_file_name(name));
    for (index, path) in paths.iter().enumerate() {
        let (width, height, color) = if index == 0 {
            (96, 48, [250_u8, 12, 0, 255])
        } else {
            (48, 96, [0, 35, 250, 255])
        };
        ::image::RgbaImage::from_pixel(width, height, ::image::Rgba(color))
            .save(path)
            .unwrap();
        let wallpaper = &mut app.wallpapers[index];
        wallpaper.file_target = path.clone();
        wallpaper.preview_path = Some(path.clone());
        wallpaper.source = WallpaperSource::LocalImage;
    }
    app.selected = Some(0);
    app.gallery_engaged = true;
    (app, submissions, paths)
}

fn section(app: &GpuCandidate, key: &'static str) -> usize {
    let title = app.settings_locale.text_static(key);
    view::settings::controller::cards(app)
        .iter()
        .position(|(card, _)| card.title == title)
        .unwrap_or_else(|| panic!("missing {title}"))
}

fn open(app: &mut GpuCandidate) {
    let _ = update(
        app,
        Message::DonorRail(donor_rail::RailIntent::ToggleSettings),
    );
    assert!(app.settings_workbench.active);
    app.settings_workbench.folio_entrance = None;
    app.settings_workbench.tab_reveal = None;
}

fn complete(app: &mut GpuCandidate) {
    let completion = app
        .settings_workbench
        .preview_completion()
        .expect("owned still request");
    let _ = update(app, completion);
    assert!(app.settings_workbench.preview_image().is_some());
}

#[test]
fn pk08_selected_still_generation_rejects_late_results_and_rebinds() {
    let (mut app, submissions, [_, b]) = fixture();
    open(&mut app);
    let late_a = app.settings_workbench.preview_completion().unwrap();
    app.selected = Some(1);
    let _ = update(&mut app, Message::SettingsSearchChanged(String::new()));
    assert!(app.settings_workbench.preview_image().is_none());
    assert!(
        app.settings_workbench.preview_completion().is_none(),
        "one job in flight"
    );
    let _ = update(&mut app, late_a.clone());
    assert!(app.settings_workbench.preview_image().is_none());
    complete(&mut app);
    let image_b = app.settings_workbench.preview_image().unwrap().id();
    let _ = update(&mut app, late_a);
    assert_eq!(
        app.settings_workbench.preview_image().unwrap().id(),
        image_b
    );

    let _ = update(&mut app, Message::ToggleSettingsPanel);
    assert!(app.settings_workbench.preview_image().is_none());
    open(&mut app);
    let pre_studio = app.settings_workbench.preview_completion().unwrap();
    let layout = section(&app, "settings-section-layout-presets");
    let generation = app.settings_workbench.generation;
    let window = app.window;
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation,
            intent: Intent::Settings(SettingsMsg::SelectSection(layout)),
        },
    );
    assert!(app.settings_workbench.layout_studio());
    let _ = update(&mut app, pre_studio.clone());
    assert!(app.settings_workbench.preview_image().is_none());
    assert!(app.settings_workbench.preview_completion().is_none());

    let generation = app.settings_workbench.generation;
    let _ = update(
        &mut app,
        Message::SettingsIntent {
            window,
            generation,
            intent: Intent::Settings(SettingsMsg::LeaveLayoutStudio),
        },
    );
    let post_studio = app.settings_workbench.preview_completion().unwrap();
    let _ = update(&mut app, pre_studio);
    assert!(app.settings_workbench.preview_image().is_none());
    let _ = update(&mut app, post_studio.clone());
    assert!(app.settings_workbench.preview_image().is_some());

    std::fs::remove_file(b).unwrap();
    let _ = update(&mut app, post_studio);
    assert!(app.settings_workbench.preview_image().is_none());
    assert!(app.settings_workbench.preview_completion().is_none());
    app.selected = None;
    let _ = update(&mut app, Message::SettingsSearchChanged(String::new()));
    assert!(app.settings_workbench.preview_completion().is_none());
    app.selected = Some(0);
    app.wallpapers[0].source = WallpaperSource::WallpaperEngine;
    app.wallpapers[0].wallpaper_type = "scene".into();
    app.wallpapers[0].preview_path = None;
    let _ = update(&mut app, Message::SettingsSearchChanged(String::new()));
    assert!(app.settings_workbench.preview_completion().is_none());
    app.wallpapers[0].preview_path = Some(app.wallpapers[0].file_target.clone());
    let _ = update(&mut app, Message::SettingsSearchChanged(String::new()));
    complete(&mut app);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn pk08_public_composition_uses_supported_controls_and_keeps_compatibility_values() {
    use candidate_state::preferences::BindingAction as B;
    let (mut app, _, _) = fixture();
    open(&mut app);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("picker".into()));
    let cards = view::settings::controller::cards(&app);
    let tag_index = section(&app, "settings-section-tag-panel");
    let windows_index = section(&app, "settings-section-windows");
    assert_eq!(
        section(&app, "settings-section-controls"),
        PICKER_CONTROLS_SECTION
    );
    assert!(!cards[0].1.iter().any(|row| matches!(&row.control,
        Control::Toggle { path, .. } if path == composed::keys::general::APPLY_ON_PICKER_MONITOR)));
    assert_eq!(cards[tag_index].1.len(), 4);
    assert!(cards[windows_index].1.iter().any(|row| matches!(
        row.control,
        Control::ActionBtn {
            id: ActionId::OpenWindowsDisplays,
            ..
        }
    )));
    let control_paths: Vec<_> = cards[PICKER_CONTROLS_SECTION]
        .1
        .iter()
        .filter_map(|row| match &row.control {
            Control::KeyBinding { path, .. } => Some(path.as_str()),
            _ => None,
        })
        .collect();
    for binding in composed::KEY_BINDINGS {
        assert_eq!(
            control_paths.contains(&binding.path),
            binding.action.local_available()
        );
    }
    assert!(!cards[PICKER_CONTROLS_SECTION]
        .1
        .iter()
        .any(|row| row.title.contains("Native Config") || row.desc.contains("remain inactive")));
    app.state
        .frontend_preferences
        .bindings
        .insert(B::SourceSteam, "ctrl+alt+q".into());
    app.state
        .frontend_preferences
        .bindings
        .insert(B::NavRight, "right, q".into());
    p3_settings_intent(
        &mut app,
        SettingsMsg::SelectSection(PICKER_CONTROLS_SECTION),
    );
    p3_settings_intent(&mut app, SettingsMsg::Run(ActionId::ResetKeybinds));
    assert_eq!(app.settings_workbench.armed, Some(ActionId::ResetKeybinds));
    p3_settings_intent(&mut app, SettingsMsg::Run(ActionId::ResetKeybinds));
    let saved = CandidateState::load(&app.state_path);
    assert_eq!(
        saved.frontend_preferences.bindings[&B::SourceSteam],
        "ctrl+alt+q"
    );
    assert!(saved.frontend_preferences.bindings[&B::NavRight].is_empty());
}

fn settings(message: &Message, accepts: impl Fn(&SettingsMsg) -> bool) -> bool {
    matches!(message, Message::SettingsIntent { intent: Intent::Settings(intent), .. } if accepts(intent))
}

fn hit_section(app: &mut GpuCandidate, renderer: &iced::Renderer, key: &'static str) {
    let index = section(app, key);
    let label = app.settings_locale.text_static(key);
    let message = Scene::new(app, renderer).hit_label(renderer, label, |m| {
        settings(
            m,
            |i| matches!(i, SettingsMsg::SelectSection(n) if *n == index),
        )
    });
    let _ = update(app, message);
    assert_eq!(app.settings_workbench.navigation.section, index);
}

fn press(key: keyboard::Key, modifiers: keyboard::Modifiers, text: Option<&str>) -> iced::Event {
    iced::Event::Keyboard(keyboard::Event::KeyPressed {
        modified_key: key.clone(),
        key,
        physical_key: keyboard::key::Physical::Unidentified(
            keyboard::key::NativeCode::Unidentified,
        ),
        location: keyboard::Location::Standard,
        modifiers,
        text: text.map(Into::into),
        repeat: false,
    })
}

fn input(
    app: &mut GpuCandidate,
    renderer: &iced::Renderer,
    id: iced::widget::Id,
    text: &str,
    accepts: impl Fn(&Message) -> bool,
) {
    let message = {
        let mut scene = Scene::new(app, renderer);
        let point = scene
            .probe(renderer)
            .inputs
            .into_iter()
            .find(|(input_id, _)| input_id.as_ref() == Some(&id))
            .unwrap()
            .1
            .center();
        let _ = scene.click(renderer, point, iced::mouse::Button::Left);
        let _ = scene.event(
            renderer,
            iced::mouse::Cursor::Unavailable,
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(keyboard::Modifiers::CTRL)),
        );
        let _ = scene.event(
            renderer,
            iced::mouse::Cursor::Unavailable,
            press(
                keyboard::Key::Character("a".into()),
                keyboard::Modifiers::CTRL,
                None,
            ),
        );
        let _ = scene.event(
            renderer,
            iced::mouse::Cursor::Unavailable,
            iced::Event::Keyboard(keyboard::Event::ModifiersChanged(
                keyboard::Modifiers::empty(),
            )),
        );
        let mut last = None;
        for ch in text.chars() {
            let ch = ch.to_string();
            last = scene
                .event(
                    renderer,
                    iced::mouse::Cursor::Unavailable,
                    press(
                        keyboard::Key::Character(ch.as_str().into()),
                        keyboard::Modifiers::empty(),
                        Some(&ch),
                    ),
                )
                .into_iter()
                .find(&accepts);
        }
        last.unwrap_or_else(|| panic!("missing final input event for {text:?}"))
    };
    let _ = update(app, message);
}

fn reading_pane(scene: &mut Scene<'_>, renderer: &iced::Renderer) -> (iced::Rectangle, f32) {
    let probe = scene.probe(renderer);
    let (_, pane, content, _) = probe
        .scrolls
        .iter()
        .filter(|(_, pane, content, _)| pane.height > 120.0 && content.height > pane.height)
        .max_by(|(_, a, _, _), (_, b, _, _)| a.x.total_cmp(&b.x))
        .expect("mounted reading scroller");
    (*pane, content.height)
}

fn reading_end(scene: &mut Scene<'_>, renderer: &iced::Renderer) {
    let (pane, height) = reading_pane(scene, renderer);
    scene.wheel(renderer, pane.center(), 0.0, -height);
}

fn hit_nav_right(scene: &mut Scene<'_>, renderer: &iced::Renderer, slot: usize) -> Message {
    let label = if slot == 0 { "Right" } else { "D" };
    let point = |scene: &mut Scene<'_>| {
        let probe = scene.probe(renderer);
        let title = probe
            .texts
            .iter()
            .find(|(value, _)| value == "Navigate right")
            .expect("mounted Navigation row")
            .1
            .center();
        probe
            .texts
            .iter()
            .filter(|(value, bounds)| value == label && bounds.x > title.x)
            .map(|(_, bounds)| bounds.center())
            .min_by(|a, b| (a.y - title.y).abs().total_cmp(&(b.y - title.y).abs()))
            .expect("mounted binding slot")
    };
    let (pane, _) = reading_pane(scene, renderer);
    let target = point(scene);
    let delta = pane.center().y - target.y;
    scene.wheel(renderer, pane.center(), 0.0, delta);
    let target = point(scene);
    assert!(
        pane.contains(target),
        "slot {slot} scrolled to {target:?} within {pane:?}"
    );
    scene
        .click(renderer, target, iced::mouse::Button::Left)
        .into_iter()
        .find(|m| {
            settings(m, |i| {
                matches!(i, SettingsMsg::KeybindCapture(path, index)
            if path == composed::keys::keybind::NAV_RIGHT && *index == slot)
            })
        })
        .unwrap_or_else(|| panic!("mounted Navigation right slot {slot} at {target:?}"))
}

#[test]
#[ignore = "PK-08 production Settings frames and widget dispatch; explicit headless invocation"]
fn pk08_production_settings_compositions_and_actions() {
    let out = PathBuf::from(std::env::var_os("P8_LAYOUT_OUTPUT").expect("owned frame directory"));
    std::fs::create_dir_all(&out).unwrap();
    let mut renderer = renderer();
    let mut receipts = Vec::new();
    for (width, height, scale) in [(1920, 1080, 1.0), (640, 600, 0.5), (760, 650, 1.0)] {
        let (mut app, submissions, _) = fixture();
        app.viewport = [width as f32, height as f32];
        app.state.frontend_preferences.interface.ui_scale = Some(scale);
        app.monitor_ids = vec!["STABLE-A".into(), "STABLE-B".into()];
        app.active_monitor_id = Some("STABLE-B".into());
        app.config.keybinds.close = "q".into();
        app.config.keybinds.switch_monitor = "bad+shortcut".into();
        open(&mut app);
        p3_settings_intent(&mut app, SettingsMsg::SelectTab("picker".into()));
        app.settings_workbench.tab_reveal = None;
        complete(&mut app);
        let save = |scene: &mut Scene<'_>,
                    renderer: &mut iced::Renderer,
                    app: &GpuCandidate,
                    name: &str| {
            scene.save(
                renderer,
                app,
                &out.join(format!("{width}x{height}-{scale}-{name}.png")),
            );
        };
        let mut scene = Scene::new(&app, &renderer);
        let visible = scene.probe(&renderer);
        assert!(!visible
            .texts
            .iter()
            .any(|(value, _)| value.contains("Apply only on the picker monitor")));
        save(&mut scene, &mut renderer, &app, "general-still-a");
        drop(scene);

        let search = app.settings_locale.text_static("settings-nav-search");
        let message = Scene::new(&app, &renderer).hit_label(&renderer, search, |m| {
            settings(m, |i| matches!(i, SettingsMsg::SearchOpen))
        });
        let _ = update(&mut app, message);
        input(
            &mut app,
            &renderer,
            composed::settings_search_input_id(),
            "Clickable tag rows",
            |m| settings(m, |i| matches!(i, SettingsMsg::SearchInput(_))),
        );
        assert_eq!(app.settings_search, "Clickable tag rows");
        let tag_index = section(&app, "settings-section-tag-panel");
        let mut scene = Scene::new(&app, &renderer);
        let hit = scene.hit_label(&renderer, "Clickable tag rows", |m|
            settings(m, |i| matches!(i, SettingsMsg::OpenSearchResult(tab, index, _) if tab == "picker" && *index == tag_index)));
        save(&mut scene, &mut renderer, &app, "tag-search-result");
        drop(scene);
        let _ = update(&mut app, hit);
        assert_eq!(app.settings_workbench.navigation.section, tag_index);
        assert!(!app.settings_workbench.layout_studio());

        let hit = Scene::new(&app, &renderer).hit_label(&renderer, "3 rows", |m|
            settings(m, |i| matches!(i, SettingsMsg::Pick(path, choice) if path == tag::TAG_CLOUD_ROWS && choice == "3")));
        let _ = update(&mut app, hit);
        for (key, raw) in [
            (tag::TAG_CLOUD_WIDTH, "515"),
            (tag::TAG_CLOUD_OFFSET_X, "18"),
            (tag::TAG_CLOUD_OFFSET_Y, "-22"),
        ] {
            input(
                &mut app,
                &renderer,
                composed::workbench_input_id(key),
                raw,
                |m| {
                    settings(
                        m,
                        |i| matches!(i, SettingsMsg::Input(path, value) if path == key && value == raw),
                    )
                },
            );
        }
        p3_settings_intent(&mut app, SettingsMsg::Commit);
        let state = CandidateState::load(&app.state_path);
        let values = &state.frontend_preferences.tag_panel;
        assert_eq!(
            (
                values.width,
                values.rows.as_deref(),
                values.offset_x,
                values.offset_y
            ),
            (Some(515.0), Some("3"), Some(18.0), Some(-22.0))
        );
        assert_eq!(app.search_panel_row_limit(), 3);
        assert_eq!(
            app.search_panel_width(),
            (515.0 * scale as f32).min(width as f32 - 40.0)
        );
        save(
            &mut Scene::new(&app, &renderer),
            &mut renderer,
            &app,
            "tag-panel",
        );
        receipts.push(format!(
            "{width}x{height} {scale} Tag panel search/hit/reload {values:?}"
        ));

        app.selected = Some(1);
        let _ = update(&mut app, Message::SettingsSearchChanged(String::new()));
        assert!(app.settings_workbench.preview_image().is_none());
        complete(&mut app);
        hit_section(&mut app, &renderer, "settings-section-general");
        save(
            &mut Scene::new(&app, &renderer),
            &mut renderer,
            &app,
            "general-still-b",
        );
        for mode in [
            GalleryMode::Slices,
            GalleryMode::Geometric,
            GalleryMode::Wall,
            GalleryMode::Depth,
        ] {
            let _ = update(&mut app, Message::SetGalleryMode(mode));
            if let Some(completion) = app.settings_workbench.preview_completion() {
                let _ = update(&mut app, completion);
            }
            let generation = app.poster_generation.load(Ordering::Acquire);
            let catalog_generation = app.catalog_generation.load(Ordering::Acquire);
            let mut posters = Vec::new();
            for (index, item) in app.wallpapers.iter().enumerate() {
                let source = poster_source_for_item(item);
                let layer = index as u32;
                posters.push(gpu_gallery::decode_poster(&source, layer).unwrap());
                app.poster_slots.insert(
                    index,
                    PosterSlot {
                        layer,
                        is_video: false,
                        generation,
                        catalog_generation,
                        item_id: item.id.clone(),
                        source,
                    },
                );
            }
            app.posters = Arc::new(posters);
            app.rebuild_depth_frame();
            hit_section(&mut app, &renderer, "settings-section-layout-presets");
            assert!(app.settings_workbench.layout_studio());
            assert!(app.settings_workbench.preview_image().is_none());
            let mut scene = Scene::new(&app, &renderer);
            let message = scene.hit_label(
                &renderer,
                app.settings_locale.text_static("settings-studio-back"),
                |m| settings(m, |i| matches!(i, SettingsMsg::LeaveLayoutStudio)),
            );
            save(
                &mut scene,
                &mut renderer,
                &app,
                &format!("studio-{}", mode.label()),
            );
            drop(scene);
            let _ = update(&mut app, message);
            assert!(!app.settings_workbench.layout_studio());
            assert!(
                app.selected.is_some(),
                "studio {mode:?} retains the owned selection"
            );
            complete(&mut app);
        }
        hit_section(&mut app, &renderer, "settings-section-windows");
        let mut scene = Scene::new(&app, &renderer);
        let message = scene.hit_label(
            &renderer,
            app.settings_locale.text_static("r5-windows-controls"),
            |m| {
                settings(m, |i| {
                    matches!(i, SettingsMsg::Run(ActionId::OpenWindowsDisplays))
                })
            },
        );
        save(&mut scene, &mut renderer, &app, "windows-entry");
        drop(scene);
        let _ = update(&mut app, message);
        assert_eq!(
            app.settings_workbench.windows_panel,
            Some(SettingsCategory::WallpapersDisplays)
        );
        let mut scene = Scene::new(&app, &renderer);
        let lock = scene.hit_label(&renderer, "Lock STABLE-A", |m|
            matches!(m, Message::SettingsLegacy { message, .. } if matches!(&**message, Message::ToggleMonitorLock(id) if id == "STABLE-A")));
        save(&mut scene, &mut renderer, &app, "windows-lock");
        drop(scene);
        let _ = update(&mut app, lock);
        assert!(CandidateState::load(&app.state_path)
            .locked_monitors
            .contains("STABLE-A"));
        assert_eq!(app.active_monitor_id.as_deref(), Some("STABLE-B"));
        let mut scene = Scene::new(&app, &renderer);
        assert!(scene
            .probe(&renderer)
            .texts
            .iter()
            .any(|(label, _)| label.contains("Invalid Config: monitor")));
        let _config = scene.hit_label(&renderer, app.settings_locale.text_static("settings-windows-open-config"), |m|
            matches!(m, Message::SettingsLegacy { message, .. } if matches!(&**message, Message::Wake(Wake::Tray(TrayAction::OpenConfig)))));
        save(&mut scene, &mut renderer, &app, "windows-config-hit");
        let back = scene.hit_label(
            &renderer,
            app.settings_locale.text_static("r5-back-settings"),
            |m| matches!(m, Message::SettingsWindowsClose { .. }),
        );
        drop(scene);
        let _ = update(&mut app, back);
        assert!(app.settings_workbench.windows_panel.is_none());
        complete(&mut app);
        hit_section(&mut app, &renderer, "settings-section-controls");
        let mut scene = Scene::new(&app, &renderer);
        assert!(!scene
            .probe(&renderer)
            .texts
            .iter()
            .any(|(label, _)| label.contains("Invalid Config: monitor")));
        save(&mut scene, &mut renderer, &app, "controls-top");
        let capture = hit_nav_right(&mut scene, &renderer, 1);
        save(&mut scene, &mut renderer, &app, "controls-secondary-hit");
        drop(scene);
        let _ = update(&mut app, capture);
        let window = app.window.unwrap();
        let _ = update(
            &mut app,
            Message::SettingsCapturedKey {
                window,
                key: keyboard::Key::Character("q".into()),
                modifiers: keyboard::Modifiers::empty(),
            },
        );
        let mut scene = Scene::new(&app, &renderer);
        let apply = scene.hit_label(&renderer, "OK", |m| {
            settings(m, |i| matches!(i, SettingsMsg::KeybindCaptureApply))
        });
        save(&mut scene, &mut renderer, &app, "capture-reserved");
        drop(scene);
        let before = std::fs::read(&app.state_path).unwrap();
        let _ = update(&mut app, apply.clone());
        assert!(app.settings_workbench.capture.is_some());
        assert!(app.settings_workbench.message.contains("Reserved"));
        let cancel = Scene::new(&app, &renderer).hit_label(&renderer, "Cancel", |m| {
            settings(m, |i| matches!(i, SettingsMsg::KeybindCaptureCancel))
        });
        let _ = update(&mut app, cancel);
        let _ = update(&mut app, apply);
        assert!(app.settings_workbench.capture.is_none());
        assert_eq!(std::fs::read(&app.state_path).unwrap(), before);
        let mut scene = Scene::new(&app, &renderer);
        let primary = hit_nav_right(&mut scene, &renderer, 0);
        drop(scene);
        let _ = update(&mut app, primary);
        let _ = update(
            &mut app,
            Message::SettingsCapturedKey {
                window,
                key: keyboard::Key::Character("z".into()),
                modifiers: keyboard::Modifiers::empty(),
            },
        );
        let default = Scene::new(&app, &renderer).hit_label(&renderer, "Default", |m| {
            settings(m, |i| matches!(i, SettingsMsg::KeybindCaptureDefault))
        });
        let _ = update(&mut app, default);
        assert!(app
            .settings_workbench
            .capture
            .as_ref()
            .unwrap()
            .triggers
            .iter()
            .any(
                |trigger| composed::input::binding_label(std::slice::from_ref(trigger)) == "Right"
            ));
        let cancel = Scene::new(&app, &renderer).hit_label(&renderer, "Cancel", |m| {
            settings(m, |i| matches!(i, SettingsMsg::KeybindCaptureCancel))
        });
        let _ = update(&mut app, cancel);
        let mut scene = Scene::new(&app, &renderer);
        reading_end(&mut scene, &renderer);
        let reset = scene.hit_label(&renderer, "Reset controls", |m| {
            settings(m, |i| {
                matches!(i, SettingsMsg::Run(ActionId::ResetKeybinds))
            })
        });
        save(&mut scene, &mut renderer, &app, "controls-reset-hit");
        drop(scene);
        let _ = update(&mut app, reset);
        assert_eq!(app.settings_workbench.armed, Some(ActionId::ResetKeybinds));
        let mut args = fluent::FluentArgs::new();
        args.set("label", "Reset controls");
        let confirm = app
            .settings_locale
            .format("settings-control-confirm", Some(&args))
            .unwrap();
        let mut scene = Scene::new(&app, &renderer);
        reading_end(&mut scene, &renderer);
        let reset = scene.hit_label(&renderer, &confirm, |m| {
            settings(m, |i| {
                matches!(i, SettingsMsg::Run(ActionId::ResetKeybinds))
            })
        });
        drop(scene);
        let _ = update(&mut app, reset);
        assert!(CandidateState::load(&app.state_path)
            .frontend_preferences
            .bindings[&candidate_state::preferences::BindingAction::NavRight]
            .is_empty());
        app.selected = None;
        let _ = update(&mut app, Message::SettingsSearchChanged(String::new()));
        assert!(app.settings_workbench.preview_image().is_none());
        hit_section(&mut app, &renderer, "settings-section-general");
        save(
            &mut Scene::new(&app, &renderer),
            &mut renderer,
            &app,
            "general-no-selection",
        );
        assert!(!app.apply_pending);
        assert_eq!(submissions.load(Ordering::Acquire), 0);
        receipts.push(format!("{width}x{height} {scale} studios=4 Windows lock=STABLE-A target=STABLE-B Config intent captured Controls primary/secondary/reserved/Cancel/stale/Default/Reset provider submissions=0"));
    }
    std::fs::write(out.join("ACTION-READBACKS.txt"), receipts.join("\n")).unwrap();
}

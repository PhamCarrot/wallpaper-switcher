use super::*;
use std::{
    sync::mpsc,
    time::{Duration, Instant},
};

#[test]
fn w4_hidden_real_watch_publication_preserves_tags_selection_and_stops_visual_demand() {
    let (mut app, _) = d3_application_fixture(0);
    let root = app.state_path.parent().unwrap().join("owned-media");
    std::fs::create_dir(&root).unwrap();
    let image = root.join("owned-keep.png");
    image::RgbaImage::new(3, 2).save(&image).unwrap();
    app.config.catalog.local_folders = vec![root.clone()];
    app.config.behavior.include_default_projects = false;
    let (tx, rx) = mpsc::channel();
    app.catalog_watch = Some(
        catalog_watch::CatalogWatch::spawn(
            vec![root.clone()],
            false,
            Default::default(),
            app.catalog_generation.clone(),
            move |wake| tx.send(wake).is_ok(),
        )
        .unwrap(),
    );
    fn converge(
        app: &mut GpuCandidate,
        rx: &mpsc::Receiver<Wake>,
        path: &std::path::Path,
        exists: bool,
    ) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            assert!(Instant::now() < deadline, "hidden catalog did not converge");
            let wake = rx.recv_timeout(Duration::from_secs(20)).unwrap();
            let loaded = matches!(wake, Wake::CatalogLoaded { .. });
            let _ = update(app, Message::Wake(wake));
            if loaded && app.wallpapers.iter().any(|i| i.file_target == path) == exists {
                return;
            }
        }
    }
    let image = image.canonicalize().unwrap();
    converge(&mut app, &rx, &image, true);
    let index = app
        .wallpapers
        .iter()
        .position(|i| i.file_target == image)
        .unwrap();
    app.selected = Some(index);
    let selected = app.wallpapers[index].id.clone();
    app.state
        .local_library
        .edit(&selected, "local-kept", true)
        .unwrap();
    app.project_local_tags();
    let _ = app.set_visible_owned(false, "w4-source-hidden-test");
    assert!(!app.visible);
    let detail = app.detail_progress;
    let generation = app.detail_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::Wake(Wake::DetailFrame {
            generation,
            value: 0.75,
            close_when_done: false,
            spring: None,
        }),
    );
    assert_eq!(app.detail_progress, detail);
    let motion = app.motion;
    let generation = app.motion_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::Wake(Wake::MotionFrame {
            generation,
            value: 0.9,
            velocity: 1.0,
            state: GalleryMotionState::source(0.0, 0.0, 250.0, 1.0),
            selection: Default::default(),
            presentation: None,
            filter_wave: None,
            ordinary: None,
            hex: None,
            wall: None,
            scene: None,
        }),
    );
    assert_eq!(app.motion, motion);
    let added = root.join("owned-added.png");
    image::RgbaImage::new(2, 3).save(&added).unwrap();
    let added = added.canonicalize().unwrap();
    converge(&mut app, &rx, &added, true);
    assert_eq!(
        app.selected
            .and_then(|i| app.wallpapers.get(i))
            .map(|i| &i.id),
        Some(&selected)
    );
    assert!(app
        .catalog
        .iter()
        .find(|i| i.id == selected)
        .unwrap()
        .tags
        .contains("local-kept"));
    app.schedule_posters();
    assert!(!app.poster_loading);
    assert!(app.poster_worker.is_none());
    assert!(app.poster_request_key.is_none());
    let generation = app.catalog_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::Wake(Wake::CatalogLoaded {
            generation: generation.wrapping_sub(1),
            wallpapers: Vec::new(),
            entries: Vec::new(),
        }),
    );
    assert!(app.wallpapers.iter().any(|i| i.id == selected));
    std::fs::remove_file(&added).unwrap();
    converge(&mut app, &rx, &added, false);
    app.catalog_watch = None;
}

#[test]
fn w4_reloaded_watch_preferences_reconfigure_only_after_successful_reconciliation() {
    let (mut app, _) = d3_application_fixture(0);
    let (tx, _rx) = mpsc::channel();
    app.catalog_watch = Some(
        catalog_watch::CatalogWatch::spawn(
            Vec::new(),
            false,
            Default::default(),
            app.catalog_generation.clone(),
            move |w| tx.send(w).is_ok(),
        )
        .unwrap(),
    );
    let mut saved = app.state.clone();
    saved.frontend_preferences.library.automatic_watch = Some(false);
    saved.frontend_preferences.library.polling_seconds = Some(19);
    candidate_state::persist_candidate_state(&mut saved, &app.state_path).unwrap();
    let prior = app.catalog_generation.load(Ordering::Acquire);
    app.reload_local_state().unwrap();
    assert!(!app.state.frontend_preferences.library.watching());
    assert!(app.catalog_generation.load(Ordering::Acquire) > prior);
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if app.catalog_watch.as_ref().unwrap().status().0 == "Automatic watching off" {
            break;
        }
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    let before = app.catalog_generation.load(Ordering::Acquire);
    std::fs::write(&app.state_path, "version = 999\n").unwrap();
    assert!(app.reload_local_state().is_err());
    assert_eq!(before, app.catalog_generation.load(Ordering::Acquire));
    assert!(!app.state.frontend_preferences.library.watching());
}

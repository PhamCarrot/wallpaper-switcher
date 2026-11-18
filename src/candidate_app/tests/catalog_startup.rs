use super::*;
use crate::candidate_app::{catalog_snapshot, catalog_watch::CatalogWatch};
use std::{sync::mpsc, time::Duration};

fn fixture() -> GpuCandidate {
    let (mut app, _) = d3_application_fixture(0);
    let media = app.state_path.with_file_name("media");
    std::fs::create_dir(&media).unwrap();
    for i in 0..3 {
        image::RgbaImage::new(3, 2)
            .save(media.join(format!("owned-{i}.png")))
            .unwrap();
    }
    app.config.catalog.local_folders = vec![media];
    app.config.behavior.include_default_projects = false;
    app.wallpapers = scanner::scan_local_images(&app.config.catalog.local_folders);
    app.catalog = app
        .wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    app.visible = false;
    app.deterministic_animation = true;
    app.gallery_mode = GalleryMode::Slices;
    app.state.gallery.mode = GalleryKind::Slices;
    // This fixture has no global runtime channel. Supply its own healthy worker,
    // as production bootstrap does, before exercising cached catalog installation.
    app.poster_worker =
        Some(poster_worker::PosterWorker::spawn(app.poster_generation.clone(), |_| true).unwrap());
    app
}
fn snapshot(app: &GpuCandidate) -> catalog_snapshot::Snapshot {
    catalog_snapshot::Snapshot::capture(
        &app.config.catalog.local_folders,
        false,
        &app.wallpapers,
        &app.catalog,
    )
    .unwrap()
}
fn publish(app: &mut GpuCandidate, generation: u64, wallpapers: Vec<WallpaperItem>) {
    let entries = wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    let _ = update(
        app,
        Message::Wake(Wake::CatalogLoaded {
            generation,
            wallpapers,
            entries,
        }),
    );
}

#[test]
fn p45_cached_catalog_is_available_before_held_scan_and_reprojects_local_tags() {
    let mut app = fixture();
    let path = app.state_path.with_file_name("snapshot.json");
    catalog_snapshot::save(&path, &snapshot(&app)).unwrap();
    let before = std::fs::read(&path).unwrap();
    let modified = path.metadata().unwrap().modified().unwrap();
    let cache = catalog_snapshot::load(&path, &app.config.catalog.local_folders, false).unwrap();
    app.state
        .local_library
        .edit(&app.wallpapers[0].id, "current local", true)
        .unwrap();
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let (tx, rx) = mpsc::channel();
    let owner = CatalogWatch::spawn_scanning(
        app.config.catalog.local_folders.clone(),
        false,
        Default::default(),
        app.catalog_scan_generation.clone(),
        move |w| tx.send(w).is_ok(),
        move |roots, _, _| {
            entered_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            scanner::scan_local_images(roots)
        },
    )
    .unwrap();
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    owner.enable_snapshots(path.clone());
    app.catalog_watch = Some(owner);
    app.wallpapers.clear();
    app.catalog.clear();
    app.catalog_loaded = false;
    app.install_cached_catalog(cache);
    assert!(
        rx.try_recv().is_err(),
        "fresh scan remains deliberately held"
    );
    assert!(app.catalog_loaded && !app.catalog_fresh && !app.startup_checked);
    assert_eq!(app.wallpapers.len(), 3);
    assert_eq!(app.filtered_indices.len(), 3);
    assert_eq!(app.catalog_presentation(), CatalogPresentation::Ready);
    assert!(app.catalog[0].tags.contains("current local"));
    assert!(!app.catalog[0].source_tags.contains("current local"));
    app.status = Some("unrelated status".into());
    assert!(app.status_for_display().unwrap().contains("stale"));
    // Build the actual production view while no fresh publication exists.
    let _ = view(&app, window::Id::unique());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(path.metadata().unwrap().modified().unwrap(), modified);
    release_tx.send(()).unwrap();
    let wake = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(matches!(wake, Wake::CatalogLoaded { .. }));
    let _ = update(&mut app, Message::Wake(wake));
    assert!(app.catalog_fresh && app.startup_checked);
    assert!(!app
        .status_for_display()
        .unwrap_or_default()
        .contains("stale"));
    let saved = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(
        matches!(saved, Wake::CatalogSnapshotSaved(Ok(()))),
        "{saved:?}"
    );
    assert_eq!(
        std::fs::read(&path).unwrap(),
        before,
        "identical fresh snapshot was refreshed"
    );
}

#[test]
fn p45_identical_first_fresh_clears_stale_once_without_changing_posters_or_revision() {
    let mut app = fixture();
    let cache = snapshot(&app);
    app.install_cached_catalog(cache);
    app.state.local_library.startup.reapply_enabled = true;
    app.launch_policy = p1_preview_policy();
    let source = poster_source_for_item(&app.wallpapers[0]);
    app.posters = Arc::new(vec![gpu_gallery::decode_poster(&source, 3).unwrap()]);
    let posters = app.posters.clone();
    let wallpaper_ptr = app.wallpapers.as_ptr();
    let catalog_ptr = app.catalog.as_ptr();
    let revision = app.catalog_generation.load(Ordering::Acquire);
    let generation = app.catalog_scan_generation.load(Ordering::Acquire);
    let clocks = (
        app.poster_generation.load(Ordering::Acquire),
        app.colours.clock.load(Ordering::Acquire),
    );
    let wallpapers = app.wallpapers.clone();
    publish(&mut app, generation, wallpapers.clone());
    assert!(app.catalog_fresh && app.startup_checked);
    assert!(Arc::ptr_eq(&posters, &app.posters));
    assert_eq!(app.wallpapers.as_ptr(), wallpaper_ptr);
    assert_eq!(app.catalog.as_ptr(), catalog_ptr);
    assert_eq!(app.catalog_generation.load(Ordering::Acquire), revision);
    assert_eq!(
        (
            app.poster_generation.load(Ordering::Acquire),
            app.colours.clock.load(Ordering::Acquire)
        ),
        clocks
    );
    let refusal = app.startup_status.clone();
    assert!(refusal.as_ref().unwrap().contains("blocked in --preview"));
    publish(&mut app, generation, wallpapers);
    assert_eq!(app.startup_status, refusal);
    assert!(!app.apply_pending);
}

#[test]
fn p45_changed_first_fresh_and_rejected_generations_preserve_freshness_boundary() {
    let mut app = fixture();
    let cache = snapshot(&app);
    app.install_cached_catalog(cache);
    let old = app.catalog_generation.load(Ordering::Acquire);
    let generation = app.catalog_scan_generation.load(Ordering::Acquire);
    let mut changed = app.wallpapers.clone();
    changed.pop();
    publish(&mut app, generation.wrapping_sub(1), changed.clone());
    assert!(!app.catalog_fresh && !app.startup_checked);
    assert_eq!(app.wallpapers.len(), 3);
    app.exit_requested = true;
    publish(&mut app, generation, changed.clone());
    assert!(!app.catalog_fresh && !app.startup_checked);
    app.exit_requested = false;
    publish(&mut app, generation, changed);
    assert!(app.catalog_fresh && app.startup_checked);
    assert_eq!(app.wallpapers.len(), 2);
    assert_eq!(app.catalog_generation.load(Ordering::Acquire), old + 1);
}

#[test]
fn p45_cached_startup_and_automation_have_no_provider_or_timer_authority() {
    let mut app = fixture();
    let cache = snapshot(&app);
    app.install_cached_catalog(cache);
    app.apply_enabled = true;
    app.state.local_library.startup.reapply_enabled = true;
    app.state.local_library.usage.targets.insert(
        "owned-monitor".into(),
        candidate_state::local_library::ConfirmedAssignment::capture(&app.wallpapers[0]),
    );
    app.startup_queue.push_back((
        "owned-monitor".into(),
        app.state.local_library.usage.targets["owned-monitor"].clone(),
    ));
    app.prepare_startup_reapply();
    app.prepare_startup_reapply_with(|_, _, _, _| panic!("stale startup submitted"));
    app.advance_startup_reapply();
    app.advance_startup_reapply_with(|_, _, _, _| panic!("stale startup advanced"));
    assert!(!app.startup_checked && !app.apply_pending);
    assert_eq!(app.startup_queue.len(), 1);
    let timers = (app.last_automation_minute, app.next_slideshow_unix);
    let generation = app.automation_generation.load(Ordering::Acquire);
    app.handle_automation_tick(generation, u64::MAX, 1, 480);
    assert_eq!(
        (app.last_automation_minute, app.next_slideshow_unix),
        timers
    );
    assert!(!app.run_playlist_with(
        None,
        None,
        AutomationTraceContext {
            generation: None,
            source: "random",
            schedule_id: None
        },
        || panic!("stale automatic action enumerated monitors"),
        |_, _, _, _| panic!("stale automatic action submitted")
    ));
    assert!(!app.apply_pending);
}

#[test]
fn p45_worker_saves_only_reducer_accepted_results_and_flushes_on_drop() {
    let mut app = fixture();
    let path = app.state_path.with_file_name("snapshot.json");
    catalog_snapshot::save(&path, &snapshot(&app)).unwrap();
    let original = std::fs::read(&path).unwrap();
    let (tx, rx) = mpsc::channel();
    let owner = CatalogWatch::spawn_scanning(
        app.config.catalog.local_folders.clone(),
        false,
        Default::default(),
        app.catalog_scan_generation.clone(),
        move |w| tx.send(w).is_ok(),
        |roots, _, _| scanner::scan_local_images(roots),
    )
    .unwrap();
    owner.enable_snapshots(path.clone());
    app.catalog_watch = Some(owner);
    let rejected = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    app.catalog_scan_generation.fetch_add(1, Ordering::AcqRel);
    let _ = update(&mut app, Message::Wake(rejected));
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert!(
        rx.recv_timeout(Duration::from_millis(150)).is_err(),
        "delivery ack cannot authorize persistence"
    );
    let media = &app.config.catalog.local_folders[0];
    image::RgbaImage::new(3, 2)
        .save(media.join("new-owned.png"))
        .unwrap();
    app.refresh_catalog();
    let accepted = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let _ = update(&mut app, Message::Wake(accepted));
    assert_eq!(app.wallpapers.len(), 4);
    // Stop immediately; bounded Drop may detach, but already accepted disk work must finish.
    drop(app.catalog_watch.take());
    let saved = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(
        matches!(saved, Wake::CatalogSnapshotSaved(Ok(()))),
        "{saved:?}"
    );
    assert_eq!(
        catalog_snapshot::load(&path, &app.config.catalog.local_folders, false)
            .unwrap()
            .into_catalog()
            .0,
        app.wallpapers
    );
}

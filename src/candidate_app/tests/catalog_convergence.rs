use super::*;
use crate::library::media_still::MediaStill;
use std::{sync::mpsc, time::Duration};

fn fixture() -> GpuCandidate {
    let (mut app, _) = d3_application_fixture(0);
    let root = app.state_path.parent().unwrap().join("catalog-owned");
    std::fs::create_dir(&root).unwrap();
    for i in 0..3 {
        image::RgbaImage::from_pixel(3, 2, image::Rgba([i * 30, 80, 120, 255]))
            .save(root.join(format!("{i}.png")))
            .unwrap();
    }
    app.wallpapers = scanner::scan_local_images(&[root]);
    app.catalog = app
        .wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    app.catalog_loaded = true;
    app.gallery_mode = GalleryMode::Slices;
    app.state.gallery.mode = GalleryKind::Slices;
    app.sort_order = SortOrder::Newest;
    app.filtered_indices = (0..3).collect();
    app.selected = Some(0);
    app.depth_frame = None;
    app.deterministic_animation = true;
    app
}
fn publish(app: &mut GpuCandidate, wallpapers: Vec<WallpaperItem>) {
    let entries = wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    let generation = app.catalog_scan_generation.load(Ordering::Acquire);
    let _ = update(
        app,
        Message::Wake(Wake::CatalogLoaded {
            generation,
            wallpapers,
            entries,
        }),
    );
}
fn noop(app: &mut GpuCandidate) {
    let wallpapers = app.wallpapers.clone();
    publish(app, wallpapers);
}
fn seed(app: &mut GpuCandidate) {
    let revision = app.catalog_generation.load(Ordering::Acquire);
    let posters = app
        .wallpapers
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let layer = index as u32 + 3;
            let path = poster_source_for_item(item);
            app.poster_slots.insert(
                index,
                PosterSlot {
                    layer,
                    kind: crate::poster_worker::PosterKind::Still,
                    generation: 0,
                    catalog_generation: revision,
                    item_id: item.id.clone(),
                    source: path.clone(),
                },
            );
            gpu_gallery::decode_poster(&path, layer).unwrap()
        })
        .collect();
    app.posters = Arc::new(posters);
}

#[test]
fn p41_noop_preserves_resources_work_detail_and_valid_draft_but_revokes_callbacks() {
    use crate::candidate_app::local_library::TagEdit;
    let mut app = fixture();
    seed(&mut app);
    app.panel_state.search_open = true;
    app.search_tags_open = true;
    app.reconcile_tag_editor();
    let old = app.tag_owner().unwrap();
    app.edit_local_tag(old.clone(), TagEdit::Text("retained draft".into()));
    app.state
        .local_library
        .edit(&app.wallpapers[0].id, "local override", true)
        .unwrap();
    app.project_local_tags();
    app.detail_open = true;
    app.detail_progress = 0.7;
    app.poster_loading = true;
    app.colours.pending = 2;
    app.colours.cursor = 2;
    app.colours.active = true;
    let input_id = app.local_tag_input_id.clone();
    let posters = app.posters.clone();
    let slots = app.poster_slots.clone();
    let clocks = (
        app.catalog_generation.load(Ordering::Acquire),
        app.poster_generation.load(Ordering::Acquire),
        app.colours.clock.load(Ordering::Acquire),
        app.detail_generation.load(Ordering::Acquire),
    );
    let wallpapers_ptr = app.wallpapers.as_ptr();
    let catalog_ptr = app.catalog.as_ptr();
    app.refresh_catalog();
    assert_eq!(app.catalog_generation.load(Ordering::Acquire), clocks.0);
    assert_eq!(app.local_tag_input, "retained draft");
    noop(&mut app);
    assert_eq!(app.wallpapers.as_ptr(), wallpapers_ptr);
    assert_eq!(app.catalog.as_ptr(), catalog_ptr);
    assert!(Arc::ptr_eq(&app.posters, &posters));
    assert_eq!(app.poster_slots, slots);
    assert_eq!(
        (
            app.catalog_generation.load(Ordering::Acquire),
            app.poster_generation.load(Ordering::Acquire),
            app.colours.clock.load(Ordering::Acquire),
            app.detail_generation.load(Ordering::Acquire)
        ),
        clocks
    );
    assert!(app.poster_loading && app.detail_open && app.colours.active);
    assert_eq!(
        (
            app.selected,
            app.detail_progress,
            app.colours.pending,
            app.colours.cursor
        ),
        (Some(0), 0.7, 2, 2)
    );
    assert_eq!(app.local_tag_input_id, input_id);
    assert_eq!(app.local_tag_input, "retained draft");
    let current = app.tag_owner().unwrap();
    assert_ne!(current, old);
    let state = app.state.clone();
    for edit in [
        TagEdit::Text("stale".into()),
        TagEdit::Add,
        TagEdit::Remove("local override".into()),
    ] {
        app.edit_local_tag(old.clone(), edit);
    }
    assert_eq!(app.state, state);
    assert_eq!(app.local_tag_input, "retained draft");
    app.edit_local_tag(current, TagEdit::Add);
    assert!(app.catalog[0].tags.contains("retained draft"));
}

#[test]
fn p41_noop_preserves_held_success_and_failure_delivery() {
    let mut app = fixture();
    let (entered, started) = mpsc::channel();
    let release = Arc::new(std::sync::Barrier::new(2));
    let held = release.clone();
    let (tx, rx) = mpsc::channel();
    let mailbox = app.poster_mailbox.clone();
    let worker = poster_worker::PosterWorker::spawn_with_builder(
        std::thread::Builder::new(),
        app.poster_generation.clone(),
        move |c| mailbox.publish(c, || tx.send(()).is_ok()).is_ok(),
        move |source, _| {
            if source.index == 0 {
                entered.send(()).unwrap();
                held.wait();
            }
            (source.index == 0)
                .then(|| gpu_gallery::decode_poster(&source.path, source.layer))
                .flatten()
        },
    )
    .unwrap();
    let generation = app.poster_generation.fetch_add(1, Ordering::AcqRel) + 1;
    let sources: Vec<_> = app
        .wallpapers
        .iter()
        .take(2)
        .enumerate()
        .map(|(index, item)| poster_worker::PosterSource {
            index,
            layer: index as u32,
            item_id: item.id.clone(),
            path: poster_source_for_item(item),
            kind: crate::poster_worker::PosterKind::Still,
            video_stamp: None,
            still: MediaStill::select(item, app.catalog_generation.load(Ordering::Acquire)),
        })
        .collect();
    app.poster_mailbox
        .expect(generation, sources.clone())
        .unwrap();
    worker
        .submit(poster_worker::PosterBatch::new(
            generation,
            sources,
            app.poster_cache_dir.clone(),
        ))
        .unwrap();
    app.poster_worker = Some(worker);
    app.poster_loading = true;
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    app.refresh_catalog();
    noop(&mut app);
    let preserved =
        app.poster_generation.load(Ordering::Acquire) == generation && app.poster_loading;
    release.wait();
    assert!(preserved, "no-op cancelled accepted poster work");
    while app.poster_loading {
        rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let _ = update(&mut app, Message::Wake(Wake::PostersReady));
    }
    assert!(app.poster_slots.contains_key(&0));
    assert!(app.poster_failed_indices.contains(&1));
}

#[test]
fn p41_noop_preserves_a_held_colour_job_and_accepted_revision() {
    let mut app = fixture();
    let still = MediaStill::select(
        &app.wallpapers[0],
        app.catalog_generation.load(Ordering::Acquire),
    )
    .unwrap();
    let (tx, rx) = mpsc::channel();
    let (entered, started) = mpsc::channel();
    let (release, resume) = mpsc::channel();
    let worker =
        poster_worker::PosterWorker::spawn(app.poster_generation.clone(), |_| true).unwrap();
    worker
        .submit_colour(poster_worker::colour::Job {
            generation: app.colours.clock.load(Ordering::Acquire),
            epoch: 0,
            catalog_clock: app.catalog_generation.clone(),
            clock: app.colours.clock.clone(),
            still: still.clone(),
            profile: app.state_path.parent().unwrap().into(),
            refresh: true,
            publish: Box::new(move |c| tx.send(c).unwrap()),
            held: Some(Box::new(move || {
                entered.send(()).unwrap();
                resume.recv_timeout(Duration::from_secs(10)).unwrap();
            })),
        })
        .unwrap();
    app.poster_worker = Some(worker);
    app.colours.pending = 1;
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    app.refresh_catalog();
    noop(&mut app);
    release.send(()).unwrap();
    let completion = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(completion.result.is_ok(), "{:?}", completion.result);
    app.install_colour(completion);
    assert!(app.colours.metadata.contains_key(&still.item_id));
}

#[test]
fn p41_changed_catalog_remaps_healthy_resources_and_rejects_changed_stamps() {
    let mut app = fixture();
    seed(&mut app);
    app.visible = false; // Inspect reducer retention before a later visible demand trims slots.
    let old = app.posters.clone();
    let selected = app.wallpapers[0].id.clone();
    let mut incoming = app.wallpapers.clone();
    incoming.reverse();
    incoming[1].title = "changed metadata".into();
    publish(&mut app, incoming.clone());
    let revision = app.catalog_generation.load(Ordering::Acquire);
    assert_eq!(app.posters.len(), 3);
    for (i, item) in incoming.iter().enumerate() {
        let slot = &app.poster_slots[&i];
        assert_eq!(slot.item_id, item.id);
        assert_eq!(slot.catalog_generation, revision);
        assert!(old.iter().any(|p| p.same_resource(&app.posters[i])));
    }
    assert_eq!(
        app.selected.map(|i| app.wallpapers[i].id.as_str()),
        Some(selected.as_str())
    );
    let changed = app.wallpapers[1].file_target.clone();
    let mut bytes = std::fs::read(&changed).unwrap();
    bytes.push(0);
    std::fs::write(&changed, bytes).unwrap();
    let incoming = scanner::scan_local_images(&[changed.parent().unwrap().into()]);
    publish(&mut app, incoming);
    assert_eq!(app.posters.len(), 2);
    assert!(!app.poster_slots.values().any(|s| s.source == changed));
    assert_eq!(
        app.selected.map(|i| app.wallpapers[i].id.as_str()),
        Some(selected.as_str())
    );
}

#[test]
fn p41_noop_preserves_populated_depth_frame_and_detail() {
    let (mut app, _) = d4a_fixture(12);
    app.catalog_loaded = true;
    d4b_seed_resident(&mut app);
    app.detail_open = true;
    app.detail_progress = 0.75;
    let frame = format!("{:?}", app.depth_frame);
    assert!(app.depth_frame.as_ref().unwrap().cards.len() > 1);
    let revision = app.depth_revision;
    let posters = app.posters.clone();
    let wallpapers = app.wallpapers.clone();
    let entries = app.catalog.clone();
    let generation = app.catalog_scan_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::Wake(Wake::CatalogLoaded {
            generation,
            wallpapers,
            entries,
        }),
    );
    assert_eq!(format!("{:?}", app.depth_frame), frame);
    assert_eq!(app.depth_revision, revision);
    assert!(Arc::ptr_eq(&app.posters, &posters));
    assert!(app.detail_open);
    assert_eq!(app.detail_progress, 0.75);
}

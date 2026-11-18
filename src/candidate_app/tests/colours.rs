use super::*;
use crate::library::{
    color_metadata::{self, ColorQuery},
    media_still::MediaStill,
};

#[test]
fn pk01_same_path_catalog_refresh_updates_mtime_without_history_or_identity_change() {
    let (mut app, submissions) = d4a_fixture(3);
    app.sort_order = SortOrder::Newest;
    app.refresh_filter();
    assert_eq!(app.filtered_indices, [2, 1, 0]);
    // Exercise the keyboard selection owner so initial-position restoration yields.
    app.navigate_selection(-1);
    assert_eq!(app.selected, Some(1));
    assert!(app.gallery_engaged);
    let id = app.wallpapers[1].id.clone();
    let history = app.state.local_library.usage.clone();
    let wallpapers = app.wallpapers.clone();
    let mut entries = app.catalog.clone();
    entries[0].modified_unix = 100;
    let generation = app.catalog_scan_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::Wake(Wake::CatalogLoaded {
            generation,
            wallpapers: wallpapers.clone(),
            entries,
        }),
    );
    assert_eq!(
        app.wallpapers
            .iter()
            .map(|w| (&w.id, &w.file_target, &w.project_json_path, &w.preview_path))
            .collect::<Vec<_>>(),
        wallpapers
            .iter()
            .map(|w| (&w.id, &w.file_target, &w.project_json_path, &w.preview_path))
            .collect::<Vec<_>>()
    );
    assert_eq!(app.filtered_indices, [0, 2, 1]);
    assert_eq!(app.selected.map(|i| app.wallpapers[i].id.clone()), Some(id));
    assert_eq!(app.state.local_library.usage, history);
    let _ = update(
        &mut app,
        Message::Wake(Wake::CatalogLoaded {
            generation: generation.wrapping_sub(1),
            wallpapers: Vec::new(),
            entries: Vec::new(),
        }),
    );
    assert_eq!(app.filtered_indices, [0, 2, 1]);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn e1_thirteen_rail_buckets_filter_toggle_clear_unknown_and_never_apply() {
    let (mut app, submissions) = d3_application_fixture(14);
    for index in 0..13 {
        let sample = image::RgbaImage::from_pixel(2, 2, image::Rgba([255, 0, 0, 255]));
        let path = app
            .state_path
            .parent()
            .unwrap()
            .join(format!("e1-bucket-{index}.png"));
        sample.save(&path).unwrap();
        app.wallpapers[index].file_target = path.clone();
        app.wallpapers[index].preview_path = Some(path.clone());
        app.wallpapers[index].source = WallpaperSource::LocalImage;
        let still = MediaStill::select(
            &app.wallpapers[index],
            app.catalog_generation.load(Ordering::Acquire),
        )
        .unwrap();
        let mut metadata = color_metadata::analyse_sample(
            &sample,
            color_metadata::hash_bytes(&std::fs::read(path).unwrap()),
        )
        .unwrap();
        metadata.hue = (index < 12).then_some(index as f32 * 30.0);
        app.colours.entries.insert(
            still.item_id.clone(),
            colours::Stored {
                still: Some(still.clone()),
                reason: None,
                persistent: false,
            },
        );
        app.colours.metadata.insert(still.item_id, metadata);
    }
    for index in 0..13 {
        let _ = update(
            &mut app,
            Message::DonorRail(donor_rail::RailIntent::HuePreset(index)),
        );
        assert_eq!(app.filtered_indices, [index]);
        assert_eq!(
            gallery_rail_model(&app).bucket,
            color_metadata::ColorBucket::from_index(index)
        );
        let before = app.colours.query.clone();
        let _ = update(
            &mut app,
            Message::DonorRail(donor_rail::RailIntent::HuePreset(13)),
        );
        assert_eq!(app.colours.query, before);
        let _ = update(
            &mut app,
            Message::DonorRail(donor_rail::RailIntent::HuePreset(index)),
        );
        assert_eq!(app.colours.query.bucket, None);
        assert_eq!(app.filtered_indices.len(), 14);
    }
    assert_eq!(submissions.load(Ordering::Acquire), 0);
    assert!(!app.apply_pending);
}

#[test]
fn w5_real_worker_result_requeries_preserves_selection_tags_geometry_and_rejects_stale() {
    use std::sync::mpsc;
    let (mut app, _) = d3_application_fixture(0);
    let media = app.state_path.parent().unwrap().join("w5-owned");
    std::fs::create_dir(&media).unwrap();
    for (name, rgb) in [("a-red", [255, 0, 0]), ("b-blue", [0, 0, 255])] {
        image::RgbaImage::from_pixel(16, 16, image::Rgba([rgb[0], rgb[1], rgb[2], 255]))
            .save(media.join(format!("{name}.png")))
            .unwrap();
    }
    app.wallpapers = scanner::scan_local_images(&[media]);
    app.catalog = app
        .wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    app.catalog_loaded = true;
    let id = app.wallpapers[0].id.clone();
    app.state.local_library.edit(&id, "w5-local", true).unwrap();
    app.project_local_tags();
    app.refresh_filter();
    app.selected = Some(0);
    let old = app.filtered_indices.clone();
    let selected = app.selected;
    let (tx, rx) = mpsc::channel();
    let worker =
        poster_worker::PosterWorker::spawn(app.poster_generation.clone(), |_| true).unwrap();
    let still = MediaStill::select(
        &app.wallpapers[0],
        app.catalog_generation.load(Ordering::Acquire),
    )
    .unwrap();
    let generation = app.colours.clock.load(Ordering::Acquire);
    worker
        .submit_colour(poster_worker::colour::Job {
            generation,
            epoch: 0,
            clock: app.colours.clock.clone(),
            catalog_clock: app.catalog_generation.clone(),
            still: still.clone(),
            profile: app.state_path.parent().unwrap().into(),
            refresh: true,
            publish: Box::new(move |c| tx.send(c).unwrap()),
            held: None,
        })
        .unwrap();
    app.poster_worker = Some(worker);
    // Establish ordinary resource demand before asserting passive metadata leaves it unchanged.
    app.schedule_posters();
    let poster_generation = app.poster_generation.load(Ordering::Acquire);
    let completion = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    assert!(completion.result.is_ok());
    let stale = completion.clone();
    app.install_colour(completion);
    assert_eq!(app.selected, selected);
    assert_eq!(app.filtered_indices, old);
    assert_eq!(
        app.poster_generation.load(Ordering::Acquire),
        poster_generation
    );
    assert!(app.catalog[0].tags.contains("w5-local"));
    assert!(app.colours.metadata.contains_key(&id));
    app.colours.active = true;
    app.colours.selection_anchor = Some(id.clone());
    app.colours.query.bucket = Some(color_metadata::ColorBucket::Blue);
    app.refresh_filter();
    assert!(app
        .selected
        .is_none_or(|i| app.filtered_indices.contains(&i)));
    assert!(app.colours.selection_anchor.is_none());
    app.colours.query = ColorQuery::default();
    app.colours.active = false;
    app.refresh_filter();
    app.selected = Some(0);
    app.colour_action(colours::Action::Bucket(color_metadata::ColorBucket::Red));
    assert_eq!(app.filtered_indices, [0]);
    assert_eq!(app.selected, selected);
    let snapshot = app.colours.query.bucket;
    app.selected = Some(1);
    assert_eq!(app.colours.query.bucket, snapshot);
    app.colours.active = true;
    app.colours.pending = 1;
    let mut failed = stale.clone();
    failed.result = Err("Colour worker stopped".into());
    failed.worker_failure = Some(poster_worker::WorkerFailure::WorkerExited);
    app.install_colour(failed);
    assert!(!app.colours.active);
    assert_eq!(app.colours.pending, 0);
    assert!(app.colours.status.contains("failed"));
    app.cancel_colours();
    app.colours.metadata.clear();
    app.install_colour(stale);
    assert!(app.colours.metadata.is_empty());
    app.colours.query = ColorQuery::default();
    app.refresh_filter();
    assert_eq!(app.filtered_indices.len(), 2);
    app.colours.query.bucket = Some(color_metadata::ColorBucket::Gray);
    app.refresh_filter();
    assert!(
        app.filtered_indices.is_empty(),
        "Unknown cannot be treated as black"
    );
}
#[test]
fn w5_actual_sort_reducer_cycles_all_delivered_orders_and_internal_cancellation_settles() {
    let (mut app, _) = d3_application_fixture(0);
    let expected = [
        SortOrder::ColourPop,
        SortOrder::Richness,
        SortOrder::Minimalist,
        SortOrder::ResolutionDescending,
        SortOrder::Newest,
        SortOrder::Recent,
        SortOrder::Colour,
    ];
    for order in expected {
        let _ = update(&mut app, Message::CycleSort);
        assert_eq!(app.sort_order, order);
    }
    let _ = update(&mut app, Message::CycleSortBack);
    assert_eq!(app.sort_order, SortOrder::Recent);
    app.start_colours(true);
    assert!(!app.colours.active);
    assert!(app.colours.status.contains("complete"));
    let clock = app.colours.clock.load(Ordering::Acquire);
    app.cancel_colours();
    assert!(app.colours.clock.load(Ordering::Acquire) > clock);
    assert_eq!(app.colours.pending, 0);
}

#[test]
fn w5_passive_anchor_never_overrides_keyboard_edge_or_detail_intent() {
    let (mut app, _) = d4a_fixture(3);
    let anchor = app.wallpapers[0].id.clone();
    app.filtered_indices = vec![1, 2];
    app.selected = Some(1);
    app.colours.active = true;
    app.colours.selection_anchor = Some(anchor.clone());
    app.navigate_selection(1);
    assert_eq!(app.selected, Some(2));
    assert!(app.colours.selection_anchor.is_none());
    app.refresh_colour_filter();
    assert_eq!(app.selected, Some(2));
    app.colours.selection_anchor = Some(anchor.clone());
    app.navigate_to_edge(true);
    assert!(app.colours.selection_anchor.is_none());
    app.colours.selection_anchor = Some(anchor);
    app.start_detail(true, "w5-explicit-detail");
    assert!(app.colours.selection_anchor.is_none());
}

#[test]
fn motion_repair_colour_publication_requeries_only_metadata_dependent_views() {
    use std::sync::mpsc;
    let (mut app, _) = d4a_fixture(0);
    let media = app.state_path.parent().unwrap().join("motion-owned-colour");
    std::fs::create_dir(&media).unwrap();
    image::RgbaImage::from_pixel(16, 16, image::Rgba([255, 0, 0, 255]))
        .save(media.join("owned-red.png"))
        .unwrap();
    app.wallpapers = scanner::scan_local_images(&[media]);
    app.catalog = app
        .wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    app.catalog_loaded = true;
    app.sort_order = SortOrder::Newest;
    app.refresh_filter();
    app.selected = Some(0);
    assert!(!app.colour_completion_affects_query());
    let still = MediaStill::select(
        &app.wallpapers[0],
        app.catalog_generation.load(Ordering::Acquire),
    )
    .unwrap();
    let (tx, rx) = mpsc::channel();
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
            held: None,
        })
        .unwrap();
    app.poster_worker = Some(worker);
    let completion = rx.recv_timeout(std::time::Duration::from_secs(10)).unwrap();
    assert!(completion.result.is_ok());
    let before = (
        app.filtered_indices.clone(),
        app.selected,
        app.motion_generation.load(Ordering::Acquire),
        app.poster_generation.load(Ordering::Acquire),
    );
    app.install_colour(completion.clone());
    assert_eq!(
        (
            app.filtered_indices.clone(),
            app.selected,
            app.motion_generation.load(Ordering::Acquire),
            app.poster_generation.load(Ordering::Acquire)
        ),
        before
    );
    assert!(app.colours.metadata.contains_key(&still.item_id));
    app.colours.query.bucket = Some(color_metadata::ColorBucket::Red);
    app.colours.metadata.clear();
    app.refresh_filter();
    assert!(app.filtered_indices.is_empty());
    assert!(app.colour_completion_affects_query());
    app.install_colour(completion);
    assert_eq!(
        app.filtered_indices,
        [0],
        "active filters see newly analysed metadata immediately"
    );
    app.colours.query = ColorQuery::default();
    for sort in [
        SortOrder::Colour,
        SortOrder::ColourPop,
        SortOrder::Richness,
        SortOrder::Minimalist,
    ] {
        app.sort_order = sort;
        assert!(
            app.colour_completion_affects_query(),
            "metadata sort {sort:?} must update immediately"
        );
    }
}

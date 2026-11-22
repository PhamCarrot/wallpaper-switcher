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

#[test]
fn wall_colour_indexing_keeps_order_and_holds_art_until_current_gpu_receipt() {
    use crate::library::sort_metrics::{self, SortMetrics};
    use iced::widget::shader::Program;

    let (mut app, _) = d4a_fixture(0);
    let media = app.state_path.parent().unwrap().join("wall-colour");
    std::fs::create_dir(&media).unwrap();
    for (name, rgb) in [
        ("a-blue.png", [0, 0, 255, 255]),
        ("b-red.png", [255, 0, 0, 255]),
    ] {
        image::RgbaImage::from_pixel(16, 16, image::Rgba(rgb))
            .save(media.join(name))
            .unwrap();
    }
    app.wallpapers = scanner::scan_local_images(&[media]);
    app.catalog = app
        .wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    app.catalog_loaded = true;
    app.visible = true;
    app.gallery_mode = GalleryMode::Wall;
    app.browse_position_restored = true;
    app.colours.active = true;
    app.colours.cursor = app.wallpapers.len();
    app.colours.pending = 2;
    app.refresh_filter();
    let [blue, red] = app.filtered_indices[..] else {
        panic!("expected two local wallpapers");
    };
    assert!(app.wallpapers[blue].title.contains("blue"));
    app.selected = Some(blue);
    app.sync_wall_presentation();
    app.wall_presentation.as_mut().unwrap().settle();

    let catalog_generation = app.catalog_generation.load(Ordering::Acquire);
    let mut posters = Vec::new();
    for (layer, entry) in app.ordinary_frame().into_iter().enumerate() {
        let layer = layer as u32;
        app.poster_slots.insert(
            entry.card.index,
            PosterSlot {
                layer,
                kind: poster_worker::PosterKind::Still,
                generation: 0,
                last_requested: 0,
                catalog_generation,
                item_id: entry.id,
                source: entry.source.clone(),
            },
        );
        posters.push(gpu_gallery::video_placeholder_poster(&entry.source, layer));
    }
    assert!(!posters.is_empty());
    app.posters = Arc::new(posters);
    let worker =
        poster_worker::PosterWorker::spawn(app.poster_generation.clone(), |_| true).unwrap();
    let epoch = worker.cache_epoch();
    app.poster_worker = Some(worker);
    let completion = |app: &GpuCandidate, index, hue, rgb| poster_worker::colour::Completion {
        generation: app.colours.clock.load(Ordering::Acquire),
        epoch,
        still: MediaStill::select(&app.wallpapers[index], catalog_generation).unwrap(),
        result: color_metadata::analyse_sample(
            &image::RgbaImage::from_pixel(1, 1, image::Rgba(rgb)),
            "0".repeat(64),
        ),
        sorting: Some(SortMetrics {
            version: sort_metrics::VERSION,
            hue,
            saturation: 100,
            richness: 100,
        }),
        persistence: Ok(()),
        worker_failure: None,
    };
    let displayed_before = app.ordinary_frame();
    let order_before = app.filtered_indices.clone();
    let generation_before = app.poster_generation.load(Ordering::Acquire);
    let motion_before = app.motion_generation.load(Ordering::Acquire);
    app.install_colour(completion(&app, red, 0, [255, 0, 0, 255]));
    app.refresh_filter();
    assert_eq!(app.filtered_indices, order_before);
    assert_eq!(
        app.poster_generation.load(Ordering::Acquire),
        generation_before
    );
    assert_eq!(app.motion_generation.load(Ordering::Acquire), motion_before);
    assert!(app.colours.metadata.contains_key(&app.wallpapers[red].id));
    assert!(app.colours.active);

    app.install_colour(completion(&app, blue, 8, [0, 0, 255, 255]));
    assert!(!app.colours.active);
    assert_eq!(app.filtered_indices, [red, blue]);
    assert_eq!(app.selected, Some(blue));
    assert!(app.wall_colour_hold.is_some());
    let current = app.ordinary_frame();
    let hold = app.wall_colour_hold.as_ref().unwrap();
    let held_wall = Some((hold.entries.clone(), hold.clip));
    let cards = |entries: &[donor_scene::CardPresentation]| {
        entries
            .iter()
            .map(|entry| (entry.id.clone(), entry.card, entry.crop))
            .collect::<Vec<_>>()
    };
    assert_eq!(cards(&hold.entries), cards(&displayed_before));
    assert_ne!(cards(&hold.entries), cards(&current));
    assert!(current
        .iter()
        .all(|entry| donor_scene::presented_hit_card(entry).is_some()));
    assert!(app.wall_colour_pins().iter().all(|pin| app
        .posters
        .iter()
        .any(|poster| poster.same_resource(&pin.poster))));

    // The view remains coherent until this demand's exact GPU receipt.
    let key = app.atlas_prewarm_key();
    let posters = app.posters.clone();
    let mut program = GalleryProgram {
        prewarm: Some((key, false)),
        snapshot: Arc::new(GallerySnapshot {
            defer_near_restore: false,
            held_wall,
            ordinary: Some(current),
            cards: Vec::new(),
            posters: posters.clone(),
            colors: GalleryColors::default(),
            depth: None,
            wall_clip: None,
            scene: DonorSceneState {
                stage: Default::default(),
                camera: 0.0,
                camera_target: 0.0,
                camera_velocity: 0.0,
                detail_progress: 0.0,
                external_detail: false,
                flip_options: Default::default(),
                detail_source: None,
                detail_card: None,
                viewport: app.viewport,
                time: 0.0,
                visibility: 1.0,
            },
        }),
        poster_feedback: None,
    };
    let wheel = iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
        delta: iced::mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
    });
    let bounds = iced::Rectangle::new(
        iced::Point::ORIGIN,
        iced::Size::new(app.viewport[0], app.viewport[1]),
    );
    let cursor = iced::mouse::Cursor::Available(iced::Point::new(100.0, 100.0));
    assert!(program
        .update(&mut Default::default(), &wheel, bounds, cursor)
        .is_some());
    let click = iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left));
    assert!(program
        .update(&mut Default::default(), &click, bounds, cursor)
        .is_none());
    Arc::make_mut(&mut program.snapshot).scene.visibility = 0.0;
    assert!(program
        .update(&mut Default::default(), &wheel, bounds, cursor)
        .is_none());

    app.poster_loading = false;
    let mut stale_key = key;
    stale_key.generation -= 1;
    let _ = update(
        &mut app,
        Message::Wake(Wake::PosterGpu(gpu_gallery::PosterFeedback::Prewarmed {
            key: stale_key,
            posters: posters.clone(),
        })),
    );
    assert!(app.wall_colour_hold.is_some());
    let _ = update(
        &mut app,
        Message::Wake(Wake::PosterGpu(gpu_gallery::PosterFeedback::Prewarmed {
            key,
            posters: Arc::new(posters.as_ref().clone()),
        })),
    );
    assert!(app.wall_colour_hold.is_some());
    let _ = update(
        &mut app,
        Message::Wake(Wake::PosterGpu(gpu_gallery::PosterFeedback::Prewarmed {
            key,
            posters,
        })),
    );
    assert!(app.wall_colour_hold.is_none());
    assert!(app.wall_colour_pins().is_empty());
    assert!(app
        .ordinary_frame()
        .iter()
        .all(|entry| donor_scene::presented_hit_card(entry).is_some()));

    app.wall_colour_hold = app.capture_wall_colour_hold();
    app.wall_colour_hold.as_mut().unwrap().deadline = std::time::Instant::now();
    app.start_motion(0.0);
    let generation = app.motion_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::Wake(Wake::MotionFrame {
            generation,
            value: 0.0,
            velocity: 0.0,
            state: GalleryMotionState::source(0.0, 0.0, 250.0, 1.0),
            selection: HashMap::new(),
            presentation: None,
            ordinary: None,
            hex: None,
            wall: None,
            scene: None,
        }),
    );
    assert!(app.wall_colour_hold.is_none());
    assert!(app.wall_colour_pins().is_empty());

    app.wall_colour_hold = app.capture_wall_colour_hold();
    let bound = crate::candidate_app::lifecycle::bind_gallery_input(
        &app,
        GalleryMessage::BoundWheel {
            amount: -1.0,
            vertical: -1.0,
            modifiers: Default::default(),
            sequence: None,
        },
    );
    let _ = update(&mut app, bound);
    assert!(app.wall_colour_hold.is_none());
    assert!(app.wall_colour_pins().is_empty());

    app.wall_colour_hold = app.capture_wall_colour_hold();
    let reordered = app.filtered_indices.clone();
    app.refresh_filter();
    assert_eq!(app.filtered_indices, reordered);
    assert!(app.wall_colour_hold.is_none());
    assert!(app.wall_colour_pins().is_empty());
}

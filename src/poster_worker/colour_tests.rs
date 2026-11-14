use super::*;
use crate::poster_worker::{cache, PosterWorker};
use std::{
    sync::{mpsc, Barrier},
    time::Duration,
};

fn fixture() -> (PathBuf, MediaStill) {
    let (profile, _) = cache::tests::fixture();
    let source = profile.join("owned-red.png");
    image::RgbaImage::from_pixel(16, 16, image::Rgba([255, 0, 0, 255]))
        .save(&source)
        .unwrap();
    let item = crate::scanner::scan_local_images(&[profile.clone()]).remove(0);
    (profile, MediaStill::select(&item, 1).unwrap())
}
#[test]
fn pk01_legacy_cache_coexists_with_versioned_sort_payload_and_retains_unknown_fields() {
    let (profile, still) = fixture();
    let first = run(&profile, &still, false);
    let modern = Entry {
        still: still.clone(),
        metadata: first.result.unwrap(),
        sorting: first.sorting,
    };
    let mut legacy = modern.clone();
    legacy.sorting = None;
    assert_ne!(legacy.name(), modern.name());
    persist(&profile, &legacy, &|| false).unwrap();
    let root = profile.join("cache/colour-v1");
    let original = std::fs::read(root.join(legacy.name())).unwrap();
    assert_eq!(valid_bytes(&legacy.name(), &original), Some(legacy.clone()));
    std::fs::remove_file(root.join(modern.name())).unwrap();
    let fresh = run(&profile, &still, false);
    assert_eq!(fresh.result.unwrap(), modern.metadata);
    assert_eq!(fresh.sorting, modern.sorting);
    assert_eq!(std::fs::read(root.join(legacy.name())).unwrap(), original);
    assert!(root.join(modern.name()).is_file());
    let mut foreign = serde_json::to_value(&modern).unwrap();
    foreign["sorting"]["future_owner"] = serde_json::json!(true);
    assert!(valid_bytes(&modern.name(), &serde_json::to_vec(&foreign).unwrap()).is_none());
}
#[test]
fn pk01_transparent_sort_metrics_do_not_fabricate_w5_values_and_stale_input_is_unavailable() {
    let (profile, mut still) = fixture();
    image::RgbaImage::from_pixel(16, 16, image::Rgba([255, 0, 0, 0]))
        .save(&still.path)
        .unwrap();
    let item = crate::scanner::local_image_item(&still.path).unwrap();
    still = MediaStill::select(&item, 1).unwrap();
    let completion = run(&profile, &still, true);
    assert!(completion.result.unwrap_err().contains("Transparent-only"));
    assert_eq!(completion.sorting.unwrap().hue, 0);
    assert!(completion.persistence.is_err());
    std::fs::remove_file(&still.path).unwrap();
    let stale = run(&profile, &still, false);
    assert!(stale.result.is_err());
    assert!(stale.sorting.is_none());
}
fn run(profile: &Path, still: &MediaStill, refresh: bool) -> Completion {
    let (tx, rx) = mpsc::channel();
    let clock = Arc::new(AtomicU64::new(1));
    execute(
        Job {
            generation: 1,
            clock,
            catalog_clock: Arc::new(AtomicU64::new(still.catalog_revision)),
            epoch: 0,
            still: still.clone(),
            profile: profile.into(),
            refresh,
            publish: Box::new(move |c| tx.send(c).unwrap()),
            held: None,
        },
        0,
        &AtomicU64::new(0),
        &|| false,
    );
    rx.recv_timeout(Duration::from_secs(10)).unwrap()
}
#[test]
fn w5_production_decode_content_refresh_provenance_missing_and_bounds() {
    let (profile, still) = fixture();
    let first = run(&profile, &still, false);
    let red = first.result.unwrap();
    assert_eq!(red.dominant_rgb, [255, 0, 0]);
    assert!(first.persistence.is_ok());
    let encoded = std::fs::read(&still.path).unwrap();
    assert_eq!(
        red.fingerprint,
        crate::library::color_metadata::hash_bytes(&encoded)
    );
    let cached = run(&profile, &still, false);
    assert_eq!(cached.result.unwrap(), red);
    // Poison a syntactically owned disposable entry: ordinary load uses it; explicit refresh bypasses it.
    let mut poisoned = Entry {
        still: still.clone(),
        metadata: red.clone(),
        sorting: first.sorting,
    };
    poisoned.metadata.dominant_rgb = [0, 0, 255];
    let path = profile.join("cache/colour-v1").join(poisoned.name());
    std::fs::write(&path, serde_json::to_vec(&poisoned).unwrap()).unwrap();
    assert_eq!(
        run(&profile, &still, false).result.unwrap().dominant_rgb,
        [0, 0, 255]
    );
    let refreshed = run(&profile, &still, true);
    assert_eq!(refreshed.result.unwrap().dominant_rgb, [255, 0, 0]);
    assert!(refreshed.persistence.is_err());
    let stamp = still.fingerprint.clone();
    // BMP keeps a fixed byte length; set mtime back to prove content hash differs despite an equal cheap stamp.
    let bmp = profile.join("fixed.bmp");
    image::RgbaImage::from_pixel(16, 16, image::Rgba([255, 0, 0, 255]))
        .save(&bmp)
        .unwrap();
    let mut bmp_still = still.clone();
    bmp_still.path = bmp.clone();
    let meta = bmp.metadata().unwrap();
    bmp_still.fingerprint.bytes = meta.len();
    bmp_still.fingerprint.modified = meta.modified().unwrap();
    let before = run(&profile, &bmp_still, true).result.unwrap();
    image::RgbaImage::from_pixel(16, 16, image::Rgba([0, 0, 255, 255]))
        .save(&bmp)
        .unwrap();
    std::fs::OpenOptions::new()
        .write(true)
        .open(&bmp)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(bmp_still.fingerprint.modified))
        .unwrap();
    assert!(bmp_still.is_current());
    let after = run(&profile, &bmp_still, true).result.unwrap();
    assert_ne!(before.fingerprint, after.fingerprint);
    assert_eq!(after.dominant_rgb, [0, 0, 255]);
    assert_eq!(still.fingerprint, stamp);
    let mut item = crate::scanner::scan_local_images(&[profile.clone()])
        .into_iter()
        .find(|i| i.file_target == still.path)
        .unwrap();
    for source in [
        crate::scanner::WallpaperSource::LocalVideo,
        crate::scanner::WallpaperSource::WallpaperEngine,
    ] {
        item.source = source;
        item.wallpaper_type = "video".into();
        item.preview_path = Some(still.path.clone());
        item.file_target = profile.join("payload.pkg");
        std::fs::write(&item.file_target, b"untouched scene/video payload").unwrap();
        let poster = MediaStill::select(&item, 1).unwrap();
        assert_eq!(
            poster.provenance,
            crate::library::media_still::Provenance::ExistingPoster
        );
        assert_eq!(
            run(&profile, &poster, true).result.unwrap().dominant_rgb,
            [255, 0, 0]
        );
        item.preview_path = None;
        assert!(MediaStill::select(&item, 1).is_none());
        assert_eq!(
            std::fs::read(&item.file_target).unwrap(),
            b"untouched scene/video payload"
        );
    }
    let alpha_path = profile.join("hidden-rgb.png");
    let mut alpha = image::RgbaImage::new(256, 256);
    for (x, y, p) in alpha.enumerate_pixels_mut() {
        *p = if (x / 3 + y / 5) % 2 == 0 {
            image::Rgba([0, 0, 255, 255])
        } else {
            image::Rgba([255, 0, 0, 0])
        };
    }
    alpha.save(&alpha_path).unwrap();
    let mut alpha_still = still.clone();
    alpha_still.path = alpha_path;
    let meta = alpha_still.path.metadata().unwrap();
    alpha_still.fingerprint.bytes = meta.len();
    alpha_still.fingerprint.modified = meta.modified().unwrap();
    // Source-pixel samples cannot mix invisible red into visible blue metrics.
    let colour = run(&profile, &alpha_still, true).result.unwrap();
    assert_eq!(colour.dominant_rgb, [0, 0, 255]);
    assert_eq!(colour.hue, Some(240.0));
    assert!((colour.brightness - 0.0722).abs() < 0.0001);
    let transparent = profile.join("transparent.png");
    image::RgbaImage::new(8, 8).save(&transparent).unwrap();
    let mut t = still.clone();
    t.path = transparent;
    t.fingerprint.bytes = t.path.metadata().unwrap().len();
    t.fingerprint.modified = t.path.metadata().unwrap().modified().unwrap();
    assert!(run(&profile, &t, true)
        .result
        .unwrap_err()
        .contains("Transparent-only"));
    let mut too_big = still.clone();
    too_big.fingerprint.bytes = MAX_BYTES as u64 + 1;
    assert!(analyse(&too_big, &profile, true, &|| false).is_err());
    assert!(analyse(&still, &profile, true, &|| true).is_err());
    std::fs::remove_file(&still.path).unwrap();
    assert!(run(&profile, &still, true).result.is_err());
}
#[test]
fn w5_colour_owner_precise_clear_refusal_and_session_only() {
    let (profile, still) = fixture();
    let completion = run(&profile, &still, false);
    let entry = Entry {
        still: still.clone(),
        metadata: completion.result.unwrap(),
        sorting: completion.sorting,
    };
    let root = profile.join("cache/colour-v1");
    let owned = root.join(entry.name());
    assert!(owned.exists());
    let mut sentinels = Vec::new();
    for name in [
        "arbitrary.json",
        "colour-old.json",
        "source.png",
        "recovery.sentinel",
        "export.png",
        "export.receipt.json",
    ] {
        let p = root.join(name);
        std::fs::write(&p, b"keep exact bytes").unwrap();
        sentinels.push(p);
    }
    let bad = root.join(format!("colour-{}.json", "a".repeat(64)));
    std::fs::write(&bad, b"not our format").unwrap();
    sentinels.push(bad);
    let mut linked = entry.clone();
    linked.still.item_id = "linked".into();
    let linked_path = root.join(linked.name());
    std::fs::write(&linked_path, serde_json::to_vec(&linked).unwrap()).unwrap();
    std::fs::hard_link(&linked_path, profile.join("protected-hardlink.json")).unwrap();
    let mut held = entry.clone();
    held.still.item_id = "held".into();
    let held_path = root.join(held.name());
    std::fs::write(&held_path, serde_json::to_vec(&held).unwrap()).unwrap();
    use std::os::windows::fs::OpenOptionsExt;
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(windows_sys::Win32::Storage::FileSystem::FILE_SHARE_READ)
        .open(&held_path)
        .unwrap();
    for (id, fingerprint_future) in [("future-still", false), ("future-stamp", true)] {
        let mut future = entry.clone();
        future.still.item_id = id.into();
        let mut value = serde_json::to_value(&future).unwrap();
        if fingerprint_future {
            value["still"]["fingerprint"]["future_identity"] = true.into();
        } else {
            value["still"]["future_owner"] = true.into();
        }
        let path = root.join(future.name());
        let bytes = serde_json::to_vec(&value).unwrap();
        assert!(valid_bytes(path.file_name().unwrap().to_str().unwrap(), &bytes).is_none());
        std::fs::write(&path, bytes).unwrap();
        sentinels.push(path);
    }
    let before = sentinels
        .iter()
        .map(|p| (p.clone(), std::fs::read(p).unwrap()))
        .collect::<Vec<_>>();
    let report = cache::clear_colour(&profile, 4);
    eprintln!("W5 colour clear partial: {report:?}");
    assert_eq!((report.deleted, report.retained, report.failed), (1, 10, 1));
    assert!(!owned.exists());
    assert!(linked_path.exists());
    assert!(held_path.exists());
    for (p, b) in &before {
        assert_eq!(std::fs::read(p).unwrap(), *b);
    }
    drop(lock);
    assert_eq!(cache::clear_colour(&profile, 5).deleted, 1);
    assert_eq!(
        std::fs::read(profile.join("protected-hardlink.json")).unwrap(),
        serde_json::to_vec(&linked).unwrap()
    );
    // A file where the owned root belongs is refused; analysis remains usable in session.
    let refused = profile.join("refused-profile");
    std::fs::create_dir(&refused).unwrap();
    std::fs::create_dir(refused.join("cache")).unwrap();
    std::fs::write(refused.join("cache/colour-v1"), b"foreign root bytes").unwrap();
    let c = run(&refused, &still, true);
    assert!(c.result.is_ok());
    assert!(c.persistence.is_err());
    assert!(cache::clear_colour(&refused, 6).error.is_some());
    assert_eq!(
        std::fs::read(refused.join("cache/colour-v1")).unwrap(),
        b"foreign root bytes"
    );
    assert!(cache::RootGuard::open_colour(&profile, &root.join("child"), true).is_err());
}
#[test]
fn w5_held_production_analysis_epoch_clear_and_regeneration() {
    let (profile, still) = fixture();
    let c = run(&profile, &still, false);
    let entry = Entry {
        still: still.clone(),
        metadata: c.result.unwrap(),
        sorting: c.sorting,
    };
    let owned = profile.join("cache/colour-v1").join(entry.name());
    let source_before = std::fs::read(&still.path).unwrap();
    let clock = Arc::new(AtomicU64::new(1));
    let catalog_clock = Arc::new(AtomicU64::new(1));
    let worker = PosterWorker::spawn(clock.clone(), |_| true).unwrap();
    let (tx, rx) = mpsc::channel();
    let (started, waiting) = mpsc::channel();
    let release = Arc::new(Barrier::new(2));
    let held = release.clone();
    worker
        .submit_colour(Job {
            generation: 1,
            clock: clock.clone(),
            catalog_clock,
            epoch: 0,
            still: still.clone(),
            profile: profile.clone(),
            refresh: true,
            publish: Box::new(move |c| tx.send(c).unwrap()),
            held: Some(Box::new(move || {
                started.send(()).unwrap();
                held.wait();
            })),
        })
        .unwrap();
    waiting.recv_timeout(Duration::from_secs(10)).unwrap();
    let (ack, ack_rx) = mpsc::channel();
    let epoch = worker
        .request_cache(cache::Request {
            profile: profile.clone(),
            root: profile.join("cache/posters"),
            names: None,
            colour: true,
            epoch: 0,
            acknowledge: Box::new(move |r| ack.send(r).unwrap()),
        })
        .unwrap();
    assert!(ack_rx.try_recv().is_err());
    assert!(worker.cache_busy());
    release.wait();
    let report = ack_rx.recv_timeout(Duration::from_secs(20)).unwrap();
    eprintln!("W5 held analysis clear: {report:?}");
    assert_eq!(report.epoch, epoch);
    assert_eq!(report.deleted, 1);
    assert_eq!(report.failed, 0);
    assert!(report.error.is_none());
    assert!(!owned.exists());
    assert!(rx.try_recv().is_err());
    assert!(run(&profile, &still, true).persistence.is_ok());
    assert!(owned.exists());
    assert_eq!(std::fs::read(&still.path).unwrap(), source_before);
    drop(worker);
}
#[test]
fn w5_cancel_catalog_and_changed_input_settlement_never_publish_stale_metrics() {
    let (profile, still) = fixture();
    let clock = Arc::new(AtomicU64::new(2));
    let catalog = Arc::new(AtomicU64::new(1));
    let (tx, rx) = mpsc::channel();
    execute(
        Job {
            generation: 1,
            clock: clock.clone(),
            catalog_clock: catalog.clone(),
            epoch: 0,
            still: still.clone(),
            profile: profile.clone(),
            refresh: true,
            publish: Box::new(move |c| tx.send(c).unwrap()),
            held: None,
        },
        0,
        &AtomicU64::new(0),
        &|| false,
    );
    assert!(rx.try_recv().is_err());
    clock.store(1, Ordering::Release);
    catalog.store(2, Ordering::Release);
    let (tx, rx) = mpsc::channel();
    execute(
        Job {
            generation: 1,
            clock,
            catalog_clock: catalog,
            epoch: 0,
            still: still.clone(),
            profile: profile.clone(),
            refresh: true,
            publish: Box::new(move |c| tx.send(c).unwrap()),
            held: None,
        },
        0,
        &AtomicU64::new(0),
        &|| false,
    );
    assert!(rx.try_recv().is_err());
    assert!(!profile.join("cache/colour-v1").exists());
}

#[test]
fn w5_queued_colour_job_settles_when_shared_poster_decoder_fails() {
    let (profile, still) = fixture();
    let clock = Arc::new(AtomicU64::new(1));
    let (started, waiting) = mpsc::channel();
    let release = Arc::new(Barrier::new(2));
    let held = release.clone();
    let worker = PosterWorker::spawn_with_builder(
        std::thread::Builder::new().name("w5-failing-shared-decode".into()),
        clock.clone(),
        |_| true,
        move |_, _| {
            started.send(()).unwrap();
            held.wait();
            panic!("owned failing poster seam")
        },
    )
    .unwrap();
    worker
        .submit(crate::poster_worker::PosterBatch::new(
            1,
            vec![crate::poster_worker::PosterSource {
                index: 0,
                layer: 0,
                item_id: still.item_id.clone(),
                is_video: false,
                still: Some(still.clone()),
                path: still.path.clone(),
            }],
            profile.join("cache/posters"),
        ))
        .unwrap();
    waiting.recv_timeout(Duration::from_secs(10)).unwrap();
    let (tx, rx) = mpsc::channel();
    worker
        .submit_colour(Job {
            generation: 1,
            epoch: 0,
            clock,
            catalog_clock: Arc::new(AtomicU64::new(1)),
            still,
            profile: profile.clone(),
            refresh: true,
            publish: Box::new(move |c| tx.send(c).unwrap()),
            held: None,
        })
        .unwrap();
    release.wait();
    let c = rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(
        c.worker_failure,
        Some(crate::poster_worker::WorkerFailure::DecoderPanicked)
    );
    assert!(c.result.is_err());
    worker.wait_until_stopped();
    assert!(!profile.join("cache/colour-v1").exists());
}

#[test]
fn w5_staged_colour_publication_collision_never_replaces_destination() {
    let (profile, _) = fixture();
    let root = profile.join("cache/colour-v1");
    let _owner = cache::RootGuard::open_colour(&profile, &root, true)
        .unwrap()
        .unwrap();
    let staged = root.join(".owned-stage.tmp");
    let destination = root.join("foreign-destination.json");
    std::fs::write(&staged, b"new colour entry").unwrap();
    // Represents a destination arriving after the caller's absence check.
    std::fs::write(&destination, b"unknown recovery bytes").unwrap();
    assert!(
        crate::file_publication::publish_staged_file_create_only(&staged, &destination).is_err()
    );
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"unknown recovery bytes"
    );
    assert_eq!(std::fs::read(&staged).unwrap(), b"new colour entry");
    let fresh = root.join("new-destination.json");
    crate::file_publication::publish_staged_file_create_only(&staged, &fresh).unwrap();
    assert_eq!(std::fs::read(&fresh).unwrap(), b"new colour entry");
    assert!(!staged.exists());
    assert_eq!(
        std::fs::read(&destination).unwrap(),
        b"unknown recovery bytes"
    );
}

#[test]
fn retirement_worker_exit_closes_colour_submission_before_drain_unlock() {
    let shared = Arc::new(crate::poster_worker::Shared::new());
    let worker = PosterWorker {
        shared: shared.clone(),
    };
    let held = Arc::new(Barrier::new(2));
    let pause = held.clone();
    let (tx, rx) = mpsc::channel();
    let exiting = shared.clone();
    let thread = std::thread::spawn(move || {
        crate::poster_worker::record_failure(
            &exiting,
            crate::poster_worker::WorkerFailure::DecoderPanicked,
        );
        *exiting.failure.lock().unwrap() = None;
        crate::poster_worker::finish_worker_exit(&exiting, || {
            tx.send(()).unwrap();
            pause.wait();
        });
    });
    rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let (profile, still) = fixture();
    let submission = worker.submit_colour(Job {
        generation: 1,
        epoch: 0,
        clock: Arc::new(AtomicU64::new(1)),
        catalog_clock: Arc::new(AtomicU64::new(1)),
        still,
        profile,
        refresh: true,
        publish: Box::new(|_| panic!("rejected job cannot publish")),
        held: None,
    });
    held.wait();
    thread.join().unwrap();
    assert!(
        submission.is_err(),
        "exit closes availability before drain-unlock interval"
    );
    assert!(shared.queue.lock().unwrap().colour.is_none());
}

#[test]
fn retirement_poisoned_worker_queue_settles_colour_and_cache_ack() {
    let shared = Arc::new(crate::poster_worker::Shared::new());
    let (profile, still) = fixture();
    let (tx, rx) = mpsc::channel();
    let (ack, ack_rx) = mpsc::channel();
    {
        let mut queue = shared.queue.lock().unwrap();
        queue.colour = Some(Job {
            generation: 1,
            epoch: 0,
            clock: Arc::new(AtomicU64::new(1)),
            catalog_clock: Arc::new(AtomicU64::new(1)),
            still,
            profile: profile.clone(),
            refresh: true,
            publish: Box::new(move |c| tx.send(c).unwrap()),
            held: None,
        });
        queue.cache_busy = true;
        queue.clear = Some(cache::Request {
            profile: profile.clone(),
            root: profile.join("cache/posters"),
            names: None,
            colour: true,
            epoch: 7,
            acknowledge: Box::new(move |r| ack.send(r).unwrap()),
        });
    }
    let poisoned = shared.clone();
    assert!(std::thread::spawn(move || {
        let _lock = poisoned.queue.lock().unwrap();
        panic!("controlled queue poison");
    })
    .join()
    .is_err());
    crate::poster_worker::finish_worker_exit(&shared, || {});
    let result = rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(result.result.is_err());
    assert_eq!(
        result.worker_failure,
        Some(crate::poster_worker::WorkerFailure::WorkerExited)
    );
    let report = ack_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(report.epoch, 7);
    assert!(report.error.unwrap().contains("no clear acknowledged"));
    assert!(!shared.running.load(Ordering::Acquire));
    assert!(
        !shared
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .cache_busy
    );
    assert!(!profile.join("cache/colour-v1").exists());
}

#[test]
fn retirement_retained_jpeg_colour_and_poster_decoding_remains_available() {
    for (name, dimensions) in [
        ("rgb-baseline.jpg", (24, 8)),
        ("rgb-progressive.jpeg", (24, 8)),
        ("gray-oriented.jpg", (24, 8)),
    ] {
        let (profile, mut still) = fixture();
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/effects-jpeg")
                .join(name),
        )
        .unwrap();
        let path = profile.join(name);
        std::fs::write(&path, &bytes).unwrap();
        let image = image::open(&path).unwrap().into_rgba8();
        assert_eq!(image.dimensions(), dimensions);
        assert!(image.pixels().all(|p| p[3] == 255));
        assert!(crate::gpu_gallery::decode_poster(&path, 0).is_some());
        still.path = path;
        let item = crate::scanner::local_image_item(&still.path).unwrap();
        still = MediaStill::select(&item, 1).unwrap();
        let completion = run(&profile, &still, true);
        assert!(completion.persistence.is_ok());
        let result = Entry {
            still: still.clone(),
            metadata: completion.result.unwrap(),
            sorting: completion.sorting,
        };
        assert!(result.metadata.valid());
        assert_eq!(
            result.metadata.fingerprint,
            crate::library::color_metadata::hash_bytes(&bytes)
        );
        assert!(result.name().starts_with("colour-"));
        assert_eq!(result.name(), result.name().to_ascii_lowercase());
    }
}

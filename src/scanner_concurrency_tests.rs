use super::*;
use std::sync::{atomic::AtomicBool, mpsc, Condvar, Mutex};
use std::time::Duration;

fn wait(gate: &(Mutex<bool>, Condvar)) {
    let (open, _) = gate
        .1
        .wait_timeout_while(gate.0.lock().unwrap(), Duration::from_secs(10), |open| {
            !*open
        })
        .unwrap();
    assert!(*open, "held I/O must be released");
}
fn release(gate: &(Mutex<bool>, Condvar)) {
    *gate.0.lock().unwrap() = true;
    gate.1.notify_all();
}

#[test]
fn p54_policy_is_adaptive_and_explicit_override_is_bounded() {
    for (logical, expected) in [(0, 2), (1, 2), (2, 4), (4, 4), (6, 6), (32, 8)] {
        assert_eq!(catalog_scan_threads(logical, None), expected);
    }
    assert_eq!(catalog_scan_threads(32, Some(2)), 2);
    assert_eq!(catalog_scan_threads(1, Some(1)), 1);
    assert_eq!(catalog_scan_threads(1, Some(255)), 8);
    let (logical, affinity) = crate::poster_worker::process_parallelism();
    if affinity != usize::MAX {
        assert_eq!(logical, affinity.count_ones() as usize);
    }
    assert_eq!(
        default_catalog_scan_threads(),
        catalog_scan_threads(logical, None)
    );
    assert_eq!(configured_catalog_scan_threads(Some(2)), 2);
}

#[test]
fn p54_held_io_reaches_but_never_exceeds_cap_and_gathers_in_order() {
    for cap in [1, 2, 4, 8] {
        let gate = (Mutex::new(false), Condvar::new());
        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let (tx, rx) = mpsc::channel();
        std::thread::scope(|scope| {
            let scan = scope.spawn(|| {
                scan_ordered(&(0..32).collect::<Vec<_>>(), cap, &|| false, |index| {
                    let count = active.fetch_add(1, Ordering::SeqCst) + 1;
                    peak.fetch_max(count, Ordering::SeqCst);
                    tx.send(*index).unwrap();
                    wait(&gate);
                    active.fetch_sub(1, Ordering::SeqCst);
                    index * 2
                })
            });
            let entered: Vec<_> = (0..cap)
                .map(|_| rx.recv_timeout(Duration::from_secs(10)))
                .collect();
            let count = active.load(Ordering::SeqCst);
            let extra = rx.try_recv();
            release(&gate);
            let results = scan.join().unwrap();
            assert!(entered.iter().all(Result::is_ok));
            assert_eq!(count, cap);
            assert!(extra.is_err(), "no claim beyond held capacity");
            assert_eq!(peak.load(Ordering::SeqCst), cap);
            assert_eq!(active.load(Ordering::SeqCst), 0);
            assert_eq!(results, (0..32).map(|n| n * 2).collect::<Vec<_>>());
        });
    }
}

#[test]
fn p54_cancellation_stops_claims_discards_results_and_joins_held_io() {
    let gate = (Mutex::new(false), Condvar::new());
    let cancelled = AtomicBool::new(false);
    let active = AtomicUsize::new(0);
    let claims = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel();
    let (done, finished) = mpsc::channel();
    std::thread::scope(|scope| {
        let scan = scope.spawn(|| {
            let results = scan_ordered(
                &(0..64).collect::<Vec<_>>(),
                2,
                &|| cancelled.load(Ordering::SeqCst),
                |index| {
                    claims.fetch_add(1, Ordering::SeqCst);
                    active.fetch_add(1, Ordering::SeqCst);
                    tx.send(()).unwrap();
                    wait(&gate);
                    active.fetch_sub(1, Ordering::SeqCst);
                    *index
                },
            );
            done.send(()).unwrap();
            results
        });
        let entered: Vec<_> = (0..2)
            .map(|_| rx.recv_timeout(Duration::from_secs(10)))
            .collect();
        cancelled.store(true, Ordering::SeqCst);
        let prematurely_returned = finished.recv_timeout(Duration::from_millis(30)).is_ok();
        let held = active.load(Ordering::SeqCst);
        release(&gate);
        let results = scan.join().unwrap();
        assert!(entered.iter().all(Result::is_ok));
        assert!(!prematurely_returned, "cancel cannot detach active I/O");
        assert_eq!(held, 2);
        assert_eq!(claims.load(Ordering::SeqCst), 2);
        assert_eq!(active.load(Ordering::SeqCst), 0);
        assert!(
            results.is_empty(),
            "cancelled work cannot become a partial catalog"
        );
    });
    let calls = AtomicUsize::new(0);
    assert!(scan_ordered(&[1, 2], 2, &|| true, |_| {
        calls.fetch_add(1, Ordering::SeqCst);
    })
    .is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn p54_first_valid_duplicate_wins_despite_later_io_finishing_first() {
    let root = tests::test_dir("p54-duplicates");
    let roots = [root.join("first"), root.join("second")];
    for (ordinal, directory) in roots.iter().enumerate() {
        for id in ["same", "invalid-first"] {
            let path = directory.join(id);
            fs::create_dir_all(&path).unwrap();
            let json = if ordinal == 0 && id == "invalid-first" {
                r#"{"title":"not a wallpaper","category":"asset"}"#.to_string()
            } else {
                format!(r#"{{"title":"{ordinal}-{id}","tags":["owned"]}}"#)
            };
            fs::write(path.join("project.json"), json).unwrap();
        }
    }
    let later_done = (Mutex::new(false), Condvar::new());
    let completion = Mutex::new(Vec::new());
    let parse = |path: &Path| {
        if path == roots[0].join("same/project.json") {
            wait(&later_done);
        }
        let project = read_project(path);
        completion.lock().unwrap().push(path.to_path_buf());
        if path == roots[1].join("same/project.json") {
            release(&later_done);
        }
        project
    };
    let items = scan_project_roots(&roots, 2, &|| false, &parse);
    assert_eq!(items.len(), 2);
    assert_eq!(
        items.iter().find(|i| i.id == "same").unwrap().folder_path,
        roots[0].join("same")
    );
    assert_eq!(
        items
            .iter()
            .find(|i| i.id == "invalid-first")
            .unwrap()
            .folder_path,
        roots[1].join("invalid-first")
    );
    let completion = completion.into_inner().unwrap();
    assert!(
        completion
            .iter()
            .position(|p| p == &roots[1].join("same/project.json"))
            .unwrap()
            < completion
                .iter()
                .position(|p| p == &roots[0].join("same/project.json"))
                .unwrap()
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn p54_recursive_overlaps_and_parallel_headers_preserve_exact_pairs_and_metadata() {
    let root = tests::test_dir("p54-pairs");
    let nested = root.join("nested");
    fs::create_dir_all(&nested).unwrap();
    for n in 0..16 {
        image::RgbaImage::new(n + 2, n + 3)
            .save(nested.join(format!("owned-{:02}.png", 15 - n)))
            .unwrap();
    }
    fs::write(root.join("owned.mp4"), b"owned-video-no-decode").unwrap();
    let roots = [root.clone(), nested, root.clone()];
    let serial = scan_local_media_until(&roots, true, 1, &|| false);
    let parallel = scan_local_media_until(&roots, true, 2, &|| false);
    assert_eq!(serial, parallel);
    assert_eq!(parallel.len(), 17);
    let gate = (Mutex::new(false), Condvar::new());
    let first = &parallel[0].id;
    let entries = scan_ordered(&parallel, 2, &|| false, |item| {
        if &item.id == first {
            wait(&gate);
        }
        let entry = crate::library::CatalogEntry::from_wallpaper(item);
        if &item.id != first {
            release(&gate);
        }
        entry
    });
    assert_eq!(
        entries,
        serial
            .iter()
            .map(crate::library::CatalogEntry::from_wallpaper)
            .collect::<Vec<_>>()
    );
    for (item, entry) in parallel.iter().zip(&entries) {
        assert_eq!(item.id, entry.id);
        assert!(item.scan_metadata.as_ref().unwrap().tags.is_empty());
        if item.source == WallpaperSource::LocalImage {
            assert_eq!(entry.height, entry.width + 1);
            let stamp = item.scan_metadata.as_ref().unwrap();
            assert_eq!(stamp.project, stamp.target);
            assert_eq!(stamp.target, stamp.preview);
        } else {
            assert_eq!(item.source, WallpaperSource::LocalVideo);
        }
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn p54_streamed_project_keeps_large_metadata_compatibility_and_rejects_trailing_json() {
    let root = tests::test_dir("p54-stream");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("project.json");
    let title = "夜🌸".repeat(512 * 1024);
    fs::write(
        &path,
        serde_json::json!({"title":title,"ignored":"x".repeat(2 * 1024 * 1024)}).to_string(),
    )
    .unwrap();
    assert_eq!(read_project(&path).unwrap().title.unwrap(), title);
    fs::write(&path, r#"{"title":"owned"} {"title":"trailing"}"#).unwrap();
    assert!(read_project(&path).is_none());
    let mut invalid = br#"{"title":"owned","ignored":""#.to_vec();
    invalid.push(0xff);
    invalid.extend_from_slice(br#""}"#);
    assert!(
        serde_json::from_str::<ProjectJson>(&String::from_utf8_lossy(&invalid)).is_ok(),
        "fixture must otherwise be valid JSON"
    );
    fs::write(&path, invalid).unwrap();
    assert!(
        read_project(&path).is_none(),
        "ignored fields must still be valid UTF-8"
    );
    let mut split = vec![b' '; 8190];
    split.extend_from_slice("🌸".as_bytes());
    let mut reader = Utf8Reader {
        reader: std::io::Cursor::new(&split),
        tail: Vec::new(),
    };
    let mut output = Vec::new();
    std::io::Read::read_to_end(&mut reader, &mut output).unwrap();
    assert_eq!(
        output, split,
        "valid multibyte boundaries survive streaming"
    );
    split.pop();
    let mut reader = Utf8Reader {
        reader: std::io::Cursor::new(&split),
        tail: Vec::new(),
    };
    assert!(
        std::io::Read::read_to_end(&mut reader, &mut Vec::new()).is_err(),
        "truncated UTF-8 is rejected at EOF"
    );
    fs::remove_dir_all(root).unwrap();
}

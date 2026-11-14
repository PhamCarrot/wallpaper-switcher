use super::*;
use crate::library::{query_catalog, CatalogEntry, SortOrder};

fn metric(hue: u16, saturation: u16, richness: u16) -> SortMetrics {
    SortMetrics {
        version: VERSION,
        hue,
        saturation,
        richness,
    }
}
#[test]
fn pk01_reference_buckets_gray_threshold_alpha_and_equal_bucket_winner() {
    for (h, b) in [
        (0, 0),
        (24, 0),
        (25, 1),
        (54, 1),
        (55, 2),
        (294, 9),
        (295, 10),
        (319, 10),
        (320, 11),
        (354, 11),
        (355, 0),
        (359, 0),
    ] {
        assert_eq!(bucket(h), b);
    }
    assert_eq!(pixel_bucket([0, 0, 0]), None);
    assert_eq!(pixel_bucket([255, 255, 255]), None);
    assert_eq!(pixel_bucket([128, 128, 128]), Some(12));
    let mut image = image::RgbaImage::from_pixel(100, 1, image::Rgba([128, 128, 128, 255]));
    for x in 0..4 {
        image.put_pixel(x, 0, image::Rgba([255, 0, 0, 0]));
    }
    assert_eq!(
        extract(&DynamicImage::ImageRgba8(image.clone())),
        metric(99, 0, 100)
    );
    image.put_pixel(4, 0, image::Rgba([255, 0, 0, 0]));
    assert_eq!(
        extract(&DynamicImage::ImageRgba8(image)),
        metric(0, 10, 100)
    );
    let image = image::RgbaImage::from_fn(100, 1, |x, _| {
        if x < 50 {
            image::Rgba([255, 0, 0, 0])
        } else {
            image::Rgba([0, 0, 255, 255])
        }
    });
    assert_eq!(
        extract(&DynamicImage::ImageRgba8(image)),
        metric(0, 50, 200)
    );
}
#[test]
fn pk01_analysis_uses_reference_center_crop_and_preshrink_without_changing_image() {
    // A tall input has a blue middle and red ends: whole-image analysis would choose red.
    let image = DynamicImage::ImageRgba8(image::RgbaImage::from_fn(1280, 4000, |_, y| {
        if (1500..2500).contains(&y) {
            image::Rgba([0, 0, 255, 255])
        } else {
            image::Rgba([255, 0, 0, 255])
        }
    }));
    let before = image.clone();
    assert_eq!(analyse(&image), metric(8, 100, 100));
    assert_eq!(image, before);
    for (rgb, expected) in [
        ([255, 0, 0], metric(0, 100, 100)),
        ([0, 255, 0], metric(4, 100, 100)),
        ([0, 0, 255], metric(8, 100, 100)),
        ([128, 128, 128], metric(99, 0, 0)),
    ] {
        assert_eq!(
            analyse(&DynamicImage::ImageRgb8(image::RgbImage::from_pixel(
                2000,
                1200,
                image::Rgb(rgb)
            ))),
            expected
        );
    }
}
#[test]
fn pk01_pop_gray_beats_diversity_and_reference_ties_use_coverage() {
    let gray = metric(99, 0, 0);
    let spectrum = metric(0, 17, 600);
    assert_eq!(
        compare(Some(&gray), Some(&spectrum), SortOrder::ColourPop),
        Ordering::Less
    );
    let strong = metric(0, 90, 200);
    let weak = metric(0, 30, 200);
    for order in [
        SortOrder::Colour,
        SortOrder::Richness,
        SortOrder::Minimalist,
    ] {
        assert_eq!(compare(Some(&strong), Some(&weak), order), Ordering::Less);
        assert_eq!(compare(Some(&strong), None, order), Ordering::Less);
    }
    assert_eq!(
        compare(Some(&metric(99, 0, 0)), Some(&strong), SortOrder::Colour),
        Ordering::Greater
    );
    assert_eq!(
        compare(Some(&metric(44, 20, 100)), None, SortOrder::Colour),
        Ordering::Equal
    );
}
fn entry(id: &str, time: u64, width: u32, height: u32) -> CatalogEntry {
    CatalogEntry {
        id: id.into(),
        title: id.into(),
        kind: "image".into(),
        folder: "J:/owned".into(),
        width,
        height,
        modified_unix: time,
        source_tags: Default::default(),
        tags: Default::default(),
    }
}
#[test]
fn pk01_query_timestamp_history_resolution_and_deterministic_unknown_ties() {
    let catalog = [
        entry("b", 20, 100, 20),
        entry("a", 20, 200, 10),
        entry("z", 0, 0, 0),
        entry("c", 30, 100, 20),
    ];
    let mut usage = crate::candidate_state::local_library::UsageHistory::default();
    let ids = |order| {
        query_catalog(
            &catalog,
            &Default::default(),
            &Default::default(),
            order,
            &Default::default(),
            &Default::default(),
            &Default::default(),
            &usage,
        )
        .iter()
        .map(|e| e.id.as_str())
        .collect::<Vec<_>>()
    };
    assert_eq!(ids(SortOrder::Newest), ["c", "a", "b", "z"]);
    assert_eq!(ids(SortOrder::Recent), ["c", "a", "b", "z"]);
    assert_eq!(ids(SortOrder::ResolutionDescending), ["a", "c", "b", "z"]);
    assert_eq!(ids(SortOrder::Colour), ["a", "b", "c", "z"]);
    usage.items.insert(
        "b".into(),
        crate::candidate_state::local_library::ItemUsage {
            confirmed_count: 1,
            last_confirmed_unix: 4,
            retained: Default::default(),
        },
    );
    usage.items.insert(
        "a".into(),
        crate::candidate_state::local_library::ItemUsage {
            confirmed_count: 99,
            last_confirmed_unix: 4,
            retained: Default::default(),
        },
    );
    let result = query_catalog(
        &catalog,
        &Default::default(),
        &Default::default(),
        SortOrder::Recent,
        &Default::default(),
        &Default::default(),
        &Default::default(),
        &usage,
    );
    assert_eq!(
        result.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["a", "b", "c", "z"]
    );
    assert_eq!(SortOrder::ALL.len(), 7);
    assert_eq!(SortOrder::default(), SortOrder::Colour);
}
#[test]
fn pk01_source_mtime_still_video_engine_project_missing_and_whole_seconds() {
    use std::{
        fs,
        time::{Duration, SystemTime},
    };
    let root = std::env::temp_dir().join(format!("pk01-source-times-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let path = root.join("image.png");
    image::RgbImage::new(2, 2).save(&path).unwrap();
    let mut item = crate::scanner::local_image_item(&path).unwrap();
    let time = SystemTime::UNIX_EPOCH + Duration::from_millis(123456);
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(time)
        .unwrap();
    assert_eq!(super::super::source_modified_unix(&item), 123);
    item.source = crate::scanner::WallpaperSource::LocalVideo;
    item.wallpaper_type = "video".into();
    assert_eq!(super::super::source_modified_unix(&item), 123);
    item.source = crate::scanner::WallpaperSource::WallpaperEngine;
    item.project_json_path = root.join("project.json");
    fs::write(&item.project_json_path, b"{}").unwrap();
    fs::File::options()
        .write(true)
        .open(&item.project_json_path)
        .unwrap()
        .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(500))
        .unwrap();
    assert_eq!(super::super::source_modified_unix(&item), 500);
    fs::remove_file(&item.project_json_path).unwrap();
    assert_eq!(super::super::source_modified_unix(&item), 0);
}

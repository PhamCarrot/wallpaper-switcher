use super::*;
use crate::library::{query_catalog, CatalogEntry, SortOrder};
use std::collections::BTreeSet;

fn metric(rgb: [u8; 3]) -> ColorMetadata {
    analyse_sample(
        &image::RgbaImage::from_pixel(8, 8, image::Rgba([rgb[0], rgb[1], rgb[2], 255])),
        "A".repeat(64),
    )
    .unwrap()
}
#[test]
fn w5_metrics_primary_grey_transparent_weighted_and_richness() {
    for (rgb, hue, luma) in [
        ([255, 0, 0], Some(0.0), 0.2126),
        ([0, 255, 0], Some(120.0), 0.7152),
        ([0, 0, 255], Some(240.0), 0.0722),
        ([128, 128, 128], None, 0.21586),
        ([255; 3], None, 1.0),
        ([0; 3], None, 0.0),
    ] {
        let m = metric(rgb);
        assert_eq!(m.dominant_rgb, rgb);
        assert_eq!(m.hue, hue);
        assert!((m.brightness - luma).abs() < 0.0001);
        assert!(m.valid());
    }
    assert!(analyse_sample(&image::RgbaImage::new(8, 8), "A".repeat(64))
        .unwrap_err()
        .contains("Transparent-only"));
    let mut weighted = image::RgbaImage::new(3, 1);
    weighted.put_pixel(0, 0, image::Rgba([255, 0, 0, 255]));
    weighted.put_pixel(1, 0, image::Rgba([0, 255, 0, 64]));
    weighted.put_pixel(2, 0, image::Rgba([0, 0, 255, 0]));
    let m = analyse_sample(&weighted, "A".repeat(64)).unwrap();
    assert_eq!(m.dominant_rgb, [255, 0, 0]);
    assert!((m.brightness - (0.2126 * 255.0 + 0.7152 * 64.0) / 319.0).abs() < 0.0001);
    assert_eq!(m.richness, 1.0);
    assert_eq!(occupied_bin_richness(1, 16384), 1.0 / 16384.0);
    assert_eq!(occupied_bin_richness(32768, 40000), 1.0);
    assert_eq!(metric([10; 3]).saturation, 0.0);
}
#[test]
fn e1_bucket_query_unknown_tags_and_metric_sort_composition() {
    let red = metric([255, 0, 0]);
    let blue = metric([0, 0, 255]);
    let q = ColorQuery {
        bucket: Some(ColorBucket::Red),
    };
    assert!(q.matches(Some(&red)));
    assert!(!q.matches(Some(&blue)));
    assert!(!q.matches(Some(&metric([128; 3]))));
    assert!(!q.matches(None));
    let entry = |id: &str| CatalogEntry {
        id: id.into(),
        title: "same".into(),
        kind: "image".into(),
        folder: std::path::PathBuf::from(r"C:\owned"),
        width: 1920,
        height: 1080,
        modified_unix: 0,
        source_tags: BTreeSet::from(["local".into()]),
        tags: BTreeSet::from(["local".into()]),
    };
    let entries = vec![entry("z"), entry("b"), entry("a"), entry("unknown")];
    let mut map = MetadataView::from([
        ("z".into(), blue),
        ("a".into(), red.clone()),
        ("b".into(), red),
    ]);
    let mut sorting = super::super::sort_metrics::MetadataView::from([
        (
            "a".into(),
            super::super::sort_metrics::SortMetrics {
                version: 1,
                hue: 0,
                saturation: 100,
                richness: 100,
            },
        ),
        (
            "b".into(),
            super::super::sort_metrics::SortMetrics {
                version: 1,
                hue: 0,
                saturation: 100,
                richness: 100,
            },
        ),
        (
            "z".into(),
            super::super::sort_metrics::SortMetrics {
                version: 1,
                hue: 8,
                saturation: 100,
                richness: 100,
            },
        ),
    ]);
    let mut rule = crate::candidate_state::SmartRule::default();
    rule.tags.insert("local".into());
    rule.text = "same".into();
    rule.minimum_width = 1920;
    rule.minimum_height = 1080;
    rule.kinds.insert("image".into());
    rule.folders.insert(r"C:\owned".into());
    rule.favorites_only = true;
    let favorites = BTreeSet::from(["z".into(), "a".into(), "b".into(), "unknown".into()]);
    let ids = |q: &ColorQuery, order| {
        query_catalog(
            &entries,
            &rule,
            &favorites,
            order,
            q,
            &map,
            &sorting,
            &Default::default(),
        )
        .iter()
        .map(|e| e.id.clone())
        .collect::<Vec<_>>()
    };
    assert_eq!(ids(&q, SortOrder::Colour), ["a", "b"]);
    assert_eq!(
        ids(&ColorQuery::default(), SortOrder::Colour),
        ["a", "b", "z", "unknown"]
    );
    let mut rich = map["z"].clone();
    rich.richness = 0.8;
    sorting.get_mut("z").unwrap().richness = 800;
    map.insert("z".into(), rich);
    let ids = |order| {
        query_catalog(
            &entries,
            &rule,
            &favorites,
            order,
            &ColorQuery::default(),
            &map,
            &sorting,
            &Default::default(),
        )
        .iter()
        .map(|e| e.id.clone())
        .collect::<Vec<_>>()
    };
    assert_eq!(ids(SortOrder::Richness), ["z", "a", "b", "unknown"]);
    assert_eq!(ids(SortOrder::Minimalist), ["a", "b", "z", "unknown"]);
    for order in [
        SortOrder::Newest,
        SortOrder::Recent,
        SortOrder::ResolutionDescending,
    ] {
        assert_eq!(ids(order), ["a", "b", "unknown", "z"]);
    }
    rule.excluded_tags.insert("local".into());
    assert!(query_catalog(
        &entries,
        &rule,
        &favorites,
        SortOrder::Colour,
        &q,
        &map,
        &sorting,
        &Default::default()
    )
    .is_empty());
}
#[test]
fn e1_all_bucket_boundaries_wrap_gray_unknown_and_checked_toggle() {
    let mut m = metric([255, 0, 0]);
    for i in 0..12 {
        let bucket = ColorBucket::from_index(i).unwrap();
        for hue in [
            (i as f32 * 30.0 - 15.0).rem_euclid(360.0),
            i as f32 * 30.0,
            (i as f32 * 30.0 + 14.99).rem_euclid(360.0),
        ] {
            m.hue = Some(hue);
            assert_eq!(ColorBucket::from_metadata(&m), Some(bucket), "hue={hue}");
            for other in ColorBucket::ALL {
                assert_eq!(
                    ColorQuery {
                        bucket: Some(other)
                    }
                    .matches(Some(&m)),
                    other == bucket
                );
            }
        }
        m.hue = Some((i as f32 * 30.0 + 15.0).rem_euclid(360.0));
        assert_eq!(
            ColorBucket::from_metadata(&m),
            ColorBucket::from_index((i + 1) % 12)
        );
        let boundary = m.hue.unwrap();
        m.hue = Some(f32::from_bits(boundary.to_bits() - 1));
        assert_eq!(ColorBucket::from_metadata(&m), Some(bucket));
    }
    m.hue = Some(359.99);
    assert_eq!(ColorBucket::from_metadata(&m), Some(ColorBucket::Red));
    m.hue = None;
    assert_eq!(ColorBucket::from_metadata(&m), Some(ColorBucket::Gray));
    assert!(ColorQuery {
        bucket: Some(ColorBucket::Gray)
    }
    .matches(Some(&m)));
    for hue in [Some(f32::NAN), Some(f32::INFINITY), Some(-1.0), Some(360.0)] {
        m.hue = hue;
        assert_eq!(ColorBucket::from_metadata(&m), None);
        for bucket in ColorBucket::ALL {
            assert!(!ColorQuery {
                bucket: Some(bucket)
            }
            .matches(Some(&m)));
        }
        assert!(ColorQuery::default().matches(Some(&m)));
    }
    m.hue = None;
    m.fingerprint.clear();
    assert_eq!(ColorBucket::from_metadata(&m), None);
    assert!(ColorQuery::default().matches(None));
    let mut query = ColorQuery::default();
    for (index, bucket) in ColorBucket::ALL.into_iter().enumerate() {
        assert_eq!(ColorBucket::from_index(index), Some(bucket));
        query.toggle(bucket);
        assert_eq!(query.bucket, Some(bucket));
        assert!(!query.matches(None));
        query.toggle(bucket);
        assert!(!query.explicit());
        assert!(query.matches(None));
    }
    assert_eq!(ColorBucket::from_index(13), None);
    assert_eq!(ColorBucket::from_index(usize::MAX), None);
}
#[test]
fn retirement_shared_sha256_preserves_exact_uppercase_fingerprints() {
    assert_eq!(
        hash_bytes(b""),
        "E3B0C44298FC1C149AFBF4C8996FB92427AE41E4649B934CA495991B7852B855"
    );
    assert_eq!(
        hash_bytes(b"abc"),
        "BA7816BF8F01CFEA414140DE5DAE2223B00361A396177A9CB410FF61F20015AD"
    );
}

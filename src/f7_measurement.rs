//! Opt-in F7 CPU layout measurement harness.
//!
//! This module is compiled only for the unit-test binary and is ignored unless
//! the caller explicitly selects `f7_cpu_layout_measurement` with `--ignored`.
//! It calls the same pure layout/aspect/poster-demand functions used by the
//! application view and poster scheduler. It deliberately owns no application,
//! provider, window, texture, or cache state.

use crate::candidate_state::{
    CandidateState, GeometricShape as StateGeometricShape, WallLayout as StateWallLayout,
};
use crate::donor_layout::{self, GridLayout, GridParams, HexCurve, HexParams, HexShape};
use crate::gpu_gallery::{
    self, GalleryLayoutParams, GalleryMode, GeometricShape, SliceCard, WallLayout,
};
use crate::library::{self, CatalogEntry};
use crate::upstream_slices_adapter::SliceLayoutState;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

const VIEWPORT: [f32; 2] = [1920.0, 1080.0];
const WARMUP_ITERATIONS: usize = 12;
const MEASURED_SAMPLES: usize = 31;
const DEFAULT_POSTER_ASPECT: f32 = 16.0 / 9.0;

#[derive(Clone)]
struct CatalogInput {
    label: String,
    source: String,
    catalog: Vec<CatalogEntry>,
    filtered_indices: Vec<usize>,
}

#[derive(Clone, Copy)]
struct LayoutCase {
    label: &'static str,
    mode: GalleryMode,
    wall_layout: Option<WallLayout>,
    geometric_shape: Option<GeometricShape>,
}

#[derive(Debug)]
enum PlacementOutput {
    Slices(Option<SliceLayoutState>),
    Geometric(Vec<donor_layout::HexPlacement>),
    Wall(Vec<donor_layout::GridPlacement>),
    Depth(Vec<SliceCard>),
}

#[derive(Debug)]
struct DrawBoundaryOutput {
    aspects: Vec<f32>,
    cards: Vec<SliceCard>,
}

#[derive(Debug)]
struct PosterBoundaryOutput {
    aspects: Vec<f32>,
    requested: Vec<usize>,
}

#[derive(Debug)]
struct CombinedBoundaryOutput {
    draw: DrawBoundaryOutput,
    poster: PosterBoundaryOutput,
}

fn mix(mut hash: u64, value: u64) -> u64 {
    hash ^= value.wrapping_add(0x9e37_79b9_7f4a_7c15);
    hash = hash.rotate_left(13).wrapping_mul(0xff51_afd7_ed55_8ccd);
    hash ^ (hash >> 29)
}

fn mix_float(hash: u64, value: f32) -> u64 {
    mix(hash, u64::from(value.to_bits()))
}

fn digest_floats(values: &[f32]) -> u64 {
    values
        .iter()
        .fold(mix(0, values.len() as u64), |hash, value| {
            mix_float(hash, *value)
        })
}

fn digest_cards(cards: &[SliceCard]) -> u64 {
    cards.iter().fold(mix(0, cards.len() as u64), |hash, card| {
        let hash = mix(hash, card.index as u64);
        let hash = mix(hash, u64::from(card.selected));
        let hash = mix(hash, card.layer.map_or(u64::MAX, u64::from));
        let hash = card
            .center
            .into_iter()
            .chain(card.size)
            .chain([
                card.opacity,
                card.skew_distance,
                card.edge_tilt_distance,
                card.rotation,
                card.corner_radius,
                card.poster_aspect_ratio,
            ])
            .fold(hash, mix_float);
        mix(hash, u64::from(card.shape))
    })
}

fn digest_grid_placements(placements: &[donor_layout::GridPlacement]) -> u64 {
    placements
        .iter()
        .fold(mix(0, placements.len() as u64), |hash, placement| {
            let hash = mix(hash, placement.index as u64);
            [placement.x, placement.y, placement.width, placement.height]
                .into_iter()
                .fold(hash, mix_float)
        })
}

fn digest_hex_placements(placements: &[donor_layout::HexPlacement]) -> u64 {
    placements
        .iter()
        .fold(mix(0, placements.len() as u64), |hash, placement| {
            let hash = mix(hash, placement.index as u64);
            let hash = mix(hash, u64::from(placement.triangle_direction));
            [
                placement.x,
                placement.y,
                placement.width,
                placement.height,
                placement.scale,
                placement.opacity,
            ]
            .into_iter()
            .fold(hash, mix_float)
        })
}

fn digest_slice_layout(layout: &Option<SliceLayoutState>) -> u64 {
    let Some(layout) = layout else {
        return mix(0, 0);
    };
    let hash = mix(0, layout.start as u64);
    let hash = mix(hash, layout.end as u64);
    let hash = digest_floats(&layout.centers).wrapping_add(hash);
    [
        layout.reference_center_x,
        layout.center_x,
        layout.center_y,
        layout.stride,
        layout.expanded_layout_width,
        layout.half_view,
    ]
    .into_iter()
    .fold(hash, mix_float)
}

fn digest_placement(output: &PlacementOutput) -> u64 {
    match output {
        PlacementOutput::Slices(layout) => digest_slice_layout(layout),
        PlacementOutput::Geometric(placements) => digest_hex_placements(placements),
        PlacementOutput::Wall(placements) => digest_grid_placements(placements),
        PlacementOutput::Depth(cards) => digest_cards(cards),
    }
}

fn digest_draw(output: &DrawBoundaryOutput) -> u64 {
    mix(digest_floats(&output.aspects), digest_cards(&output.cards))
}

fn digest_poster(output: &PosterBoundaryOutput) -> u64 {
    output
        .requested
        .iter()
        .fold(digest_floats(&output.aspects), |hash, index| {
            mix(hash, *index as u64)
        })
}

fn digest_combined(output: &CombinedBoundaryOutput) -> u64 {
    mix(digest_draw(&output.draw), digest_poster(&output.poster))
}

fn timed<T, F, D>(label: &str, mut operation: F, digest: D) -> Value
where
    F: FnMut(usize) -> T,
    D: Fn(&T) -> u64,
{
    let cold_start = Instant::now();
    let cold_output = black_box(operation(0));
    let cold_ns = cold_start.elapsed().as_nanos() as u64;
    let cold_digest = digest(&cold_output);
    let mut sink = cold_digest;

    for iteration in 0..WARMUP_ITERATIONS {
        let output = black_box(operation(iteration + 1));
        sink = mix(sink, digest(&output));
    }

    let mut samples = Vec::with_capacity(MEASURED_SAMPLES);
    for iteration in 0..MEASURED_SAMPLES {
        let start = Instant::now();
        let output = black_box(operation(iteration + WARMUP_ITERATIONS + 1));
        let elapsed_ns = start.elapsed().as_nanos() as u64;
        sink = mix(sink, digest(&output));
        samples.push(elapsed_ns);
    }
    black_box(sink);

    samples.sort_unstable();
    let sum = samples.iter().map(|sample| *sample as f64).sum::<f64>();
    let mean = sum / samples.len() as f64;
    let variance = samples
        .iter()
        .map(|sample| {
            let delta = *sample as f64 - mean;
            delta * delta
        })
        .sum::<f64>()
        / samples.len() as f64;
    let p95_index = ((samples.len() as f64 * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(samples.len() - 1);

    json!({
        "operation": label,
        "cold_ns": cold_ns,
        "warmup_iterations": WARMUP_ITERATIONS,
        "samples": samples.len(),
        "warm_min_ns": samples[0],
        "warm_median_ns": samples[samples.len() / 2],
        "warm_p95_ns": samples[p95_index],
        "warm_max_ns": samples[samples.len() - 1],
        "warm_mean_ns": mean,
        "warm_stddev_ns": variance.sqrt(),
        "output_digest": cold_digest,
        "sink": sink,
    })
}

fn synthetic_catalog(count: usize) -> Vec<CatalogEntry> {
    const DIMENSIONS: [(u32, u32); 8] = [
        (1920, 1080),
        (2560, 1440),
        (1080, 1920),
        (1440, 2560),
        (1200, 1200),
        (1024, 768),
        (768, 1024),
        (3440, 1440),
    ];
    (0..count)
        .map(|index| {
            let (width, height) = DIMENSIONS[index % DIMENSIONS.len()];
            CatalogEntry {
                id: format!("synthetic-{index}"),
                title: format!("Synthetic {index}"),
                kind: "image".into(),
                folder: PathBuf::from("synthetic-catalog"),
                width,
                height,
                modified_unix: 0,
                source_tags: Default::default(),
                tags: Default::default(),
            }
        })
        .collect()
}

fn input(
    label: impl Into<String>,
    source: impl Into<String>,
    catalog: Vec<CatalogEntry>,
) -> CatalogInput {
    let filtered_indices = (0..catalog.len()).collect();
    CatalogInput {
        label: label.into(),
        source: source.into(),
        catalog,
        filtered_indices,
    }
}

fn real_catalog_input() -> CatalogInput {
    let wallpapers =
        std::panic::catch_unwind(|| crate::scanner::scan_catalog(&[], false)).unwrap_or_default();
    let catalog = wallpapers
        .iter()
        .map(library::CatalogEntry::from_wallpaper)
        .collect();
    input(
        "real-catalog",
        "scanner::scan_catalog(local_roots=[], include_default_projects=false)",
        catalog,
    )
}

fn aspect_distribution(catalog: &[CatalogEntry], indices: &[usize]) -> Value {
    let mut counts = BTreeMap::<&'static str, usize>::new();
    let mut bins = BTreeMap::<&'static str, usize>::new();
    for index in indices {
        let Some(entry) = catalog.get(*index) else {
            *counts.entry("invalid-index").or_default() += 1;
            continue;
        };
        let Some((width, height)) =
            (entry.width > 0 && entry.height > 0).then_some((entry.width, entry.height))
        else {
            *counts.entry("fallback-16:9").or_default() += 1;
            *bins.entry("fallback").or_default() += 1;
            continue;
        };
        let key = match width.cmp(&height) {
            std::cmp::Ordering::Less => "portrait",
            std::cmp::Ordering::Equal => "square",
            std::cmp::Ordering::Greater => "landscape",
        };
        *counts.entry(key).or_default() += 1;
        let aspect = width as f32 / height as f32;
        let bin = if aspect < 0.9 {
            "portrait-narrow"
        } else if aspect <= 1.1 {
            "near-square"
        } else if aspect <= 2.0 {
            "landscape-standard"
        } else {
            "landscape-wide"
        };
        *bins.entry(bin).or_default() += 1;
    }
    json!({
        "eligible_count": indices.len(),
        "orientation_counts": counts,
        "aspect_bins": bins,
        "default_aspect_for_missing_dimensions": DEFAULT_POSTER_ASPECT,
    })
}

fn params_for(case: LayoutCase, motion: f32, scroll_offset: f32) -> GalleryLayoutParams {
    let mut state = CandidateState::default();
    if let Some(layout) = case.wall_layout {
        state.gallery.wall.layout = match layout {
            WallLayout::Uniform => StateWallLayout::Uniform,
            WallLayout::Brick => StateWallLayout::Brick,
            WallLayout::Masonry => StateWallLayout::Masonry,
            WallLayout::Justified => StateWallLayout::Justified,
            WallLayout::Editorial => StateWallLayout::Editorial,
            WallLayout::Cylinder => StateWallLayout::Cylinder,
        };
    }
    if let Some(shape) = case.geometric_shape {
        state.gallery.geometric.shape = match shape {
            GeometricShape::Hexagon => StateGeometricShape::Hexagon,
            GeometricShape::Circle => StateGeometricShape::Circle,
            GeometricShape::Triangle => StateGeometricShape::Triangle,
            GeometricShape::Diamond => StateGeometricShape::Diamond,
            GeometricShape::Rhombus => StateGeometricShape::Rhombus,
        };
    }
    super::gallery_layout_params(&state, motion, scroll_offset)
}

fn wall_grid_params(params: GalleryLayoutParams) -> GridParams {
    let wall = params.wall;
    GridParams {
        cols: wall.columns.max(1),
        rows: wall.rows.max(1),
        thumb_w: wall.card_width.max(1.0),
        thumb_h: wall.card_height.max(1.0),
        gap_x: wall.gap_x,
        gap_y: wall.gap_y,
        corner_radius: wall.corner_radius.max(0.0),
        border_width: 1.0,
        layout: match wall.layout {
            WallLayout::Uniform => GridLayout::Uniform,
            WallLayout::Brick => GridLayout::Brick,
            WallLayout::Masonry => GridLayout::Masonry,
            WallLayout::Justified => GridLayout::Justified,
            WallLayout::Editorial => GridLayout::Editorial,
            WallLayout::Cylinder => GridLayout::Cylinder,
        },
        stagger: wall.stagger,
        selected_scale: wall.selected_scale,
        flow_wave: wall.flow_wave,
        flow_frequency: wall.flow_frequency,
        scatter: wall.scatter,
        scale_variance: wall.scale_variance,
        cylinder_bend: wall.cylinder_bend,
        cylinder_radius: wall.cylinder_radius,
        stage: wall.stage,
    }
}

fn hex_params(params: GalleryLayoutParams) -> Option<HexParams> {
    let geometric = params.geometric;
    let shape = match geometric.shape {
        GeometricShape::Hexagon => HexShape::Hexagon,
        GeometricShape::Triangle => HexShape::Triangle,
        GeometricShape::Diamond => HexShape::Diamond,
        GeometricShape::Rhombus => HexShape::Rhombus,
        GeometricShape::Circle => return None,
    };
    let curve = match geometric.curve {
        crate::gpu_gallery::GeometricCurve::Flat => HexCurve::Flat,
        crate::gpu_gallery::GeometricCurve::Arc => HexCurve::Arc,
        crate::gpu_gallery::GeometricCurve::Wave => HexCurve::Wave,
        crate::gpu_gallery::GeometricCurve::S => HexCurve::S,
        crate::gpu_gallery::GeometricCurve::Cylinder => HexCurve::Cylinder,
    };
    let orbit_radius = if geometric.orbit_radius.is_finite() && geometric.orbit_radius > 0.0 {
        geometric.orbit_radius
    } else {
        VIEWPORT[0].min(VIEWPORT[1]).max(1.0) * 0.55
    };
    Some(HexParams {
        r: geometric.radius.max(donor_layout::MIN_HEX_R),
        rows: geometric.rows.max(1),
        cols: geometric.columns.max(1),
        scroll_step: 1,
        curve,
        shape,
        curve_strength: geometric.curve_strength,
        curve_frequency: geometric.curve_frequency,
        gap_x: geometric.gap_x,
        gap_y: geometric.gap_y,
        aspect: geometric.aspect.max(0.05),
        stagger: geometric.stagger,
        lens: geometric.lens,
        lens_radius: orbit_radius,
        orbit: geometric.orbit,
        orbit_radius,
        twist: geometric.twist,
        scatter: geometric.scatter,
        stage: geometric.stage,
    })
}

fn placement_output(
    case: LayoutCase,
    count: usize,
    selected_position: usize,
    params: GalleryLayoutParams,
    aspects: &[f32],
) -> PlacementOutput {
    match case.mode {
        GalleryMode::Depth => PlacementOutput::Depth(gpu_gallery::layout_gallery_with_aspects(
            GalleryMode::Depth,
            count,
            selected_position,
            VIEWPORT,
            params,
            aspects,
        )),
        GalleryMode::Slices => {
            let slices = params.slices;
            PlacementOutput::Slices(crate::upstream_slices_adapter::build_slice_layout(
                count,
                selected_position,
                VIEWPORT,
                slices.offset_x,
                slices.offset_y,
                slices.width,
                slices.expanded_width,
                slices.spacing,
                slices.skew_distance,
                slices.visible_count,
                slices.motion,
            ))
        }
        GalleryMode::Geometric => {
            let Some(hex) = hex_params(params) else {
                return PlacementOutput::Geometric(Vec::new());
            };
            PlacementOutput::Geometric(donor_layout::hex_placements(
                count,
                selected_position,
                VIEWPORT,
                &hex,
                params.geometric.motion,
            ))
        }
        GalleryMode::Wall => PlacementOutput::Wall(donor_layout::grid_placements(
            count,
            &wall_grid_params(params),
            aspects,
        )),
    }
}

fn draw_boundary(
    input: &CatalogInput,
    case: LayoutCase,
    selected_position: usize,
    params: GalleryLayoutParams,
) -> DrawBoundaryOutput {
    let aspects = super::filtered_poster_aspects(&input.catalog, &input.filtered_indices);
    let cards = gpu_gallery::layout_gallery_with_aspects(
        case.mode,
        input.filtered_indices.len(),
        selected_position,
        VIEWPORT,
        params,
        &aspects,
    );
    DrawBoundaryOutput { aspects, cards }
}

fn poster_boundary(
    input: &CatalogInput,
    case: LayoutCase,
    selected_master: usize,
    params: GalleryLayoutParams,
) -> PosterBoundaryOutput {
    let aspects = super::filtered_poster_aspects(&input.catalog, &input.filtered_indices);
    let requested = super::poster_indices_for_view(
        case.mode,
        &input.filtered_indices,
        Some(selected_master),
        VIEWPORT,
        params,
        &aspects,
    );
    PosterBoundaryOutput { aspects, requested }
}

fn combined_boundary(
    input: &CatalogInput,
    case: LayoutCase,
    selected_position: usize,
    params: GalleryLayoutParams,
) -> CombinedBoundaryOutput {
    let selected_master = input.filtered_indices[selected_position];
    CombinedBoundaryOutput {
        draw: draw_boundary(input, case, selected_position, params),
        poster: poster_boundary(input, case, selected_master, params),
    }
}

fn validate_cards(cards: &[SliceCard], count: usize) {
    let mut seen = std::collections::HashSet::new();
    assert!(cards.iter().all(|card| {
        card.index < count
            && card.center.iter().all(|value| value.is_finite())
            && card
                .size
                .iter()
                .all(|value| value.is_finite() && *value > 0.0)
            && card.opacity.is_finite()
            && card.poster_aspect_ratio.is_finite()
    }));
    assert!(cards.iter().all(|card| seen.insert(card.index)));
    assert!(cards.iter().filter(|card| card.selected).count() <= 1);
}

fn validate_demand(requested: &[usize], filtered_indices: &[usize]) {
    assert!(requested.len() <= gpu_gallery::MAX_POSTERS);
    let mut seen = std::collections::HashSet::new();
    assert!(requested
        .iter()
        .all(|index| filtered_indices.contains(index) && seen.insert(*index)));
}

fn validate_determinism(input: &CatalogInput, case: LayoutCase, params: GalleryLayoutParams) {
    let selected_position = input.filtered_indices.len() / 2;
    let aspects_a = super::filtered_poster_aspects(&input.catalog, &input.filtered_indices);
    let aspects_b = super::filtered_poster_aspects(&input.catalog, &input.filtered_indices);
    assert_eq!(aspects_a, aspects_b);
    assert_eq!(aspects_a.len(), input.filtered_indices.len());
    assert!(aspects_a
        .iter()
        .all(|aspect| aspect.is_finite() && *aspect > 0.0));

    let cards_a = gpu_gallery::layout_gallery_with_aspects(
        case.mode,
        input.filtered_indices.len(),
        selected_position,
        VIEWPORT,
        params,
        &aspects_a,
    );
    let cards_b = gpu_gallery::layout_gallery_with_aspects(
        case.mode,
        input.filtered_indices.len(),
        selected_position,
        VIEWPORT,
        params,
        &aspects_a,
    );
    assert_eq!(
        (cards_a.len(), digest_cards(&cards_a)),
        (cards_b.len(), digest_cards(&cards_b))
    );
    validate_cards(&cards_a, input.filtered_indices.len());

    let selected_master = input.filtered_indices[selected_position];
    let requested_a = super::poster_indices_for_view(
        case.mode,
        &input.filtered_indices,
        Some(selected_master),
        VIEWPORT,
        params,
        &aspects_a,
    );
    let requested_b = super::poster_indices_for_view(
        case.mode,
        &input.filtered_indices,
        Some(selected_master),
        VIEWPORT,
        params,
        &aspects_a,
    );
    assert_eq!(requested_a, requested_b);
    validate_demand(&requested_a, &input.filtered_indices);
}

fn context(input: &CatalogInput, case: LayoutCase, selected_position: usize, kind: &str) -> Value {
    json!({
        "kind": kind,
        "scenario": input.label,
        "source": input.source,
        "mode": case.label,
        "filtered_count": input.filtered_indices.len(),
        "selected_position": selected_position,
        "viewport": VIEWPORT,
    })
}

fn with_timing(mut context: Value, timing: Value) -> Value {
    let Value::Object(ref mut object) = context else {
        unreachable!("measurement context must be a JSON object");
    };
    if let Value::Object(timing_object) = timing {
        object.extend(timing_object);
    }
    context
}

fn measurement_cases() -> [LayoutCase; 9] {
    [
        LayoutCase {
            label: "slices",
            mode: GalleryMode::Slices,
            wall_layout: None,
            geometric_shape: None,
        },
        LayoutCase {
            label: "geometric-hexagon",
            mode: GalleryMode::Geometric,
            wall_layout: None,
            geometric_shape: Some(GeometricShape::Hexagon),
        },
        LayoutCase {
            label: "geometric-circle",
            mode: GalleryMode::Geometric,
            wall_layout: None,
            geometric_shape: Some(GeometricShape::Circle),
        },
        LayoutCase {
            label: "wall-uniform",
            mode: GalleryMode::Wall,
            wall_layout: Some(WallLayout::Uniform),
            geometric_shape: None,
        },
        LayoutCase {
            label: "wall-brick",
            mode: GalleryMode::Wall,
            wall_layout: Some(WallLayout::Brick),
            geometric_shape: None,
        },
        LayoutCase {
            label: "wall-masonry",
            mode: GalleryMode::Wall,
            wall_layout: Some(WallLayout::Masonry),
            geometric_shape: None,
        },
        LayoutCase {
            label: "wall-justified",
            mode: GalleryMode::Wall,
            wall_layout: Some(WallLayout::Justified),
            geometric_shape: None,
        },
        LayoutCase {
            label: "wall-editorial",
            mode: GalleryMode::Wall,
            wall_layout: Some(WallLayout::Editorial),
            geometric_shape: None,
        },
        LayoutCase {
            label: "wall-cylinder",
            mode: GalleryMode::Wall,
            wall_layout: Some(WallLayout::Cylinder),
            geometric_shape: None,
        },
    ]
}

fn representative_case(label: &str) -> LayoutCase {
    measurement_cases()
        .into_iter()
        .find(|case| case.label == label)
        .expect("representative measurement case must exist")
}

fn filtered_variants(input: &CatalogInput) -> Vec<Vec<usize>> {
    let all = input.filtered_indices.clone();
    let even = all.iter().copied().step_by(2).collect::<Vec<_>>();
    let first_half = all[..all.len().max(1) / 2].to_vec();
    let every_third = all.iter().copied().step_by(3).collect::<Vec<_>>();
    [all, even, first_half, every_third]
        .into_iter()
        .filter(|indices| !indices.is_empty())
        .collect()
}

fn timed_boundary_record(
    input: &CatalogInput,
    case: LayoutCase,
    selected_position: usize,
    kind: &str,
    operation: impl FnMut(usize) -> CombinedBoundaryOutput,
) -> Value {
    let timing = timed(kind, operation, digest_combined);
    with_timing(context(input, case, selected_position, kind), timing)
}

fn run_repeated_scenarios(results: &mut Vec<Value>, input: &CatalogInput) {
    let selected_positions = [
        0,
        input.filtered_indices.len() / 4,
        input.filtered_indices.len() / 2,
        input.filtered_indices.len().saturating_mul(3) / 4,
        input.filtered_indices.len().saturating_sub(1),
    ];
    let offsets = [
        (0.0_f32, 0.0_f32),
        (0.25, 80.0),
        (0.5, 220.0),
        (0.75, 420.0),
        (-0.25, 180.0),
        (0.0, 0.0),
    ];
    for label in ["slices", "geometric-hexagon", "wall-masonry"] {
        let case = representative_case(label);
        let base_selected = input.filtered_indices.len() / 2;
        let offset_params = offsets
            .iter()
            .map(|(motion, scroll)| params_for(case, *motion, *scroll))
            .collect::<Vec<_>>();
        let mut offset_step = 0usize;
        let timing = timed_boundary_record(
            input,
            case,
            base_selected,
            "combined-repeat-offsets",
            |_| {
                let params = offset_params[offset_step % offset_params.len()];
                offset_step = offset_step.wrapping_add(1);
                combined_boundary(input, case, base_selected, params)
            },
        );
        results.push(timing);

        let mut selection_step = 0usize;
        let settled_params = params_for(case, 0.0, 0.0);
        let timing = timed_boundary_record(
            input,
            case,
            base_selected,
            "combined-selection-changes",
            |_| {
                let selected = selected_positions[selection_step % selected_positions.len()];
                selection_step = selection_step.wrapping_add(1);
                combined_boundary(input, case, selected, settled_params)
            },
        );
        results.push(timing);

        let variants = filtered_variants(input)
            .into_iter()
            .map(|filtered_indices| CatalogInput {
                label: input.label.clone(),
                source: "synthetic/filter-variant-or-real-filter-snapshot".into(),
                catalog: input.catalog.clone(),
                filtered_indices,
            })
            .collect::<Vec<_>>();
        let mut filter_step = 0usize;
        let timing = timed(
            "combined-filter-changes",
            |_| {
                let filtered_input = &variants[filter_step % variants.len()];
                filter_step = filter_step.wrapping_add(1);
                let selected = filtered_input.filtered_indices.len() / 2;
                combined_boundary(filtered_input, case, selected, settled_params)
            },
            digest_combined,
        );
        results.push(with_timing(
            context(input, case, base_selected, "combined-filter-changes"),
            timing,
        ));
    }
}

#[test]
#[ignore = "opt-in F7 measurement; run only with --ignored and an isolated command"]
fn f7_cpu_layout_measurement() {
    let mut inputs = vec![real_catalog_input()];
    for count in [250, 1_000, 5_000] {
        inputs.push(input(
            format!("synthetic-{count}"),
            format!("synthetic-mixed-dimensions-{count}"),
            synthetic_catalog(count),
        ));
    }

    let cases = measurement_cases();
    let mut results = Vec::new();
    let mut deterministic_checks = 0usize;
    let mut snapshots = Vec::new();

    for input in &inputs {
        snapshots.push(json!({
            "label": input.label,
            "source": input.source,
            "catalog_count": input.catalog.len(),
            "eligible_count": input.filtered_indices.len(),
            "aspect_distribution": aspect_distribution(&input.catalog, &input.filtered_indices),
        }));
        if input.filtered_indices.is_empty() {
            continue;
        }

        let aspects_timing = timed(
            "aspect-extraction",
            |_| super::filtered_poster_aspects(&input.catalog, &input.filtered_indices),
            |aspects| digest_floats(aspects),
        );
        results.push(with_timing(
            json!({
                "kind": "aspect-extraction",
                "scenario": input.label,
                "source": input.source,
                "filtered_count": input.filtered_indices.len(),
                "viewport": VIEWPORT,
            }),
            aspects_timing,
        ));

        for case in cases {
            let params = params_for(case, 0.0, 0.0);
            validate_determinism(input, case, params);
            deterministic_checks += 1;
            let selected_position = input.filtered_indices.len() / 2;
            let aspects = super::filtered_poster_aspects(&input.catalog, &input.filtered_indices);

            let placement_timing = timed(
                "placement-generation",
                |_| {
                    placement_output(
                        case,
                        input.filtered_indices.len(),
                        selected_position,
                        params,
                        &aspects,
                    )
                },
                digest_placement,
            );
            results.push(with_timing(
                context(input, case, selected_position, "placement-generation"),
                placement_timing,
            ));

            let layout_timing = timed(
                "gallery-layout-total",
                |_| {
                    gpu_gallery::layout_gallery_with_aspects(
                        case.mode,
                        input.filtered_indices.len(),
                        selected_position,
                        VIEWPORT,
                        params,
                        &aspects,
                    )
                },
                |cards| digest_cards(cards),
            );
            results.push(with_timing(
                context(input, case, selected_position, "gallery-layout-total"),
                layout_timing,
            ));

            let selected_master = input.filtered_indices[selected_position];
            let demand_timing = timed(
                "poster-demand-derivation",
                |_| {
                    super::poster_indices_for_view(
                        case.mode,
                        &input.filtered_indices,
                        Some(selected_master),
                        VIEWPORT,
                        params,
                        &aspects,
                    )
                },
                |requested| {
                    requested
                        .iter()
                        .fold(mix(0, requested.len() as u64), |hash, index| {
                            mix(hash, *index as u64)
                        })
                },
            );
            results.push(with_timing(
                context(input, case, selected_position, "poster-demand-derivation"),
                demand_timing,
            ));

            let draw_timing = timed(
                "view-catalog-layout-boundary",
                |_| draw_boundary(input, case, selected_position, params),
                |output| digest_draw(output),
            );
            results.push(with_timing(
                context(
                    input,
                    case,
                    selected_position,
                    "view-catalog-layout-boundary",
                ),
                draw_timing,
            ));

            let poster_timing = timed(
                "schedule-posters-catalog-boundary",
                |_| poster_boundary(input, case, selected_master, params),
                |output| digest_poster(output),
            );
            results.push(with_timing(
                context(
                    input,
                    case,
                    selected_position,
                    "schedule-posters-catalog-boundary",
                ),
                poster_timing,
            ));

            let combined_timing = timed(
                "combined-view-and-schedule-repeated-work",
                |_| combined_boundary(input, case, selected_position, params),
                |output| digest_combined(output),
            );
            results.push(with_timing(
                context(
                    input,
                    case,
                    selected_position,
                    "combined-view-and-schedule-repeated-work",
                ),
                combined_timing,
            ));
        }
        run_repeated_scenarios(&mut results, input);
    }

    let report = json!({
        "schema": "f7-cpu-layout-baseline-v1",
        "scope": "pure CPU aspect extraction, placement, layout/projection-culling, poster demand, and caller-boundary repetition",
        "cold_definition": "first invocation after in-memory inputs and settings are prepared; no application startup, disk, provider, window, GPU, or texture work",
        "sample_method": {
            "warmup_iterations": WARMUP_ITERATIONS,
            "measured_samples": MEASURED_SAMPLES,
            "timing_clock": "std::time::Instant",
            "dead_code_guard": "std::hint::black_box plus deterministic output digest",
            "timing_isolation": "one ignored test selected by exact filter; CARGO_BUILD_JOBS=1",
        },
        "viewport": VIEWPORT,
        "settings": "CandidateState::default() mapped through the production gallery_layout_params helper; Wall layout is the only changed setting for Wall variants",
        "inputs": snapshots,
        "deterministic_checks": deterministic_checks,
        "results": results,
        "limitations": [
            "Synthetic catalogs are scaling evidence, not ordinary-use acceptance.",
            "gallery-layout-total includes placement plus projection/culling; current private Wall projection is not timed as a separately callable stage.",
            "No FPS, GPU/resource, application-latency, compositor, visual, provider, or daily-use conclusion is supported.",
        ],
    });
    println!(
        "F7_REPORT {}",
        serde_json::to_string(&report).expect("F7 report must serialize")
    );
}

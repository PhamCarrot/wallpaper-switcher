//! Explicit mode snapshot projection; no shared flip or whole-gallery capture.
use super::{
    bridge::Source,
    composed::{keys::selector as K, SettingsSource},
};
use crate::candidate_app::*;
use crate::candidate_state::mode_profiles::*;

fn local_number(app: &GpuCandidate, key: &str) -> f32 {
    Source { app }.number(key) as f32
}
fn stage(app: &GpuCandidate, mode: Mode) -> StageTransform {
    let stored = match mode {
        Mode::Geometric => app.state.gallery.geometric.stage.as_ref(),
        Mode::Wall => app.state.gallery.wall.stage.as_ref(),
        _ => None,
    };
    if let Some(stored) = stored {
        return stored.clone();
    }
    let keys = match mode {
        Mode::Geometric => [
            K::HEX_STAGE_X,
            K::HEX_STAGE_Y,
            K::HEX_STAGE_SCALE,
            K::HEX_STAGE_ROTATION,
            K::HEX_STAGE_PERSPECTIVE,
            K::HEX_STAGE_SHEAR_X,
            K::HEX_STAGE_SHEAR_Y,
            K::HEX_STAGE_DEPTH_ANGLE,
        ],
        Mode::Wall => [
            K::GRID_STAGE_X,
            K::GRID_STAGE_Y,
            K::GRID_STAGE_SCALE,
            K::GRID_STAGE_ROTATION,
            K::GRID_STAGE_PERSPECTIVE,
            K::GRID_STAGE_SHEAR_X,
            K::GRID_STAGE_SHEAR_Y,
            K::GRID_STAGE_DEPTH_ANGLE,
        ],
        _ => unreachable!("Only Hex and Wall have source stage transforms"),
    };
    let n = keys.map(|key| local_number(app, key));
    StageTransform {
        x: n[0] / 100.0,
        y: n[1] / 100.0,
        scale: n[2] / 100.0,
        rotation_degrees: n[3],
        perspective: n[4] / 100.0,
        shear_x: n[5] / 100.0,
        shear_y: n[6] / 100.0,
        depth_angle_degrees: n[7],
    }
}
pub(in crate::candidate_app) fn capture(app: &GpuCandidate) -> Snapshot {
    let source = Source { app };
    let gallery = &app.state.gallery;
    match app.gallery_mode {
        GalleryMode::Slices => {
            let g = &gallery.slices;
            Snapshot::Slices(SlicesProfileSnapshot {
                shadows: source.flag(K::SLICE_SHADOWS),
                parallax: source.flag(K::SLICE_PARALLAX),
                offset_x: g.offset_x,
                offset_y: g.offset_y,
                height: g.height,
                visible_count: g.visible_count as u64,
                expanded_width: g.expanded_width,
                width: g.width,
                spacing: g.spacing,
                skew_distance: g.skew_distance,
                edge_tilt_distance: g.edge_tilt_distance,
                wobble: g.wobble,
                wobble_strength: g.wobble_strength,
                round_corners: source.flag(K::ROUND_CORNERS),
                corners: [K::CORNER_TL, K::CORNER_TR, K::CORNER_BR, K::CORNER_BL]
                    .map(|key| local_number(app, key)),
            })
        }
        GalleryMode::Geometric => {
            let g = &gallery.geometric;
            Snapshot::Hex(HexProfileSnapshot {
                parallax: source.flag(K::HEX_PARALLAX),
                radius: g.radius,
                rows: g.rows as u64,
                columns: g.columns as u64,
                scroll_step: g.scroll_step.map_or_else(
                    || (source.number(K::HEX_SCROLL_STEP) as usize).max(1) as u64,
                    candidate_state::HexScrollStep::effective,
                ),
                arc: source.flag(K::HEX_ARC),
                curve_strength: g.curve_strength,
                curve: g.curve,
                shape: g.shape,
                curve_frequency: g.curve_frequency,
                gap_x: g.gap_x,
                gap_y: g.gap_y,
                aspect: g.aspect,
                stagger: g.stagger,
                lens: g.lens,
                lens_radius: local_number(app, K::HEX_LENS_RADIUS),
                orbit: g.orbit,
                orbit_radius: g.orbit_radius,
                twist: g.twist,
                scatter: g.scatter,
                stage: stage(app, Mode::Geometric),
            })
        }
        GalleryMode::Wall => {
            let g = &gallery.wall;
            Snapshot::Wall(WallProfileSnapshot {
                columns: g.columns as u64,
                rows: g.rows as u64,
                card_width: g.card_width,
                card_height: g.card_height,
                gap_x: g.gap_x,
                gap_y: g.gap_y,
                round_corners: source.flag(K::GRID_ROUND_CORNERS),
                corner_radius: g.corner_radius,
                border_width: local_number(app, K::GRID_BORDER_WIDTH),
                layout: g.layout,
                stagger: g.stagger,
                selected_scale: g.selected_scale,
                flow_wave: g.flow_wave,
                flow_frequency: g.flow_frequency,
                scatter: g.scatter,
                scale_variance: g.scale_variance,
                cylinder_bend: g.cylinder_bend,
                cylinder_radius: g.cylinder_radius,
                stage: stage(app, Mode::Wall),
            })
        }
        GalleryMode::Depth => {
            let g = &gallery.depth;
            Snapshot::Depth(DepthProfileSnapshot {
                offset_x: g.offset_x,
                offset_y: g.offset_y,
                shadows: g.shadows,
                selection_frame: g.selection_frame,
                height: g.height,
                width: g.width,
                spacing: g.spacing,
                visible_count: g.visible_count,
                falloff: g.falloff,
                corner_radius: g.corner_radius,
                skew_distance: g.skew_distance,
                navigation_ms: g.navigation_ms,
            })
        }
    }
}

/// Exact source-key adapter for local intent. Ratios use source percentages;
/// canonical arc intensity never goes through the UI-only X10 alias.
pub(in crate::candidate_app) fn source_values(
    snapshot: &Snapshot,
) -> Vec<(&'static str, super::composed::draft::Value)> {
    use super::composed::draft::Value as V;
    let number = |key, value: f32| (key, V::Number(f64::from(value)));
    let flag = |key, value| (key, V::Boolean(value));
    let mut values = match snapshot {
        Snapshot::Slices(g) => vec![
            flag(K::SLICE_SHADOWS, g.shadows),
            flag(K::SLICE_PARALLAX, g.parallax),
            number(K::SLICE_STAGE_X, g.offset_x * 100.0),
            number(K::SLICE_STAGE_Y, g.offset_y * 100.0),
            number(K::SLICE_HEIGHT, g.height),
            number(K::VISIBLE_COUNT, g.visible_count as f32),
            number(K::EXPANDED_WIDTH, g.expanded_width),
            number(K::SLICE_WIDTH, g.width),
            number(K::SLICE_SPACING, g.spacing),
            number(K::SKEW_OFFSET, g.skew_distance),
            number(K::SLICE_EDGE_TILT, g.edge_tilt_distance),
            flag(K::SLICE_WOBBLE, g.wobble),
            number(K::SLICE_WOBBLE_STRENGTH, g.wobble_strength * 100.0),
            flag(K::ROUND_CORNERS, g.round_corners),
            number(K::CORNER_TL, g.corners[0]),
            number(K::CORNER_TR, g.corners[1]),
            number(K::CORNER_BR, g.corners[2]),
            number(K::CORNER_BL, g.corners[3]),
        ],
        Snapshot::Hex(g) => vec![
            flag(K::HEX_PARALLAX, g.parallax),
            number(K::HEX_RADIUS, g.radius),
            number(K::HEX_ROWS, g.rows as f32),
            number(K::HEX_COLS, g.columns as f32),
            (K::HEX_SCROLL_STEP, V::Number(g.scroll_step as f64)),
            flag(K::HEX_ARC, g.arc),
            number(K::HEX_ARC_INTENSITY, g.curve_strength),
            (
                K::HEX_CURVE,
                V::Text(
                    match g.curve {
                        StateGeometricCurve::Flat => "flat",
                        StateGeometricCurve::Arc => "arc",
                        StateGeometricCurve::Wave => "wave",
                        StateGeometricCurve::S => "s",
                        StateGeometricCurve::Cylinder => "cylinder",
                    }
                    .into(),
                ),
            ),
            (
                K::HEX_SHAPE,
                V::Text(
                    match g.shape {
                        StateGeometricShape::Triangle => "triangle",
                        StateGeometricShape::Hexagon => "hexagon",
                        StateGeometricShape::Diamond => "diamond",
                        StateGeometricShape::Rhombus => "rhombus",
                        StateGeometricShape::Circle => "circle",
                    }
                    .into(),
                ),
            ),
            number(K::HEX_CURVE_FREQUENCY, g.curve_frequency),
            number(K::HEX_GAP_X, g.gap_x),
            number(K::HEX_GAP_Y, g.gap_y),
            number(K::HEX_ASPECT, g.aspect * 100.0),
            number(K::HEX_STAGGER, g.stagger * 100.0),
            number(K::HEX_LENS, g.lens * 100.0),
            number(K::HEX_LENS_RADIUS, g.lens_radius),
            number(K::HEX_ORBIT, g.orbit * 100.0),
            number(K::HEX_ORBIT_RADIUS, g.orbit_radius),
            number(K::HEX_TWIST, g.twist),
            number(K::HEX_SCATTER, g.scatter),
        ],
        Snapshot::Wall(g) => vec![
            number(K::GRID_COLUMNS, g.columns as f32),
            number(K::GRID_ROWS, g.rows as f32),
            number(K::GRID_THUMB_WIDTH, g.card_width),
            number(K::GRID_THUMB_HEIGHT, g.card_height),
            number(K::GRID_GAP_X, g.gap_x),
            number(K::GRID_GAP_Y, g.gap_y),
            flag(K::GRID_ROUND_CORNERS, g.round_corners),
            number(K::GRID_CORNER_RADIUS, g.corner_radius),
            number(K::GRID_BORDER_WIDTH, g.border_width),
            (
                K::GRID_LAYOUT,
                V::Text(
                    match g.layout {
                        StateWallLayout::Uniform => "uniform",
                        StateWallLayout::Brick => "brick",
                        StateWallLayout::Masonry => "masonry",
                        StateWallLayout::Justified => "justified",
                        StateWallLayout::Editorial => "editorial",
                        StateWallLayout::Cylinder => "cylinder",
                    }
                    .into(),
                ),
            ),
            number(K::GRID_STAGGER, g.stagger * 100.0),
            number(K::GRID_SELECTED_SCALE, g.selected_scale * 100.0),
            number(K::GRID_FLOW_WAVE, g.flow_wave),
            number(K::GRID_FLOW_FREQUENCY, g.flow_frequency),
            number(K::GRID_SCATTER, g.scatter),
            number(K::GRID_SCALE_VARIANCE, g.scale_variance * 100.0),
            number(K::GRID_CYLINDER_BEND, g.cylinder_bend * 100.0),
            number(K::GRID_CYLINDER_RADIUS, g.cylinder_radius),
        ],
        Snapshot::Depth(g) => vec![
            number(K::DEPTH_STAGE_X, g.offset_x * 100.0),
            number(K::DEPTH_STAGE_Y, g.offset_y * 100.0),
            flag(K::DEPTH_SHADOWS, g.shadows),
            flag(K::DEPTH_SELECTION_FRAME, g.selection_frame),
            number(K::DEPTH_HEIGHT, g.height),
            number(K::DEPTH_WIDTH_PX, g.width),
            number(K::DEPTH_SPACING_PX, g.spacing),
            number(K::DEPTH_COUNT, g.visible_count),
            number(K::DEPTH_FALLOFF_FACTOR, g.falloff),
            number(K::DEPTH_CORNERS, g.corner_radius),
            number(K::DEPTH_SKEW, g.skew_distance),
            number(K::DEPTH_NAVIGATION_MS, g.navigation_ms),
        ],
    };
    let stage = match snapshot {
        Snapshot::Hex(g) => Some((
            &g.stage,
            [
                K::HEX_STAGE_X,
                K::HEX_STAGE_Y,
                K::HEX_STAGE_SCALE,
                K::HEX_STAGE_ROTATION,
                K::HEX_STAGE_PERSPECTIVE,
                K::HEX_STAGE_SHEAR_X,
                K::HEX_STAGE_SHEAR_Y,
                K::HEX_STAGE_DEPTH_ANGLE,
            ],
        )),
        Snapshot::Wall(g) => Some((
            &g.stage,
            [
                K::GRID_STAGE_X,
                K::GRID_STAGE_Y,
                K::GRID_STAGE_SCALE,
                K::GRID_STAGE_ROTATION,
                K::GRID_STAGE_PERSPECTIVE,
                K::GRID_STAGE_SHEAR_X,
                K::GRID_STAGE_SHEAR_Y,
                K::GRID_STAGE_DEPTH_ANGLE,
            ],
        )),
        _ => None,
    };
    if let Some((g, keys)) = stage {
        values.extend(
            keys.into_iter()
                .zip([
                    g.x * 100.0,
                    g.y * 100.0,
                    g.scale * 100.0,
                    g.rotation_degrees,
                    g.perspective * 100.0,
                    g.shear_x * 100.0,
                    g.shear_y * 100.0,
                    g.depth_angle_degrees,
                ])
                .map(|(key, value)| number(key, value)),
        );
    }
    values
}

/// Build a mode-only geometry proposal. No caller may apply a profile by
/// switching modes or copying the legacy whole-gallery preset payload.
pub(in crate::candidate_app) fn geometry_proposal(
    app: &GpuCandidate,
    snapshot: &Snapshot,
) -> Result<crate::candidate_state::GallerySettings, String> {
    if snapshot.mode() != app.state.gallery.mode {
        return Err("Choose a profile from the current mode.".into());
    }
    let mut gallery = app.state.gallery.clone();
    gallery.variant = None;
    match snapshot {
        Snapshot::Slices(g) => {
            let target = &mut gallery.slices;
            target.offset_x = g.offset_x;
            target.offset_y = g.offset_y;
            target.height = g.height;
            target.visible_count = usize::try_from(g.visible_count)
                .map_err(|_| "Slice count cannot be represented.")?;
            target.expanded_width = g.expanded_width;
            target.width = g.width;
            target.spacing = g.spacing;
            target.skew_distance = g.skew_distance;
            target.edge_tilt_distance = g.edge_tilt_distance;
            target.wobble = g.wobble;
            target.wobble_strength = g.wobble_strength;
            target.shadows = Some(g.shadows);
            target.parallax = Some(g.parallax);
            target.round_corners = Some(g.round_corners);
            target.configured_corners = Some(g.corners);
            // Existing scalar remains intact until its P7 renderer replacement.
        }
        Snapshot::Hex(g) => {
            let target = &mut gallery.geometric;
            target.radius = g.radius;
            target.rows = usize::try_from(g.rows).map_err(|_| "Hex rows cannot be represented.")?;
            target.columns =
                usize::try_from(g.columns).map_err(|_| "Hex columns cannot be represented.")?;
            target.curve = g.curve;
            target.curve_strength = g.curve_strength;
            target.shape = g.shape;
            target.curve_frequency = g.curve_frequency;
            target.gap_x = g.gap_x;
            target.gap_y = g.gap_y;
            target.aspect = g.aspect;
            target.stagger = g.stagger;
            target.lens = g.lens;
            target.orbit = g.orbit;
            target.orbit_radius = g.orbit_radius;
            target.twist = g.twist;
            target.scatter = g.scatter;
            target.parallax = Some(g.parallax);
            target.scroll_step = Some(candidate_state::HexScrollStep::from_profile(g.scroll_step)?);
            target.arc = Some(g.arc);
            target.lens_radius = Some(g.lens_radius);
            target.stage = Some(g.stage.clone());
        }
        Snapshot::Wall(g) => {
            let target = &mut gallery.wall;
            target.columns =
                usize::try_from(g.columns).map_err(|_| "Wall columns cannot be represented.")?;
            target.rows =
                usize::try_from(g.rows).map_err(|_| "Wall rows cannot be represented.")?;
            target.card_width = g.card_width;
            target.card_height = g.card_height;
            target.gap_x = g.gap_x;
            target.gap_y = g.gap_y;
            target.corner_radius = g.corner_radius;
            target.layout = g.layout;
            target.stagger = g.stagger;
            target.selected_scale = g.selected_scale;
            target.flow_wave = g.flow_wave;
            target.flow_frequency = g.flow_frequency;
            target.scatter = g.scatter;
            target.scale_variance = g.scale_variance;
            target.cylinder_bend = g.cylinder_bend;
            target.cylinder_radius = g.cylinder_radius;
            target.round_corners = Some(g.round_corners);
            target.border_width = Some(g.border_width);
            target.stage = Some(g.stage.clone());
        }
        Snapshot::Depth(g) => {
            let target = &mut gallery.depth;
            target.offset_x = g.offset_x;
            target.offset_y = g.offset_y;
            target.shadows = g.shadows;
            target.selection_frame = g.selection_frame;
            target.height = g.height;
            target.width = g.width;
            target.spacing = g.spacing;
            target.visible_count = g.visible_count;
            target.falloff = g.falloff;
            target.corner_radius = g.corner_radius;
            target.skew_distance = g.skew_distance;
            target.navigation_ms = g.navigation_ms;
            // target.detail is the preserved shared-flip legacy seed, excluded.
        }
    }
    let values = source_values(snapshot);
    for (key, value) in &values {
        if let super::composed::draft::Value::Number(number) = value {
            if !number.is_finite() {
                return Err(format!("Non-finite profile value: {key}"));
            }
        }
    }
    if !current_geometry_is_valid(&gallery) {
        return Err("Profile geometry is outside the current renderer bounds.".into());
    }
    Ok(gallery)
}

pub(in crate::candidate_app) fn install(
    app: &mut GpuCandidate,
    snapshot: Result<&Snapshot, &str>,
    selected_id: Option<u64>,
) -> (Task<Message>, Result<(), String>) {
    match super::controller::finish_geometry_owner_edit(app) {
        Ok(task) => {
            let result = snapshot
                .map_err(str::to_owned)
                .and_then(|snapshot| install_after_edit(app, snapshot, selected_id));
            (task, result)
        }
        Err(task) => (task, Err(app.settings_workbench.message.clone())),
    }
}

fn install_after_edit(
    app: &mut GpuCandidate,
    snapshot: &Snapshot,
    selected_id: Option<u64>,
) -> Result<(), String> {
    let geometry = geometry_proposal(app, snapshot)?;
    let mut proposed = app.state.clone();
    proposed.gallery = geometry;
    if let Some(id) = selected_id {
        if !proposed.mode_profiles.select(snapshot.mode(), id) {
            return Err("Profile identity or geometry is no longer available.".into());
        }
    }
    let previous = app.state.gallery.depth.clone();
    let changed = proposed.gallery != app.state.gallery;
    let ordinary_outgoing = if changed && app.gallery_mode != GalleryMode::Depth {
        app.capture_ordinary_roll()?
    } else {
        None
    };
    let depth_outgoing = if changed && app.gallery_mode == GalleryMode::Depth {
        app.capture_depth_roll()
    } else {
        None
    };
    app.commit_local_state_change(proposed).map_err(|error| {
        app.save_failure
            .clone()
            .unwrap_or_else(|| error.to_string())
    })?;
    for (key, value) in source_values(snapshot) {
        app.settings_workbench
            .draft
            .values
            .insert(key.into(), value);
    }
    if changed {
        if app.gallery_mode == GalleryMode::Depth {
            app.depth_settings_changed(&previous);
            if depth_outgoing.is_some() {
                app.depth_roll = depth_outgoing;
                app.start_motion(0.0);
            }
        } else {
            app.ordinary_roll = ordinary_outgoing;
            app.start_motion(0.0);
        }
        app.wall_scroll_offset = 0.0;
        app.schedule_posters();
    }
    Ok(())
}

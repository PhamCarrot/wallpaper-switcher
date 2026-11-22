//! Source-only current geometry in the existing mode owners. Renderer ports
//! remain separate. Optional new members preserve historical scalar/absence data.
use super::composed::{draft::Value, keys::selector as K, schema};
use crate::candidate_state::{CandidateState, GalleryKind};

pub(in crate::candidate_app) fn read(state: &CandidateState, path: &str) -> Option<Value> {
    match path {
        K::SLICE_SHADOWS => state.gallery.slices.shadows.map(Value::Boolean),
        K::SLICE_PARALLAX => state.gallery.slices.parallax.map(Value::Boolean),
        K::ROUND_CORNERS => state.gallery.slices.round_corners.map(Value::Boolean),
        K::HEX_PARALLAX => state.gallery.geometric.parallax.map(Value::Boolean),
        K::HEX_ARC => state.gallery.geometric.arc.map(Value::Boolean),
        K::GRID_ROUND_CORNERS => state.gallery.wall.round_corners.map(Value::Boolean),
        K::HEX_SCROLL_STEP => state
            .gallery
            .geometric
            .scroll_step
            .map(|value| Value::Number(value.configured_number())),
        K::HEX_LENS_RADIUS => state
            .gallery
            .geometric
            .lens_radius
            .map(|value| Value::Number(value as f64 * 1.0)),
        K::GRID_BORDER_WIDTH => state
            .gallery
            .wall
            .border_width
            .map(|value| Value::Number(value as f64 * 1.0)),
        K::CORNER_TL => state
            .gallery
            .slices
            .configured_corners
            .map(|values| Value::Number(f64::from(values[0]))),
        K::CORNER_TR => state
            .gallery
            .slices
            .configured_corners
            .map(|values| Value::Number(f64::from(values[1]))),
        K::CORNER_BR => state
            .gallery
            .slices
            .configured_corners
            .map(|values| Value::Number(f64::from(values[2]))),
        K::CORNER_BL => state
            .gallery
            .slices
            .configured_corners
            .map(|values| Value::Number(f64::from(values[3]))),
        K::HEX_STAGE_X => state
            .gallery
            .geometric
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.x) * 100.0)),
        K::HEX_STAGE_Y => state
            .gallery
            .geometric
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.y) * 100.0)),
        K::HEX_STAGE_SCALE => state
            .gallery
            .geometric
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.scale) * 100.0)),
        K::HEX_STAGE_ROTATION => state
            .gallery
            .geometric
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.rotation_degrees) * 1.0)),
        K::HEX_STAGE_PERSPECTIVE => state
            .gallery
            .geometric
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.perspective) * 100.0)),
        K::HEX_STAGE_SHEAR_X => state
            .gallery
            .geometric
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.shear_x) * 100.0)),
        K::HEX_STAGE_SHEAR_Y => state
            .gallery
            .geometric
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.shear_y) * 100.0)),
        K::HEX_STAGE_DEPTH_ANGLE => state
            .gallery
            .geometric
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.depth_angle_degrees) * 1.0)),
        K::GRID_STAGE_X => state
            .gallery
            .wall
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.x) * 100.0)),
        K::GRID_STAGE_Y => state
            .gallery
            .wall
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.y) * 100.0)),
        K::GRID_STAGE_SCALE => state
            .gallery
            .wall
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.scale) * 100.0)),
        K::GRID_STAGE_ROTATION => state
            .gallery
            .wall
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.rotation_degrees) * 1.0)),
        K::GRID_STAGE_PERSPECTIVE => state
            .gallery
            .wall
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.perspective) * 100.0)),
        K::GRID_STAGE_SHEAR_X => state
            .gallery
            .wall
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.shear_x) * 100.0)),
        K::GRID_STAGE_SHEAR_Y => state
            .gallery
            .wall
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.shear_y) * 100.0)),
        K::GRID_STAGE_DEPTH_ANGLE => state
            .gallery
            .wall
            .stage
            .as_ref()
            .map(|stage| Value::Number(f64::from(stage.depth_angle_degrees) * 1.0)),
        _ => None,
    }
}
pub(in crate::candidate_app) fn write(
    state: &mut CandidateState,
    path: &str,
    value: &Value,
) -> Result<bool, String> {
    let owner = match path {
        K::SLICE_SHADOWS
        | K::SLICE_PARALLAX
        | K::ROUND_CORNERS
        | K::SLICE_STAGE_X
        | K::SLICE_STAGE_Y
        | K::CORNER_TL
        | K::CORNER_TR
        | K::CORNER_BR
        | K::CORNER_BL => GalleryKind::Slices,
        K::HEX_PARALLAX
        | K::HEX_ARC
        | K::HEX_SCROLL_STEP
        | K::HEX_LENS_RADIUS
        | K::HEX_ARC_INTENSITY
        | K::HEX_ASPECT
        | K::HEX_STAGGER
        | K::HEX_LENS
        | K::HEX_ORBIT
        | K::HEX_TWIST
        | K::HEX_SCATTER
        | K::HEX_STAGE_X
        | K::HEX_STAGE_Y
        | K::HEX_STAGE_SCALE
        | K::HEX_STAGE_ROTATION
        | K::HEX_STAGE_PERSPECTIVE
        | K::HEX_STAGE_SHEAR_X
        | K::HEX_STAGE_SHEAR_Y
        | K::HEX_STAGE_DEPTH_ANGLE => GalleryKind::Geometric,
        K::DEPTH_STAGE_X | K::DEPTH_STAGE_Y => GalleryKind::Depth,
        K::GRID_ROUND_CORNERS
        | K::GRID_BORDER_WIDTH
        | K::GRID_SELECTED_SCALE
        | K::GRID_FLOW_WAVE
        | K::GRID_FLOW_FREQUENCY
        | K::GRID_SCATTER
        | K::GRID_SCALE_VARIANCE
        | K::GRID_STAGE_X
        | K::GRID_STAGE_Y
        | K::GRID_STAGE_SCALE
        | K::GRID_STAGE_ROTATION
        | K::GRID_STAGE_PERSPECTIVE
        | K::GRID_STAGE_SHEAR_X
        | K::GRID_STAGE_SHEAR_Y
        | K::GRID_STAGE_DEPTH_ANGLE => GalleryKind::Wall,
        _ => return Ok(false),
    };
    if let (K::HEX_SCROLL_STEP, Value::Number(value)) = (path, value) {
        if state.gallery.mode != GalleryKind::Geometric {
            return Ok(false);
        }
        state.gallery.geometric.scroll_step =
            Some(crate::candidate_state::HexScrollStep::from_input(*value)?);
        state.gallery.variant = None;
        return Ok(true);
    }
    let number = match value {
        Value::Number(number) => {
            if !number.is_finite() {
                return Err("Enter a finite geometry value.".into());
            }
            let normalized = schema::find(path)
                .and_then(|spec| spec.number_bounds)
                .map_or(*number, |bounds| number.clamp(bounds.min, bounds.max));
            let narrowed = normalized as f32;
            if !narrowed.is_finite() {
                return Err("This geometry value cannot be represented.".into());
            }
            Some(narrowed)
        }
        _ => None,
    };
    match (path, value) {
        (K::SLICE_SHADOWS, Value::Boolean(value)) if state.gallery.mode == GalleryKind::Slices => {
            state.gallery.slices.shadows = Some(*value)
        }
        (K::SLICE_PARALLAX, Value::Boolean(value)) if state.gallery.mode == GalleryKind::Slices => {
            state.gallery.slices.parallax = Some(*value)
        }
        (K::ROUND_CORNERS, Value::Boolean(value)) if state.gallery.mode == GalleryKind::Slices => {
            state.gallery.slices.round_corners = Some(*value)
        }
        (K::HEX_PARALLAX, Value::Boolean(value))
            if state.gallery.mode == GalleryKind::Geometric =>
        {
            state.gallery.geometric.parallax = Some(*value)
        }
        (K::HEX_ARC, Value::Boolean(value)) if state.gallery.mode == GalleryKind::Geometric => {
            state.gallery.geometric.arc = Some(*value)
        }
        (K::GRID_ROUND_CORNERS, Value::Boolean(value))
            if state.gallery.mode == GalleryKind::Wall =>
        {
            state.gallery.wall.round_corners = Some(*value)
        }

        (K::HEX_LENS_RADIUS, Value::Number(_)) if state.gallery.mode == GalleryKind::Geometric => {
            state.gallery.geometric.lens_radius = Some((number.unwrap() / 1.0))
        }
        (K::GRID_BORDER_WIDTH, Value::Number(_)) if state.gallery.mode == GalleryKind::Wall => {
            state.gallery.wall.border_width = Some((number.unwrap() / 1.0))
        }
        (K::DEPTH_STAGE_X, Value::Number(_)) => {
            state.gallery.depth.offset_x = number.unwrap() / 100.0
        }
        (K::DEPTH_STAGE_Y, Value::Number(_)) => {
            state.gallery.depth.offset_y = number.unwrap() / 100.0
        }
        (K::SLICE_STAGE_X, Value::Number(_)) => {
            state.gallery.slices.offset_x = number.unwrap() / 100.0
        }
        (K::SLICE_STAGE_Y, Value::Number(_)) => {
            state.gallery.slices.offset_y = number.unwrap() / 100.0
        }
        (K::HEX_ARC_INTENSITY, Value::Number(_))
            if state.gallery.mode == GalleryKind::Geometric =>
        {
            state.gallery.geometric.curve_strength = number.unwrap() / 1.0
        }
        (K::HEX_ASPECT, Value::Number(_)) if state.gallery.mode == GalleryKind::Geometric => {
            state.gallery.geometric.aspect = number.unwrap() / 100.0
        }
        (K::HEX_STAGGER, Value::Number(_)) if state.gallery.mode == GalleryKind::Geometric => {
            state.gallery.geometric.stagger = number.unwrap() / 100.0
        }
        (K::HEX_LENS, Value::Number(_)) if state.gallery.mode == GalleryKind::Geometric => {
            state.gallery.geometric.lens = number.unwrap() / 100.0
        }
        (K::HEX_ORBIT, Value::Number(_)) if state.gallery.mode == GalleryKind::Geometric => {
            state.gallery.geometric.orbit = number.unwrap() / 100.0
        }
        (K::HEX_TWIST, Value::Number(_)) if state.gallery.mode == GalleryKind::Geometric => {
            state.gallery.geometric.twist = number.unwrap() / 1.0
        }
        (K::HEX_SCATTER, Value::Number(_)) if state.gallery.mode == GalleryKind::Geometric => {
            state.gallery.geometric.scatter = number.unwrap() / 1.0
        }
        (K::GRID_SELECTED_SCALE, Value::Number(_)) if state.gallery.mode == GalleryKind::Wall => {
            state.gallery.wall.selected_scale = number.unwrap() / 100.0
        }
        (K::GRID_FLOW_WAVE, Value::Number(_)) if state.gallery.mode == GalleryKind::Wall => {
            state.gallery.wall.flow_wave = number.unwrap() / 1.0
        }
        (K::GRID_FLOW_FREQUENCY, Value::Number(_)) if state.gallery.mode == GalleryKind::Wall => {
            state.gallery.wall.flow_frequency = number.unwrap() / 1.0
        }
        (K::GRID_SCATTER, Value::Number(_)) if state.gallery.mode == GalleryKind::Wall => {
            state.gallery.wall.scatter = number.unwrap() / 1.0
        }
        (K::GRID_SCALE_VARIANCE, Value::Number(_)) if state.gallery.mode == GalleryKind::Wall => {
            state.gallery.wall.scale_variance = number.unwrap() / 100.0
        }
        (K::CORNER_TL, Value::Number(_)) if state.gallery.mode == GalleryKind::Slices => {
            let values = state
                .gallery
                .slices
                .configured_corners
                .get_or_insert([state.gallery.slices.corner_radius; 4]);
            values[0] = number.unwrap();
        }
        (K::CORNER_TR, Value::Number(_)) if state.gallery.mode == GalleryKind::Slices => {
            let values = state
                .gallery
                .slices
                .configured_corners
                .get_or_insert([state.gallery.slices.corner_radius; 4]);
            values[1] = number.unwrap();
        }
        (K::CORNER_BR, Value::Number(_)) if state.gallery.mode == GalleryKind::Slices => {
            let values = state
                .gallery
                .slices
                .configured_corners
                .get_or_insert([state.gallery.slices.corner_radius; 4]);
            values[2] = number.unwrap();
        }
        (K::CORNER_BL, Value::Number(_)) if state.gallery.mode == GalleryKind::Slices => {
            let values = state
                .gallery
                .slices
                .configured_corners
                .get_or_insert([state.gallery.slices.corner_radius; 4]);
            values[3] = number.unwrap();
        }
        (K::HEX_STAGE_X, Value::Number(_)) => {
            state
                .gallery
                .geometric
                .stage
                .get_or_insert_with(Default::default)
                .x = number.unwrap() / 100.0
        }
        (K::HEX_STAGE_Y, Value::Number(_)) => {
            state
                .gallery
                .geometric
                .stage
                .get_or_insert_with(Default::default)
                .y = number.unwrap() / 100.0
        }
        (K::HEX_STAGE_SCALE, Value::Number(_)) if state.gallery.mode == GalleryKind::Geometric => {
            state
                .gallery
                .geometric
                .stage
                .get_or_insert_with(Default::default)
                .scale = number.unwrap() / 100.0
        }
        (K::HEX_STAGE_ROTATION, Value::Number(_))
            if state.gallery.mode == GalleryKind::Geometric =>
        {
            state
                .gallery
                .geometric
                .stage
                .get_or_insert_with(Default::default)
                .rotation_degrees = number.unwrap() / 1.0
        }
        (K::HEX_STAGE_PERSPECTIVE, Value::Number(_))
            if state.gallery.mode == GalleryKind::Geometric =>
        {
            state
                .gallery
                .geometric
                .stage
                .get_or_insert_with(Default::default)
                .perspective = number.unwrap() / 100.0
        }
        (K::HEX_STAGE_SHEAR_X, Value::Number(_))
            if state.gallery.mode == GalleryKind::Geometric =>
        {
            state
                .gallery
                .geometric
                .stage
                .get_or_insert_with(Default::default)
                .shear_x = number.unwrap() / 100.0
        }
        (K::HEX_STAGE_SHEAR_Y, Value::Number(_))
            if state.gallery.mode == GalleryKind::Geometric =>
        {
            state
                .gallery
                .geometric
                .stage
                .get_or_insert_with(Default::default)
                .shear_y = number.unwrap() / 100.0
        }
        (K::HEX_STAGE_DEPTH_ANGLE, Value::Number(_))
            if state.gallery.mode == GalleryKind::Geometric =>
        {
            state
                .gallery
                .geometric
                .stage
                .get_or_insert_with(Default::default)
                .depth_angle_degrees = number.unwrap() / 1.0
        }
        (K::GRID_STAGE_X, Value::Number(_)) => {
            state
                .gallery
                .wall
                .stage
                .get_or_insert_with(Default::default)
                .x = number.unwrap() / 100.0
        }
        (K::GRID_STAGE_Y, Value::Number(_)) => {
            state
                .gallery
                .wall
                .stage
                .get_or_insert_with(Default::default)
                .y = number.unwrap() / 100.0
        }
        (K::GRID_STAGE_SCALE, Value::Number(_)) if state.gallery.mode == GalleryKind::Wall => {
            state
                .gallery
                .wall
                .stage
                .get_or_insert_with(Default::default)
                .scale = number.unwrap() / 100.0
        }
        (K::GRID_STAGE_ROTATION, Value::Number(_)) if state.gallery.mode == GalleryKind::Wall => {
            state
                .gallery
                .wall
                .stage
                .get_or_insert_with(Default::default)
                .rotation_degrees = number.unwrap() / 1.0
        }
        (K::GRID_STAGE_PERSPECTIVE, Value::Number(_))
            if state.gallery.mode == GalleryKind::Wall =>
        {
            state
                .gallery
                .wall
                .stage
                .get_or_insert_with(Default::default)
                .perspective = number.unwrap() / 100.0
        }
        (K::GRID_STAGE_SHEAR_X, Value::Number(_)) if state.gallery.mode == GalleryKind::Wall => {
            state
                .gallery
                .wall
                .stage
                .get_or_insert_with(Default::default)
                .shear_x = number.unwrap() / 100.0
        }
        (K::GRID_STAGE_SHEAR_Y, Value::Number(_)) if state.gallery.mode == GalleryKind::Wall => {
            state
                .gallery
                .wall
                .stage
                .get_or_insert_with(Default::default)
                .shear_y = number.unwrap() / 100.0
        }
        (K::GRID_STAGE_DEPTH_ANGLE, Value::Number(_))
            if state.gallery.mode == GalleryKind::Wall =>
        {
            state
                .gallery
                .wall
                .stage
                .get_or_insert_with(Default::default)
                .depth_angle_degrees = number.unwrap() / 1.0
        }
        _ => return Ok(false),
    }
    if owner == state.gallery.mode {
        state.gallery.variant = None;
    }
    Ok(true)
}

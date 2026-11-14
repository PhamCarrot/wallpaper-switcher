//! Pure scene layout boundary reused from the pinned skwd-wall frontend.
//!
//! This module intentionally contains only presentation data and transforms:
//! StageParams, Hex/Geometric parameters, Grid placement primitives and hit
//! testing. The donor SceneCore, atlas ownership, preview services, daemon,
//! WindowsHost and provider actions are not imported.  supplies its own
//! stable catalog identities, bounded static poster cache and WGPU upload path.
//!
//! Provenance: skwd-wall v1.0.0-beta.13, commit
//! 9735be4ee1cf8b367773170e9e24c4f07ec3dd8b, adapted from
//! frontend/scene/layout.rs and app/scene/core/view_grid.rs.
//! See docs/third_party/SKWD-WALL-NOTICE.md.
pub const MIN_HEX_R: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HexCurve {
    Flat,
    Arc,
    Wave,
    S,
    Cylinder,
}

impl HexCurve {
    pub fn from_key(value: &str) -> Self {
        match value {
            "flat" => Self::Flat,
            "wave" => Self::Wave,
            "s" => Self::S,
            "cylinder" => Self::Cylinder,
            _ => Self::Arc,
        }
    }

    pub const fn as_key(self) -> &'static str {
        match self {
            Self::Flat => "flat",
            Self::Arc => "arc",
            Self::Wave => "wave",
            Self::S => "s",
            Self::Cylinder => "cylinder",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HexShape {
    #[default]
    Hexagon,
    Triangle,
    Diamond,
    Rhombus,
}

impl HexShape {
    pub fn from_key(value: &str) -> Self {
        match value {
            "triangle" => Self::Triangle,
            "diamond" => Self::Diamond,
            "rhombus" => Self::Rhombus,
            _ => Self::Hexagon,
        }
    }

    pub const fn as_key(self) -> &'static str {
        match self {
            Self::Hexagon => "hexagon",
            Self::Triangle => "triangle",
            Self::Diamond => "diamond",
            Self::Rhombus => "rhombus",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GridLayout {
    Uniform,
    Brick,
    Masonry,
    Justified,
    Editorial,
    Cylinder,
}

impl GridLayout {
    pub fn from_key(value: &str) -> Self {
        match value {
            "brick" => Self::Brick,
            "masonry" => Self::Masonry,
            "justified" => Self::Justified,
            "editorial" => Self::Editorial,
            "cylinder" => Self::Cylinder,
            _ => Self::Uniform,
        }
    }

    pub const fn as_key(self) -> &'static str {
        match self {
            Self::Uniform => "uniform",
            Self::Brick => "brick",
            Self::Masonry => "masonry",
            Self::Justified => "justified",
            Self::Editorial => "editorial",
            Self::Cylinder => "cylinder",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageParams {
    pub offset_x: f32,
    pub offset_y: f32,
    pub scale: f32,
    pub rotation: f32,
    pub perspective: f32,
    pub shear_x: f32,
    pub shear_y: f32,
    pub depth_angle: f32,
}

impl Default for StageParams {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            scale: 1.0,
            rotation: 0.0,
            perspective: 0.0,
            shear_x: 0.0,
            shear_y: 0.0,
            depth_angle: 0.0,
        }
    }
}

impl StageParams {
    pub fn morph_toward(&mut self, target: &Self, amt: f32) {
        self.offset_x = lerp(self.offset_x, target.offset_x, amt);
        self.offset_y = lerp(self.offset_y, target.offset_y, amt);
        self.scale = lerp(self.scale, target.scale, amt);
        self.rotation = lerp(self.rotation, target.rotation, amt);
        self.perspective = lerp(self.perspective, target.perspective, amt);
        self.shear_x = lerp(self.shear_x, target.shear_x, amt);
        self.shear_y = lerp(self.shear_y, target.shear_y, amt);
        self.depth_angle = lerp(self.depth_angle, target.depth_angle, amt);
    }

    pub fn settled_to(&self, target: &Self) -> bool {
        feq(self.offset_x, target.offset_x)
            && feq(self.offset_y, target.offset_y)
            && feq(self.scale, target.scale)
            && feq(self.rotation, target.rotation)
            && feq(self.perspective, target.perspective)
            && feq(self.shear_x, target.shear_x)
            && feq(self.shear_y, target.shear_y)
            && feq(self.depth_angle, target.depth_angle)
    }

    pub fn transform(
        &self,
        x: f32,
        y: f32,
        origin: (f32, f32),
        viewport: (f32, f32),
    ) -> (f32, f32, f32) {
        let (ox, oy) = origin;
        let radians = self.rotation.to_radians();
        let (sin, cos) = radians.sin_cos();
        let lx = (x - ox) * self.scale;
        let ly = (y - oy) * self.scale;
        let sheared_x = lx + self.shear_x * ly;
        let sheared_y = ly + self.shear_y * lx;
        let rx = sheared_x * cos - sheared_y * sin;
        let ry = sheared_x * sin + sheared_y * cos;
        let depth_radians = self.depth_angle.to_radians();
        let depth_axis = rx * depth_radians.sin() + ry * depth_radians.cos();
        let depth =
            (1.0 + self.perspective * depth_axis / (viewport.1 * 0.5).max(1.0)).clamp(0.35, 2.0);
        (
            ox + self.offset_x * viewport.0 * 0.5 + rx * depth,
            oy + self.offset_y * viewport.1 * 0.5 + ry,
            self.scale * depth,
        )
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HexParams {
    pub r: f32,
    pub rows: usize,
    pub cols: usize,
    pub scroll_step: usize,
    pub curve: HexCurve,
    pub shape: HexShape,
    pub curve_strength: f32,
    pub curve_frequency: f32,
    pub gap_x: f32,
    pub gap_y: f32,
    pub aspect: f32,
    pub stagger: f32,
    pub lens: f32,
    pub lens_radius: f32,
    pub orbit: f32,
    pub orbit_radius: f32,
    pub twist: f32,
    pub scatter: f32,
    pub stage: StageParams,
}

impl Default for HexParams {
    fn default() -> Self {
        Self {
            r: 140.0,
            rows: 3,
            cols: 7,
            scroll_step: 1,
            curve: HexCurve::Arc,
            shape: HexShape::Hexagon,
            curve_strength: 1.2,
            curve_frequency: 1.0,
            gap_x: 6.0,
            gap_y: 6.0,
            aspect: 1.0,
            stagger: 0.5,
            lens: 0.0,
            lens_radius: 620.0,
            orbit: 0.0,
            orbit_radius: 620.0,
            twist: 0.0,
            scatter: 0.0,
            stage: StageParams::default(),
        }
    }
}

impl HexParams {
    pub fn topology_differs(&self, target: &HexParams) -> bool {
        self.rows != target.rows
            || self.cols != target.cols
            || self.scroll_step != target.scroll_step
            || self.curve != target.curve
            || self.shape != target.shape
    }

    pub fn morph_toward(&mut self, target: &HexParams, amt: f32) {
        self.r = lerp(self.r, target.r, amt);
        self.rows = target.rows;
        self.cols = target.cols;
        self.scroll_step = target.scroll_step;
        self.curve = target.curve;
        self.shape = target.shape;
        self.curve_strength = lerp(self.curve_strength, target.curve_strength, amt);
        self.curve_frequency = lerp(self.curve_frequency, target.curve_frequency, amt);
        self.gap_x = lerp(self.gap_x, target.gap_x, amt);
        self.gap_y = lerp(self.gap_y, target.gap_y, amt);
        self.aspect = lerp(self.aspect, target.aspect, amt);
        self.stagger = lerp(self.stagger, target.stagger, amt);
        self.lens = lerp(self.lens, target.lens, amt);
        self.lens_radius = lerp(self.lens_radius, target.lens_radius, amt);
        self.orbit = lerp(self.orbit, target.orbit, amt);
        self.orbit_radius = lerp(self.orbit_radius, target.orbit_radius, amt);
        self.twist = lerp(self.twist, target.twist, amt);
        self.scatter = lerp(self.scatter, target.scatter, amt);
        self.stage.morph_toward(&target.stage, amt);
    }

    pub fn settled_to(&self, target: &HexParams) -> bool {
        feq(self.r, target.r)
            && self.rows == target.rows
            && self.cols == target.cols
            && self.scroll_step == target.scroll_step
            && self.curve == target.curve
            && self.shape == target.shape
            && feq(self.curve_strength, target.curve_strength)
            && feq(self.curve_frequency, target.curve_frequency)
            && feq(self.gap_x, target.gap_x)
            && feq(self.gap_y, target.gap_y)
            && feq(self.aspect, target.aspect)
            && feq(self.stagger, target.stagger)
            && feq(self.lens, target.lens)
            && feq(self.lens_radius, target.lens_radius)
            && feq(self.orbit, target.orbit)
            && feq(self.orbit_radius, target.orbit_radius)
            && feq(self.twist, target.twist)
            && feq(self.scatter, target.scatter)
            && self.stage.settled_to(&target.stage)
    }

    pub fn hex_h(&self) -> f32 {
        (self.r * 1.73205).ceil()
    }

    pub fn item_half_w(&self) -> f32 {
        match self.shape {
            HexShape::Rhombus => 2.0 * self.r * self.aspect,
            HexShape::Hexagon | HexShape::Triangle | HexShape::Diamond => self.r * self.aspect,
        }
    }

    pub fn item_half_h(&self) -> f32 {
        match self.shape {
            HexShape::Diamond => self.hex_h(),
            HexShape::Hexagon | HexShape::Triangle | HexShape::Rhombus => self.hex_h() / 2.0,
        }
    }

    pub fn step_x(&self) -> f32 {
        match self.shape {
            HexShape::Triangle => self.r * self.aspect + self.gap_x,
            HexShape::Diamond | HexShape::Rhombus => self.item_half_w() + self.gap_x,
            HexShape::Hexagon => 1.5 * self.r * self.aspect + self.gap_x,
        }
    }

    pub fn step_y(&self) -> f32 {
        self.item_half_h() * 2.0 + self.gap_y
    }

    pub fn content_h(&self) -> f32 {
        let stagger_tail = if matches!(
            self.shape,
            HexShape::Hexagon | HexShape::Diamond | HexShape::Rhombus
        ) {
            self.step_y() * self.stagger
        } else {
            0.0
        };
        (self.rows.saturating_sub(1)) as f32 * self.step_y()
            + self.item_half_h() * 2.0
            + stagger_tail
    }

    pub fn visible_band(&self) -> f32 {
        self.column_x(self.cols.saturating_sub(1)) + self.item_half_w()
    }

    pub fn column_x(&self, col: usize) -> f32 {
        col as f32 * self.step_x() + self.item_half_w()
    }

    pub fn row_y(&self, row: usize) -> f32 {
        row as f32 * self.step_y() + self.item_half_h()
    }

    pub fn stagger_offset(&self, col: usize) -> f32 {
        if col.is_multiple_of(2)
            || !matches!(
                self.shape,
                HexShape::Hexagon | HexShape::Diamond | HexShape::Rhombus
            )
        {
            0.0
        } else {
            self.step_y() * self.stagger
        }
    }

    pub fn deform(&self, index: usize, x: f32, y: f32, origin: (f32, f32)) -> (f32, f32) {
        let mut lx = x - origin.0;
        let mut ly = y - origin.1;
        let radius = self.orbit_radius.max(1.0);
        let orbit = if self.curve == HexCurve::Cylinder {
            self.curve_strength.clamp(-1.0, 1.0)
        } else {
            self.orbit
        };
        let orbit_mix = orbit.abs().clamp(0.0, 1.0);
        if orbit_mix > 0.0 {
            let angle = lx / radius * orbit.signum();
            let ring = (radius + ly).max(radius * 0.08);
            lx = lerp(lx, angle.sin() * ring, orbit_mix);
            ly = lerp(ly, radius - angle.cos() * ring, orbit_mix);
        }
        if self.twist.abs() > 0.001 {
            let angle = self.twist.to_radians() * lx.hypot(ly) / radius;
            let (sin, cos) = angle.sin_cos();
            (lx, ly) = (lx * cos - ly * sin, lx * sin + ly * cos);
        }
        if self.scatter > 0.001 {
            lx += signed_hash(index, 0x4f1b_bcdd) * self.scatter;
            ly += signed_hash(index, 0x9e37_79b9) * self.scatter;
        }
        (origin.0 + lx, origin.1 + ly)
    }
}

pub(crate) fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub(crate) fn feq(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.05
}

pub(crate) fn signed_hash(index: usize, salt: u32) -> f32 {
    let mut value = (index as u32).wrapping_add(salt);
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    value as f32 / u32::MAX as f32 * 2.0 - 1.0
}

#[derive(Debug, Clone, Copy)]
pub struct SliceParams {
    pub offset_x: f32,
    pub offset_y: f32,
    pub slice_w: f32,
    pub expanded_w: f32,
    pub slice_h: f32,
    pub spacing: f32,
    pub skew: f32,
    pub edge_tilt: f32,
    pub visible_count: usize,
    pub corners: [f32; 4],
    pub wobble: bool,
    pub wobble_strength: f32,
}

pub fn wobble_pad(strength: f32) -> f32 {
    1.05 + 0.30 * strength.max(1.0)
}

pub fn wobble_bend(vel: f32, scale: f32) -> f32 {
    (vel / (scale * 2.2).max(1.0)).clamp(-1.35, 1.35)
}

pub fn slice_midline_width(width: f32, skew: f32) -> f32 {
    (width - skew.abs()).max(1.0)
}

impl SliceParams {
    pub fn topology_differs(&self, target: &SliceParams) -> bool {
        self.visible_count != target.visible_count || self.wobble != target.wobble
    }

    pub fn layout_width(&self, width: f32) -> f32 {
        slice_midline_width(width, self.skew)
    }

    pub fn slice_stride(&self) -> f32 {
        self.layout_width(self.slice_w) + self.spacing
    }

    pub fn morph_toward(&mut self, target: &SliceParams, amt: f32) {
        self.offset_x = lerp(self.offset_x, target.offset_x, amt);
        self.offset_y = lerp(self.offset_y, target.offset_y, amt);
        self.slice_w = lerp(self.slice_w, target.slice_w, amt);
        self.expanded_w = lerp(self.expanded_w, target.expanded_w, amt);
        self.slice_h = lerp(self.slice_h, target.slice_h, amt);
        self.spacing = lerp(self.spacing, target.spacing, amt);
        self.skew = lerp(self.skew, target.skew, amt);
        self.edge_tilt = lerp(self.edge_tilt, target.edge_tilt, amt);
        self.visible_count = target.visible_count;
        self.wobble = target.wobble;
        self.wobble_strength = lerp(self.wobble_strength, target.wobble_strength, amt);
        for idx in 0..4 {
            self.corners[idx] = lerp(self.corners[idx], target.corners[idx], amt);
        }
    }

    pub fn settled_to(&self, target: &SliceParams) -> bool {
        feq(self.offset_x, target.offset_x)
            && feq(self.offset_y, target.offset_y)
            && feq(self.slice_w, target.slice_w)
            && feq(self.expanded_w, target.expanded_w)
            && feq(self.slice_h, target.slice_h)
            && feq(self.spacing, target.spacing)
            && feq(self.skew, target.skew)
            && feq(self.edge_tilt, target.edge_tilt)
            && self.visible_count == target.visible_count
            && self.wobble == target.wobble
            && feq(self.wobble_strength, target.wobble_strength)
            && self
                .corners
                .iter()
                .zip(target.corners)
                .all(|(a, b)| feq(*a, b))
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GridParams {
    pub cols: usize,
    pub rows: usize,
    pub thumb_w: f32,
    pub thumb_h: f32,
    pub gap_x: f32,
    pub gap_y: f32,
    pub corner_radius: f32,
    pub border_width: f32,
    pub layout: GridLayout,
    pub stagger: f32,
    pub selected_scale: f32,
    pub flow_wave: f32,
    pub flow_frequency: f32,
    pub scatter: f32,
    pub scale_variance: f32,
    pub cylinder_bend: f32,
    pub cylinder_radius: f32,
    pub stage: StageParams,
}

impl Default for GridParams {
    fn default() -> Self {
        Self {
            cols: 6,
            rows: 3,
            thumb_w: 300.0,
            thumb_h: 169.0,
            gap_x: 16.0,
            gap_y: 16.0,
            corner_radius: 12.0,
            border_width: 1.0,
            layout: GridLayout::Uniform,
            stagger: 0.5,
            selected_scale: 1.0,
            flow_wave: 0.0,
            flow_frequency: 1.0,
            scatter: 0.0,
            scale_variance: 0.0,
            cylinder_bend: 0.65,
            cylinder_radius: 720.0,
            stage: StageParams::default(),
        }
    }
}

impl GridParams {
    pub fn topology_differs(&self, target: &GridParams) -> bool {
        self.cols != target.cols || self.rows != target.rows || self.layout != target.layout
    }

    pub fn morph_toward(&mut self, target: &GridParams, amt: f32) {
        self.cols = target.cols;
        self.rows = target.rows;
        self.thumb_w = lerp(self.thumb_w, target.thumb_w, amt);
        self.thumb_h = lerp(self.thumb_h, target.thumb_h, amt);
        self.gap_x = lerp(self.gap_x, target.gap_x, amt);
        self.gap_y = lerp(self.gap_y, target.gap_y, amt);
        self.corner_radius = lerp(self.corner_radius, target.corner_radius, amt);
        self.border_width = lerp(self.border_width, target.border_width, amt);
        self.layout = target.layout;
        self.stagger = lerp(self.stagger, target.stagger, amt);
        self.selected_scale = lerp(self.selected_scale, target.selected_scale, amt);
        self.flow_wave = lerp(self.flow_wave, target.flow_wave, amt);
        self.flow_frequency = lerp(self.flow_frequency, target.flow_frequency, amt);
        self.scatter = lerp(self.scatter, target.scatter, amt);
        self.scale_variance = lerp(self.scale_variance, target.scale_variance, amt);
        self.cylinder_bend = lerp(self.cylinder_bend, target.cylinder_bend, amt);
        self.cylinder_radius = lerp(self.cylinder_radius, target.cylinder_radius, amt);
        self.stage.morph_toward(&target.stage, amt);
    }

    pub fn settled_to(&self, target: &GridParams) -> bool {
        self.cols == target.cols
            && self.rows == target.rows
            && feq(self.thumb_w, target.thumb_w)
            && feq(self.thumb_h, target.thumb_h)
            && feq(self.gap_x, target.gap_x)
            && feq(self.gap_y, target.gap_y)
            && feq(self.corner_radius, target.corner_radius)
            && feq(self.border_width, target.border_width)
            && self.layout == target.layout
            && feq(self.stagger, target.stagger)
            && feq(self.selected_scale, target.selected_scale)
            && feq(self.flow_wave, target.flow_wave)
            && feq(self.flow_frequency, target.flow_frequency)
            && feq(self.scatter, target.scatter)
            && feq(self.scale_variance, target.scale_variance)
            && feq(self.cylinder_bend, target.cylinder_bend)
            && feq(self.cylinder_radius, target.cylinder_radius)
            && self.stage.settled_to(&target.stage)
    }
}

impl GridParams {
    pub fn cylinder_transform(&self, x: f32, y: f32, origin: (f32, f32)) -> (f32, f32, f32) {
        let bend = self.cylinder_bend.clamp(-1.0, 1.0);
        if self.layout != GridLayout::Cylinder || bend.abs() <= 0.001 {
            return (x, y, 1.0);
        }
        let radius = self.cylinder_radius.max(1.0);
        let local_x = x - origin.0;
        let angle = (local_x / radius).clamp(-1.35, 1.35);
        let projected_x = angle.sin() * radius;
        let mix = bend.abs();
        let curved_x = lerp(local_x, projected_x, mix);
        let edge_depth = 1.0 - angle.cos();
        let scale = (1.0 - bend * edge_depth).clamp(0.35, 2.0);
        (origin.0 + curved_x, y, scale)
    }

    pub fn cell_w(&self) -> f32 {
        self.thumb_w + self.gap_x
    }

    pub fn cell_h(&self) -> f32 {
        self.thumb_h + self.gap_y
    }

    pub fn total_w(&self) -> f32 {
        self.span_w(self.cols)
    }

    pub fn total_h(&self) -> f32 {
        self.span_h(self.rows)
    }

    pub fn span_w(&self, columns: usize) -> f32 {
        self.thumb_w * columns as f32 + self.gap_x * columns.saturating_sub(1) as f32
    }

    pub fn span_h(&self, rows: usize) -> f32 {
        self.thumb_h * rows as f32 + self.gap_y * rows.saturating_sub(1) as f32
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Hit {
    pub index: usize,
    pub cx: f32,
    pub cy: f32,
    pub hw: f32,
    pub hh: f32,
    pub skew: f32,
    pub edge_tilt: f32,
    pub hex: bool,
    pub hex_shape: HexShape,
    pub triangle_direction: u8,
}

impl Hit {
    pub fn contains(&self, px: f32, py: f32) -> bool {
        if self.hex {
            let qx = (px - self.cx).abs();
            let qy = (py - self.cy).abs();
            if self.hw <= 0.0
                || self.hh <= 0.0
                || ![self.hw, self.hh, self.cx, self.cy, px, py]
                    .into_iter()
                    .all(f32::is_finite)
            {
                return false;
            }
            if matches!(self.hex_shape, HexShape::Diamond | HexShape::Rhombus) {
                let x = (px - self.cx).abs() / self.hw;
                let y = (py - self.cy).abs() / self.hh;
                return x + y <= 1.0;
            }
            if self.hex_shape == HexShape::Triangle {
                let mut x = (px - self.cx) / self.hw;
                let mut y = (py - self.cy) / self.hh;
                match self.triangle_direction {
                    1 => y = -y,
                    2 => (x, y) = (y, x),
                    3 => (x, y) = (y, -x),
                    _ => {}
                }
                let side = |a: (f32, f32), b: (f32, f32)| {
                    (x - b.0) * (a.1 - b.1) - (a.0 - b.0) * (y - b.1)
                };
                let d1 = side((0.0, -1.0), (1.0, 1.0));
                let d2 = side((1.0, 1.0), (-1.0, 1.0));
                let d3 = side((-1.0, 1.0), (0.0, -1.0));
                let has_negative = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
                let has_positive = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
                return !(has_negative && has_positive);
            }
            return qy <= self.hh && qx / self.hw + 0.5 * qy / self.hh <= 1.0;
        }
        sheared_contains(
            self.cx,
            self.cy,
            self.hw,
            self.hh,
            self.skew,
            self.edge_tilt,
            px,
            py,
        )
    }
}

pub fn sheared_contains(
    cx: f32,
    cy: f32,
    hw: f32,
    hh: f32,
    skew: f32,
    edge_tilt: f32,
    px: f32,
    py: f32,
) -> bool {
    if hw <= 0.0 || hh <= 0.0 {
        return false;
    }
    let sx = skew * 0.5;
    let ty = edge_tilt * 0.5;
    let bx = (hw - sx.abs()).max(1.0);
    let by = (hh - ty.abs()).max(1.0);
    let det = bx * by - sx * ty;
    if det.abs() < 1.0 {
        return false;
    }
    let x = px - cx;
    let y = py - cy;
    let u = (by * x + sx * y) / det;
    let v = (ty * x + bx * y) / det;
    u.abs() <= 1.0 && v.abs() <= 1.0
}

pub fn corner_clamp(corners: [f32; 4], flat_w: f32, slant_len: f32) -> [f32; 4] {
    let rc_max = (flat_w / 2.0 - 1.0).min(slant_len / 2.0 - 1.0).max(0.0);
    corners.map(|r| r.clamp(0.0, rc_max))
}

pub fn slice_clamped_corners(
    corners: [f32; 4],
    w: f32,
    h: f32,
    skew: f32,
    edge_tilt: f32,
) -> [f32; 4] {
    let sk_abs = skew.abs();
    let tilt_abs = edge_tilt.abs();
    let horizontal_edge = ((w - sk_abs).max(0.001).powi(2) + edge_tilt.powi(2)).sqrt();
    let vertical_edge = (skew.powi(2) + (h - tilt_abs).max(0.001).powi(2)).sqrt();
    corner_clamp(corners, horizontal_edge, vertical_edge)
}

pub fn cover_crop(rect_w: f32, rect_h: f32, tex_w: f32, tex_h: f32) -> ([f32; 2], [f32; 2]) {
    let scale = (rect_w / tex_w).max(rect_h / tex_h);
    let vis_w = (rect_w / scale / tex_w).min(1.0);
    let vis_h = (rect_h / scale / tex_h).min(1.0);
    ([(1.0 - vis_w) * 0.5, (1.0 - vis_h) * 0.5], [vis_w, vis_h])
}

pub fn slice_opacity(
    item_center_x: f32,
    view_center_x: f32,
    half_view: f32,
    expanded_layout_w: f32,
    slice_stride: f32,
) -> f32 {
    if half_view <= 0.0 {
        return 1.0;
    }
    let full_zone = (0.6_f32).min((expanded_layout_w / 2.0 + 2.0 * slice_stride) / half_view);
    let norm_dist = (item_center_x - view_center_x).abs() / half_view;
    if norm_dist <= full_zone {
        1.0
    } else {
        (1.0 - (norm_dist - full_zone) / (1.2 - full_zone)).max(0.0)
    }
}

/// Content-space placement emitted by the donor Grid family before Stage and
/// cylinder transforms. The index is the filtered catalog position.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridPlacement {
    pub index: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Adapt the donor view_grid::grid_placements path without SceneCore or atlas
/// ownership. All aspect inputs are presentation-only and must be supplied in
/// filtered order.
pub fn grid_placements(count: usize, params: &GridParams, aspects: &[f32]) -> Vec<GridPlacement> {
    let cols = params.cols.max(1);
    let mut out = match params.layout {
        GridLayout::Uniform | GridLayout::Brick | GridLayout::Cylinder => (0..count)
            .map(|index| {
                let row = index / cols;
                let col = index % cols;
                let stagger = if params.layout == GridLayout::Brick && row % 2 == 1 {
                    params.cell_w() * params.stagger
                } else {
                    0.0
                };
                GridPlacement {
                    index,
                    x: col as f32 * params.cell_w() + stagger,
                    y: row as f32 * params.cell_h(),
                    width: params.thumb_w.max(1.0),
                    height: params.thumb_h.max(1.0),
                }
            })
            .collect(),
        GridLayout::Masonry => {
            let mut bottoms = vec![0.0_f32; cols];
            let mut placements = Vec::with_capacity(count);
            for index in 0..count {
                let column = bottoms
                    .iter()
                    .enumerate()
                    .min_by(|a, b| a.1.total_cmp(b.1))
                    .map_or(0, |(column, _)| column);
                let aspect = aspects
                    .get(index)
                    .copied()
                    .filter(|aspect| aspect.is_finite() && *aspect > 0.0)
                    .unwrap_or(params.thumb_w / params.thumb_h.max(1.0));
                let height = (params.thumb_w / aspect.max(0.15))
                    .clamp(params.thumb_h * 0.55, params.thumb_h * 1.8);
                let y = bottoms[column];
                placements.push(GridPlacement {
                    index,
                    x: column as f32 * params.cell_w(),
                    y,
                    width: params.thumb_w.max(1.0),
                    height,
                });
                bottoms[column] = y + height + params.gap_y;
            }
            placements
        }
        GridLayout::Justified => {
            let mut placements = Vec::with_capacity(count);
            let total_width = params.total_w().max(1.0);
            let mut y = 0.0;
            for start in (0..count).step_by(cols) {
                let end = (start + cols).min(count);
                let row_aspects: Vec<f32> = (start..end)
                    .map(|index| {
                        aspects
                            .get(index)
                            .copied()
                            .filter(|aspect| aspect.is_finite() && *aspect > 0.0)
                            .unwrap_or(params.thumb_w / params.thumb_h.max(1.0))
                            .clamp(0.45, 2.5)
                    })
                    .collect();
                let gaps = params.gap_x * row_aspects.len().saturating_sub(1) as f32;
                let height = ((total_width - gaps) / row_aspects.iter().sum::<f32>().max(0.1))
                    .clamp(params.thumb_h * 0.58, params.thumb_h * 1.5);
                let mut x = 0.0;
                for (offset, aspect) in row_aspects.into_iter().enumerate() {
                    let width = aspect * height;
                    placements.push(GridPlacement {
                        index: start + offset,
                        x,
                        y,
                        width,
                        height,
                    });
                    x += width + params.gap_x;
                }
                y += height + params.gap_y;
            }
            placements
        }
        GridLayout::Editorial => {
            let mut placements = Vec::with_capacity(count);
            let block_height = params.thumb_h * 2.0 + params.gap_y;
            let hero_fraction = (0.5 + 0.16 * params.stagger).clamp(0.35, 0.72);
            let hero_width = (params.total_w() * hero_fraction).max(1.0);
            let side_width = (params.total_w() - hero_width - params.gap_x).max(1.0);
            let small_width = ((side_width - params.gap_x) * 0.5).max(1.0);
            let small_height = ((block_height - params.gap_y) * 0.5).max(1.0);
            for start in (0..count).step_by(5) {
                let block = start / 5;
                let y = block as f32 * (block_height + params.gap_y);
                let hero_right = block % 2 == 1;
                let hero_x = if hero_right {
                    side_width + params.gap_x
                } else {
                    0.0
                };
                placements.push(GridPlacement {
                    index: start,
                    x: hero_x,
                    y,
                    width: hero_width,
                    height: block_height,
                });
                for step in 1..5 {
                    let index = start + step;
                    if index >= count {
                        break;
                    }
                    let slot = step - 1;
                    let x0 = if hero_right {
                        0.0
                    } else {
                        hero_width + params.gap_x
                    };
                    placements.push(GridPlacement {
                        index,
                        x: x0 + (slot % 2) as f32 * (small_width + params.gap_x),
                        y: y + (slot / 2) as f32 * (small_height + params.gap_y),
                        width: small_width,
                        height: small_height,
                    });
                }
            }
            placements
        }
    };
    let flow_span = params.span_h(params.rows.max(1)).max(1.0);
    for placement in &mut out {
        if params.flow_wave.abs() > 0.001 {
            let phase = (placement.y + placement.height * 0.5) / flow_span
                * std::f32::consts::TAU
                * params.flow_frequency;
            placement.x += phase.sin() * params.flow_wave;
        }
        if params.scatter.abs() > 0.001 {
            placement.x += signed_hash(placement.index, 0xc2b2_ae35) * params.scatter;
            placement.y += signed_hash(placement.index, 0x27d4_eb2f) * params.scatter;
        }
        if params.scale_variance.abs() > 0.001 {
            let factor = (1.0 + signed_hash(placement.index, 0x1656_67b1) * params.scale_variance)
                .clamp(0.3, 2.0);
            let next_width = placement.width * factor;
            let next_height = placement.height * factor;
            placement.x += (placement.width - next_width) * 0.5;
            placement.y += (placement.height - next_height) * 0.5;
            placement.width = next_width;
            placement.height = next_height;
        }
    }
    out
}

/// Content-space placement emitted by the donor Hex family.  This is the
/// pure part of `view_hex::rebuild_hex`: camera targeting, curve/stagger
/// offsets, deformation, Stage projection and viewport clipping are kept
/// together, while  remains responsible for catalog identity and poster
/// upload.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HexPlacement {
    pub index: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub scale: f32,
    pub opacity: f32,
    pub shape: HexShape,
    pub triangle_direction: u8,
}

fn hex_curve_offset(params: &HexParams, col_center: f32, origin_x: f32) -> f32 {
    let normalized = (col_center - origin_x) / origin_x.max(1.0);
    let phase = normalized * std::f32::consts::PI * params.curve_frequency;
    (match params.curve {
        HexCurve::Flat | HexCurve::Cylinder => 0.0,
        HexCurve::Arc => -normalized * normalized * params.r,
        HexCurve::Wave => phase.sin() * params.r,
        HexCurve::S => normalized.powi(3) * params.r,
    }) * params.curve_strength
}

/// Adapt the donor `view_hex` placement path without SceneCore ownership.
/// `motion` is 's bounded camera displacement; it is applied before the
/// donor deformation and Stage transform so reversal/retargeting remains a
/// camera concern rather than a per-card translation.
pub fn hex_placements(
    count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: &HexParams,
    motion: f32,
) -> Vec<HexPlacement> {
    hex_placements_inner(count, selected, viewport, params, motion, None)
}

/// Source camera/column springs; the compatibility entry point stays stateless.
pub(crate) fn hex_placements_current(
    count: usize,
    viewport: [f32; 2],
    params: &HexParams,
    camera: f32,
    center_x: f32,
    scales: &std::collections::HashMap<usize, f32>,
) -> Vec<HexPlacement> {
    hex_placements_inner(
        count,
        0,
        viewport,
        params,
        0.0,
        Some((camera, center_x, scales)),
    )
}

fn hex_placements_inner(
    count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: &HexParams,
    motion: f32,
    current: Option<(f32, f32, &std::collections::HashMap<usize, f32>)>,
) -> Vec<HexPlacement> {
    if count == 0 {
        return Vec::new();
    }
    let rows = params.rows.max(1);
    let total_cols = count.div_ceil(rows);
    let selected = selected.min(count - 1);
    let selected_col = selected / rows;
    let origin = current.map_or(
        (viewport[0] * 0.5, viewport[1] * 0.5 + 24.0),
        |(_, cx, _)| (cx, viewport[1] * 0.5),
    );
    let selected_center = params.column_x(selected_col);
    let camera = current.map_or(
        selected_center - motion * params.step_x(),
        |(camera, _, _)| camera,
    );
    let content_h = params.content_h().max(1.0);
    let view_top = origin.1 - content_h * 0.5 + if current.is_some() { 22.5 } else { 0.0 };
    let lens_origin =
        params
            .stage
            .transform(origin.0, origin.1, origin, (viewport[0], viewport[1]));
    let coarse_margin = viewport[0] + params.item_half_w().hypot(params.item_half_h()) * 2.0;
    let mut placements = Vec::with_capacity(count.min(params.cols.max(1) * rows));

    for col in 0..total_cols {
        // beta.24 rebuild_hex omits faded columns from bodies, hits and demand.
        if current.is_some_and(|(_, _, scales)| scales.get(&col).copied().unwrap_or(1.0) <= 0.01) {
            continue;
        }
        let raw_x = origin.0 - camera + params.column_x(col);
        if raw_x < -coarse_margin || raw_x > viewport[0] + coarse_margin {
            continue;
        }
        if current.is_some()
            && params.orbit.abs() > 0.001
            && (raw_x - origin.0).abs()
                > (params.orbit_radius * std::f32::consts::PI * 1.25).max(viewport[0] * 1.75)
        {
            continue;
        }
        let (curve, odd_offset) = (
            hex_curve_offset(params, raw_x, origin.0),
            params.stagger_offset(col),
        );
        for row in 0..rows {
            let index = col * rows + row;
            if index >= count {
                break;
            }
            let raw_y = view_top + params.row_y(row) + odd_offset + curve;
            let (deformed_x, deformed_y) = params.deform(index, raw_x, raw_y, origin);
            let (x, y, stage_scale) =
                params
                    .stage
                    .transform(deformed_x, deformed_y, origin, (viewport[0], viewport[1]));
            let distance = ((x - lens_origin.0).powi(2) + (y - lens_origin.1).powi(2)).sqrt();
            let lens_t = (1.0 - distance / params.lens_radius.max(1.0)).clamp(0.0, 1.0);
            let lens_scale = (1.0 + params.lens * smoothstep(lens_t)).max(0.15);
            let edge = ((x - lens_origin.0).abs() / (viewport[0] * 0.5).max(1.0)).clamp(0.0, 1.5);
            let base_scale = if let Some((_, _, scales)) = current {
                scales.get(&col).copied().unwrap_or(1.0)
            } else if params.curve == HexCurve::Flat {
                1.0
            } else {
                (1.0 - 0.2 * smoothstep((edge - 0.7) / 0.3)).max(0.15)
            };
            let scale = stage_scale * lens_scale * base_scale;
            let width = params.item_half_w() * 2.0 * scale;
            let height = params.item_half_h() * 2.0 * scale;
            if x + width * 0.5 < -10.0
                || x - width * 0.5 > viewport[0] + 10.0
                || y + height * 0.5 < -10.0
                || y - height * 0.5 > viewport[1] + 10.0
            {
                continue;
            }
            let triangle_direction = if params.shape == HexShape::Triangle {
                u8::from(!(row + col).is_multiple_of(2))
            } else {
                0
            };
            placements.push(HexPlacement {
                index,
                x,
                y,
                width,
                height,
                scale,
                opacity: if current.is_some() {
                    scale.clamp(0.0, 1.0)
                } else {
                    base_scale.clamp(0.0, 1.0)
                },
                shape: params.shape,
                triangle_direction,
            });
        }
    }
    placements
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn donor_grid_preserves_filtered_order_and_brick_stagger() {
        let params = GridParams {
            cols: 3,
            rows: 2,
            thumb_w: 100.0,
            thumb_h: 60.0,
            gap_x: 10.0,
            gap_y: 8.0,
            layout: GridLayout::Brick,
            stagger: 0.5,
            ..Default::default()
        };
        let placements = grid_placements(5, &params, &[]);
        assert_eq!(
            placements.iter().map(|p| p.index).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4]
        );
        assert_eq!(placements[0].x, 0.0);
        assert_eq!(placements[3].x, params.cell_w() * 0.5);
        assert_eq!(placements[3].y, params.cell_h());
    }

    #[test]
    fn donor_hex_stage_and_camera_change_projected_cards_without_reordering() {
        let base = HexParams {
            rows: 2,
            cols: 5,
            ..Default::default()
        };
        let settled = hex_placements(10, 2, [1200.0, 800.0], &base, 0.0);
        let mut staged = base;
        staged.stage.offset_x = 0.12;
        staged.stage.scale = 0.9;
        let moved = hex_placements(10, 2, [1200.0, 800.0], &staged, 0.4);
        let settled_indices: Vec<_> = settled.iter().map(|p| p.index).collect();
        let moved_indices: Vec<_> = moved.iter().map(|p| p.index).collect();
        assert!(moved_indices
            .iter()
            .all(|index| settled_indices.contains(index)));
        assert!(moved_indices.contains(&2));
        let a = settled.iter().find(|p| p.index == 2).unwrap();
        let b = moved.iter().find(|p| p.index == 2).unwrap();
        assert_ne!((a.x, a.y, a.width, a.height), (b.x, b.y, b.width, b.height));
        assert!(moved.iter().all(|p| p.x.is_finite() && p.y.is_finite()));
    }

    #[test]
    fn donor_hit_respects_hex_shape_boundary() {
        let hit = Hit {
            index: 0,
            cx: 100.0,
            cy: 100.0,
            hw: 50.0,
            hh: 40.0,
            skew: 0.0,
            edge_tilt: 0.0,
            hex: true,
            hex_shape: HexShape::Diamond,
            triangle_direction: 0,
        };
        assert!(hit.contains(100.0, 100.0));
        assert!(!hit.contains(149.0, 139.0));
    }
}

/// Internal D1 geometry only. Beta.24 view_depth.rs; see the narrow reuse record.
/// Width and spacing are independent physical inputs, with no selected expansion.
#[derive(Debug, Clone, Copy)]
pub(crate) struct DepthParams {
    pub width: f32,
    pub height: f32,
    pub spacing: f32,
    pub visible_count: usize,
    pub falloff: f32,
    pub corner_radius: f32,
    pub skew: f32,
}

impl Default for DepthParams {
    fn default() -> Self {
        Self {
            width: 280.0,
            height: 520.0,
            spacing: 280.0,
            visible_count: 5,
            falloff: 0.05,
            corner_radius: 0.0,
            skew: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct DepthPlacement {
    pub filtered_index: usize,
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub skew: f32,
    pub radii: [f32; 4],
    pub opacity: f32,
    pub parallax: f32,
}

#[derive(Debug, Default)]
pub(crate) struct DepthLayout {
    pub camera: f64,
    pub fit: f32,
    pub candidate_range: Option<(usize, usize)>,
    pub cards: Vec<DepthPlacement>,
}

pub(crate) fn depth_offset(distance: f32, falloff: f32) -> f32 {
    if falloff < 0.0001 {
        distance
    } else {
        distance.signum() * (1.0 + falloff * distance.abs()).ln() / falloff
    }
}

/// Composition is (scene center x, available width); initially the app will
/// supply (viewport width / 2, viewport width). No rail inset is invented here.
pub(crate) fn depth_layout(
    count: usize,
    camera: f32,
    viewport: [f32; 2],
    composition: [f32; 2],
    params: DepthParams,
    entrance: f32,
) -> DepthLayout {
    let mut result = DepthLayout::default();
    if count == 0
        || ![
            camera,
            viewport[0],
            viewport[1],
            composition[0],
            composition[1],
            params.width,
            params.height,
            params.spacing,
            params.falloff,
            params.corner_radius,
            params.skew,
            entrance,
        ]
        .into_iter()
        .all(f32::is_finite)
        || viewport[0] <= 0.0
        || viewport[1] <= 0.0
        || composition[1] <= 0.0
        || params.width <= 0.0
        || params.spacing < 0.0
        || params.falloff < 0.0
    {
        return result;
    }
    // Do window/index arithmetic in f64: a large f32 index's ULP must not
    // expand the bounded window or lose the integer identity of its neighbors.
    let c = (camera as f64).clamp(0.0, (count - 1) as f64);
    let radius = (params.visible_count.clamp(3, 21) as f32 - 1.0) * 0.5;
    let height = params.height.max(1.0);
    let span = depth_offset(radius, params.falloff) * params.spacing + params.width * 0.5;
    let fit = ((composition[1] * 0.5 - 36.0).max(1.0) / span)
        .min(viewport[1] * 0.7 / height)
        .min(1.0);
    let lo = ((c - radius as f64 - 1.0).floor().max(0.0) as usize).min(count - 1);
    let hi = ((c + radius as f64 + 1.0).ceil() as usize).min(count - 1);
    result.camera = c;
    result.fit = fit;
    result.candidate_range = Some((lo, hi));
    let extent = depth_offset(radius + 1.0, params.falloff);
    let mut indices: Vec<_> = (lo..=hi).collect();
    // Stable sort preserves ascending filtered index at equal distances.
    indices.sort_by(|a, b| ((*b as f64 - c).abs()).total_cmp(&(*a as f64 - c).abs()));
    for index in indices {
        let n = (index as f64 - c) as f32;
        let opacity =
            smoothstep((radius + 1.0 - n.abs()).clamp(0.0, 1.0)) * entrance.clamp(0.0, 1.0);
        if opacity <= 0.0001 {
            continue;
        }
        let scale = 1.0 / (1.0 + params.falloff * n.abs());
        let size = [params.width * fit * scale, height * fit * scale];
        let skew =
            (params.skew * fit).clamp(-params.width * fit * 0.3, params.width * fit * 0.3) * scale;
        let offset = depth_offset(n, params.falloff);
        result.cards.push(DepthPlacement {
            filtered_index: index,
            center: [
                composition[0] + offset * params.spacing * fit,
                viewport[1] * 0.5,
            ],
            size,
            skew,
            radii: slice_clamped_corners(
                [params.corner_radius * fit * scale; 4],
                size[0],
                size[1],
                skew,
                0.0,
            ),
            opacity,
            parallax: offset / extent,
        });
    }
    result
}

/// Depth-only refinement of the donor affine envelope. Rounded membership
/// matches the existing item's sheared rounded mask (solid interior, no AA band).
/// This does not change Hit::contains or any existing gallery shape path.
pub(crate) fn depth_contains(
    center: [f32; 2],
    size: [f32; 2],
    skew: f32,
    radii: [f32; 4],
    point: [f32; 2],
) -> bool {
    if !center
        .into_iter()
        .chain(size)
        .chain(radii)
        .chain([skew, point[0], point[1]])
        .all(f32::is_finite)
        || size[0] <= 0.0
        || size[1] <= 0.0
    {
        return false;
    }
    let [hw, hh] = size.map(|v| v * 0.5);
    // The donor distance branch keeps a minimum 1px inverse core even when
    // a roll cuts the width below the skew. The raster quad still bounds the
    // drawn body; its 2px padding is AA, never additional pointer geometry.
    if (point[0] - center[0]).abs() > hw || (point[1] - center[1]).abs() > hh {
        return false;
    }
    if !sheared_contains(center[0], center[1], hw, hh, skew, 0.0, point[0], point[1]) {
        return false;
    }
    let sx = skew * 0.5;
    let bx = (hw - sx.abs()).max(1.0);
    let by = hh.max(1.0);
    let y = point[1] - center[1];
    let x = point[0] - center[0] + sx * y / by;
    let r = radii[match (y < 0.0, x < 0.0) {
        (true, true) => 0,
        (true, false) => 1,
        (false, false) => 2,
        (false, true) => 3,
    }];
    let dx = (x.abs() - bx + r).max(0.0);
    let dy = (y.abs() - by + r).max(0.0);
    dx.hypot(dy) <= r
}

#[cfg(test)]
mod depth_tests {
    use super::*;
    fn contains(card: &DepthPlacement, point: [f32; 2]) -> bool {
        depth_contains(card.center, card.size, card.skew, card.radii, point)
    }
    fn layout(n: usize, c: f32, p: DepthParams, vp: [f32; 2]) -> DepthLayout {
        depth_layout(n, c, vp, [vp[0] / 2.0, vp[0]], p, 1.0)
    }
    fn near(a: f32, b: f64) {
        assert!((a as f64 - b).abs() < 0.002, "{a} != {b}");
    }

    #[test]
    fn d1_source_fit_and_independent_dimensions() {
        // Linear, zero-decay oracle: span = 2*spacing + width/2.
        let p = DepthParams {
            width: 400.0,
            height: 100.0,
            spacing: 200.0,
            falloff: 0.0,
            ..DepthParams::default()
        };
        let a = layout(10, 4.0, p, [800.0, 600.0]);
        near(a.fit, 364.0 / 600.0);
        let center = a.cards.iter().find(|x| x.filtered_index == 4).unwrap();
        near(center.size[0], 400.0 * 364.0 / 600.0);
        near(center.size[1], 100.0 * 364.0 / 600.0);
        let tall = layout(
            10,
            4.0,
            DepthParams {
                height: 1600.0,
                ..p
            },
            [800.0, 600.0],
        );
        near(tall.fit, 420.0 / 1600.0);
        let wider = layout(10, 4.0, DepthParams { width: 800.0, ..p }, [800.0, 600.0]);
        near(wider.fit, 364.0 / 800.0);
        let composition = depth_layout(1, 0.0, [1200.0, 900.0], [750.0, 900.0], p, 1.0);
        assert_eq!(composition.cards[0].center, [750.0, 450.0]);
    }

    #[test]
    fn d1_log_offset_threshold_and_fit_oracle() {
        for f in [0.0, 0.000099, 0.0001, 0.05, 1.0] {
            for n in [-11.0_f32, -2.5, 0.0, 0.5, 11.0] {
                let oracle = if f < 0.0001 {
                    n as f64
                } else {
                    (n as f64).signum() * (f as f64 * (n as f64).abs()).ln_1p() / f as f64
                };
                near(depth_offset(n, f), oracle);
            }
            let p = DepthParams {
                visible_count: 6,
                falloff: f,
                ..DepthParams::default()
            };
            let d = if f < 0.0001 {
                2.5
            } else {
                (1.0_f64 + f as f64 * 2.5).ln() / f as f64
            };
            let oracle = (564.0 / (d * 280.0 + 140.0)).min(630.0 / 520.0).min(1.0);
            near(layout(50, 20.25, p, [1200.0, 900.0]).fit, oracle);
        }
    }

    #[test]
    fn d1_geometry_matrix_window_and_finiteness() {
        for n in [0_usize, 1, 2, 7, 1000, 100_000_000] {
            for c in [0.0, n as f32 * 0.5 + 0.25, n.saturating_sub(1) as f32] {
                for v in [3, 4, 5, 20, 21] {
                    for (w, h, s, f, k, r) in [
                        (40.0, 1600.0, 0.0, 0.0, -200.0, 100.0),
                        (2400.0, 100.0, 4000.0, 1.0, 200.0, 0.0),
                        (280.0, 520.0, 280.0, 0.0001, 0.0, 100.0),
                    ] {
                        for vp in [[1.0, 1.0], [240.0, 900.0], [1920.0, 1080.0]] {
                            let p = DepthParams {
                                width: w,
                                height: h,
                                spacing: s,
                                falloff: f,
                                visible_count: v,
                                skew: k,
                                corner_radius: r,
                            };
                            let result = layout(n, c, p, vp);
                            if n == 0 {
                                assert!(result.cards.is_empty());
                                assert!(result.candidate_range.is_none());
                                continue;
                            }
                            let (lo, hi) = result.candidate_range.unwrap();
                            assert!(lo <= hi && hi < n && hi - lo + 1 <= 24);
                            assert!(result.fit > 0.0 && result.fit <= 1.0);
                            let mut prev = f64::INFINITY;
                            for card in &result.cards {
                                assert!(card.filtered_index >= lo && card.filtered_index <= hi);
                                assert!(card
                                    .center
                                    .into_iter()
                                    .chain(card.size)
                                    .chain(card.radii)
                                    .chain([card.skew, card.opacity, card.parallax])
                                    .all(f32::is_finite));
                                assert!(card.opacity > 0.0001 && card.opacity <= 1.0);
                                assert!(card.size.into_iter().all(|x| x > 0.0));
                                assert!(card.skew.abs() <= card.size[0] * 0.3 + 0.001);
                                let distance = (card.filtered_index as f64 - result.camera).abs();
                                assert!(distance <= prev);
                                prev = distance;
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn d1_fractional_ties_fade_and_no_selected_expansion() {
        let p = DepthParams {
            spacing: 0.0,
            falloff: 0.0,
            ..DepthParams::default()
        };
        let half = layout(10, 4.5, p, [1920.0, 1080.0]);
        assert_eq!(
            half.cards
                .iter()
                .map(|x| x.filtered_index)
                .collect::<Vec<_>>(),
            vec![2, 7, 3, 6, 4, 5]
        );
        assert_eq!(half.cards.last().unwrap().filtered_index, 5);
        assert!(half.cards.iter().all(|x| x.size == half.cards[0].size));
        let quarter = layout(10, 4.25, p, [1920.0, 1080.0]);
        assert_eq!(quarter.cards.last().unwrap().filtered_index, 4);
        assert_eq!(
            quarter
                .cards
                .iter()
                .find(|x| x.filtered_index == 7)
                .unwrap()
                .opacity,
            0.15625
        );
        let threshold = depth_layout(10, 4.0, [1920.0, 1080.0], [960.0, 1920.0], p, 0.0001);
        assert!(threshold.cards.is_empty());
        assert!(layout(0, 0.0, p, [1.0, 1.0]).cards.is_empty());
    }

    #[test]
    fn d1_affine_rounded_membership_independent_probes() {
        for skew in [-60.0, 0.0, 60.0] {
            let card = DepthPlacement {
                filtered_index: 0,
                center: [300.0, 300.0],
                size: [200.0, 240.0],
                skew,
                radii: [30.0; 4],
                opacity: 1.0,
                parallax: 0.0,
            };
            // Construct world points from a rounded box in fitted affine coordinates.
            let bx = 100.0 - skew.abs() / 2.0;
            for (q, expected) in [
                ([0.0, 0.0], true),
                ([bx - 1.0, -119.0], false),
                ([bx - 30.0, -119.0], true),
                ([bx + 1.0, 0.0], false),
                ([bx - 5.0, -115.0], false),
                ([bx - 9.0, -111.0], true),
            ] {
                let world = [300.0 + q[0] - skew * 0.5 * q[1] / 120.0, 300.0 + q[1]];
                assert_eq!(contains(&card, world), expected, "skew={skew}, q={q:?}");
            }
            assert!(!contains(&card, [f32::NAN, 300.0]));
            assert!(!contains(
                &DepthPlacement {
                    size: [0.0, 240.0],
                    ..card
                },
                [300.0, 300.0]
            ));
            let square = DepthPlacement {
                radii: [0.0; 4],
                ..card
            };
            assert!(contains(&square, [300.0 + bx - skew * 0.5, 420.0]));
        }
    }

    #[test]
    fn d1_fitted_corner_and_skew_endpoints() {
        for skew in [-200.0, 200.0] {
            let p = DepthParams {
                width: 100.0,
                height: 200.0,
                spacing: 0.0,
                skew,
                corner_radius: 100.0,
                ..DepthParams::default()
            };
            let full = layout(1, 0.0, p, [1200.0, 900.0]);
            // 0.3 is not exactly representable in f32; use the same bounded
            // physical-pixel tolerance as the independent fit oracle.
            near(full.cards[0].skew, skew.signum() as f64 * 30.0);
            for radius in full.cards[0].radii {
                near(radius, 34.0);
            } // 70px flat edge / 2 - 1
            let fitted = layout(1, 0.0, p, [100.0, 900.0]);
            near(fitted.fit, 0.28);
            near(fitted.cards[0].skew, skew.signum() as f64 * 8.4);
            for radius in fitted.cards[0].radii {
                near(radius, 8.8);
            }
        }
    }
}

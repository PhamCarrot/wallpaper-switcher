use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use iced::widget::shader::{self, Action, Pipeline, Primitive, Viewport};
use iced::{mouse, Event, Point, Rectangle, Size};

use crate::apply_trace;
use crate::donor_layout::{
    self, GridLayout as DonorGridLayout, GridParams as DonorGridParams, HexCurve as DonorHexCurve,
    HexParams as DonorHexParams, HexShape as DonorHexShape, StageParams,
};
use crate::donor_scene::{scene_instances, DonorSceneState, InstanceRaw};
use crate::upstream_slices_adapter::{build_slice_layout, slice_edge_tilt, slice_opacity};

// The gallery keeps a bounded poster set, but selected cards and detail enlarge
// these textures substantially. 512px preserves that bounded cache while
// avoiding the severe magnification blur of the former 256px tier.
pub const POSTER_WIDTH: u32 = 512;
pub const POSTER_HEIGHT: u32 = 512;
pub const MAX_POSTERS: usize = 32;
/// Ordinary modes may grow the same atlas to 256 MiB (512² RGBA per layer).
/// Depth's retained outgoing/live resource contract remains MAX_POSTERS.
pub const MAX_ORDINARY_POSTERS: usize = 256;
static POSTER_LAYER_LIMIT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(MAX_ORDINARY_POSTERS);
pub(crate) fn ordinary_poster_limit() -> usize {
    POSTER_LAYER_LIMIT.load(std::sync::atomic::Ordering::Relaxed)
}

const SLICE_VARIANTS: [&str; 3] = ["CLASSIC", "DENSE", "CINEMA"];
const GEOMETRIC_VARIANTS: [&str; 4] = ["HEX ARC", "TRI WAVE", "SHOWCASE DIAMOND", "RHOMBUS"];
const WALL_VARIANTS: [&str; 6] = [
    "UNIFORM",
    "BRICK",
    "MASONRY",
    "JUSTIFIED",
    "EDITORIAL",
    "CYLINDER",
];
const DEFAULT_POSTER_ASPECT: f32 = 16.0 / 9.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GalleryMode {
    Slices,
    Geometric,
    Wall,
    Depth,
}

impl GalleryMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Slices => "Slices",
            Self::Geometric => "Geometric",
            Self::Wall => "Wall",
            Self::Depth => "Depth",
        }
    }

    pub const fn variant_labels(self) -> &'static [&'static str] {
        match self {
            Self::Slices => &SLICE_VARIANTS,
            Self::Geometric => &GEOMETRIC_VARIANTS,
            Self::Wall => &WALL_VARIANTS,
            Self::Depth => &[],
        }
    }

    pub fn variant_label(self, index: usize) -> &'static str {
        let labels = self.variant_labels();
        if labels.is_empty() {
            "Custom"
        } else {
            labels[index % labels.len()]
        }
    }
}

#[derive(Debug, Clone)]
pub struct Poster {
    pub layer: u32,
    pub source: PathBuf,
    /// Width / height of the decoded image content, before it is fitted to the square texture.
    pub aspect_ratio: f32,
    rgba: Arc<Mutex<Option<Vec<u8>>>>,
    recovery: PosterRecovery,
}

#[derive(Debug, Clone)]
enum PosterRecovery {
    Image(PathBuf),
    CachedRgba {
        cache_path: PathBuf,
        source_path: PathBuf,
    },
    VideoPlaceholder(PathBuf),
}

impl Poster {
    pub(crate) fn same_resource(&self, other: &Self) -> bool {
        self.layer == other.layer
            && self.source == other.source
            && Arc::ptr_eq(&self.rgba, &other.rgba)
    }

    fn current_cached_thumbnail(&self) -> Option<(Vec<u8>, f32)> {
        let PosterRecovery::CachedRgba {
            cache_path,
            source_path,
        } = &self.recovery
        else {
            return None;
        };
        if poster_cache_path(source_path, cache_path.parent()?).as_ref() != Some(cache_path) {
            return None;
        }
        let pixels = self.rgba.lock().ok()?.clone();
        let pixels = pixels.or_else(|| crate::poster_worker::cache::read_poster(cache_path))?;
        Some((pixels, self.aspect_ratio))
    }

    pub fn has_pixels(&self) -> bool {
        self.rgba.lock().is_ok_and(|pixels| pixels.is_some())
    }

    fn take_pixels(&self) -> Option<Vec<u8>> {
        self.rgba.lock().ok()?.take()
    }

    fn pixels_for_upload(&self) -> Option<Vec<u8>> {
        self.take_pixels().or_else(|| match &self.recovery {
            PosterRecovery::Image(path) => decode_poster_pixels(path),
            PosterRecovery::CachedRgba {
                cache_path,
                source_path,
            } => {
                let expected_len = (POSTER_WIDTH * POSTER_HEIGHT * 4) as usize;
                crate::poster_worker::cache::read_poster(cache_path)
                    .filter(|pixels| pixels.len() == expected_len)
                    .or_else(|| decode_poster_pixels(source_path))
            }
            PosterRecovery::VideoPlaceholder(path) => Some(video_placeholder_pixels(path)),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SliceParams {
    pub offset_x: f32,
    pub offset_y: f32,
    pub width: f32,
    pub expanded_width: f32,
    pub height: f32,
    pub spacing: f32,
    /// Horizontal distance between the top and bottom edges, in logical pixels.
    pub skew_distance: f32,
    /// Vertical distance between the left and right edges, in logical pixels.
    pub edge_tilt_distance: f32,
    pub visible_count: usize,
    pub corner_radius: f32,
    pub wobble: bool,
    pub wobble_strength: f32,
    /// Transient signed navigation displacement. Zero means fully settled.
    pub motion: f32,
}

impl Default for SliceParams {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            width: 250.0,
            expanded_width: 420.0,
            height: 420.0,
            spacing: -72.0,
            skew_distance: 48.0,
            edge_tilt_distance: 0.0,
            visible_count: 11,
            corner_radius: 12.0,
            wobble: false,
            wobble_strength: 1.0,
            motion: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GeometricShape {
    #[default]
    Hexagon,
    Circle,
    Triangle,
    Diamond,
    Rhombus,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GeometricCurve {
    Flat,
    #[default]
    Arc,
    Wave,
    S,
    Cylinder,
}

#[derive(Debug, Clone, Copy)]
pub struct GeometricParams {
    pub radius: f32,
    pub rows: usize,
    pub columns: usize,
    pub shape: GeometricShape,
    pub curve: GeometricCurve,
    pub curve_strength: f32,
    pub curve_frequency: f32,
    pub gap_x: f32,
    pub gap_y: f32,
    pub aspect: f32,
    pub stagger: f32,
    pub lens: f32,
    pub orbit: f32,
    /// A non-positive value keeps the viewport-derived cylinder radius.
    pub orbit_radius: f32,
    pub twist: f32,
    pub scatter: f32,
    /// Transient signed navigation displacement. Zero means fully settled.
    pub motion: f32,
    /// Donor Stage state applied after geometric deformation.
    pub stage: StageParams,
}

impl Default for GeometricParams {
    fn default() -> Self {
        Self {
            radius: 140.0,
            rows: 3,
            columns: 7,
            shape: GeometricShape::Hexagon,
            curve: GeometricCurve::Arc,
            curve_strength: 1.2,
            curve_frequency: 1.0,
            gap_x: 6.0,
            gap_y: 6.0,
            aspect: 1.0,
            stagger: 0.5,
            lens: 0.0,
            orbit: 0.0,
            orbit_radius: 0.0,
            twist: 0.0,
            scatter: 0.0,
            motion: 0.0,
            stage: StageParams::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WallLayout {
    Uniform,
    #[default]
    Brick,
    Masonry,
    Justified,
    Editorial,
    Cylinder,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallParams {
    pub columns: usize,
    pub rows: usize,
    pub card_width: f32,
    pub card_height: f32,
    pub gap_x: f32,
    pub gap_y: f32,
    pub corner_radius: f32,
    pub layout: WallLayout,
    pub stagger: f32,
    pub selected_scale: f32,
    pub flow_wave: f32,
    pub flow_frequency: f32,
    pub scatter: f32,
    pub scale_variance: f32,
    pub cylinder_bend: f32,
    pub cylinder_radius: f32,
    /// Persistent absolute Wall camera position in content coordinates.
    pub scroll_offset: f32,
    /// Transient signed navigation displacement. Zero means fully settled.
    pub motion: f32,
    /// Donor Stage state applied after Grid placement and cylinder projection.
    pub stage: StageParams,
    /// Actual horizontal rail footprint; None means no rail in the composition.
    pub rail_height: Option<f32>,
    pub rail_width: Option<f32>,
    pub rail_rtl: bool,
    pub rail_gap: f32,
    pub border_width: f32,
    pub user_engaged: bool,
}

impl Default for WallParams {
    fn default() -> Self {
        Self {
            columns: 6,
            rows: 3,
            card_width: 300.0,
            card_height: 169.0,
            gap_x: 8.0,
            gap_y: 8.0,
            corner_radius: 12.0,
            layout: WallLayout::Uniform,
            stagger: 0.5,
            selected_scale: 1.0,
            flow_wave: 0.0,
            flow_frequency: 1.0,
            scatter: 0.0,
            scale_variance: 0.0,
            cylinder_bend: 0.65,
            cylinder_radius: 720.0,
            scroll_offset: 0.0,
            motion: 0.0,
            stage: StageParams::default(),
            rail_height: Some(24.0),
            rail_width: None,
            rail_rtl: false,
            rail_gap: 8.0,
            border_width: 2.0,
            user_engaged: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GalleryLayoutParams {
    pub slices: SliceParams,
    pub geometric: GeometricParams,
    pub wall: WallParams,
    pub depth: donor_layout::DepthParams,
    pub depth_camera: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliceCard {
    pub index: usize,
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub layer: Option<u32>,
    pub selected: bool,
    pub opacity: f32,
    pub skew_distance: f32,
    pub edge_tilt_distance: f32,
    pub rotation: f32,
    /// Triangle mask direction, independent of poster/world rotation (0 up, 1 down).
    pub triangle_direction: u8,
    pub corner_radius: f32,
    /// 0 = sheared card, 1 = circle, 2 = hexagon, 3 = diamond, 4 = triangle.
    pub shape: u32,
    /// Width / height of the original poster before cover-cropping to this card.
    pub poster_aspect_ratio: f32,
}

#[derive(Debug, Clone)]
pub struct GallerySnapshot {
    pub ordinary: Option<Vec<crate::donor_scene::CardPresentation>>,
    pub cards: Vec<SliceCard>,
    pub posters: Arc<Vec<Poster>>,
    pub colors: GalleryColors,
    /// Donor scene concepts retained at the  adapter boundary. The
    /// renderer consumes the projected cards, while this state keeps Stage,
    /// camera and detail ownership explicit for later controls/motion work.
    pub scene: DonorSceneState,
    pub depth: Option<DepthSnapshot>,
    /// Logical Wall frame shared by shader clipping and input hit gating.
    pub wall_clip: Option<[f32; 4]>,
}

impl GallerySnapshot {
    fn hit(&self, x: f32, y: f32) -> Option<usize> {
        if self.wall_clip.is_some_and(|clip| !inside_clip(clip, x, y)) {
            return None;
        }
        if let Some(entries) = &self.ordinary {
            if entries.iter().any(|entry| entry.source_style) {
                return entries
                    .iter()
                    .rev()
                    .find(|entry| {
                        crate::donor_scene::presented_slice_contains(
                            entry,
                            &self.colors,
                            self.scene.viewport,
                            [x, y],
                        )
                    })
                    .map(|entry| entry.card.index);
            }
        }
        hit_test(&self.cards, x, y)
    }

    fn physical_clip(&self, scale: f32) -> [f32; 4] {
        self.wall_clip
            .map_or([-1.0e9, -1.0e9, 1.0e9, 1.0e9], |clip| {
                clip.map(|v| v * scale)
            })
    }

    /// Every ordinary consumer, including an off-frame external detail, uses
    /// the exact uploaded poster resource rather than a stale atlas layer.
    pub(crate) fn ordinary_resident_instances(
        &self,
        resident: &[Poster],
        scale: f32,
    ) -> Vec<InstanceRaw> {
        let mut snapshot = self.clone();
        if let Some(entries) = &mut snapshot.ordinary {
            entries.retain(|e| {
                e.retained
                    .as_ref()
                    .is_none_or(|p| resident.iter().any(|r| r.same_resource(p)))
            });
            for entry in entries {
                if let Some(layer) = entry.card.layer {
                    let resident = self
                        .posters
                        .iter()
                        .find(|p| p.layer == layer)
                        .is_some_and(|p| resident.iter().any(|r| r.same_resource(p)));
                    if !resident {
                        entry.card.layer = None;
                        entry.readiness = crate::donor_scene::CardReadiness::Unavailable;
                    }
                }
            }
        }
        if let Some(card) = &mut snapshot.scene.detail_card {
            if card.layer.is_some_and(|layer| {
                !self
                    .posters
                    .iter()
                    .find(|p| p.layer == layer)
                    .is_some_and(|p| resident.iter().any(|r| r.same_resource(p)))
            }) {
                card.layer = None;
            }
        }
        snapshot.ordinary_instances(scale)
    }

    fn ordinary_instances(&self, scale: f32) -> Vec<InstanceRaw> {
        if let Some(entries) = &self.ordinary {
            let raw =
                crate::donor_scene::presented_instances(entries, &self.colors, scale, &self.scene);
            return raw;
        }
        let mut raw = scene_instances(&self.cards, &self.colors, scale, &self.scene);
        if self.wall_clip.is_some() {
            // scene_instances emits gallery bodies first, then separate detail
            // scrim/shadow/body. Only gallery bodies use the Wall frame clip.
            for body in raw.iter_mut().take(self.cards.len()) {
                body.misc[2] = 1;
            }
        }
        raw
    }
}

fn inside_clip(clip: [f32; 4], x: f32, y: f32) -> bool {
    x >= clip[0] && y >= clip[1] && x <= clip[2] && y <= clip[3]
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WallFrame {
    pub center: [f32; 2],
    pub top: f32,
    pub left: f32,
    pub clip: [f32; 4],
}

/// beta.24 SceneCore::wall_composition_center, with actual rail orientation.
/// Geometry keeps the full configured frame; clipping intersects the viewport.
pub(crate) fn wall_frame(viewport: [f32; 2], p: WallParams) -> WallFrame {
    let width = (p.card_width.max(1.0) * p.columns.max(1) as f32
        + p.gap_x * p.columns.max(1).saturating_sub(1) as f32)
        .max(1.0);
    let height = (p.card_height.max(1.0) * p.rows.max(1) as f32
        + p.gap_y * p.rows.max(1).saturating_sub(1) as f32)
        .max(1.0);
    let shift_y = p
        .rail_height
        .map_or(0.0, |height| (height + p.rail_gap) * 0.5);
    let shift_x = p.rail_width.map_or(0.0, |width| (width + p.rail_gap) * 0.5)
        * if p.rail_rtl { -1.0 } else { 1.0 };
    let center = [viewport[0] * 0.5 + shift_x, viewport[1] * 0.5 + shift_y];
    let left = center[0] - width * 0.5;
    let top = center[1] - height * 0.5;
    WallFrame {
        center,
        left,
        top,
        clip: [
            left.max(0.0),
            top.max(0.0),
            (left + width).min(viewport[0]),
            (top + height).min(viewport[1]),
        ],
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GalleryColors {
    pub surface: [f32; 3],
    pub accent: [f32; 3],
    pub unselected_brightness: f32,
    pub selection_border_width: f32,
    pub show_selection_border: bool,
    pub border_glow: bool,
    pub border_glow_radius: f32,
}

impl Default for GalleryColors {
    fn default() -> Self {
        Self {
            surface: [0.16, 0.15, 0.22],
            accent: [0.80, 0.65, 0.95],
            unselected_brightness: 0.68,
            selection_border_width: 2.0,
            show_selection_border: true,
            border_glow: false,
            border_glow_radius: 0.0,
        }
    }
}

pub fn parse_hex_rgb(value: &str, fallback: [f32; 3]) -> [f32; 3] {
    let Some([red, green, blue]) = crate::rgb_color::parse_hex_rgb_bytes(value) else {
        return fallback;
    };
    [
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
    ]
}

#[derive(Debug, Clone)]
pub enum GalleryMessage {
    Pointer {
        index: Option<usize>,
        id: Option<String>,
        source: Option<PathBuf>,
        revision: Option<u64>,
        presented_catalog_generation: Option<u64>,
        button: crate::candidate_state::binding_codec::MouseButton,
        modifiers: crate::candidate_state::binding_codec::Mods,
        sequence: Option<u64>,
    },
    Hovered(Option<usize>),
    #[cfg(test)]
    PresentedClick {
        index: usize,
        id: String,
        source: PathBuf,
        catalog_generation: u64,
        sequence: Option<u64>,
    },
    #[cfg(test)]
    Clicked(usize, Option<u64>),
    DepthPointer {
        revision: u64,
        item_id: Option<String>,
        right: bool,
        hover: bool,
    },
    BackdropClicked(Option<u64>),
    Wheel(f32, Option<u64>),
}

pub struct GalleryProgram {
    pub snapshot: Arc<GallerySnapshot>,
}

impl shader::Program<GalleryMessage> for GalleryProgram {
    type State = iced::keyboard::Modifiers;
    type Primitive = GalleryPrimitive;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<GalleryMessage>> {
        if let Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) = event {
            *state = *modifiers;
            return None;
        }
        if let Event::Mouse(mouse::Event::ButtonPressed(button)) = event {
            use crate::candidate_state::binding_codec::{Mods, MouseButton as B};
            let button = match button {
                mouse::Button::Left => B::Left,
                mouse::Button::Right => B::Right,
                mouse::Button::Middle => B::Middle,
                _ => return None,
            };
            let point = cursor.position_in(bounds)?;
            let (index, id, source, revision, presented_catalog_generation) =
                if let Some(frame) = &self.snapshot.depth {
                    let hit = frame.hit(point.x, point.y);
                    (
                        hit.map(|c| c.card.index),
                        hit.map(|c| c.id.clone()),
                        None,
                        Some(frame.revision),
                        None,
                    )
                } else {
                    let index = self.snapshot.hit(point.x, point.y);
                    let entry = index.and_then(|i| {
                        self.snapshot
                            .ordinary
                            .as_ref()?
                            .iter()
                            .find(|e| e.card.index == i)
                    });
                    (
                        index,
                        entry.map(|e| e.id.clone()),
                        entry.map(|e| e.source.clone()),
                        None,
                        entry.map(|e| e.catalog_generation),
                    )
                };
            let sequence = widget_mouse_sequence();
            let name = match button {
                B::Left => "left",
                B::Right => "right",
                B::Middle => "middle",
            };
            trace_mouse_receipt(
                sequence,
                "binding-pointer",
                name,
                "none",
                None,
                cursor,
                bounds,
            );
            let message = GalleryMessage::Pointer {
                index,
                id,
                source,
                revision,
                presented_catalog_generation,
                button,
                modifiers: Mods {
                    ctrl: state.control(),
                    alt: state.alt(),
                    shift: state.shift(),
                    logo: state.logo(),
                },
                sequence,
            };
            trace_mouse_publication(sequence, "binding-pointer", &message);
            return Some(Action::publish(message).and_capture());
        }
        if let Some(frame) = &self.snapshot.depth {
            if matches!(
                event,
                Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
            ) {
                let item_id = cursor
                    .position_in(bounds)
                    .and_then(|p| frame.hit(p.x, p.y))
                    .map(|card| card.id.clone());
                return Some(Action::publish(GalleryMessage::DepthPointer {
                    revision: frame.revision,
                    item_id,
                    right: false,
                    hover: true,
                }));
            }
        }
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                self.snapshot.ordinary.as_ref()?;
                Some(Action::publish(GalleryMessage::Hovered(
                    cursor
                        .position_in(bounds)
                        .and_then(|p| self.snapshot.hit(p.x, p.y)),
                )))
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let (amount, delta_kind, delta_x, delta_y) = match delta {
                    mouse::ScrollDelta::Lines { y, x } => {
                        let amount = if *y == 0.0 { *x } else { *y };
                        (amount, "lines", *x, *y)
                    }
                    mouse::ScrollDelta::Pixels { y, x } => {
                        let amount = if *y == 0.0 { *x / 60.0 } else { *y / 60.0 };
                        (amount, "pixels", *x, *y)
                    }
                };
                let sequence = widget_mouse_sequence();
                trace_mouse_receipt(
                    sequence,
                    "wheel",
                    "none",
                    delta_kind,
                    Some((delta_x, delta_y, amount)),
                    cursor,
                    bounds,
                );
                if amount == 0.0 {
                    trace_mouse_ignored(sequence, "normalized_wheel_delta_zero");
                    return None;
                }
                let message = GalleryMessage::Wheel(amount, sequence);
                trace_mouse_publication(sequence, "wheel", &message);
                Some(Action::publish(message).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        GalleryPrimitive {
            snapshot: self.snapshot.clone(),
        }
    }
}

fn widget_mouse_sequence() -> Option<u64> {
    apply_trace::enabled().then(apply_trace::next_widget_mouse_sequence)
}

fn trace_mouse_receipt(
    sequence: Option<u64>,
    event_kind: &str,
    button: &str,
    delta_kind: &str,
    wheel: Option<(f32, f32, f32)>,
    cursor: mouse::Cursor,
    bounds: Rectangle,
) {
    let Some(sequence) = sequence else {
        return;
    };
    apply_trace::event_lazy("iced-widget-mouse-receipt", || {
        let cursor = cursor.position();
        let (wheel_x, wheel_y, normalized_amount) = wheel.unwrap_or((0.0, 0.0, 0.0));
        format!(
            "sequence={sequence} receipt=iced_widget event_kind={event_kind} button={button} delta_kind={delta_kind} wheel_x={wheel_x} wheel_y={wheel_y} normalized_amount={normalized_amount} cursor_available={} cursor_x={} cursor_y={} bounds_x={} bounds_y={} bounds_width={} bounds_height={}",
            cursor.is_some(),
            cursor.map_or(0.0, |point| point.x),
            cursor.map_or(0.0, |point| point.y),
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
        )
    });
}

fn trace_mouse_publication(sequence: Option<u64>, event_kind: &str, message: &GalleryMessage) {
    let Some(sequence) = sequence else {
        return;
    };
    let (message_kind, index, amount) = match message {
        GalleryMessage::Pointer { index, .. } => ("binding-pointer", *index, None),
        GalleryMessage::Hovered(index) => ("hovered", *index, None),
        #[cfg(test)]
        GalleryMessage::PresentedClick { index, .. } => ("presented-click", Some(*index), None),
        GalleryMessage::DepthPointer { .. } => ("depth-pointer", None, None),
        #[cfg(test)]
        GalleryMessage::Clicked(index, _) => ("clicked", Some(*index), None),
        GalleryMessage::BackdropClicked(_) => ("backdrop-clicked", None, None),
        GalleryMessage::Wheel(amount, _) => ("wheel", None, Some(*amount)),
    };
    apply_trace::event_lazy("iced-widget-mouse-publication", || {
        format!(
            "sequence={sequence} receipt=iced_widget event_kind={event_kind} message_kind={message_kind} card_index={index:?} wheel_amount={amount:?} captured=true"
        )
    });
}

fn trace_mouse_ignored(sequence: Option<u64>, reason: &str) {
    let Some(sequence) = sequence else {
        return;
    };
    apply_trace::event_lazy("iced-widget-mouse-ignored", || {
        format!("sequence={sequence} receipt=iced_widget reason={reason}")
    });
}

pub fn layout_slices(
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: SliceParams,
) -> Vec<SliceCard> {
    if item_count == 0 {
        return Vec::new();
    }
    let Some(layout) = build_slice_layout(
        item_count,
        selected,
        viewport,
        params.offset_x,
        params.offset_y,
        params.width,
        params.expanded_width,
        params.spacing,
        params.skew_distance,
        params.visible_count,
        params.motion,
    ) else {
        return Vec::new();
    };
    let selected = selected.min(item_count - 1);
    let width_of = |index: usize| {
        if index == selected {
            params.expanded_width.max(1.0)
        } else {
            params.width.max(1.0)
        }
    };
    let mut cards: Vec<_> = (layout.start..layout.end)
        .map(|index| {
            let selected_card = index == selected;
            let fade = slice_opacity(
                layout.centers[index - layout.start],
                layout.reference_center_x,
                layout.half_view,
                layout.expanded_layout_width,
                layout.stride,
            );
            SliceCard {
                triangle_direction: 0,
                index,
                center: [layout.centers[index - layout.start], layout.center_y],
                size: [width_of(index), params.height.max(1.0)],
                layer: (index < MAX_POSTERS).then_some(index as u32),
                selected: selected_card,
                opacity: fade.clamp(0.0, 1.0),
                skew_distance: params.skew_distance,
                edge_tilt_distance: slice_edge_tilt(
                    params.edge_tilt_distance,
                    params.motion,
                    params.wobble,
                    params.wobble_strength,
                ),
                rotation: 0.0,
                corner_radius: params.corner_radius.max(0.0),
                shape: 0,
                poster_aspect_ratio: DEFAULT_POSTER_ASPECT,
            }
        })
        .collect();
    selected_last(&mut cards, selected);
    cards
}

pub fn layout_gallery(
    mode: GalleryMode,
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: GalleryLayoutParams,
) -> Vec<SliceCard> {
    layout_gallery_with_aspects(mode, item_count, selected, viewport, params, &[])
}

pub fn layout_gallery_with_aspects(
    mode: GalleryMode,
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: GalleryLayoutParams,
    poster_aspects: &[f32],
) -> Vec<SliceCard> {
    let mut cards = match mode {
        GalleryMode::Slices => layout_slices(item_count, selected, viewport, params.slices),
        GalleryMode::Geometric => {
            layout_geometric(item_count, selected, viewport, params.geometric)
        }
        GalleryMode::Depth => donor_layout::depth_layout(
            item_count,
            params.depth_camera,
            viewport,
            [viewport[0] * 0.5, viewport[0]],
            params.depth,
            1.0,
        )
        .cards
        .into_iter()
        .map(|p| SliceCard {
            triangle_direction: 0,
            index: p.filtered_index,
            center: p.center,
            size: p.size,
            layer: None,
            selected: p.filtered_index == selected,
            opacity: p.opacity,
            skew_distance: p.skew,
            edge_tilt_distance: 0.0,
            rotation: 0.0,
            corner_radius: p.radii[0],
            shape: 0,
            poster_aspect_ratio: valid_aspect(poster_aspects.get(p.filtered_index).copied()),
        })
        .collect(),
        GalleryMode::Wall => {
            layout_wall_with_aspects(item_count, selected, viewport, params.wall, poster_aspects)
        }
    };
    for card in &mut cards {
        card.poster_aspect_ratio = valid_aspect(poster_aspects.get(card.index).copied());
    }
    cards
}

fn layout_geometric(
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: GeometricParams,
) -> Vec<SliceCard> {
    if item_count == 0 {
        return Vec::new();
    }
    let selected = selected.min(item_count - 1);
    if params.shape == GeometricShape::Circle {
        return layout_geometric_circle(item_count, selected, viewport, params);
    }

    let shape = match params.shape {
        GeometricShape::Hexagon => DonorHexShape::Hexagon,
        GeometricShape::Triangle => DonorHexShape::Triangle,
        GeometricShape::Diamond => DonorHexShape::Diamond,
        GeometricShape::Rhombus => DonorHexShape::Rhombus,
        GeometricShape::Circle => unreachable!(),
    };
    let curve = match params.curve {
        GeometricCurve::Flat => DonorHexCurve::Flat,
        GeometricCurve::Arc => DonorHexCurve::Arc,
        GeometricCurve::Wave => DonorHexCurve::Wave,
        GeometricCurve::S => DonorHexCurve::S,
        GeometricCurve::Cylinder => DonorHexCurve::Cylinder,
    };
    let orbit_radius = if params.orbit_radius.is_finite() && params.orbit_radius > 0.0 {
        params.orbit_radius
    } else {
        viewport[0].min(viewport[1]).max(1.0) * 0.55
    };
    let donor = DonorHexParams {
        r: params.radius.max(donor_layout::MIN_HEX_R),
        rows: params.rows.max(1),
        cols: params.columns.max(1),
        scroll_step: 1,
        curve,
        shape,
        curve_strength: params.curve_strength,
        curve_frequency: params.curve_frequency,
        gap_x: params.gap_x,
        gap_y: params.gap_y,
        aspect: params.aspect.max(0.05),
        stagger: params.stagger,
        lens: params.lens,
        lens_radius: orbit_radius,
        orbit: params.orbit,
        orbit_radius,
        twist: params.twist,
        scatter: params.scatter,
        stage: params.stage,
    };
    let mut cards: Vec<_> =
        donor_layout::hex_placements(item_count, selected, viewport, &donor, params.motion)
            .into_iter()
            .map(|placement| SliceCard {
                triangle_direction: placement.triangle_direction,
                index: placement.index,
                center: [placement.x, placement.y],
                size: [placement.width, placement.height],
                layer: (placement.index < MAX_POSTERS).then_some(placement.index as u32),
                selected: placement.index == selected,
                opacity: placement.opacity,
                skew_distance: 0.0,
                edge_tilt_distance: 0.0,
                rotation: 0.0,
                corner_radius: 0.0,
                shape: match placement.shape {
                    DonorHexShape::Hexagon => 2,
                    DonorHexShape::Diamond | DonorHexShape::Rhombus => 3,
                    DonorHexShape::Triangle => 4,
                },
                poster_aspect_ratio: DEFAULT_POSTER_ASPECT,
            })
            .collect();
    selected_last(&mut cards, selected);
    cards
}

/// Circle is a  compatibility shape not present in the pinned donor Hex
/// family. Keep its bounded layout while applying the donor Stage transform
/// and the same camera displacement contract.
fn layout_geometric_circle(
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: GeometricParams,
) -> Vec<SliceCard> {
    let rows = params.rows.max(1);
    let total_columns = item_count.div_ceil(rows);
    let visible_columns = params.columns.max(1).min(total_columns);
    let selected_column = selected / rows;
    let start_column = selected_column
        .saturating_sub(visible_columns / 2)
        .min(total_columns.saturating_sub(visible_columns));
    let end_column = (start_column + visible_columns).min(total_columns);
    let radius = params.radius.max(1.0);
    let aspect = params.aspect.max(0.05);
    let step_x = radius * 2.0 * aspect + params.gap_x;
    let step_y = radius * 2.0 + params.gap_y;
    let origin = (
        viewport[0] * 0.5 + params.motion * step_x,
        viewport[1] * 0.5 + 24.0,
    );
    let stage_origin = (viewport[0] * 0.5, viewport[1] * 0.5 + 24.0);
    let mut cards = Vec::with_capacity(visible_columns * rows);
    for column in start_column..end_column {
        for row in 0..rows {
            let index = column * rows + row;
            if index >= item_count {
                break;
            }
            let x = origin.0 + (column as isize - selected_column as isize) as f32 * step_x;
            let y = origin.1 + (row as f32 - (rows.saturating_sub(1) as f32 * 0.5)) * step_y;
            let (x, y, stage_scale) =
                params
                    .stage
                    .transform(x, y, stage_origin, (viewport[0], viewport[1]));
            cards.push(SliceCard {
                triangle_direction: 0,
                index,
                center: [x, y],
                size: [
                    radius * 2.0 * aspect * stage_scale,
                    radius * 2.0 * stage_scale,
                ],
                layer: (index < MAX_POSTERS).then_some(index as u32),
                selected: index == selected,
                opacity: 1.0,
                skew_distance: 0.0,
                edge_tilt_distance: 0.0,
                rotation: 0.0,
                corner_radius: 0.0,
                shape: 1,
                poster_aspect_ratio: DEFAULT_POSTER_ASPECT,
            });
        }
    }
    selected_last(&mut cards, selected);
    cards
}
fn layout_wall(
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: WallParams,
) -> Vec<SliceCard> {
    layout_wall_with_aspects(item_count, selected, viewport, params, &[])
}

pub(crate) fn wall_placements(
    item_count: usize,
    params: WallParams,
    poster_aspects: &[f32],
) -> (DonorGridParams, Vec<donor_layout::GridPlacement>) {
    let layout = match params.layout {
        WallLayout::Uniform => DonorGridLayout::Uniform,
        WallLayout::Brick => DonorGridLayout::Brick,
        WallLayout::Masonry => DonorGridLayout::Masonry,
        WallLayout::Justified => DonorGridLayout::Justified,
        WallLayout::Editorial => DonorGridLayout::Editorial,
        WallLayout::Cylinder => DonorGridLayout::Cylinder,
    };
    let donor = DonorGridParams {
        cols: params.columns.max(1),
        rows: params.rows.max(1),
        thumb_w: params.card_width.max(1.0),
        thumb_h: params.card_height.max(1.0),
        gap_x: params.gap_x,
        gap_y: params.gap_y,
        corner_radius: params.corner_radius.max(0.0),
        border_width: params.border_width,
        layout,
        stagger: params.stagger,
        selected_scale: params.selected_scale,
        flow_wave: params.flow_wave,
        flow_frequency: params.flow_frequency,
        scatter: params.scatter,
        scale_variance: params.scale_variance,
        cylinder_bend: params.cylinder_bend,
        cylinder_radius: params.cylinder_radius,
        stage: params.stage,
    };
    let placements = donor_layout::grid_placements(item_count, &donor, poster_aspects);
    (donor, placements)
}

fn layout_wall_with_aspects(
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: WallParams,
    poster_aspects: &[f32],
) -> Vec<SliceCard> {
    if item_count == 0 {
        return Vec::new();
    }
    let selected = selected.min(item_count - 1);
    let (donor, placements) = wall_placements(item_count, params, poster_aspects);
    let frame_height = donor.total_h().max(1.0);
    let content_bottom = placements
        .iter()
        .map(|placement| placement.y + placement.height)
        .fold(0.0_f32, f32::max);
    let max_scroll = (content_bottom - frame_height).max(0.0);
    let camera = params.scroll_offset.clamp(0.0, max_scroll);
    let frame = wall_frame(viewport, params);
    let frame_center = frame.center;
    let left = frame.left;
    let cards: Vec<_> = placements
        .into_iter()
        .filter_map(|placement| {
            let raw_x = left + placement.x + placement.width * 0.5;
            let raw_y =
                frame.top + params.motion * donor.cell_h() + placement.y + placement.height * 0.5
                    - camera;
            let (surface_x, surface_y, surface_scale) =
                donor.cylinder_transform(raw_x, raw_y, (frame_center[0], frame_center[1]));
            let (center_x, center_y, stage_scale) = donor.stage.transform(
                surface_x,
                surface_y,
                (frame_center[0], frame_center[1]),
                (viewport[0], viewport[1]),
            );
            let focus = if placement.index == selected && params.user_engaged {
                params.selected_scale.max(0.1)
            } else {
                1.0
            };
            let draw_width = placement.width * surface_scale * stage_scale * focus;
            let draw_height = placement.height * surface_scale * stage_scale * focus;
            if center_x + draw_width * 0.5 <= frame.clip[0]
                || center_x - draw_width * 0.5 >= frame.clip[2]
                || center_y + draw_height * 0.5 <= frame.clip[1]
                || center_y - draw_height * 0.5 >= frame.clip[3]
            {
                return None;
            }
            Some(SliceCard {
                triangle_direction: 0,
                index: placement.index,
                center: [center_x, center_y],
                size: [draw_width, draw_height],
                layer: (placement.index < MAX_POSTERS).then_some(placement.index as u32),
                selected: placement.index == selected,
                opacity: 1.0,
                skew_distance: 0.0,
                edge_tilt_distance: 0.0,
                rotation: 0.0,
                corner_radius: params.corner_radius.max(0.0),
                shape: 0,
                poster_aspect_ratio: valid_aspect(poster_aspects.get(placement.index).copied()),
            })
        })
        .collect();
    cards
}

fn valid_aspect(aspect: Option<f32>) -> f32 {
    aspect
        .filter(|aspect| aspect.is_finite() && *aspect > 0.0)
        .unwrap_or(DEFAULT_POSTER_ASPECT)
        .clamp(0.01, 100.0)
}

fn selected_last(cards: &mut [SliceCard], selected: usize) {
    cards.sort_by_key(|card| {
        if card.index == selected {
            usize::MAX
        } else {
            usize::MAX - card.index.abs_diff(selected)
        }
    });
}

pub fn hit_test(cards: &[SliceCard], x: f32, y: f32) -> Option<usize> {
    cards
        .iter()
        .rev()
        .find(|card| contains_point(card, x, y))
        .map(|card| card.index)
}

fn contains_point(card: &SliceCard, x: f32, y: f32) -> bool {
    let world_x = x - card.center[0];
    let world_y = y - card.center[1];
    let (sin, cos) = (-card.rotation).sin_cos();
    let point_x = world_x * cos - world_y * sin;
    let point_y = world_x * sin + world_y * cos;
    let half_x = card.size[0] * 0.5;
    let half_y = card.size[1] * 0.5;
    // The shader rotates placement, leaving shape coordinates local. Use the
    // donor's independently scaled flat-top polygon for every Hex rotation,
    // including tiny rotations and sub-pixel positive extents.
    if card.shape == 2 {
        return donor_layout::Hit {
            index: card.index,
            cx: 0.0,
            cy: 0.0,
            hw: half_x,
            hh: half_y,
            skew: 0.0,
            edge_tilt: 0.0,
            hex: true,
            hex_shape: DonorHexShape::Hexagon,
            triangle_direction: 0,
        }
        .contains(point_x, point_y);
    }
    if card.rotation.abs() < 0.0001 {
        match card.shape {
            0 => {
                return {
                    donor_layout::Hit {
                        index: card.index,
                        cx: card.center[0],
                        cy: card.center[1],
                        hw: half_x,
                        hh: half_y,
                        skew: card.skew_distance,
                        edge_tilt: card.edge_tilt_distance,
                        hex: false,
                        hex_shape: DonorHexShape::Hexagon,
                        triangle_direction: 0,
                    }
                    .contains(x, y)
                }
            }
            3 | 4 => {
                let donor_shape = match card.shape {
                    3 => DonorHexShape::Diamond,
                    _ => DonorHexShape::Triangle,
                };
                return donor_layout::Hit {
                    index: card.index,
                    cx: card.center[0],
                    cy: card.center[1],
                    hw: half_x,
                    hh: half_y,
                    skew: 0.0,
                    edge_tilt: 0.0,
                    hex: true,
                    hex_shape: donor_shape,
                    triangle_direction: card.triangle_direction.min(1),
                }
                .contains(x, y);
            }
            _ => {}
        }
    }
    let normalized_x = point_x / half_x.max(1.0);
    let normalized_y = point_y / half_y.max(1.0);
    match card.shape {
        1 => return normalized_x * normalized_x + normalized_y * normalized_y <= 1.0,
        3 => return normalized_x.abs() + normalized_y.abs() <= 1.0,
        4 => {
            let y = if card.triangle_direction == 0 {
                normalized_y
            } else {
                -normalized_y
            };
            return ((2.0 * normalized_x.abs() - y - 1.0) / 2.236_068).max(y - 1.0) <= 0.0;
        }
        _ => {}
    }
    let skew_half = card.skew_distance * 0.5;
    let tilt_half = card.edge_tilt_distance * 0.5;
    let body_x = (half_x - skew_half.abs()).max(1.0);
    let body_y = (half_y - tilt_half.abs()).max(1.0);
    let determinant = body_x * body_y - skew_half * tilt_half;
    if determinant.abs() < 1.0 {
        return false;
    }
    let transformed_x = (body_y * point_x + skew_half * point_y) / determinant * body_x;
    let transformed_y = (tilt_half * point_x + body_x * point_y) / determinant * body_y;
    transformed_x.abs() <= body_x && transformed_y.abs() <= body_y
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    resolution: [f32; 2],
    time: f32,
    visibility: f32,
    clip: [f32; 4],
}

#[derive(Debug)]
pub struct GalleryPrimitive {
    snapshot: Arc<GallerySnapshot>,
}

pub struct GalleryPipeline {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    instance_count: u32,
    posters: wgpu::Texture,
    preview: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    bind_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uploaded_posters: Arc<Vec<Poster>>,
    resident_posters: Vec<Poster>,
}

impl Pipeline for GalleryPipeline {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        POSTER_LAYER_LIMIT.store(
            (device.limits().max_texture_array_layers as usize).min(MAX_ORDINARY_POSTERS),
            std::sync::atomic::Ordering::Relaxed,
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("skwd donor item presentation"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/skwd_item.wgsl").into()),
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("skwd donor scene globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let posters = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("skwd donor far poster array"),
            size: wgpu::Extent3d {
                width: POSTER_WIDTH,
                height: POSTER_HEIGHT,
                depth_or_array_layers: MAX_POSTERS as u32,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let preview = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("skwd donor preview fallback"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &preview,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0, 0, 0, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let poster_view = posters.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let preview_view = preview.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("skwd donor scene sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skwd donor scene bind layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Slices bind group"),
            layout: &bind_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&poster_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&poster_view),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::TextureView(&preview_view),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skwd donor scene pipeline layout"),
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x4,
                1 => Float32x4,
                2 => Float32x4,
                3 => Float32x4,
                4 => Float32x4,
                5 => Float32x4,
                6 => Float32x4,
                7 => Float32x4,
                8 => Uint32x4,
                9 => Float32x4,
                10 => Float32x4,
            ],
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("skwd donor item render pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[instance_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let instance_capacity = 16usize;
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("skwd donor scene instances"),
            size: (instance_capacity * std::mem::size_of::<InstanceRaw>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            globals,
            instances,
            instance_capacity,
            instance_count: 0,
            posters,
            preview,
            bind_group,
            bind_layout,
            sampler,
            uploaded_posters: Arc::new(Vec::new()),
            resident_posters: Vec::new(),
        }
    }
}

impl Primitive for GalleryPrimitive {
    type Pipeline = GalleryPipeline;

    fn prepare(
        &self,
        pipeline: &mut GalleryPipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let scale = viewport.scale_factor();
        let _timing = crate::apply_trace::cpu_span("detail-gpu-prepare", || {
            format!("posters={}", self.snapshot.posters.len())
        });
        let globals = Globals {
            resolution: [
                viewport.physical_width() as f32,
                viewport.physical_height() as f32,
            ],
            time: self.snapshot.scene.time,
            visibility: self.snapshot.scene.visibility.clamp(0.0, 1.0),
            clip: self.snapshot.physical_clip(scale),
        };
        queue.write_buffer(&pipeline.globals, 0, bytemuck::bytes_of(&globals));

        let required = self
            .snapshot
            .posters
            .iter()
            .map(|p| p.layer as usize + 1)
            .max()
            .unwrap_or(MAX_POSTERS);
        let wanted = required
            .max(MAX_POSTERS)
            .next_power_of_two()
            .min(ordinary_poster_limit());
        if wanted > pipeline.posters.size().depth_or_array_layers as usize {
            // Replace the existing atlas and rehydrate every current resource.
            // The old GPU allocation can coexist until submission retirement;
            // Each atlas/decoded owner is capped at 256 MiB. GUI, mailbox and
            // in-flight decoded sets plus retired GPU arrays may coexist;
            // this is a resource-count limit, not a measured process ceiling.
            pipeline.posters = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("grown ordinary poster array"),
                size: wgpu::Extent3d {
                    width: POSTER_WIDTH,
                    height: POSTER_HEIGHT,
                    depth_or_array_layers: wanted as u32,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            });
            let posters = pipeline.posters.create_view(&wgpu::TextureViewDescriptor {
                dimension: Some(wgpu::TextureViewDimension::D2Array),
                ..Default::default()
            });
            let preview = pipeline
                .preview
                .create_view(&wgpu::TextureViewDescriptor::default());
            pipeline.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("grown ordinary poster binding"),
                layout: &pipeline.bind_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: pipeline.globals.as_entire_binding(),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::TextureView(&posters),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&posters),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&pipeline.sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::TextureView(&preview),
                    },
                ],
            });
            pipeline.resident_posters.clear();
            pipeline.uploaded_posters = Arc::new(Vec::new());
            apply_trace::event_lazy("gallery-atlas-growth", || {
                format!(
                    "layers={wanted} bytes={} device_limit={}",
                    wanted * POSTER_WIDTH as usize * POSTER_HEIGHT as usize * 4,
                    device.limits().max_texture_array_layers
                )
            });
        }

        if !Arc::ptr_eq(&pipeline.uploaded_posters, &self.snapshot.posters) {
            pipeline.resident_posters.retain(|resident| {
                self.snapshot
                    .posters
                    .iter()
                    .any(|wanted| wanted.same_resource(resident))
            });
            for poster in self.snapshot.posters.iter().take(MAX_ORDINARY_POSTERS) {
                if poster.layer >= pipeline.posters.size().depth_or_array_layers {
                    continue;
                }
                if pipeline
                    .resident_posters
                    .iter()
                    .any(|resident| resident.same_resource(poster))
                {
                    continue;
                }
                let Some(rgba) = poster.pixels_for_upload() else {
                    continue;
                };
                if rgba.len() != (POSTER_WIDTH * POSTER_HEIGHT * 4) as usize {
                    continue;
                }
                queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &pipeline.posters,
                        mip_level: 0,
                        origin: wgpu::Origin3d {
                            x: 0,
                            y: 0,
                            z: poster.layer,
                        },
                        aspect: wgpu::TextureAspect::All,
                    },
                    rgba.as_slice(),
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(POSTER_WIDTH * 4),
                        rows_per_image: Some(POSTER_HEIGHT),
                    },
                    wgpu::Extent3d {
                        width: POSTER_WIDTH,
                        height: POSTER_HEIGHT,
                        depth_or_array_layers: 1,
                    },
                );
                pipeline.resident_posters.push(poster.clone());
            }
            pipeline.uploaded_posters = self.snapshot.posters.clone();
        }

        let raw = if let Some(depth) = &self.snapshot.depth {
            depth_resident_instances(
                depth,
                &self.snapshot.posters,
                &pipeline.resident_posters,
                &self.snapshot.colors,
                scale,
                &self.snapshot.scene,
            )
        } else {
            self.snapshot
                .ordinary_resident_instances(&pipeline.resident_posters, scale)
        };
        if raw.len() > pipeline.instance_capacity {
            pipeline.instance_capacity = raw.len().next_power_of_two();
            pipeline.instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("skwd donor scene instances"),
                size: (pipeline.instance_capacity * std::mem::size_of::<InstanceRaw>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !raw.is_empty() {
            queue.write_buffer(&pipeline.instances, 0, bytemuck::cast_slice(&raw));
        }
        pipeline.instance_count = raw.len() as u32;
    }

    fn draw(&self, _pipeline: &GalleryPipeline, _render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        false
    }

    fn render(
        &self,
        pipeline: &GalleryPipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        if pipeline.instance_count == 0 {
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Slices pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.set_vertex_buffer(0, pipeline.instances.slice(..));
        pass.draw(0..6, 0..pipeline.instance_count);
    }
}

pub fn decode_poster(path: &std::path::Path, layer: u32) -> Option<Poster> {
    let (rgba, aspect_ratio) = decode_poster_pixels_with_aspect(path)?;
    Some(Poster {
        layer,
        source: path.to_path_buf(),
        aspect_ratio,
        rgba: Arc::new(Mutex::new(Some(rgba))),
        recovery: PosterRecovery::Image(path.to_path_buf()),
    })
}

fn decode_poster_pixels(path: &std::path::Path) -> Option<Vec<u8>> {
    decode_poster_pixels_with_aspect(path).map(|(rgba, _)| rgba)
}

/// Settings uses the existing poster pixels without their atlas padding.
pub(crate) fn still_thumbnail(
    path: &std::path::Path,
    resident: Option<&Poster>,
) -> Option<iced::widget::image::Handle> {
    let (rgba, aspect) = resident
        .and_then(Poster::current_cached_thumbnail)
        .or_else(|| decode_poster_pixels_with_aspect(path))?;
    let canvas = image::RgbaImage::from_raw(POSTER_WIDTH, POSTER_HEIGHT, rgba)?;
    let width = (POSTER_HEIGHT as f32 * aspect)
        .round()
        .clamp(1.0, POSTER_WIDTH as f32) as u32;
    let height = (POSTER_WIDTH as f32 / aspect)
        .round()
        .clamp(1.0, POSTER_HEIGHT as f32) as u32;
    let crop = image::imageops::crop_imm(
        &canvas,
        (POSTER_WIDTH - width) / 2,
        (POSTER_HEIGHT - height) / 2,
        width,
        height,
    );
    Some(iced::widget::image::Handle::from_rgba(
        width,
        height,
        crop.to_image().into_raw(),
    ))
}

fn decode_poster_pixels_with_aspect(path: &std::path::Path) -> Option<(Vec<u8>, f32)> {
    let source = image::open(path).ok()?.to_rgba8();
    if source.width() == 0 || source.height() == 0 {
        return None;
    }
    let aspect_ratio = source.width() as f32 / source.height() as f32;
    let fitted = image::DynamicImage::ImageRgba8(source)
        .resize(
            POSTER_WIDTH,
            POSTER_HEIGHT,
            image::imageops::FilterType::Triangle,
        )
        .to_rgba8();
    let mut canvas =
        image::RgbaImage::from_pixel(POSTER_WIDTH, POSTER_HEIGHT, image::Rgba([30, 30, 46, 255]));
    let left = ((POSTER_WIDTH - fitted.width()) / 2) as i64;
    let top = ((POSTER_HEIGHT - fitted.height()) / 2) as i64;
    image::imageops::overlay(&mut canvas, &fitted, left, top);
    Some((canvas.into_raw(), aspect_ratio))
}

pub fn decode_poster_cached(
    path: &std::path::Path,
    layer: u32,
    cache_dir: &std::path::Path,
) -> Option<Poster> {
    let (source_width, source_height) = image::image_dimensions(path).ok()?;
    if source_width == 0 || source_height == 0 {
        return None;
    }
    let aspect_ratio = source_width as f32 / source_height as f32;
    let _cache_guard = match crate::poster_worker::cache::RootGuard::open(
        &crate::config::Config::config_dir(),
        cache_dir,
        true,
    ) {
        Ok(Some(guard)) => guard,
        _ => return decode_poster(path, layer),
    };
    let cache_path = poster_cache_path(path, cache_dir)?;
    decode_poster_at(path, layer, cache_dir, cache_path, aspect_ratio)
}

pub fn poster_cache_path(path: &std::path::Path, cache_dir: &std::path::Path) -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    const CACHE_VERSION: u8 = 3;
    let metadata = path.metadata().ok()?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    CACHE_VERSION.hash(&mut hasher);
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
        .hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    metadata.modified().ok().hash(&mut hasher);
    POSTER_WIDTH.hash(&mut hasher);
    POSTER_HEIGHT.hash(&mut hasher);
    Some(cache_dir.join(format!("poster-{:016x}.rgba", hasher.finish())))
}

fn publish_poster_cache(cache_path: &std::path::Path, pixels: &[u8], nonce: u64) {
    let temporary = cache_path.with_extension(format!("rgba.{nonce}.tmp"));
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
    {
        use std::io::Write;
        let written = file.write_all(pixels).is_ok();
        drop(file);
        // Link publication is create-only: neither an unknown destination nor a temp collision is replaced.
        if written {
            let _ = std::fs::hard_link(&temporary, cache_path);
        }
        let _ = std::fs::remove_file(&temporary);
    }
}

fn decode_poster_at(
    path: &std::path::Path,
    layer: u32,
    _cache_dir: &std::path::Path,
    cache_path: PathBuf,
    aspect_ratio: f32,
) -> Option<Poster> {
    let expected_len = (POSTER_WIDTH * POSTER_HEIGHT * 4) as usize;
    let rgba = crate::poster_worker::cache::read_poster(&cache_path)
        .filter(|pixels| pixels.len() == expected_len)
        .or_else(|| {
            let (pixels, _) = decode_poster_pixels_with_aspect(path)?;
            static TEMP_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
            publish_poster_cache(
                &cache_path,
                &pixels,
                TEMP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            );
            Some(pixels)
        })?;
    Some(Poster {
        layer,
        source: path.to_path_buf(),
        aspect_ratio,
        rgba: Arc::new(Mutex::new(Some(rgba))),
        recovery: PosterRecovery::CachedRgba {
            cache_path,
            source_path: path.to_path_buf(),
        },
    })
}

pub fn video_placeholder_poster(path: &std::path::Path, layer: u32) -> Poster {
    Poster {
        layer,
        source: path.to_path_buf(),
        aspect_ratio: DEFAULT_POSTER_ASPECT,
        rgba: Arc::new(Mutex::new(Some(video_placeholder_pixels(path)))),
        recovery: PosterRecovery::VideoPlaceholder(path.to_path_buf()),
    }
}

fn video_placeholder_pixels(path: &std::path::Path) -> Vec<u8> {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy()
        .to_ascii_lowercase()
        .hash(&mut hasher);
    let sample = hasher.finish();
    let base = [
        45 + (sample & 0x3f) as u8,
        42 + ((sample >> 8) & 0x3f) as u8,
        64 + ((sample >> 16) & 0x4f) as u8,
    ];
    let mut rgba = vec![0; (POSTER_WIDTH * POSTER_HEIGHT * 4) as usize];
    for y in 0..POSTER_HEIGHT {
        let shade = (y * 36 / POSTER_HEIGHT) as u8;
        for x in 0..POSTER_WIDTH {
            let offset = ((y * POSTER_WIDTH + x) * 4) as usize;
            rgba[offset] = base[0].saturating_add(shade);
            rgba[offset + 1] = base[1].saturating_add(shade);
            rgba[offset + 2] = base[2].saturating_add(shade);
            rgba[offset + 3] = 255;
        }
    }
    rgba
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pk08_thumbnail_reloads_an_uncached_resident_at_the_same_path() {
        let path =
            std::env::temp_dir().join(format!("pk08-still-replacement-{}.png", std::process::id()));
        let write = |w, h, rgba| {
            image::RgbaImage::from_pixel(w, h, image::Rgba(rgba))
                .save(&path)
                .unwrap()
        };
        write(16, 8, [240, 0, 0, 255]);
        let resident = decode_poster(&path, 0).unwrap();
        write(8, 16, [0, 0, 240, 255]);
        let thumbnail = still_thumbnail(&path, Some(&resident)).unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(
            matches!(thumbnail, iced::widget::image::Handle::Rgba {width: 256, height: 512, pixels, ..}
            if pixels[..4] == [0, 0, 240, 255])
        );
    }

    // An independent edge/cross-product oracle, expressed as six vertices.
    // Distances are to finite segments, not the implementation's half planes.
    fn r8_hex_oracle(p: [f32; 2], half: [f32; 2]) -> (bool, f32) {
        let vertices = [
            [half[0], 0.0],
            [half[0] * 0.5, half[1]],
            [-half[0] * 0.5, half[1]],
            [-half[0], 0.0],
            [-half[0] * 0.5, -half[1]],
            [half[0] * 0.5, -half[1]],
        ];
        let mut inside = true;
        let mut distance = f32::INFINITY;
        for i in 0..6 {
            let a = vertices[i];
            let b = vertices[(i + 1) % 6];
            let edge = [b[0] - a[0], b[1] - a[1]];
            let q = [p[0] - a[0], p[1] - a[1]];
            inside &= edge[0] * q[1] - edge[1] * q[0] >= 0.0;
            let t = ((q[0] * edge[0] + q[1] * edge[1]) / (edge[0] * edge[0] + edge[1] * edge[1]))
                .clamp(0.0, 1.0);
            distance = distance.min((q[0] - t * edge[0]).hypot(q[1] - t * edge[1]));
        }
        (inside, distance)
    }

    fn r8_world(card: &SliceCard, local: [f32; 2]) -> [f32; 2] {
        let (sin, cos) = card.rotation.sin_cos();
        [
            card.center[0] + cos * local[0] - sin * local[1],
            card.center[1] + sin * local[0] + cos * local[1],
        ]
    }

    #[test]
    fn r8_hex_polygon_contract() {
        for aspect in [0.05, 0.5, 1.0, 2.0, 3.75, 10.0] {
            let cards = layout_geometric(
                1,
                0,
                [1200.0, 800.0],
                GeometricParams {
                    radius: 100.0,
                    aspect,
                    ..Default::default()
                },
            );
            let mut card = cards[0].clone();
            let half = [card.size[0] * 0.5, card.size[1] * 0.5];
            for rotation in [0.0, 0.00005, 0.37, -1.1, std::f32::consts::PI] {
                card.rotation = rotation;
                for ix in -24..=24 {
                    for iy in -24..=24 {
                        let local = [ix as f32 / 20.0 * half[0], iy as f32 / 20.0 * half[1]];
                        let (inside, edge_distance) = r8_hex_oracle(local, half);
                        // A bounded roundoff band, not fractional-alpha equivalence.
                        if edge_distance < 0.002 {
                            continue;
                        }
                        let p = r8_world(&card, local);
                        assert_eq!(
                            hit_test(&[card.clone()], p[0], p[1]),
                            inside.then_some(card.index),
                            "aspect={aspect}, rotation={rotation}, local={local:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn r8_recorded_points_vertices_and_edge_band() {
        for aspect in [0.05, 0.5, 1.0, 2.0, 10.0] {
            let mut card = layout_geometric(
                1,
                0,
                [1200.0, 800.0],
                GeometricParams {
                    radius: 100.0,
                    aspect,
                    ..Default::default()
                },
            )[0]
            .clone();
            let half = [100.0 * aspect, 87.0];
            assert_eq!(card.size, [half[0] * 2.0, half[1] * 2.0]);
            // Original review point: the old aspect-2 shader was visibly inside.
            // The opposite mismatch: this stretched interior was masked away
            // by the old aspect-0.5 shader despite being clickable.
            for rotation in [0.0, 0.63, -1.2] {
                card.rotation = rotation;
                for sx in [-1.0, 1.0] {
                    for sy in [-1.0, 1.0] {
                        for (normalized, expected) in [([0.7, 0.9], false), ([0.4, 0.8], true)] {
                            let p = r8_world(
                                &card,
                                [sx * normalized[0] * half[0], sy * normalized[1] * half[1]],
                            );
                            assert_eq!(
                                hit_test(&[card.clone()], p[0], p[1]),
                                expected.then_some(0)
                            );
                        }
                        // Top edge, slope midpoint, top corner and side vertex.
                        // Exact fractional alpha/hits within 0.25 logical pixels
                        // of the edge are intentionally not equated.
                        for edge in [[0.0, 1.0], [0.25, 1.0], [0.5, 1.0], [0.75, 0.5], [1.0, 0.0]] {
                            for offset in [-0.25, 0.25] {
                                let local = [sx * edge[0] * half[0], sy * edge[1] * half[1]];
                                let length = local[0].hypot(local[1]);
                                let p = [
                                    local[0] * (1.0 + offset / length),
                                    local[1] * (1.0 + offset / length),
                                ];
                                let (inside, distance) = r8_hex_oracle(p, half);
                                assert!(distance > 0.002);
                                let world = r8_world(&card, p);
                                assert_eq!(
                                    hit_test(&[card.clone()], world[0], world[1]),
                                    inside.then_some(0)
                                );
                            }
                        }
                    }
                }
            }
            // Exactly representable local boundary coordinates, with no
            // translation/inverse-rotation rounding at this check.
            card.center = [0.0; 2];
            card.rotation = 0.0;
            for point in [
                [half[0], 0.0],
                [-half[0], 0.0],
                [half[0] * 0.5, half[1]],
                [-half[0] * 0.5, half[1]],
                [half[0] * 0.5, -half[1]],
                [-half[0] * 0.5, -half[1]],
                [0.0, half[1]],
                [0.0, -half[1]],
            ] {
                assert_eq!(hit_test(&[card.clone()], point[0], point[1]), Some(0));
            }
        }
    }

    #[test]
    fn r8_hex_deformed_layout_and_instance_contract() {
        for curve in [
            GeometricCurve::Flat,
            GeometricCurve::Arc,
            GeometricCurve::Wave,
            GeometricCurve::S,
            GeometricCurve::Cylinder,
        ] {
            let params = GeometricParams {
                aspect: 2.0,
                radius: 35.0,
                rows: 3,
                columns: 5,
                curve,
                curve_strength: 0.3,
                curve_frequency: 0.8,
                lens: 0.4,
                orbit: 0.2,
                twist: 0.25,
                scatter: 0.2,
                motion: 0.3,
                stage: StageParams {
                    offset_x: 0.1,
                    offset_y: -0.1,
                    scale: 0.8,
                    rotation: 15.0,
                    shear_x: 0.1,
                    shear_y: -0.15,
                    perspective: 0.2,
                    depth_angle: 20.0,
                },
                ..Default::default()
            };
            let cards = layout_geometric(15, 7, [1000.0, 700.0], params);
            assert!(!cards.is_empty());
            assert_eq!(cards.last().unwrap().index, 7);
            for card in cards {
                for scale in [0.75, 1.0, 2.0] {
                    let inst = crate::donor_scene::instance_for_card(
                        &card,
                        &GalleryColors::default(),
                        scale,
                        0.0,
                    );
                    assert_eq!(
                        inst.rect,
                        [
                            card.center[0] * scale,
                            card.center[1] * scale,
                            card.size[0] * 0.5 * scale,
                            card.size[1] * 0.5 * scale
                        ]
                    );
                    assert_eq!(inst.misc[3] & 1, 1);
                    assert_eq!((inst.misc[3] >> 8) & 15, 0); // Hex fallback only
                    assert_eq!(inst.shape[1], card.rotation);
                }
                let half = [card.size[0] * 0.5, card.size[1] * 0.5];
                for normalized in [[0.0, 0.0], [0.4, 0.8], [-0.8, -0.7], [1.1, 0.0]] {
                    let local = [normalized[0] * half[0], normalized[1] * half[1]];
                    let (inside, _) = r8_hex_oracle(local, half);
                    let p = r8_world(&card, local);
                    assert_eq!(
                        hit_test(&[card.clone()], p[0], p[1]),
                        inside.then_some(card.index)
                    );
                }
            }
        }
    }

    #[test]
    fn r8_hex_invalid_and_small_extents() {
        let template = layout_geometric(
            1,
            0,
            [200.0, 200.0],
            GeometricParams {
                radius: 40.0,
                rows: 1,
                columns: 1,
                ..Default::default()
            },
        )[0]
        .clone();
        for rotation in [0.0, 0.00005, 0.6] {
            for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
                for axis in 0..2 {
                    let mut card = template.clone();
                    card.rotation = rotation;
                    card.size[axis] = invalid;
                    assert_eq!(
                        hit_test(&[card.clone()], card.center[0], card.center[1]),
                        None
                    );
                }
            }
            let mut card = template.clone();
            card.center = [2.0, -3.0];
            card.rotation = rotation;
            card.size = [0.5, 0.8];
            for (local, inside) in [([0.1, 0.3], true), ([0.22, 0.3], false)] {
                let p = r8_world(&card, local);
                assert_eq!(
                    hit_test(&[card.clone()], p[0], p[1]),
                    inside.then_some(card.index)
                );
            }
            assert_eq!(hit_test(&[card.clone()], f32::NAN, 0.0), None);
            assert_eq!(hit_test(&[card], 0.0, f32::INFINITY), None);
        }
    }

    #[test]
    fn r8_hex_widget_routes_frontmost_selection_and_background() {
        let mut front = layout_geometric(
            1,
            0,
            [800.0, 600.0],
            GeometricParams {
                radius: 100.0,
                aspect: 2.0,
                ..Default::default()
            },
        )[0]
        .clone();
        front.index = 7;
        front.rotation = 0.43;
        let mut back = front.clone();
        back.index = 3;
        back.size = [700.0, 400.0];
        back.selected = false;
        let mut cards = vec![front.clone(), back];
        selected_last(&mut cards, 7);
        let program = GalleryProgram {
            snapshot: Arc::new(GallerySnapshot {
                ordinary: None,
                wall_clip: None,
                depth: None,
                cards,
                posters: Arc::new(Vec::new()),
                colors: GalleryColors::default(),
                scene: DonorSceneState {
                    stage: StageParams::default(),
                    camera: 0.0,
                    camera_target: 0.0,
                    camera_velocity: 0.0,
                    detail_progress: 0.0,
                    external_detail: false,
                    flip_options: Default::default(),
                    detail_source: None,
                    detail_card: None,
                    viewport: [800.0, 600.0],
                    time: 0.0,
                    visibility: 1.0,
                },
            }),
        };
        let bounds = Rectangle::new(Point::new(19.0, 27.0), Size::new(800.0, 600.0));
        for (local, expected) in [
            ([0.0, 0.0], Some(7)),
            ([80.0, 69.6], Some(7)),
            ([140.0, 78.3], Some(3)),
            ([300.0, 140.0], None),
        ] {
            let p = r8_world(&front, local);
            let action = <GalleryProgram as shader::Program<GalleryMessage>>::update(
                &program,
                &mut Default::default(),
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(Point::new(p[0] + bounds.x, p[1] + bounds.y)),
            )
            .expect("in-bounds click must route");
            let (message, _, status) = action.into_inner();
            assert_eq!(status, iced::event::Status::Captured);
            match (message, expected) {
                (
                    Some(GalleryMessage::Pointer {
                        index: Some(actual),
                        ..
                    }),
                    Some(expected),
                ) => {
                    assert_eq!(actual, expected)
                }
                (Some(GalleryMessage::Pointer { index: None, .. }), None) => {}
                other => panic!("wrong gallery route: {other:?}"),
            }
        }
        // Later draws win even when selected flags alone do not dictate order.
        let mut reversed = program.snapshot.cards.clone();
        reversed.reverse();
        assert_eq!(
            hit_test(&reversed, front.center[0], front.center[1]),
            Some(3)
        );
    }

    #[test]
    fn r8_shared_non_hex_hits_remain_unchanged() {
        for (shape, inside, outside) in [
            (GeometricShape::Circle, [0.6, 0.6], [0.9, 0.9]),
            (GeometricShape::Triangle, [0.0, 0.0], [0.8, -0.8]),
            (GeometricShape::Diamond, [0.3, 0.3], [0.8, 0.8]),
            (GeometricShape::Rhombus, [0.3, 0.3], [0.8, 0.8]),
        ] {
            let mut card = layout_geometric(
                1,
                0,
                [500.0, 500.0],
                GeometricParams {
                    shape,
                    radius: 80.0,
                    rows: 1,
                    columns: 1,
                    ..Default::default()
                },
            )[0]
            .clone();
            for rotation in [0.0, 0.7] {
                card.rotation = rotation;
                for (p, expected) in [(inside, true), (outside, false)] {
                    let world = r8_world(
                        &card,
                        [p[0] * card.size[0] * 0.5, p[1] * card.size[1] * 0.5],
                    );
                    assert_eq!(
                        hit_test(&[card.clone()], world[0], world[1]),
                        expected.then_some(card.index),
                        "{shape:?}"
                    );
                }
            }
        }
        let mut card = layout_slices(1, 0, [1000.0, 800.0], SliceParams::default())[0].clone();
        card.skew_distance = 0.0;
        card.edge_tilt_distance = 0.0;
        for rotation in [0.0, 0.7] {
            card.rotation = rotation;
            let center = r8_world(&card, [0.0, 0.0]);
            let outside = r8_world(&card, [card.size[0], 0.0]);
            assert_eq!(
                hit_test(&[card.clone()], center[0], center[1]),
                Some(card.index)
            );
            assert_eq!(hit_test(&[card.clone()], outside[0], outside[1]), None);
        }
    }

    #[test]
    fn r8_production_wgsl_parses_and_validates() {
        let module = wgpu::naga::front::wgsl::parse_str(include_str!("../shaders/skwd_item.wgsl"))
            .expect("production WGSL must parse");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("production WGSL must validate");
        assert_eq!(module.entry_points.len(), 2);
        // This is semantic validation of the entire production shader, not
        // execution, pipeline creation or a rendered-mask/CPU equivalence test.
    }

    #[test]
    fn curated_variant_labels_match_the_thirteen_supported_arrangements() {
        assert_eq!(
            GalleryMode::Slices.variant_labels(),
            &["CLASSIC", "DENSE", "CINEMA"]
        );
        assert_eq!(
            GalleryMode::Geometric.variant_labels(),
            &["HEX ARC", "TRI WAVE", "SHOWCASE DIAMOND", "RHOMBUS"]
        );
        assert_eq!(
            GalleryMode::Wall.variant_labels(),
            &[
                "UNIFORM",
                "BRICK",
                "MASONRY",
                "JUSTIFIED",
                "EDITORIAL",
                "CYLINDER"
            ]
        );
    }

    #[test]
    fn renderer_slice_defaults_disable_wobble() {
        assert!(!SliceParams::default().wobble);
    }

    #[test]
    fn slice_distances_remain_pixel_units() {
        let params = SliceParams {
            skew_distance: 72.0,
            edge_tilt_distance: 18.0,
            ..Default::default()
        };
        let cards = layout_slices(3, 1, [1920.0, 1080.0], params);
        let selected = cards.iter().find(|card| card.selected).unwrap();

        assert_eq!(selected.skew_distance, 72.0);
        assert_eq!(selected.edge_tilt_distance, 18.0);
    }

    #[test]
    fn slices_honor_persisted_geometry_and_visible_count() {
        let params = SliceParams {
            offset_x: 0.1,
            offset_y: -0.2,
            width: 120.0,
            expanded_width: 300.0,
            height: 240.0,
            visible_count: 5,
            corner_radius: 27.0,
            ..Default::default()
        };
        let cards = layout_slices(20, 10, [1000.0, 800.0], params);
        let selected = cards.iter().find(|card| card.selected).unwrap();

        assert_eq!(cards.len(), 5);
        assert_eq!(selected.size, [300.0, 240.0]);
        assert_eq!(selected.center, [550.0, 320.0]);
        assert_eq!(selected.corner_radius, 27.0);
    }

    #[test]
    fn slices_motion_moves_cards_but_fades_against_the_fixed_viewport() {
        let params = SliceParams {
            width: 120.0,
            expanded_width: 300.0,
            spacing: 10.0,
            visible_count: 5,
            motion: 0.75,
            ..Default::default()
        };
        let moving = layout_slices(8, 4, [1000.0, 800.0], params);
        let settled = layout_slices(
            8,
            4,
            [1000.0, 800.0],
            SliceParams {
                motion: 0.0,
                ..params
            },
        );
        let moving_card = moving.iter().find(|card| card.index == 2).unwrap();
        let settled_card = settled.iter().find(|card| card.index == 2).unwrap();
        let expected_stride = (params.width - params.skew_distance.abs()).max(1.0) + params.spacing;

        assert!(
            (moving_card.center[0] - settled_card.center[0] - params.motion * expected_stride)
                .abs()
                < 0.001
        );
        let expected_opacity =
            slice_opacity(moving_card.center[0], 500.0, 290.0, 252.0, expected_stride);
        let translated_reference_opacity = slice_opacity(
            moving_card.center[0],
            moving_card.center[0],
            290.0,
            252.0,
            expected_stride,
        );
        assert!((moving_card.opacity - expected_opacity).abs() < 0.001);
        assert_ne!(expected_opacity, translated_reference_opacity);
        assert_ne!(moving_card.opacity, settled_card.opacity);
    }

    #[test]
    fn slices_measure_spacing_on_the_sheared_midline() {
        let params = SliceParams {
            width: 200.0,
            expanded_width: 300.0,
            skew_distance: 80.0,
            spacing: 10.0,
            visible_count: 3,
            ..Default::default()
        };
        let cards = layout_slices(3, 1, [1000.0, 800.0], params);
        let center = cards.iter().find(|card| card.index == 1).unwrap();
        let right = cards.iter().find(|card| card.index == 2).unwrap();

        assert_eq!(right.center[0] - center.center[0], 180.0);
    }

    #[test]
    fn selected_overlapping_card_wins_hit_test() {
        let cards = layout_slices(5, 2, [800.0, 600.0], SliceParams::default());
        let selected = cards.iter().find(|card| card.selected).unwrap();

        assert_eq!(
            hit_test(&cards, selected.center[0], selected.center[1]),
            Some(2)
        );
    }

    #[test]
    fn texture_budget_is_bounded_by_count_and_bytes() {
        let bytes = MAX_POSTERS * POSTER_WIDTH as usize * POSTER_HEIGHT as usize * 4;

        assert_eq!(MAX_POSTERS, 32);
        assert_eq!(POSTER_WIDTH, 512);
        assert_eq!(POSTER_HEIGHT, 512);
        assert_eq!(bytes, 32 * 512 * 512 * 4);
        assert!(bytes <= 40 * 1024 * 1024);
    }

    #[test]
    fn decoded_cpu_pixels_are_released_after_the_upload_claims_them() {
        let poster = Poster {
            layer: 0,
            source: PathBuf::from("poster.png"),
            aspect_ratio: 1.0,
            rgba: Arc::new(Mutex::new(Some(vec![7; 16]))),
            recovery: PosterRecovery::Image(PathBuf::from("missing-poster.png")),
        };

        assert!(poster.has_pixels());
        assert_eq!(poster.take_pixels(), Some(vec![7; 16]));
        assert!(!poster.has_pixels());
        assert_eq!(poster.take_pixels(), None);
    }

    #[test]
    fn released_video_pixels_are_recreated_for_a_new_gpu_pipeline() {
        let poster = video_placeholder_poster(std::path::Path::new("sample.mp4"), 0);
        let initial = poster.pixels_for_upload().unwrap();

        assert!(!poster.has_pixels());
        assert_eq!(poster.pixels_for_upload(), Some(initial));
        assert!(!poster.has_pixels());
    }

    #[test]
    fn released_cached_pixels_are_reloaded_for_a_new_gpu_pipeline() {
        let (_profile, root) = crate::poster_worker::cache::tests::fixture();
        let cache_path = root.join("poster-0123456789abcdef.rgba");
        let expected = vec![19; (POSTER_WIDTH * POSTER_HEIGHT * 4) as usize];
        std::fs::write(&cache_path, &expected).unwrap();
        let poster = Poster {
            layer: 0,
            source: PathBuf::from("missing-source.png"),
            aspect_ratio: 1.0,
            rgba: Arc::new(Mutex::new(Some(expected.clone()))),
            recovery: PosterRecovery::CachedRgba {
                cache_path: cache_path.clone(),
                source_path: PathBuf::from("missing-source.png"),
            },
        };

        assert_eq!(poster.pixels_for_upload(), Some(expected.clone()));
        assert!(!poster.has_pixels());
        assert_eq!(poster.pixels_for_upload(), Some(expected));
        assert!(!poster.has_pixels());

        std::fs::remove_file(cache_path).unwrap();
    }

    #[test]
    fn every_required_gallery_mode_places_the_selected_item_last() {
        for mode in [
            GalleryMode::Slices,
            GalleryMode::Geometric,
            GalleryMode::Wall,
        ] {
            let cards = layout_gallery(
                mode,
                40,
                17,
                [1920.0, 1080.0],
                GalleryLayoutParams::default(),
            );

            assert_eq!(cards.last().map(|card| card.index), Some(17), "{mode:?}");
        }
    }

    #[test]
    fn every_geometric_shape_uses_its_persisted_shape_and_aspect() {
        let cases = [
            (GeometricShape::Circle, 1),
            (GeometricShape::Hexagon, 2),
            (GeometricShape::Triangle, 4),
            (GeometricShape::Diamond, 3),
            (GeometricShape::Rhombus, 3),
        ];
        for (shape, code) in cases {
            let params = GeometricParams {
                shape,
                aspect: 1.4,
                ..Default::default()
            };
            let cards = layout_geometric(40, 17, [1920.0, 1080.0], params);
            let selected = cards.iter().find(|card| card.selected).unwrap();

            assert_eq!(selected.shape, code, "{shape:?}");
            assert!(selected.size[0] > 0.0 && selected.size[1] > 0.0);
        }
    }

    #[test]
    fn geometric_deformation_controls_materially_change_layout() {
        let base = layout_geometric(60, 25, [1600.0, 900.0], GeometricParams::default());
        let changed = layout_geometric(
            60,
            25,
            [1600.0, 900.0],
            GeometricParams {
                curve: GeometricCurve::Wave,
                curve_strength: 1.8,
                curve_frequency: 2.0,
                lens: 0.35,
                orbit: 0.4,
                twist: 18.0,
                scatter: 20.0,
                ..Default::default()
            },
        );

        let base_card = base.iter().find(|card| card.index == 24).unwrap();
        let changed_card = changed.iter().find(|card| card.index == 24).unwrap();
        assert_ne!(base_card.center, changed_card.center);
        assert_ne!(base_card.size, changed_card.size);
    }

    #[test]
    fn every_wall_arrangement_explicit_restore_keeps_the_selected_card_and_radius() {
        for layout in [
            WallLayout::Uniform,
            WallLayout::Brick,
            WallLayout::Masonry,
            WallLayout::Justified,
            WallLayout::Editorial,
            WallLayout::Cylinder,
        ] {
            let mut p = WallParams {
                layout,
                corner_radius: 31.0,
                ..Default::default()
            };
            let (grid, placements) = wall_placements(80, p, &[]);
            let c = placements.iter().find(|c| c.index == 23).unwrap();
            p.scroll_offset = (c.y + c.height * 0.5 - grid.total_h() * 0.5).max(0.0);
            let cards = layout_wall(80, 23, [1920.0, 1080.0], p);
            let selected = cards.iter().find(|card| card.selected).unwrap();

            assert!(
                cards.windows(2).all(|pair| pair[0].index < pair[1].index),
                "source Wall painter order: {layout:?}"
            );
            assert_eq!(selected.corner_radius, 31.0);
            assert!(
                selected.center[0] >= 0.0 && selected.center[0] <= 1920.0,
                "{layout:?}"
            );
        }
    }

    #[test]
    fn first_wall_item_is_navigable_or_explicitly_clipped_by_donor_frame() {
        for layout in [
            WallLayout::Uniform,
            WallLayout::Brick,
            WallLayout::Masonry,
            WallLayout::Justified,
            WallLayout::Editorial,
            WallLayout::Cylinder,
        ] {
            let cards = layout_wall(
                80,
                0,
                [1920.0, 1080.0],
                WallParams {
                    layout,
                    ..Default::default()
                },
            );
            let selected = cards.iter().find(|card| card.selected).unwrap();
            assert!(
                selected.center.iter().all(|value| value.is_finite()),
                "{layout:?}"
            );
            assert!(
                selected.size.iter().all(|value| value.is_finite()),
                "{layout:?}"
            );
            assert!(selected.size.iter().all(|value| *value > 0.0), "{layout:?}");
            let in_viewport = selected.center[0] + selected.size[0] * 0.5 > 0.0
                && selected.center[0] - selected.size[0] * 0.5 < 1920.0
                && selected.center[1] + selected.size[1] * 0.5 > 0.0
                && selected.center[1] - selected.size[1] * 0.5 < 1080.0;
            assert!(
                in_viewport,
                "selected first item must remain navigable: {layout:?}"
            );
            assert_eq!(
                hit_test(&cards, selected.center[0], selected.center[1]),
                Some(0),
                "{layout:?}"
            );
        }
    }

    #[test]
    fn uniform_wall_keeps_donor_composition_when_selection_moves() {
        let settled = WallParams {
            layout: WallLayout::Uniform,
            ..Default::default()
        };
        let first = layout_wall(40, 0, [1920.0, 1080.0], settled);
        let moved = layout_wall(40, 5, [1920.0, 1080.0], settled);
        let first_card = first.iter().find(|card| card.index == 0).unwrap();
        let moved_first_card = moved.iter().find(|card| card.index == 0).unwrap();
        let moved_selected = moved.iter().find(|card| card.index == 5).unwrap();

        assert_eq!(first_card.center, [190.0, 379.0]);
        assert_eq!(moved_first_card.center, first_card.center);
        assert_eq!(moved_selected.center[0], 1730.0);
        assert_ne!(moved_selected.center[0], 960.0);
    }

    #[test]
    fn wall_endpoint_uniform_first_interior_last_rows_follow_clamped_absolute_camera() {
        let p = WallParams {
            columns: 4,
            rows: 3,
            card_width: 90.0,
            card_height: 60.0,
            gap_x: 7.0,
            gap_y: 10.0,
            selected_scale: 1.0,
            ..Default::default()
        };
        // beta.24 horizontal rail: cy = vh/2 + (bar_height + gap)/2.
        // Frame H=200, content H=410, max camera=210; centers30,170,380.
        let viewport = [900.0, 700.0];
        for (selected, camera, expected_y) in
            [(0, 0.0, 296.0), (8, 70.0, 366.0), (20, 210.0, 436.0)]
        {
            let cards = layout_wall(
                24,
                selected,
                viewport,
                WallParams {
                    scroll_offset: camera,
                    ..p
                },
            );
            let card = cards.iter().find(|c| c.index == selected).unwrap();
            assert_eq!(card.center[1], expected_y, "selected {selected}");
            assert_eq!(
                hit_test(&cards, card.center[0], card.center[1]),
                Some(selected)
            );
        }
    }

    #[test]
    fn wall_endpoint_short_and_exact_fit_catalogs_do_not_move_with_selected_row() {
        let p = WallParams {
            columns: 4,
            rows: 3,
            card_width: 90.0,
            card_height: 60.0,
            gap_x: 7.0,
            gap_y: 10.0,
            selected_scale: 1.0,
            ..Default::default()
        };
        for count in [8, 12] {
            let first = layout_wall(count, 0, [900.0, 700.0], p);
            let last = layout_wall(count, count - 1, [900.0, 700.0], p);
            assert_eq!(first.len(), count);
            assert_eq!(last.len(), count, "fitting catalog must not lose first row");
            for c in &first {
                let other = last.iter().find(|x| x.index == c.index).unwrap();
                assert_eq!(c.center, other.center, "count {count}, item {}", c.index);
            }
            assert_eq!(
                first.iter().find(|c| c.index == 0).unwrap().center[1],
                296.0
            );
        }
    }

    #[test]
    fn wall_endpoint_absolute_scroll_bounds_and_same_row_selection() {
        let p = WallParams {
            columns: 4,
            rows: 3,
            card_width: 90.0,
            card_height: 60.0,
            gap_x: 7.0,
            gap_y: 10.0,
            selected_scale: 1.0,
            ..Default::default()
        };
        let positions = |selected, offset| {
            let mut cards = layout_wall(
                24,
                selected,
                [900.0, 700.0],
                WallParams {
                    scroll_offset: offset,
                    ..p
                },
            );
            cards.sort_by_key(|c| c.index);
            cards
                .iter()
                .map(|c| (c.index, c.center))
                .collect::<Vec<_>>()
        };
        assert_eq!(positions(8, 0.0), positions(11, 0.0));
        assert_eq!(positions(8, -1e6), positions(0, 0.0));
        assert_eq!(positions(8, 1e6), positions(23, 210.0));
        let partial = layout_wall(
            24,
            0,
            [900.0, 700.0],
            WallParams {
                scroll_offset: 20.0,
                ..p
            },
        );
        assert_eq!(
            partial.iter().find(|c| c.index == 0).unwrap().center[1],
            276.0
        );
    }

    #[test]
    fn wall_endpoint_frame_tracks_wrapped_hidden_rail_and_viewport_clip() {
        let p = WallParams {
            columns: 4,
            rows: 3,
            card_width: 90.0,
            card_height: 60.0,
            gap_x: 7.0,
            gap_y: 10.0,
            ..Default::default()
        };
        for (height, center_y) in [(None, 350.0), (Some(24.0), 366.0), (Some(72.0), 390.0)] {
            let frame = wall_frame(
                [900.0, 700.0],
                WallParams {
                    rail_height: height,
                    ..p
                },
            );
            assert_eq!(frame.center, [450.0, center_y]);
            assert_eq!(frame.top, center_y - 100.0);
            let rail_height = height.unwrap_or(0.0);
            if height.is_some() {
                let rail_top = (700.0 - rail_height - 8.0 - 200.0) * 0.5;
                assert_eq!(frame.top - (rail_top + rail_height), 8.0);
            }
        }
        let frame = wall_frame([160.0, 120.0], p);
        assert_eq!(frame.clip, [0.0, 0.0, 160.0, 120.0]);
    }

    #[test]
    fn wall_endpoint_motion_and_cylinder_stage_transform_remain_after_camera_placement() {
        let mut p = WallParams {
            columns: 4,
            rows: 3,
            card_width: 90.0,
            card_height: 60.0,
            gap_x: 7.0,
            gap_y: 10.0,
            selected_scale: 1.0,
            motion: 0.2,
            layout: WallLayout::Cylinder,
            ..Default::default()
        };
        p.stage.shear_y = 0.1;
        let viewport = [900.0, 700.0];
        let frame = wall_frame(viewport, p);
        let donor = DonorGridParams {
            cols: 4,
            rows: 3,
            thumb_w: 90.0,
            thumb_h: 60.0,
            gap_x: 7.0,
            gap_y: 10.0,
            layout: DonorGridLayout::Cylinder,
            cylinder_bend: p.cylinder_bend,
            cylinder_radius: p.cylinder_radius,
            stage: p.stage,
            ..Default::default()
        };
        let raw = [frame.left + 45.0, frame.top + 30.0 + 0.2 * 70.0];
        let (x, y, surface) =
            donor.cylinder_transform(raw[0], raw[1], (frame.center[0], frame.center[1]));
        let (x, y, stage) =
            donor
                .stage
                .transform(x, y, (frame.center[0], frame.center[1]), (900.0, 700.0));
        let cards = layout_wall(24, 0, viewport, p);
        let c = cards.iter().find(|c| c.index == 0).unwrap();
        assert_eq!(c.center, [x, y]);
        assert_eq!(c.size, [90.0 * surface * stage, 60.0 * surface * stage]);
    }

    #[test]
    fn wall_endpoint_shader_clip_and_real_widget_hits_share_frame_but_not_external_detail() {
        let p = WallParams {
            columns: 2,
            rows: 1,
            card_width: 100.0,
            card_height: 80.0,
            selected_scale: 1.2,
            user_engaged: true,
            ..Default::default()
        };
        let viewport = [500.0, 300.0];
        let frame = wall_frame(viewport, p);
        let cards = layout_wall(2, 0, viewport, p);
        let selected = cards.iter().find(|c| c.selected).unwrap();
        let point = [selected.center[0], frame.top - 2.0];
        assert_eq!(
            hit_test(&cards, point[0], point[1]),
            Some(0),
            "body extends beyond frame"
        );
        let snapshot = GallerySnapshot {
            ordinary: None,
            cards: cards.clone(),
            posters: Arc::new(Vec::new()),
            colors: GalleryColors::default(),
            depth: None,
            wall_clip: Some(frame.clip),
            scene: DonorSceneState {
                stage: StageParams::default(),
                camera: 0.0,
                camera_target: 0.0,
                camera_velocity: 0.0,
                detail_progress: 1.0,
                external_detail: true,
                flip_options: Default::default(),
                detail_source: None,
                detail_card: None,
                viewport,
                time: 0.0,
                visibility: 1.0,
            },
        };
        assert_eq!(snapshot.physical_clip(1.5), frame.clip.map(|v| v * 1.5));
        let raw = snapshot.ordinary_instances(1.5);
        assert!(raw[..cards.len()].iter().all(|r| r.misc[2] == 1));
        assert!(
            raw[cards.len()..].iter().all(|r| r.misc[2] == 0),
            "external detail is not a gallery body"
        );
        let program = GalleryProgram {
            snapshot: Arc::new(snapshot),
        };
        let event = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(viewport[0], viewport[1]));
        for (point, expected) in [(point, None), (selected.center, Some(0))] {
            let (message, _, _) = <GalleryProgram as shader::Program<GalleryMessage>>::update(
                &program,
                &mut Default::default(),
                &event,
                bounds,
                mouse::Cursor::Available(Point::new(point[0], point[1])),
            )
            .unwrap()
            .into_inner();
            match (message, expected) {
                (Some(GalleryMessage::Pointer { index: None, .. }), None) => {}
                (
                    Some(GalleryMessage::Pointer {
                        index: Some(actual),
                        ..
                    }),
                    Some(expected),
                ) => {
                    assert_eq!(actual, expected)
                }
                other => panic!("wrong clipped hit: {other:?}"),
            }
        }
    }

    #[test]
    fn wall_masonry_and_justified_use_catalog_aspect_ratios() {
        let aspects = [4.0 / 3.0, 16.0 / 9.0, 3.0 / 4.0, 1.0];
        for layout in [WallLayout::Masonry, WallLayout::Justified] {
            let donor_layout = match layout {
                WallLayout::Masonry => DonorGridLayout::Masonry,
                WallLayout::Justified => DonorGridLayout::Justified,
                _ => unreachable!(),
            };
            let donor_params = DonorGridParams {
                layout: donor_layout,
                ..Default::default()
            };
            let expected = donor_layout::grid_placements(aspects.len(), &donor_params, &aspects);
            let cards = layout_wall_with_aspects(
                aspects.len(),
                2,
                [1920.0, 1080.0],
                WallParams {
                    layout,
                    ..Default::default()
                },
                &aspects,
            );
            for (index, expected_aspect) in aspects.iter().enumerate() {
                let card = cards.iter().find(|card| card.index == index).unwrap();
                let placement = expected
                    .iter()
                    .find(|placement| placement.index == index)
                    .unwrap();
                let focus = 1.0; // No engagement: source selected scale is not applied.
                assert_eq!(
                    card.size,
                    [placement.width * focus, placement.height * focus]
                );
                assert_eq!(card.poster_aspect_ratio, *expected_aspect);
            }
        }
    }

    #[test]
    fn wall_camera_clamps_to_catalog_content_bounds() {
        let make = |scroll_offset| {
            layout_wall(
                120,
                0,
                [1920.0, 1080.0],
                WallParams {
                    scroll_offset,
                    ..Default::default()
                },
            )
        };
        let far = make(1_000_000.0);
        let farther = make(100_000_000.0);

        assert!(!far.is_empty());
        assert_eq!(
            far.iter()
                .map(|card| (card.index, card.center))
                .collect::<Vec<_>>(),
            farther
                .iter()
                .map(|card| (card.index, card.center))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn current_wall_camera_and_ordinary_transient_motion_move_visible_cards() {
        let settled = GalleryLayoutParams::default();
        let moving = GalleryLayoutParams {
            depth: settled.depth,
            depth_camera: settled.depth_camera,
            slices: SliceParams {
                motion: 0.75,
                ..settled.slices
            },
            geometric: GeometricParams {
                motion: 0.75,
                ..settled.geometric
            },
            wall: WallParams {
                motion: 0.75,
                ..settled.wall
            },
        };
        for mode in [
            GalleryMode::Slices,
            GalleryMode::Geometric,
            GalleryMode::Wall,
        ] {
            let mut rest = settled;
            let mut current = moving;
            if mode == GalleryMode::Wall {
                // Wall's persistent content camera replaces selected-derived displacement.
                rest.wall.scroll_offset = 300.0;
                current.wall.scroll_offset = 400.0;
                current.wall.motion = 0.0;
            }
            let at_rest = layout_gallery(mode, 60, 20, [1600.0, 900.0], rest);
            let in_motion = layout_gallery(mode, 60, 20, [1600.0, 900.0], current);
            let rest_selected = at_rest.iter().find(|card| card.selected).unwrap();
            let moving_selected = in_motion.iter().find(|card| card.selected).unwrap();

            assert_ne!(rest_selected.center, moving_selected.center, "{mode:?}");
        }
    }

    #[test]
    fn detail_preserves_gallery_card_geometry_and_neighbors() {
        for mode in [
            GalleryMode::Slices,
            GalleryMode::Geometric,
            GalleryMode::Wall,
        ] {
            let browse_cards = layout_gallery(
                mode,
                60,
                20,
                [1600.0, 900.0],
                GalleryLayoutParams::default(),
            );
            let detail_cards = layout_gallery(
                mode,
                60,
                20,
                [1600.0, 900.0],
                GalleryLayoutParams {
                    ..Default::default()
                },
            );
            assert_eq!(detail_cards.len(), browse_cards.len(), "{mode:?}");
            for detail_card in &detail_cards {
                let browse_card = browse_cards
                    .iter()
                    .find(|card| card.index == detail_card.index)
                    .expect("detail card must retain its gallery identity");
                assert_eq!(detail_card.center, browse_card.center, "{mode:?}");
                assert_eq!(detail_card.size, browse_card.size, "{mode:?}");
                assert_eq!(detail_card.opacity, browse_card.opacity, "{mode:?}");
                assert_eq!(detail_card.rotation, browse_card.rotation, "{mode:?}");
                assert_eq!(
                    detail_card.skew_distance, browse_card.skew_distance,
                    "{mode:?}"
                );
                assert_eq!(
                    detail_card.edge_tilt_distance, browse_card.edge_tilt_distance,
                    "{mode:?}"
                );
            }
            assert!(detail_cards
                .iter()
                .any(|card| !card.selected && card.opacity > 0.0));
        }
    }

    #[test]
    fn slices_detail_keeps_card_geometry_at_midpoint() {
        let browse_cards = layout_gallery(
            GalleryMode::Slices,
            20,
            10,
            [1000.0, 800.0],
            GalleryLayoutParams::default(),
        );
        let detail_cards = layout_gallery(
            GalleryMode::Slices,
            20,
            10,
            [1000.0, 800.0],
            GalleryLayoutParams::default(),
        );
        let browse_selected = browse_cards.iter().find(|card| card.selected).unwrap();
        let detail_selected = detail_cards.iter().find(|card| card.selected).unwrap();
        assert_eq!(detail_selected.center, browse_selected.center);
        assert_eq!(detail_selected.size, browse_selected.size);
        assert!(detail_cards
            .iter()
            .any(|card| !card.selected && card.opacity > 0.0));
    }

    #[test]
    fn geometric_hit_testing_rejects_the_square_corner_of_a_circle() {
        let card = SliceCard {
            triangle_direction: 0,
            index: 4,
            center: [100.0, 100.0],
            size: [100.0, 100.0],
            layer: None,
            selected: true,
            opacity: 1.0,
            skew_distance: 0.0,
            edge_tilt_distance: 0.0,
            rotation: 0.0,
            corner_radius: 0.0,
            shape: 1,
            poster_aspect_ratio: 1.0,
        };

        assert_eq!(hit_test(&[card], 149.0, 149.0), None);
        assert_eq!(hit_test(&[card], 100.0, 100.0), Some(4));
    }

    #[test]
    fn hit_testing_follows_card_rotation() {
        let card = SliceCard {
            triangle_direction: 0,
            index: 8,
            center: [100.0, 100.0],
            size: [120.0, 30.0],
            layer: None,
            selected: true,
            opacity: 1.0,
            skew_distance: 0.0,
            edge_tilt_distance: 0.0,
            rotation: std::f32::consts::FRAC_PI_2,
            corner_radius: 0.0,
            shape: 0,
            poster_aspect_ratio: 4.0 / 3.0,
        };

        assert_eq!(hit_test(&[card], 100.0, 150.0), Some(8));
        assert_eq!(hit_test(&[card], 150.0, 100.0), None);
    }

    #[test]
    fn gallery_canvas_publishes_a_captured_click_for_a_hit_card() {
        let tracing_was_enabled = apply_trace::enabled();
        apply_trace::set_enabled(false);
        let card = SliceCard {
            triangle_direction: 0,
            index: 4,
            center: [100.0, 100.0],
            size: [80.0, 80.0],
            layer: None,
            selected: true,
            opacity: 1.0,
            skew_distance: 0.0,
            edge_tilt_distance: 0.0,
            rotation: 0.0,
            corner_radius: 0.0,
            shape: 0,
            poster_aspect_ratio: 1.0,
        };
        let program = GalleryProgram {
            snapshot: Arc::new(GallerySnapshot {
                ordinary: None,
                wall_clip: None,
                depth: None,
                cards: vec![card],
                posters: Arc::new(Vec::new()),
                colors: GalleryColors::default(),
                scene: DonorSceneState {
                    stage: StageParams::default(),
                    camera: 0.0,
                    camera_target: 0.0,
                    camera_velocity: 0.0,
                    detail_progress: 0.0,
                    external_detail: false,
                    flip_options: Default::default(),
                    detail_source: None,
                    detail_card: None,
                    viewport: [200.0, 200.0],
                    time: 0.0,
                    visibility: 1.0,
                },
            }),
        };
        let event = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let action = <GalleryProgram as shader::Program<GalleryMessage>>::update(
            &program,
            &mut Default::default(),
            &event,
            Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0)),
            mouse::Cursor::Available(Point::new(100.0, 100.0)),
        )
        .expect("gallery hit must publish an action");
        let (message, _, status) = action.into_inner();
        assert!(matches!(
            message,
            Some(GalleryMessage::Pointer {
                index: Some(4),
                sequence: None,
                ..
            })
        ));
        assert_eq!(status, iced::event::Status::Captured);

        apply_trace::set_enabled(true);
        let traced_action = <GalleryProgram as shader::Program<GalleryMessage>>::update(
            &program,
            &mut Default::default(),
            &event,
            Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0)),
            mouse::Cursor::Available(Point::new(100.0, 100.0)),
        )
        .expect("traced gallery hit must publish an action");
        let (traced_message, _, traced_status) = traced_action.into_inner();
        assert!(matches!(
            traced_message,
            Some(GalleryMessage::Pointer {
                index: Some(4),
                sequence: Some(_),
                ..
            })
        ));
        assert_eq!(traced_status, iced::event::Status::Captured);
        apply_trace::set_enabled(tracing_was_enabled);
        let mut state = iced::keyboard::Modifiers::default();
        let _ = <GalleryProgram as shader::Program<GalleryMessage>>::update(
            &program,
            &mut state,
            &Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                iced::keyboard::Modifiers::CTRL,
            )),
            Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0)),
            mouse::Cursor::Available(Point::new(100.0, 100.0)),
        );
        let (message, _, status) = <GalleryProgram as shader::Program<GalleryMessage>>::update(
            &program,
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0)),
            mouse::Cursor::Available(Point::new(100.0, 100.0)),
        )
        .unwrap()
        .into_inner();
        assert!(matches!(
            message,
            Some(GalleryMessage::Pointer {
                button: crate::candidate_state::binding_codec::MouseButton::Right,
                modifiers: crate::candidate_state::binding_codec::Mods { ctrl: true, .. },
                index: Some(4),
                ..
            })
        ));
        assert_eq!(status, iced::event::Status::Captured);
    }

    #[test]
    fn gallery_canvas_keeps_zero_wheel_input_unpublished() {
        let program = GalleryProgram {
            snapshot: Arc::new(GallerySnapshot {
                ordinary: None,
                wall_clip: None,
                depth: None,
                cards: Vec::new(),
                posters: Arc::new(Vec::new()),
                colors: GalleryColors::default(),
                scene: DonorSceneState {
                    stage: StageParams::default(),
                    camera: 0.0,
                    camera_target: 0.0,
                    camera_velocity: 0.0,
                    detail_progress: 0.0,
                    external_detail: false,
                    flip_options: Default::default(),
                    detail_source: None,
                    detail_card: None,
                    viewport: [200.0, 200.0],
                    time: 0.0,
                    visibility: 1.0,
                },
            }),
        };
        let event = Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: 0.0 },
        });

        let action = <GalleryProgram as shader::Program<GalleryMessage>>::update(
            &program,
            &mut Default::default(),
            &event,
            Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0)),
            mouse::Cursor::Available(Point::new(100.0, 100.0)),
        );

        assert!(action.is_none());
    }

    #[test]
    fn donor_instance_abi_matches_pinned_scene_payload() {
        assert_eq!(std::mem::size_of::<InstanceRaw>(), 176);
        assert_eq!(std::mem::align_of::<InstanceRaw>(), 4);
    }

    #[test]
    fn manual_hex_colors_parse_without_panicking_on_invalid_input() {
        let fallback = [0.1, 0.2, 0.3];

        assert_eq!(
            parse_hex_rgb("#ff8000", fallback),
            [1.0, 128.0 / 255.0, 0.0]
        );
        assert_eq!(
            parse_hex_rgb("  A1b2C3  ", fallback),
            [
                0xa1 as f32 / 255.0,
                0xb2 as f32 / 255.0,
                0xc3 as f32 / 255.0
            ]
        );
        assert_eq!(
            parse_hex_rgb("\t#AaBbCc\n", fallback),
            [
                0xaa as f32 / 255.0,
                0xbb as f32 / 255.0,
                0xcc as f32 / 255.0
            ]
        );
        assert_eq!(parse_hex_rgb("not-a-color", fallback), fallback);

        for value in ["", "#", "#12", "#12345", "#1234567", "zzzzzz", "#12xz45"] {
            assert_eq!(parse_hex_rgb(value, fallback), fallback, "input: {value:?}");
        }
    }

    #[test]
    fn six_byte_unicode_color_input_falls_back_without_panicking() {
        let fallback = [0.1, 0.2, 0.3];

        assert_eq!(parse_hex_rgb("あabc", fallback), fallback);
    }

    #[test]
    fn decoded_poster_keeps_source_aspect_and_letterboxes_without_cropping() {
        let path = std::env::temp_dir().join(format!(
            "wallpaper-switcher-poster-aspect-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = image::RgbaImage::from_pixel(400, 200, image::Rgba([255, 0, 0, 255]));
        source.save(&path).unwrap();

        let (pixels, aspect) = decode_poster_pixels_with_aspect(&path).unwrap();
        assert_eq!(pixels.len(), (POSTER_WIDTH * POSTER_HEIGHT * 4) as usize);
        let center = ((POSTER_HEIGHT / 2 * POSTER_WIDTH + POSTER_WIDTH / 2) * 4) as usize;
        let corner = 0;
        assert_eq!(aspect, 2.0);
        assert_eq!(&pixels[center..center + 4], &[255, 0, 0, 255]);
        assert_eq!(&pixels[corner..corner + 4], &[30, 30, 46, 255]);
        let _ = std::fs::remove_file(path);
    }
}

/// Internal D1 frame; no GalleryMode/Kind branch or application event consumer.
/// Identities borrow the exact catalog revision used to construct this frame.
pub(crate) struct DepthItem<'a> {
    pub id: &'a str,
    pub master_index: usize,
    pub source_aspect: Option<f32>,
}

#[derive(Debug, Clone)]
pub(crate) struct DepthFrameCard<'a> {
    pub id: &'a str,
    pub card: SliceCard,
    pub filtered_index: usize,
    pub radii: [f32; 4],
    pub parallax: f32,
    pub crop_uv: [f32; 4],
    pub hovered: bool,
}

pub(crate) struct DepthFrame<'a> {
    pub revision: u64,
    pub viewport: [f32; 2],
    pub camera: f64,
    pub fit: f32,
    pub candidate_range: Option<(usize, usize)>,
    pub cards: Vec<DepthFrameCard<'a>>,
}

impl<'a> DepthFrame<'a> {
    pub fn visible_ids(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.cards.iter().map(|card| card.id)
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<&DepthFrameCard<'a>> {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x > self.viewport[0]
            || y > self.viewport[1]
        {
            return None;
        }
        self.cards.iter().rev().find(|entry| {
            donor_layout::depth_contains(
                entry.card.center,
                entry.card.size,
                entry.card.skew_distance,
                entry.radii,
                [x, y],
            )
        })
    }
}

pub(crate) fn depth_frame<'a>(
    items: &[DepthItem<'a>],
    revision: u64,
    selected: Option<&str>,
    hovered: Option<&str>,
    camera: f32,
    viewport: [f32; 2],
    composition: [f32; 2],
    params: donor_layout::DepthParams,
    entrance: f32,
) -> DepthFrame<'a> {
    depth_frame_for(
        items.len(),
        |index| {
            items.get(index).map(|item| DepthItem {
                id: item.id,
                master_index: item.master_index,
                source_aspect: item.source_aspect,
            })
        },
        revision,
        selected,
        hovered,
        camera,
        viewport,
        composition,
        params,
        entrance,
    )
}

/// Resolve only the donor's bounded candidate window rather than cloning all
/// catalog identities/aspects on every animation frame.
pub(crate) fn depth_frame_for<'a>(
    count: usize,
    mut item_at: impl FnMut(usize) -> Option<DepthItem<'a>>,
    revision: u64,
    selected: Option<&str>,
    hovered: Option<&str>,
    camera: f32,
    viewport: [f32; 2],
    composition: [f32; 2],
    params: donor_layout::DepthParams,
    entrance: f32,
) -> DepthFrame<'a> {
    let layout = donor_layout::depth_layout(count, camera, viewport, composition, params, entrance);
    let cards = layout
        .cards
        .into_iter()
        .filter_map(|placement| {
            let item = item_at(placement.filtered_index)?;
            let aspect = valid_aspect(item.source_aspect);
            Some(DepthFrameCard {
                id: item.id,
                card: SliceCard {
                    triangle_direction: 0,
                    index: item.master_index,
                    center: placement.center,
                    size: placement.size,
                    layer: None,
                    selected: selected == Some(item.id),
                    opacity: placement.opacity,
                    skew_distance: placement.skew,
                    edge_tilt_distance: 0.0,
                    rotation: 0.0,
                    corner_radius: placement.radii[0],
                    shape: 0,
                    poster_aspect_ratio: aspect,
                },
                filtered_index: placement.filtered_index,
                radii: placement.radii,
                parallax: placement.parallax,
                crop_uv: depth_crop_uv(aspect, placement.size, placement.parallax),
                hovered: hovered == Some(item.id),
            })
        })
        .collect();
    DepthFrame {
        revision,
        viewport,
        camera: layout.camera,
        fit: layout.fit,
        candidate_range: layout.candidate_range,
        cards,
    }
}

/// Apply positional zoom/pan within decoded content, then map to the square
/// letterboxed poster. Pointer coordinates are deliberately absent.
fn depth_crop_uv(aspect: f32, size: [f32; 2], parallax: f32) -> [f32; 4] {
    let [content_x, content_y, content_w, content_h] = if aspect >= 1.0 {
        [0.0, (1.0 - 1.0 / aspect) * 0.5, 1.0, 1.0 / aspect]
    } else {
        [(1.0 - aspect) * 0.5, 0.0, aspect, 1.0]
    };
    let cover = crate::donor_scene::poster_crop_uv(aspect, size);
    let nw = cover[2] / content_w / 1.4;
    let nh = cover[3] / content_h / 1.4;
    [
        content_x + (1.0 - nw) * (0.5 + parallax.clamp(-1.0, 1.0) * 0.5) * content_w,
        content_y + (1.0 - nh) * 0.5 * content_h,
        nw * content_w,
        nh * content_h,
    ]
}

#[cfg(test)]
mod depth_frame_tests {
    use super::*;
    use donor_layout::DepthParams;
    #[test]
    fn d1_frame_ties_identity_and_reverse_hits_without_selected_last() {
        let items: Vec<_> = ["f", "b", "q", "a", "x", "z", "r", "p"]
            .iter()
            .enumerate()
            .map(|(i, id)| DepthItem {
                id,
                master_index: 100 - i,
                source_aspect: None,
            })
            .collect();
        for (camera, front) in [(3.25, "a"), (3.5, "x")] {
            let frame = depth_frame(
                &items,
                71,
                Some("z"),
                Some("a"),
                camera,
                [1200.0, 900.0],
                [600.0, 1200.0],
                DepthParams {
                    spacing: 0.0,
                    ..DepthParams::default()
                },
                1.0,
            );
            assert_eq!(frame.revision, 71);
            assert_eq!(frame.camera, camera as f64);
            assert!(frame.fit > 0.0 && frame.fit <= 1.0);
            assert_eq!(frame.candidate_range, Some((0, 7)));
            assert_eq!(frame.hit(600.0, 450.0).unwrap().id, front);
            assert_eq!(frame.cards.last().unwrap().id, front);
            assert_eq!(
                frame.visible_ids().collect::<Vec<_>>(),
                frame.cards.iter().map(|x| x.id).collect::<Vec<_>>()
            );
            for entry in &frame.cards {
                assert_eq!(entry.card.index, 100 - entry.filtered_index);
                assert_eq!(entry.card.selected, entry.id == "z");
                assert_eq!(entry.hovered, entry.id == "a");
                assert!(entry.parallax.is_finite());
                assert_eq!(entry.card.corner_radius, entry.radii[0]);
                assert_eq!(
                    (
                        entry.card.rotation,
                        entry.card.edge_tilt_distance,
                        entry.card.shape
                    ),
                    (0.0, 0.0, 0)
                );
            }
        }
    }

    #[test]
    fn d1_frame_visible_underlying_rounded_and_scissor_hits() {
        let items: Vec<_> = ["a", "b", "c"]
            .iter()
            .enumerate()
            .map(|(i, id)| DepthItem {
                id,
                master_index: i,
                source_aspect: Some(1.0),
            })
            .collect();
        let frame = depth_frame(
            &items,
            1,
            Some("a"),
            None,
            1.0,
            [1200.0, 900.0],
            [600.0, 1200.0],
            DepthParams {
                width: 300.0,
                height: 300.0,
                spacing: 200.0,
                falloff: 0.0,
                corner_radius: 50.0,
                ..DepthParams::default()
            },
            1.0,
        );
        assert_eq!(frame.hit(500.0, 450.0).unwrap().id, "b"); // overlapping body is occluded
        assert_eq!(frame.hit(330.0, 450.0).unwrap().id, "a"); // visible side of underlying body
        assert_eq!(frame.hit(451.0, 301.0).unwrap().id, "a"); // front cutout reveals underlying body
        assert!(frame.hit(251.0, 301.0).is_none()); // outermost rounded cutout
        assert!(frame.hit(550.0, 301.0).is_some());
        let outside = depth_frame(
            &items,
            1,
            None,
            None,
            1.0,
            [50.0, 50.0],
            [-20.0, 1200.0],
            DepthParams {
                spacing: 0.0,
                ..DepthParams::default()
            },
            1.0,
        );
        assert!(outside.hit(-20.0, 25.0).is_none());
        let empty = depth_frame(
            &[],
            2,
            None,
            None,
            0.0,
            [1.0, 1.0],
            [0.5, 1.0],
            DepthParams::default(),
            1.0,
        );
        assert_eq!(empty.visible_ids().count(), 0);
        assert!(empty.hit(0.5, 0.5).is_none());
    }

    #[test]
    fn d1_crop_content_relative_bounds_fallback_and_pointer_independence() {
        for aspect in [0.25, 0.5, 1.0, 2.0, 4.0] {
            let (cx, cy, cw, ch) = if aspect >= 1.0 {
                (0.0, (1.0 - 1.0 / aspect) / 2.0, 1.0, 1.0 / aspect)
            } else {
                ((1.0 - aspect) / 2.0, 0.0, aspect, 1.0)
            };
            for size in [[100.0, 1600.0], [2400.0, 100.0], [280.0, 520.0]] {
                for pan in [-2.0, -1.0, -0.25, 0.0, 0.5, 1.0, 2.0] {
                    let crop = depth_crop_uv(aspect, size, pan);
                    assert!(crop[0] >= cx - 1e-6 && crop[1] >= cy - 1e-6);
                    assert!(
                        crop[0] + crop[2] <= cx + cw + 1e-6 && crop[1] + crop[3] <= cy + ch + 1e-6
                    );
                    assert!(crop[2] > 0.0 && crop[3] > 0.0);
                    // Independent cover fractions computed from aspect ratios.
                    let ratio = (size[0] / size[1]) / aspect;
                    assert!((crop[2] / cw - ratio.min(1.0) / 1.4).abs() < 1e-5);
                    assert!((crop[3] / ch - (1.0 / ratio).min(1.0) / 1.4).abs() < 1e-5);
                }
            }
        }
        let items = [DepthItem {
            id: "wide",
            master_index: 42,
            source_aspect: None,
        }];
        let frame = depth_frame(
            &items,
            7,
            None,
            None,
            0.0,
            [1200.0, 900.0],
            [600.0, 1200.0],
            DepthParams::default(),
            1.0,
        );
        let before = frame.cards[0].crop_uv;
        for point in [[0.0, 0.0], [600.0, 450.0], [1100.0, 800.0]] {
            let _ = frame.hit(point[0], point[1]);
        }
        assert_eq!(before, frame.cards[0].crop_uv);
        assert_eq!(
            frame.cards[0].card.poster_aspect_ratio,
            DEFAULT_POSTER_ASPECT
        );
        assert_eq!(
            before,
            depth_crop_uv(DEFAULT_POSTER_ASPECT, frame.cards[0].card.size, 0.0)
        );
    }

    #[test]
    fn d1_fading_edge_crop_continues_until_body_omission() {
        let ids: Vec<_> = (0..8).map(|i| i.to_string()).collect();
        let items: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| DepthItem {
                id,
                master_index: i,
                source_aspect: Some(0.5),
            })
            .collect();
        let make = |camera| {
            depth_frame(
                &items,
                1,
                None,
                None,
                camera,
                [1200.0, 900.0],
                [600.0, 1200.0],
                DepthParams::default(),
                1.0,
            )
        };
        let fading = make(3.25);
        let edge = fading.cards.iter().find(|x| x.id == "6").unwrap();
        assert_eq!(edge.card.opacity, 0.15625);
        assert!(edge.parallax > 0.9 && edge.parallax < 1.0);
        assert!(edge.crop_uv[0] > 0.25);
        let omitted = make(3.0);
        assert!(!omitted.visible_ids().any(|id| id == "6"));
        assert!(omitted.cards.iter().all(|card| card.card.opacity > 0.0001));
    }
}

/// One owned, revision-bound frame shared by the actual widget, instance path,
/// inline detail composition, and poster demand. Catalog indices are master IDs.
#[derive(Debug, Clone)]
pub(crate) struct DepthSnapshotCard {
    pub id: String,
    pub filtered_index: usize,
    pub card: SliceCard,
    pub crop_uv: [f32; 4],
    pub hovered: bool,
    pub selection_progress: f32,
    /// Captured shader-only visual state for outgoing bodies, never actions.
    pub visual_detail_progress: Option<f32>,
    pub retained_texture: Option<Poster>,
}
#[derive(Debug, Clone)]
pub(crate) struct DepthSnapshot {
    pub revision: u64,
    pub viewport: [f32; 2],
    pub selection_frame: bool,
    pub shadows: bool,
    pub entrance: f32,
    pub launch: crate::donor_scene::LaunchAnimation,
    pub detail: crate::donor_scene::DepthDetailPresentation,
    pub cards: Vec<DepthSnapshotCard>,
    /// Visual-only outgoing stream. Never consumed by hit/detail/chrome.
    pub outgoing: Vec<DepthSnapshotCard>,
    /// Upcoming current-window demand before the zero-width incoming cut;
    /// bounded alongside geometry, so hidden incoming bodies can preload.
    pub live_poster_indices: Vec<usize>,
    pub poster_anchor: usize,
}
/// Exact successful upload ownership is required before any Depth image body
/// may sample a layer. A failed/unavailable live upload uses the existing solid
/// placeholder; an unavailable outgoing image is omitted. Physical old atlas
/// bytes may remain, but no instance can sample them as a different identity.
fn depth_resident_instances(
    frame: &DepthSnapshot,
    wanted: &[Poster],
    resident: &[Poster],
    colors: &GalleryColors,
    scale: f32,
    scene: &DonorSceneState,
) -> Vec<InstanceRaw> {
    let available = |layer| {
        wanted
            .iter()
            .find(|p| p.layer == layer)
            .is_some_and(|wanted| resident.iter().any(|p| p.same_resource(wanted)))
    };
    // Only texture bindings change; never compute a second geometry frame.
    let mut frame = frame.clone();
    for body in &mut frame.cards {
        if body.card.layer.is_some_and(|layer| !available(layer)) {
            body.card.layer = None;
        }
    }
    frame.outgoing.retain(|body| {
        body.retained_texture.as_ref().is_some_and(|exact| {
            body.card.layer == Some(exact.layer)
                && wanted.iter().any(|p| p.same_resource(exact))
                && resident.iter().any(|p| p.same_resource(exact))
        })
    });
    crate::donor_scene::depth_instances(&frame, colors, scale, scene)
}

/// Donor reel cut applied to the same logical geometry and content-relative
/// crop consumed by drawing, rounded/skewed input and inline detail. Widths
/// here are full widths (InstanceRaw uses half widths). A <0.5px half-width
/// body is omitted, matching the donor outgoing/body eligibility threshold.
pub(crate) fn depth_roll_cut(body: &mut DepthSnapshotCard, fraction: f32, incoming: bool) -> bool {
    let fraction = fraction.clamp(0.0, 1.0);
    let half_width = body.card.size[0] * 0.5;
    if incoming {
        body.card.center[0] += half_width * (1.0 - fraction);
        body.card.size[0] *= fraction;
        body.crop_uv[2] *= fraction;
    } else {
        body.card.center[0] -= half_width * fraction;
        body.card.size[0] *= 1.0 - fraction;
        body.crop_uv[0] += body.crop_uv[2] * fraction;
        body.crop_uv[2] *= 1.0 - fraction;
    }
    body.card.corner_radius = body
        .card
        .corner_radius
        .min(body.card.size[0] * 0.5)
        .min(body.card.size[1] * 0.5);
    body.card.size[0].is_finite()
        && body.card.size[0] >= 1.0
        && body.card.size[1].is_finite()
        && body.card.size[1] > 0.0
        && body.card.opacity > 0.0001
}

pub(crate) fn depth_roll_cell(center: [f32; 2]) -> (i32, i32) {
    (
        (center[0] * 0.5).round() as i32,
        (center[1] * 0.5).round() as i32,
    )
}

pub(crate) fn depth_roll_start(x: f32, viewport_width: f32) -> f32 {
    -0.6 * (x / viewport_width.max(1.0)).clamp(0.0, 1.0)
}

pub(crate) fn depth_roll_step(dt: f32, duration_ms: f32) -> f32 {
    if !dt.is_finite() || dt <= 0.0 {
        return 0.0;
    }
    dt.min(0.05) * 1000.0 / duration_ms.max(50.0)
}

pub(crate) fn depth_roll_fraction(phase: f32) -> f32 {
    let t = phase.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl DepthSnapshot {
    pub fn poster_indices(&self, filtered: &[usize]) -> Vec<usize> {
        let mut result: Vec<_> = self
            .live_poster_indices
            .iter()
            .copied()
            .take(MAX_POSTERS)
            .collect();
        if filtered.is_empty() {
            return result;
        }
        let center = self.poster_anchor.min(filtered.len() - 1);
        // Visible front-to-back priority, then nearest neighbors with the
        // donor's stable lower-index tie. Work and storage stay bounded by
        // the resident-layer budget, independently of the catalog size.
        let mut distance = 0;
        while result.len() < MAX_POSTERS && distance < filtered.len() {
            if let Some(left) = center.checked_sub(distance) {
                let index = filtered[left];
                if !result.contains(&index) {
                    result.push(index);
                }
            }
            if result.len() == MAX_POSTERS {
                break;
            }
            if distance > 0 {
                if let Some(right) = center
                    .checked_add(distance)
                    .filter(|right| *right < filtered.len())
                {
                    let index = filtered[right];
                    if !result.contains(&index) {
                        result.push(index);
                    }
                }
            }
            distance += 1;
        }
        result
    }
    pub fn from_frame(frame: DepthFrame<'_>) -> Self {
        Self {
            revision: frame.revision,
            viewport: frame.viewport,
            selection_frame: false,
            shadows: false,
            entrance: 1.0,
            launch: crate::donor_scene::LaunchAnimation::None,
            detail: crate::donor_scene::DepthDetailPresentation::default(),
            outgoing: Vec::new(),
            poster_anchor: frame.cards.last().map_or(0, |entry| entry.filtered_index),
            live_poster_indices: frame.cards.iter().rev().map(|e| e.card.index).collect(),
            cards: frame
                .cards
                .into_iter()
                .map(|entry| DepthSnapshotCard {
                    id: entry.id.to_owned(),
                    filtered_index: entry.filtered_index,
                    card: entry.card,
                    crop_uv: entry.crop_uv,
                    hovered: entry.hovered,
                    selection_progress: if entry.card.selected { 1.0 } else { 0.0 },
                    visual_detail_progress: None,
                    retained_texture: None,
                })
                .collect(),
        }
    }
    pub fn hit(&self, x: f32, y: f32) -> Option<&DepthSnapshotCard> {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x > self.viewport[0]
            || y > self.viewport[1]
        {
            return None;
        }
        self.cards.iter().rev().find(|entry| {
            donor_layout::depth_contains(
                entry.card.center,
                entry.card.size,
                entry.card.skew_distance,
                [entry.card.corner_radius; 4],
                [x, y],
            )
        })
    }
}

#[cfg(test)]
mod d3_tests {
    use super::*;
    #[test]
    fn d3_integrated_painter_instances_reverse_hits_and_intermediate_camera_demand() {
        let ids: Vec<_> = (0..1000).map(|i| format!("stable-{i}")).collect();
        let filtered: Vec<_> = (0..1000).map(|i| 999 - i).collect();
        let items: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| DepthItem {
                id,
                master_index: filtered[i],
                source_aspect: Some(if i % 2 == 0 { 0.5 } else { 2.0 }),
            })
            .collect();
        let mut camera = crate::upstream_slices_adapter::GalleryMotionState::absolute_camera(
            0.0, 999.0, 1000.0, 280.0, 280.0,
        );
        let mut params = donor_layout::DepthParams::default();
        params.spacing = 0.0;
        for fraction in [0.0, 0.25, 0.5, 0.75] {
            let frame = DepthSnapshot::from_frame(depth_frame(
                &items,
                19,
                Some(&ids[998]),
                None,
                400.0 + fraction,
                [1200.0, 900.0],
                [600.0, 1200.0],
                params,
                1.0,
            ));
            let scene = DonorSceneState {
                stage: donor_layout::StageParams::default(),
                camera: 400.0 + fraction,
                camera_target: 999.0,
                camera_velocity: 0.0,
                detail_progress: 0.0,
                external_detail: false,
                flip_options: Default::default(),
                detail_source: None,
                detail_card: None,
                viewport: [1200.0, 900.0],
                time: 0.0,
                visibility: 1.0,
            };
            let raw =
                crate::donor_scene::depth_instances(&frame, &GalleryColors::default(), 1.0, &scene);
            assert_eq!(raw.len(), frame.cards.len());
            for (entry, instance) in frame.cards.iter().zip(&raw) {
                assert_eq!(entry.crop_uv, instance.crop);
                assert_eq!(entry.card.center, instance.rect[..2]);
            }
            let hit = frame.hit(600.0, 450.0).unwrap();
            assert_eq!(hit.id, frame.cards.last().unwrap().id);
            assert_eq!(frame.poster_indices(&filtered)[0], hit.card.index);
        }
        let mut intermediate = false;
        for revision in 0..200 {
            let moving = camera.tick(0.016);
            let frame = DepthSnapshot::from_frame(depth_frame(
                &items,
                revision,
                Some(&ids[999]),
                None,
                camera.value(),
                [1200.0, 900.0],
                [600.0, 1200.0],
                donor_layout::DepthParams::default(),
                1.0,
            ));
            let demand = frame.poster_indices(&filtered);
            assert!(demand.len() <= MAX_POSTERS);
            for card in &frame.cards {
                assert!(demand.contains(&card.card.index));
            }
            if (100.0..900.0).contains(&camera.value()) {
                intermediate = true;
                assert!(demand.iter().any(|i| (100..900).contains(i)));
                assert!(!demand.contains(&0));
            }
            if revision == 18 {
                let before = (camera.x, camera.velocity);
                camera.retarget_absolute(0.0);
                assert_eq!((camera.x, camera.velocity), before);
            }
            if !moving {
                break;
            }
        }
        assert!(intermediate);
        for count in [0, 1, 2, 3, 5] {
            let frame = DepthSnapshot::from_frame(depth_frame(
                &items[..count],
                6,
                items[..count].first().map(|i| i.id),
                None,
                0.0,
                [1200.0, 900.0],
                [600.0, 1200.0],
                donor_layout::DepthParams::default(),
                1.0,
            ));
            assert_eq!(frame.cards.is_empty(), count == 0);
            assert!(frame.poster_indices(&filtered[..count]).len() <= count);
        }
    }
    #[test]
    fn d3_large_catalog_demand_matches_visible_prefix_and_nearest_oracle() {
        let filtered: Vec<_> = (0..100_000).map(|i| 100_000 - i).collect();
        let ids: Vec<_> = filtered.iter().map(|i| format!("item-{i}")).collect();
        let items: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| DepthItem {
                id,
                master_index: filtered[i],
                source_aspect: None,
            })
            .collect();
        for camera in [0.0, 0.25, 50_000.5, 99_999.0] {
            let frame = DepthSnapshot::from_frame(depth_frame(
                &items,
                1,
                Some(&ids[0]),
                None,
                camera,
                [1200.0, 900.0],
                [600.0, 1200.0],
                donor_layout::DepthParams::default(),
                1.0,
            ));
            let mut expected: Vec<_> = frame
                .cards
                .iter()
                .rev()
                .map(|entry| entry.card.index)
                .take(MAX_POSTERS)
                .collect();
            let center = frame
                .cards
                .last()
                .map(|entry| {
                    filtered
                        .iter()
                        .position(|index| *index == entry.card.index)
                        .unwrap()
                })
                .unwrap_or(0);
            let mut oracle: Vec<_> = filtered.iter().enumerate().collect();
            oracle.sort_by_key(|(position, _)| position.abs_diff(center));
            for (_, index) in oracle {
                if expected.len() == MAX_POSTERS {
                    break;
                }
                if !expected.contains(index) {
                    expected.push(*index);
                }
            }
            assert_eq!(frame.poster_indices(&filtered), expected);
            assert_eq!(expected.len(), MAX_POSTERS);
        }
    }

    #[test]
    fn d3_default_instance_frame_uses_depth_owned_style_and_crop() {
        let items = [DepthItem {
            id: "selected",
            master_index: 7,
            source_aspect: Some(0.5),
        }];
        let frame = DepthSnapshot::from_frame(depth_frame(
            &items,
            1,
            Some("selected"),
            None,
            0.0,
            [1200.0, 900.0],
            [600.0, 1200.0],
            donor_layout::DepthParams::default(),
            1.0,
        ));
        let scene = DonorSceneState {
            stage: donor_layout::StageParams::default(),
            camera: 0.0,
            camera_target: 0.0,
            camera_velocity: 0.0,
            detail_progress: 0.0,
            external_detail: false,
            flip_options: Default::default(),
            detail_source: None,
            detail_card: None,
            viewport: [1200.0, 900.0],
            time: 0.0,
            visibility: 1.0,
        };
        let raw =
            crate::donor_scene::depth_instances(&frame, &GalleryColors::default(), 1.0, &scene);
        assert_eq!(
            raw[0].params[1], 0.0,
            "Depth default must ignore the old global frame switch"
        );
        assert_eq!(raw[0].border[3], 0.0);
        assert_eq!(raw[0].crop, frame.cards[0].crop_uv);
        assert_eq!(raw[0].shape[2], 1.0);
    }
}

#[cfg(test)]
mod d4b_cut_tests {
    use super::*;

    #[test]
    fn d4b_actual_resident_binding_omits_unavailable_or_overwritten_old_layer() {
        let ids = [DepthItem {
            id: "exact",
            master_index: 7,
            source_aspect: Some(2.0),
        }];
        let mut frame = DepthSnapshot::from_frame(depth_frame(
            &ids,
            1,
            Some("exact"),
            None,
            0.0,
            [1200.0, 900.0],
            [600.0, 1200.0],
            Default::default(),
            1.0,
        ));
        let exact = video_placeholder_poster(std::path::Path::new("exact-old.png"), 3);
        frame.cards[0].card.layer = Some(3);
        let mut ghost = frame.cards[0].clone();
        ghost.retained_texture = Some(exact.clone());
        frame.outgoing = vec![ghost];
        let scene = DonorSceneState {
            stage: Default::default(),
            camera: 0.0,
            camera_target: 0.0,
            camera_velocity: 0.0,
            detail_progress: 0.0,
            external_detail: false,
            flip_options: Default::default(),
            detail_source: None,
            detail_card: None,
            viewport: frame.viewport,
            time: 0.0,
            visibility: 1.0,
        };
        let colors = GalleryColors::default();
        let raw = depth_resident_instances(&frame, &[exact.clone()], &[], &colors, 1.0, &scene);
        assert_eq!(
            raw.len(),
            1,
            "unavailable old image is omitted while live geometry remains"
        );
        assert_eq!(
            raw[0].params[3], 0.0,
            "live texture failure is a truthful solid placeholder"
        );
        let raw = depth_resident_instances(
            &frame,
            &[exact.clone()],
            &[exact.clone()],
            &colors,
            1.0,
            &scene,
        );
        assert_eq!(raw.len(), 2);
        assert!(raw
            .iter()
            .all(|body| body.misc[1] == 3 && body.params[3] == 1.0));
        // A different resource using the very same ID/path/layer cannot be an
        // exact old image, even if its upload succeeded in the shared atlas.
        let changed = video_placeholder_poster(std::path::Path::new("exact-old.png"), 3);
        assert!(!changed.same_resource(&exact));
        let raw =
            depth_resident_instances(&frame, &[changed.clone()], &[changed], &colors, 1.0, &scene);
        assert_eq!(
            raw.len(),
            1,
            "bare layer/path agreement never authorizes a wrong-image ghost"
        );
        assert_eq!(raw[0].rect[2], frame.cards[0].card.size[0] * 0.5);
    }

    #[test]
    fn d4b_bounded_candidate_resolution_preserves_large_index_order() {
        let count = 1_000_000;
        let mut seen = Vec::new();
        let params = donor_layout::DepthParams {
            visible_count: 21,
            ..Default::default()
        };
        let f = depth_frame_for(
            count,
            |index| {
                seen.push(index);
                Some(DepthItem {
                    id: "same",
                    master_index: index,
                    source_aspect: None,
                })
            },
            1,
            None,
            None,
            123456.5,
            [1200.0, 900.0],
            [600.0, 1200.0],
            params,
            1.0,
        );
        assert!(seen.len() <= 24);
        assert!(seen.iter().all(|i| (123445..=123468).contains(i)));
        assert_eq!(
            seen,
            f.cards.iter().map(|c| c.card.index).collect::<Vec<_>>()
        );
        let last = f
            .cards
            .iter()
            .rev()
            .take(2)
            .map(|c| c.card.index)
            .collect::<Vec<_>>();
        assert_eq!(last, [123457, 123456]);
    }

    #[test]
    fn d4b_extreme_cut_does_not_leave_hits_outside_drawn_quad() {
        let ids = [DepthItem {
            id: "cut",
            master_index: 7,
            source_aspect: Some(2.0),
        }];
        let mut f = DepthSnapshot::from_frame(depth_frame(
            &ids,
            1,
            Some("cut"),
            None,
            0.0,
            [1200.0, 900.0],
            [600.0, 1200.0],
            donor_layout::DepthParams {
                skew: 100.0,
                ..Default::default()
            },
            1.0,
        ));
        assert!(depth_roll_cut(&mut f.cards[0], 0.01, true));
        let body = &f.cards[0].card;
        let p = [
            body.center[0] - body.skew_distance * 0.25,
            body.center[1] + body.size[1] * 0.25,
        ];
        assert!(p[0] < body.center[0] - body.size[0] * 0.5 - 2.0);
        assert!(f.hit(p[0],p[1]).is_none(),"an extreme narrow/skewed cut must not retain a target beyond the raster quad, away from AA");
    }

    #[test]
    fn d4b_shader_silhouette_oracle_matches_cut_hits_away_from_aa() {
        let ids = [DepthItem {
            id: "oracle",
            master_index: 7,
            source_aspect: Some(0.5),
        }];
        for skew in [-100.0, 0.0, 100.0] {
            for radius in [0.0, 45.0] {
                let base = DepthSnapshot::from_frame(depth_frame(
                    &ids,
                    1,
                    None,
                    None,
                    0.0,
                    [1200.0, 900.0],
                    [600.0, 1200.0],
                    donor_layout::DepthParams {
                        skew,
                        corner_radius: radius,
                        ..Default::default()
                    },
                    1.0,
                ));
                for fraction in [0.01, 0.1, 0.25, 0.5, 1.0] {
                    let mut f = base.clone();
                    assert!(depth_roll_cut(&mut f.cards[0], fraction, true));
                    let b = &f.cards[0].card;
                    let hw = b.size[0] * 0.5;
                    let hh = b.size[1] * 0.5;
                    let sx = b.skew_distance * 0.5;
                    let bx = (hw - sx.abs()).max(1.0);
                    let by = hh.max(1.0);
                    let r = b.corner_radius;
                    for ix in -12..=12 {
                        for iy in -12..=12 {
                            let x = ix as f32 * (hw + sx.abs() + 6.0) / 10.0;
                            let y = iy as f32 * (hh + 6.0) / 10.0;
                            // Independent production WGSL inverse and rounded SDF;
                            // shader's padded raster quad and viewport are separate clips.
                            let qx = x + sx * y / by;
                            let dx = qx.abs() - bx + r;
                            let dy = y.abs() - by + r;
                            let sdf = dx.max(dy).min(0.0) + dx.max(0.0).hypot(dy.max(0.0)) - r;
                            let clip = (x.abs() - hw).max(y.abs() - hh);
                            if sdf.abs() < 2.01 || clip.abs() < 2.01 {
                                continue;
                            }
                            let expected = sdf < 0.0 && clip < 0.0;
                            assert_eq!(f.hit(b.center[0]+x,b.center[1]+y).is_some(),expected,
                            "skew={skew} radius={radius} fraction={fraction} x={x} y={y} sdf={sdf} clip={clip}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn d4b_independent_cut_timing_crop_and_rounded_skew_oracle() {
        for aspect in [0.25, 0.5, 2.0, 4.0] {
            let ids = [DepthItem {
                id: "oracle",
                master_index: 7,
                source_aspect: Some(aspect),
            }];
            let params = donor_layout::DepthParams {
                corner_radius: 50.0,
                skew: 30.0,
                ..Default::default()
            };
            let base = DepthSnapshot::from_frame(depth_frame(
                &ids,
                9,
                Some("oracle"),
                None,
                0.0,
                [1200.0, 900.0],
                [600.0, 1200.0],
                params,
                1.0,
            ))
            .cards
            .remove(0);
            for phase in [-0.6, 0.0, 0.25, 0.5, 0.75, 1.0, 2.0] {
                let t: f32 = phase;
                let t = t.clamp(0.0, 1.0);
                let f = t * t * (3.0 - 2.0 * t);
                assert_eq!(depth_roll_fraction(phase), f);
                for incoming in [false, true] {
                    let mut cut = base.clone();
                    let included = depth_roll_cut(&mut cut, f, incoming);
                    let remaining = if incoming { f } else { 1.0 - f };
                    assert!((cut.card.size[0] - base.card.size[0] * remaining).abs() < 1e-4);
                    let offset = if incoming { 1.0 - f } else { -f };
                    assert!(
                        (cut.card.center[0]
                            - base.card.center[0]
                            - base.card.size[0] * 0.5 * offset)
                            .abs()
                            < 1e-4
                    );
                    assert!((cut.crop_uv[2] - base.crop_uv[2] * remaining).abs() < 1e-6);
                    assert!(
                        (cut.crop_uv[0]
                            - base.crop_uv[0]
                            - if incoming { 0.0 } else { base.crop_uv[2] * f })
                        .abs()
                            < 1e-6
                    );
                    assert_eq!(included, cut.card.size[0] >= 1.0);
                    assert_eq!(cut.card.skew_distance, base.card.skew_distance);
                    assert!(cut.card.corner_radius <= cut.card.size[0] * 0.5);
                    assert!(cut.crop_uv[0] >= base.crop_uv[0] - 1e-6);
                    assert!(
                        cut.crop_uv[0] + cut.crop_uv[2] <= base.crop_uv[0] + base.crop_uv[2] + 1e-6
                    );
                }
            }
        }
        for x in [-1.0, 0.0, 300.0, 600.0, 1200.0, 2400.0] {
            assert_eq!(
                depth_roll_start(x, 1200.0),
                -0.6 * (x / 1200.0).clamp(0.0, 1.0)
            );
        }
        for duration in [0.0, 1.0, 50.0, 180.0, 250.0, 450.0] {
            assert_eq!(depth_roll_step(1.0, duration), 50.0 / duration.max(50.0));
            assert_eq!(depth_roll_step(0.025, duration), 25.0 / duration.max(50.0));
        }
        assert_eq!(depth_roll_step(f32::NAN, 450.0), 0.0);
    }
}

#[cfg(test)]
mod w4_cache_writer_tests {
    use super::*;
    #[test]
    fn w4_regeneration_preserves_existing_temp_hardlink_unknown_destination_and_refused_root() {
        let (profile, root) = crate::poster_worker::cache::tests::fixture();
        let _guard = crate::poster_worker::cache::RootGuard::open(&profile, &root, true)
            .unwrap()
            .unwrap();
        let destination = root.join("poster-0123456789abcdef.rgba");
        let recovery = profile.join("recovery.sentinel");
        std::fs::write(&recovery, b"must survive").unwrap();
        let temporary = destination.with_extension("rgba.1.tmp");
        std::fs::hard_link(&recovery, &temporary).unwrap();
        publish_poster_cache(&destination, b"new pixels", 1);
        assert!(!destination.exists());
        assert_eq!(std::fs::read(&recovery).unwrap(), b"must survive");
        assert!(temporary.exists());
        std::fs::write(&destination, b"unknown legacy format").unwrap();
        publish_poster_cache(&destination, b"new pixels", 2);
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"unknown legacy format"
        );
        assert!(!destination.with_extension("rgba.2.tmp").exists());
        let fresh = root.join("poster-1111111111111111.rgba");
        publish_poster_cache(&fresh, b"fresh pixels", 3);
        assert_eq!(std::fs::read(&fresh).unwrap(), b"fresh pixels");
        // A supplied root outside the actual current Config owner cannot receive cache writes.
        let source = profile.join("owned.png");
        image::RgbaImage::new(2, 2).save(&source).unwrap();
        let uncached = decode_poster_cached(&source, 0, &root).unwrap();
        assert!(matches!(uncached.recovery, PosterRecovery::Image(_)));
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"unknown legacy format"
        );
    }
}

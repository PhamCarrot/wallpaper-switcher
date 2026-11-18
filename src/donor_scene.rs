//! In-process presentation boundary adapted from the pinned skwd-wall scene.
//!
//! The donor's `InstanceRaw` contract and item shader are kept together here
//! so the  gallery no longer invents a second GPU card ABI.   still
//! owns catalog identities, bounded poster decoding, window state and all
//! provider actions; this module only projects an already-laid-out card into
//! the donor presentation payload.
//!
//! Source provenance: pinned skwd-wall v1.0.0-beta.13,
//! `contracts/rendering/snapshot.rs` and `shaders/item.wgsl`, commit
//! `9735be4ee1cf8b367773170e9e24c4f07ec3dd8b`.  The `Circle` shape code is a
//! small  compatibility extension because the donor scene represents
//! that shape through a later presentation branch.

use crate::donor_layout::StageParams;
use crate::gpu_gallery::{GalleryColors, SliceCard, POSTER_MATTE_RGB};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CardReadiness {
    Ready,
    Loading,
    Unavailable,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum LaunchAnimation {
    None,
    Fade,
    Rise,
    Zoom,
}

/// Wobble and detail share the ABI's flip vector, with exclusive meanings.
#[derive(Debug, Clone, Copy)]
pub(crate) enum SurfacePayload {
    Plain,
    Wobble { pad: f32, bend: f32, parallax: f32 },
    Detail,
}

#[derive(Debug, Clone)]
pub(crate) struct CardPresentation {
    pub card: SliceCard,
    pub id: String,
    pub source: std::path::PathBuf,
    pub catalog_generation: u64,
    pub selection: f32,
    pub hovered: bool,
    pub radii: [f32; 4],
    pub crop: [f32; 4],
    pub payload: SurfacePayload,
    pub readiness: CardReadiness,
    pub source_style: bool,
    pub hex_style: bool,
    /// Source Wall outer surface and keyboard/hover border alpha/width.
    pub wall_border: Option<(f32, f32)>,
    pub shadows: bool,
    pub loading_eligible: bool,
    pub entrance: f32,
    pub launch: LaunchAnimation,
    pub outgoing: bool,
    pub roll: f32,
    pub captured_raw: Option<Vec<InstanceRaw>>,
    pub retained: Option<crate::gpu_gallery::Poster>,
}

pub(crate) fn presented_instances(
    entries: &[CardPresentation],
    colors: &GalleryColors,
    scale: f32,
    scene: &DonorSceneState,
) -> Vec<InstanceRaw> {
    let mut result = Vec::new();
    for entry in entries {
        let first = result.len();
        if let Some(captured) = &entry.captured_raw {
            if entry.card.layer.is_none() {
                continue;
            }
            for raw in captured {
                let mut raw = *raw;
                raw.rect = raw.rect.map(|v| v * scale);
                raw.radii = raw.radii.map(|v| v * scale);
                raw.params[0] *= scale;
                raw.params[1] *= scale;
                raw.shape[0] *= scale;
                roll_raw(&mut raw, entry.roll, false);
                if raw.rect[2] >= 0.5 * scale {
                    result.push(raw);
                }
            }
            continue;
        }
        let card = &entry.card;
        let mut raw = instance_for_card(
            card,
            colors,
            scale,
            if scene.external_detail {
                0.0
            } else {
                scene.detail_progress
            },
        );
        if card.selected
            && !scene.external_detail
            && matches!(entry.payload, SurfacePayload::Detail)
        {
            let flags = scene.flip_options;
            raw.flip = card_flip_shader_payload(
                card_flip_phases(
                    scene.detail_progress,
                    flags.animate_shader,
                    flags.animate_back,
                )
                .shader,
                card.index,
                flags.effect,
            );
            raw.misc[3] &= !32;
        }
        raw.radii = entry.radii.map(|r| r * scale);
        raw.crop = entry.crop;
        if entry.loading_eligible && entry.readiness == CardReadiness::Loading {
            raw.misc[3] |= 2;
        }
        if entry.source_style {
            let selected = entry.selection.clamp(0.0, 1.0);
            let base = if entry.hovered {
                [colors.accent[0], colors.accent[1], colors.accent[2], 0.4]
            } else {
                [0.0, 0.0, 0.0, 0.6]
            };
            raw.border = std::array::from_fn(|i| {
                base[i] + ((if i == 3 { 1.0 } else { colors.accent[i] }) - base[i]) * selected
            });
            raw.params[1] = (1.0 + 2.0 * selected) * scale;
            raw.tint = [
                0.0,
                0.0,
                0.0,
                (if entry.hovered { 0.15 } else { 0.4 }) * (1.0 - selected),
            ];
            raw.shape[2] = 1.0;
            raw.misc[3] &= !4;
        }
        if entry.hex_style {
            let sel = entry.selection.clamp(0.0, 1.0);
            let (alpha, width) = if sel > 0.01 {
                (0.55 + 0.45 * sel, 1.5 + 1.5 * sel)
            } else if entry.hovered {
                (0.55, 2.0)
            } else {
                (0.5, 1.5)
            };
            raw.border = if sel > 0.01 || entry.hovered {
                [colors.accent[0], colors.accent[1], colors.accent[2], alpha]
            } else {
                [0.0, 0.0, 0.0, alpha]
            };
            raw.params[1] = width * scale;
            raw.tint = [0.0; 4];
            raw.shape[2] = 1.0;
            raw.misc[3] &= !4;
        }
        if let Some((alpha, width)) = entry.wall_border {
            raw.misc[2] = 1;
            raw.misc[3] &= !4;
            raw.tint = [0.0; 4];
            raw.shape[2] = 1.0;
            raw.border = [colors.accent[0], colors.accent[1], colors.accent[2], alpha];
            raw.params[1] =
                width.clamp(0.0, (card.size[0].min(card.size[1]) * 0.5 - 1.0).max(0.0)) * scale;
            let mut surface = raw;
            surface.fill = [colors.surface[0], colors.surface[1], colors.surface[2], 0.6];
            surface.tint = [0.0; 4];
            surface.border = [0.0; 4];
            surface.params = [0.0, 0.0, card.opacity, 0.0];
            surface.misc = [0, 0, 1, 0];
            surface.flip = [0.0; 4];
            result.push(surface);
        }
        if entry.shadows {
            let mut shadow = raw;
            let (x, y, alpha) = if entry.hex_style {
                (3.0, 7.0, 0.35 * entry.selection.clamp(0.0, 1.0))
            } else if card.selected {
                (4.0, 10.0, 0.5)
            } else {
                (2.0, 5.0, 0.3)
            };
            shadow.rect[0] += x * scale;
            shadow.rect[1] += y * scale;
            shadow.fill = [0.0, 0.0, 0.0, alpha];
            shadow.tint = [0.0; 4];
            shadow.border = [0.0; 4];
            shadow.params[1] = 0.0;
            shadow.params[3] = 0.0;
            shadow.misc = [0, 0, 0, raw.misc[3] & (1 | 0xff00)];
            shadow.flip = [0.0; 4];
            result.push(shadow);
        }
        if let SurfacePayload::Wobble {
            pad,
            bend,
            parallax,
        } = entry.payload
        {
            raw.rect[2] *= pad;
            raw.rect[3] *= pad;
            raw.misc[3] |= 32;
            raw.flip = [0.0, parallax, pad, bend];
        }
        roll_raw(&mut raw, entry.roll, true);
        if raw.rect[2] >= 0.5 * scale {
            result.push(raw);
        }
        for raw in &mut result[first..] {
            raw.params[2] *= entry.entrance.clamp(0.0, 1.0);
            if entry.wall_border.is_none() {
                apply_launch(raw, entry.entrance, entry.launch, scene.viewport, scale);
            }
        }
    }
    let cards: Vec<_> = entries
        .iter()
        .filter(|e| !e.outgoing)
        .map(|e| e.card)
        .collect();
    append_external_detail(&mut result, &cards, colors, scale, scene, None);
    result
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DepthDetailPresentation {
    pub effect: u32,
    pub animate_shader: bool,
    pub animate_back: bool,
}
impl Default for DepthDetailPresentation {
    fn default() -> Self {
        Self {
            effect: 0,
            animate_shader: true,
            animate_back: true,
        }
    }
}
/// Depth retains the painter order, fitted crop and ID-bound style in its
/// current frame. Old modes continue through the original instance path.
pub(crate) fn depth_instances(
    frame: &crate::gpu_gallery::DepthSnapshot,
    colors: &GalleryColors,
    scale: f32,
    scene: &DonorSceneState,
) -> Vec<InstanceRaw> {
    let mut result = depth_body_instances(
        &frame.cards,
        frame,
        colors,
        scale,
        if scene.external_detail {
            0.0
        } else {
            scene.detail_progress
        },
    );
    result.extend(depth_body_instances(
        &frame.outgoing,
        frame,
        colors,
        scale,
        0.0,
    ));
    if scene.external_detail {
        let cards: Vec<_> = frame.cards.iter().map(|entry| entry.card).collect();
        append_external_detail(
            &mut result,
            &cards,
            colors,
            scale,
            scene,
            Some(frame.detail),
        );
    }
    result
}

fn depth_body_instances(
    bodies: &[crate::gpu_gallery::DepthSnapshotCard],
    frame: &crate::gpu_gallery::DepthSnapshot,
    colors: &GalleryColors,
    scale: f32,
    detail_progress: f32,
) -> Vec<InstanceRaw> {
    bodies
        .iter()
        .flat_map(|entry| {
            let detail_progress = entry.visual_detail_progress.unwrap_or(detail_progress);
            let mut style = *colors;
            style.show_selection_border = frame.selection_frame;
            style.selection_border_width = if entry.card.selected { 3.0 } else { 1.0 };
            style.unselected_brightness = 1.0;
            style.border_glow = false;
            let mut raw = instance_for_card(&entry.card, &style, scale, detail_progress);
            raw.crop = entry.crop_uv;
            let phases = card_flip_phases(
                detail_progress,
                frame.detail.animate_shader,
                frame.detail.animate_back,
            );
            raw.flip = if entry.card.selected {
                card_flip_shader_payload(phases.shader, entry.card.index, frame.detail.effect)
            } else {
                [0.0; 4]
            };
            raw.misc[3] |= 262144; // Depth clean rectangle/style; ABI unchanged.
            raw.misc[3] &= !2; // The old selection sweep is not Depth's frame.
            raw.tint = [
                0.0,
                0.0,
                0.0,
                if entry.card.selected {
                    0.0
                } else {
                    (if entry.hovered { 0.15 } else { 0.4 })
                        * (1.0 - entry.selection_progress.clamp(0.0, 1.0))
                },
            ];
            raw.shape[2] = 1.0;
            let progress = entry.selection_progress.clamp(0.0, 1.0);
            let base = if entry.hovered {
                [colors.accent[0], colors.accent[1], colors.accent[2], 0.4]
            } else {
                [0.0, 0.0, 0.0, 0.6]
            };
            for (i, channel) in raw.border.iter_mut().enumerate() {
                let selected = if i == 3 { 1.0 } else { colors.accent[i] };
                *channel = base[i] + (selected - base[i]) * progress;
            }
            raw.params[1] = (1.0 + 2.0 * progress) * scale;
            if !frame.selection_frame
                && entry.card.skew_distance == 0.0
                && entry.card.corner_radius == 0.0
            {
                raw.misc[3] |= 131072;
            }
            if !style.show_selection_border {
                raw.border[3] = 0.0;
                raw.params[1] = 0.0;
            }
            if frame.shadows {
                let mut shadow = raw;
                let (x, y, alpha) = if entry.card.selected {
                    (4.0, 10.0, 0.5)
                } else {
                    (2.0, 5.0, 0.3)
                };
                shadow.rect[0] += x * scale;
                shadow.rect[1] += y * scale;
                shadow.fill = [0.0, 0.0, 0.0, alpha];
                shadow.tint = [0.0; 4];
                shadow.border = [0.0; 4];
                shadow.params[1] = 0.0;
                shadow.params[3] = 0.0;
                shadow.misc = [0, 0, 0, raw.misc[3] & (131072 | 262144)];
                shadow.flip = [0.0; 4];
                vec![shadow, raw]
            } else {
                vec![raw]
            }
        })
        .collect()
}

/// Frame state shared by the adapted donor scene boundary.  still owns
/// the event loop and catalog, but the snapshot carries the donor concepts
/// that drive placement, motion and detail rendering together.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DonorSceneState {
    pub(crate) stage: StageParams,
    pub(crate) camera: f32,
    pub(crate) camera_target: f32,
    pub(crate) camera_velocity: f32,
    pub(crate) detail_progress: f32,
    pub(crate) external_detail: bool,
    pub(crate) flip_options: DepthDetailPresentation,
    pub(crate) detail_source: Option<crate::donor_detail::CardGeometry>,
    pub(crate) detail_card: Option<SliceCard>,
    pub(crate) viewport: [f32; 2],
    pub(crate) time: f32,
    pub(crate) visibility: f32,
}

/// Gallery and separate external overlay share a bounded instance stream.
pub(crate) fn scene_instances(
    cards: &[SliceCard],
    colors: &GalleryColors,
    scale: f32,
    scene: &DonorSceneState,
) -> Vec<InstanceRaw> {
    let mut result: Vec<_> = cards
        .iter()
        .map(|card| {
            instance_for_card(
                card,
                colors,
                scale,
                if scene.external_detail {
                    0.0
                } else {
                    scene.detail_progress
                },
            )
        })
        .collect();
    append_external_detail(&mut result, cards, colors, scale, scene, None);
    result
}

fn append_external_detail(
    result: &mut Vec<InstanceRaw>,
    cards: &[SliceCard],
    colors: &GalleryColors,
    scale: f32,
    scene: &DonorSceneState,
    depth: Option<DepthDetailPresentation>,
) {
    let p = scene.detail_progress.clamp(0.0, 1.0);
    if !scene.external_detail || p <= 0.001 {
        return;
    }
    let Some(source) = cards
        .iter()
        .find(|card| card.selected)
        .or(scene.detail_card.as_ref())
    else {
        return;
    };
    let ease = 1.0 - (1.0 - p).powi(2);
    let reveal = ((p - 0.42) / 0.58).clamp(0.0, 1.0);
    // Depth keeps normal panel metrics throughout reveal/reversal. Old external
    // modes retain their original source-to-target geometry animation.
    let geometry = if depth.is_some() {
        crate::donor_detail::external_target(scene.viewport)
    } else {
        crate::donor_detail::external_body(
            scene
                .detail_source
                .unwrap_or(crate::donor_detail::CardGeometry {
                    center: source.center,
                    size: source.size,
                    rotation: 0.0,
                    skew_distance: 0.0,
                    edge_tilt_distance: 0.0,
                }),
            scene.viewport,
            p,
        )
    };
    let solid = |center: [f32; 2], size: [f32; 2], alpha: f32| InstanceRaw {
        rect: [
            center[0] * scale,
            center[1] * scale,
            size[0] * 0.5 * scale,
            size[1] * 0.5 * scale,
        ],
        fill: [0.0, 0.0, 0.0, alpha],
        params: [0.0, 0.0, 1.0, 0.0],
        radii: [0.0; 4],
        tint: [0.0; 4],
        border: [0.0; 4],
        uv: [0.0, 0.0, 1.0, 1.0],
        crop: [0.0, 0.0, 1.0, 1.0],
        misc: [0; 4],
        flip: [0.0; 4],
        shape: [0.0, 0.0, 1.0, 0.0],
    };
    result.push(solid(
        [scene.viewport[0] * 0.5, scene.viewport[1] * 0.5],
        scene.viewport,
        0.72 * ease,
    ));
    result.push(solid(
        [geometry.center[0], geometry.center[1] + 14.0 * ease],
        [
            geometry.size[0] + 16.0 * ease,
            geometry.size[1] + 16.0 * ease,
        ],
        0.45 * ease,
    ));
    let mut body = source.clone();
    body.center = geometry.center;
    body.size = geometry.size;
    body.shape = 0;
    body.triangle_direction = 0;
    body.rotation = 0.0;
    body.corner_radius = 0.0;
    body.skew_distance = 0.0;
    body.edge_tilt_distance = 0.0;
    body.opacity = (p * 8.0).min(1.0);
    let mut instance = instance_for_card(&body, colors, scale, p);
    {
        let flags = depth.unwrap_or(scene.flip_options);
        instance.flip = card_flip_shader_payload(
            card_flip_phases(p, flags.animate_shader, flags.animate_back).shader,
            body.index,
            flags.effect,
        );
    }
    instance.misc[3] &= !(2 | 4);
    instance.border = [0.0; 4];
    instance.tint = [0.0, 0.0, 0.0, 0.20 * reveal];
    result.push(instance);
}

/// Timing phases copied from the pinned donor's `scene/card_flip.rs`.
///
///  keeps the surrounding detail surface and state machine, but the
/// selected poster now uses the same shader handoff windows as the donor while
/// it opens. Keeping these phases here prevents the GPU adapter from inventing
/// a second detail animation contract.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CardFlipPhases {
    pub(crate) shader: f32,
    pub(crate) surface: f32,
    pub(crate) content: f32,
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn window(progress: f32, start: f32, end: f32) -> f32 {
    smoothstep((progress - start) / (end - start))
}

/// Donor-compatible detail handoff phases for an animated back surface.
pub(crate) fn card_flip_phases(
    progress: f32,
    animate_shader: bool,
    animate_back: bool,
) -> CardFlipPhases {
    const SHADER_END: f32 = 0.82;
    const SURFACE_START: f32 = 0.74;
    const SURFACE_END: f32 = 0.94;
    const CONTENT_START: f32 = 0.84;
    const CONTENT_END: f32 = 1.0;
    const BACK_ONLY_SURFACE_START: f32 = 0.05;
    const BACK_ONLY_SURFACE_END: f32 = 0.65;
    const BACK_ONLY_CONTENT_START: f32 = 0.25;
    const BACK_ONLY_CONTENT_END: f32 = 0.85;

    let progress = progress.clamp(0.0, 1.0);
    let shader = if animate_shader {
        (progress / SHADER_END).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (surface, content) = match (animate_shader, animate_back) {
        (true, true) => (
            window(progress, SURFACE_START, SURFACE_END),
            ((progress - CONTENT_START) / (CONTENT_END - CONTENT_START)).clamp(0.0, 1.0),
        ),
        (false, true) => (
            window(progress, BACK_ONLY_SURFACE_START, BACK_ONLY_SURFACE_END),
            ((progress - BACK_ONLY_CONTENT_START)
                / (BACK_ONLY_CONTENT_END - BACK_ONLY_CONTENT_START))
                .clamp(0.0, 1.0),
        ),
        (true, false) => {
            let revealed = f32::from(progress >= SHADER_END);
            (revealed, revealed)
        }
        (false, false) => {
            let revealed = f32::from(progress >= 0.5);
            (revealed, revealed)
        }
    };
    CardFlipPhases {
        shader,
        surface,
        content,
    }
}

pub(crate) fn card_flip_shader_payload(
    shader_progress: f32,
    card_index: usize,
    effect: u32,
) -> [f32; 4] {
    if shader_progress <= 0.001 {
        return [0.0; 4];
    }
    [
        shader_progress,
        (card_index as f32 % 7.0) * 1.3 + 0.5,
        effect as f32,
        0.0,
    ]
}

pub(crate) fn poster_crop_uv(source_aspect_ratio: f32, card_size: [f32; 2]) -> [f32; 4] {
    let source_aspect = if source_aspect_ratio.is_finite() && source_aspect_ratio > 0.0 {
        source_aspect_ratio
    } else {
        16.0 / 9.0
    };
    let card_aspect = (card_size[0].max(1.0) / card_size[1].max(1.0)).max(0.001);
    let (content_x, content_y, content_w, content_h) = if source_aspect >= 1.0 {
        (
            0.0,
            (1.0 - 1.0 / source_aspect) * 0.5,
            1.0,
            1.0 / source_aspect,
        )
    } else {
        ((1.0 - source_aspect) * 0.5, 0.0, source_aspect, 1.0)
    };
    let (crop_x, crop_y, crop_w, crop_h) = if card_aspect >= source_aspect {
        let height = (source_aspect / card_aspect).clamp(0.001, 1.0);
        (0.0, (1.0 - height) * 0.5, 1.0, height)
    } else {
        let width = (card_aspect / source_aspect).clamp(0.001, 1.0);
        ((1.0 - width) * 0.5, 0.0, width, 1.0)
    };
    [
        content_x + crop_x * content_w,
        content_y + crop_y * content_h,
        crop_w * content_w,
        crop_h * content_h,
    ]
}

/// Per-card payload consumed by the donor `item.wgsl` presentation shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct InstanceRaw {
    pub(crate) rect: [f32; 4],
    pub(crate) radii: [f32; 4],
    pub(crate) fill: [f32; 4],
    pub(crate) tint: [f32; 4],
    pub(crate) border: [f32; 4],
    pub(crate) params: [f32; 4],
    pub(crate) uv: [f32; 4],
    pub(crate) crop: [f32; 4],
    pub(crate) misc: [u32; 4],
    pub(crate) flip: [f32; 4],
    pub(crate) shape: [f32; 4],
}

/// Convert a  card shape into the donor shader's shape flags.
fn shape_flags(shape: u32, triangle_direction: u8) -> u32 {
    match shape {
        // Donor triangle directions are encoded as 1..=4.
        4 => 1 + u32::from(triangle_direction.min(1)),
        // Donor has two diamond branches and a hex fallback.
        3 => 5,
        // Circle is a  compatibility branch in the copied shader.
        1 => 7,
        // Hexagon is the donor fallback branch.
        2 => 0,
        _ => 0,
    }
}

/// Project a rendered  card into the pinned donor presentation ABI.
pub(crate) fn instance_for_card(
    card: &SliceCard,
    colors: &GalleryColors,
    scale: f32,
    detail_progress: f32,
) -> InstanceRaw {
    let scale = scale.max(0.01);
    let mut flags = 0_u32;
    if card.shape != 0 {
        flags |= 1 | (shape_flags(card.shape, card.triangle_direction) << 8);
    }
    if card.selected && colors.border_glow && colors.border_glow_radius > 0.0 {
        flags |= 4;
    }

    // Keep the selected card on the donor's back-facing presentation once the
    // detail handoff settles. Resetting this payload at 1.0 made the poster
    // reappear underneath 's separate detail sheet instead of completing
    // the card-associated metadata presentation.
    let flip = if card.selected && detail_progress > 0.001 {
        let phases = card_flip_phases(detail_progress, true, true);
        card_flip_shader_payload(phases.shader, card.index, 0)
    } else {
        [0.0; 4]
    };
    // Pinned view_slices enables bit 32 only for unflipped wobble cards, with
    // a different payload (pad/bend/parallax). Detail uses flip_color instead.
    // Setting it here interpreted the flip seed as parallax, clamped the
    // poster edge and bypassed the back-facing shader handoff entirely.

    // The poster textures decode sRGB on sampling. Match that linear value
    // when a card body has no resident texture to sample.
    let [r, g, b] = POSTER_MATTE_RGB;
    InstanceRaw {
        rect: [
            card.center[0] * scale,
            card.center[1] * scale,
            card.size[0] * 0.5 * scale,
            card.size[1] * 0.5 * scale,
        ],
        radii: [card.corner_radius * scale; 4],
        fill: iced::Color::from_rgb8(r, g, b).into_linear(),
        tint: [colors.surface[0], colors.surface[1], colors.surface[2], 0.0],
        border: [
            colors.accent[0],
            colors.accent[1],
            colors.accent[2],
            if card.selected { 1.0 } else { 0.0 },
        ],
        params: [
            card.skew_distance * scale,
            colors.selection_border_width.max(0.0) * scale,
            card.opacity.clamp(0.0, 1.0),
            if card.layer.is_some() { 1.0 } else { 0.0 },
        ],
        uv: [0.0, 0.0, 1.0, 1.0],
        crop: poster_crop_uv(card.poster_aspect_ratio, card.size),
        misc: [
            if card.layer.is_some() { 2 } else { 0 },
            card.layer.unwrap_or_default(),
            0,
            flags,
        ],
        flip,
        // Genuine world rotation belongs to vertices; triangle parity belongs
        // to the mask flags so upright posters keep their orientation.
        // The flip payload is fragment-only.
        // `shape.z` carries the existing unselected dim factor as a small
        // compatibility payload; selected cards retain full brightness.
        shape: [
            card.edge_tilt_distance * scale,
            card.rotation,
            if card.selected {
                1.0
            } else {
                colors.unselected_brightness.clamp(0.0, 1.0)
            },
            0.0,
        ],
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn external_detail_is_separate_and_retains_source_poster_crop() {
        use super::*;
        for aspect in [0.5625, 1.777778] {
            let card = SliceCard {
                triangle_direction: 0,
                index: 0,
                center: [300.0, 240.0],
                size: [280.0, 240.0],
                layer: Some(0),
                selected: true,
                opacity: 1.0,
                skew_distance: 0.0,
                edge_tilt_distance: 0.0,
                rotation: 0.0,
                corner_radius: 0.0,
                shape: 2,
                poster_aspect_ratio: aspect,
            };
            let colors = GalleryColors::default();
            let mut scene = DonorSceneState {
                stage: StageParams::default(),
                camera: 0.0,
                camera_target: 0.0,
                camera_velocity: 0.0,
                detail_progress: 0.0,
                external_detail: true,
                flip_options: Default::default(),
                detail_source: None,
                detail_card: None,
                viewport: [1920.0, 1080.0],
                time: 0.0,
                visibility: 1.0,
            };
            let idle = scene_instances(std::slice::from_ref(&card), &colors, 1.0, &scene);
            assert_eq!(idle.len(), 1);
            for p in [0.25, 0.5, 1.0] {
                scene.detail_progress = p;
                let raw = scene_instances(std::slice::from_ref(&card), &colors, 1.0, &scene);
                assert_eq!(raw.len(), 4);
                assert_eq!(bytemuck::bytes_of(&raw[0]), bytemuck::bytes_of(&idle[0]));
                let body = raw.last().unwrap();
                assert_eq!(body.misc[1], 0);
                assert_eq!(body.misc[3] & 1, 0);
                assert_eq!(
                    body.crop,
                    poster_crop_uv(aspect, [body.rect[2] * 2.0, body.rect[3] * 2.0])
                );
                assert!((raw[1].fill[3] - 0.72 * (1.0 - (1.0 - p).powi(2))).abs() < 0.0001);
            }
        }
    }
    use super::*;

    #[test]
    fn donor_instance_preserves_poster_layer_and_selection_border_without_loading() {
        let card = SliceCard {
            triangle_direction: 0,
            index: 2,
            center: [100.0, 80.0],
            size: [200.0, 120.0],
            layer: Some(4),
            selected: true,
            opacity: 0.75,
            skew_distance: 12.0,
            edge_tilt_distance: 3.0,
            rotation: 0.25,
            corner_radius: 8.0,
            shape: 2,
            poster_aspect_ratio: 16.0 / 9.0,
        };
        let instance = instance_for_card(&card, &GalleryColors::default(), 1.0, 0.0);
        assert_eq!(instance.misc[0], 2);
        assert_eq!(instance.misc[1], 4);
        assert_eq!(instance.misc[3] & 2, 0);
        assert_eq!(instance.border[3], 1.0);
        assert_eq!(instance.params[2], 0.75);
        assert_eq!(instance.shape[0], 3.0);
        assert_eq!(instance.shape[1], 0.25);
        assert_eq!(instance.shape[2], 1.0);
    }

    #[test]
    fn donor_instance_uses_poster_matte_for_unselected_card_without_texture() {
        let card = SliceCard {
            triangle_direction: 0,
            index: 0,
            center: [0.0, 0.0],
            size: [10.0, 10.0],
            layer: None,
            selected: false,
            opacity: 1.0,
            skew_distance: 0.0,
            edge_tilt_distance: 0.0,
            rotation: 0.0,
            corner_radius: 0.0,
            shape: 0,
            poster_aspect_ratio: 1.0,
        };
        let colors = GalleryColors {
            surface: [0.7, 0.4, 0.3],
            accent: [0.3, 0.6, 0.9],
            ..GalleryColors::default()
        };
        let instance = instance_for_card(&card, &colors, 1.0, 0.0);
        assert_eq!(
            instance.fill,
            iced::Color::from_rgb8(30, 30, 46).into_linear()
        );
        assert_eq!(instance.border, [0.3, 0.6, 0.9, 0.0]);
        assert_eq!(instance.misc[0], 0);
        assert_eq!(instance.misc[3] & 2, 0);
        assert_eq!(instance.params[3], 0.0);
        assert_eq!(
            instance.shape[2],
            GalleryColors::default().unselected_brightness
        );
    }

    #[test]
    fn donor_card_flip_phases_match_the_pinned_handoff_windows() {
        let showcase = card_flip_phases(0.74, true, true);
        assert!(showcase.shader > 0.9);
        assert_eq!(showcase.surface, 0.0);
        assert_eq!(showcase.content, 0.0);

        let readable = card_flip_phases(0.94, true, true);
        assert_eq!(readable.shader, 1.0);
        assert_eq!(readable.surface, 1.0);
        assert!(readable.content > 0.5);
    }

    #[test]
    fn selected_detail_projects_the_donor_flip_payload_without_affecting_idle_cards() {
        let selected = SliceCard {
            triangle_direction: 0,
            index: 4,
            center: [100.0, 80.0],
            size: [200.0, 120.0],
            layer: Some(1),
            selected: true,
            opacity: 1.0,
            skew_distance: 0.0,
            edge_tilt_distance: 0.0,
            rotation: 0.0,
            corner_radius: 8.0,
            shape: 0,
            poster_aspect_ratio: 16.0 / 9.0,
        };
        let instance = instance_for_card(&selected, &GalleryColors::default(), 1.0, 0.5);
        assert!(instance.flip[0] > 0.0);
        assert_eq!(instance.flip[2], 0.0);
        assert_eq!(instance.misc[3] & 32, 0);

        let idle = instance_for_card(&selected, &GalleryColors::default(), 1.0, 0.0);
        assert_eq!(idle.flip, [0.0; 4]);
        assert_eq!(idle.misc[3] & 32, 0);

        let settled = instance_for_card(&selected, &GalleryColors::default(), 1.0, 1.0);
        assert_eq!(settled.flip[0], 1.0);
        assert_eq!(settled.misc[3] & 32, 0);
        for index in 0..14 {
            for progress in [0.2, 0.5, 0.82, 1.0] {
                let card = SliceCard {
                    triangle_direction: 0,
                    index,
                    ..selected.clone()
                };
                let instance = instance_for_card(&card, &GalleryColors::default(), 1.0, progress);
                let phases = card_flip_phases(progress, true, true);
                assert_eq!(
                    instance.flip,
                    card_flip_shader_payload(phases.shader, index, 0)
                );
                assert_eq!(
                    instance.misc[3] & 32,
                    0,
                    "flip must never enter the wobble shader"
                );
            }
        }
    }

    #[test]
    fn poster_crop_uses_image_content_and_card_aspect() {
        let wide = poster_crop_uv(16.0 / 9.0, [1600.0, 900.0]);
        assert_eq!(wide, [0.0, 0.21875, 1.0, 0.5625]);

        let square = poster_crop_uv(16.0 / 9.0, [900.0, 900.0]);
        assert_eq!(square, [0.21875, 0.21875, 0.5625, 0.5625]);
    }
}

/// Source horizontal crop/half-width cuts. This is a reveal, not a vertex rotation.
pub(crate) fn roll_raw(raw: &mut InstanceRaw, fraction: f32, incoming: bool) {
    let f = fraction.clamp(0.0, 1.0);
    let hw = raw.rect[2];
    if incoming {
        raw.rect[0] += hw * (1.0 - f);
        raw.rect[2] = hw * f;
        raw.crop[2] *= f;
    } else {
        raw.rect[0] -= hw * f;
        raw.rect[2] = hw * (1.0 - f);
        raw.crop[0] += raw.crop[2] * f;
        raw.crop[2] *= 1.0 - f;
    }
    for r in &mut raw.radii {
        *r = r.min(raw.rect[2]).min(raw.rect[3]);
    }
}

pub(crate) fn presented_hit_card(entry: &CardPresentation) -> Option<SliceCard> {
    if entry.outgoing {
        return None;
    }
    let mut card = entry.card;
    card.center[0] += card.size[0] * 0.5 * (1.0 - entry.roll);
    card.size[0] *= entry.roll;
    card.corner_radius = card.corner_radius.min(card.size[0].min(card.size[1]) * 0.5);
    (card.size[0] >= 1.0).then_some(card)
}

/// The solid interior of the Slices body, excluding its shadow and AA fringe.
pub(crate) fn presented_slice_contains(
    entry: &CardPresentation,
    colors: &GalleryColors,
    viewport: [f32; 2],
    point: [f32; 2],
) -> bool {
    if entry.outgoing {
        return false;
    }
    let mut raw = instance_for_card(&entry.card, colors, 1.0, 0.0);
    raw.radii = entry.radii;
    let (pad, bend) = match entry.payload {
        SurfacePayload::Wobble { pad, bend, .. } => (pad, bend),
        _ => (1.0, 0.0),
    };
    raw.rect[2] *= pad;
    raw.rect[3] *= pad;
    roll_raw(&mut raw, entry.roll, true);
    if raw.rect[2] < 0.5 {
        return false;
    }
    apply_launch(&mut raw, entry.entrance, entry.launch, viewport, 1.0);
    let [cx, cy, mut hw, mut hh] = raw.rect;
    let (sin, cos) = (-raw.shape[1]).sin_cos();
    let [x, y] = [point[0] - cx, point[1] - cy];
    let [mut x, mut y] = [x * cos - y * sin, x * sin + y * cos];
    if x.abs() > hw || y.abs() > hh {
        return false;
    }
    if pad != 1.0 && entry.card.layer.is_some() {
        hw /= pad;
        hh /= pad;
        let yn = (y / hh.max(1.0)).clamp(-1.35, 1.35);
        x -= bend * hw * 0.5 * yn * yn;
        y *= 1.0 + bend.abs() * 0.12 * (1.0 - (x / hw.max(1.0)).abs().min(1.0));
    }
    // Mirrors sd_sheared_rounded_box in skwd_item.wgsl.
    let sx = raw.params[0] * 0.5;
    let ty = raw.shape[0] * 0.5;
    let bx = (hw - sx.abs()).max(1.0);
    let by = (hh - ty.abs()).max(1.0);
    let det = bx * by - sx * ty;
    let det = if det.abs() < 1.0 {
        if det >= 0.0 {
            1.0
        } else {
            -1.0
        }
    } else {
        det
    };
    let qx = (by * x + sx * y) / det * bx;
    let qy = (ty * x + bx * y) / det * by;
    let r = raw.radii[match (qy < 0.0, qx < 0.0) {
        (true, true) => 0,
        (true, false) => 1,
        (false, false) => 2,
        (false, true) => 3,
    }];
    let dx = (qx.abs() - bx + r).max(0.0);
    let dy = (qy.abs() - by + r).max(0.0);
    dx.hypot(dy) <= r
}

fn apply_launch(
    raw: &mut InstanceRaw,
    p: f32,
    launch: LaunchAnimation,
    viewport: [f32; 2],
    scale: f32,
) {
    match launch {
        LaunchAnimation::Rise => raw.rect[1] += (1.0 - p).powi(2) * viewport[1] * 0.03 * scale,
        LaunchAnimation::Zoom => {
            let k = 0.94 + 0.06 * p;
            for (i, c) in viewport.into_iter().enumerate() {
                raw.rect[i] = c * 0.5 * scale + (raw.rect[i] - c * 0.5 * scale) * k;
                raw.rect[i + 2] *= k;
            }
        }
        _ => {}
    }
}

/// Transform the authoritative current card; returns the uniform geometry scale.
pub(crate) fn launch_card(
    card: &mut SliceCard,
    p: f32,
    launch: LaunchAnimation,
    viewport: [f32; 2],
) -> f32 {
    let p = p.clamp(0.0, 1.0);
    match launch {
        LaunchAnimation::Rise => {
            card.center[1] += (1.0 - p).powi(2) * viewport[1] * 0.03;
            1.0
        }
        LaunchAnimation::Zoom => {
            let k = 0.94 + 0.06 * p;
            for i in 0..2 {
                card.center[i] = viewport[i] * 0.5 + (card.center[i] - viewport[i] * 0.5) * k;
                card.size[i] *= k;
            }
            card.corner_radius *= k;
            card.skew_distance *= k;
            card.edge_tilt_distance *= k;
            k
        }
        _ => 1.0,
    }
}

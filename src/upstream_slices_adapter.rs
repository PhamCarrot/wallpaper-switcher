//! Narrow adapter for the pinned skwd-wall Slices presentation.
//!
//! The layout helpers are adapted from the Windows-adapted `skwd-wall`
//! prototype at commit `9735be4ee1cf8b367773170e9e24c4f07ec3dd8b`,
//! `src/frontend/scene/layout.rs`.  The upstream renderer's full SceneCore,
//! atlas and daemon graph is intentionally not imported here.   keeps its
//! own bounded poster cache and `WallpaperCoordinator`; this module only
//! carries the presentation math and the catalog-index seam needed to preserve
//!  wallpaper identities.
//!
//! The adapted source is GPL-3.0-or-later.  Provenance and the reuse boundary
//! are recorded in `docs/third_party/SKWD-WALL-NOTICE.md`.

use crate::scanner::WallpaperItem;

/// One card placement produced by the pinned donor Grid row calculation.
///
/// The donor's `GridLayout::Uniform`, `Brick` and `Cylinder` modes share this
/// row/column placement before their stage and cylinder transforms.
/// consumes the placement in its existing poster renderer, so atlas, texture
/// and SceneCore ownership stay outside this boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridPlacement {
    pub index: usize,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Adapt the pinned `view_grid::grid_placements` calculation.
///
/// `brick_stagger` is enabled only for the donor Brick variant; Uniform and
/// Cylinder pass `false`. The result is raw content-space data so  can
/// apply its own viewport clipping, poster cache and renderer.
pub fn donor_grid_row_placements(
    item_count: usize,
    columns: usize,
    thumb_width: f32,
    thumb_height: f32,
    gap_x: f32,
    gap_y: f32,
    stagger: f32,
    brick_stagger: bool,
) -> Vec<GridPlacement> {
    let columns = columns.max(1);
    let cell_width = thumb_width.max(1.0) + gap_x;
    let cell_height = thumb_height.max(1.0) + gap_y;
    (0..item_count)
        .map(|index| {
            let row = index / columns;
            let column = index % columns;
            let stagger_offset = if brick_stagger && row % 2 == 1 {
                cell_width * stagger
            } else {
                0.0
            };
            GridPlacement {
                index,
                x: column as f32 * cell_width + stagger_offset,
                y: row as f32 * cell_height,
                width: thumb_width.max(1.0),
                height: thumb_height.max(1.0),
            }
        })
        .collect()
}

/// The geometry/motion state consumed by 's Slices renderer.
///
/// Keeping the filtered window, card centers, camera offset and opacity span
/// in one value mirrors the pinned scene's rebuild contract.  The adapter
/// deliberately stops before the donor's atlas/SceneCore ownership boundary;
///  still owns poster uploads and rendering.
#[derive(Debug, Clone, PartialEq)]
pub struct SliceLayoutState {
    pub start: usize,
    pub end: usize,
    pub centers: Vec<f32>,
    /// Fixed viewport reference for donor-style opacity falloff.
    /// Card centers may translate during camera motion, but the fade span
    /// does not move with them.
    pub reference_center_x: f32,
    pub center_x: f32,
    pub center_y: f32,
    pub stride: f32,
    pub expanded_layout_width: f32,
    pub half_view: f32,
}

/// Build the filtered Slices geometry and its transient navigation position.
///
/// This is adapted from the pinned `view_slices` rebuild path.  It does not
/// create or own textures, providers, windows or mutation state.
pub fn build_slice_layout(
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    offset_x: f32,
    offset_y: f32,
    width: f32,
    expanded_width: f32,
    spacing: f32,
    skew: f32,
    visible_count: usize,
    motion: f32,
) -> Option<SliceLayoutState> {
    if item_count == 0 {
        return None;
    }

    let selected = selected.min(item_count - 1);
    let visible_count = visible_count.max(1).min(item_count);
    let mut start = selected.saturating_sub(visible_count / 2);
    let end = (start + visible_count).min(item_count);
    start = end.saturating_sub(visible_count);
    let settled_center_x = viewport[0] * (0.5 + offset_x * 0.5);
    let center_y = viewport[1] * (0.5 + offset_y * 0.5);
    let width = width.max(1.0);
    let expanded_width = expanded_width.max(1.0);
    let stride = (slice_layout_width(width, skew) + spacing).max(1.0);
    let center_x = settled_center_x + motion * stride;
    let width_of = |index: usize| {
        if index == selected {
            expanded_width
        } else {
            width
        }
    };

    let mut centers = vec![center_x; end - start];
    for index in (selected + 1)..end {
        let previous = index - 1;
        centers[index - start] = centers[previous - start]
            + slice_layout_width(width_of(previous), skew) * 0.5
            + spacing
            + slice_layout_width(width_of(index), skew) * 0.5;
    }
    for index in (start..selected).rev() {
        let next = index + 1;
        centers[index - start] = centers[next - start]
            - slice_layout_width(width_of(next), skew) * 0.5
            - spacing
            - slice_layout_width(width_of(index), skew) * 0.5;
    }

    let expanded_layout_width = slice_layout_width(expanded_width, skew);
    let strip_width = (expanded_layout_width + visible_count.saturating_sub(1) as f32 * stride)
        .min((viewport[0] - 40.0).max(1.0));

    Some(SliceLayoutState {
        start,
        end,
        centers,
        reference_center_x: settled_center_x,
        center_x,
        center_y,
        stride,
        expanded_layout_width,
        half_view: strip_width * 0.5,
    })
}

/// Keep 's existing normalized transition helper for callers that need a
/// stateless value. The included gallery paths use [`GalleryMotionState`]
/// below so their camera can be retargeted without losing position or
/// velocity.
pub fn slice_motion_value(direction: f32, progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    let eased = progress * progress * (3.0 - 2.0 * progress);
    let direction = if direction == 0.0 {
        0.0
    } else {
        direction.signum()
    };
    direction * (1.0 - eased)
}

/// A small donor-shaped spring for the transient gallery camera offset.
///
///  keeps its existing normalized motion value and renderer, so this is
/// deliberately limited to the camera transition rather than importing the
/// donor SceneCore/atlas graph. Slices, Geometric and Wall share this bounded
/// offset. A reversal retargets the in-flight camera to the new direction and
/// then settles back to the centered layout.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GalleryMotionState {
    pub x: f32,
    pub velocity: f32,
    pub target: f32,
    settle_after_retarget: bool,
    policy: MotionPolicy,
    // Absolute coordinates lose fractional integration steps at large indices.
    // Retain only the rounding remainder; the shared kernel stays unchanged.
    position_remainder: f32,
    stiffness: f32,
    damping: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum MotionPolicy {
    Normalized,
    Source,
    Absolute { pixels_per_unit: f32 },
}

/// Shared semi-implicit critical-damped integration. Preserve normalized
/// arithmetic/order exactly; absolute cameras feed target-relative coordinates.
fn integrate_gallery_spring(
    x: &mut f32,
    velocity: &mut f32,
    target: f32,
    stiffness: f32,
    damping: f32,
    dt_seconds: f32,
) {
    let mut remaining = dt_seconds.clamp(0.0, 0.05);
    let step_limit = 1.0 / 240.0;
    while remaining > 0.0 {
        let step = remaining.min(step_limit);
        *velocity += (-stiffness * (*x - target) - damping * *velocity) * step;
        *x += *velocity * step;
        remaining -= step;
    }
}

impl GalleryMotionState {
    /// beta.24 pixel/value spring, with source damping and default precision.
    pub(crate) fn source(current: f32, target: f32, duration_ms: f32, zeta: f32) -> Self {
        let mut spring = Self::absolute_camera(current, target, duration_ms, 1.0, 1.0);
        spring.policy = MotionPolicy::Source;
        spring.set_zeta(zeta);
        spring
    }

    pub(crate) fn set_zeta(&mut self, zeta: f32) {
        self.damping = 2.0 * self.stiffness.sqrt() * zeta;
    }

    pub(crate) fn retarget_source(&mut self, target: f32) {
        assert!(matches!(self.policy, MotionPolicy::Source));
        assert!(target.is_finite());
        self.target = target;
    }
    #[cfg(test)]
    pub fn for_duration_ms(current: f32, direction: f32, duration_ms: u64) -> Self {
        Self::with_velocity(current, 0.0, direction, duration_ms)
    }

    pub fn with_velocity(current: f32, velocity: f32, direction: f32, duration_ms: u64) -> Self {
        let direction = if direction == 0.0 {
            0.0
        } else {
            direction.signum()
        };
        let duration = (duration_ms as f32).max(1.0);
        let omega = 6.64 / (duration / 1000.0);
        let mut state = Self {
            x: current,
            velocity,
            target: 0.0,
            settle_after_retarget: false,
            policy: MotionPolicy::Normalized,
            position_remainder: 0.0,
            stiffness: omega * omega,
            damping: 2.0 * omega,
        };
        if direction != 0.0 && current.abs() < 0.05 {
            state.x = direction;
        } else if current.signum() != direction && direction != 0.0 {
            // The selected item changed direction while the old camera was
            // still moving. Keep the current position, retarget across the
            // new direction, then return to the settled center.
            state.target = direction;
            state.settle_after_retarget = true;
        }
        state
    }

    /// D1 absolute filtered camera; never uses normalized displacement policy.
    pub(crate) fn absolute_camera(
        current: f32,
        target: f32,
        duration_ms: f32,
        spacing: f32,
        width: f32,
    ) -> Self {
        assert!([current, target, duration_ms, spacing, width]
            .into_iter()
            .all(f32::is_finite));
        let omega = 6.64 / (duration_ms.clamp(35.0, 8000.0) / 1000.0);
        Self {
            x: current,
            velocity: 0.0,
            target,
            settle_after_retarget: false,
            position_remainder: 0.0,
            policy: MotionPolicy::Absolute {
                pixels_per_unit: spacing.max(width).max(1.0),
            },
            stiffness: omega * omega,
            damping: 2.0 * omega,
        }
    }

    pub(crate) fn retarget_absolute(&mut self, target: f32) {
        assert!(matches!(self.policy, MotionPolicy::Absolute { .. }));
        assert!(target.is_finite());
        self.target = target;
    }

    /// Geometry morph changes camera's pixel precision without retiming motion.
    pub(crate) fn set_absolute_extent(&mut self, spacing: f32, width: f32) {
        assert!([spacing, width].into_iter().all(f32::is_finite));
        if matches!(self.policy, MotionPolicy::Absolute { .. }) {
            self.policy = MotionPolicy::Absolute {
                pixels_per_unit: spacing.max(width).max(1.0),
            };
        }
    }

    /// Donor retime resets velocity, unlike ordinary navigation reversal.
    pub(crate) fn retime_absolute(&mut self, duration_ms: f32) {
        assert!(matches!(self.policy, MotionPolicy::Absolute { .. }));
        assert!(duration_ms.is_finite());
        let omega = 6.64 / (duration_ms.clamp(35.0, 8000.0) / 1000.0);
        self.stiffness = omega * omega;
        self.damping = 2.0 * omega;
        self.velocity = 0.0;
    }

    pub(crate) fn precision(&self) -> f32 {
        match self.policy {
            MotionPolicy::Normalized | MotionPolicy::Source => 0.05,
            MotionPolicy::Absolute { pixels_per_unit } => {
                (0.05 / pixels_per_unit).max(self.target.abs() * f32::EPSILON)
            }
        }
    }

    pub fn tick(&mut self, dt_seconds: f32) -> bool {
        match self.policy {
            MotionPolicy::Normalized => integrate_gallery_spring(
                &mut self.x,
                &mut self.velocity,
                self.target,
                self.stiffness,
                self.damping,
                dt_seconds,
            ),
            MotionPolicy::Absolute { .. } | MotionPolicy::Source => {
                let mut offset = (self.x - self.target) + self.position_remainder;
                integrate_gallery_spring(
                    &mut offset,
                    &mut self.velocity,
                    0.0,
                    self.stiffness,
                    self.damping,
                    dt_seconds,
                );
                self.x = self.target + offset;
                self.position_remainder = offset - (self.x - self.target);
            }
        }
        if self.settled() {
            self.x = self.target;
            self.position_remainder = 0.0;
            self.velocity = 0.0;
            if self.settle_after_retarget {
                self.target = 0.0;
                self.settle_after_retarget = false;
            }
        }
        !self.settled()
    }

    pub fn settled(&self) -> bool {
        let precision = self.precision();
        ((self.x - self.target) + self.position_remainder).abs() < precision
            && self.velocity.abs() < 10.0 * precision
    }

    pub fn value(&self) -> f32 {
        if self.settled() {
            self.target
        } else {
            self.x
        }
    }
}

pub fn slice_edge_tilt(base: f32, motion: f32, wobble: bool, wobble_strength: f32) -> f32 {
    base + if wobble {
        motion * wobble_strength * 18.0
    } else {
        0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowseIntent {
    Select,
    OpenDetail,
}

/// Keep the candidate's non-mutating browse contract in the adapter seam:
/// clicking another card selects it; clicking the selected card opens detail.
pub fn browse_intent(selected: Option<usize>, clicked: usize) -> BrowseIntent {
    if selected == Some(clicked) {
        BrowseIntent::OpenDetail
    } else {
        BrowseIntent::Select
    }
}

/// Resolve a layout index back to the stable  catalog index.
///
/// The upstream presentation works with filtered positions, while 's
/// favorites/playlists/coordinator use `WallpaperItem.id` at the master index.
pub fn resolve_master_index(
    wallpapers: &[WallpaperItem],
    filtered_indices: &[usize],
    filtered_position: usize,
) -> Option<usize> {
    let master_index = *filtered_indices.get(filtered_position)?;
    wallpapers.get(master_index).map(|_| master_index)
}

pub fn resolve_item<'a>(
    wallpapers: &'a [WallpaperItem],
    filtered_indices: &[usize],
    filtered_position: usize,
) -> Option<&'a WallpaperItem> {
    resolve_master_index(wallpapers, filtered_indices, filtered_position)
        .and_then(|index| wallpapers.get(index))
}

/// Adapted from skwd-wall's `slice_midline_width`.
pub fn slice_layout_width(width: f32, skew: f32) -> f32 {
    (width - skew.abs()).max(1.0)
}

/// Adapted from skwd-wall's `slice_opacity`.
pub fn slice_opacity(
    item_center_x: f32,
    view_center_x: f32,
    half_view: f32,
    expanded_layout_width: f32,
    slice_stride: f32,
) -> f32 {
    if half_view <= 0.0 {
        return 1.0;
    }
    let full_zone = 0.6_f32.min((expanded_layout_width / 2.0 + 2.0 * slice_stride) / half_view);
    let normalized_distance = (item_center_x - view_center_x).abs() / half_view;
    if normalized_distance <= full_zone {
        1.0
    } else {
        (1.0 - (normalized_distance - full_zone) / (1.2 - full_zone)).max(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn item(id: &str) -> WallpaperItem {
        WallpaperItem {
            scan_metadata: None,
            id: id.into(),
            title: id.into(),
            folder_path: PathBuf::from("C:/wallpapers"),
            project_json_path: PathBuf::new(),
            file_target: PathBuf::from(format!("C:/wallpapers/{id}.png")),
            preview_path: None,
            wallpaper_type: "image".into(),
            source: crate::scanner::WallpaperSource::LocalImage,
        }
    }

    #[test]
    fn browse_contract_never_applies() {
        assert_eq!(browse_intent(None, 2), BrowseIntent::Select);
        assert_eq!(browse_intent(Some(2), 2), BrowseIntent::OpenDetail);
    }

    #[test]
    fn filtered_positions_resolve_to_stable_gallery_items() {
        let wallpapers = vec![item("first"), item("second"), item("third")];
        let filtered = vec![2, 0];
        assert_eq!(resolve_master_index(&wallpapers, &filtered, 0), Some(2));
        assert_eq!(
            resolve_item(&wallpapers, &filtered, 1).map(|item| item.id.as_str()),
            Some("first")
        );
        assert_eq!(resolve_master_index(&wallpapers, &filtered, 2), None);
    }

    #[test]
    fn donor_grid_rows_preserve_order_and_brick_stagger() {
        let uniform = donor_grid_row_placements(5, 3, 100.0, 50.0, 10.0, 8.0, 0.5, false);
        assert_eq!(
            uniform,
            vec![
                GridPlacement {
                    index: 0,
                    x: 0.0,
                    y: 0.0,
                    width: 100.0,
                    height: 50.0,
                },
                GridPlacement {
                    index: 1,
                    x: 110.0,
                    y: 0.0,
                    width: 100.0,
                    height: 50.0,
                },
                GridPlacement {
                    index: 2,
                    x: 220.0,
                    y: 0.0,
                    width: 100.0,
                    height: 50.0,
                },
                GridPlacement {
                    index: 3,
                    x: 0.0,
                    y: 58.0,
                    width: 100.0,
                    height: 50.0,
                },
                GridPlacement {
                    index: 4,
                    x: 110.0,
                    y: 58.0,
                    width: 100.0,
                    height: 50.0,
                },
            ]
        );

        let brick = donor_grid_row_placements(5, 3, 100.0, 50.0, 10.0, 8.0, 0.5, true);
        assert_eq!(brick[3].x, 55.0);
        assert_eq!(brick[4].x, 165.0);
        assert_eq!(
            brick
                .iter()
                .map(|placement| placement.index)
                .collect::<Vec<_>>(),
            (0..5).collect::<Vec<_>>()
        );
    }

    #[test]
    fn upstream_slice_opacity_has_a_soft_tail() {
        let full_zone = slice_opacity(50.0, 0.0, 100.0, 200.0, 20.0);
        let tail = slice_opacity(115.0, 0.0, 100.0, 200.0, 20.0);
        assert_eq!(full_zone, 1.0);
        assert!(tail > 0.0 && tail < 1.0);
        assert_eq!(slice_layout_width(40.0, 48.0), 1.0);
    }

    #[test]
    fn slice_layout_keeps_filtered_window_and_motion_together() {
        let layout = build_slice_layout(
            8,
            4,
            [1000.0, 800.0],
            0.1,
            -0.2,
            120.0,
            300.0,
            10.0,
            80.0,
            5,
            1.0,
        )
        .expect("non-empty layout");

        assert_eq!((layout.start, layout.end), (2, 7));
        assert_eq!(layout.centers.len(), 5);
        assert!(layout.center_x > 500.0);
        assert!(layout.stride > 0.0);
        assert!(layout.half_view > 0.0);
    }

    #[test]
    fn slice_layout_keeps_first_and_last_items_inside_small_catalog_windows() {
        for selected in [0, 1, 2, 3] {
            let layout = build_slice_layout(
                4,
                selected,
                [1000.0, 800.0],
                0.0,
                0.0,
                120.0,
                300.0,
                10.0,
                48.0,
                7,
                0.0,
            )
            .expect("small catalog remains laid out");
            assert_eq!((layout.start, layout.end), (0, 4));
            assert_eq!(layout.centers[selected], layout.reference_center_x);
        }
    }

    #[test]
    fn slice_layout_accounts_for_selected_width_when_navigation_changes_target() {
        let first = build_slice_layout(
            5,
            1,
            [1000.0, 800.0],
            0.0,
            0.0,
            120.0,
            300.0,
            10.0,
            48.0,
            5,
            0.0,
        )
        .expect("first selected layout");
        let second = build_slice_layout(
            5,
            2,
            [1000.0, 800.0],
            0.0,
            0.0,
            120.0,
            300.0,
            10.0,
            48.0,
            5,
            0.0,
        )
        .expect("second selected layout");

        assert_eq!(first.centers[1 - first.start], first.reference_center_x);
        assert_eq!(second.centers[2 - second.start], second.reference_center_x);
        assert_eq!(first.stride, second.stride);
        assert_ne!(
            first.centers[2 - first.start] - first.reference_center_x,
            second.centers[1 - second.start] - second.reference_center_x
        );
    }

    #[test]
    fn slice_motion_uses_pinned_smoothstep_and_direction() {
        assert_eq!(slice_motion_value(1.0, 0.0), 1.0);
        assert_eq!(slice_motion_value(-1.0, 0.0), -1.0);
        assert_eq!(slice_motion_value(1.0, 1.0), 0.0);
        assert!(slice_motion_value(1.0, 0.5) > 0.0);
        assert_eq!(slice_motion_value(0.0, 0.5), 0.0);
    }

    #[test]
    fn gallery_motion_spring_settles_after_a_reversal() {
        let mut first = GalleryMotionState::for_duration_ms(0.0, 1.0, 250);
        assert_eq!(first.value(), 1.0);
        for _ in 0..8 {
            assert!(first.tick(0.016));
        }
        let in_flight = first.value();
        assert!(in_flight > 0.0 && in_flight < 1.0);

        let mut reversal = GalleryMotionState::with_velocity(in_flight, first.velocity, -1.0, 250);
        assert_eq!(reversal.x, in_flight);
        assert_eq!(reversal.target, -1.0);
        for _ in 0..240 {
            if !reversal.tick(0.016) {
                break;
            }
        }
        assert!(reversal.settled());
        assert_eq!(reversal.value(), 0.0);
    }

    #[test]
    fn gallery_motion_spring_keeps_current_position_when_retargeted() {
        let mut state = GalleryMotionState::with_velocity(0.4, -1.5, 1.0, 250);
        assert_eq!(state.x, 0.4);
        assert_eq!(state.target, 0.0);
        assert!(state.tick(0.016));
        assert!(state.value() < 0.4);
    }
}

#[cfg(test)]
mod depth_motion_tests {
    use super::*;
    #[test]
    fn p4_absolute_spring_preserves_fractional_duration_and_retime_policy() {
        let duration = 1000.25_f32;
        let mut motion = GalleryMotionState::absolute_camera(1.0, 5.0, duration, 280.0, 280.0);
        let omega = 6.64 / (duration / 1000.0);
        assert_eq!(motion.stiffness, omega * omega);
        assert_eq!(motion.damping, 2.0 * omega);
        let integer = GalleryMotionState::absolute_camera(1.0, 5.0, 1000.0, 280.0, 280.0);
        assert_ne!(motion.stiffness, integer.stiffness);
        assert!(motion.tick(0.016));
        let position = motion.x;
        let target = motion.target;
        motion.retime_absolute(321.5);
        let omega = 6.64 / (321.5 / 1000.0);
        assert_eq!(motion.stiffness, omega * omega);
        assert_eq!(motion.damping, 2.0 * omega);
        assert_eq!(
            (motion.x, motion.target, motion.velocity),
            (position, target, 0.0)
        );
    }
    #[test]
    fn d1_absolute_camera_does_not_use_normalized_zero_policy() {
        let mut motion = GalleryMotionState::absolute_camera(4.25, 9.0, 1000.0, 280.0, 280.0);
        assert_eq!((motion.x, motion.target, motion.velocity), (4.25, 9.0, 0.0));
        for _ in 0..1000 {
            if !motion.tick(1.0 / 60.0) {
                break;
            }
        }
        assert_eq!((motion.x, motion.target, motion.velocity), (9.0, 9.0, 0.0));
    }

    #[test]
    fn d1_camera_duration_matrix_delayed_frames_and_idle_snap() {
        for ms in [35, 83, 250, 1000, 4000, 8000] {
            for target in [0.0, 17.0, 100_000.0, 10_000_000.0] {
                let mut motion = GalleryMotionState::absolute_camera(
                    target + 20.0,
                    target,
                    ms as f32,
                    280.0,
                    600.0,
                );
                let mut elapsed = 0.0;
                let precision = (0.05_f32 / 600.0).max(target.abs() * f32::EPSILON);
                assert_eq!(motion.precision(), precision);
                for dt in [0.016, 0.025, 0.003, 0.05].into_iter().cycle().take(20_000) {
                    let running = motion.tick(dt);
                    assert!(motion.x.is_finite() && motion.velocity.is_finite());
                    elapsed += dt;
                    if !running {
                        // At small indices precision expresses a 0.05 pixel budget;
                        // large f32 indices explicitly use the representability floor.
                        assert_eq!((motion.x, motion.velocity), (target, 0.0));
                        assert!(elapsed < ms as f32 / 1000.0 * 8.0 + 1.0);
                        break;
                    }
                }
                assert!(
                    motion.settled(),
                    "ms={ms}, target={target}, x={}, v={}",
                    motion.x,
                    motion.velocity
                );
                assert!(!motion.tick(0.016));
            }
            let mut delayed =
                GalleryMotionState::absolute_camera(0.0, 4.0, ms as f32, 280.0, 280.0);
            let mut capped = delayed;
            delayed.tick(2.0);
            capped.tick(0.05);
            assert_eq!(delayed, capped);
            let mut no_frame = delayed;
            no_frame.tick(-0.5);
            assert_eq!(no_frame, delayed);
        }
    }

    #[test]
    fn d1_camera_fixed_substep_and_analytic_oracles() {
        let mut motion = GalleryMotionState::absolute_camera(0.0, 3.0, 1000.0, 280.0, 280.0);
        // One semi-implicit Euler step, independently evaluated in f64.
        let dt = 1.0_f32 / 240.0;
        let omega = 6.64_f64;
        let velocity = omega * omega * 3.0 * dt as f64;
        motion.tick(dt);
        assert!((motion.velocity as f64 - velocity).abs() < 1e-6);
        assert!((motion.x as f64 - velocity * dt as f64).abs() < 1e-6);
        // Critical-damped closed-form position provides a separate trajectory oracle.
        let mut motion = GalleryMotionState::absolute_camera(0.0, 3.0, 1000.0, 280.0, 280.0);
        for i in 1..=120 {
            motion.tick(1.0 / 240.0);
            let t = i as f64 / 240.0;
            let expected = 3.0 * (1.0 - (1.0 + omega * t) * (-omega * t).exp());
            assert!((motion.x as f64 - expected).abs() < 0.03);
        }
    }

    #[test]
    fn d1_camera_reversal_retains_velocity_but_duration_retime_resets_it() {
        for steps in [1, 8, 30, 100] {
            let mut motion = GalleryMotionState::absolute_camera(20.0, 30.0, 1000.0, 280.0, 280.0);
            for _ in 0..steps {
                motion.tick(0.016);
            }
            let before = (motion.x, motion.velocity);
            motion.retarget_absolute(10.0);
            assert_eq!((motion.x, motion.velocity), before);
            assert_eq!(motion.target, 10.0);
            let mut retimed = motion;
            retimed.retime_absolute(250.0);
            assert_eq!((retimed.x, retimed.target), (before.0, 10.0));
            assert_eq!(retimed.velocity, 0.0);
            assert!((retimed.stiffness.sqrt() - 26.56).abs() < 0.0001);
            for _ in 0..2000 {
                if !motion.tick(0.016) {
                    break;
                }
            }
            assert_eq!(
                (motion.x, motion.velocity, motion.target),
                (10.0, 0.0, 10.0)
            );
            motion.retarget_absolute(24.0);
            assert_eq!((motion.x, motion.velocity), (10.0, 0.0));
        }
        let mut motion = GalleryMotionState::absolute_camera(0.0, 10.0, 1000.0, 0.0, 1.0);
        assert_eq!(motion.precision(), 0.05);
        motion.retarget_absolute(1_000_000.0);
        assert_eq!(motion.precision(), 1_000_000.0 * f32::EPSILON);
    }

    #[test]
    fn d1_camera_final_subpixel_and_velocity_thresholds() {
        let mut motion = GalleryMotionState::absolute_camera(0.00005, 0.0, 1000.0, 1000.0, 400.0);
        motion.velocity = 0.0006;
        assert!(!motion.settled());
        motion.x = 0.000049;
        motion.velocity = 0.000499;
        assert!(motion.settled());
        let previous = motion.x;
        assert!(!motion.tick(0.0));
        assert!((previous - motion.x) * 1000.0 < 0.05);
        assert_eq!(motion.velocity, 0.0);
    }

    #[test]
    fn d1_old_normalized_constructors_keep_two_stage_behavior() {
        for ctor in [GalleryMotionState::for_duration_ms, |x, d, ms| {
            GalleryMotionState::with_velocity(x, 0.0, d, ms)
        }] {
            let mut forward = ctor(0.0, 1.0, 450);
            assert_eq!((forward.x, forward.target), (1.0, 0.0));
            for _ in 0..1000 {
                if !forward.tick(0.016) {
                    break;
                }
            }
            assert_eq!(
                (forward.x, forward.velocity, forward.target),
                (0.0, 0.0, 0.0)
            );
            let mut reversed = ctor(0.6, -1.0, 450);
            assert_eq!((reversed.x, reversed.target), (0.6, -1.0));
            assert!(reversed.settle_after_retarget);
            let mut saw_return = false;
            for _ in 0..1000 {
                let was_retarget = reversed.settle_after_retarget;
                let running = reversed.tick(0.016);
                if was_retarget && !reversed.settle_after_retarget {
                    assert_eq!(
                        (reversed.x, reversed.velocity, reversed.target),
                        (-1.0, 0.0, 0.0)
                    );
                    saw_return = true;
                }
                if !running {
                    break;
                }
            }
            assert!(saw_return);
            assert_eq!(
                (reversed.x, reversed.velocity, reversed.target),
                (0.0, 0.0, 0.0)
            );
        }
        let retained = GalleryMotionState::with_velocity(0.6, -2.0, -1.0, 450);
        assert_eq!(
            (retained.x, retained.velocity, retained.target),
            (0.6, -2.0, -1.0)
        );
    }
}

#[cfg(test)]
mod motion_repair_regressions {
    use super::GalleryMotionState;

    #[test]
    fn motion_repair_neutral_direction_never_injects_or_reverses_displacement() {
        for direction in [0.0_f32, -0.0_f32] {
            let idle = GalleryMotionState::with_velocity(0.0, 0.0, direction, 250);
            assert_eq!(idle.x, 0.0);
            assert_eq!(idle.target, 0.0);
            assert!(idle.settled());
            let resume = GalleryMotionState::with_velocity(-0.4, 1.5, direction, 250);
            assert_eq!(resume.x, -0.4);
            assert_eq!(resume.velocity, 1.5);
            assert_eq!(resume.target, 0.0);
            for current in [-0.04, 0.04] {
                let resume = GalleryMotionState::with_velocity(current, -3.0, direction, 250);
                assert_eq!(resume.x, current);
                assert_eq!(resume.velocity, -3.0);
                assert_eq!(resume.target, 0.0);
            }
        }
        assert_eq!(
            GalleryMotionState::with_velocity(0.0, 0.0, -1.0, 250).x,
            -1.0
        );
        assert_eq!(GalleryMotionState::with_velocity(0.0, 0.0, 1.0, 250).x, 1.0);
    }
}

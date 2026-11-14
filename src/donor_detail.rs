//! Donor-backed geometry for 's card-associated detail surface.
//!
//! The pinned `BackLayout` has two important presentation contracts: Slices
//! embeds the metadata surface inside the selected card, while the other
//! gallery modes reserve a narrow external rail.  keeps its own content
//! and messages; paths, field positions and inverse hits share this boundary so the
//! selected card, metadata, and actions settle as one composition.

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct CardGeometry {
    pub(crate) center: [f32; 2],
    pub(crate) size: [f32; 2],
    pub(crate) rotation: f32,
    pub(crate) skew_distance: f32,
    pub(crate) edge_tilt_distance: f32,
}

/// Transient choice latched by the current Depth detail identity until close.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DepthDetailMode {
    Inline,
    Panel,
}

impl DepthDetailMode {
    pub(crate) fn resolve(self, readable_inline: bool) -> Self {
        if self == Self::Panel || !readable_inline {
            Self::Panel
        } else {
            Self::Inline
        }
    }
}

impl CardGeometry {
    /// Pinned BackGeometry affine boundary, followed by 's card rotation.
    pub(crate) fn point(self, point: [f32; 2]) -> [f32; 2] {
        let hw = (self.size[0] * 0.5).max(1.0);
        let hh = (self.size[1] * 0.5).max(1.0);
        let sx = self.skew_distance * 0.5;
        let ty = self.edge_tilt_distance * 0.5;
        let x = point[0] - self.center[0];
        let y = point[1] - self.center[1];
        let px = x * (hw - sx.abs()).max(1.0) / hw - y * sx / hh;
        let py = y * (hh - ty.abs()).max(1.0) / hh - x * ty / hw;
        let (sin, cos) = self.rotation.sin_cos();
        [
            self.center[0] + px * cos - py * sin,
            self.center[1] + px * sin + py * cos,
        ]
    }

    pub(crate) fn inverse_point(self, point: [f32; 2]) -> Option<[f32; 2]> {
        let hw = (self.size[0] * 0.5).max(1.0);
        let hh = (self.size[1] * 0.5).max(1.0);
        let sx = self.skew_distance * 0.5;
        let ty = self.edge_tilt_distance * 0.5;
        let (a, b, c, d) = (
            (hw - sx.abs()).max(1.0) / hw,
            -ty / hw,
            -sx / hh,
            (hh - ty.abs()).max(1.0) / hh,
        );
        let det = a * d - b * c;
        if det.abs() < 0.000_001 {
            return None;
        }
        let (sin, cos) = (-self.rotation).sin_cos();
        let x = point[0] - self.center[0];
        let y = point[1] - self.center[1];
        let (x, y) = (x * cos - y * sin, x * sin + y * cos);
        Some([
            self.center[0] + (d * x - c * y) / det,
            self.center[1] + (-b * x + a * y) / det,
        ])
    }
}

/// Bounded adaptation of SceneCore::rebuild_flip_overlay. Gallery tiles are
/// never resized: the separate body travels from the selected source to target.
pub(crate) fn external_body(
    source: CardGeometry,
    viewport: [f32; 2],
    progress: f32,
) -> CardGeometry {
    let p = progress.clamp(0.0, 1.0);
    let ease = 1.0 - (1.0 - p).powi(2);
    let target = external_target(viewport);
    CardGeometry {
        center: std::array::from_fn(|i| {
            source.center[i] + (target.center[i] - source.center[i]) * ease
        }),
        size: std::array::from_fn(|i| source.size[i] + (target.size[i] - source.size[i]) * ease),
        rotation: 0.0,
        skew_distance: 0.0,
        edge_tilt_distance: 0.0,
    }
}

pub(crate) fn external_target(viewport: [f32; 2]) -> CardGeometry {
    CardGeometry {
        center: [viewport[0] * 0.5, viewport[1] * 0.5],
        size: [
            2.0 * (viewport[0] * 0.455).min((viewport[0] - 72.0).max(320.0) * 0.5),
            2.0 * (viewport[1] * 0.43).min((viewport[1] - 64.0).max(240.0) * 0.5),
        ],
        rotation: 0.0,
        skew_distance: 0.0,
        edge_tilt_distance: 0.0,
    }
}

/// 's existing retargetable frame interpolation, shared with its worker.
pub(crate) fn motion_value(start: f32, target: f32, frame_progress: f32) -> f32 {
    let p = frame_progress.clamp(0.0, 1.0);
    let eased = p * p * (3.0 - 2.0 * p);
    start + (target - start) * eased
}

#[cfg(test)]
mod tests {
    #[test]
    fn transformed_rectangles_have_matching_inverse_hits() {
        for rotation in [0.0, -0.4, 0.7] {
            for (skew, tilt) in [(48.0, 0.0), (-72.0, 30.0), (90.0, -60.0)] {
                let c = super::CardGeometry {
                    center: [500.0, 400.0],
                    size: [420.0, 520.0],
                    rotation,
                    skew_distance: skew,
                    edge_tilt_distance: tilt,
                };
                for p in [[320.0, 180.0], [680.0, 620.0], [500.0, 400.0]] {
                    let roundtrip = c.inverse_point(c.point(p)).unwrap();
                    assert!(
                        (roundtrip[0] - p[0]).abs() < 0.001 && (roundtrip[1] - p[1]).abs() < 0.001
                    );
                }
            }
        }
    }
    #[test]
    fn external_overlay_matches_pinned_source_target_and_midpoint() {
        let c = super::CardGeometry {
            center: [300.0, 240.0],
            size: [280.0, 240.0],
            rotation: 0.0,
            skew_distance: 0.0,
            edge_tilt_distance: 0.0,
        };
        assert_eq!(super::external_body(c, [1920.0, 1080.0], 0.0), c);
        let target = super::external_target([1920.0, 1080.0]);
        assert_eq!(target.center, [960.0, 540.0]);
        assert!((target.size[0] - 1747.2).abs() < 0.01 && (target.size[1] - 928.8).abs() < 0.01);
        assert_eq!(super::external_body(c, [1920.0, 1080.0], 1.0), target);
        let mid = super::external_body(c, [1920.0, 1080.0], 0.5);
        assert_eq!(mid.center, [795.0, 465.0]);
    }
    #[test]
    fn interrupted_motion_retargets_continuously_and_closes_to_each_original_source() {
        for x in [240.0, 1680.0] {
            let source = super::CardGeometry {
                center: [x, 250.0],
                size: [280.0, 240.0],
                rotation: 0.0,
                skew_distance: 0.0,
                edge_tilt_distance: 0.0,
            };
            let open = super::motion_value(0.0, 1.0, 0.4);
            let reversed = super::motion_value(open, 0.0, 0.0);
            assert_eq!(reversed, open);
            assert_eq!(
                super::external_body(source, [1920.0, 1080.0], reversed),
                super::external_body(source, [1920.0, 1080.0], open)
            );
            let close = super::motion_value(open, 0.0, 0.6);
            assert!(close >= 0.0 && close < open);
            let reopen = super::motion_value(close, 1.0, 0.0);
            assert_eq!(reopen, close);
            assert_eq!(
                super::external_body(
                    source,
                    [1920.0, 1080.0],
                    super::motion_value(close, 0.0, 1.0)
                ),
                source
            );
        }
    }
}

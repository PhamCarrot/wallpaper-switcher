#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DesktopRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl DesktopRect {
    pub fn width(self) -> u32 {
        (self.right - self.left).max(0) as u32
    }

    pub fn height(self) -> u32 {
        (self.bottom - self.top).max(0) as u32
    }

    fn overlap_area(self, other: Self) -> i64 {
        let width = (self.right.min(other.right) - self.left.max(other.left)).max(0) as i64;
        let height = (self.bottom.min(other.bottom) - self.top.max(other.top)).max(0) as i64;
        width * height
    }
}

pub fn best_rect_match(target: DesktopRect, candidates: &[DesktopRect]) -> Option<usize> {
    candidates
        .iter()
        .enumerate()
        .map(|(index, candidate)| (index, target.overlap_area(*candidate)))
        .filter(|(_, overlap)| *overlap > 0)
        .max_by_key(|(_, overlap)| *overlap)
        .map(|(index, _)| index)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_by_geometry_instead_of_enumeration_order() {
        let picker_monitor = DesktopRect {
            left: 1920,
            top: 0,
            right: 4480,
            bottom: 1440,
        };
        let native_order = [
            DesktopRect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1080,
            },
            picker_monitor,
        ];

        assert_eq!(best_rect_match(picker_monitor, &native_order), Some(1));
    }

    #[test]
    fn selects_the_largest_overlap_when_rectangles_are_not_identical() {
        let target = DesktopRect {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1080,
        };
        let candidates = [
            DesktopRect {
                left: -1280,
                top: 0,
                right: 0,
                bottom: 1024,
            },
            DesktopRect {
                left: 0,
                top: 0,
                right: 1919,
                bottom: 1079,
            },
        ];

        assert_eq!(best_rect_match(target, &candidates), Some(1));
    }
}

//! A small, -owned adaptation of the pinned donor FilterBar.
//!
//! The donor rail is a drawn, mode-aware composition rather than a row of
//! stock rounded buttons. This module keeps that presentation boundary pure:
//! it reports intents, while `gpu_candidate` continues to own filtering,
//! panels, monitor targeting, and provider actions.

use crate::donor_back_layout::{measured_lines, measured_width};
use crate::donor_folio::source_ui::typography;
use crate::library::color_metadata::ColorBucket;
use crate::library::SortOrder;
use iced::widget::{canvas, text};
use iced::{alignment, mouse, Color, Element, Font, Length, Pixels, Point, Rectangle, Size, Theme};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RailMode {
    Slices,
    Geometric,
    Wall,
}

#[cfg(test)]
mod tests {
    use super::*;
    fn model(mode: RailMode, width: f32) -> RailModel {
        RailModel {
            visibility: RailVisibility::default(),
            reveal: 1.0,
            max_width: width,
            viewport: [width + 32.0, 1080.0],
            vertical: false,
            max_height: 1080.0,
            scale: 1.0,
            rtl: false,
            resolution_label: "SIZE".into(),
            sort_order: SortOrder::Colour,
            sort_visibility: [true; 7],
            folder_active: false,
            folder_open: false,
            mode,
            title: None,
            source_labels: ["ALL".into(), "PIC".into(), "VID".into(), "WE".into()],
            source_selected: 1,
            shape_label: "SHAPE".into(),
            shape_active: false,
            folder_label: "FOLDER".into(),
            favorites_active: false,
            random_state: crate::candidate_state::RandomRotationState::Off,
            muted: false,
            playlists_active: false,
            search_active: false,
            settings_active: false,
            bucket: None,
            show_close: true,
            palette: RailPalette {
                background: Color::BLACK,
                surface: Color::BLACK,
                container: Color::BLACK,
                hover: Color::BLACK,
                active_text: Color::BLACK,
                text: Color::WHITE,
                accent: Color::WHITE,
                outline: Color::WHITE,
            },
            font: Font::default(),
            icon_font: Font::default(),
        }
    }
    #[test]
    fn ret03_folder_demand_reserves_painted_label_before_approved_cap() {
        for mode in [RailMode::Slices, RailMode::Geometric, RailMode::Wall] {
            let mut m = model(mode, 1896.0);
            for label in ["ALL WALLPAPERS", "Pictures"] {
                m.folder_label = label.into();
                let size = footprint(&m);
                let item = layout(&m, Rectangle::with_size(Size::new(size[0], size[1])))
                    .into_iter()
                    .find(|i| i.action == Some(RailIntent::ToggleFolder))
                    .unwrap();
                let content = folder_content(&item, 1.0);
                assert!(item.rect.width < 240.0, "ordinary label should be uncapped");
                assert!(
                    rail_text_width(label, label_size(1.0), m.font) <= content.label.width + 0.01,
                    "{mode:?} {label}"
                );
            }
            m.scale = 0.5;
            m.folder_label = "Long folder 音楽e\u{301} ".repeat(20);
            let size = footprint(&m);
            let item = layout(&m, Rectangle::with_size(Size::new(size[0], size[1])))
                .into_iter()
                .find(|i| i.action == Some(RailIntent::ToggleFolder))
                .unwrap();
            assert_eq!(
                item.rect.width,
                if mode == RailMode::Slices {
                    120.0
                } else {
                    115.0
                },
                "source visual width after cap"
            );
            let content = folder_content(&item, 0.5);
            assert!(
                rail_text_width(&m.folder_label, label_size(m.scale), m.font) > content.label.width
            );
            assert!(measured_lines(
                &m.folder_label,
                label_size(m.scale),
                content.label.width,
                m.font,
                1
            )
            .last()
            .unwrap()
            .ends_with('…'));
        }
    }

    #[test]
    fn ret03_minimum_slices_share_overlap_paint_and_hit_slant() {
        let mut m = model(RailMode::Slices, 616.0);
        m.scale = 0.5;
        let items = layout(&m, Rectangle::with_size(Size::new(616.0, 720.0)));
        for pair in items
            .windows(2)
            .filter(|p| p[0].rect.y == p[1].rect.y && p[1].group_gap == 0.0)
        {
            // Beta.26 keeps a 5px logical overlap and caps the painted
            // slant at 35% of the width (4.9px on a minimum-scale swatch).
            assert_eq!(pair[0].skew, 5.0_f32.min(pair[0].rect.width * 0.35));
            assert!((pair[1].rect.x - (pair[0].rect.x + pair[0].rect.width - 5.0)).abs() < 0.01);
            // The lower leading edge and upper trailing edge meet with no transparent crack.
            let seam = Point::new(pair[1].rect.x + pair[1].skew * 0.5, pair[1].rect.center_y());
            assert!(item_contains(&pair[0], seam, m.mode) || item_contains(&pair[1], seam, m.mode));
        }
    }

    #[test]
    fn ret03_measured_label_demand_matches_paint_insets() {
        for mode in [RailMode::Slices, RailMode::Geometric, RailMode::Wall] {
            for scale in [1.0, 0.5] {
                let mut m = model(mode, 616.0);
                m.scale = scale;
                let size = footprint(&m);
                for item in layout(&m, Rectangle::with_size(Size::new(size[0], size[1]))) {
                    if !item.label.is_empty()
                        && item.action != Some(RailIntent::ToggleFolder)
                        && !matches!(item.action, Some(RailIntent::HuePreset(_)))
                    {
                        let padding = if item.icon.is_some() { 39.0 } else { 20.0 } * scale;
                        assert!(
                            rail_text_width(&item.label, label_size(scale), m.font)
                                <= item.rect.width - padding + 0.01,
                            "{mode:?} {scale} {}",
                            item.label
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn pk04_literal_minimum_and_popup_use_final_rectangles() {
        for mode in [RailMode::Slices, RailMode::Geometric, RailMode::Wall] {
            for vertical in [false, true] {
                for rtl in [false, true] {
                    let mut m = model(mode, 616.0);
                    m.scale = 0.5;
                    m.max_height = 480.0;
                    m.vertical = vertical;
                    m.rtl = rtl;
                    m.folder_label = "音楽e\u{301} long folder ".repeat(30);
                    let size = footprint(&m);
                    let items = layout(&m, Rectangle::with_size(Size::new(size[0], size[1])));
                    assert!(items.iter().all(|i| i.rect.height == 12.0));
                    assert_eq!((label_size(m.scale), icon_size(m.scale)), (5.0, 7.0));
                    assert_eq!(items.last().unwrap().action, Some(RailIntent::Close));
                    for rail_at in [[0.0, 0.0], [24.0, 32.0], [640.0 - size[0], 512.0 - size[1]]] {
                        let at = folder_popup_anchor(&m, rail_at, [640.0, 512.0], [220.0, 300.0]);
                        assert!(
                            at[0] >= 0.0
                                && at[1] >= 0.0
                                && at[0] + 220.0 <= 640.0
                                && at[1] + 300.0 <= 512.0
                        );
                    }
                }
            }
        }
        let m = model(RailMode::Wall, 1600.0);
        let local = folder_anchor(&m);
        let at = folder_popup_anchor(&m, [0.0, 900.0], [1920.0, 1080.0], [300.0, 300.0]);
        assert!(at[1] + 300.0 < 900.0 + local[1]);
    }

    #[test]
    fn w1_hidden_items_have_no_footprint_or_hit_in_both_orientations() {
        for mode in [RailMode::Slices, RailMode::Geometric, RailMode::Wall] {
            for vertical in [false, true] {
                let mut m = model(mode, 1600.0);
                m.vertical = vertical;
                let before = footprint(&m);
                m.visibility.sources = [false; 4];
                m.visibility.folder = false;
                m.visibility.favorites = false;
                m.visibility.resolution = false;
                let size = footprint(&m);
                let items = layout(
                    &m,
                    Rectangle::new(Point::ORIGIN, Size::new(size[0], size[1])),
                );
                assert!(items.iter().all(|item| !matches!(
                    item.action,
                    Some(
                        RailIntent::Source(_)
                            | RailIntent::ToggleFolder
                            | RailIntent::ToggleFavorites
                            | RailIntent::CycleResolution
                    )
                )));
                assert!(size[0] <= before[0] && size[1] <= before[1]);
                for (index, item) in items.iter().enumerate() {
                    if item.action.is_some() {
                        assert_eq!(
                            hovered_item(&items, Some(item.rect.center()), mode),
                            Some(index)
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn r3_long_labels_retained_actions_hidden_items_and_shape_hits_share_geometry() {
        for (mode, vertical, scale) in [
            (RailMode::Slices, false, 1.0),
            (RailMode::Geometric, true, 1.25),
            (RailMode::Wall, false, 1.0),
        ] {
            let mut m = model(mode, 960.0);
            m.vertical = vertical;
            m.scale = scale;
            m.max_height = 640.0;
            m.folder_label = "Long folder name with Unicode 音楽 and repeated detail ".repeat(8);
            let size = footprint(&m);
            let items = layout(
                &m,
                Rectangle::new(Point::ORIGIN, Size::new(size[0], size[1])),
            );
            assert!(size[0] <= m.max_width && size[1] <= m.max_height);
            for (index, item) in items.iter().enumerate() {
                assert!(item.rect.x + item.rect.width <= size[0] + 0.01);
                assert!(item.rect.y + item.rect.height <= size[1] + 0.01);
                assert_eq!(
                    hovered_item(&items, Some(item.rect.center()), mode),
                    Some(index)
                );
                if mode == RailMode::Slices {
                    assert!(!item_contains(
                        item,
                        Point::new(item.rect.x + 0.01, item.rect.y + 0.01),
                        mode
                    ));
                }
            }
            for action in [
                RailIntent::HuePreset(8),
                RailIntent::OpenSearch,
                RailIntent::Close,
            ] {
                assert!(items.iter().any(|item| item.action == Some(action)));
            }
            m.visibility.colors = false;
            m.visibility.tagcloud = false;
            m.visibility.search = false;
            let hidden = layout(
                &m,
                Rectangle::new(Point::ORIGIN, Size::new(m.max_width, m.max_height)),
            );
            assert!(hidden.iter().all(|item| !matches!(
                item.action,
                Some(RailIntent::HuePreset(_) | RailIntent::OpenSearch)
            )));
            // Removing items can repack rows into a wider, shorter footprint.
            // Compare occupied geometry rather than requiring both axes to shrink.
            assert!(
                hidden
                    .iter()
                    .map(|i| i.rect.width * i.rect.height)
                    .sum::<f32>()
                    < items
                        .iter()
                        .map(|i| i.rect.width * i.rect.height)
                        .sum::<f32>()
            );
        }
    }

    #[test]
    fn r3_vertical_supported_scale_fits_final_actions_after_long_folder() {
        let mut m = model(RailMode::Geometric, 960.0);
        m.vertical = true;
        m.max_height = 640.0;
        m.scale = 2.0;
        m.folder_label = "abcdefghijklmnopqrst".into();
        let size = footprint(&m);
        let items = layout(
            &m,
            Rectangle::new(Point::ORIGIN, Size::new(size[0], size[1])),
        );
        assert!(items
            .iter()
            .all(|i| i.rect.x + i.rect.width <= size[0] + 0.01
                && i.rect.y + i.rect.height <= size[1] + 0.01));
        let last = items.last().unwrap();
        assert_eq!(last.action, Some(RailIntent::Close));
        assert_eq!(
            hovered_item(&items, Some(last.rect.center()), m.mode),
            Some(items.len() - 1)
        );
    }

    #[test]
    fn scaled_rail_footprints_and_hits_use_the_same_physical_rectangles() {
        for (mode, vertical, scale) in [
            (RailMode::Wall, false, 2.0),
            (RailMode::Wall, true, 2.0),
            (RailMode::Geometric, true, 0.75),
            (RailMode::Slices, false, 0.75),
        ] {
            let mut m = model(mode, 800.0);
            m.vertical = vertical;
            m.scale = scale;
            m.max_height = 420.0;
            let size = footprint(&m);
            let items = layout(
                &m,
                Rectangle::new(Point::ORIGIN, Size::new(size[0], size[1])),
            );
            assert!(!items.is_empty());
            for (index, item) in items.iter().enumerate() {
                assert_eq!(item.rect.height, rail_height(scale));
                assert!(item.rect.x + item.rect.width <= size[0] + 0.001);
                assert!(item.rect.y + item.rect.height <= size[1] + 0.001);
                if item.action.is_some() {
                    assert_eq!(
                        hovered_item(&items, Some(item.rect.center()), mode),
                        Some(index)
                    );
                }
            }
            if vertical {
                assert!(size[1] <= 420.0);
            }
        }
        let mut clipped = model(RailMode::Wall, 240.0);
        clipped.vertical = true;
        clipped.scale = 2.0;
        clipped.max_height = 100.0;
        let size = footprint(&clipped);
        assert!(
            size[0] <= 240.0 && (size[0] - 240.0).abs() < 0.001 && size[1] == 100.0,
            "source clips the visible multi-column footprint to the viewport"
        );
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(size[0], size[1]));
        let program = RailProgram {
            input_active: true,
            model: clipped,
        };
        assert_eq!(
            <RailProgram as canvas::Program<RailIntent>>::mouse_interaction(
                &program,
                &(),
                bounds,
                mouse::Cursor::Available(Point::new(245.0, 20.0))
            ),
            mouse::Interaction::None
        );
    }

    #[test]
    fn rtl_rail_mirrors_natural_columns_and_matches_slice_shape_hits() {
        for (mode, vertical, scale) in
            [(RailMode::Wall, true, 2.0), (RailMode::Slices, false, 0.75)]
        {
            let mut m = model(mode, 1200.0);
            m.vertical = vertical;
            m.scale = scale;
            m.max_height = 180.0;
            let bounds = Rectangle::new(Point::ORIGIN, Size::new(1200.0, 1080.0));
            let ltr = layout(&m, bounds);
            let extent = ltr
                .iter()
                .map(|i| i.rect.x + i.rect.width)
                .fold(0.0, f32::max);
            m.rtl = true;
            let rtl = layout(&m, bounds);
            for (a, b) in ltr.iter().zip(&rtl) {
                assert!((b.rect.x - (extent - a.rect.x - a.rect.width)).abs() < 0.001);
                assert_eq!(a.action, b.action);
                assert_eq!(a.rect.y, b.rect.y);
                let center = a.rect.center();
                assert_eq!(
                    item_contains(a, center, mode),
                    item_contains(b, Point::new(extent - center.x, center.y), mode)
                );
            }
            if mode == RailMode::Slices {
                let a = &ltr[0];
                let b = &rtl[0];
                let point = Point::new(a.rect.x + 0.1, a.rect.y + 0.1);
                assert!(!item_contains(a, point, mode));
                assert!(!item_contains(
                    b,
                    Point::new(extent - point.x, point.y),
                    mode
                ));
            } else {
                assert!(ltr.iter().any(|i| i.rect.x > 0.0));
            }
        }
    }

    #[test]
    fn included_groups_wrap_without_losing_last_actions() {
        for mode in [RailMode::Slices, RailMode::Geometric, RailMode::Wall] {
            for width in [320.0, 800.0, 1896.0] {
                let m = model(mode, width);
                let items = layout(&m, Rectangle::new(Point::ORIGIN, Size::new(width, 1000.0)));
                assert_eq!(items.last().unwrap().action, Some(RailIntent::Close));
                for i in &items {
                    assert_eq!(i.rect.height, 24.0);
                    assert!(i.rect.x >= 0.0 && i.rect.x + i.rect.width <= width + 0.001);
                }
                let kinds: Vec<_> = items.iter().filter_map(|i| i.action).collect();
                let folder = kinds
                    .iter()
                    .position(|k| *k == RailIntent::ToggleFolder)
                    .unwrap();
                let sort = kinds
                    .iter()
                    .position(|k| *k == RailIntent::SetSort(SortOrder::Recent))
                    .unwrap();
                assert!(folder < sort);
                assert!(footprint(&m)[1] >= 24.0);
                let size = footprint(&m);
                let drawn = layout(
                    &m,
                    Rectangle::new(Point::ORIGIN, Size::new(size[0], size[1])),
                );
                for (actual, expected) in drawn.iter().zip(&items) {
                    assert_eq!(actual.rect, expected.rect);
                }
            }
        }
    }
    #[test]
    fn e1_slices_polygon_and_non_slices_visual_bounds_are_hit_regions() {
        for mode in [RailMode::Slices, RailMode::Geometric, RailMode::Wall] {
            let m = model(mode, 1896.0);
            let items = layout(&m, Rectangle::new(Point::ORIGIN, Size::new(1896.0, 24.0)));
            let item = &items[0];
            assert_eq!(
                item_contains(item, Point::new(item.rect.x + 0.1, item.rect.y + 0.1), mode),
                mode != RailMode::Slices
            );
            assert!(item_contains(item, item.rect.center(), mode));
        }
    }
    #[test]
    fn pinned_anchor_and_folder_menu_share_control_coordinates() {
        let m = model(RailMode::Slices, 1896.0);
        let size = footprint(&m);
        let p = anchor(RailMode::Slices, [1920.0, 1080.0], size, 520.0);
        assert_eq!(p[1], 238.0);
        assert!((p[0] - (1920.0 - size[0]) * 0.5).abs() < 0.001);
        let folder = folder_anchor(&m);
        assert_eq!(folder[1], 30.0);
    }
    #[test]
    fn donor_fast_reveal_reverses_continuously_and_clamps_frame_time() {
        let half = reveal_step(0.0, 1.0, 0.05);
        assert!((half - 0.2777778).abs() < 0.00001);
        assert_eq!(reveal_step(0.0, 1.0, 1.0), half);
        let closing = reveal_step(half, 0.0, 0.016);
        assert!(closing < half && closing >= 0.0);
        let reopened = reveal_step(closing, 1.0, 0.016);
        assert!((reopened - half).abs() < 0.00001);
        assert_eq!(reveal_step(0.01, 0.0, 0.05), 0.0);
        assert_eq!(reveal_step(0.99, 1.0, 0.05), 1.0);
    }

    #[test]
    fn rail_mouse_interaction_is_none_outside_pass_through_and_hidden_regions() {
        let mut base_model = model(RailMode::Slices, 1896.0);
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1896.0, 24.0));
        let program = RailProgram {
            input_active: true,
            model: base_model.clone(),
        };

        assert_eq!(
            <RailProgram as canvas::Program<RailIntent>>::mouse_interaction(
                &program,
                &(),
                bounds,
                mouse::Cursor::Available(Point::new(-1.0, 12.0)),
            ),
            mouse::Interaction::None
        );
        assert_eq!(
            <RailProgram as canvas::Program<RailIntent>>::mouse_interaction(
                &program,
                &(),
                bounds,
                mouse::Cursor::Available(Point::new(0.1, 0.1)),
            ),
            mouse::Interaction::None
        );

        base_model.title = Some("Non-interactive title".into());
        let title_program = RailProgram {
            input_active: true,
            model: base_model,
        };
        assert_eq!(
            <RailProgram as canvas::Program<RailIntent>>::mouse_interaction(
                &title_program,
                &(),
                bounds,
                mouse::Cursor::Available(Point::new(50.0, 12.0)),
            ),
            mouse::Interaction::None
        );

        let mut hidden_model = model(RailMode::Wall, 1896.0);
        hidden_model.reveal = 0.5;
        let hidden_program = RailProgram {
            input_active: true,
            model: hidden_model,
        };
        assert_eq!(
            <RailProgram as canvas::Program<RailIntent>>::mouse_interaction(
                &hidden_program,
                &(),
                bounds,
                mouse::Cursor::Available(Point::new(50.0, 12.0)),
            ),
            mouse::Interaction::None
        );
    }

    #[test]
    fn valid_rail_hit_reports_pointer_and_publishes_captured_intent() {
        let model = model(RailMode::Wall, 1896.0);
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(1896.0, 24.0));
        let program = RailProgram {
            input_active: true,
            model,
        };
        let items = layout(&program.model, bounds);
        let item = items
            .iter()
            .find(|item| item.action == Some(RailIntent::Source(0)))
            .expect("source control must be laid out");
        let point = item.rect.center();
        assert_eq!(
            <RailProgram as canvas::Program<RailIntent>>::mouse_interaction(
                &program,
                &(),
                bounds,
                mouse::Cursor::Available(point),
            ),
            mouse::Interaction::Pointer
        );

        let event = canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let action = <RailProgram as canvas::Program<RailIntent>>::update(
            &program,
            &mut (),
            &event,
            bounds,
            mouse::Cursor::Available(point),
        )
        .expect("valid rail control must publish an action");
        let (message, _, status) = action.into_inner();
        assert_eq!(message, Some(RailIntent::Source(0)));
        assert_eq!(status, iced::event::Status::Captured);
    }

    #[test]
    fn rail_filter_cluster_precedes_sort_and_separate_independent_colours() {
        let cluster = [
            RailIntent::Source(0),
            RailIntent::Source(1),
            RailIntent::Source(2),
            RailIntent::Source(3),
            RailIntent::CycleShape,
            RailIntent::CycleResolution,
            RailIntent::ToggleFolder,
        ];
        for mode in [RailMode::Slices, RailMode::Geometric, RailMode::Wall] {
            for vertical in [false, true] {
                let mut m = model(mode, 1896.0);
                m.vertical = vertical;
                m.favorites_active = true;
                m.bucket = Some(ColorBucket::ALL[4]);
                let size = footprint(&m);
                let items = layout(&m, Rectangle::with_size(Size::new(size[0], size[1])));
                assert_eq!(
                    items[..cluster.len()]
                        .iter()
                        .map(|i| i.action.unwrap())
                        .collect::<Vec<_>>(),
                    cluster
                );
                for pair in items[..cluster.len()].windows(2) {
                    assert_eq!(pair[1].group_gap, 0.0);
                    if vertical {
                        assert_eq!(pair[1].rect.y - pair[0].rect.y, 27.0);
                    } else {
                        let overlap = if mode == RailMode::Slices { 10.0 } else { 0.0 };
                        assert!(
                            (pair[1].rect.x - (pair[0].rect.x + pair[0].rect.width - overlap))
                                .abs()
                                < 0.01
                        );
                    }
                }
                assert_eq!(
                    items[7].action,
                    Some(RailIntent::SetSort(SortOrder::Newest))
                );
                assert_eq!(items[7].group_gap, 0.0);
                assert_eq!(items[14].action, Some(RailIntent::ToggleFavorites));
                assert_eq!(items[15].action, Some(RailIntent::ToggleRandomRotation));
                assert_eq!(items[16].action, Some(RailIntent::HuePreset(0)));
                assert_eq!(items[16].group_gap, 6.0);
                assert!(items[14].active);
                for (index, item) in items.iter().enumerate() {
                    assert_ne!(item.label, "Theme editor");
                    assert_eq!(
                        hovered_item(&items, Some(item.rect.center()), mode),
                        Some(index)
                    );
                    if let Some(RailIntent::HuePreset(i)) = item.action {
                        assert_eq!(item.active, i == 4);
                    }
                }
                assert_eq!(items.last().unwrap().action, Some(RailIntent::Close));
            }
        }
    }

    #[test]
    fn rail_folder_content_clears_shape_and_preserves_hits_at_supported_scales() {
        for (mode, width, height, vertical, scale, rtl) in [
            (RailMode::Slices, 1896.0, 1048.0, false, 1.0, false),
            (RailMode::Slices, 936.0, 608.0, false, 0.5, false),
            (RailMode::Geometric, 320.0, 400.0, false, 1.0, false),
            (RailMode::Wall, 288.0, 208.0, true, 2.0, false),
            (RailMode::Slices, 288.0, 208.0, true, 2.0, true),
            (RailMode::Geometric, 936.0, 608.0, true, 1.25, true),
        ] {
            let mut m = model(mode, width);
            m.max_height = height;
            m.vertical = vertical;
            m.scale = scale;
            m.rtl = rtl;
            m.folder_label = "Very long folder 音楽 with a technical suffix ".repeat(8);
            let size = footprint(&m);
            let items = layout(&m, Rectangle::with_size(Size::new(size[0], size[1])));
            let index = items
                .iter()
                .position(|i| i.action == Some(RailIntent::ToggleFolder))
                .unwrap();
            let item = &items[index];
            let content = folder_content(item, scale);
            if let Some(icon) = content.icon {
                let half_width = typography::glyph_width(label_size(scale)) * 0.5;
                let half_height = label_size(scale) * 0.5;
                for dx in [-half_width, half_width] {
                    for dy in [-half_height, half_height] {
                        assert!(item_contains(
                            item,
                            Point::new(icon.x + dx, icon.y + dy),
                            mode
                        ));
                    }
                }
                assert_eq!(hovered_item(&items, Some(icon), mode), Some(index));
            }
            if let Some(arrow) = content.arrow {
                assert!(item_contains(item, arrow, mode));
                assert!(content.label.width >= 3.0 * scale);
                assert!(
                    content.label.x >= item.rect.x
                        && content.label.x + content.label.width <= item.rect.x + item.rect.width
                );
            } else {
                assert_eq!(content.label.width, 0.0);
            }
            assert_eq!(
                hovered_item(&items, Some(item.rect.center()), mode),
                Some(index)
            );
            assert_eq!(
                folder_anchor(&m),
                [item.rect.x, item.rect.y + item.rect.height + 6.0]
            );
            for i in &items {
                assert!(i.rect.x + i.rect.width <= width + 0.01);
                assert!(i.rect.y + i.rect.height <= size[1] + 0.01);
            }
        }
    }

    #[test]
    fn rail_compact_folder_uses_the_same_safe_insets_in_ltr_and_rtl() {
        for mode in [RailMode::Slices, RailMode::Geometric, RailMode::Wall] {
            for rtl in [false, true] {
                for scale in [0.5, 1.0, 2.0] {
                    for width in [28.0, 32.0, 39.9, 40.0, 48.0, 63.9, 64.0] {
                        let item = RailItem {
                            action: Some(RailIntent::ToggleFolder),
                            label: "Long folder label".into(),
                            icon: Some("\u{f024b}".into()),
                            rect: Rectangle::new(
                                Point::ORIGIN,
                                Size::new(width * scale, 24.0 * scale),
                            ),
                            active: false,
                            rtl,
                            group_gap: 0.0,
                            skew: (10.0 * scale).min(width * scale * 0.35),
                        };
                        let content = folder_content(&item, scale);
                        let (lead, trail, glyph) = folder_insets(item.skew, scale);
                        let expanded = item.rect.width >= lead + trail + 2.0 * glyph + 10.0;
                        assert_eq!(
                            content.icon.is_some(),
                            expanded || item.rect.width >= glyph + 2.0 * item.skew
                        );
                        assert_eq!(content.arrow.is_some(), expanded);
                        assert_eq!(item.action, Some(RailIntent::ToggleFolder));
                        assert!(item_contains(&item, item.rect.center(), mode));
                        if let Some(icon) = content.icon {
                            let half = typography::glyph_width(label_size(scale)) * 0.5;
                            for dx in [-half, half] {
                                for dy in [-label_size(scale) * 0.5, label_size(scale) * 0.5] {
                                    assert!(
                                        item_contains(
                                            &item,
                                            Point::new(icon.x + dx, icon.y + dy),
                                            mode
                                        ),
                                        "{mode:?} rtl{rtl} scale{scale} width{width}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn pk04_final_order_glyphs_boundaries_visibility_and_shared_hits() {
        for mode in [RailMode::Slices, RailMode::Geometric, RailMode::Wall] {
            for scale in [1.0, 0.5] {
                for vertical in [false, true] {
                    for rtl in [false, true] {
                        let mut m = model(mode, 616.0);
                        m.scale = scale;
                        m.vertical = vertical;
                        m.rtl = rtl;
                        m.max_height = 208.0;
                        let size = footprint(&m);
                        let items = layout(&m, Rectangle::with_size(Size::new(size[0], size[1])));
                        let actions = items
                            .iter()
                            .filter_map(|item| item.action)
                            .collect::<Vec<_>>();
                        let mut expected = (0..4).map(RailIntent::Source).collect::<Vec<_>>();
                        expected.extend([
                            RailIntent::CycleShape,
                            RailIntent::CycleResolution,
                            RailIntent::ToggleFolder,
                        ]);
                        expected.extend(SortOrder::ALL.map(RailIntent::SetSort));
                        expected.extend([
                            RailIntent::ToggleFavorites,
                            RailIntent::ToggleRandomRotation,
                        ]);
                        expected.extend((0..13).map(RailIntent::HuePreset));
                        expected.extend([
                            RailIntent::OpenSearch,
                            RailIntent::ToggleEngineMute,
                            RailIntent::TogglePlaylists,
                            RailIntent::ToggleSettings,
                            RailIntent::Close,
                        ]);
                        assert_eq!(actions, expected);
                        assert_eq!(items.last().unwrap().icon.as_deref(), Some("\u{eae7}"));
                        assert_eq!(items[15].icon.as_deref(), Some("\u{f076e}"));
                        assert!(items[15].label.is_empty());
                        assert!(!items[15].active);
                        for (index, item) in items.iter().enumerate() {
                            assert_eq!(item.rect.height, 24.0 * scale);
                            assert!(
                                item.rect.x >= 0.0
                                    && item.rect.y >= 0.0
                                    && item.rect.x + item.rect.width <= size[0] + 0.01
                                    && item.rect.y + item.rect.height <= size[1] + 0.01
                            );
                            assert_eq!(
                                hovered_item(&items, Some(item.rect.center()), mode),
                                Some(index)
                            );
                            assert_eq!(
                                item.group_gap,
                                if index == 16 || index == 29 {
                                    6.0 * scale
                                } else {
                                    0.0
                                }
                            );
                        }
                        for (index, (_, _, glyph, _)) in SORT_SHORTCUTS.iter().enumerate() {
                            assert_eq!(items[7 + index].icon.as_deref(), *glyph);
                        }
                        m.visibility.random = false;
                        m.visibility.colors = false;
                        let hidden = layout(&m, Rectangle::with_size(Size::new(size[0], size[1])));
                        assert!(hidden.iter().all(|i| !matches!(
                            i.action,
                            Some(RailIntent::ToggleRandomRotation | RailIntent::HuePreset(_))
                        )));
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RailIntent {
    Source(usize),
    CycleShape,
    CycleResolution,
    SetSort(SortOrder),
    ToggleFolder,
    ToggleFavorites,
    ToggleRandomRotation,
    ToggleEngineMute,
    TogglePlaylists,
    OpenSearch,
    ToggleSettings,
    HuePreset(usize),
    Close,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RailPalette {
    pub(crate) background: Color,
    pub(crate) surface: Color,
    pub(crate) container: Color,
    pub(crate) hover: Color,
    pub(crate) active_text: Color,
    pub(crate) text: Color,
    pub(crate) accent: Color,
    pub(crate) outline: Color,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RailVisibility {
    pub sources: [bool; 4],
    pub orientation: bool,
    pub resolution: bool,
    pub folder: bool,
    pub sort: bool,
    pub favorites: bool,
    pub random: bool,
    pub playlists: bool,
    pub colors: bool,
    pub tagcloud: bool,
    pub mute: bool,
    pub search: bool,
    pub settings: bool,
}
impl Default for RailVisibility {
    fn default() -> Self {
        Self {
            sources: [true; 4],
            orientation: true,
            resolution: true,
            folder: true,
            sort: true,
            favorites: true,
            random: true,
            playlists: true,
            colors: true,
            tagcloud: true,
            mute: true,
            search: true,
            settings: true,
        }
    }
}
impl RailVisibility {
    fn includes(self, action: Option<RailIntent>) -> bool {
        match action {
            Some(RailIntent::Source(index)) => self.sources.get(index).copied().unwrap_or(false),
            Some(RailIntent::CycleShape) => self.orientation,
            Some(RailIntent::CycleResolution) => self.resolution,
            Some(RailIntent::ToggleFolder) => self.folder,
            Some(RailIntent::SetSort(_)) => self.sort,
            Some(RailIntent::ToggleFavorites) => self.favorites,
            Some(RailIntent::ToggleRandomRotation) => self.random,
            Some(RailIntent::TogglePlaylists) => self.playlists,
            Some(RailIntent::HuePreset(_)) => self.colors,
            Some(RailIntent::ToggleEngineMute) => self.mute,
            Some(RailIntent::OpenSearch) => self.search || self.tagcloud,
            Some(RailIntent::ToggleSettings) => self.settings,
            _ => true,
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct RailModel {
    pub(crate) visibility: RailVisibility,
    pub(crate) reveal: f32,
    pub(crate) max_width: f32,
    pub(crate) viewport: [f32; 2],
    pub(crate) vertical: bool,
    pub(crate) max_height: f32,
    pub(crate) scale: f32,
    pub(crate) rtl: bool,
    pub(crate) resolution_label: String,
    pub(crate) sort_order: SortOrder,
    pub(crate) sort_visibility: [bool; 7],
    pub(crate) folder_active: bool,
    pub(crate) folder_open: bool,
    pub(crate) mode: RailMode,
    pub(crate) title: Option<String>,
    pub(crate) source_labels: [String; 4],
    pub(crate) source_selected: usize,
    pub(crate) shape_label: String,
    pub(crate) shape_active: bool,
    pub(crate) folder_label: String,
    pub(crate) favorites_active: bool,
    pub(crate) random_state: crate::candidate_state::RandomRotationState,
    pub(crate) muted: bool,
    pub(crate) playlists_active: bool,
    pub(crate) search_active: bool,
    pub(crate) settings_active: bool,
    pub(crate) bucket: Option<ColorBucket>,
    pub(crate) show_close: bool,
    pub(crate) palette: RailPalette,
    pub(crate) font: Font,
    pub(crate) icon_font: Font,
}

#[derive(Debug, Clone)]
struct RailItem {
    action: Option<RailIntent>,
    label: String,
    icon: Option<String>,
    rect: Rectangle,
    active: bool,
    rtl: bool,
    group_gap: f32,
    skew: f32,
}

struct FolderContent {
    icon: Option<Point>,
    label: Rectangle,
    arrow: Option<Point>,
}

fn folder_insets(skew: f32, scale: f32) -> (f32, f32, f32) {
    let glyph = typography::glyph_width(label_size(scale));
    let lead = if skew > 0.0 {
        skew.max(6.0) + 4.0
    } else {
        10.0
    };
    let trail = skew.max(6.0) + 6.0;
    (lead, trail, glyph)
}
fn folder_content(item: &RailItem, scale: f32) -> FolderContent {
    let rect = item.rect;
    let (lead, trail, glyph) = folder_insets(item.skew, scale);
    let inset = lead + trail + 2.0 * glyph + 10.0;
    if rect.width < inset {
        return FolderContent {
            icon: (rect.width >= glyph + 2.0 * item.skew).then_some(rect.center()),
            label: Rectangle::new(rect.position(), Size::ZERO),
            arrow: None,
        };
    }
    let logical = |offset| {
        if item.rtl {
            rect.x + rect.width - offset
        } else {
            rect.x + offset
        }
    };
    let start = lead + glyph + 5.0;
    let end = trail + glyph + 5.0;
    FolderContent {
        icon: Some(Point::new(logical(lead + glyph * 0.5), rect.center_y())),
        label: Rectangle::new(
            Point::new(rect.x + if item.rtl { end } else { start }, rect.y),
            Size::new((rect.width - start - end).max(0.0), rect.height),
        ),
        arrow: Some(Point::new(
            logical(rect.width - trail - glyph * 0.5),
            rect.center_y(),
        )),
    }
}

#[derive(Debug, Clone)]
struct RailProgram {
    model: RailModel,
    input_active: bool,
}

pub(crate) fn view_with_opacity<'a>(
    mut model: RailModel,
    opacity: f32,
    input_active: bool,
) -> Element<'a, RailIntent> {
    let [width, height] = footprint(&model);
    let palette = &mut model.palette;
    for color in [
        &mut palette.background,
        &mut palette.surface,
        &mut palette.container,
        &mut palette.hover,
        &mut palette.active_text,
        &mut palette.text,
        &mut palette.accent,
        &mut palette.outline,
    ] {
        color.a *= opacity;
    }
    canvas(RailProgram {
        model,
        input_active: input_active && opacity > 0.02,
    })
    .width(Length::Fixed(width))
    .height(Length::Fixed(height))
    .into()
}

/// Pinned animation.rs Tween::tick, MotionTier::Fast default 180ms.
pub(crate) fn reveal_step(value: f32, target: f32, dt: f32) -> f32 {
    let step = 1000.0 / 180.0 * dt.min(0.05);
    if value < target {
        (value + step).min(target)
    } else {
        (value - step).max(target)
    }
}

pub(crate) fn geometric_height(
    radius: f32,
    rows: usize,
    gap: f32,
    stagger: f32,
    shape: crate::donor_layout::HexShape,
) -> f32 {
    crate::donor_layout::HexParams {
        r: radius.max(crate::donor_layout::MIN_HEX_R),
        rows: rows.max(1),
        gap_y: gap,
        stagger,
        shape,
        ..Default::default()
    }
    .content_h()
}

pub(crate) fn footprint(model: &RailModel) -> [f32; 2] {
    let items = layout(
        model,
        Rectangle::new(Point::ORIGIN, Size::new(model.max_width, 1.0)),
    );
    let mut size = [
        items
            .iter()
            .map(|i| i.rect.x + i.rect.width)
            .fold(1.0, f32::max),
        items
            .iter()
            .map(|i| i.rect.y + i.rect.height)
            .fold(rail_height(model.scale), f32::max),
    ];
    if model.vertical {
        size[0] = size[0].min(model.max_width);
        // Source verticalize_bar retains a trailing gap, or the full column height.
        size[1] = if items.iter().any(|item| item.rect.x > 0.0) {
            model.max_height.max(rail_height(model.scale))
        } else {
            (size[1] + 3.0 * model.scale).max(rail_height(model.scale))
        };
    }
    size
}
pub(crate) fn folder_popup_anchor(
    model: &RailModel,
    rail_at: [f32; 2],
    viewport: [f32; 2],
    popup: [f32; 2],
) -> [f32; 2] {
    let size = footprint(model);
    let items = layout(model, Rectangle::with_size(Size::new(size[0], size[1])));
    let Some(item) = items
        .iter()
        .find(|i| i.action == Some(RailIntent::ToggleFolder))
    else {
        return [0.0, 0.0];
    };
    let gap = 6.0 * model.scale;
    let rect = Rectangle::new(
        Point::new(rail_at[0] + item.rect.x, rail_at[1] + item.rect.y),
        item.rect.size(),
    );
    let (x, y) = if model.vertical {
        // A spill column can sit between the Folder item and the outer rail edge.
        let right = rail_at[0] + size[0] + gap;
        let left = rail_at[0] - gap - popup[0];
        let prefer_left = rail_at[0] + size[0] * 0.5 < viewport[0] * 0.5;
        let x = match (left >= 0.0, right + popup[0] <= viewport[0]) {
            (true, true) if prefer_left => left,
            (true, false) => left,
            (_, true) => right,
            (false, false) if viewport[0] - right > rail_at[0] - gap => right,
            (false, false) => left,
        };
        (x, rect.y)
    } else {
        // A wrapped rail still owns every row. Keep the open panel clear
        // of the whole footprint when either side has space.
        let below = rail_at[1] + size[1] + gap;
        let above = rail_at[1] - gap - popup[1];
        let x = if model.rtl {
            rect.x + rect.width - popup[0]
        } else {
            rect.x
        };
        let y = if below + popup[1] <= viewport[1] {
            below
        } else if above >= 0.0 {
            above
        } else if viewport[1] - below >= rail_at[1] - gap {
            below
        } else {
            above
        };
        (x, y)
    };
    [
        x.clamp(0.0, (viewport[0] - popup[0]).max(0.0)),
        y.clamp(0.0, (viewport[1] - popup[1]).max(0.0)),
    ]
}

#[cfg(test)]
pub(crate) fn folder_anchor(model: &RailModel) -> [f32; 2] {
    layout(
        model,
        Rectangle::new(Point::ORIGIN, Size::new(model.max_width, 1.0)),
    )
    .iter()
    .find(|i| i.action == Some(RailIntent::ToggleFolder))
    .map_or([0.0, 24.0], |i| [i.rect.x, i.rect.y + i.rect.height + 6.0])
}

#[cfg(test)]
pub(crate) fn swatch_center(model: &RailModel, index: usize) -> Option<Point> {
    intent_center(model, RailIntent::HuePreset(index))
}

#[cfg(test)]
pub(crate) fn intent_center(model: &RailModel, intent: RailIntent) -> Option<Point> {
    let size = footprint(model);
    layout(
        model,
        Rectangle::new(Point::ORIGIN, Size::new(size[0], size[1])),
    )
    .iter()
    .find(|item| item.action == Some(intent))
    .map(|item| item.rect.center())
}

/// Horizontal branch of pinned app/view/chrome.rs. 's gallery owns its
/// viewport; this only anchors the floating bar beside the configured footprint.
pub(crate) fn anchor(
    mode: RailMode,
    viewport: [f32; 2],
    size: [f32; 2],
    gallery_height: f32,
) -> [f32; 2] {
    let top = match mode {
        RailMode::Wall => (viewport[1] - size[1] - 8.0 - gallery_height) * 0.5,
        RailMode::Slices => (viewport[1] - gallery_height - 110.0) * 0.5 + 13.0,
        RailMode::Geometric => (viewport[1] - gallery_height - 140.0) * 0.5 + 13.0,
    };
    [
        ((viewport[0] - size[0]) * 0.5).max(0.0),
        top.clamp(0.0, (viewport[1] - size[1]).max(0.0)),
    ]
}

thread_local! {
    static WIDTHS: std::cell::RefCell<std::collections::VecDeque<(String,u32,Font,iced::advanced::graphics::text::Version,f32)>> = const {std::cell::RefCell::new(std::collections::VecDeque::new())};
}
fn rail_text_width(value: &str, size: f32, font: Font) -> f32 {
    let version = iced::advanced::graphics::text::font_system()
        .read()
        .unwrap()
        .version();
    if let Some(width) = WIDTHS.with(|cache| {
        cache
            .borrow()
            .iter()
            .find(|(v, s, f, ver, _)| {
                v == value && *s == size.to_bits() && *f == font && *ver == version
            })
            .map(|entry| entry.4)
    }) {
        return width;
    }
    let width = measured_width(value, size, font);
    if value.len() <= 4096 {
        WIDTHS.with(|cache| {
            let mut cache = cache.borrow_mut();
            cache.truncate(127);
            cache.push_front((value.into(), size.to_bits(), font, version, width));
        });
    }
    width
}
// Shared by the rail, Search buttons and keyboard cycle.
pub(crate) const SORT_SHORTCUTS: [(SortOrder, &str, Option<&str>, &str); 7] = [
    (
        SortOrder::Newest,
        "",
        Some("\u{f00f0}"),
        "Newest — source modified time",
    ),
    (
        SortOrder::Recent,
        "",
        Some("\u{f02da}"),
        "Recently applied — local confirmed",
    ),
    (
        SortOrder::Colour,
        "",
        Some("\u{f03d8}"),
        "Colour — hue order",
    ),
    (SortOrder::ColourPop, "", Some("\u{f0238}"), "Colour pop"),
    (
        SortOrder::Richness,
        "",
        Some("\u{f0b74}"),
        "Richness — highest first",
    ),
    (
        SortOrder::Minimalist,
        "",
        Some("\u{f0764}"),
        "Minimalist — lowest richness first",
    ),
    (
        SortOrder::ResolutionDescending,
        "",
        Some("\u{f0a24}"),
        "Resolution — largest pixel area first",
    ),
];

fn rail_height(scale: f32) -> f32 {
    24.0 * scale
}
fn label_size(scale: f32) -> f32 {
    10.0 * scale
}
fn icon_size(scale: f32) -> f32 {
    14.0 * scale
}

fn label_insets(has_icon: bool, scale: f32) -> [f32; 2] {
    [
        if has_icon { 29.0 * scale } else { 10.0 * scale },
        10.0 * scale,
    ]
}

fn fitted_label(item: &RailItem, scale: f32, font: Font, available: f32) -> String {
    measured_lines(&item.label, label_size(scale), available, font, 1)
        .into_iter()
        .next()
        .unwrap_or_default()
}

#[cfg(test)]
pub(crate) fn random_painted_caption(model: &RailModel) -> Option<String> {
    let item = layout(model, Rectangle::with_size(Size::new(model.max_width, 1.0)))
        .into_iter()
        .find(|item| item.action == Some(RailIntent::ToggleRandomRotation))?;
    let [lead, trail] = label_insets(item.icon.is_some(), model.scale);
    Some(fitted_label(
        &item,
        model.scale,
        model.font,
        item.rect.width - lead - trail,
    ))
}

fn layout(model: &RailModel, _bounds: Rectangle) -> Vec<RailItem> {
    let scale = model.scale.max(0.001);
    let height = rail_height(scale);
    let inset = 12.0 * scale;
    let gap = 6.0 * scale;
    let row_gap = 6.0 * scale;
    let max_width = model.max_width.max(1.0);
    let mut items = Vec::new();
    let (mut x, mut y) = (0.0, 0.0);
    let mut last_group = None;
    let mut add = |action: Option<RailIntent>,
                   label: String,
                   icon: Option<String>,
                   active: bool,
                   group: u8| {
        if !model.visibility.includes(action) {
            return;
        }
        let group_gap = if last_group.is_some_and(|last| last != group && (last == 2 || group == 2))
        {
            gap
        } else {
            0.0
        };
        let swatch = matches!(action, Some(RailIntent::HuePreset(_)));
        let skew = 10.0 * scale;
        let icon_width = icon.as_ref().map_or(0.0, |icon| {
            rail_text_width(icon, icon_size(scale), model.icon_font)
        });
        let mut width = if swatch {
            28.0 * scale
        } else if action == Some(RailIntent::ToggleFolder) {
            let label_width = rail_text_width(&label, label_size(scale), model.font);
            let (lead, trail, glyph) = folder_insets(
                if model.mode == RailMode::Slices {
                    skew
                } else {
                    0.0
                },
                scale,
            );
            // Source dropdown demand, with its actual pixel insets reserved before fitting.
            (label_width + 2.0 * typography::glyph_width(label_size(scale)) + 44.0 * scale).max(
                label_width
                    + lead
                    + trail
                    + 2.0 * glyph
                    + 10.0
                    + if model.mode == RailMode::Slices {
                        0.0
                    } else {
                        skew
                    },
            )
        } else {
            let padding = if icon.is_some() && !label.is_empty() {
                let [lead, trail] = label_insets(true, scale);
                // Hex and Wall subtract the logical skew before painting.
                // Reserve the full mixed caption lane even with a narrow glyph.
                (icon_width + 4.0 * scale + inset * 2.0).max(lead + trail + 0.25 * scale)
            } else {
                icon_width + inset * 2.0
            };
            rail_text_width(&label, label_size(scale), model.font) + padding + skew
        };
        if action == Some(RailIntent::ToggleFolder) {
            width = width.min(240.0 * scale);
        }
        width = width.min(max_width);
        let overlap = skew.min(width * 0.5);
        if !model.vertical {
            let next_x = x + if x > 0.0 { group_gap } else { 0.0 };
            if x > 0.0 && next_x + width > max_width {
                x = 0.0;
                y += height + row_gap;
            } else {
                x = next_x;
            }
        }
        items.push(RailItem {
            action,
            label,
            icon,
            rect: Rectangle::new(
                Point::new(x, y),
                Size::new(
                    if model.mode == RailMode::Slices {
                        width
                    } else {
                        (width - overlap).max(1.0)
                    },
                    height,
                ),
            ),
            active,
            rtl: model.rtl,
            group_gap,
            skew: if model.mode == RailMode::Slices {
                overlap.min(width * 0.35)
            } else {
                0.0
            },
        });
        x += width - overlap;
        last_group = Some(group);
    };
    if let Some(title) = &model.title {
        add(None, title.clone(), None, false, 0);
    }
    for (i, label) in model.source_labels.iter().enumerate() {
        add(
            Some(RailIntent::Source(i)),
            label.clone(),
            None,
            model.source_selected == i,
            0,
        );
    }
    add(
        Some(RailIntent::CycleShape),
        model.shape_label.clone(),
        None,
        model.shape_active,
        0,
    );
    add(
        Some(RailIntent::CycleResolution),
        model.resolution_label.clone(),
        None,
        model.resolution_label != "SIZE",
        0,
    );
    add(
        Some(RailIntent::ToggleFolder),
        model.folder_label.clone(),
        Some("\u{f024b}".into()),
        model.folder_open || model.folder_active,
        0,
    );
    for (index, (order, label, icon, _)) in SORT_SHORTCUTS.into_iter().enumerate() {
        if !model.sort_visibility[index] {
            continue;
        }
        add(
            Some(RailIntent::SetSort(order)),
            label.into(),
            icon.map(str::to_owned),
            model.sort_order == order,
            0,
        );
    }
    add(
        Some(RailIntent::ToggleFavorites),
        String::new(),
        Some("\u{f02d1}".into()),
        model.favorites_active,
        0,
    );
    add(
        Some(RailIntent::ToggleRandomRotation),
        String::new(),
        Some("\u{f076e}".into()),
        model.random_state == crate::candidate_state::RandomRotationState::On,
        0,
    );
    for (i, bucket) in ColorBucket::ALL.iter().enumerate() {
        add(
            Some(RailIntent::HuePreset(i)),
            bucket.label().into(),
            None,
            model.bucket == Some(*bucket),
            2,
        );
    }
    add(
        Some(RailIntent::OpenSearch),
        String::new(),
        Some("\u{f0349}".into()),
        model.search_active,
        3,
    );
    add(
        Some(RailIntent::ToggleEngineMute),
        String::new(),
        Some(
            if model.muted {
                "\u{f075f}"
            } else {
                "\u{f057e}"
            }
            .into(),
        ),
        model.muted,
        3,
    );
    add(
        Some(RailIntent::TogglePlaylists),
        String::new(),
        Some("\u{f0cb8}".into()),
        model.playlists_active,
        3,
    );
    add(
        Some(RailIntent::ToggleSettings),
        String::new(),
        Some("\u{f0493}".into()),
        model.settings_active,
        3,
    );
    if model.show_close {
        add(
            Some(RailIntent::Close),
            String::new(),
            Some("\u{eae7}".into()),
            false,
            3,
        );
    }
    if model.vertical {
        let max_height = model.max_height.max(height);
        let mut columns = 1;
        let mut next_y = 0.0;
        for item in &items {
            let item_gap = if next_y > 0.0 { item.group_gap } else { 0.0 };
            if next_y > 0.0 && next_y + item_gap + height > max_height {
                columns += 1;
                next_y = 0.0;
            } else {
                next_y += item_gap;
            }
            next_y += height + 3.0 * scale;
        }
        let column_limit =
            ((model.max_width - (columns - 1) as f32 * 3.0 * scale) / columns as f32).max(1.0);
        let width = items
            .iter()
            .map(|i| i.rect.width)
            .fold(height + 12.0, f32::max)
            .min(column_limit);
        let (mut x, mut y) = (0.0, 0.0);
        for item in &mut items {
            let item_gap = if y > 0.0 { item.group_gap } else { 0.0 };
            if y > 0.0 && y + item_gap + height > max_height {
                x += width + 3.0 * scale;
                y = 0.0;
            } else {
                y += item_gap;
            }
            item.rect = Rectangle::new(Point::new(x, y), Size::new(width, height));
            item.skew = item.skew.min(width * 0.35);
            y += height + 3.0 * scale;
        }
    }
    if model.rtl {
        let extent = items
            .iter()
            .map(|i| i.rect.x + i.rect.width)
            .fold(0.0, f32::max);
        for item in &mut items {
            item.rect.x = extent - item.rect.x - item.rect.width;
        }
    }
    items
}

fn item_path(rect: Rectangle, mode: RailMode, rtl: bool, skew: f32) -> canvas::Path {
    let left = rect.x;
    let right = rect.x + rect.width;
    let top = rect.y;
    let bottom = rect.y + rect.height;
    match mode {
        RailMode::Slices => {
            let skew = skew.min(rect.width * 0.35);
            if rtl {
                return canvas::Path::new(|path| {
                    path.move_to(Point::new(left, top));
                    path.line_to(Point::new(right - skew, top));
                    path.line_to(Point::new(right, bottom));
                    path.line_to(Point::new(left + skew, bottom));
                    path.close();
                });
            }
            canvas::Path::new(|path| {
                path.move_to(Point::new(left + skew, top));
                path.line_to(Point::new(right, top));
                path.line_to(Point::new(right - skew, bottom));
                path.line_to(Point::new(left, bottom));
                path.close();
            })
        }
        RailMode::Geometric => {
            let cut = (rect.height * 0.28)
                .min(rect.width * 0.18)
                .max(2.0)
                .min(rect.width * 0.5);
            canvas::Path::new(|path| {
                path.move_to(Point::new(left + cut, top));
                path.line_to(Point::new(right - cut, top));
                path.line_to(Point::new(right, (top + bottom) * 0.5));
                path.line_to(Point::new(right - cut, bottom));
                path.line_to(Point::new(left + cut, bottom));
                path.line_to(Point::new(left, (top + bottom) * 0.5));
                path.close();
            })
        }
        RailMode::Wall => canvas::Path::rectangle(
            Point::new(left + 0.5, top + 0.5),
            Size::new((rect.width - 1.0).max(1.0), (rect.height - 1.0).max(1.0)),
        ),
    }
}

fn color_with_alpha(color: Color, alpha: f32) -> Color {
    Color {
        a: color.a * alpha,
        ..color
    }
}

fn item_contains(item: &RailItem, point: Point, mode: RailMode) -> bool {
    let r = item.rect;
    if !r.contains(point) {
        return false;
    }
    let x = point.x - r.x;
    let y = point.y - r.y;
    match mode {
        RailMode::Slices => {
            let skew = item.skew;
            let ratio = if item.rtl {
                y / r.height
            } else {
                1.0 - y / r.height
            };
            x >= skew * ratio && x <= r.width - skew * (1.0 - ratio)
        }
        RailMode::Geometric | RailMode::Wall => true,
    }
}
fn hovered_item(items: &[RailItem], point: Option<Point>, mode: RailMode) -> Option<usize> {
    let point = point?;
    let mut order: Vec<_> = (0..items.len()).collect();
    order.sort_by_key(|i| items[*i].active);
    order
        .into_iter()
        .rev()
        .find(|i| item_contains(&items[*i], point, mode))
}

impl<Message> canvas::Program<Message> for RailProgram
where
    Message: From<RailIntent>,
{
    type State = ();

    fn update(
        &self,
        _state: &mut Self::State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Message>> {
        if !self.input_active || self.model.reveal < 0.95 {
            return None;
        }
        if let canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event {
            let items = layout(&self.model, bounds);
            if let Some(index) = hovered_item(&items, cursor.position_in(bounds), self.model.mode) {
                if let Some(action) = items[index].action {
                    return Some(canvas::Action::publish(action.into()).and_capture());
                }
            }
        }
        None
    }

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let scale = self.model.scale;
        let text_scale = scale;
        // The canvas uses the same final production footprint as draw and hits.
        let mut frame = canvas::Frame::with_bounds(
            renderer,
            Rectangle {
                x: -bounds.x,
                y: -bounds.y,
                width: self.model.viewport[0],
                height: self.model.viewport[1],
            },
        );
        let items = layout(&self.model, bounds);
        let hovered = self
            .input_active
            .then(|| hovered_item(&items, cursor.position_in(bounds), self.model.mode))
            .flatten();
        let mut order: Vec<_> = (0..items.len()).collect();
        order.sort_by_key(|i| {
            if items[*i].active {
                10
            } else if hovered == Some(*i) {
                5
            } else {
                1
            }
        });
        for index in order {
            let item = &items[index];
            let folder = (item.action == Some(RailIntent::ToggleFolder))
                .then(|| folder_content(item, text_scale));
            let logical_x = |offset: f32| {
                if item.rtl {
                    item.rect.x + item.rect.width - offset
                } else {
                    item.rect.x + offset
                }
            };
            let is_hovered = hovered == Some(index);
            let shape = item_path(item.rect, self.model.mode, self.model.rtl, item.skew);
            let fill = if item.active {
                color_with_alpha(self.model.palette.accent, 1.0)
            } else if is_hovered {
                color_with_alpha(self.model.palette.hover, 0.82)
            } else {
                color_with_alpha(self.model.palette.container, 0.92)
            };
            let border = color_with_alpha(
                self.model.palette.outline,
                if item.active {
                    0.72
                } else if is_hovered {
                    0.58
                } else {
                    0.4
                },
            );
            if item.action.is_some() {
                frame.fill(&shape, fill);
                frame.stroke(
                    &shape,
                    canvas::Stroke {
                        style: canvas::Style::Solid(border),
                        width: scale,
                        ..Default::default()
                    },
                );
            }
            if let Some(center) = folder.as_ref().and_then(|content| content.arrow) {
                let x = center.x;
                let y = center.y;
                let sign = if self.model.folder_open { -1.0 } else { 1.0 };
                let arrow = canvas::Path::new(|p| {
                    p.move_to(Point::new(x - 3.0 * scale, y - sign * 2.0 * scale));
                    p.line_to(Point::new(x + 3.0 * scale, y - sign * 2.0 * scale));
                    p.line_to(Point::new(x, y + sign * 2.0 * scale));
                    p.close();
                });
                frame.fill(
                    &arrow,
                    if item.active {
                        self.model.palette.active_text
                    } else {
                        self.model.palette.text
                    },
                );
            }
            let text_color = if item.active {
                self.model.palette.active_text
            } else if item.action.is_none() {
                color_with_alpha(self.model.palette.text, 0.7)
            } else {
                self.model.palette.text
            };
            let swatch_rgb = match item.action {
                Some(RailIntent::HuePreset(i)) => ColorBucket::from_index(i).map(ColorBucket::rgb),
                _ => None,
            };
            let center_y = item.rect.center_y();
            if let Some(rgb) = swatch_rgb {
                let outer = shape;
                frame.fill(&outer, color_with_alpha(self.model.palette.background, 0.9));
                let inset = scale.min(item.rect.width * 0.2);
                let inner = Rectangle::new(
                    Point::new(item.rect.x + inset, item.rect.y + inset),
                    Size::new(
                        (item.rect.width - 2.0 * inset).max(0.1),
                        (item.rect.height - 2.0 * inset).max(0.1),
                    ),
                );
                let mut color = Color::from_rgb8(rgb[0], rgb[1], rgb[2]);
                if is_hovered && !item.active {
                    color.r = (color.r * 1.2).min(1.0);
                    color.g = (color.g * 1.2).min(1.0);
                    color.b = (color.b * 1.2).min(1.0);
                }
                frame.fill(
                    &item_path(inner, self.model.mode, item.rtl, item.skew),
                    color_with_alpha(color, self.model.palette.text.a),
                );
                if item.active {
                    frame.stroke(
                        &outer,
                        canvas::Stroke::default()
                            .with_color(self.model.palette.accent)
                            .with_width(1.5 * scale),
                    );
                }
                continue;
            }
            if let Some(icon) = &item.icon {
                let position = if let Some(content) = &folder {
                    content.icon
                } else {
                    (item.rect.width >= 18.0 * scale).then(|| {
                        Point::new(
                            if item.label.is_empty() {
                                item.rect.center_x()
                            } else {
                                logical_x(16.0 * text_scale)
                            },
                            center_y,
                        )
                    })
                };
                if let Some(position) = position {
                    frame.fill_text(canvas::Text {
                        content: icon.clone(),
                        position,
                        color: text_color,
                        size: Pixels::from(if folder.is_some() {
                            label_size(scale)
                        } else {
                            icon_size(scale)
                        }),
                        font: self.model.icon_font,
                        align_x: text::Alignment::Center,
                        align_y: alignment::Vertical::Center,
                        line_height: text::LineHeight::Relative(typography::CONTROL_LINE_HEIGHT),
                        ..Default::default()
                    });
                }
            }
            let [lead, trail] = label_insets(item.icon.is_some(), text_scale);
            let available = folder.as_ref().map_or_else(
                || (item.rect.width - lead - trail).max(0.0),
                |content| content.label.width,
            );
            let label = fitted_label(item, text_scale, self.model.font, available);
            let center_x = folder.as_ref().map_or_else(
                || logical_x(lead + available * 0.5),
                |content| content.label.center_x(),
            );
            if !item.label.is_empty() && available >= label_size(scale) * 0.64 {
                frame.fill_text(canvas::Text {
                    content: label,
                    position: Point::new(center_x, center_y),
                    color: text_color,
                    size: Pixels::from(label_size(scale)),
                    font: self.model.font,
                    shaping: text::Shaping::Advanced,
                    line_height: text::LineHeight::Relative(typography::CONTROL_LINE_HEIGHT),
                    align_x: text::Alignment::Center,
                    align_y: alignment::Vertical::Center,
                    max_width: available,
                    ..Default::default()
                });
            }
        }
        vec![frame.into_geometry()]
    }

    fn mouse_interaction(
        &self,
        _state: &Self::State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if !self.input_active || self.model.reveal < 0.95 {
            return mouse::Interaction::None;
        }

        let items = layout(&self.model, bounds);
        hovered_item(&items, cursor.position_in(bounds), self.model.mode)
            .and_then(|index| items.get(index).and_then(|item| item.action))
            .map_or(mouse::Interaction::None, |_| mouse::Interaction::Pointer)
    }
}

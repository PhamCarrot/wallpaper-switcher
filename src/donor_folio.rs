//! Included folio composition adapted from pinned beta.13 ui/misc/{folio,
//! layout,style} and settings/folio/folio.rs. Content and messages stay -owned.
#[path = "donor_folio/source_ui/mod.rs"]
pub(crate) mod source_ui;
use crate::donor_ui::{self, Palette};
use iced::widget::canvas::{self, Frame, Path, Stroke};
use iced::widget::{button, column, container, mouse_area, row, scrollable, stack, text, Space};
use iced::{Element, Length, Padding};
use iced::{Point, Rectangle};

#[derive(Clone, Copy)]
pub(crate) enum BlueprintKind {
    Settings,
    Folio,
}

struct Blueprint {
    palette: Palette,
    kind: BlueprintKind,
}
impl<M> canvas::Program<M> for Blueprint {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &iced::Renderer,
        _: &iced::Theme,
        bounds: Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let (x, y, radii, line_alpha, accent_alpha, marker_width) = match self.kind {
            BlueprintKind::Settings => (0.72, 0.56, [90.0, 176.0, 264.0], 0.075, 0.09, 1.3),
            BlueprintKind::Folio => (0.76, 0.48, [92.0, 184.0, 276.0], 0.07, 0.1, 1.0),
        };
        let center = Point::new(bounds.width * x, bounds.height * y);
        let stroke = Stroke::default()
            .with_color(donor_ui::with_alpha_color(self.palette.outline, line_alpha))
            .with_width(1.0);
        for radius in radii {
            frame.stroke(&Path::circle(center, radius), stroke);
        }
        frame.stroke(
            &Path::line(
                Point::new(center.x, 0.0),
                Point::new(center.x, bounds.height),
            ),
            stroke,
        );
        frame.stroke(
            &Path::line(
                Point::new(0.0, center.y),
                Point::new(bounds.width, center.y),
            ),
            stroke,
        );
        frame.stroke(
            &diamond(center, 7.0),
            Stroke::default()
                .with_color(donor_ui::with_alpha_color(
                    self.palette.primary,
                    accent_alpha,
                ))
                .with_width(marker_width),
        );
        vec![frame.into_geometry()]
    }
}
fn diamond(center: Point, radius: f32) -> Path {
    Path::new(|b| {
        b.move_to(Point::new(center.x, center.y - radius));
        b.line_to(Point::new(center.x + radius, center.y));
        b.line_to(Point::new(center.x, center.y + radius));
        b.line_to(Point::new(center.x - radius, center.y));
        b.close();
    })
}
fn tracker_position(active: usize, count: usize) -> f32 {
    if count <= 1 {
        0.0
    } else {
        active.min(count - 1) as f32 / (count - 1) as f32
    }
}
pub(crate) fn index_rail<'a, M: 'a>(
    active: usize,
    count: usize,
    palette: Palette,
    scale: f32,
) -> Element<'a, M> {
    iced::widget::canvas(IndexRail {
        active,
        count,
        palette,
    })
    .width(14.0 * scale)
    .height(Length::Fill)
    .into()
}
struct IndexRail {
    active: usize,
    count: usize,
    palette: Palette,
}
impl<M> canvas::Program<M> for IndexRail {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &iced::Renderer,
        _: &iced::Theme,
        bounds: Rectangle,
        _: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let x = bounds.width * 0.5;
        let top = 8.0;
        let bottom = (bounds.height - 8.0).max(top);
        frame.stroke(
            &Path::line(Point::new(x, top), Point::new(x, bottom)),
            Stroke::default()
                .with_color(donor_ui::with_alpha_color(self.palette.outline, 0.48))
                .with_width(1.0),
        );
        for index in 0..self.count {
            let y = top + (bottom - top) * tracker_position(index, self.count);
            frame.stroke(
                &Path::line(Point::new(x - 2.5, y), Point::new(x + 2.5, y)),
                Stroke::default()
                    .with_color(donor_ui::with_alpha_color(self.palette.outline, 0.58))
                    .with_width(1.0),
            );
        }
        if self.count > 0 {
            let marker = diamond(
                Point::new(
                    x,
                    top + (bottom - top) * tracker_position(self.active, self.count),
                ),
                5.0,
            );
            frame.fill(&marker, self.palette.background);
            frame.stroke(
                &marker,
                Stroke::default()
                    .with_color(self.palette.primary)
                    .with_width(1.5),
            );
        }
        vec![frame.into_geometry()]
    }
}

pub(crate) fn dimensions(viewport: [f32; 2], scale: f32) -> [f32; 3] {
    let (w, h) = donor_ui::sheet_dimensions(viewport.into(), scale);
    [w, h, (donor_ui::FOLIO_INDEX_WIDTH * scale).min(w * 0.38)]
}
pub(crate) fn rule<'a, M: 'a>(palette: Palette) -> Element<'a, M> {
    container(Space::new())
        .width(Length::Fill)
        .height(1)
        .style(move |_| {
            donor_ui::box_style(
                donor_ui::with_alpha_color(palette.outline, 0.58),
                iced::Color::TRANSPARENT,
            )
        })
        .into()
}
pub(crate) fn index<'a, M: 'a>(
    title: impl Into<String>,
    note: impl Into<String>,
    content: Element<'a, M>,
    palette: Palette,
    scale: f32,
) -> Element<'a, M> {
    container(
        column![
            text(title.into()).size(20.0 * scale),
            text(note.into())
                .size(11.0 * scale)
                .color(donor_ui::with_alpha_color(palette.surface_text, 0.56)),
            content
        ]
        .spacing(17.0 * scale),
    )
    .padding(Padding {
        top: 25.0 * scale,
        right: 20.0 * scale,
        bottom: 20.0 * scale,
        left: 22.0 * scale,
    })
    .height(Length::Fill)
    .width(Length::Fill)
    .style(move |_| donor_ui::index_style(palette))
    .into()
}
pub(crate) fn section<'a, M: 'a>(
    title: impl Into<String>,
    note: impl Into<String>,
    body: Element<'a, M>,
    palette: Palette,
    scale: f32,
) -> Element<'a, M> {
    column![
        text(title.into()).size(14.0 * scale).color(palette.primary),
        text(note.into())
            .size(11.0 * scale)
            .color(donor_ui::with_alpha_color(palette.surface_text, 0.56)),
        rule(palette),
        body
    ]
    .spacing(12.0 * scale)
    .width(Length::Fill)
    .into()
}
pub(crate) fn page<'a, M: Clone + 'a>(
    title: impl Into<String>,
    note: impl Into<String>,
    body: Element<'a, M>,
    close: Option<M>,
    footer: Option<Element<'a, M>>,
    palette: Palette,
    kind: BlueprintKind,
    scale: f32,
) -> Element<'a, M> {
    let mut heading = row![text(title.into())
        .size(
            match kind {
                BlueprintKind::Settings => 46.0,
                BlueprintKind::Folio => 34.0,
            } * scale
        )
        .width(Length::Fill)]
    .align_y(iced::Alignment::Start);
    if let Some(close) = close {
        heading = heading.push(
            button(text("×").size(14))
                .on_press(close)
                .padding([6, 12])
                .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
        );
    }
    let header = column![
        heading,
        text(note.into())
            .size(11.0 * scale)
            .color(donor_ui::with_alpha_color(palette.surface_text, 0.56)),
        rule(palette)
    ]
    .spacing(12.0 * scale);
    let mut page = column![header, scrollable(body).height(Length::Fill)]
        .spacing(28.0 * scale)
        .width(Length::Fill)
        .height(Length::Fill);
    if let Some(footer) = footer {
        page = page.push(rule(palette)).push(footer);
    }
    let reading = container(page)
        .padding(Padding {
            top: 34.0 * scale,
            right: 38.0 * scale,
            bottom: 48.0 * scale,
            left: 38.0 * scale,
        })
        .width(Length::Fill)
        .height(Length::Fill);
    container(stack![
        iced::widget::canvas(Blueprint { palette, kind })
            .width(Length::Fill)
            .height(Length::Fill),
        reading
    ])
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| {
        donor_ui::box_style(
            donor_ui::with_alpha_color(palette.surface, 0.965),
            iced::Color::TRANSPARENT,
        )
    })
    .into()
}
pub(crate) fn sheet<'a, M: Clone + 'a>(
    viewport: [f32; 2],
    index: Option<Element<'a, M>>,
    page: Element<'a, M>,
    palette: Palette,
    capture: M,
    masthead: Option<(String, M)>,
    scale: f32,
) -> Element<'a, M> {
    sheet_with_scroll_capture(
        viewport,
        index,
        page,
        palette,
        capture.clone(),
        capture,
        masthead,
        scale,
    )
}

pub(crate) fn sheet_with_scroll_capture<'a, M: Clone + 'a>(
    viewport: [f32; 2],
    index: Option<Element<'a, M>>,
    page: Element<'a, M>,
    palette: Palette,
    capture: M,
    scroll_capture: M,
    masthead: Option<(String, M)>,
    scale: f32,
) -> Element<'a, M> {
    let [w, h, iw] = dimensions(viewport, scale);
    let body: Element<'a, M> = if let Some(index) = index {
        row![container(index).width(iw), page]
            .spacing(0)
            .height(Length::Fill)
            .into()
    } else {
        page
    };
    let body = if let Some((label, close)) = masthead {
        column![
            container(
                row![
                    text("Wallpaper Switcher").size(14.0 * scale),
                    Space::new().width(Length::Fill),
                    text(label)
                        .size(10.0 * scale)
                        .color(donor_ui::with_alpha_color(palette.surface_text, 0.54)),
                    button("×")
                        .on_press(close)
                        .style(move |_, status| donor_ui::action_style(
                            palette, false, false, status
                        ))
                ]
                .spacing(13.0 * scale)
                .align_y(iced::Alignment::Center)
            )
            .height(54.0 * scale.max(1.0))
            .padding([10.0 * scale, 20.0 * scale])
            .style(move |_| donor_ui::box_style(
                donor_ui::with_alpha_color(palette.background, 0.94),
                donor_ui::with_alpha_color(palette.outline, 0.5)
            )),
            body
        ]
        .spacing(0)
        .height(Length::Fill)
        .into()
    } else {
        body
    };
    let panel = container(body)
        .width(w)
        .height(h)
        .clip(true)
        .style(move |_| donor_ui::sheet_style(palette, 1.0));
    let panel = sheet_panel(panel.into(), scroll_capture.clone());
    let backdrop = mouse_area(
        container(Space::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| donor_ui::scrim_style(1.0)),
    )
    .interaction(iced::mouse::Interaction::Idle)
    .on_press(capture)
    .on_scroll(move |_| scroll_capture.clone());
    stack![
        backdrop,
        container(panel)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
    ]
    .into()
}

/// Capture wheel input at the sheet boundary when the nested body cannot
/// consume it. The panel stays above the scrim, but the wrapper is sized to
/// the panel rather than the whole viewport so scrim events remain owned by
/// the backdrop instead of levitating the cursor through `Stack`.
pub(crate) fn sheet_panel<'a, M, Renderer>(
    panel: Element<'a, M, iced::Theme, Renderer>,
    capture: M,
) -> Element<'a, M, iced::Theme, Renderer>
where
    M: Clone + 'a,
    Renderer: iced::advanced::Renderer + 'a,
{
    let panel_scroll_capture = capture;
    mouse_area(panel)
        .on_scroll(move |_| panel_scroll_capture.clone())
        .into()
}
#[cfg(test)]
mod tests {
    use super::*;
    use iced::Event;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum ProbeMessage {
        PanelScroll,
        BackdropScroll,
        InnerScroll(i32),
        PanelPress,
    }

    fn probe_modal() -> iced::Element<'static, ProbeMessage, iced::Theme, ()> {
        let content: iced::Element<'static, ProbeMessage, iced::Theme, ()> = text("panel").into();
        let panel: iced::Element<'static, ProbeMessage, iced::Theme, ()> = container(content)
            .width(Length::Fixed(100.0))
            .height(Length::Fixed(100.0))
            .into();
        let backdrop = mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .interaction(iced::mouse::Interaction::Idle)
        .on_scroll(|_| ProbeMessage::BackdropScroll);

        stack![
            backdrop,
            container(sheet_panel(panel, ProbeMessage::PanelScroll))
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        ]
        .into()
    }

    fn probe_overflowing_modal() -> iced::Element<'static, ProbeMessage, iced::Theme, ()> {
        let content: iced::Element<'static, ProbeMessage, iced::Theme, ()> =
            iced::widget::column![Space::new()
                .width(Length::Fill)
                .height(Length::Fixed(500.0)),]
            .into();
        let body: iced::Element<'static, ProbeMessage, iced::Theme, ()> = scrollable(content)
            .width(Length::Fill)
            .height(Length::Fixed(100.0))
            .on_scroll(|viewport| {
                ProbeMessage::InnerScroll(viewport.absolute_offset().y.round() as i32)
            })
            .into();
        let panel: iced::Element<'static, ProbeMessage, iced::Theme, ()> = container(body)
            .width(Length::Fixed(100.0))
            .height(Length::Fixed(100.0))
            .into();
        let backdrop = mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .interaction(iced::mouse::Interaction::Idle)
        .on_scroll(|_| ProbeMessage::BackdropScroll);

        stack![
            backdrop,
            container(sheet_panel(panel, ProbeMessage::PanelScroll))
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        ]
        .into()
    }

    fn probe_button_modal() -> iced::Element<'static, ProbeMessage, iced::Theme, ()> {
        let panel: iced::Element<'static, ProbeMessage, iced::Theme, ()> = mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .on_press(ProbeMessage::PanelPress)
        .into();
        let panel = container(panel)
            .width(Length::Fixed(100.0))
            .height(Length::Fixed(100.0))
            .into();
        let backdrop = mouse_area(
            container(Space::new())
                .width(Length::Fill)
                .height(Length::Fill),
        )
        .interaction(iced::mouse::Interaction::Idle)
        .on_scroll(|_| ProbeMessage::BackdropScroll);

        stack![
            backdrop,
            container(sheet_panel(panel, ProbeMessage::PanelScroll))
                .center_x(Length::Fill)
                .center_y(Length::Fill),
        ]
        .into()
    }

    fn dispatch_wheel(
        element: &mut iced::Element<'static, ProbeMessage, iced::Theme, ()>,
        point: Point,
    ) -> Vec<ProbeMessage> {
        dispatch_wheel_with_delta(element, point, 1.0)
    }

    fn dispatch_wheel_with_delta(
        element: &mut iced::Element<'static, ProbeMessage, iced::Theme, ()>,
        point: Point,
        delta_y: f32,
    ) -> Vec<ProbeMessage> {
        dispatch_wheel_repeated(element, point, delta_y, 1)
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    fn dispatch_wheel_repeated(
        element: &mut iced::Element<'static, ProbeMessage, iced::Theme, ()>,
        point: Point,
        delta_y: f32,
        count: usize,
    ) -> Vec<Vec<ProbeMessage>> {
        use iced::advanced::mouse;
        use iced::Event;

        let events = (0..count)
            .map(|_| {
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: 0.0, y: delta_y },
                })
            })
            .collect();
        dispatch_events(element, point, events)
    }

    fn dispatch_events(
        element: &mut iced::Element<'static, ProbeMessage, iced::Theme, ()>,
        point: Point,
        events: Vec<Event>,
    ) -> Vec<Vec<ProbeMessage>> {
        use iced::advanced::{clipboard, layout, mouse, widget::Tree, Layout, Shell};
        use iced::Size;

        let renderer = ();
        let mut tree = Tree::new(element.as_widget());
        let limits = layout::Limits::new(Size::ZERO, Size::new(300.0, 300.0));
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        let layout = Layout::new(&node);
        events
            .into_iter()
            .map(|event| {
                let mut messages = Vec::new();
                let mut shell = Shell::new(&mut messages);
                let mut clipboard = clipboard::Null;
                element.as_widget_mut().update(
                    &mut tree,
                    &event,
                    layout,
                    mouse::Cursor::Available(point),
                    &renderer,
                    &mut clipboard,
                    &mut shell,
                    &Rectangle::new(Point::ORIGIN, Size::new(300.0, 300.0)),
                );
                messages
            })
            .collect()
    }

    fn mouse_interaction_at(
        element: &mut iced::Element<'static, ProbeMessage, iced::Theme, ()>,
        point: Point,
    ) -> iced::mouse::Interaction {
        use iced::advanced::{layout, mouse, widget::Tree, Layout};
        use iced::Size;

        let renderer = ();
        let mut tree = Tree::new(element.as_widget());
        let limits = layout::Limits::new(Size::ZERO, Size::new(300.0, 300.0));
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        element.as_widget().mouse_interaction(
            &tree,
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &Rectangle::new(Point::ORIGIN, Size::new(300.0, 300.0)),
            &renderer,
        )
    }

    fn dispatch_wheel_roundtrip(
        element: &mut iced::Element<'static, ProbeMessage, iced::Theme, ()>,
        point: Point,
        count: usize,
    ) -> (Vec<Vec<ProbeMessage>>, Vec<Vec<ProbeMessage>>) {
        use iced::advanced::{clipboard, layout, mouse, widget::Tree, Layout, Shell};
        use iced::Size;

        let renderer = ();
        let mut tree = Tree::new(element.as_widget());
        let limits = layout::Limits::new(Size::ZERO, Size::new(300.0, 300.0));
        let node = element
            .as_widget_mut()
            .layout(&mut tree, &renderer, &limits);
        let layout = Layout::new(&node);
        let mut dispatch = |event: Event| {
            let mut messages = Vec::new();
            let mut shell = Shell::new(&mut messages);
            let mut clipboard = clipboard::Null;
            element.as_widget_mut().update(
                &mut tree,
                &event,
                layout,
                mouse::Cursor::Available(point),
                &renderer,
                &mut clipboard,
                &mut shell,
                &Rectangle::new(Point::ORIGIN, Size::new(300.0, 300.0)),
            );
            messages
        };

        let mut run_wheels = |delta_y: f32| {
            (0..count)
                .map(|_| {
                    dispatch(Event::Mouse(mouse::Event::WheelScrolled {
                        delta: mouse::ScrollDelta::Lines { x: 0.0, y: delta_y },
                    }))
                })
                .collect::<Vec<_>>()
        };
        let mut down = run_wheels(-1.0);
        drop(run_wheels);
        std::thread::sleep(std::time::Duration::from_millis(120));
        down.push(dispatch(Event::Mouse(mouse::Event::CursorMoved {
            position: point,
        })));
        down.push(dispatch(Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: -1.0 },
        })));

        let mut run_wheels = |delta_y: f32| {
            (0..count)
                .map(|_| {
                    dispatch(Event::Mouse(mouse::Event::WheelScrolled {
                        delta: mouse::ScrollDelta::Lines { x: 0.0, y: delta_y },
                    }))
                })
                .collect::<Vec<_>>()
        };
        let mut up = run_wheels(1.0);
        drop(run_wheels);
        std::thread::sleep(std::time::Duration::from_millis(120));
        up.push(dispatch(Event::Mouse(mouse::Event::CursorMoved {
            position: point,
        })));
        up.push(dispatch(Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: 1.0 },
        })));
        (down, up)
    }
    #[test]
    fn pinned_sheet_and_index_dimensions_leave_usable_reading_space() {
        assert_eq!(
            dimensions([1920.0, 1080.0], 0.9),
            [1251.0, 783.0, 286.19998]
        );
        for viewport in [[800.0, 600.0], [1280.0, 720.0], [1920.0, 1080.0]] {
            let [w, h, i] = dimensions(viewport, 0.9);
            assert!(w <= viewport[0] && h <= viewport[1]);
            assert!(w - i >= 480.0 || viewport[0] < 800.0);
        }
    }

    #[test]
    fn sheet_panel_captures_scrim_wheels_without_clicking_through() {
        let mut modal = probe_modal();
        assert_eq!(
            dispatch_wheel(&mut modal, Point::new(8.0, 8.0)),
            vec![ProbeMessage::BackdropScroll]
        );
    }

    #[test]
    fn sheet_scrim_owns_pointer_interaction_over_underlying_gallery() {
        let mut modal = probe_modal();
        assert_eq!(
            mouse_interaction_at(&mut modal, Point::new(8.0, 8.0)),
            iced::mouse::Interaction::Idle
        );
    }

    #[test]
    fn sheet_panel_captures_panel_wheels_when_no_inner_scrollable_can_move() {
        let mut modal = probe_modal();
        assert_eq!(
            dispatch_wheel(&mut modal, Point::new(150.0, 150.0)),
            vec![ProbeMessage::PanelScroll]
        );
    }

    #[test]
    fn sheet_panel_preserves_inner_scrollable_capture() {
        let mut modal = probe_overflowing_modal();
        assert!(matches!(
            dispatch_wheel(&mut modal, Point::new(150.0, 150.0)).as_slice(),
            [ProbeMessage::InnerScroll(_)]
        ));
    }

    #[test]
    fn sheet_panel_preserves_pointer_capture_inside_the_panel() {
        use iced::advanced::mouse;

        let mut modal = probe_button_modal();
        let messages = dispatch_events(
            &mut modal,
            Point::new(150.0, 150.0),
            vec![Event::Mouse(mouse::Event::ButtonPressed(
                mouse::Button::Left,
            ))],
        );
        assert_eq!(messages, vec![vec![ProbeMessage::PanelPress]]);
    }

    #[test]
    fn sheet_panel_captures_wheels_at_the_top_scroll_limit() {
        let mut modal = probe_overflowing_modal();
        let point = Point::new(150.0, 150.0);
        let (down, up) = dispatch_wheel_roundtrip(&mut modal, point, 100);
        assert!(down.iter().any(|messages| {
            messages
                .iter()
                .any(|message| matches!(message, ProbeMessage::InnerScroll(_)))
        }));
        assert!(down
            .iter()
            .flat_map(|messages| messages.iter())
            .any(|message| {
                matches!(message, ProbeMessage::InnerScroll(offset) if *offset > 0)
            }));
        assert_eq!(down.last(), Some(&vec![ProbeMessage::PanelScroll]));
        assert!(up.iter().any(|messages| {
            messages
                .iter()
                .any(|message| matches!(message, ProbeMessage::InnerScroll(_)))
        }));
        assert!(up
            .iter()
            .flat_map(|messages| messages.iter())
            .any(|message| {
                matches!(message, ProbeMessage::InnerScroll(offset) if *offset == 0)
            }));
        assert_eq!(up.last(), Some(&vec![ProbeMessage::PanelScroll]));
    }
}

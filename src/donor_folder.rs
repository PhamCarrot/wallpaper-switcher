//! Measured presentation for the existing folder popup owner. No catalogue or filter mutation.
use crate::donor_back_layout::{measured_full_lines, measured_lines, measured_width};
use crate::donor_rail::RailMode;
use iced::widget::{canvas, text};
use iced::{alignment, mouse, Color, Element, Font, Length, Pixels, Point, Rectangle, Size, Theme};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct Row {
    pub path: Option<PathBuf>,
    pub label: String,
    pub depth: usize,
    pub selected: bool,
    pub unavailable: bool,
}
#[derive(Debug, Clone)]
pub(crate) struct Model {
    pub rows: Vec<Row>,
    pub identity: Vec<String>,
    pub width: f32,
    pub height: f32,
    pub content_height: f32,
    pub header_height: f32,
    pub scale: f32,
    pub mode: RailMode,
    pub rtl: bool,
    pub surface: Color,
    pub primary: Color,
    pub text: Color,
    pub font: Font,
    pub icon_font: Font,
}
impl Model {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        choices: &[PathBuf],
        configured_roots: &[PathBuf],
        selected: &[PathBuf],
        viewport: [f32; 2],
        scale: f32,
        mode: RailMode,
        rtl: bool,
        surface: Color,
        primary: Color,
        text: Color,
        font: Font,
        icon_font: Font,
    ) -> Self {
        let roots: Vec<_> = choices
            .iter()
            .filter(|folder| {
                configured_roots.contains(folder)
                    || !choices
                        .iter()
                        .any(|other| *folder != other && folder.starts_with(other))
            })
            .cloned()
            .collect();
        let mut folders = choices.to_vec();
        folders.sort();
        let mut rows = vec![Row {
            path: None,
            label: "All wallpapers".into(),
            depth: 0,
            selected: selected.is_empty(),
            unavailable: false,
        }];
        for folder in folders {
            let root = roots
                .iter()
                .filter(|root| folder.starts_with(root))
                .max_by_key(|root| root.components().count());
            let depth = root
                .and_then(|root| folder.strip_prefix(root).ok())
                .map_or(0, |relative| relative.components().count())
                .min(6);
            let label = short_label(&folder, choices);
            rows.push(Row {
                selected: selected.contains(&folder),
                unavailable: !folder.is_dir(),
                path: Some(folder),
                label,
                depth,
            });
        }
        let size = 10.0 * scale;
        let width = rows
            .iter()
            .map(|row| {
                measured_width(&row.label, size, font) + (65.0 + row.depth as f32 * 16.0) * scale
            })
            .fold(170.0 * scale, f32::max)
            .min((viewport[0] - 16.0).max(1.0));
        let full = selected
            .iter()
            .map(|folder| {
                format!(
                    "{}{}",
                    folder.display(),
                    if folder.is_dir() { "" } else { "\nunavailable" }
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        // No row-focus capability is invented: this adaptation exposes selected paths only.
        let identity = if full.is_empty() {
            Vec::new()
        } else {
            measured_full_lines(
                &full,
                size,
                (width
                    - 24.0 * scale
                    - if mode == RailMode::Slices { 14.0 } else { 8.0 }
                    - if mode == RailMode::Geometric {
                        width * 0.36
                    } else {
                        0.0
                    })
                .max(1.0),
                font,
            )
        };
        let header_height = if identity.is_empty() {
            0.0
        } else {
            identity.len() as f32 * size * 1.25 + 10.0 * scale
        };
        let content_height = 14.0 + header_height + rows.len() as f32 * 26.0 * scale;
        let height =
            (14.0 + header_height.min(78.0 * scale) + rows.len().min(12) as f32 * 26.0 * scale)
                .min((viewport[1] - 16.0).max(1.0));
        Self {
            rows,
            identity,
            width,
            height,
            content_height,
            header_height,
            scale,
            mode,
            rtl,
            surface,
            primary,
            text,
            font,
            icon_font,
        }
    }
    pub(crate) fn max_scroll(&self) -> f32 {
        (self.content_height - self.height).max(0.0)
    }
    pub(crate) fn row_rect(&self, index: usize, scroll: f32) -> Rectangle {
        Rectangle::new(
            Point::new(
                4.0,
                4.0 + self.header_height + index as f32 * 26.0 * self.scale - scroll,
            ),
            Size::new((self.width - 8.0).max(1.0), 26.0 * self.scale),
        )
    }
    fn scroll_track(&self) -> Option<Rectangle> {
        let edge = panel_cut(self.width, self.height, self.mode);
        (self.max_scroll() > 0.0 && self.width > edge * 2.0 + 12.0 && self.height > 12.0).then(
            || {
                Rectangle::new(
                    Point::new(self.width - edge - 8.0, 4.0),
                    Size::new(6.0, self.height - 8.0),
                )
            },
        )
    }

    pub(crate) fn scroll_thumb(&self, scroll: f32) -> Option<Rectangle> {
        let track = self.scroll_track()?;
        let height = (track.height * self.height / self.content_height)
            .max(12.0)
            .min(track.height);
        let y = track.y + scroll / self.max_scroll() * (track.height - height);
        Some(Rectangle::new(
            Point::new(track.x + 2.0, y),
            Size::new(3.0, height),
        ))
    }

    fn scroll_for_pointer(&self, cursor_y: f32, grab: f32) -> Option<f32> {
        let track = self.scroll_track()?;
        let travel = track.height - self.scroll_thumb(0.0)?.height;
        (travel > 0.0)
            .then(|| ((cursor_y - track.y - grab) / travel).clamp(0.0, 1.0) * self.max_scroll())
    }

    fn panel_contains(&self, point: Point) -> bool {
        contains(
            &polygon(
                Rectangle::with_size(Size::new(self.width, self.height)),
                self.mode,
                self.rtl,
            ),
            point,
        )
    }

    pub(crate) fn row_at(&self, point: Point, scroll: f32) -> Option<usize> {
        if !self.panel_contains(point)
            || point.x < 4.0
            || point.x >= self.width - 4.0
            || self
                .scroll_track()
                .is_some_and(|track| track.contains(point))
            || point.y < 4.0
            || point.y >= self.height - 4.0
        {
            return None;
        }
        let y = point.y + scroll - 4.0 - self.header_height;
        if y < 0.0 {
            return None;
        }
        let index = (y / (26.0 * self.scale)).floor() as usize;
        (index < self.rows.len()
            && contains(
                &polygon(self.row_rect(index, scroll), self.mode, self.rtl),
                point,
            ))
        .then_some(index)
    }
}
pub(crate) fn short_label(folder: &Path, choices: &[PathBuf]) -> String {
    let base = folder.file_name().map_or_else(
        || folder.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    if choices
        .iter()
        .filter(|other| other.file_name() == folder.file_name())
        .count()
        <= 1
    {
        return base;
    }
    let components: Vec<_> = folder.components().collect();
    for count in 2..=components.len() {
        let suffix: PathBuf = components[components.len() - count..].iter().collect();
        if choices
            .iter()
            .filter(|other| other.ends_with(&suffix))
            .count()
            == 1
        {
            return suffix.display().to_string();
        }
    }
    folder.display().to_string()
}
#[derive(Debug, Default)]
pub(crate) struct State {
    pub scroll: f32,
    grab_offset: Option<f32>,
}
#[derive(Debug, Clone)]
pub(crate) struct Program {
    pub model: Model,
}
pub(crate) fn view(model: Model) -> Element<'static, Option<PathBuf>> {
    let size = [model.width, model.height];
    canvas::Canvas::new(Program { model })
        .width(Length::Fixed(size[0]))
        .height(Length::Fixed(size[1]))
        .into()
}
fn panel_cut(width: f32, height: f32, mode: RailMode) -> f32 {
    match mode {
        RailMode::Slices => 10.0_f32.min(width * 0.25).min(height * 0.5),
        RailMode::Geometric => (height * 0.28).min(width * 0.18).max(2.0).min(width * 0.5),
        RailMode::Wall => 0.0,
    }
}

fn polygon(rect: Rectangle, mode: RailMode, rtl: bool) -> Vec<Point> {
    let (x, y, w, h) = (rect.x, rect.y, rect.width, rect.height);
    let cut = panel_cut(w, h, mode);
    let points = match mode {
        RailMode::Wall => vec![(x, y), (x + w, y), (x + w, y + h), (x, y + h)],
        RailMode::Geometric => vec![
            (x + cut, y),
            (x + w - cut, y),
            (x + w, y + h * 0.5),
            (x + w - cut, y + h),
            (x + cut, y + h),
            (x, y + h * 0.5),
        ],
        RailMode::Slices if rtl => vec![
            (x, y),
            (x + w - cut, y),
            (x + w, y + cut),
            (x + w, y + h),
            (x + cut, y + h),
            (x, y + h - cut),
        ],
        RailMode::Slices => vec![
            (x + cut, y),
            (x + w, y),
            (x + w, y + h - cut),
            (x + w - cut, y + h),
            (x, y + h),
            (x, y + cut),
        ],
    };
    points.into_iter().map(|(x, y)| Point::new(x, y)).collect()
}
fn path(points: &[Point]) -> canvas::Path {
    canvas::Path::new(|p| {
        if let Some(first) = points.first() {
            p.move_to(*first);
            for point in &points[1..] {
                p.line_to(*point);
            }
            p.close();
        }
    })
}
fn panel(rect: Rectangle, mode: RailMode, rtl: bool) -> canvas::Path {
    path(&polygon(rect, mode, rtl))
}
fn cross(a: Point, b: Point, p: Point) -> f32 {
    (b.x - a.x) * (p.y - a.y) - (b.y - a.y) * (p.x - a.x)
}
fn contains(points: &[Point], point: Point) -> bool {
    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .all(|(a, b)| cross(*a, *b, point) >= -0.001)
}
fn intersect(mut points: Vec<Point>, clip: &[Point]) -> Vec<Point> {
    for (a, b) in clip
        .iter()
        .zip(clip.iter().cycle().skip(1))
        .take(clip.len())
    {
        let old = std::mem::take(&mut points);
        if old.is_empty() {
            break;
        }
        let mut previous = *old.last().unwrap();
        let mut before = cross(*a, *b, previous);
        for current in old {
            let after = cross(*a, *b, current);
            if (after >= 0.0) != (before >= 0.0) {
                let ratio = before / (before - after);
                points.push(Point::new(
                    previous.x + (current.x - previous.x) * ratio,
                    previous.y + (current.y - previous.y) * ratio,
                ));
            }
            if after >= 0.0 {
                points.push(current);
            }
            previous = current;
            before = after;
        }
    }
    points
}
fn alpha(mut color: Color, value: f32) -> Color {
    color.a *= value;
    color
}
impl canvas::Program<Option<PathBuf>> for Program {
    type State = State;
    fn update(
        &self,
        state: &mut State,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<Option<PathBuf>>> {
        let m = &self.model;
        state.scroll = state.scroll.clamp(0.0, m.max_scroll());
        let point = cursor.position_in(bounds).filter(|p| m.panel_contains(*p));
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
                if state.grab_offset.take().is_some() =>
            {
                return Some(canvas::Action::request_redraw().and_capture());
            }
            canvas::Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if let (Some(grab), Some(p)) = (state.grab_offset, cursor.position()) {
                    if let Some(scroll) = m.scroll_for_pointer(p.y - bounds.y, grab) {
                        state.scroll = scroll;
                    }
                    return Some(canvas::Action::request_redraw().and_capture());
                }
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta }) if point.is_some() => {
                let dy = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => y * 26.0 * m.scale,
                    mouse::ScrollDelta::Pixels { y, .. } => *y,
                };
                state.scroll = (state.scroll - dy).clamp(0.0, m.max_scroll());
                return Some(canvas::Action::request_redraw().and_capture());
            }
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = point?;
                if m.scroll_track().is_some_and(|track| track.contains(p)) {
                    if let Some(thumb) = m.scroll_thumb(state.scroll) {
                        let grab = if p.y >= thumb.y && p.y <= thumb.y + thumb.height {
                            p.y - thumb.y
                        } else {
                            thumb.height * 0.5
                        };
                        state.grab_offset = Some(grab);
                        if let Some(scroll) = m.scroll_for_pointer(p.y, grab) {
                            state.scroll = scroll;
                        }
                    }
                    return Some(canvas::Action::request_redraw().and_capture());
                }
                if let Some(index) = m.row_at(p, state.scroll) {
                    return Some(canvas::Action::publish(m.rows[index].path.clone()).and_capture());
                }
                return Some(canvas::Action::capture());
            }
            _ => {}
        }
        None
    }
    fn draw(
        &self,
        state: &State,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let m = &self.model;
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let background = panel(Rectangle::with_size(bounds.size()), m.mode, m.rtl);
        frame.fill(&background, alpha(m.surface, 0.97));
        frame.stroke(
            &background,
            canvas::Stroke::default()
                .with_color(alpha(m.primary, 0.4))
                .with_width(1.0),
        );
        let scroll = state.scroll.clamp(0.0, m.max_scroll());
        let hovered = cursor.position_in(bounds).and_then(|p| m.row_at(p, scroll));
        let size = 10.0 * m.scale;
        let lead = |inset: f32| if m.rtl { m.width - inset } else { inset };
        let track_inset = m
            .scroll_track()
            .map_or(0.0, |track| m.width - track.x + 2.0);
        let align = if m.rtl {
            text::Alignment::Right
        } else {
            text::Alignment::Left
        };
        frame.with_clip(
            Rectangle::new(
                Point::new(4.0, 4.0),
                Size::new((m.width - 12.0).max(1.0), (m.height - 8.0).max(1.0)),
            ),
            |frame| {
                for (index, line) in m.identity.iter().enumerate() {
                    frame.fill_text(canvas::Text {
                        content: line.clone(),
                        position: Point::new(
                            lead(
                                (12.0 * m.scale
                                    + if m.mode == RailMode::Geometric {
                                        m.width * 0.18
                                    } else {
                                        0.0
                                    })
                                .max(if m.rtl {
                                    track_inset
                                } else {
                                    0.0
                                }),
                            ),
                            8.0 * m.scale + index as f32 * size * 1.25 - scroll,
                        ),
                        color: m.text,
                        size: Pixels(size),
                        font: m.font,
                        shaping: text::Shaping::Advanced,
                        align_x: align,
                        align_y: alignment::Vertical::Top,
                        line_height: text::LineHeight::Relative(1.25),
                        ..Default::default()
                    });
                }
                for (index, row) in m.rows.iter().enumerate() {
                    let rect = m.row_rect(index, scroll);
                    if rect.y + rect.height < 4.0 || rect.y > m.height - 4.0 {
                        continue;
                    }
                    if row.selected || hovered == Some(index) {
                        let mut r = rect;
                        r.height = (r.height - 2.0).max(1.0);
                        let clipped = intersect(
                            polygon(r, m.mode, m.rtl),
                            &polygon(Rectangle::with_size(bounds.size()), m.mode, m.rtl),
                        );
                        frame.fill(
                            &path(&clipped),
                            alpha(m.primary, if row.selected { 0.16 } else { 0.08 }),
                        );
                    }
                    let outer_inset = if m.mode == RailMode::Geometric {
                        m.width * 0.18
                    } else {
                        0.0
                    };
                    let indent = (12.0 + outer_inset).max(if m.rtl { track_inset } else { 0.0 })
                        + row.depth as f32 * 16.0 * m.scale;
                    let icon_space = if row.path.is_some() {
                        size * 0.6 + 5.0
                    } else {
                        0.0
                    };
                    let start = indent + icon_space;
                    if row.path.is_some() {
                        frame.fill_text(canvas::Text {
                            content: "\u{f024b}".into(),
                            position: Point::new(lead(indent), rect.center_y()),
                            color: alpha(m.text, if row.unavailable { 0.5 } else { 0.7 }),
                            size: Pixels(size),
                            font: m.icon_font,
                            align_x: align,
                            align_y: alignment::Vertical::Center,
                            ..Default::default()
                        });
                    }
                    let tail = (16.0 + outer_inset).max(if m.rtl { 0.0 } else { track_inset });
                    let available = (m.width - start - tail).max(0.0);
                    let shown = measured_lines(&row.label, size, available, m.font, 1)
                        .into_iter()
                        .next()
                        .unwrap_or_default();
                    frame.fill_text(canvas::Text {
                        content: shown,
                        position: Point::new(lead(start), rect.center_y()),
                        color: if row.selected { m.primary } else { m.text },
                        size: Pixels(size),
                        font: m.font,
                        shaping: text::Shaping::Advanced,
                        align_x: align,
                        align_y: alignment::Vertical::Center,
                        max_width: available,
                        ..Default::default()
                    });
                }
            },
        );
        if let Some(thumb) = m.scroll_thumb(scroll) {
            frame.fill_rectangle(thumb.position(), thumb.size(), alpha(m.primary, 0.5));
        }
        vec![frame.into_geometry()]
    }
    fn mouse_interaction(
        &self,
        _state: &State,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor
            .position_in(bounds)
            .is_some_and(|p| self.model.panel_contains(p))
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::None
        }
    }
}

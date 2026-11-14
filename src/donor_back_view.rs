//! Drawn included BackPanel adaptation of pinned beta.13 back/{view,geometry,
//! background,content,actions}. Geometry transforms surfaces and positions;
//! text follows the donor angle treatment, without shearing glyphs.
use crate::donor_back_layout::{self, Action, BackLayout, DetailIntent};
use crate::donor_detail::CardGeometry;
use crate::donor_scene::card_flip_phases;
use crate::donor_ui::{with_alpha_color as alpha, Palette};
use iced::widget::{canvas, text};
use iced::{
    alignment, keyboard, mouse, Color, Element, Font, Length, Point, Rectangle, Theme, Vector,
};

#[derive(Clone)]
pub(crate) struct BackModel {
    pub identity: String,
    pub input_active: bool,
    pub card: CardGeometry,
    pub embedded: bool,
    pub scale: f32,
    pub progress: f32,
    pub depth: Option<crate::donor_scene::DepthDetailPresentation>,
    pub flip_options: crate::donor_scene::DepthDetailPresentation,
    pub opacity: f32,
    pub depth_revision: Option<u64>,
    pub radius: f32,
    pub title: String,
    pub kind: String,
    pub fields: Vec<(String, String)>,
    pub tags: String,
    pub favorite: bool,
    pub actions: Vec<Action>,
    pub palette: Palette,
    pub font: Font,
    pub icon_font: Font,
}

pub(crate) fn view<'a>(model: BackModel) -> Element<'a, DetailIntent> {
    canvas(BackProgram { model })
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}
struct BackProgram {
    model: BackModel,
}
#[derive(Default)]
pub(crate) struct BackState {
    identity: String,
    depth_geometry: Option<(CardGeometry, f32, bool, Vec<Rectangle>)>,
    focus: Option<usize>,
    pressed: Option<DetailIntent>,
    scroll: f32,
}

fn smoothstep(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    v * v * (3.0 - 2.0 * v)
}
fn rise(p: f32, slot: f32) -> (f32, f32) {
    let start = (slot * 0.095).min(0.58);
    let amount = smoothstep((p - start) / 0.42);
    (amount, (1.0 - amount) * 14.0)
}

impl BackProgram {
    fn alpha(&self, color: Color, amount: f32) -> Color {
        alpha(color, amount * self.model.opacity.clamp(0.0, 1.0))
    }

    fn layout(&self) -> BackLayout {
        let _timing = crate::apply_trace::cpu_span("detail-layout", || self.model.identity.clone());
        if self.model.depth.is_some() && self.model.embedded {
            return donor_back_layout::depth_layout(
                self.model.card,
                self.model.radius,
                self.model.fields.len(),
                &self.model.actions,
                self.model.scale,
                self.model.font,
                &self.model.title,
            );
        }
        donor_back_layout::layout(
            self.model.card,
            self.model.embedded,
            self.model.fields.len(),
            &self.model.actions,
            self.model.scale,
            self.model.font,
            &self.model.title,
        )
    }
    fn phases(&self) -> (f32, f32) {
        if self.model.embedded || self.model.depth.is_some() {
            let flags = self.model.depth.unwrap_or(self.model.flip_options);
            let p = card_flip_phases(
                self.model.progress,
                flags.animate_shader,
                flags.animate_back,
            );
            (p.surface, p.content)
        } else {
            let reveal = ((self.model.progress - 0.42) / 0.58).clamp(0.0, 1.0);
            let p = 0.55 + reveal * 0.45;
            (
                smoothstep((p - 0.46) / 0.3),
                ((p - 0.56) / 0.44).clamp(0.0, 1.0),
            )
        }
    }
    fn action_rect(&self, layout: &BackLayout, index: usize) -> Rectangle {
        let mut r = layout.actions[index];
        r.y += rise(self.phases().1, 3.35 + index as f32 * 0.15).1 * layout.content_scale;
        r
    }
    fn tag_button(&self, layout: &BackLayout, scroll: f32) -> Rectangle {
        let (_, value) = layout.source_text_bounds();
        let s = layout.content_scale;
        let height = donor_back_layout::TAG_BUTTON_HEIGHT * s;
        Rectangle {
            x: value.x,
            y: value.center_y() - scroll - height * 0.5,
            width: value.width.min(62.0 * s),
            height,
        }
    }

    fn controls(&self, layout: &BackLayout, scroll: f32) -> Vec<(Rectangle, DetailIntent, bool)> {
        let (_, p) = self.phases();
        let mut favorite = layout.favorite;
        favorite.y += rise(p, 0.0).1 * layout.content_scale;
        let mut close = layout.close;
        close.y += rise(p, 0.0).1 * layout.content_scale;
        let mut result: Vec<_> = self
            .model
            .actions
            .iter()
            .enumerate()
            .map(|(i, a)| {
                (
                    self.action_rect(layout, i),
                    a.intent,
                    a.enabled && rise(p, 3.35 + i as f32 * 0.15).0 > 0.5,
                )
            })
            .collect();
        result.extend([
            (favorite, DetailIntent::Favorite, rise(p, 0.0).0 > 0.5),
            (close, DetailIntent::Close, rise(p, 0.0).0 > 0.5),
        ]);
        let tag_button = self.tag_button(layout, scroll);
        let visible = if layout.wide {
            layout.source.height >= 24.0 * layout.content_scale
        } else {
            self.line_visible(layout, tag_button.center_y(), tag_button.height)
        };
        if visible {
            result.push((tag_button, DetailIntent::LocalTags, p > 0.5));
        }
        result
    }
    fn contains(&self, r: Rectangle, p: Point) -> bool {
        self.model.card.inverse_point([p.x, p.y]).is_some_and(|q| {
            if self.model.depth.is_some() {
                if !crate::donor_layout::depth_contains(
                    self.model.card.center,
                    self.model.card.size,
                    self.model.card.skew_distance,
                    [self.model.radius; 4],
                    [p.x, p.y],
                ) {
                    return false;
                }
            }
            r.contains(Point::new(q[0], q[1]))
        })
    }
    fn hit_at(&self, layout: &BackLayout, p: Point, scroll: f32) -> Option<DetailIntent> {
        if self.phases().1 <= 0.01 || self.model.opacity <= 0.0001 {
            return None;
        }
        self.controls(layout, scroll)
            .iter()
            .rev()
            .find_map(|(r, i, enabled)| {
                (*enabled && self.contains(*r, p) && self.contains(layout.card, p)).then_some(*i)
            })
    }
    fn content_region(&self, l: &BackLayout) -> Rectangle {
        let s = l.content_scale;
        Rectangle::new(
            Point::new(l.title.x, l.title.y + l.title.height + 4.0 * s),
            iced::Size::new(
                l.title.width,
                (l.actions_label.y - 12.0 * s - l.title.y - l.title.height - 4.0 * s).max(0.0),
            ),
        )
    }
    fn scroll_limit(&self, l: &BackLayout) -> f32 {
        if l.wide {
            return 0.0;
        }
        let region = self.content_region(l);
        let (_, value) = l.source_text_bounds();
        let tag_button = self.tag_button(l, 0.0);
        let tag_bottom = tag_button.y + tag_button.height;
        (tag_bottom.max(value.y + value.height) + 4.0 * l.content_scale - region.y - region.height)
            .max(0.0)
    }
    fn line_visible(&self, l: &BackLayout, center: f32, height: f32) -> bool {
        let region = self.content_region(l);
        l.wide
            || (center - height * 0.5 >= region.y
                && center + height * 0.5 <= region.y + region.height)
    }
    fn scroll_step(&self, l: &BackLayout, current: f32, forward: bool) -> f32 {
        let region = self.content_region(l);
        let limit = self.scroll_limit(l);
        // At short card heights, one field line is a page. Wheel navigation
        // visits each label/value instead of skipping rows that can never fit.
        let mut steps = vec![0.0, limit];
        for r in &l.facts {
            let (label, value) = l.fact_text_bounds(*r);
            for y in [label.center_y(), value.center_y()] {
                steps.push((y - region.center_y()).clamp(0.0, limit));
            }
        }
        let (label, value) = l.source_text_bounds();
        for y in [label.center_y(), value.center_y()] {
            steps.push((y - region.center_y()).clamp(0.0, limit));
        }
        steps.sort_by(f32::total_cmp);
        let precision = 0.01 * l.content_scale;
        steps.dedup_by(|a, b| (*a - *b).abs() < precision);
        if forward {
            steps
                .into_iter()
                .find(|v| *v > current + precision)
                .unwrap_or(limit)
        } else {
            steps
                .into_iter()
                .rev()
                .find(|v| *v < current - precision)
                .unwrap_or(0.0)
        }
    }
    fn path(&self, r: Rectangle, radii: [f32; 4]) -> canvas::Path {
        // Transform every path point using the same BackGeometry as hits.
        let hw = (self.model.card.size[0] * 0.5).max(1.0);
        let hh = (self.model.card.size[1] * 0.5).max(1.0);
        let sx = self.model.card.skew_distance * 0.5;
        let ty = self.model.card.edge_tilt_distance * 0.5;
        let a = (hw - sx.abs()).max(1.0) / hw;
        let b = -ty / hw;
        let c = -sx / hh;
        let d = (hh - ty.abs()).max(1.0) / hh;
        let (s, co) = self.model.card.rotation.sin_cos();
        let (a, b, c, d) = (
            co * a - s * b,
            s * a + co * b,
            co * c - s * d,
            s * c + co * d,
        );
        let [cx, cy] = self.model.card.center;
        let transform = canvas::path::lyon_path::math::Transform::new(
            a,
            b,
            c,
            d,
            cx - cx * a - cy * c,
            cy - cx * b - cy * d,
        );
        if self.model.depth.is_some() {
            let squeeze = (hw - sx.abs()).max(1.0) / hw;
            let sheared = Rectangle::new(
                Point::new(cx + (r.x - cx) * squeeze, r.y),
                iced::Size::new(r.width * squeeze, r.height),
            );
            let transform = canvas::path::lyon_path::math::Transform::new(
                co,
                s,
                co * c - s,
                s * c + co,
                cx - cx * co - cy * (co * c - s),
                cy - cx * s - cy * (s * c + co),
            );
            return canvas::Path::rounded_rectangle(
                sheared.position(),
                sheared.size(),
                iced::border::Radius {
                    top_left: radii[0],
                    top_right: radii[1],
                    bottom_right: radii[2],
                    bottom_left: radii[3],
                },
            )
            .transform(&transform);
        }
        canvas::Path::rounded_rectangle(
            r.position(),
            r.size(),
            iced::border::Radius {
                top_left: radii[0],
                top_right: radii[1],
                bottom_right: radii[2],
                bottom_left: radii[3],
            },
        )
        .transform(&transform)
    }
    fn line(&self, frame: &mut canvas::Frame, from: Point, to: Point, color: Color) {
        let a = self.model.card.point([from.x, from.y]);
        let b = self.model.card.point([to.x, to.y]);
        frame.stroke(
            &canvas::Path::line(Point::new(a[0], a[1]), Point::new(b[0], b[1])),
            canvas::Stroke::default()
                .with_color(color)
                .with_width(self.model.scale),
        );
    }
    fn text(
        &self,
        frame: &mut canvas::Frame,
        value: &str,
        position: Point,
        size: f32,
        width: f32,
        color: Color,
        center: bool,
        icon: bool,
    ) {
        if width <= 0.0 || color.a <= 0.01 {
            return;
        }
        let size = size * self.model.scale;
        let p = self.model.card.point([position.x, position.y]);
        let axis = self.model.card.point([position.x + 1.0, position.y]);
        let angle = (axis[1] - p[1]).atan2(axis[0] - p[0]);
        frame.with_save(|frame| {
            frame.translate(Vector::new(p[0], p[1]));
            frame.rotate(angle);
            frame.fill_text(canvas::Text {
                content: self.rendered_text(value, size, width, icon),
                position: Point::ORIGIN,
                size: size.into(),
                shaping: iced::advanced::text::Shaping::Advanced,
                line_height: iced::advanced::text::LineHeight::Relative(1.25),
                color,
                font: if icon {
                    self.model.icon_font
                } else {
                    self.model.font
                },
                align_x: if center {
                    text::Alignment::Center
                } else {
                    text::Alignment::Left
                },
                align_y: alignment::Vertical::Center,
                ..Default::default()
            });
        });
    }

    fn title_clip(&self, l: &BackLayout, dy: f32) -> Rectangle {
        donor_back_layout::title_clip(
            self.model.card,
            Rectangle {
                y: l.title.y + dy,
                ..l.title
            },
            l.title_line_height * l.content_scale * l.title_lines.len().max(1) as f32,
        )
    }
    fn draw_title(&self, frame: &mut canvas::Frame, l: &BackLayout, dy: f32, color: Color) {
        let clip = self.title_clip(l, dy);
        let height = l.title_line_height * l.content_scale;
        let block_height = height * l.title_lines.len() as f32;
        let width = donor_back_layout::title_glyph_width(self.model.card, clip, block_height);
        let origin = self.model.card.point(self.model.card.center);
        let axis = self
            .model
            .card
            .point([self.model.card.center[0] + 1.0, self.model.card.center[1]]);
        let angle = (axis[1] - origin[1]).atan2(axis[0] - origin[0]);
        let (sin, cos) = angle.sin_cos();
        frame.with_clip(clip, |frame| {
            for (i, line) in l.title_lines.iter().enumerate() {
                let y = (i as f32 + 0.5) * height - block_height * 0.5;
                let point = [
                    clip.center_x() - cos * width * 0.5 - sin * y,
                    clip.center_y() - sin * width * 0.5 + cos * y,
                ];
                if let Some(point) = self.model.card.inverse_point(point) {
                    self.text(
                        frame,
                        line,
                        Point::new(point[0], point[1]),
                        l.title_size,
                        l.title.width,
                        color,
                        false,
                        false,
                    );
                }
            }
        });
    }

    fn rendered_text(&self, value: &str, size: f32, width: f32, icon: bool) -> String {
        if icon {
            return value.to_owned();
        }
        // Depth shears positions but deliberately keeps glyphs at readable
        // logical-pixel size. Their budget must therefore follow the physical
        // horizontal axis, rather than the larger pre-transform rectangle.
        let width = if self.model.depth.is_some() {
            let origin = self.model.card.point(self.model.card.center);
            let axis = self
                .model
                .card
                .point([self.model.card.center[0] + 1.0, self.model.card.center[1]]);
            width * (axis[0] - origin[0]).hypot(axis[1] - origin[1])
        } else {
            width
        };
        donor_back_layout::measured_lines(value, size, width, self.model.font, 1)
            .pop()
            .unwrap_or_default()
    }
}

impl canvas::Program<DetailIntent> for BackProgram {
    type State = BackState;
    fn update(
        &self,
        state: &mut BackState,
        event: &canvas::Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<canvas::Action<DetailIntent>> {
        let l = self.layout();
        if !self.model.input_active {
            state.focus = None;
            state.pressed = None;
            if self.model.depth.is_some()
                && !self.model.embedded
                && matches!(event, canvas::Event::Mouse(_))
            {
                return Some(canvas::Action::request_redraw().and_capture());
            }
            return None;
        }
        if state.identity != self.model.identity {
            *state = BackState {
                identity: self.model.identity.clone(),
                ..Default::default()
            };
        }
        // Hover-only revisions may keep the same shape. A changed card/radius
        // cancels any old press; the release callback still carries the current
        // frame revision supplied by the app composition.
        let geometry = self.model.depth_revision.map(|_| {
            (
                self.model.card,
                self.model.radius,
                self.model.embedded,
                l.actions.clone(),
            )
        });
        let cancelled_press = if state.depth_geometry != geometry {
            state.depth_geometry = geometry;
            state.pressed.take().is_some()
        } else {
            false
        };
        if cancelled_press
            && matches!(
                event,
                canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
            )
        {
            return Some(canvas::Action::request_redraw().and_capture());
        }
        match event {
            canvas::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let p = cursor.position_in(bounds)?;
                state.pressed = self.hit_at(&l, p, state.scroll);
                if state.pressed.is_none() && !self.contains(l.card, p) && !self.model.embedded {
                    state.pressed = Some(DetailIntent::Close);
                }
                if state.pressed.is_some() || self.contains(l.card, p) || !self.model.embedded {
                    return Some(canvas::Action::request_redraw().and_capture());
                }
            }
            canvas::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                let pressed = state.pressed.take()?;
                let p = cursor.position_in(bounds)?;
                let hit = self.hit_at(&l, p, state.scroll).or_else(|| {
                    (!self.model.embedded && !self.contains(l.card, p))
                        .then_some(DetailIntent::Close)
                });
                if hit == Some(pressed) {
                    return Some(canvas::Action::publish(pressed).and_capture());
                }
                return Some(canvas::Action::request_redraw().and_capture());
            }
            canvas::Event::Mouse(mouse::Event::WheelScrolled { delta })
                if !self.model.embedded
                    || cursor
                        .position_in(bounds)
                        .is_some_and(|p| self.contains(l.card, p)) =>
            {
                state.focus = None;
                state.pressed = None;
                state.scroll = match delta {
                    mouse::ScrollDelta::Lines { y, .. } if *y != 0.0 => {
                        self.scroll_step(&l, state.scroll, *y < 0.0)
                    }
                    mouse::ScrollDelta::Pixels { y, .. } => {
                        (state.scroll - *y).clamp(0.0, self.scroll_limit(&l))
                    }
                    _ => state.scroll,
                };
                // Never scroll the gallery underneath the open detail.
                return Some(canvas::Action::request_redraw().and_capture());
            }
            canvas::Event::Keyboard(keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Tab),
                modifiers,
                ..
            }) if self.phases().1 > 0.9 => {
                let region = self.content_region(&l);
                let tag = self.tag_button(&l, 0.0);
                let tag_scroll =
                    (tag.center_y() - region.center_y()).clamp(0.0, self.scroll_limit(&l));
                let len = self.controls(&l, tag_scroll).len();
                state.focus = match state.focus {
                    None => Some(if modifiers.shift() { len - 1 } else { 0 }),
                    Some(0) if modifiers.shift() => None,
                    Some(i) if modifiers.shift() => Some(i - 1),
                    Some(i) if i + 1 == len => None,
                    Some(i) => Some(i + 1),
                };
                if state.focus == Some(self.model.actions.len() + 2) {
                    state.scroll = tag_scroll;
                }
                if state.focus.is_none() {
                    return None;
                }
                return Some(canvas::Action::request_redraw().and_capture());
            }
            canvas::Event::Keyboard(keyboard::Event::KeyPressed { key, .. })
                if state.focus.is_some()
                    && matches!(
                        key,
                        keyboard::Key::Named(
                            keyboard::key::Named::Enter | keyboard::key::Named::Space
                        )
                    ) =>
            {
                let controls = self.controls(&l, state.scroll);
                if let Some((_, intent, true)) = controls.get(state.focus.unwrap()) {
                    return Some(canvas::Action::publish(*intent).and_capture());
                }
                return Some(canvas::Action::request_redraw().and_capture());
            }
            _ => {}
        }
        None
    }
    fn draw(
        &self,
        state: &BackState,
        renderer: &iced::Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let _timing = crate::apply_trace::cpu_span("detail-draw", || self.model.identity.clone());
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        let l = self.layout();
        let s = l.content_scale;
        let (surface, p) = self.phases();
        if surface <= 0.01 {
            return vec![frame.into_geometry()];
        }
        let palette = self.model.palette;
        let inset = (l.sheet.x - l.card.x).max(0.0);
        let radius = (self.model.radius - inset).max(0.0);
        for (r, radii, opacity) in [
            (l.masthead, [radius, radius, 0.0, 0.0], 0.78),
            (l.sheet, [0.0, 0.0, radius, radius], 0.92),
        ] {
            let path = self.path(r, radii);
            frame.fill(
                &path,
                self.alpha(
                    if self.model.embedded {
                        palette.surface
                    } else {
                        palette.background
                    },
                    opacity * surface,
                ),
            );
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(self.alpha(palette.outline, 0.58 * surface))
                    .with_width(s),
            );
        }
        let fade = smoothstep(p);
        let (amount, dy) = rise(p, 0.0);
        self.text(
            &mut frame,
            &format!("Library / {}", self.model.kind),
            Point::new(l.title.x, l.masthead.center_y() + dy * s),
            10.0,
            (l.close.x - l.title.x - 8.0 * s).max(0.0),
            self.alpha(palette.primary, fade * amount),
            false,
            false,
        );
        let (_, dy) = rise(p, 0.65);
        self.draw_title(
            &mut frame,
            &l,
            dy * s,
            self.alpha(palette.surface_text, fade * rise(p, 0.65).0),
        );
        for (i, ((label, value), r)) in self.model.fields.iter().zip(&l.facts).enumerate() {
            let (a, dy) = rise(p, 1.15 + i as f32 * 0.18);
            let (label_bounds, value_bounds) = l.fact_text_bounds(*r);
            let scroll = if l.wide { 0.0 } else { state.scroll };
            let label_y = label_bounds.center_y() + dy * s - scroll;
            let value_y = value_bounds.center_y() + dy * s - scroll;
            if self.line_visible(&l, label_y, label_bounds.height) {
                self.text(
                    &mut frame,
                    &label.to_uppercase(),
                    Point::new(r.x, label_y),
                    l.label_size,
                    r.width,
                    self.alpha(palette.primary, 0.72 * fade * a),
                    false,
                    false,
                );
            }
            if self.line_visible(&l, value_y, value_bounds.height) {
                self.text(
                    &mut frame,
                    value,
                    Point::new(r.x, value_y),
                    l.value_size,
                    r.width,
                    self.alpha(palette.surface_text, 0.9 * fade * a),
                    false,
                    false,
                );
            }
        }
        if l.wide || l.rule_y < l.actions_label.y - 6.0 * s {
            self.line(
                &mut frame,
                Point::new(l.title.x, l.rule_y),
                Point::new(l.sheet.x + l.sheet.width - inset - 12.0 * s, l.rule_y),
                self.alpha(palette.outline, 0.34 * fade * rise(p, 1.55).0),
            );
        }
        let (source_label, source_value) = l.source_text_bounds();
        let scroll = if l.wide { 0.0 } else { state.scroll };
        for (value, r, size, opacity) in [
            ("TAGS", source_label, l.label_size, 0.52),
            (
                self.model.tags.as_str(),
                Rectangle {
                    x: source_value.x + self.tag_button(&l, scroll).width + 8.0 * s,
                    width: (source_value.width - self.tag_button(&l, scroll).width - 8.0 * s)
                        .max(0.0),
                    ..source_value
                },
                l.source_size,
                0.84,
            ),
        ] {
            let y = r.center_y() - scroll;
            if (l.wide && l.source.height >= 24.0 * s)
                || (!l.wide && self.line_visible(&l, y, r.height))
            {
                self.text(
                    &mut frame,
                    value,
                    Point::new(r.x, y),
                    size,
                    r.width,
                    self.alpha(palette.surface_text, opacity * fade),
                    false,
                    false,
                );
            }
        }
        self.text(
            &mut frame,
            "ACTIONS",
            l.actions_label,
            l.label_size,
            74.0 * s,
            self.alpha(palette.surface_text, 0.52 * fade * rise(p, 3.0).0),
            false,
            false,
        );
        if self.scroll_limit(&l) > 0.0 {
            self.text(
                &mut frame,
                "↑↓ Scroll details",
                Point::new(l.actions_label.x + 74.0 * s, l.actions_label.y),
                8.5,
                (l.sheet.width - 100.0 * s).max(0.0),
                self.alpha(palette.surface_text, 0.52 * fade),
                false,
                false,
            );
        }
        let controls = self.controls(&l, state.scroll);
        let hovered = cursor
            .position_in(bounds)
            .and_then(|p| self.hit_at(&l, p, state.scroll));
        for (i, (r, intent, enabled)) in controls.iter().enumerate() {
            let slot = if i < self.model.actions.len() {
                3.35 + i as f32 * 0.15
            } else {
                0.0
            };
            let (a, _) = rise(p, slot);
            let opacity = fade * a * if *enabled { 1.0 } else { 0.45 };
            let primary = *intent == DetailIntent::Apply;
            let active = *intent == DetailIntent::Favorite && self.model.favorite;
            let selected = state.focus == Some(i) || hovered == Some(*intent);
            let path = self.path(*r, [0.0; 4]);
            frame.fill(
                &path,
                self.alpha(
                    if primary || active {
                        palette.primary
                    } else {
                        palette.background
                    },
                    if primary || active {
                        0.94 * opacity
                    } else {
                        0.58 * opacity
                    },
                ),
            );
            frame.stroke(
                &path,
                canvas::Stroke::default()
                    .with_color(self.alpha(
                        palette.primary,
                        if selected { opacity } else { 0.42 * opacity },
                    ))
                    .with_width(if selected { 2.0 * s } else { s }),
            );
            if !primary && !active {
                self.line(
                    &mut frame,
                    Point::new(r.x, r.y + r.height - 2.0 * s),
                    Point::new(r.x + r.width * a, r.y + r.height - 2.0 * s),
                    self.alpha(palette.primary, 0.72 * opacity),
                );
            }
            let label = if i < self.model.actions.len() {
                self.model.actions[i].label.as_str()
            } else if *intent == DetailIntent::Favorite {
                if self.model.favorite {
                    "\u{f02d1}"
                } else {
                    "\u{f02d5}"
                }
            } else if *intent == DetailIntent::LocalTags {
                "+ Add tag"
            } else {
                "×"
            };
            self.text(
                &mut frame,
                label,
                r.center(),
                if *intent == DetailIntent::Favorite {
                    18.0
                } else {
                    l.action_size
                },
                r.width - 12.0 * s,
                self.alpha(
                    if primary || active {
                        palette.primary_text
                    } else {
                        palette.surface_text
                    },
                    opacity,
                ),
                true,
                *intent == DetailIntent::Favorite,
            );
        }
        vec![frame.into_geometry()]
    }
    fn mouse_interaction(
        &self,
        state: &BackState,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> mouse::Interaction {
        if cursor
            .position_in(bounds)
            .and_then(|p| self.hit_at(&self.layout(), p, state.scroll))
            .is_some()
        {
            mouse::Interaction::Pointer
        } else {
            mouse::Interaction::default()
        }
    }
}

#[cfg(test)]
#[path = "donor_back_view_tests.rs"]
mod tests;

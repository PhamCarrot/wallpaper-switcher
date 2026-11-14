use super::*;
use iced::advanced::{
    clipboard, layout,
    renderer::{Headless, Renderer as _},
    widget::{operation, Operation, Tree},
    Layout, Shell,
};

pub(super) struct Scene<'a> {
    element: Element<'a, Message>,
    tree: Tree,
    node: layout::Node,
    bounds: iced::Rectangle,
    cursor: iced::mouse::Cursor,
}

#[derive(Default)]
pub(super) struct Probe {
    offset: iced::Vector,
    pending: Option<iced::Vector>,
    pub(super) texts: Vec<(String, iced::Rectangle)>,
    pub(super) inputs: Vec<(Option<iced::widget::Id>, iced::Rectangle)>,
    containers: Vec<(Option<iced::widget::Id>, iced::Rectangle)>,
    pub(super) scrolls: Vec<(
        Option<iced::widget::Id>,
        iced::Rectangle,
        iced::Rectangle,
        iced::Vector,
    )>,
}

impl Operation for Probe {
    fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
        let old = self.offset;
        if let Some(delta) = self.pending.take() {
            self.offset += delta;
        }
        visit(self);
        self.offset = old;
    }

    fn text(&mut self, _: Option<&iced::widget::Id>, bounds: iced::Rectangle, value: &str) {
        self.texts.push((value.into(), bounds - self.offset));
    }

    fn text_input(
        &mut self,
        id: Option<&iced::widget::Id>,
        bounds: iced::Rectangle,
        _: &mut dyn operation::TextInput,
    ) {
        self.inputs.push((id.cloned(), bounds - self.offset));
    }

    fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
        self.containers.push((id.cloned(), bounds - self.offset));
    }

    fn scrollable(
        &mut self,
        id: Option<&iced::widget::Id>,
        bounds: iced::Rectangle,
        content: iced::Rectangle,
        translation: iced::Vector,
        _: &mut dyn operation::Scrollable,
    ) {
        self.scrolls
            .push((id.cloned(), bounds - self.offset, content, translation));
        self.pending = Some(translation);
    }
}

impl<'a> Scene<'a> {
    pub(super) fn new(app: &'a GpuCandidate, renderer: &iced::Renderer) -> Self {
        let size = iced::Size::new(app.viewport[0], app.viewport[1]);
        let mut element = view(app, app.window.unwrap());
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, renderer, &layout::Limits::new(size, size));
        Self {
            element,
            tree,
            node,
            bounds: iced::Rectangle::with_size(size),
            cursor: iced::mouse::Cursor::Unavailable,
        }
    }

    fn operate(&mut self, renderer: &iced::Renderer, operation: &mut dyn Operation) {
        self.element.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&self.node),
            renderer,
            operation,
        );
    }

    pub(super) fn probe(&mut self, renderer: &iced::Renderer) -> Probe {
        let mut probe = Probe::default();
        self.operate(renderer, &mut probe);
        probe
    }

    pub(super) fn event(
        &mut self,
        renderer: &iced::Renderer,
        cursor: iced::mouse::Cursor,
        event: iced::Event,
    ) -> Vec<Message> {
        self.cursor = cursor;
        let mut messages = Vec::new();
        self.element.as_widget_mut().update(
            &mut self.tree,
            &event,
            Layout::new(&self.node),
            cursor,
            renderer,
            &mut clipboard::Null,
            &mut Shell::new(&mut messages),
            &self.bounds,
        );
        messages
    }

    pub(super) fn click(
        &mut self,
        renderer: &iced::Renderer,
        point: iced::Point,
        button: iced::mouse::Button,
    ) -> Vec<Message> {
        [
            iced::mouse::Event::ButtonPressed(button),
            iced::mouse::Event::ButtonReleased(button),
        ]
        .into_iter()
        .flat_map(|event| {
            self.event(
                renderer,
                iced::mouse::Cursor::Available(point),
                iced::Event::Mouse(event),
            )
        })
        .collect()
    }

    pub(super) fn wheel(&mut self, renderer: &iced::Renderer, point: iced::Point, x: f32, y: f32) {
        let _ = self.event(
            renderer,
            iced::mouse::Cursor::Available(point),
            iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                delta: iced::mouse::ScrollDelta::Pixels { x, y },
            }),
        );
    }

    pub(super) fn hit_label(
        &mut self,
        renderer: &iced::Renderer,
        label: &str,
        accepts: impl Fn(&Message) -> bool,
    ) -> Message {
        for _ in 0..16 {
            let probe = self.probe(renderer);
            let mut scroll = None;
            for (_, bounds) in probe.texts.iter().rev().filter(|(text, _)| text == label) {
                let point = bounds.center();
                if self.bounds.contains(point) {
                    if let Some(message) = self
                        .click(renderer, point, iced::mouse::Button::Left)
                        .into_iter()
                        .find(&accepts)
                    {
                        return message;
                    }
                }
                if let Some((_, pane, _, _)) =
                    probe.scrolls.iter().rev().find(|(_, pane, content, _)| {
                        content.height > pane.height + 1.0
                            && self.bounds.contains(pane.center())
                            && (pane.x..pane.x + pane.width).contains(&point.x)
                            && (point.y < pane.y || point.y > pane.y + pane.height)
                    })
                {
                    scroll = Some((
                        pane.center(),
                        if point.y < pane.y { 1.0 } else { -1.0 } * pane.height * 0.55,
                    ));
                }
            }
            let Some((point, delta)) = scroll else {
                panic!(
                    "No mounted hit for {label}; matching labels: {:?}",
                    probe
                        .texts
                        .iter()
                        .filter(|(t, _)| t == label)
                        .collect::<Vec<_>>()
                );
            };
            self.wheel(renderer, point, 0.0, delta);
        }
        panic!("No mounted hit for {label} after scrolling");
    }

    pub(super) fn save(
        &mut self,
        renderer: &mut iced::Renderer,
        app: &GpuCandidate,
        path: &std::path::Path,
    ) {
        let now = iced::time::Instant::now();
        for offset in [0, 1_000] {
            let _ = self.event(
                renderer,
                self.cursor,
                iced::Event::Window(window::Event::RedrawRequested(
                    now + std::time::Duration::from_millis(offset),
                )),
            );
        }
        let theme = theme(app, app.window.unwrap());
        let style = application_style(app, &theme);
        let [width, height] = [self.bounds.width as u32, self.bounds.height as u32];
        // The image worker needs submitted frames to finish loading and uploading.
        let mut bytes = Vec::new();
        for frame in 0..4 {
            renderer.reset(self.bounds);
            self.element.as_widget().draw(
                &self.tree,
                renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: style.text_color,
                },
                Layout::new(&self.node),
                self.cursor,
                &self.bounds,
            );
            if let Some(overlay) = self.element.as_widget_mut().overlay(
                &mut self.tree,
                Layout::new(&self.node),
                renderer,
                &self.bounds,
                iced::Vector::ZERO,
            ) {
                let mut overlay = iced::advanced::overlay::Nested::new(overlay);
                let node = overlay.layout(renderer, self.bounds.size());
                overlay.draw(
                    renderer,
                    &theme,
                    &iced::advanced::renderer::Style {
                        text_color: style.text_color,
                    },
                    Layout::new(&node),
                    self.cursor,
                );
            }
            bytes = Headless::screenshot(
                renderer,
                iced::Size::new(width, height),
                1.0,
                style.background_color,
            );
            if frame < 3 {
                std::thread::sleep(std::time::Duration::from_millis(40));
            }
        }
        ::image::save_buffer_with_format(
            path,
            &bytes,
            width,
            height,
            ::image::ColorType::Rgba8,
            ::image::ImageFormat::Png,
        )
        .unwrap();
    }
}

pub(super) fn renderer() -> iced::Renderer {
    let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .unwrap();
    let mut fonts = iced::advanced::graphics::text::font_system()
        .write()
        .unwrap();
    for language in ["en-US", "zh-CN", "ja-JP"] {
        fonts.load_font(std::borrow::Cow::Borrowed(
            localization::Catalog::new(language, language)
                .unwrap()
                .font_bytes(),
        ));
    }
    fonts.load_font(std::borrow::Cow::Borrowed(
        include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
    ));
    renderer
}

fn fixture() -> (GpuCandidate, Arc<std::sync::atomic::AtomicUsize>) {
    let (mut app, calls) = d3_application_fixture(32);
    app.launch_policy = p1_preview_policy();
    app.gallery_mode = GalleryMode::Slices;
    app.catalog_loaded = true;
    app.state.playlists = (0..28)
        .map(|n| Playlist {
            id: format!("saved-{n:02}"),
            name: format!("Saved playlist {n:02}"),
            order: PlaybackOrder::Sequential,
            membership: PlaylistMembership::Explicit {
                wallpaper_ids: app.catalog.iter().map(|e| e.id.clone()).collect(),
            },
        })
        .collect();
    app.state.active_playlist_id = None;
    app.state.slideshow_enabled = false;
    app.state.automation_intent = Some(candidate_state::AutomationIntent::default());
    app.state.schedules.clear();
    app.state.save(&app.state_path).unwrap();
    (app, calls)
}

fn fits(outer: iced::Rectangle, inner: iced::Rectangle) -> bool {
    inner.x >= outer.x - 0.5
        && inner.y >= outer.y - 0.5
        && inner.x + inner.width <= outer.x + outer.width + 0.5
        && inner.y + inner.height <= outer.y + outer.height + 0.5
}
fn has_laid_out_text(tree: &Tree, expected: &str) -> bool {
    type Paragraph = iced::advanced::text::paragraph::Plain<
        <iced::Renderer as iced::advanced::text::Renderer>::Paragraph,
    >;
    matches!(&tree.state, iced::advanced::widget::tree::State::Some(state)
        if state.downcast_ref::<Paragraph>().is_some_and(|p| p.content() == expected))
        || tree
            .children
            .iter()
            .any(|child| has_laid_out_text(child, expected))
}
fn overlay_leaves_fit(layout: Layout<'_>, viewport: iced::Rectangle) -> bool {
    let children: Vec<_> = layout.children().collect();
    if children.is_empty() {
        fits(viewport, layout.bounds())
    } else {
        children
            .into_iter()
            .all(|child| overlay_leaves_fit(child, viewport))
    }
}
fn region(probe: &Probe, name: &'static str) -> iced::Rectangle {
    probe
        .containers
        .iter()
        .find(|(id, _)| *id == Some(iced::widget::Id::new(name)))
        .unwrap_or_else(|| panic!("Missing region {name}"))
        .1
}
fn scroll(probe: &Probe, name: &'static str) -> (iced::Rectangle, iced::Rectangle, iced::Vector) {
    let (_, pane, content, offset) = probe
        .scrolls
        .iter()
        .find(|(id, ..)| *id == Some(iced::widget::Id::new(name)))
        .unwrap_or_else(|| panic!("Missing scroll {name}"));
    (*pane, *content, *offset)
}
fn rail_open(app: &GpuCandidate, renderer: &iced::Renderer) -> Message {
    let intent = donor_rail::RailIntent::TogglePlaylists;
    let model = gallery_rail_model(app);
    let at = view::gallery_rail_anchor(app, &model, app.state.gallery.slices.height);
    let center = donor_rail::intent_center(&model, intent).unwrap();
    Scene::new(app, renderer)
        .click(
            renderer,
            iced::Point::new(at[0] + center.x, at[1] + center.y),
            iced::mouse::Button::Left,
        )
        .into_iter()
        .find(|m| matches!(m,Message::DonorRail(hit) if *hit==intent))
        .unwrap()
}
fn open(app: &mut GpuCandidate, renderer: &iced::Renderer) {
    let message = rail_open(app, renderer);
    let _ = update(app, message);
    assert!(app.playlist_chooser);
    let message = Scene::new(app, renderer).hit_label(
        renderer,
        app.settings_locale.text_static("playlists-edit"),
        |m| matches!(m, Message::PlaylistBrowse(_, playlists::Browse::Edit)),
    );
    let _ = update(app, message);
    assert!(app.playlist_panel_open && !app.playlist_chooser && !app.playlist_picker);
}
fn full_action(
    scene: &mut Scene<'_>,
    renderer: &iced::Renderer,
    id: &'static str,
) -> iced::Rectangle {
    for _ in 0..200 {
        let probe = scene.probe(renderer);
        let bounds = region(&probe, id);
        let pane = scroll(&probe, "playlist-definition").0;
        if fits(pane, bounds) {
            assert!(
                bounds.x + bounds.width <= pane.x + pane.width - 11.5,
                "Action overlaps track: {id} {pane:?} {bounds:?}"
            );
            return bounds;
        }
        let delta = if bounds.y < pane.y { 1.0 } else { -1.0 };
        scene.wheel(renderer, pane.center(), 0.0, delta * pane.height * 0.25);
    }
    panic!("Full action never reachable: {id}");
}

#[test]
#[ignore = "PK-07 production manager frames/actions/independent scrolling; explicit headless invocation"]
fn pk07_production_manager_composition_and_actions() {
    let output =
        std::path::PathBuf::from(std::env::var_os("P7_LAYOUT_OUTPUT").expect("owned output"));
    std::fs::create_dir_all(&output).unwrap();
    let mut renderer = renderer();
    let media = output.parent().unwrap().join("candidate/owned-media");
    let files = [
        "FHD-lower.png",
        "FHD-upper.png",
        "QHD.png",
        "4K.png",
        "Square.png",
        "Tall-FHD.png",
        "Below-FHD.png",
        "8K.png",
    ];
    // Hold actual renderer allocations for the owned fixture, avoiding transient
    // blank asynchronous first-demand images in static composition evidence.
    let _allocations: Vec<_> = files
        .iter()
        .map(|file| {
            let path = media.join(file);
            assert!(path.is_file(), "owned fixture missing: {path:?}");
            iced::advanced::image::Renderer::load_image(
                &renderer,
                &iced::widget::image::Handle::from_path(path),
            )
            .unwrap()
        })
        .collect();
    let mut receipts = Vec::new();
    for (width, height, scale, locale) in [
        (1920.0, 1080.0, 1.0, "en-US"),
        (2560.0, 1440.0, 1.0, "en-US"),
        (1280.0, 720.0, 0.5, "en-US"),
        (640.0, 600.0, 1.0, "zh-CN"),
        (640.0, 600.0, 0.5, "zh-CN"),
    ] {
        for case in [
            "manual",
            "smart",
            "empty",
            "no-lists",
            "active",
            "safe-stop",
            "long-cjk",
            "no-wallpaper",
        ] {
            let (mut app, calls) = fixture();
            app.viewport = [width, height];
            for (n, wallpaper) in app.wallpapers.iter_mut().enumerate() {
                let path = media.join(files[n % files.len()]);
                assert!(path.is_file(), "owned fixture missing: {path:?}");
                wallpaper.preview_path = Some(path);
            }
            app.state.frontend_preferences.interface.ui_scale = Some(scale);
            app.settings_locale = localization::Catalog::new(locale, locale).unwrap();
            if case == "smart" {
                app.state.playlists[0].membership = PlaylistMembership::Smart {
                    rule: SmartRule::default(),
                };
            }
            if case == "empty" {
                app.state.playlists[0].membership = PlaylistMembership::Explicit {
                    wallpaper_ids: Vec::new(),
                };
            }
            if case == "no-lists" {
                app.state.playlists.clear();
            }
            if case == "long-cjk" {
                app.state.playlists[0].name = "星空のとても長いプレイリスト・合成é壁紙".repeat(8);
                app.state.playlists[0].id = "安定した長い識別子".repeat(4);
                app.catalog[31].title = "最後の長い壁紙タイトル・海と星空".repeat(8);
                app.wallpapers[31].title = app.catalog[31].title.clone();
            }
            if case == "no-wallpaper" {
                app.selected = None;
            }
            if case == "active" {
                app.state.active_playlist_id = Some(app.state.playlists[0].id.clone());
                app.state.automation_intent = Some(candidate_state::AutomationIntent {
                    random_enabled: true,
                    playlist_playing: true,
                    ..Default::default()
                });
            }
            if case == "safe-stop" {
                app.state.active_playlist_id = Some(app.state.playlists[0].id.clone());
                app.state.automation_intent = Some(candidate_state::AutomationIntent {
                    random_enabled: true,
                    safely_stopped: true,
                    ..Default::default()
                });
            }
            app.playlist_name = "New saved list 音楽".into();
            app.state.save(&app.state_path).unwrap();
            let disk_before = std::fs::read(&app.state_path).unwrap();
            let intent_before = app.state.automation_intent();
            open(&mut app, &renderer);
            // Opening/reconciliation owns and resets the create draft.
            let _ = update(
                &mut app,
                Message::PlaylistNameChanged("New saved list 音楽".into()),
            );
            assert_eq!(std::fs::read(&app.state_path).unwrap(), disk_before);
            assert_eq!(app.state.automation_intent(), intent_before);
            let stem = format!("manager-{width}x{height}-{scale}-{locale}-{case}");
            let mut scene = Scene::new(&app, &renderer);
            let probe = scene.probe(&renderer);
            let index = region(&probe, "playlist-index-region");
            assert!(fits(scene.bounds, index));
            for id in [
                "playlist-create-manual-action",
                "playlist-create-smart-action",
            ] {
                assert!(fits(index, region(&probe, id)), "{stem} fixed create {id}");
            }
            let close = probe.texts.iter().find(|(t, _)| t == "×").unwrap().1;
            assert!(fits(scene.bounds, close));
            scene.save(&mut renderer, &app, &output.join(format!("{stem}-top.png")));
            if case != "no-lists" {
                let details = region(&probe, "playlist-details-region");
                let sequence = region(&probe, "playlist-sequence-region");
                assert!(fits(scene.bounds, details) && fits(scene.bounds, sequence));
                assert!(
                    details.x + details.width <= sequence.x + 0.5
                        || details.y + details.height <= sequence.y + 0.5
                );
                assert!(!probe
                    .scrolls
                    .iter()
                    .any(|(id, ..)| *id == Some(iced::widget::Id::new("playlist-reading"))));
                assert!(probe.texts.iter().any(|(t, _)| t.starts_with("01  ")));
                assert!(probe.texts.iter().any(|(t, _)| t.starts_with("02  ")));
                let policy_key = if case == "safe-stop" {
                    "playlist-manager-safe-stop"
                } else if case == "active" {
                    "playlist-manager-playing"
                } else {
                    "playlist-manager-browse"
                };
                assert!(probe
                    .texts
                    .iter()
                    .any(|(t, _)| t == app.settings_locale.text_static(policy_key)));
                assert!(
                    probe
                        .texts
                        .iter()
                        .any(|(t, _)| t
                            == app.settings_locale.text_static("playlist-manager-preview"))
                );
                assert!(probe
                    .texts
                    .iter()
                    .any(|(t, _)| t == app.random_rotation_state().label()));
                let title_bounds = region(&probe, "playlist-selected-title");
                assert!(fits(scroll(&probe, "playlist-definition").0, title_bounds));
                assert!(
                    title_bounds.height
                        <= 42.0 * (scale as f32).clamp(0.88, 1.08) * 0.98 * 2.0 + 1.0
                );
                if case == "long-cjk" {
                    let point = title_bounds.center();
                    let _ = scene.event(
                        &renderer,
                        iced::mouse::Cursor::Available(point),
                        iced::Event::Mouse(iced::mouse::Event::CursorMoved { position: point }),
                    );
                    let overlay = scene
                        .element
                        .as_widget_mut()
                        .overlay(
                            &mut scene.tree,
                            Layout::new(&scene.node),
                            &renderer,
                            &scene.bounds,
                            iced::Vector::ZERO,
                        )
                        .expect("mounted full-name tooltip");
                    let mut overlay = iced::advanced::overlay::Nested::new(overlay);
                    let node = overlay.layout(&renderer, scene.bounds.size());
                    assert!(overlay_leaves_fit(Layout::new(&node), scene.bounds));
                    drop(overlay);
                    // Iced's Tooltip overlay does not forward Operation; its laid-out
                    // paragraph retains the exact text actually used by draw.
                    assert!(
                        has_laid_out_text(&scene.tree, &app.state.playlists[0].name),
                        "exact raw full name in actual tooltip paragraph"
                    );
                    scene.save(
                        &mut renderer,
                        &app,
                        &output.join(format!("{stem}-full-name.png")),
                    );
                    let _ = scene.event(
                        &renderer,
                        iced::mouse::Cursor::Unavailable,
                        iced::Event::Mouse(iced::mouse::Event::CursorLeft),
                    );
                }
                let (seq, content, _) = scroll(&probe, "playlist-sequence");
                scene.wheel(&renderer, seq.center(), 0.0, -100000.0);
                let end = scene.probe(&renderer);
                assert_eq!(scroll(&end, "playlist-definition").2, iced::Vector::ZERO);
                assert_eq!(scroll(&end, "playlist-navigation").2, iced::Vector::ZERO);
                if case != "empty" {
                    assert!(scroll(&end, "playlist-sequence").2.y > 0.0);
                    assert!(content.height > seq.height);
                    let last = end
                        .texts
                        .iter()
                        .find(|(t, _)| t == &app.catalog[31].title)
                        .unwrap()
                        .1;
                    if !fits(seq, last) {
                        // The title and artwork together may exceed this pane. Check the
                        // full title at its own reachable position before returning to end.
                        scene.wheel(&renderer, seq.center(), 0.0, seq.y - last.y + 2.0);
                        let title_probe = scene.probe(&renderer);
                        let full_title = title_probe
                            .texts
                            .iter()
                            .find(|(t, _)| t == &app.catalog[31].title)
                            .unwrap()
                            .1;
                        assert!(
                            fits(seq, full_title),
                            "{stem} full last title: {seq:?} {full_title:?}"
                        );
                        scene.save(
                            &mut renderer,
                            &app,
                            &output.join(format!("{stem}-last-title.png")),
                        );
                        scene.wheel(&renderer, seq.center(), 0.0, -100000.0);
                    } else {
                        assert!(fits(seq, last));
                    }
                }
                scene.save(
                    &mut renderer,
                    &app,
                    &output.join(format!("{stem}-sequence-end.png")),
                );
                let seq_offset = scroll(&end, "playlist-sequence").2;
                let nav = scroll(&end, "playlist-navigation").0;
                scene.wheel(&renderer, nav.center(), 0.0, -100000.0);
                let nav_end = scene.probe(&renderer);
                assert!(scroll(&nav_end, "playlist-navigation").2.y > 0.0);
                assert_eq!(scroll(&nav_end, "playlist-sequence").2, seq_offset);
                assert_eq!(
                    scroll(&nav_end, "playlist-definition").2,
                    iced::Vector::ZERO
                );
                let last = nav_end
                    .texts
                    .iter()
                    .find(|(t, _)| t == "Saved playlist 27")
                    .unwrap()
                    .1;
                assert!(fits(nav, last) && last.x + last.width <= nav.x + nav.width - 11.5);
                scene.save(
                    &mut renderer,
                    &app,
                    &output.join(format!("{stem}-index-end.png")),
                );
                for id in [
                    "playlist-start-action",
                    "playlist-stop-action",
                    "playlist-delete-action",
                    "playlist-order-action",
                ] {
                    full_action(&mut scene, &renderer, id);
                }
                if app.selected.is_some() {
                    full_action(&mut scene, &renderer, "playlist-membership-action");
                }
                let order = full_action(&mut scene, &renderer, "playlist-order-action");
                let message = scene
                    .click(&renderer, order.center(), iced::mouse::Button::Left)
                    .into_iter()
                    .find(|m| matches!(m, Message::TogglePlaylistOrder(_)))
                    .expect("full order hit");
                scene.save(
                    &mut renderer,
                    &app,
                    &output.join(format!("{stem}-order-hit.png")),
                );
                receipts.push(format!(
                    "{stem}: {message:?}; sequence={seq:?}; details={details:?}; index={index:?}"
                ));
                drop(scene);
                let _ = update(&mut app, message);
                assert_eq!(app.state.playlists[0].order, PlaybackOrder::Shuffle);
                assert_eq!(
                    CandidateState::load(&app.state_path).playlists[0].order,
                    PlaybackOrder::Shuffle
                );
                assert_eq!(app.state.automation_intent(), intent_before);
            } else {
                drop(scene);
            }
            assert_eq!(calls.load(Ordering::Acquire), 0);
        }
    }
    live_actions(&output, &mut renderer, &mut receipts);
    std::fs::write(output.join("ACTION-READBACKS.txt"), receipts.join("\n")).unwrap();
}

fn live_actions(
    output: &std::path::Path,
    renderer: &mut iced::Renderer,
    receipts: &mut Vec<String>,
) {
    for (width, height, scale) in [(1920.0, 1080.0, 1.0), (640.0, 600.0, 0.5)] {
        let (mut app, calls) = fixture();
        app.viewport = [width, height];
        app.state.frontend_preferences.interface.ui_scale = Some(scale);
        app.state.automation_intent = Some(candidate_state::AutomationIntent {
            random_enabled: true,
            ..Default::default()
        });
        app.state.save(&app.state_path).unwrap();
        open(&mut app, renderer);
        let before = std::fs::read(&app.state_path).unwrap();
        let browse = Scene::new(&app, renderer).hit_label(
            renderer,
            "Saved playlist 01",
            |m| matches!(m,Message::BrowsePlaylist(id) if id=="saved-01"),
        );
        let _ = update(&mut app, browse);
        assert_eq!(app.playlist_browsing_id.as_deref(), Some("saved-01"));
        assert_eq!(std::fs::read(&app.state_path).unwrap(), before);
        let start = {
            let mut scene = Scene::new(&app, renderer);
            let bounds = full_action(&mut scene, renderer, "playlist-start-action");
            let message = scene
                .click(renderer, bounds.center(), iced::mouse::Button::Left)
                .into_iter()
                .find(|m| matches!(m,Message::SetActivePlaylist(id,_) if id=="saved-01"))
                .unwrap();
            scene.save(
                renderer,
                &app,
                &output.join(format!("actions-{width}-{scale}-start-hit.png")),
            );
            message
        };
        let _ = update(&mut app, start);
        assert!(app.playlist_playing() && app.state.automation_intent().random_enabled);
        assert_eq!(app.state.active_playlist_id.as_deref(), Some("saved-01"));
        let stop = {
            let mut scene = Scene::new(&app, renderer);
            let bounds = full_action(&mut scene, renderer, "playlist-stop-action");
            let message = scene
                .click(renderer, bounds.center(), iced::mouse::Button::Left)
                .into_iter()
                .find(|m| matches!(m, Message::StopPlaylist { .. }))
                .unwrap();
            scene.save(
                renderer,
                &app,
                &output.join(format!("actions-{width}-{scale}-stop-hit.png")),
            );
            message
        };
        let _ = update(&mut app, stop);
        assert!(!app.playlist_playing() && app.state.automation_intent().random_enabled);
        let selected_id = app.wallpapers[app.selected.unwrap()].id.clone();
        let membership = {
            let mut scene = Scene::new(&app, renderer);
            let bounds = full_action(&mut scene, renderer, "playlist-membership-action");
            let message = scene
                .click(renderer, bounds.center(), iced::mouse::Button::Left)
                .into_iter()
                .find(|m| matches!(m,Message::TogglePlaylistMembership(id) if id=="saved-01"))
                .unwrap();
            scene.save(
                renderer,
                &app,
                &output.join(format!("actions-{width}-{scale}-membership-hit.png")),
            );
            message
        };
        let _ = update(&mut app, membership);
        assert!(
            matches!(&CandidateState::load(&app.state_path).playlists[1].membership,
            PlaylistMembership::Explicit {wallpaper_ids} if !wallpaper_ids.contains(&selected_id))
        );
        for smart in [false, true] {
            let name = if smart {
                "New smart 音楽"
            } else {
                "New manual 音楽"
            };
            assert!(app.playlist_name.is_empty());
            let disk_before_draft = std::fs::read(&app.state_path).unwrap();
            let query = app.playlist_query.clone();
            let filter = app.filter_rule.clone();
            let browse = app.browse_playlist_id.clone();
            let editor = app.playlist_browsing_id.clone();
            let owner = app.playlist_owner();
            let draft = {
                let mut scene = Scene::new(&app, renderer);
                let probe = scene.probe(renderer);
                let index = region(&probe, "playlist-index-region");
                let input = probe
                    .inputs
                    .iter()
                    .find(|(_, bounds)| fits(index, *bounds))
                    .expect("mounted manager name input")
                    .1;
                assert!(scene
                    .click(renderer, input.center(), iced::mouse::Button::Left)
                    .is_empty());
                let mut draft = None;
                for (offset, ch) in name.char_indices() {
                    let key = ch.to_string();
                    let messages = scene.event(
                        renderer,
                        iced::mouse::Cursor::Unavailable,
                        iced::Event::Keyboard(keyboard::Event::KeyPressed {
                            key: keyboard::Key::Character(key.as_str().into()),
                            modified_key: keyboard::Key::Character(key.as_str().into()),
                            physical_key: keyboard::key::Physical::Unidentified(
                                keyboard::key::NativeCode::Unidentified,
                            ),
                            location: keyboard::Location::Standard,
                            modifiers: keyboard::Modifiers::empty(),
                            text: Some(key.as_str().into()),
                            repeat: false,
                        }),
                    );
                    let expected = &name[..offset + ch.len_utf8()];
                    assert!(
                        matches!(messages.as_slice(), [Message::PlaylistNameChanged(value)] if value == expected),
                        "mounted draft {expected:?}: {messages:?}"
                    );
                    draft = messages.into_iter().next();
                }
                draft.expect("mounted Unicode draft")
            };
            let _ = update(&mut app, draft);
            assert_eq!(app.playlist_name, name);
            assert_eq!(app.playlist_query, query);
            assert_eq!(app.filter_rule, filter);
            assert_eq!(app.browse_playlist_id, browse);
            assert_eq!(app.playlist_browsing_id, editor);
            assert_eq!(app.playlist_owner(), owner);
            assert_eq!(std::fs::read(&app.state_path).unwrap(), disk_before_draft);
            let create = {
                let mut scene = Scene::new(&app, renderer);
                let probe = scene.probe(renderer);
                let bounds = region(
                    &probe,
                    if smart {
                        "playlist-create-smart-action"
                    } else {
                        "playlist-create-manual-action"
                    },
                );
                assert!(fits(region(&probe, "playlist-index-region"), bounds));
                let message = scene
                    .click(renderer, bounds.center(), iced::mouse::Button::Left)
                    .into_iter()
                    .find(|m| matches!(m,Message::CreatePlaylist{smart:s} if *s==smart))
                    .unwrap();
                scene.save(
                    renderer,
                    &app,
                    &output.join(format!("actions-{width}-{scale}-create-{smart}-hit.png")),
                );
                message
            };
            let _ = update(&mut app, create);
            let created = app.state.playlists.last().unwrap();
            assert_eq!(created.name, name);
            receipts.push(format!(
                "{width}x{height} scale={scale} create smart={smart}: mounted draft {name:?} left query, browse, editor owner and disk bytes unchanged; saved ID {}",
                created.id
            ));
            assert_eq!(
                matches!(created.membership, PlaylistMembership::Smart { .. }),
                smart
            );
            assert_eq!(
                &CandidateState::load(&app.state_path).playlists,
                &app.state.playlists
            );
        }
        let delete_id = app.state.playlists.last().unwrap().id.clone();
        for confirm in [false, true] {
            let request = {
                let mut scene = Scene::new(&app, renderer);
                let bounds = full_action(&mut scene, renderer, "playlist-delete-action");
                scene.click(renderer,bounds.center(),iced::mouse::Button::Left)
                    .into_iter().find(|m|matches!(m,Message::PlaylistEdit(_,playlists::Edit::DeleteRequest(id)) if id==&delete_id)).unwrap()
            };
            let _ = update(&mut app, request);
            assert_eq!(app.playlist_delete.as_ref().unwrap().playlist.id, delete_id);
            let action = {
                let mut scene = Scene::new(&app, renderer);
                let key = if confirm {
                    "playlist-delete-action"
                } else {
                    "playlist-delete-cancel"
                };
                let message=scene.hit_label(renderer,app.settings_locale.text_static(key),|m|
                    matches!(m,Message::PlaylistEdit(_,playlists::Edit::DeleteConfirm) if confirm)
                    || matches!(m,Message::PlaylistEdit(_,playlists::Edit::DeleteCancel) if !confirm));
                let probe = scene.probe(renderer);
                let label = probe
                    .texts
                    .iter()
                    .rev()
                    .find(|(t, _)| t == app.settings_locale.text_static(key))
                    .unwrap()
                    .1;
                let action_scale = scale as f32;
                let full = iced::Rectangle {
                    x: label.x - 12.0 * action_scale,
                    y: label.y - 7.0 * action_scale,
                    width: label.width + 24.0 * action_scale,
                    height: label.height + 14.0 * action_scale,
                };
                assert!(fits(scene.bounds, full));
                scene.save(
                    renderer,
                    &app,
                    &output.join(format!("actions-{width}-{scale}-delete-{confirm}-hit.png")),
                );
                message
            };
            let _ = update(&mut app, action);
            assert!(app.playlist_delete.is_none());
            assert_eq!(
                app.state.playlists.iter().any(|p| p.id == delete_id),
                !confirm
            );
        }
        let close = Scene::new(&app, renderer).hit_label(renderer, "×", |m| {
            matches!(m, Message::TogglePlaylistPanel)
        });
        let _ = update(&mut app, close);
        assert!(!app.playlist_panel_open);
        assert_eq!(calls.load(Ordering::Acquire), 0);
    }
}

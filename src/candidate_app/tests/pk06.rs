use super::*;
use iced::advanced::{
    clipboard, layout,
    renderer::{Headless, Renderer as _},
    widget::{operation, Operation, Tree},
    Layout, Shell,
};
use view::settings::folio::intent::{Message as Intent, SettingsMsg};

struct Scene<'a> {
    element: Element<'a, Message>,
    tree: Tree,
    node: layout::Node,
    bounds: iced::Rectangle,
}

#[derive(Default)]
struct Probe {
    offset: iced::Vector,
    pending: Option<iced::Vector>,
    texts: Vec<(String, iced::Rectangle)>,
    inputs: Vec<(Option<iced::widget::Id>, iced::Rectangle)>,
    containers: Vec<(Option<iced::widget::Id>, iced::Rectangle)>,
    scrolls: Vec<(
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
    fn new(app: &'a GpuCandidate, renderer: &iced::Renderer) -> Self {
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

    fn probe(&mut self, renderer: &iced::Renderer) -> Probe {
        let mut probe = Probe::default();
        self.operate(renderer, &mut probe);
        probe
    }

    fn event(
        &mut self,
        renderer: &iced::Renderer,
        cursor: iced::mouse::Cursor,
        event: iced::Event,
    ) -> Vec<Message> {
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

    fn click(
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

    fn wheel(&mut self, renderer: &iced::Renderer, point: iced::Point, x: f32, y: f32) {
        let _ = self.event(
            renderer,
            iced::mouse::Cursor::Available(point),
            iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                delta: iced::mouse::ScrollDelta::Pixels { x, y },
            }),
        );
    }

    fn hit_label(
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

    fn save(&mut self, renderer: &mut iced::Renderer, app: &GpuCandidate, path: &std::path::Path) {
        let theme = theme(app, app.window.unwrap());
        let style = application_style(app, &theme);
        renderer.reset(self.bounds);
        self.element.as_widget().draw(
            &self.tree,
            renderer,
            &theme,
            &iced::advanced::renderer::Style {
                text_color: style.text_color,
            },
            Layout::new(&self.node),
            iced::mouse::Cursor::Unavailable,
            &self.bounds,
        );
        let [width, height] = [self.bounds.width as u32, self.bounds.height as u32];
        let bytes = Headless::screenshot(
            renderer,
            iced::Size::new(width, height),
            1.0,
            style.background_color,
        );
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

fn renderer() -> iced::Renderer {
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

fn output_dir() -> std::path::PathBuf {
    let path =
        std::path::PathBuf::from(std::env::var_os("P6_LAYOUT_OUTPUT").expect("owned output"));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn rail_click(
    app: &GpuCandidate,
    renderer: &iced::Renderer,
    intent: donor_rail::RailIntent,
) -> Message {
    assert_eq!(app.gallery_mode, GalleryMode::Slices);
    let model = gallery_rail_model(app);
    let at = view::gallery_rail_anchor(app, &model, app.state.gallery.slices.height);
    let point = donor_rail::intent_center(&model, intent).unwrap();
    let messages = Scene::new(app, renderer).click(
        renderer,
        iced::Point::new(at[0] + point.x, at[1] + point.y),
        iced::mouse::Button::Left,
    );
    messages
        .iter()
        .find(|message| matches!(message, Message::DonorRail(hit) if *hit == intent))
        .unwrap_or_else(|| panic!("rail {intent:?} at {at:?}/{point:?}: {messages:?}"))
        .clone()
}

fn clear_settings_motion(app: &mut GpuCandidate) {
    app.settings_workbench.folio_entrance = None;
    app.settings_workbench.tab_reveal = None;
}

fn wraps(message: &Message, matches: &impl Fn(&Message) -> bool) -> bool {
    match message {
        Message::SettingsLegacy { message, .. } => wraps(message, matches),
        message => matches(message),
    }
}

#[test]
#[ignore = "PK-06 mounted Settings and detail routes; explicit headless invocation"]
fn pk06_mounted_settings_and_detail_routes() {
    let output = output_dir();
    let mut renderer = renderer();
    let mut receipts = Vec::new();
    for (width, height, scale) in [(1920.0, 1080.0, 1.0), (900.0, 720.0, 0.5)] {
        let (mut app, calls) = pk06_fixture(3);
        app.viewport = [width, height];
        app.state.frontend_preferences.interface.ui_scale = Some(scale);
        app.monitor_ids = vec!["STABLE-A".into()];
        app.active_monitor_id = Some("STABLE-A".into());
        let ids = app
            .catalog
            .iter()
            .map(|entry| entry.id.clone())
            .collect::<Vec<_>>();
        app.config
            .excluded_wallpapers
            .insert("Monitor0".into(), vec![ids[1].clone()]);
        for (entry, dimensions) in
            app.catalog
                .iter_mut()
                .zip([[1920, 1080], [1920, 1080], [900, 1200]])
        {
            [entry.width, entry.height] = dimensions;
        }
        app.refresh_filter();
        assert_eq!(app.filtered_indices, [2, 0]);
        let message = rail_click(&app, &renderer, donor_rail::RailIntent::ToggleSettings);
        let _ = update(&mut app, message);
        clear_settings_motion(&mut app);
        let message = Scene::new(&app, &renderer).hit_label(
            &renderer, &crate::donor_folio::source_ui::sentence_case(app.settings_locale.text_static("settings-tab-filter")),
            |message| matches!(message, Message::SettingsIntent { intent: Intent::Settings(SettingsMsg::SelectTab(tab)), .. } if tab == "filter"),
        );
        let _ = update(&mut app, message);
        clear_settings_motion(&mut app);
        let message = Scene::new(&app, &renderer).hit_label(
            &renderer,
            app.settings_locale.text_static("r5-windows-controls"),
            |message| {
                matches!(
                    message,
                    Message::SettingsWindowsOpen {
                        category: SettingsCategory::CatalogFilters,
                        ..
                    }
                )
            },
        );
        let _ = update(&mut app, message);
        assert_eq!(
            app.settings_workbench.windows_panel,
            Some(SettingsCategory::CatalogFilters)
        );
        let message = Scene::new(&app, &renderer).hit_label(
            &renderer,
            app.settings_locale.text_static("r5-panel-copy-56"),
            |m| wraps(m, &|m| matches!(m, Message::ToggleShowExcluded)),
        );
        let _ = update(&mut app, message);
        assert!(app.show_excluded);
        assert_eq!(app.filtered_indices, [2, 1, 0]);
        let message = Scene::new(&app, &renderer).hit_label(&renderer, "Any orientation", |m| {
            wraps(m, &|m| matches!(m, Message::CycleOrientation))
        });
        let _ = update(&mut app, message);
        assert_eq!(app.filter_rule.orientation, OrientationFilter::Landscape);
        assert_eq!(app.filtered_indices, [1, 0]);
        let settings_path = output.join(format!("settings-filter-windows-{width}-{scale}.png"));
        Scene::new(&app, &renderer).save(&mut renderer, &app, &settings_path);
        receipts.push(serde_json::json!({
            "path": settings_path, "scale": scale, "viewport": [width, height],
            "route": "Rail Settings → Filter & search → Windows controls",
            "before": [&ids[2], &ids[0]], "show_excluded": [&ids[2], &ids[1], &ids[0]],
            "landscape": [&ids[1], &ids[0]], "actual_hits": true,
        }));
        assert_eq!(calls.load(Ordering::Acquire), 0);

        let (mut app, calls) = d4a_fixture(3);
        app.viewport = [width, height];
        app.state.frontend_preferences.interface.ui_scale = Some(scale);
        app.set_gallery_mode(GalleryMode::Geometric);
        app.ordinary_roll = None;
        app.deterministic_animation = true;
        assert!(app.depth_frame.is_none());
        app.start_detail(true, "pk06-headless-detail");
        app.detail_progress = 1.0;
        assert!(app.detail_source.is_some() && view::detail_controls_active(&app));
        let wallpaper = &app.wallpapers[app.selected.unwrap()];
        let id = wallpaper.id.clone();
        let actions = view::detail_actions(&app, wallpaper);
        let card = donor_detail::external_target(app.viewport);
        let detail = donor_back_layout::layout(
            card,
            false,
            donor_back_layout::DETAIL_FACT_COUNT,
            &actions,
            view::settings::bridge::effective_ui_scale(&app),
            UI_FONT,
            &wallpaper.title,
        );
        let (_, tags) = detail.source_text_bounds();
        let center = iced::Point::new(
            tags.x + tags.width.min(62.0 * detail.content_scale) * 0.5,
            tags.center_y(),
        );
        let [x, y] = card.point([center.x, center.y]);
        let messages = Scene::new(&app, &renderer).click(
            &renderer,
            iced::Point::new(x, y),
            iced::mouse::Button::Left,
        );
        assert!(messages.iter().any(|message| matches!(message, Message::DonorDetail(target, donor_back_layout::DetailIntent::LocalTags) if target == &id)), "actual detail canvas Tags hit: {messages:?}");
        let message = messages.into_iter().find(|message| matches!(message, Message::DonorDetail(target, donor_back_layout::DetailIntent::LocalTags) if target == &id)).unwrap();
        assert_eq!(update(&mut app, message).units(), 1);
        assert_eq!(app.tag_drawer_item.as_ref(), Some(&id));
        assert_eq!(app.tag_owner().unwrap().item_id(), id);
        use crate::candidate_app::local_library::TagEdit;
        for (key, expected) in [("海", "海"), ("辺", "海辺")] {
            let mut editor = Scene::new(&app, &renderer);
            focus(&mut editor, &renderer, &app.local_tag_input_id);
            let typed = type_text(&mut editor, &renderer, key);
            assert!(typed.iter().any(|message| matches!(message, Message::LocalTag { owner, edit: TagEdit::Text(value) } if owner.item_id() == id && value == expected)));
            drop(editor);
            for message in typed {
                let _ = update(&mut app, message);
            }
        }
        let add = Scene::new(&app, &renderer).hit_label(
            &renderer, app.settings_locale.text_static("r5-add"),
            |message| matches!(message, Message::LocalTag { owner, edit: TagEdit::Add } if owner.item_id() == id),
        );
        let _ = update(&mut app, add);
        let saved = CandidateState::load(&app.state_path);
        assert_eq!(saved.local_library, app.state.local_library);
        assert!(app
            .catalog
            .iter()
            .find(|entry| entry.id == id)
            .unwrap()
            .tags
            .contains("海辺"));
        let detail_path = output.join(format!("detail-captured-tags-{width}-{scale}.png"));
        Scene::new(&app, &renderer).save(&mut renderer, &app, &detail_path);
        let back = Scene::new(&app, &renderer).hit_label(
            &renderer,
            app.settings_locale.text_static("tags-back"),
            |message| matches!(message, Message::BackToSearchFilters),
        );
        assert_eq!(update(&mut app, back).units(), 1);
        assert!(app.panel_state.search_open && app.tag_drawer_item.is_none());
        assert!(Scene::new(&app, &renderer)
            .probe(&renderer)
            .inputs
            .iter()
            .any(|(key, _)| key.as_ref() == Some(&app.search_input_id)));
        d3_key(&mut app, keyboard::key::Named::Escape);
        assert!(!app.panel_state.search_open && view::detail_controls_active(&app));
        assert_eq!(calls.load(Ordering::Acquire), 0);
        receipts.push(serde_json::json!({
            "path": detail_path, "scale": scale, "viewport": [width, height],
            "route": "Detail canvas Tags → captured item → add Unicode tag → Back → Escape",
            "item": id, "persisted_tag": "海辺", "actual_hits": true,
        }));
    }
    std::fs::write(
        output.join("PK06-ROUTE-INDEX.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
}

fn set_tags(app: &mut GpuCandidate, index: usize, tags: &[&str]) {
    app.catalog[index].source_tags = tags.iter().map(|tag| (*tag).into()).collect();
}

fn pk06_fixture(count: usize) -> (GpuCandidate, Arc<std::sync::atomic::AtomicUsize>) {
    let (mut app, calls) = d4a_fixture(count);
    app.launch_policy = p1_preview_policy();
    app.gallery_mode = GalleryMode::Slices;
    app.config.ui.show_top_bar = true;
    app.state.frontend_preferences.filter_bar.always_visible = Some(true);
    app.state.frontend_preferences.launch.animation = Some("none".into());
    app.config.catalog.local_folders.clear();
    app.sort_order = SortOrder::Newest;
    app.state.playlists.clear();
    app.state.favorites.clear();
    (app, calls)
}

fn choose_browse(app: &mut GpuCandidate, id: &str) {
    use crate::candidate_app::playlists::Browse;
    let _ = update(
        app,
        Message::DonorRail(donor_rail::RailIntent::TogglePlaylists),
    );
    let owner = app.playlist_owner().unwrap();
    let _ = update(
        app,
        Message::PlaylistBrowse(owner, Browse::Choose(Some(id.into()))),
    );
    assert_eq!(app.browse_playlist_id.as_deref(), Some(id));
}

#[test]
fn pk06_any_gallery_keeps_union_and_cloud_requires_all_tag_conditions() {
    let (mut app, calls) = pk06_fixture(5);
    set_tags(&mut app, 0, &["red", "night", "空", "a"]);
    set_tags(&mut app, 1, &["red", "red-only", "空"]);
    set_tags(&mut app, 2, &["night", "night-only", "空"]);
    set_tags(&mut app, 3, &["red", "night", "omit", "forbidden"]);
    set_tags(&mut app, 4, &["red", "night", "空", "outside-playlist"]);
    app.project_local_tags();
    app.state.playlists.push(Playlist {
        id: "manual".into(),
        name: "Manual".into(),
        order: PlaybackOrder::Sequential,
        membership: PlaylistMembership::Explicit {
            wallpaper_ids: app.catalog[..4]
                .iter()
                .map(|entry| entry.id.clone())
                .collect(),
        },
    });
    choose_browse(&mut app, "manual");
    app.selected = Some(0);
    for message in [
        Message::SetTagMatchAny(true),
        Message::SearchTag("red".into(), false),
        Message::SearchTag("night".into(), false),
        Message::SearchTag("omit".into(), true),
    ] {
        let _ = update(&mut app, message);
    }
    assert_eq!(app.filtered_indices, [2, 1, 0]);
    assert_eq!(app.selected, Some(0));
    assert_eq!(
        app.search_tag_entries(),
        [("night", 1), ("red", 1), ("omit", 0), ("a", 1), ("空", 1)]
    );
    let _ = update(&mut app, Message::SetTagMatchAny(false));
    assert_eq!(app.filtered_indices, [0]);
    assert_eq!(
        app.search_tag_entries(),
        [("night", 1), ("red", 1), ("omit", 0), ("a", 1), ("空", 1)]
    );
    let _ = update(&mut app, Message::SearchTag("空".into(), false));
    let _ = update(&mut app, Message::SearchTag("空".into(), true));
    assert!(!app.filter_rule.tags.contains("空") && app.filter_rule.excluded_tags.contains("空"));
    assert!(app.filtered_indices.is_empty());
    assert_eq!(
        app.search_tag_entries(),
        [("night", 0), ("omit", 0), ("red", 0), ("空", 0)]
    );
    let _ = update(&mut app, Message::SearchTag("空".into(), true));
    assert!(!app.filter_rule.excluded_tags.contains("空"));
    let _ = update(&mut app, Message::SearchTag("night".into(), false));
    assert!(!app.filter_rule.tags.contains("night"));
    let _ = update(&mut app, Message::SetTagMatchAny(true));
    assert_eq!(
        app.search_tag_entries(),
        [
            ("red", 2),
            ("omit", 0),
            ("空", 2),
            ("a", 1),
            ("night", 1),
            ("red-only", 1)
        ]
    );
    let _ = update(&mut app, Message::ToggleTagSort);
    assert_eq!(
        app.search_tag_entries(),
        [
            ("omit", 0),
            ("red", 2),
            ("a", 1),
            ("night", 1),
            ("red-only", 1),
            ("空", 2)
        ]
    );

    let _ = update(
        &mut app,
        Message::SearchTag("missing-positive".into(), false),
    );
    assert_eq!(
        app.filtered_indices,
        [1, 0],
        "gallery Any keeps the existing red results"
    );
    assert_eq!(
        app.search_tag_entries(),
        [("missing-positive", 0), ("omit", 0), ("red", 0)]
    );
    let _ = update(
        &mut app,
        Message::SearchTag("missing-positive".into(), false),
    );

    let folder = app.state_path.with_file_name("pk06-retained-folder");
    app.catalog[0].folder = folder.clone();
    app.catalog[0].title = "Coast Needle".into();
    let item_id = app.catalog[0].id.clone();
    let query = format!("NEEDLE {item_id} retained-folder 空");
    let _ = update(&mut app, Message::SearchChanged(query.clone()));
    assert_eq!(
        app.filtered_indices,
        [0],
        "title/ID/folder/Unicode tag can match separate words"
    );
    app.filter_rule.folders.insert(folder);
    app.refresh_filter();
    let mut expected = app.filter_rule.clone();
    expected.tags.clear();
    expected.excluded_tags.clear();
    let _ = update(&mut app, Message::ClearSearchTags);
    assert_eq!(app.filter_rule, expected);
    assert_eq!(app.browse_playlist_id.as_deref(), Some("manual"));
    assert_eq!(app.filtered_indices, [0]);
    assert_eq!(app.selected, Some(0));
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

#[test]
fn pk06_non_tag_scope_and_current_browse_membership_drive_counts() {
    use crate::candidate_state::preferences::{Orientation, ResolutionPreset};
    use library::color_metadata::{self, ColorBucket};

    let (mut app, calls) = pk06_fixture(2);
    set_tags(&mut app, 0, &["a", "scope"]);
    set_tags(&mut app, 1, &["b", "scope"]);
    app.project_local_tags();
    for (index, entry) in app.catalog.iter_mut().enumerate() {
        entry.title = if index == 0 {
            "Wallpaper survivor".into()
        } else {
            "Wallpaper alternate".into()
        };
        entry.kind = if index == 0 {
            "image".into()
        } else {
            "video".into()
        };
        entry.folder = app
            .state_path
            .with_file_name(format!("pk06-folder-{index}"));
        [entry.width, entry.height] = if index == 0 {
            [1920, 1080]
        } else {
            [900, 1200]
        };
    }
    app.wallpapers[1].source = WallpaperSource::LocalVideo;
    let assert_scope = |app: &GpuCandidate, index| {
        assert_eq!(app.filtered_indices, [index]);
        assert_eq!(
            app.search_tag_entries(),
            [(if index == 0 { "a" } else { "b" }, 1), ("scope", 1)]
        );
    };
    let initial = app.filter_rule.clone();
    for rule in [
        SmartRule {
            text: "WALL SURVIVOR".into(),
            ..initial.clone()
        },
        SmartRule {
            kinds: ["image".into()].into(),
            ..initial.clone()
        },
        SmartRule {
            folders: [app.catalog[0].folder.clone()].into(),
            ..initial.clone()
        },
        SmartRule {
            minimum_width: 1920,
            ..initial.clone()
        },
        SmartRule {
            orientation: OrientationFilter::Landscape,
            ..initial.clone()
        },
        SmartRule {
            resolution_preset: Some(ResolutionPreset {
                label: "FHD only".into(),
                orientation: Orientation::Wide,
                from: (1920, 1080),
                to: Some((1920, 1080)),
                retained: Default::default(),
            }),
            ..initial.clone()
        },
    ] {
        app.filter_rule = rule;
        app.refresh_filter();
        assert_scope(&app, 0);
    }
    app.filter_rule = initial;
    app.source_filter = SourceFilter::Pictures;
    app.refresh_filter();
    assert_scope(&app, 0);
    app.source_filter = SourceFilter::All;
    app.monitor_ids = vec!["STABLE-A".into()];
    app.active_monitor_id = Some("STABLE-A".into());
    let ids = [app.catalog[0].id.clone(), app.catalog[1].id.clone()];
    app.config
        .excluded_wallpapers
        .insert("Monitor0".into(), vec![ids[1].clone()]);
    app.refresh_filter();
    assert_scope(&app, 0);
    app.show_excluded = true;
    app.config
        .disabled_wallpapers
        .insert("Monitor0".into(), vec![ids[0].clone()]);
    app.refresh_filter();
    assert_scope(&app, 1);
    app.config.disabled_wallpapers.clear();
    app.config.excluded_wallpapers.clear();

    app.wallpapers[1].source = WallpaperSource::LocalImage;
    for (index, rgb) in [[255, 0, 0], [0, 0, 255]].into_iter().enumerate() {
        let sample = image::RgbaImage::from_pixel(2, 2, image::Rgba([rgb[0], rgb[1], rgb[2], 255]));
        let path = app
            .state_path
            .with_file_name(format!("pk06-colour-{index}.png"));
        sample.save(&path).unwrap();
        app.wallpapers[index].file_target = path.clone();
        app.wallpapers[index].preview_path = Some(path.clone());
        let still = library::media_still::MediaStill::select(
            &app.wallpapers[index],
            app.catalog_generation.load(Ordering::Acquire),
        )
        .unwrap();
        let metadata = color_metadata::analyse_sample(
            &sample,
            color_metadata::hash_bytes(&std::fs::read(&path).unwrap()),
        )
        .unwrap();
        app.colours.entries.insert(
            still.item_id.clone(),
            colours::Stored {
                still: Some(still.clone()),
                reason: None,
                persistent: false,
            },
        );
        app.colours.metadata.insert(still.item_id, metadata);
    }
    app.colours.query.bucket = Some(ColorBucket::Blue);
    app.refresh_filter();
    assert_scope(&app, 1);
    app.colours.query = Default::default();
    app.filter_rule.favorites_only = true;
    app.state.favorites.insert(ids[1].clone());
    app.refresh_filter();
    assert_scope(&app, 1);
    app.filter_rule.favorites_only = false;
    app.state.favorites = ids.iter().cloned().collect();

    app.state.playlists.push(Playlist {
        id: "smart".into(),
        name: "Smart".into(),
        order: PlaybackOrder::Sequential,
        membership: PlaylistMembership::Smart {
            rule: SmartRule {
                tags: ["scope".into()].into(),
                favorites_only: true,
                ..Default::default()
            },
        },
    });
    choose_browse(&mut app, "smart");
    assert_eq!(app.search_tag_entries(), [("scope", 2), ("a", 1), ("b", 1)]);
    app.selected = Some(0);
    let _ = update(&mut app, Message::ToggleFavorite);
    assert_scope(&app, 1);
    assert_eq!(app.selected, Some(1));
    let _ = update(&mut app, Message::OpenLocalTags(ids[1].clone()));
    let owner = app.tag_owner().unwrap();
    let _ = update(
        &mut app,
        Message::LocalTag {
            owner,
            edit: crate::candidate_app::local_library::TagEdit::Remove("scope".into()),
        },
    );
    assert!(app.filtered_indices.is_empty() && app.search_tag_entries().is_empty());
    assert_eq!(
        app.browse_playlist_id.as_deref(),
        Some("smart"),
        "existing empty smart list is retained"
    );
    assert!(!app.browse_playlist_unavailable);
    app.state.playlists.clear();
    app.refresh_filter();
    assert_eq!(app.search_tag_entries(), [("a", 1), ("b", 1), ("scope", 1)]);
    assert!(app.browse_playlist_id.is_none() && app.browse_playlist_unavailable);
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

fn fits(outer: iced::Rectangle, inner: iced::Rectangle) -> bool {
    inner.width > 0.0
        && inner.height > 0.0
        && outer.x <= inner.x + 0.1
        && outer.y <= inner.y + 0.1
        && outer.x + outer.width + 0.1 >= inner.x + inner.width
        && outer.y + outer.height + 0.1 >= inner.y + inner.height
}

fn region(probe: &Probe, id: &'static str) -> iced::Rectangle {
    probe
        .containers
        .iter()
        .find(|(key, _)| key.as_ref() == Some(&iced::widget::Id::new(id)))
        .unwrap_or_else(|| panic!("mounted container {id}"))
        .1
}

fn scroll_region(
    probe: &Probe,
    id: &'static str,
) -> Option<(iced::Rectangle, iced::Rectangle, iced::Vector)> {
    probe
        .scrolls
        .iter()
        .find(|(key, _, _, _)| key.as_ref() == Some(&iced::widget::Id::new(id)))
        .map(|(_, bounds, content, translation)| (*bounds, *content, *translation))
}

fn input_bounds(probe: &Probe, id: &iced::widget::Id) -> iced::Rectangle {
    probe
        .inputs
        .iter()
        .find(|(key, _)| key.as_ref() == Some(id))
        .expect("mounted input")
        .1
}

fn text_bounds(probe: &Probe, label: &str) -> iced::Rectangle {
    probe
        .texts
        .iter()
        .find(|(text, _)| text == label)
        .unwrap_or_else(|| panic!("mounted text {label}"))
        .1
}

fn hit_toolbar(scene: &mut Scene<'_>, renderer: &iced::Renderer, label: &str) -> Vec<Message> {
    for _ in 0..20 {
        let probe = scene.probe(renderer);
        let (bar, _, _) = scroll_region(&probe, "search-tag-toolbar-scroll").unwrap();
        let target = text_bounds(&probe, label);
        if fits(bar, target) {
            return scene.click(renderer, target.center(), iced::mouse::Button::Left);
        }
        let step = if target.x < bar.x { 1.0 } else { -1.0 } * bar.width * 0.35;
        scene.wheel(renderer, bar.center(), step, 0.0);
    }
    panic!("unreachable toolbar label {label}");
}

fn type_text(scene: &mut Scene<'_>, renderer: &iced::Renderer, value: &str) -> Vec<Message> {
    scene.event(
        renderer,
        iced::mouse::Cursor::Unavailable,
        iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Character(value.into()),
            modified_key: keyboard::Key::Character(value.into()),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: Some(value.into()),
            repeat: false,
        }),
    )
}

fn focus(scene: &mut Scene<'_>, renderer: &iced::Renderer, id: &iced::widget::Id) {
    scene.operate(renderer, &mut operation::focusable::focus::<()>(id.clone()));
}

#[test]
#[ignore = "PK-06 Search production layout, hits, focus and scroll; explicit headless invocation"]
fn pk06_search_composition_default_minimum_and_narrow() {
    let output = output_dir();
    let mut renderer = renderer();
    let mut receipts = Vec::new();
    let long = "星空非常に長い壁紙标签・合成したアクセントé".repeat(12);
    for (width, height, scale, locale, panel_width) in [
        (1920.0, 1080.0, 1.0, "en-US", 760.0),
        (1280.0, 720.0, 0.5, "zh-CN", 760.0),
        (640.0, 600.0, 1.0, "en-US", 360.0),
        (640.0, 600.0, 0.5, "zh-CN", 360.0),
        (640.0, 600.0, 0.5, "en-US", 360.0),
    ] {
        for case in [
            "collapsed",
            "empty",
            "populated",
            "no-match",
            "overflow",
            "long-cjk",
        ] {
            let items = match case {
                "empty" => 0,
                "collapsed" => 13,
                _ => 1,
            };
            let (mut app, calls) = pk06_fixture(items);
            app.viewport = [width, height];
            app.settings_locale = localization::Catalog::new(locale, locale).unwrap();
            app.state.frontend_preferences.interface.ui_scale = Some(scale);
            app.state.frontend_preferences.tag_panel.width = Some(panel_width);
            app.catalog_loaded = true;
            let tag_count = match case {
                "overflow" => 45,
                "populated" => 3,
                _ => 0,
            };
            if let Some(entry) = app.catalog.first_mut() {
                entry.source_tags = (0..tag_count)
                    .map(|index| format!("pk06-chip-{index:02}"))
                    .collect();
                if case == "populated" {
                    entry.source_tags.insert("sea\nside".into());
                }
                if case == "long-cjk" {
                    entry
                        .source_tags
                        .insert("available-only-without-selection".into());
                }
            }
            app.project_local_tags();
            if case == "populated" {
                let _ = update(&mut app, Message::SearchChanged("item".into()));
            }
            if case == "no-match" {
                let _ = update(&mut app, Message::SearchChanged("missing query 404".into()));
                assert!(app.filtered_indices.is_empty() && !app.catalog.is_empty());
            }
            if case == "long-cjk" {
                let _ = update(&mut app, Message::SearchTag(long.clone(), false));
                assert_eq!(app.search_tag_entries(), [(long.as_str(), 0)]);
            }
            let open = rail_click(&app, &renderer, donor_rail::RailIntent::OpenSearch);
            assert_eq!(update(&mut app, open).units(), 1);
            assert!(app.panel_state.search_open && !app.search_tags_open);
            let mut collapsed = Scene::new(&app, &renderer);
            let collapsed_probe = collapsed.probe(&renderer);
            assert!(scroll_region(&collapsed_probe, "r5-search-body").is_none());
            let collapsed_height = region(&collapsed_probe, "search-tag-panel").height;
            let collapsed_field = region(&collapsed_probe, "search-tag-field");
            let close = rail_click(&app, &renderer, donor_rail::RailIntent::OpenSearch);
            let chip = text_bounds(
                &collapsed_probe,
                app.settings_locale.text_static("tags-search-mode-tags"),
            );
            assert!(collapsed
                .click(&renderer, chip.center(), iced::mouse::Button::Left)
                .iter()
                .any(|message| matches!(message, Message::CloseSearchPanel)));
            drop(collapsed);
            let _ = update(&mut app, close);
            assert!(!app.panel_state.search_open);
            let reopen = rail_click(&app, &renderer, donor_rail::RailIntent::OpenSearch);
            assert_eq!(update(&mut app, reopen).units(), 1);
            let mut collapsed = Scene::new(&app, &renderer);
            if case != "collapsed" {
                let toggle = hit_toolbar(
                    &mut collapsed,
                    &renderer,
                    app.settings_locale.text_static("r5-panel-copy-05"),
                )
                .into_iter()
                .find(|message| matches!(message, Message::ToggleSearchTags))
                .expect("expand hit");
                drop(collapsed);
                assert_eq!(update(&mut app, toggle).units(), 1);
                assert!(app.search_tags_open);
            } else {
                drop(collapsed);
            }
            let query = app.filter_rule.text.clone();
            let mut scene = Scene::new(&app, &renderer);
            let probe = scene.probe(&renderer);
            let panel = region(&probe, "search-tag-panel");
            let field = region(&probe, "search-tag-field");
            let input = input_bounds(&probe, &app.search_input_id);
            let (bar, bar_content, _) = scroll_region(&probe, "search-tag-toolbar-scroll").unwrap();
            let title = text_bounds(&probe, app.settings_locale.text_static("tags-filter-title"));
            let count = text_bounds(&probe, &format!("· {}", app.filtered_indices.len()));
            if case == "collapsed" {
                assert_eq!(app.filtered_indices.len(), 13);
            }
            let mode = text_bounds(
                &probe,
                app.settings_locale.text_static("tags-search-mode-tags"),
            );
            let magnifier = text_bounds(&probe, "\u{f0349}");
            let help = text_bounds(
                &probe,
                app.settings_locale.text_static("search-tag-counts-help"),
            );
            assert!(
                fits(scene.bounds, panel)
                    && fits(panel, bar)
                    && fits(panel, title)
                    && fits(panel, count)
                    && fits(panel, field)
                    && fits(panel, help),
                "{case} {width} {scale}: {panel:?} {bar:?} {title:?} {field:?} {help:?}"
            );
            assert!(
                title.x + title.width <= count.x
                    && count.x + count.width <= bar.x
                    && bar.y + bar.height <= field.y
                    && field.y + field.height <= help.y
            );
            assert!(fits(field, mode) && fits(field, magnifier) && fits(field, input));
            assert!(mode.x + mode.width <= magnifier.x && magnifier.x + magnifier.width <= input.x);
            assert_eq!(
                (field.x, field.width, field.height),
                (
                    collapsed_field.x,
                    collapsed_field.width,
                    collapsed_field.height
                )
            );
            assert!(!probe.texts.iter().any(|(label, _)| label
                == app.settings_locale.text_static("r5-panel-copy-01")
                || label == app.settings_locale.text_static("tags-edit-selected")
                || label == "Any orientation"));
            if bar_content.width <= bar.width + 0.1 {
                let esc = text_bounds(&probe, "ESC");
                assert!(bar.x + bar.width - (esc.x + esc.width) < 24.0 * (scale as f32).max(1.0));
            }
            let path = output.join(format!(
                "search-{width}x{height}-{scale}-{locale}-{case}.png"
            ));
            scene.save(&mut renderer, &app, &path);

            let mut reached = 0;
            let mut scroll_path = None;
            let mut payload = false;
            let mut source_payload = false;
            if case == "collapsed" {
                assert_eq!(panel.height, collapsed_height);
            } else {
                let (body, content, _) = scroll_region(&probe, "r5-search-body").unwrap();
                assert!(
                    fits(panel, body)
                        && body.y >= help.y + help.height
                        && panel.height > collapsed_height
                );
                if matches!(case, "empty" | "no-match") {
                    assert!(fits(
                        body,
                        text_bounds(&probe, app.settings_locale.text_static("r5-panel-copy-10"))
                    ));
                }
                if case == "long-cjk" {
                    let (label, tag_bounds) = probe
                        .texts
                        .iter()
                        .find(|(label, _)| label.starts_with("星空"))
                        .unwrap();
                    assert!(
                        label.contains('…') && label.ends_with("  0") && fits(body, *tag_bounds)
                    );
                    for (button, exclude) in [
                        (iced::mouse::Button::Left, false),
                        (iced::mouse::Button::Right, true),
                    ] {
                        assert!(scene.click(&renderer, tag_bounds.center(), button).iter()
                            .any(|m| matches!(m, Message::SearchTag(value, negative) if value == &long && *negative == exclude)));
                    }
                    let messages = hit_toolbar(
                        &mut scene,
                        &renderer,
                        app.settings_locale.text_static("search-clear-tags"),
                    );
                    assert!(messages
                        .iter()
                        .any(|m| matches!(m, Message::ClearSearchTags)));
                    payload = true;
                }
                if tag_count > 0 {
                    let mut tags = std::collections::BTreeSet::new();
                    let steps = (content.height / (body.height * 0.45)).ceil() as usize + 1;
                    for step in 0..steps {
                        let p = scene.probe(&renderer);
                        assert_eq!(input_bounds(&p, &app.search_input_id), input);
                        for (label, bounds) in &p.texts {
                            if case == "populated"
                                && !source_payload
                                && label == "sea side  1"
                                && fits(body, *bounds)
                            {
                                for (button, excluded) in [
                                    (iced::mouse::Button::Left, false),
                                    (iced::mouse::Button::Right, true),
                                ] {
                                    assert!(scene.click(&renderer, bounds.center(), button).iter().any(|m|
                                        matches!(m, Message::SearchTag(value, negative) if value == "sea\nside" && *negative == excluded)));
                                }
                                source_payload = true;
                            }
                            if label.starts_with("pk06-chip-") && fits(body, *bounds) {
                                tags.insert(label.clone());
                                if step == 0 && label.starts_with("pk06-chip-00") {
                                    assert!(scene.click(&renderer, bounds.center(), iced::mouse::Button::Left)
                                        .iter().any(|m| matches!(m, Message::SearchTag(value, false) if value == "pk06-chip-00")));
                                }
                            }
                        }
                        scene.wheel(&renderer, body.center(), 0.0, -body.height * 0.45);
                    }
                    assert_eq!(
                        source_payload,
                        case == "populated",
                        "flattened source label and count are reachable within the body"
                    );
                    reached = tags.len();
                    assert_eq!(
                        reached, tag_count,
                        "all chips reachable in {case} at {width}, {scale}"
                    );
                    if case == "overflow" {
                        let (_, _, translation) =
                            scroll_region(&scene.probe(&renderer), "r5-search-body").unwrap();
                        assert!(translation.y.abs() > body.height && content.height > body.height);
                        let path = output.join(format!(
                            "search-{width}x{height}-{scale}-{locale}-overflow-end.png"
                        ));
                        scene.save(&mut renderer, &app, &path);
                        scroll_path = Some(path);
                    }
                }
            }

            // The reducer returns this same ID-based focus operation. Exercise it
            // on the mounted widget and require the widget to consume real typing.
            focus(&mut scene, &renderer, &app.search_input_id);
            let typed = type_text(&mut scene, &renderer, "q");
            assert!(typed.iter().any(
                |m| matches!(m, Message::SearchChanged(value) if value == &format!("{query}q"))
            ));
            for (label, accept) in [
                (app.settings_locale.text_static("r5-panel-copy-06"), 0),
                (app.settings_locale.text_static("r5-panel-copy-07"), 1),
                ("A–Z", 2),
            ] {
                let messages = hit_toolbar(&mut scene, &renderer, label);
                assert!(
                    messages.iter().any(|m| match accept {
                        0 => matches!(m, Message::SetTagMatchAny(false)),
                        1 => matches!(m, Message::SetTagMatchAny(true)),
                        _ => matches!(m, Message::ToggleTagSort),
                    }),
                    "toolbar hit {case} {width} {label}: {messages:?}"
                );
            }
            let esc = hit_toolbar(&mut scene, &renderer, "ESC")
                .into_iter()
                .find(|m| matches!(m, Message::CloseSearchPanel))
                .expect("Escape affordance hit");
            if case == "populated" {
                let collapse = hit_toolbar(
                    &mut scene,
                    &renderer,
                    app.settings_locale.text_static("r5-panel-copy-04"),
                )
                .into_iter()
                .find(|m| matches!(m, Message::ToggleSearchTags))
                .expect("collapse hit");
                drop(scene);
                assert_eq!(update(&mut app, collapse).units(), 1);
                assert!(
                    !app.search_tags_open
                        && app.panel_state.search_open
                        && app.filter_rule.text == query
                );
                let mut scene = Scene::new(&app, &renderer);
                focus(&mut scene, &renderer, &app.search_input_id);
                assert!(type_text(&mut scene, &renderer, "q").iter().any(
                    |m| matches!(m, Message::SearchChanged(value) if value == &format!("{query}q"))
                ));
                drop(scene);
                d3_key(&mut app, keyboard::key::Named::Escape);
            } else {
                drop(scene);
                let _ = update(&mut app, esc);
            }
            assert!(!app.panel_state.search_open && app.visible && app.filter_rule.text == query);
            assert_eq!(calls.load(Ordering::Acquire), 0);
            receipts.push(serde_json::json!({
                "path": path, "scroll_path": scroll_path, "case": case, "scale": scale, "locale": locale,
                "viewport": [width, height], "panel": format!("{panel:?}"), "toolbar": format!("{bar:?}"),
                "input": format!("{input:?}"), "chip_labels_reached": reached, "complete_unicode_payload": payload,
                "filtered_count": app.filtered_indices.len(), "configured_panel_width": panel_width,
                "flattened_source_label_raw_payload": source_payload,
                "mounted_rail_and_toolbar_hits": true, "mounted_focus_typing": true,
            }));
        }
    }
    std::fs::write(
        output.join("PK06-CAPTURE-INDEX.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
}

//! PK-04 routes through the actual rail, folder catalogue and Settings owners.
use super::*;
use crate::candidate_state::{AutomationIntent, RandomRotationState as Random};

#[test]
fn pk04_rail_typed_sorts_resolution_and_random_keep_real_owners_and_preview() {
    let (mut app, calls) = d3_application_fixture(3);
    app.launch_policy = p1_preview_policy();
    for order in SortOrder::ALL {
        let _ = update(
            &mut app,
            Message::DonorRail(donor_rail::RailIntent::SetSort(order)),
        );
        assert_eq!(app.sort_order, order);
        assert_eq!(gallery_rail_model(&app).sort_order, order);
    }
    assert!(!gallery_rail_model(&app).visibility.resolution);
    app.filter_rule.minimum_width = 1920;
    app.state.frontend_preferences.visibility.resolution = Some(false);
    assert!(gallery_rail_model(&app).visibility.resolution);
    let _ = update(
        &mut app,
        Message::DonorRail(donor_rail::RailIntent::CycleResolution),
    );
    assert!(!app.resolution_active());
    app.state.automation_intent = Some(AutomationIntent::default());
    let before = unix_now();
    let interval = app.state.slideshow_interval_seconds.max(10);
    let _ = update(
        &mut app,
        Message::DonorRail(donor_rail::RailIntent::ToggleRandomRotation),
    );
    assert_eq!(gallery_rail_model(&app).random_state, Random::On);
    assert!(app.next_slideshow_unix >= before + interval);
    app.safely_stop_automation();
    assert_eq!(gallery_rail_model(&app).random_state, Random::SafelyStopped);
    let _ = update(
        &mut app,
        Message::DonorRail(donor_rail::RailIntent::ToggleRandomRotation),
    );
    assert!(!app.state.automation_intent().random_enabled);
    assert!(app.state.automation_intent().safely_stopped || app.automation_suspended);
    // Intentional re-enable is PK03's explicit recovery owner; unresolved provider blocks it.
    app.provider_state_unknown = true;
    let _ = update(
        &mut app,
        Message::DonorRail(donor_rail::RailIntent::ToggleRandomRotation),
    );
    assert!(!app.state.automation_intent().random_enabled);
    assert!(app.state.automation_intent().safely_stopped || app.automation_suspended);
    app.provider_state_unknown = false;
    let _ = update(&mut app, Message::ResumeRandomRotation);
    assert_eq!(gallery_rail_model(&app).random_state, Random::On);
    let before = app.state.automation_intent();
    let _ = update(&mut app, Message::ApplySelected);
    assert_eq!(app.state.automation_intent(), before);
    assert_eq!(calls.load(Ordering::Acquire), 0);
    assert!(!app.apply_pending);
}

#[test]
fn pk04_folder_hierarchy_identity_clamping_scroll_and_polygon_hits() {
    use crate::donor_folder::{Model, Program, State};
    use iced::widget::canvas::Program as _;
    let (mut app, calls) = d3_application_fixture(0);
    let root = app.state_path.parent().unwrap().join("root");
    let child = root.join("nested").join("音楽");
    let twin = app.state_path.parent().unwrap().join("second").join("音楽");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::create_dir_all(&twin).unwrap();
    let root = crate::candidate_app::catalog::folder_filter_path(root);
    let child = crate::candidate_app::catalog::folder_filter_path(child);
    let twin = crate::candidate_app::catalog::folder_filter_path(twin);
    let missing = root.join("missing").join("非常に長い名前".repeat(12));
    app.config.catalog.local_folders = vec![root.clone(), child.clone(), twin.clone()];
    app.filter_rule.folders.insert(missing.clone());
    let choices = app.available_folders();
    assert!(choices.contains(&missing));
    for mode in [
        donor_rail::RailMode::Slices,
        donor_rail::RailMode::Geometric,
        donor_rail::RailMode::Wall,
    ] {
        for scale in [1.0, 0.5] {
            for rtl in [false, true] {
                let m = Model::new(
                    &choices,
                    &[root.clone()],
                    &[missing.clone()],
                    [260.0, 160.0],
                    scale,
                    mode,
                    rtl,
                    iced::Color::BLACK,
                    iced::Color::WHITE,
                    iced::Color::WHITE,
                    UI_FONT,
                    NERD_FONT,
                );
                assert!(m.width <= 244.0 && m.height <= 144.0);
                assert!(m.rows[0].path.is_none());
                let child_index = m
                    .rows
                    .iter()
                    .position(|r| r.path.as_ref() == Some(&child))
                    .unwrap();
                assert_eq!(m.rows[child_index].depth, 2);
                assert_ne!(
                    m.rows[child_index].label,
                    m.rows
                        .iter()
                        .find(|r| r.path.as_ref() == Some(&twin))
                        .unwrap()
                        .label
                );
                assert!(m.identity.join("").contains("unavailable"));
                assert!(!m.identity.iter().any(|line| line.ends_with('…')));
                let program = Program { model: m.clone() };
                let bounds =
                    iced::Rectangle::new(iced::Point::ORIGIN, iced::Size::new(m.width, m.height));
                for (index, row) in m.rows.iter().enumerate() {
                    let scroll = (m.row_rect(index, 0.0).y - 4.0).clamp(0.0, m.max_scroll());
                    let point = m.row_rect(index, scroll).center();
                    assert!(point.y >= 4.0 && point.y < m.height - 4.0);
                    let mut state = State::default();
                    state.scroll = scroll;
                    let action = program
                        .update(
                            &mut state,
                            &iced::widget::canvas::Event::Mouse(iced::mouse::Event::ButtonPressed(
                                iced::mouse::Button::Left,
                            )),
                            bounds,
                            iced::mouse::Cursor::Available(point),
                        )
                        .expect("real canvas callback");
                    assert_eq!(action.into_inner().0, Some(row.path.clone()));
                }
                if mode != donor_rail::RailMode::Wall {
                    let corner = if rtl {
                        iced::Point::new(m.width - 0.1, 0.1)
                    } else {
                        iced::Point::new(0.1, 0.1)
                    };
                    assert!(m.row_at(corner, 0.0).is_none());
                }
                let mut state = State::default();
                let _ = program.update(
                    &mut state,
                    &iced::widget::canvas::Event::Mouse(iced::mouse::Event::WheelScrolled {
                        delta: iced::mouse::ScrollDelta::Lines {
                            x: 0.0,
                            y: -10000.0,
                        },
                    }),
                    bounds,
                    iced::mouse::Cursor::Available(bounds.center()),
                );
                assert_eq!(state.scroll, m.max_scroll());
            }
        }
    }
    let _ = update(&mut app, Message::SetFolderFilter(Some(child.clone())));
    assert!(app.filter_rule.folders.contains(&child));
    assert!(!app.folder_menu_open);
    app.folder_menu_open = true;
    let _ = update(&mut app, Message::ToggleFolderMenu);
    assert!(!app.folder_menu_open);
    assert!(app.filter_rule.folders.contains(&child));
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

#[test]
fn pk04_hex_scrollbar_grabs_the_painted_thumb_and_drags_to_last_row() {
    use crate::donor_folder::{Model, Program, State};
    use iced::widget::canvas::Program as _;
    let choices = (0..40)
        .map(|i| format!("J:/library/row-{i:02}").into())
        .collect::<Vec<_>>();
    let model = Model::new(
        &choices,
        &[],
        &[],
        [900.0, 480.0],
        1.0,
        donor_rail::RailMode::Geometric,
        false,
        iced::Color::BLACK,
        iced::Color::WHITE,
        iced::Color::WHITE,
        UI_FONT,
        NERD_FONT,
    );
    let bounds = iced::Rectangle::with_size(iced::Size::new(model.width, model.height));
    let first = model.scroll_thumb(0.0).unwrap().center();
    let last = model.scroll_thumb(model.max_scroll()).unwrap().center();
    let program = Program { model };
    let mut state = State::default();
    let action = program
        .update(
            &mut state,
            &iced::widget::canvas::Event::Mouse(iced::mouse::Event::ButtonPressed(
                iced::mouse::Button::Left,
            )),
            bounds,
            iced::mouse::Cursor::Available(first),
        )
        .expect("painted thumb accepts press");
    assert_eq!(action.into_inner().0, None);
    assert_eq!(state.scroll, 0.0);
    assert!(program
        .update(
            &mut state,
            &iced::widget::canvas::Event::Mouse(iced::mouse::Event::CursorMoved { position: last }),
            bounds,
            iced::mouse::Cursor::Available(last)
        )
        .is_some());
    assert!((state.scroll - program.model.max_scroll()).abs() < 0.01);
    let last_row = program.model.rows.len() - 1;
    assert_eq!(
        program.model.row_at(
            program.model.row_rect(last_row, state.scroll).center(),
            state.scroll
        ),
        Some(last_row)
    );
}

#[test]
fn pk04_windows_route_is_mounted_and_preserves_stable_target_and_apply_policy() {
    use view::settings::{
        composed::{ActionId, Control},
        controller,
        folio::intent::SettingsMsg,
    };
    let (mut app, calls) = d3_application_fixture(1);
    app.launch_policy = p1_preview_policy();
    app.monitor_ids = vec!["STABLE-A".into(), "STABLE-B".into()];
    app.active_monitor_id = Some("STABLE-B".into());
    let _ = update(
        &mut app,
        Message::DonorRail(donor_rail::RailIntent::ToggleSettings),
    );
    controller::reconcile(&mut app);
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("picker".into()));
    assert!(controller::cards(&app)
        .iter()
        .flat_map(|(_, rows)| rows)
        .any(|row| matches!(
            row.control,
            Control::ActionBtn {
                id: ActionId::OpenDisplays,
                ..
            }
        )));
    p3_settings_intent(&mut app, SettingsMsg::Run(ActionId::OpenDisplays));
    assert_eq!(app.settings_workbench.tab, "displays");
    assert_eq!(app.active_monitor_id.as_deref(), Some("STABLE-B"));
    assert!(!app.launch_policy.allows_provider());
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

#[test]
fn pk04_full_identity_wrap_advances_clusters_and_preserves_multiple_paths() {
    for value in [
        "J:/root/音楽e\u{301}/長い path",
        "J:/first  spaced/path\nJ:/second/path · unavailable",
    ] {
        for width in [0.0, 1.0, 20.0, 200.0] {
            let lines = crate::donor_back_layout::measured_full_lines(value, 10.0, width, UI_FONT);
            assert!(lines.len() <= value.chars().count() + 1);
            assert_eq!(lines.concat(), value.replace('\n', ""));
            assert!(lines.iter().all(|line| !line.contains('\n')));
            assert!(!lines.iter().any(|line| line.ends_with('…')));
        }
    }
    let (mut app, _) = d3_application_fixture(0);
    app.viewport = [17.0, 17.0];
    app.filter_rule
        .folders
        .extend(["J:/one/音楽".into(), "J:/two/e\u{301}".into()]);
    let m = view::folder_popup_model(&app);
    assert_eq!(m.width, 1.0);
    assert_eq!(m.height, 1.0);
    assert!(m.header_height > 0.0);
}

#[test]
#[ignore = "PK04 actual production rail/folder/Windows widget frames and hits; explicit invocation"]
fn pk04_production_compositions_and_public_routes() {
    use iced::advanced::{
        clipboard, layout,
        renderer::{Headless, Renderer as _},
        widget::{operation, Operation, Tree},
        Layout, Shell,
    };
    use view::settings::{
        composed::{keys, ActionId},
        folio::intent::{Message as Intent, SettingsMsg},
    };
    #[derive(Default)]
    struct Probe {
        offset: iced::Vector,
        pending: Option<iced::Vector>,
        texts: Vec<(String, iced::Rectangle)>,
        inputs: Vec<iced::Rectangle>,
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
            _: Option<&iced::widget::Id>,
            bounds: iced::Rectangle,
            _: &mut dyn operation::TextInput,
        ) {
            self.inputs.push(bounds - self.offset);
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
    fn probe(
        element: &mut Element<'_, Message>,
        tree: &mut Tree,
        node: &layout::Node,
        renderer: &iced::Renderer,
    ) -> Probe {
        let mut p = Probe::default();
        element
            .as_widget_mut()
            .operate(tree, Layout::new(node), renderer, &mut p);
        p
    }
    fn click(
        element: &mut Element<'_, Message>,
        tree: &mut Tree,
        node: &layout::Node,
        renderer: &iced::Renderer,
        viewport: iced::Rectangle,
        point: iced::Point,
    ) -> Vec<Message> {
        let mut messages = Vec::new();
        for event in [
            iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),
            iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left),
        ] {
            element.as_widget_mut().update(
                tree,
                &iced::Event::Mouse(event),
                Layout::new(node),
                iced::mouse::Cursor::Available(point),
                renderer,
                &mut clipboard::Null,
                &mut Shell::new(&mut messages),
                &viewport,
            );
        }
        messages
    }

    fn action(message: &Message, expected: ActionId) -> bool {
        match message {
            Message::SettingsPageAction { message, .. } => action(message, expected),
            Message::SettingsIntent {
                intent: Intent::Settings(SettingsMsg::Run(actual)),
                ..
            } => *actual == expected,
            _ => false,
        }
    }
    fn closes(message: &Message) -> bool {
        match message {
            Message::SettingsPageAction { message, .. } => closes(message),
            Message::SettingsIntent {
                intent: Intent::ToggleSettings,
                ..
            } => true,
            _ => false,
        }
    }

    fn save(
        element: &mut Element<'_, Message>,
        tree: &mut Tree,
        node: &layout::Node,
        renderer: &mut iced::Renderer,
        viewport: iced::Rectangle,
        cursor: iced::mouse::Cursor,
        path: &std::path::Path,
    ) {
        renderer.reset(iced::Rectangle::with_size(viewport.size()));
        element.as_widget().draw(
            tree,
            renderer,
            &iced::Theme::Dark,
            &iced::advanced::renderer::Style {
                text_color: iced::Color::WHITE,
            },
            Layout::new(node),
            cursor,
            &viewport,
        );
        let pixels = Headless::screenshot(
            renderer,
            iced::Size::new(viewport.width as u32, viewport.height as u32),
            1.0,
            iced::Color::from_rgb8(40, 42, 50),
        );
        image::save_buffer_with_format(
            path,
            &pixels,
            viewport.width as u32,
            viewport.height as u32,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .unwrap();
    }
    fn send(
        element: &mut Element<'_, Message>,
        tree: &mut Tree,
        node: &layout::Node,
        renderer: &iced::Renderer,
        viewport: iced::Rectangle,
        point: iced::Point,
        event: iced::mouse::Event,
    ) -> Vec<Message> {
        let mut messages = Vec::new();
        element.as_widget_mut().update(
            tree,
            &iced::Event::Mouse(event),
            Layout::new(node),
            iced::mouse::Cursor::Available(point),
            renderer,
            &mut clipboard::Null,
            &mut Shell::new(&mut messages),
            &viewport,
        );
        messages
    }
    let output =
        std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned output"));
    std::fs::create_dir_all(&output).unwrap();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .unwrap();
    {
        let mut fonts = iced::advanced::graphics::text::font_system()
            .write()
            .unwrap();
        for language in ["en-US", "zh-CN", "ar-SA"] {
            fonts.load_font(std::borrow::Cow::Borrowed(
                localization::Catalog::new(language, language)
                    .unwrap()
                    .font_bytes(),
            ));
        }
        fonts.load_font(std::borrow::Cow::Borrowed(
            include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
        ));
    }
    let mut index = Vec::new();
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
        GalleryMode::Depth,
    ] {
        for scale in [1.0, 0.5] {
            for vertical in [false, true] {
                let (mut app, calls) = d3_application_fixture(3);
                app.launch_policy = p1_preview_policy();
                app.gallery_mode = mode;
                app.state.gallery.mode = match mode {
                    GalleryMode::Slices => GalleryKind::Slices,
                    GalleryMode::Geometric => GalleryKind::Geometric,
                    GalleryMode::Wall => GalleryKind::Wall,
                    GalleryMode::Depth => GalleryKind::Depth,
                };
                app.viewport = if vertical {
                    [640.0, 512.0]
                } else {
                    [1920.0, 1080.0]
                };
                app.config.ui.show_top_bar = true;
                app.state.frontend_preferences.interface.ui_scale = Some(scale);
                app.state.frontend_preferences.filter_bar.always_visible = Some(true);
                app.state.frontend_preferences.filter_bar.orientation =
                    Some(if vertical { "vertical" } else { "horizontal" }.into());
                app.state.frontend_preferences.filter_bar.visual_style = Some("match".into());
                app.filter_rule.minimum_width = 1920;
                let model = gallery_rail_model(&app);
                assert_eq!(
                    model.mode,
                    match mode {
                        GalleryMode::Slices | GalleryMode::Depth => donor_rail::RailMode::Slices,
                        GalleryMode::Geometric => donor_rail::RailMode::Geometric,
                        GalleryMode::Wall => donor_rail::RailMode::Wall,
                    }
                );
                let size = iced::Size::new(app.viewport[0], app.viewport[1]);
                let viewport = iced::Rectangle::with_size(size);
                let footprint = donor_rail::footprint(&model);
                // Production root has gallery-owned anchoring, so obtain the actual root rail container offset by the same owner.
                let gh = match mode {
                    GalleryMode::Slices => app.state.gallery.slices.height,
                    GalleryMode::Depth => app.state.gallery.depth.height,
                    GalleryMode::Wall => {
                        app.state.gallery.wall.card_height * app.state.gallery.wall.rows as f32
                            + app.state.gallery.wall.gap_y
                                * app.state.gallery.wall.rows.saturating_sub(1) as f32
                    }
                    GalleryMode::Geometric => {
                        let g = &app.state.gallery.geometric;
                        donor_rail::geometric_height(
                            g.radius,
                            g.rows,
                            g.gap_y,
                            g.stagger,
                            match g.shape {
                                StateGeometricShape::Circle => donor_layout::HexShape::Hexagon,
                                _ => donor_layout::HexShape::Hexagon,
                            },
                        )
                    }
                };
                let at = view::gallery_rail_anchor(&app, &model, gh);
                let mut element = view(&app, app.window.unwrap());
                let mut tree = Tree::new(&element);
                let node = element.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(size, size),
                );
                let path = output.join(format!("rail-{mode:?}-{scale}-vertical{vertical}.png"));
                save(
                    &mut element,
                    &mut tree,
                    &node,
                    &mut renderer,
                    viewport,
                    iced::mouse::Cursor::Unavailable,
                    &path,
                );
                for intent in (0..4)
                    .map(donor_rail::RailIntent::Source)
                    .chain(SortOrder::ALL.map(donor_rail::RailIntent::SetSort))
                    .chain((0..13).map(donor_rail::RailIntent::HuePreset))
                    .chain([
                        donor_rail::RailIntent::CycleResolution,
                        donor_rail::RailIntent::ToggleFolder,
                        donor_rail::RailIntent::ToggleFavorites,
                        donor_rail::RailIntent::ToggleRandomRotation,
                        donor_rail::RailIntent::OpenSearch,
                        donor_rail::RailIntent::ToggleEngineMute,
                        donor_rail::RailIntent::TogglePlaylists,
                        donor_rail::RailIntent::ToggleSettings,
                        donor_rail::RailIntent::Close,
                    ])
                {
                    if let Some(local) = donor_rail::intent_center(&model, intent) {
                        let messages = click(
                            &mut element,
                            &mut tree,
                            &node,
                            &renderer,
                            viewport,
                            iced::Point::new(at[0] + local.x, at[1] + local.y),
                        );
                        assert!(
                            messages
                                .iter()
                                .any(|m| matches!(m,Message::DonorRail(actual) if *actual==intent)),
                            "{mode:?}/{scale}/{vertical} missing {intent:?}: {messages:?}"
                        );
                    }
                }
                if !vertical && scale == 1.0 {
                    let local =
                        donor_rail::intent_center(&model, donor_rail::RailIntent::ToggleFolder)
                            .unwrap();
                    let hover = iced::Point::new(at[0] + local.x, at[1] + local.y);
                    let hover_path = output.join(format!("rail-{mode:?}-hover-no-tooltip.png"));
                    save(
                        &mut element,
                        &mut tree,
                        &node,
                        &mut renderer,
                        viewport,
                        iced::mouse::Cursor::Available(hover),
                        &hover_path,
                    );
                    let before = image::open(&path).unwrap().to_rgba8();
                    let after = image::open(&hover_path).unwrap().to_rgba8();
                    let mut changed = 0;
                    for (x, y, pixel) in before.enumerate_pixels() {
                        if *pixel != *after.get_pixel(x, y) {
                            changed += 1;
                            assert!(
                                x as f32 >= at[0] - 2.0
                                    && x as f32 <= at[0] + footprint[0] + 2.0
                                    && y as f32 >= at[1] - 2.0
                                    && y as f32 <= at[1] + footprint[1] + 2.0,
                                "hover paints outside final rail bounds"
                            );
                        }
                    }
                    assert!(changed > 0, "hover highlight remains");
                    index.push(serde_json::json!({"path":hover_path,"case":"hover-no-tooltip","mode":format!("{mode:?}"),"changed_pixels":changed,"outside_rail_changed_pixels":0}));
                }
                index.push(serde_json::json!({"path":path,"case":"rail","mode":format!("{mode:?}"),"skin":format!("{:?}",model.mode),"scale":scale,"vertical":vertical,"anchor":at,"footprint":footprint,"actual_action_hits":true,"provider_calls":calls.load(Ordering::Acquire)}));
            }
        }
    }
    for (name, mode, scale, rtl, count, selected) in [
        ("empty", GalleryMode::Slices, 1.0, false, 0, false),
        ("simple", GalleryMode::Slices, 1.0, false, 1, false),
        ("nested-slices", GalleryMode::Slices, 1.0, false, 6, true),
        ("nested-hex", GalleryMode::Geometric, 1.0, false, 6, true),
        ("nested-wall", GalleryMode::Wall, 1.0, false, 6, true),
        ("nested-depth", GalleryMode::Depth, 1.0, false, 6, true),
        ("min-slices", GalleryMode::Slices, 0.5, false, 6, true),
        ("min-hex", GalleryMode::Geometric, 0.5, false, 6, true),
        ("min-wall", GalleryMode::Wall, 0.5, false, 6, true),
        ("min-depth", GalleryMode::Depth, 0.5, false, 6, true),
        ("vertical-spill", GalleryMode::Slices, 1.0, false, 6, false),
        (
            "vertical-spill-rtl",
            GalleryMode::Slices,
            1.0,
            true,
            6,
            false,
        ),
        ("overflow-rtl", GalleryMode::Slices, 0.5, true, 35, true),
        ("overflow", GalleryMode::Slices, 1.0, false, 35, true),
        (
            "overflow-hex",
            GalleryMode::Geometric,
            1.0,
            false,
            35,
            false,
        ),
    ] {
        let (mut app, calls) = d3_application_fixture(0);
        app.launch_policy = p1_preview_policy();
        app.gallery_mode = mode;
        let vertical_spill = name.starts_with("vertical-spill");
        app.viewport = if vertical_spill {
            [900.0, 480.0]
        } else if count > 30 {
            [640.0, 512.0]
        } else {
            [1920.0, 1080.0]
        };
        app.config.ui.show_top_bar = true;
        app.folder_menu_open = !vertical_spill;
        if vertical_spill {
            app.state.frontend_preferences.filter_bar.orientation = Some("vertical".into());
        }
        app.state.frontend_preferences.filter_bar.always_visible = Some(true);
        app.state.frontend_preferences.filter_bar.visual_style = Some("match".into());
        app.state.frontend_preferences.interface.ui_scale = Some(scale);
        app.config.catalog.local_folders = (0..count)
            .map(|i| {
                std::path::PathBuf::from(if i == 0 {
                    "J:/PK04-owned".into()
                } else if i == 1 {
                    "J:/PK04-owned/nested/音楽".into()
                } else if i == 2 {
                    "J:/second/音楽".into()
                } else {
                    format!("J:/PK04-owned/nested/row-{i:02}")
                })
            })
            .collect();
        if vertical_spill {
            app.filter_rule
                .folders
                .insert(app.config.catalog.local_folders[1].clone());
        }
        if selected {
            app.filter_rule
                .folders
                .insert(std::path::PathBuf::from(format!(
                    "J:/PK04-owned/missing/{}",
                    "Long path 音楽 ".repeat(10)
                )));
        }
        if rtl {
            app.settings_locale.language = localization::LANGUAGES
                .iter()
                .find(|l| l.tag == "ar-SA")
                .unwrap();
            assert!(app.settings_locale.language.rtl);
        }
        let m = view::folder_popup_model(&app);
        let rail = gallery_rail_model(&app);
        let gh = match mode {
            GalleryMode::Geometric => {
                let g = &app.state.gallery.geometric;
                donor_rail::geometric_height(
                    g.radius,
                    g.rows,
                    g.gap_y,
                    g.stagger,
                    donor_layout::HexShape::Hexagon,
                )
            }
            GalleryMode::Wall => {
                app.state.gallery.wall.card_height * app.state.gallery.wall.rows as f32
                    + app.state.gallery.wall.gap_y
                        * app.state.gallery.wall.rows.saturating_sub(1) as f32
            }
            GalleryMode::Depth => app.state.gallery.depth.height,
            _ => app.state.gallery.slices.height,
        };
        let rail_at = view::gallery_rail_anchor(&app, &rail, gh);
        let at = donor_rail::folder_popup_anchor(&rail, rail_at, app.viewport, [m.width, m.height]);
        assert!(
            at[0] >= 0.0
                && at[1] >= 0.0
                && at[0] + m.width <= app.viewport[0]
                && at[1] + m.height <= app.viewport[1]
        );
        if count > 30 && !rail.vertical {
            let height = donor_rail::footprint(&rail)[1];
            assert!(at[1] >= rail_at[1] + height || at[1] + m.height <= rail_at[1]);
        }
        let size = iced::Size::new(app.viewport[0], app.viewport[1]);
        let viewport = iced::Rectangle::with_size(size);
        if vertical_spill {
            let rail_size = donor_rail::footprint(&rail);
            let folder =
                donor_rail::intent_center(&rail, donor_rail::RailIntent::ToggleFolder).unwrap();
            let hide = donor_rail::intent_center(&rail, donor_rail::RailIntent::Close).unwrap();
            assert!(
                (folder.x - hide.x).abs() > 30.0,
                "fixture must spill into other columns"
            );
            assert!(at[0] >= rail_at[0] + rail_size[0] || at[0] + m.width <= rail_at[0]);
            let open = {
                let mut root = view(&app, app.window.unwrap());
                let mut tree = Tree::new(&root);
                let node = root.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(size, size),
                );
                click(
                    &mut root,
                    &mut tree,
                    &node,
                    &renderer,
                    viewport,
                    iced::Point::new(rail_at[0] + folder.x, rail_at[1] + folder.y),
                )
                .into_iter()
                .find(|msg| {
                    matches!(
                        msg,
                        Message::DonorRail(donor_rail::RailIntent::ToggleFolder)
                    )
                })
                .expect("rendered Folder button")
            };
            let _ = update(&mut app, open);
            assert!(app.folder_menu_open);
        }
        // Empty no-folder fixtures have no rail anchor; direct existing sheets owner still composes a clearable All row.
        let mut element: Element<'_, Message> = if name == "empty" {
            iced::widget::container(view::composed_folder_popup_view(m.clone()))
                .padding(iced::Padding {
                    left: at[0],
                    top: at[1],
                    ..iced::Padding::ZERO
                })
                .into()
        } else {
            view(&app, app.window.unwrap())
        };
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        if vertical_spill {
            let hide = donor_rail::intent_center(&rail, donor_rail::RailIntent::Close).unwrap();
            let hits = click(
                &mut element,
                &mut tree,
                &node,
                &renderer,
                viewport,
                iced::Point::new(rail_at[0] + hide.x, rail_at[1] + hide.y),
            );
            assert!(
                hits.iter()
                    .any(|msg| matches!(msg, Message::DonorRail(donor_rail::RailIntent::Close))),
                "open popup must leave Hide reachable: {hits:?}"
            );
        }
        let path = output.join(format!("folder-{name}-top.png"));
        save(
            &mut element,
            &mut tree,
            &node,
            &mut renderer,
            viewport,
            iced::mouse::Cursor::Unavailable,
            &path,
        );
        let mut scroll = 0.0;
        let cursor = iced::Point::new(at[0] + m.width * 0.5, at[1] + m.height * 0.5);
        for (i, row) in m.rows.iter().enumerate() {
            let desired = (m.row_rect(i, 0.0).y - 4.0).clamp(0.0, m.max_scroll());
            let delta = scroll - desired;
            let messages = send(
                &mut element,
                &mut tree,
                &node,
                &renderer,
                viewport,
                cursor,
                iced::mouse::Event::WheelScrolled {
                    delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: delta },
                },
            );
            assert!(messages.is_empty());
            scroll = desired;
            let local = m.row_rect(i, scroll).center();
            let messages = click(
                &mut element,
                &mut tree,
                &node,
                &renderer,
                viewport,
                iced::Point::new(at[0] + local.x, at[1] + local.y),
            );
            assert!(
                messages
                    .iter()
                    .any(|message| matches!(message,Message::SetFolderFilter(p) if *p==row.path)),
                "{name} row{i} {:?}: {messages:?}",
                row.path
            );
        }
        if m.max_scroll() > 0.0 {
            let end = output.join(format!("folder-{name}-end.png"));
            save(
                &mut element,
                &mut tree,
                &node,
                &mut renderer,
                viewport,
                iced::mouse::Cursor::Unavailable,
                &end,
            );
            index.push(serde_json::json!({"path":end,"case":"folder-end","name":name,"rows":m.rows.len(),"scroll":scroll}));
        }
        let dismiss = if vertical_spill {
            let corner = if rtl { m.width - 0.5 } else { 0.5 };
            click(
                &mut element,
                &mut tree,
                &node,
                &renderer,
                viewport,
                iced::Point::new(at[0] + corner, at[1] + 0.5),
            )
            .into_iter()
            .find(|msg| matches!(msg, Message::DismissFolderMenu))
        } else {
            None
        };
        if vertical_spill {
            drop(element);
            let prior = app.filter_rule.folders.clone();
            let _ = update(
                &mut app,
                dismiss.expect("transparent corner routes to the outside-dismiss layer"),
            );
            assert!(!app.folder_menu_open && app.visible);
            assert_eq!(app.filter_rule.folders, prior);
            let _ = update(&mut app, Message::ToggleFolderMenu);
            d3_key(&mut app, keyboard::key::Named::Escape);
            assert!(!app.folder_menu_open && app.visible);
            assert_eq!(app.filter_rule.folders, prior);
        }
        index.push(serde_json::json!({"path":path,"case":"folder","name":name,"mode":format!("{mode:?}"),"scale":scale,"rtl":rtl,"rtl_qualification":if rtl{"forced descriptor branch, not public Arabic/native selection"}else{"fixture locale"},"anchor":at,"size":[m.width,m.height],"rows":m.rows.iter().map(|r|serde_json::json!({"path":r.path,"label":r.label,"depth":r.depth,"selected":r.selected,"unavailable":r.unavailable})).collect::<Vec<_>>(),"identity":m.identity,"every_row_actual_widget_hit":true,"provider_calls":calls.load(Ordering::Acquire)}));
    }
    for (name, random) in [
        ("off", Random::Off),
        ("on", Random::On),
        ("paused", Random::PausedForPlaylist),
        ("stopped", Random::SafelyStopped),
    ] {
        let (mut app, _) = d3_application_fixture(0);
        app.config.ui.show_top_bar = true;
        app.state.frontend_preferences.filter_bar.always_visible = Some(true);
        app.state.automation_intent = Some(AutomationIntent {
            random_enabled: random != Random::Off,
            playlist_playing: random == Random::PausedForPlaylist,
            safely_stopped: random == Random::SafelyStopped,
            ..Default::default()
        });
        let m = gallery_rail_model(&app);
        assert_eq!(m.random_state, random);
        let caption = "";
        for mode in [
            donor_rail::RailMode::Slices,
            donor_rail::RailMode::Geometric,
            donor_rail::RailMode::Wall,
        ] {
            for scale in [1.0, 0.5] {
                let mut narrow = m.clone();
                narrow.mode = mode;
                narrow.scale = scale;
                narrow.max_width = 616.0;
                assert_eq!(
                    donor_rail::random_painted_caption(&narrow).as_deref(),
                    Some(caption),
                    "{mode:?}/{scale}/{random:?}"
                );
            }
        }
        let size = iced::Size::new(app.viewport[0], app.viewport[1]);
        let viewport = iced::Rectangle::with_size(size);
        let mut element = view(&app, app.window.unwrap());
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let path = output.join(format!("random-{name}.png"));
        save(
            &mut element,
            &mut tree,
            &node,
            &mut renderer,
            viewport,
            iced::mouse::Cursor::Unavailable,
            &path,
        );
        index.push(serde_json::json!({"path":path,"case":"random","state":format!("{random:?}")}));
    }
    for (width, height, scale) in [
        (1920.0, 1080.0, 1.0),
        (2560.0, 1440.0, 0.5),
        (900.0, 720.0, 1.0),
    ] {
        let (mut app, calls) = d3_application_fixture(1);
        app.viewport = [width, height];
        app.launch_policy = p1_preview_policy();
        app.config.ui.show_top_bar = true;
        app.state.frontend_preferences.filter_bar.always_visible = Some(true);
        app.state.frontend_preferences.interface.ui_scale = Some(scale);
        app.monitor_ids = vec!["STABLE-A".into(), "STABLE-B".into()];
        app.active_monitor_id = Some("STABLE-B".into());
        // Actual rail Settings widget message, followed by actual mounted action widget.
        let rail = gallery_rail_model(&app);
        let at = view::gallery_rail_anchor(&app, &rail, app.state.gallery.depth.height);
        let local =
            donor_rail::intent_center(&rail, donor_rail::RailIntent::ToggleSettings).unwrap();
        let size = iced::Size::new(width, height);
        let viewport = iced::Rectangle::with_size(size);
        let messages = {
            let mut element = view(&app, app.window.unwrap());
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            click(
                &mut element,
                &mut tree,
                &node,
                &renderer,
                viewport,
                iced::Point::new(at[0] + local.x, at[1] + local.y),
            )
        };
        assert!(messages.iter().any(|m| matches!(
            m,
            Message::DonorRail(donor_rail::RailIntent::ToggleSettings)
        )));
        for message in messages {
            let _ = update(&mut app, message);
        }
        view::settings::controller::reconcile(&mut app);
        app.settings_workbench.folio_entrance = None;
        app.settings_workbench.tab_reveal = None;
        let navigation = {
            let mut element = view(&app, app.window.unwrap());
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            let p = probe(&mut element, &mut tree, &node, &renderer);
            let label = app.settings_locale.text_static("settings-tab-picker");
            let r = p
                .texts
                .iter()
                .find(|(t, _)| t == label)
                .expect("public Picker navigation")
                .1;
            click(
                &mut element,
                &mut tree,
                &node,
                &renderer,
                viewport,
                r.center(),
            )
        };
        assert!(navigation.iter().any(|m|matches!(m,Message::SettingsIntent{intent:Intent::Settings(SettingsMsg::SelectTab(tab)),..} if tab=="picker")));
        for message in navigation {
            let _ = update(&mut app, message);
        }
        view::settings::controller::reconcile(&mut app);
        app.settings_workbench.tab_reveal = None;
        let windows_section = view::settings::controller::cards(&app)
            .iter()
            .position(|(card, _)| {
                card.title == app.settings_locale.text_static("settings-section-windows")
            })
            .unwrap();
        p3_settings_intent(&mut app, SettingsMsg::SelectSection(windows_section));
        let messages = {
            let mut element = view(&app, app.window.unwrap());
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            let p = probe(&mut element, &mut tree, &node, &renderer);
            let mut result = Vec::new();
            let label = app.settings_locale.text_static("settings-tab-displays");
            for (_, r) in p.texts.iter().filter(|(t, _)| t == label) {
                let candidate = click(
                    &mut element,
                    &mut tree,
                    &node,
                    &renderer,
                    viewport,
                    r.center(),
                );
                if candidate.iter().any(|m| action(m, ActionId::OpenDisplays)) {
                    result = candidate;
                    break;
                }
            }
            let path = output.join(format!("windows-entry-{width}-{scale}.png"));
            save(
                &mut element,
                &mut tree,
                &node,
                &mut renderer,
                viewport,
                iced::mouse::Cursor::Unavailable,
                &path,
            );
            index.push(serde_json::json!({"path":path,"case":"windows-entry","actual_action_hit":!result.is_empty()}));
            result
        };
        assert!(messages.iter().any(|m| action(m, ActionId::OpenDisplays)));
        for message in messages {
            let _ = update(&mut app, message);
        }
        assert_eq!(app.settings_workbench.tab, "displays");
        let messages = {
            let mut element = view(&app, app.window.unwrap());
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            let p = probe(&mut element, &mut tree, &node, &renderer);
            assert!(p.texts.iter().any(|(t, _)| t.contains("STABLE-B")));
            let title = app.settings_locale.text_static("r5-panel-copy-12");
            let target = p
                .texts
                .iter()
                .find(|(t, _)| t == title)
                .expect("mounted cycle target")
                .1;
            let messages = click(
                &mut element,
                &mut tree,
                &node,
                &renderer,
                viewport,
                target.center(),
            );
            let path = output.join(format!("windows-target-{width}-{scale}.png"));
            save(
                &mut element,
                &mut tree,
                &node,
                &mut renderer,
                viewport,
                iced::mouse::Cursor::Unavailable,
                &path,
            );
            index.push(serde_json::json!({"path":path,"case":"windows-target","stable_ids":app.monitor_ids,"active_id":app.active_monitor_id,"actual_cycle_hit":messages.iter().any(|m|matches!(m,Message::SettingsPageAction{message,..} if matches!(**message,Message::CycleTargetMonitor)))}));
            messages
        };
        let cycle = messages.into_iter().find(|m| matches!(m, Message::SettingsPageAction { message, .. } if matches!(**message, Message::CycleTargetMonitor))).expect("actual mounted cycle callback");
        let previous_ids = app.monitor_ids.clone();
        super::super::operations::TARGET_MONITORS_FOR_TEST.with(|slot| {
            assert!(slot
                .borrow_mut()
                .replace(
                    previous_ids
                        .iter()
                        .map(|device_id| {
                            crate::native_wallpaper::NativeMonitor {
                                device_id: device_id.clone(),
                                rect: crate::monitor::DesktopRect {
                                    left: 0,
                                    top: 0,
                                    right: 1920,
                                    bottom: 1080,
                                },
                            }
                        })
                        .collect()
                )
                .is_none());
        });
        let _ = update(&mut app, cycle);
        super::super::operations::TARGET_MONITORS_FOR_TEST
            .with(|slot| assert!(slot.borrow().is_none()));
        assert_eq!(app.monitor_ids, previous_ids);
        assert_eq!(app.active_monitor_id.as_deref(), Some("STABLE-A"));
        index.last_mut().unwrap()["cycle_dispatch_active_id"] =
            serde_json::json!(app.active_monitor_id);
        assert_eq!(calls.load(Ordering::Acquire), 0);
        assert!(!app.launch_policy.allows_provider());
    }
    std::fs::write(
        output.join("PK04-CAPTURE-INDEX.json"),
        serde_json::to_vec_pretty(&index).unwrap(),
    )
    .unwrap();
}

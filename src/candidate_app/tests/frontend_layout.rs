//! Explicit owned headless captures. These never stand in for native input,
//! display/DPI observations, motion acceptance or human visual acceptance.
#[test]
#[ignore = "Owned Playlist composition, actual pointer hits and scroll reachability; explicit invocation"]
fn playlist_presentation_composed_surfaces_and_reachable_actions() {
    use crate::candidate_app::playlists::Edit;
    use iced::advanced::{
        clipboard, layout,
        renderer::{Headless, Renderer as _},
        widget::{operation, Operation, Tree},
        Layout, Shell,
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
    fn visible_delete(p: &Probe, label: &str) -> Option<iced::Rectangle> {
        let bounds = p.texts.iter().find(|(s, _)| s == label)?.1;
        let clip = p
            .scrolls
            .iter()
            .find(|(id, ..)| id == &Some(iced::widget::Id::new("playlist-definition")))
            .or_else(|| {
                p.scrolls
                    .iter()
                    .find(|(id, ..)| id == &Some(iced::widget::Id::new("playlist-reading")))
            })?
            .1;
        (clip.contains(bounds.center())
            && bounds.y >= clip.y
            && bounds.y + bounds.height <= clip.y + clip.height)
            .then_some(bounds)
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
    let mut receipts = Vec::new();
    // Each layout exercises all three surfaces; compact composition uses the existing responsive decision.
    for (width, height, scale, locale) in [
        (1920u32, 1080u32, 1.0, "en-US"),
        (1920, 1080, 0.5, "zh-CN"),
        (2560, 1440, 1.0, "zh-CN"),
        (2560, 1440, 0.5, "en-US"),
        (640, 720, 1.0, "en-US"),
        (960, 720, 0.5, "zh-CN"),
    ] {
        for case in [
            "picker-empty",
            "picker-overflow",
            "manager-empty",
            "manager-manual",
            "manager-overflow",
            "manager-smart",
            "manager-no-selection",
            "delete-active",
            "delete-other",
        ] {
            let (mut app, submissions) = d4a_fixture(3);
            app.viewport = [width as f32, height as f32];
            app.launch_policy = p1_preview_policy();
            app.settings_locale = localization::Catalog::new(locale, locale).unwrap();
            app.state.frontend_preferences.interface.ui_scale = Some(scale);
            iced::advanced::graphics::text::font_system()
                .write()
                .unwrap()
                .load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
            app.selected = Some(1);
            app.wallpapers[1].title =
                "Captured wallpaper / 森林海風星空與非常長的壁紙名稱".repeat(3);
            app.playlist_panel_open = true;
            app.playlist_picker = case.starts_with("picker");
            app.detail_open = app.playlist_picker;
            app.state.playlists.clear();
            app.state.active_playlist_id = None;
            if !case.ends_with("empty") {
                for n in 0..if case.ends_with("overflow") { 38 } else { 3 } {
                    app.state.playlists.push(Playlist {
                        id: format!("owned-{n}"),
                        name: format!(
                            "List {n:02} / 森林海風星空與非常長的播放列表名稱 {}",
                            "长名称".repeat(8)
                        ),
                        order: PlaybackOrder::Sequential,
                        membership: if case == "manager-smart"
                            || (case == "picker-overflow" && n == 1)
                        {
                            PlaylistMembership::Smart {
                                rule: app.filter_rule.clone(),
                            }
                        } else {
                            PlaylistMembership::Explicit {
                                wallpaper_ids: app
                                    .wallpapers
                                    .iter()
                                    .map(|w| w.id.clone())
                                    .collect(),
                            }
                        },
                    });
                }
                app.playlist_browsing_id = Some("owned-0".into());
                app.state.active_playlist_id = Some(
                    if case == "delete-active" {
                        "owned-0"
                    } else {
                        "owned-2"
                    }
                    .into(),
                );
            }
            if case == "manager-no-selection" {
                app.selected = None;
            }
            app.reconcile_playlist_editor();
            app.playlist_name = "Create captured / 新列表".into();
            if case.starts_with("delete") {
                app.state.schedules.push(candidate_state::Schedule {
                    id: "retained".into(),
                    playlist_id: "owned-0".into(),
                    ..Default::default()
                });
                let owner = app.playlist_owner().unwrap();
                let _ = update(
                    &mut app,
                    Message::PlaylistEdit(owner, Edit::DeleteRequest("owned-0".into())),
                );
            }
            let before = app.state.clone();
            let captured = app.selected;
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let style = application_style(&app, &theme);
            let size = iced::Size::new(width as f32, height as f32);
            let viewport = iced::Rectangle::with_size(size);
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            let initial = probe(&mut element, &mut tree, &node, &renderer);
            let in_view = |b: iced::Rectangle| {
                b.x >= -0.1
                    && b.y >= -0.1
                    && b.x + b.width <= width as f32 + 0.1
                    && b.y + b.height <= height as f32 + 0.1
            };
            let close = initial
                .texts
                .iter()
                .rev()
                .find(|(s, _)| s == "×")
                .expect("modal close")
                .1;
            assert!(in_view(close), "close {case} {width} {scale} {close:?}");
            let close_messages = click(
                &mut element,
                &mut tree,
                &node,
                &renderer,
                viewport,
                close.center(),
            );
            assert!(
                close_messages.iter().any(|m| matches!(
                    m,
                    Message::TogglePlaylistPanel
                        | Message::PlaylistEdit(_, Edit::Close | Edit::DeleteCancel)
                )),
                "close action {case}"
            );
            if case.starts_with("picker") {
                assert!(
                    initial.inputs.iter().any(|b| in_view(*b)),
                    "creation field remains visible"
                );
                let input = initial.inputs.iter().find(|b| in_view(**b)).unwrap();
                let _ = click(
                    &mut element,
                    &mut tree,
                    &node,
                    &renderer,
                    viewport,
                    input.center(),
                );
                let mut typing = Vec::new();
                element.as_widget_mut().update(
                    &mut tree,
                    &iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                        key: iced::keyboard::Key::Character("新".into()),
                        modified_key: iced::keyboard::Key::Character("新".into()),
                        physical_key: iced::keyboard::key::Physical::Unidentified(
                            iced::keyboard::key::NativeCode::Unidentified,
                        ),
                        location: iced::keyboard::Location::Standard,
                        modifiers: iced::keyboard::Modifiers::empty(),
                        text: Some("新".into()),
                        repeat: false,
                    }),
                    Layout::new(&node),
                    iced::mouse::Cursor::Unavailable,
                    &renderer,
                    &mut clipboard::Null,
                    &mut Shell::new(&mut typing),
                    &viewport,
                );
                assert!(typing.iter().any(|m| matches!(m, Message::PlaylistEdit(_, Edit::Name(name)) if name.contains('新'))), "clicked captured creation field receives text");
                let create = initial
                    .texts
                    .iter()
                    .find(|(s, _)| s == app.settings_locale.text_static("r5-create"))
                    .unwrap()
                    .1;
                assert!(in_view(create));
                assert!(click(
                    &mut element,
                    &mut tree,
                    &node,
                    &renderer,
                    viewport,
                    create.center()
                )
                .iter()
                .any(|m| matches!(m, Message::PlaylistEdit(_, Edit::Create))));
                assert!(
                    initial
                        .texts
                        .iter()
                        .any(|(s, b)| s == &app.wallpapers[1].title && in_view(*b)),
                    "full captured wallpaper title visible"
                );
                if case == "picker-overflow" {
                    let row = initial
                        .texts
                        .iter()
                        .find(|(s, _)| s == &app.state.playlists[0].name)
                        .unwrap()
                        .1;
                    assert!(click(&mut element,&mut tree,&node,&renderer,viewport,row.center()).iter().any(|m|matches!(m,Message::PlaylistEdit(_,Edit::Membership{playlist,..}) if playlist=="owned-0")));
                    assert!(
                        !initial
                            .texts
                            .iter()
                            .any(|(s, _)| s == &app.state.playlists[1].name),
                        "smart not manually editable"
                    );
                }
            } else if case.starts_with("delete") {
                for key in ["playlist-delete-cancel", "playlist-delete-record-only"] {
                    let b = initial
                        .texts
                        .iter()
                        .find(|(s, _)| s == app.settings_locale.text_static(key))
                        .unwrap()
                        .1;
                    assert!(in_view(b), "{key} {case}");
                }
                let cancel = initial
                    .texts
                    .iter()
                    .find(|(s, _)| s == app.settings_locale.text_static("playlist-delete-cancel"))
                    .unwrap()
                    .1;
                assert!(click(
                    &mut element,
                    &mut tree,
                    &node,
                    &renderer,
                    viewport,
                    cancel.center()
                )
                .iter()
                .any(|m| matches!(m, Message::PlaylistEdit(_, Edit::DeleteCancel))));
                let confirm = initial
                    .texts
                    .iter()
                    .rev()
                    .find(|(s, _)| s == app.settings_locale.text_static("playlist-delete-action"))
                    .unwrap()
                    .1;
                assert!(in_view(confirm));
                assert!(click(
                    &mut element,
                    &mut tree,
                    &node,
                    &renderer,
                    viewport,
                    confirm.center()
                )
                .iter()
                .any(|m| matches!(m, Message::PlaylistEdit(_, Edit::DeleteConfirm))));
                assert!(initial
                    .texts
                    .iter()
                    .any(|(s, b)| s == &app.state.playlists[0].name && in_view(*b)));
            } else {
                assert!(
                    initial.inputs.iter().any(|b| in_view(*b)),
                    "manager creation field"
                );
                if !case.ends_with("empty") {
                    let activate = initial
                        .texts
                        .iter()
                        .find(|(s, _)| s == app.settings_locale.text_static("r5-set-active"))
                        .unwrap()
                        .1;
                    let point = activate.center();
                    if in_view(activate)
                        && initial
                            .scrolls
                            .iter()
                            .filter(|(id, ..)| {
                                id == &Some(iced::widget::Id::new("playlist-definition"))
                            })
                            .all(|(_, b, ..)| b.contains(point))
                    {
                        assert!(click(&mut element, &mut tree, &node, &renderer, viewport, point).iter().any(|m| matches!(m, Message::SetActivePlaylist(id, _) if id == "owned-0")), "explicit rotation action keeps browsed target");
                    }
                }
            }
            let mut manager_delete_hit = false;
            let mut navigation_seen = std::collections::BTreeSet::new();
            let mut observe = |p: &Probe,
                               element: &mut Element<'_, Message>,
                               tree: &mut Tree,
                               renderer: &iced::Renderer| {
                if case.starts_with("manager") && !case.ends_with("empty") {
                    if let Some(bounds) =
                        visible_delete(p, app.settings_locale.text_static("playlist-delete-action"))
                    {
                        assert!(click(element,tree,&node,&renderer,viewport,bounds.center()).iter().any(|m|matches!(m,Message::PlaylistEdit(_,Edit::DeleteRequest(id)) if id=="owned-0")), "actual visible manager deletion hit {case} {width}/{scale}");
                        manager_delete_hit = true;
                    }
                    let nav = p
                        .scrolls
                        .iter()
                        .find(|(id, ..)| id == &Some(iced::widget::Id::new("playlist-navigation")))
                        .unwrap()
                        .1;
                    for (n, playlist) in app.state.playlists.iter().enumerate() {
                        if navigation_seen.contains(&playlist.id) {
                            continue;
                        }
                        let prefix = format!("List {n:02}");
                        if let Some((_, bounds)) = p.texts.iter().find(|(text, bounds)| {
                            text.starts_with(&prefix) && nav.contains(bounds.center())
                        }) {
                            assert!(click(element, tree, &node, renderer, viewport, bounds.center())
                                .iter().any(|m| matches!(m, Message::BrowsePlaylist(id) if id == &playlist.id)),
                                "visible fitted navigation row must hit {} at {width}/{scale}", playlist.id);
                            navigation_seen.insert(playlist.id.clone());
                        }
                    }
                }
            };
            observe(&initial, &mut element, &mut tree, &renderer);
            let mut frames = Vec::new();
            for end in [false, true] {
                if end {
                    // Wheel every overflow owner at its own viewport; fixed controls must remain reachable.
                    for (_, body, content, _) in &initial.scrolls {
                        if content.height <= body.height + 1.0 {
                            continue;
                        }
                        for _ in
                            0..((content.height / (body.height.max(1.0) * 0.6)).ceil() as usize + 2)
                        {
                            let mut messages = Vec::new();
                            element.as_widget_mut().update(
                                &mut tree,
                                &iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                                    delta: iced::mouse::ScrollDelta::Pixels {
                                        x: 0.0,
                                        y: -body.height * 0.6,
                                    },
                                }),
                                Layout::new(&node),
                                iced::mouse::Cursor::Available(body.center()),
                                &renderer,
                                &mut clipboard::Null,
                                &mut Shell::new(&mut messages),
                                &viewport,
                            );
                            let step = probe(&mut element, &mut tree, &node, &renderer);
                            observe(&step, &mut element, &mut tree, &renderer);
                        }
                    }
                }
                let actual = probe(&mut element, &mut tree, &node, &renderer);
                assert!(actual
                    .texts
                    .iter()
                    .all(|(_, b)| b.width.is_finite() && b.height.is_finite()));
                if case == "picker-overflow" {
                    let body = actual
                        .scrolls
                        .iter()
                        .find(|(id, ..)| {
                            id == &Some(iced::widget::Id::new("playlist-picker-memberships"))
                        })
                        .unwrap()
                        .1;
                    for playlist in &app.state.playlists {
                        if !matches!(playlist.membership, PlaylistMembership::Explicit { .. }) {
                            continue;
                        }
                        let b = actual
                            .texts
                            .iter()
                            .find(|(s, _)| s == &playlist.name)
                            .unwrap()
                            .1;
                        assert!(
                            b.x + b.width <= body.x + body.width - 12.0 + 0.1,
                            "membership reserves track {width}/{scale} end={end}: {b:?} body {body:?}"
                        );
                    }
                }
                if end && case == "picker-overflow" {
                    let last = &app.state.playlists.last().unwrap();
                    let b = actual
                        .texts
                        .iter()
                        .find(|(s, _)| s == &last.name)
                        .unwrap()
                        .1;
                    let body = actual
                        .scrolls
                        .iter()
                        .find(|(id, ..)| {
                            id == &Some(iced::widget::Id::new("playlist-picker-memberships"))
                        })
                        .unwrap()
                        .1;
                    assert!(
                        body.contains(b.center()),
                        "last membership reachable {width}/{scale}: {b:?} body {body:?}"
                    );
                    assert!(click(&mut element,&mut tree,&node,&renderer,viewport,b.center()).iter().any(|m|matches!(m,Message::PlaylistEdit(_,Edit::Membership{playlist,..}) if playlist==&last.id)));
                    assert!(actual
                        .texts
                        .iter()
                        .any(|(s, b)| s == app.settings_locale.text_static("r5-create")
                            && in_view(*b)));
                }
                if case.starts_with("manager") && !case.ends_with("empty") {
                    let body = actual
                        .scrolls
                        .iter()
                        .find(|(id, ..)| id == &Some(iced::widget::Id::new("playlist-definition")))
                        .or_else(|| {
                            actual.scrolls.iter().find(|(id, ..)| {
                                id == &Some(iced::widget::Id::new("playlist-reading"))
                            })
                        })
                        .unwrap()
                        .1;
                    let definition = actual
                        .containers
                        .iter()
                        .find(|(id, _)| {
                            id == &Some(iced::widget::Id::new("playlist-selected-title"))
                        })
                        .expect("mounted selected playlist title")
                        .1;
                    assert!(
                        definition.x + definition.width <= body.x + body.width - 5.0,
                        "definition reserves track {width}/{scale} {definition:?} {body:?}"
                    );
                    let selected = actual.texts.iter().find(|(s, b)| {
                        s.starts_with(app.settings_locale.text_static("r5-selected-wallpaper"))
                            && b.x >= body.x
                    });
                    if let Some((_, b)) = selected {
                        assert!(
                            b.x + b.width <= body.x + body.width - 5.0,
                            "captured identity reserves track"
                        );
                    }
                    let art = actual
                        .containers
                        .iter()
                        .find(|(id, _)| id == &Some(iced::widget::Id::new("playlist-art")))
                        .unwrap()
                        .1;
                    let text = actual
                        .texts
                        .iter()
                        .find(|(s, _)| s == &app.catalog[0].title)
                        .unwrap()
                        .1;
                    assert!(
                        (art.width * 1.5 - text.width).abs() < 0.2,
                        "thumbnail horizontal reduction {art:?}/{text:?}"
                    );
                    assert!(
                        (art.height - (158.0 * scale as f32 * 2.0 / 3.0)).abs() < 0.2,
                        "thumbnail vertical reduction"
                    );
                }
                renderer.reset(viewport);
                element.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &theme,
                    &iced::advanced::renderer::Style {
                        text_color: style.text_color,
                    },
                    Layout::new(&node),
                    iced::mouse::Cursor::Unavailable,
                    &viewport,
                );
                let bytes = Headless::screenshot(
                    &mut renderer,
                    iced::Size::new(width, height),
                    1.0,
                    style.background_color,
                );
                let path = output.join(format!(
                    "playlist-{case}-{width}x{height}-{scale}-{locale}-{}.png",
                    if end { "end" } else { "top" }
                ));
                ::image::save_buffer_with_format(
                    &path,
                    &bytes,
                    width,
                    height,
                    ::image::ColorType::Rgba8,
                    ::image::ImageFormat::Png,
                )
                .unwrap();
                frames.push(serde_json::json!({"path":path,"texts":actual.texts.iter().map(|(s,b)|serde_json::json!({"text":s,"bounds":[b.x,b.y,b.width,b.height]})).collect::<Vec<_>>(),"scrolls":format!("{:?}",actual.scrolls)}));
            }
            drop(observe);
            if case.starts_with("manager") && !case.ends_with("empty") {
                assert!(manager_delete_hit,"manager Delete reachable through actual viewport traversal {case} {width}/{scale}");
                assert_eq!(
                    navigation_seen.len(),
                    app.state.playlists.len(),
                    "all navigation rows reached {case} {width}/{scale}"
                );
            }
            drop(element);
            assert_eq!(app.state, before, "layout and captured hits do not publish");
            assert_eq!(app.selected, captured);
            assert_eq!(submissions.load(Ordering::Acquire), 0);
            receipts.push(serde_json::json!({"case":case,"width":width,"height":height,"scale":scale,"locale":locale,"frames":frames,"provider_submissions":0}));
        }
    }
    std::fs::write(
        output.join("PLAYLIST-COMPOSITIONS.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
}

use super::*;
use crate::donor_folio;
use view::settings::{
    composed::{self, Control},
    folio::intent::SettingsMsg,
};

#[test]
fn e1_combined_visibility_hydrates_publishes_reloads_and_preserves_tag_rows() {
    use view::settings::{
        bridge::Source,
        composed::{draft::Value, SettingsSource},
        preference_projection,
    };
    let mut app = p3_composed_editor_app(SettingsCategory::CatalogFilters);
    for (search, tags, expected) in [
        (false, false, false),
        (false, true, true),
        (true, false, true),
        (true, true, true),
    ] {
        app.state.frontend_preferences.visibility.search = Some(search);
        app.state.frontend_preferences.visibility.tagcloud = Some(tags);
        assert_eq!(
            Source { app: &app }.flag_default_true("filterBar.show.search"),
            expected
        );
        assert_eq!(gallery_rail_model(&app).visibility.search, expected);
    }
    app.state.frontend_preferences.tag_panel.rows = Some("3".into());
    p3_settings_intent(&mut app, SettingsMsg::SelectTab("filter".into()));
    for enabled in [false, true] {
        p3_settings_intent(
            &mut app,
            SettingsMsg::Toggle("filterBar.show.search".into(), enabled),
        );
        let saved = CandidateState::load(&app.state_path);
        assert_eq!(saved.frontend_preferences.visibility.search, Some(enabled));
        assert_eq!(
            saved.frontend_preferences.visibility.tagcloud,
            Some(enabled)
        );
        assert_eq!(
            saved.frontend_preferences.tag_panel.rows.as_deref(),
            Some("3")
        );
        assert_eq!(
            preference_projection::read(&saved.frontend_preferences, "filterBar.show.search"),
            Some(Value::Boolean(enabled))
        );
        app.state = saved;
    }
    assert_eq!(app.search_panel_row_limit(), 3);
    assert!(!app.apply_pending);
}

#[test]
fn e1_effective_scale_is_draft_first_for_rail_folio_and_drawer() {
    use view::settings::composed::draft::Value;
    let (mut app, _) = d4a_fixture(3);
    app.viewport = [1920.0, 1080.0];
    app.state.frontend_preferences.interface.ui_scale = Some(1.0);
    let before = app.search_panel_width();
    app.settings_workbench
        .draft
        .values
        .insert("general.uiScale".into(), Value::Number(1.25));
    let scale = view::settings::bridge::effective_ui_scale(&app);
    assert_eq!(scale, 1.25);
    assert_eq!(gallery_rail_model(&app).scale, scale);
    let [width, height, index] = donor_folio::dimensions(app.viewport, scale);
    assert!(width <= app.viewport[0] && height <= app.viewport[1] && index < width);
    let after = app.search_panel_width();
    assert!(after > before);
    assert_eq!(app.state.frontend_preferences.interface.ui_scale, Some(1.0));
    app.settings_workbench.draft.values.clear();
    assert_eq!(view::settings::bridge::effective_ui_scale(&app), 1.0);
}

#[test]
fn e1_rail_anchor_follows_current_slices_and_hex_through_morph_reversal() {
    for mode in [GalleryMode::Slices, GalleryMode::Geometric] {
        let (mut app, _) = d4a_fixture(20);
        app.viewport = [1920.0, 1080.0];
        app.gallery_mode = mode;
        app.sync_gallery_presentation();
        let initial = if mode == GalleryMode::Slices {
            app.state.gallery.slices.height
        } else {
            app.state.gallery.geometric.radius
        };
        for target in [initial + 180.0, initial] {
            if mode == GalleryMode::Slices {
                app.state.gallery.slices.height = target;
            } else {
                app.state.gallery.geometric.radius = target;
            }
            app.sync_gallery_presentation();
            for dt in [0.0, 0.016, 0.05] {
                let height = if mode == GalleryMode::Slices {
                    let p = app.gallery_presentation.as_mut().unwrap();
                    p.tick(dt);
                    p.slices.height
                } else {
                    let p = app.hex_presentation.as_mut().unwrap();
                    p.tick(dt);
                    p.params.content_h()
                };
                let model = gallery_rail_model(&app);
                let actual = view::gallery_rail_anchor(&app, &model, 999.0);
                let expected = donor_rail::anchor(
                    model.mode,
                    app.viewport,
                    donor_rail::footprint(&model),
                    height,
                );
                assert_eq!(actual, expected, "{mode:?} current height {height}");
            }
        }
    }
}

#[test]
#[ignore = "E1 owned production root layouts and real input contracts; explicit invocation only"]
fn e1_owned_root_layouts_and_no_selection_engine_input() {
    use iced::advanced::{
        clipboard, layout,
        renderer::{Headless, Renderer as _},
        widget::{operation, Operation, Tree},
        Layout, Shell,
    };
    #[derive(Default)]
    struct Probe {
        offset: iced::Vector,
        pending: Option<iced::Vector>,
        texts: Vec<(String, iced::Rectangle)>,
        containers: Vec<(Option<iced::widget::Id>, iced::Rectangle)>,
        scrolls: Vec<(iced::Rectangle, iced::Rectangle)>,
        inputs: Vec<(Option<iced::widget::Id>, iced::Rectangle)>,
    }
    impl Operation for Probe {
        fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
            self.containers.push((id.cloned(), bounds - self.offset));
        }
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
            let before = self.offset;
            if let Some(delta) = self.pending.take() {
                self.offset += delta;
            }
            visit(self);
            self.offset = before;
        }
        fn scrollable(
            &mut self,
            _: Option<&iced::widget::Id>,
            bounds: iced::Rectangle,
            content: iced::Rectangle,
            translation: iced::Vector,
            _: &mut dyn operation::Scrollable,
        ) {
            self.scrolls.push((bounds - self.offset, content));
            self.pending = Some(translation);
        }
        fn text_input(
            &mut self,
            id: Option<&iced::widget::Id>,
            bounds: iced::Rectangle,
            _: &mut dyn operation::TextInput,
        ) {
            self.inputs.push((id.cloned(), bounds - self.offset));
        }
        fn text(&mut self, _: Option<&iced::widget::Id>, bounds: iced::Rectangle, value: &str) {
            self.texts.push((value.into(), bounds - self.offset));
        }
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
            iced::mouse::Event::CursorMoved { position: point },
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
    fn root_click(app: &GpuCandidate, renderer: &iced::Renderer, label: &str) -> Vec<Message> {
        let size = iced::Size::new(app.viewport[0], app.viewport[1]);
        let viewport = iced::Rectangle::with_size(size);
        let mut element = view(app, app.window.unwrap());
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, renderer, &layout::Limits::new(size, size));
        let mut probe = Probe::default();
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), renderer, &mut probe);
        let point = probe
            .texts
            .iter()
            .filter(|(text, b)| text == label && viewport.contains(b.center()))
            .min_by(|a, b| a.1.y.total_cmp(&b.1.y))
            .unwrap_or_else(|| panic!("missing visible {label}: {:?}", probe.texts))
            .1
            .center();
        click(&mut element, &mut tree, &node, renderer, viewport, point)
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
    let mut receipts = Vec::new();
    for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
        for case in [
            "rail-slices",
            "rail-hex",
            "rail-wall",
            "rail-depth",
            "search",
            "search-collapsed",
            "playlist-empty",
            "playlist-no-match",
            "playlist-stale",
            "library",
            "theme-copy",
            "theme-designer",
            "engine",
            "folder",
            "search-scale",
        ] {
            if case == "search-scale" && width != 1920 {
                continue;
            }
            let (mut app, submissions) = d4a_fixture(if case == "playlist-empty" { 0 } else { 3 });
            app.viewport = [width as f32, height as f32];
            app.launch_policy = p1_preview_policy();
            app.rail_reveal = 1.0;
            app.state.frontend_preferences.filter_bar.always_visible = Some(true);
            app.config.ui.show_top_bar = true;
            app.config.ui.show_title = false;
            app.selected = (!app.wallpapers.is_empty()).then_some(0);
            {
                let mut fonts = iced::advanced::graphics::text::font_system()
                    .write()
                    .unwrap();
                fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                fonts.load_font(std::borrow::Cow::Borrowed(
                    include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
                ));
            }
            if case.starts_with("rail-") {
                app.gallery_mode = match case {
                    "rail-hex" => GalleryMode::Geometric,
                    "rail-wall" => GalleryMode::Wall,
                    "rail-depth" => GalleryMode::Depth,
                    _ => GalleryMode::Slices,
                };
                app.sync_gallery_presentation();
            } else if case.starts_with("search") {
                if case == "search-scale" {
                    app.state.frontend_preferences.interface.ui_scale = Some(1.25);
                }
                app.search_tags_open = false;
                let _ = update(&mut app, Message::OpenSearch);
                if case != "search-collapsed" {
                    let _ = update(&mut app, Message::ToggleSearchTags);
                }
                app.catalog[0].source_tags =
                    (0..36).map(|i| format!("E1 local tag {i:02}")).collect();
                app.project_local_tags();
                app.reconcile_tag_editor();
            } else if case.starts_with("playlist") {
                let count = match case {
                    "playlist-empty" => 0,
                    "playlist-no-match" => 1,
                    _ => 2,
                };
                app.state.playlists.clear();
                app.state.active_playlist_id = None;
                for n in 0..count {
                    app.state.playlists.push(Playlist {
                        id: format!("e1-{n}"),
                        name: format!("E1 list {n}"),
                        order: PlaybackOrder::Sequential,
                        membership: PlaylistMembership::Explicit {
                            wallpaper_ids: app.wallpapers.iter().map(|w| w.id.clone()).collect(),
                        },
                    });
                }
                if case == "playlist-no-match" {
                    let _ = update(
                        &mut app,
                        Message::SearchChanged("E1-no-such-wallpaper".into()),
                    );
                    assert!(app.filtered_indices.is_empty());
                } else if case == "playlist-stale" {
                    app.selected = Some(usize::MAX);
                }
                let _ = update(&mut app, Message::TogglePlaylistPanel);
                let _ = update(
                    &mut app,
                    Message::PlaylistNameChanged("E1 created without selection".into()),
                );
                assert!(sheet_owns_keyboard(
                    app.panel_state,
                    app.playlist_panel_open,
                    app.engine_properties_open,
                    app.controls_panel_open,
                    app.folder_menu_open
                ));
            } else if case == "engine" {
                app.engine_properties_open = true;
                r5_schema_fixture(&mut app);
                let property = app
                    .engine_properties
                    .iter_mut()
                    .find(|p| p.key == "mode")
                    .unwrap();
                property.kind = engine_properties::EnginePropertyKind::Combo {
                    options: vec![
                        engine_properties::EnginePropertyOption {
                            label: "Same".into(),
                            value: serde_json::json!(7.125),
                        },
                        engine_properties::EnginePropertyOption {
                            label: "Same".into(),
                            value: serde_json::json!(0.0625),
                        },
                    ],
                };
                app.engine_property_values
                    .insert("mode".into(), serde_json::json!(7.125));
            } else if case == "folder" {
                app.folder_menu_open = true;
            } else {
                let _ = update(&mut app, Message::ToggleSettingsPanel);
                p3_settings_intent(
                    &mut app,
                    SettingsMsg::SelectTab(
                        if case == "library" {
                            "library"
                        } else {
                            "theme"
                        }
                        .into(),
                    ),
                );
                if case == "library" {
                    p3_settings_intent(
                        &mut app,
                        SettingsMsg::Run(composed::ActionId::OpenLibraryTools),
                    );
                    assert_eq!(
                        app.settings_workbench.windows_panel,
                        Some(SettingsCategory::Library)
                    );
                    app.colours.status = "E1 owned automatic-analysis diagnostic".into();
                }
                if case == "theme-designer" {
                    let _ = update(
                        &mut app,
                        Message::SetSettingsCategory(SettingsCategory::Theme),
                    );
                }
                app.settings_workbench.folio_entrance = None;
                app.settings_workbench.tab_reveal = None;
            }
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let style = application_style(&app, &theme);
            let size = iced::Size::new(width as f32, height as f32);
            let viewport = iced::Rectangle::with_size(size);
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            assert_eq!(node.size(), size);
            let mut probe = Probe::default();
            element
                .as_widget_mut()
                .operate(&mut tree, Layout::new(&node), &renderer, &mut probe);
            assert!(probe
                .texts
                .iter()
                .all(|(_, r)| r.width.is_finite() && r.height.is_finite()));
            if case.starts_with("playlist") {
                assert!(probe
                    .texts
                    .iter()
                    .any(|(text, _)| text == app.settings_locale.text_static("r5-new-playlist")));
            }
            if case.starts_with("search") {
                let field = probe
                    .containers
                    .iter()
                    .find(|(id, _)| id.as_ref() == Some(&iced::widget::Id::new("search-tag-field")))
                    .unwrap()
                    .1;
                assert!(
                    (field.height - 46.0 * view::settings::bridge::effective_ui_scale(&app)).abs()
                        < 0.01
                );
                let panel = probe
                    .containers
                    .iter()
                    .find(|(id, _)| id.as_ref() == Some(&iced::widget::Id::new("search-tag-panel")))
                    .unwrap()
                    .1;
                let help = probe
                    .texts
                    .iter()
                    .find(|(text, _)| {
                        text == app.settings_locale.text_static("search-tag-counts-help")
                    })
                    .unwrap()
                    .1;
                let minimum_line = 9.0
                    * 1.25
                    * crate::donor_folio::source_ui::legible_type_scale(
                        view::settings::bridge::effective_ui_scale(&app),
                    );
                assert!(
                    help.height >= minimum_line - 0.1,
                    "collapsed help is truncated: {help:?}"
                );
                assert!(help.y >= panel.y && help.y + help.height <= panel.y + panel.height);
            }
            if case == "engine" {
                assert_eq!(
                    probe.texts.iter().filter(|(text, _)| text == "R").count(),
                    1,
                    "invalid raw colour must not acquire white RGB sliders"
                );
            }
            let cursor = if case == "rail-hex" {
                let model = gallery_rail_model(&app);
                let at = view::gallery_rail_anchor(&app, &model, 0.0);
                let point = donor_rail::swatch_center(&model, 0).unwrap();
                iced::mouse::Cursor::Available(iced::Point::new(at[0] + point.x, at[1] + point.y))
            } else {
                iced::mouse::Cursor::Unavailable
            };
            renderer.reset(viewport);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: style.text_color,
                },
                Layout::new(&node),
                cursor,
                &viewport,
            );
            let bytes = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                style.background_color,
            );
            let path = output.join(format!("e1-{width}x{height}-{case}.png"));
            ::image::save_buffer_with_format(
                &path,
                &bytes,
                width,
                height,
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
            if case == "library" || case == "search-scale" {
                let pane = probe
                    .scrolls
                    .iter()
                    .filter(|(b, c)| c.height > b.height + 1.0)
                    .max_by(|a, b| a.0.width.total_cmp(&b.0.width))
                    .unwrap()
                    .0;
                for _ in 0..30 {
                    let mut messages = Vec::new();
                    element.as_widget_mut().update(
                        &mut tree,
                        &iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                            delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: -150.0 },
                        }),
                        Layout::new(&node),
                        iced::mouse::Cursor::Available(pane.center()),
                        &renderer,
                        &mut clipboard::Null,
                        &mut Shell::new(&mut messages),
                        &viewport,
                    );
                }
                let mut end = Probe::default();
                element
                    .as_widget_mut()
                    .operate(&mut tree, Layout::new(&node), &renderer, &mut end);
                if case == "search-scale" {
                    let editor = end
                        .inputs
                        .iter()
                        .find(|(id, _)| id.as_ref() == Some(&app.local_tag_input_id))
                        .unwrap()
                        .1;
                    assert!(editor.y >= pane.y && editor.y + editor.height <= pane.y + pane.height);
                } else {
                    assert!(
                        end.texts.iter().any(|(text, b)| text == &app.colours.status
                            && b.y >= pane.y
                            // Iced settles the scroll offset to physical pixels.
                            && b.y + b.height <= pane.y + pane.height + 0.5),
                        "diagnostic {:?} not visible inside {:?}: {:?}",
                        app.colours.status,
                        pane,
                        end.texts
                    );
                }
                renderer.reset(viewport);
                element.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &theme,
                    &iced::advanced::renderer::Style {
                        text_color: style.text_color,
                    },
                    Layout::new(&node),
                    iced::mouse::Cursor::Unavailable,
                    &viewport,
                );
                let bytes = Headless::screenshot(
                    &mut renderer,
                    iced::Size::new(width, height),
                    1.0,
                    style.background_color,
                );
                ::image::save_buffer_with_format(
                    output.join(format!("e1-{width}x{height}-{case}-end.png")),
                    &bytes,
                    width,
                    height,
                    ::image::ColorType::Rgba8,
                    ::image::ImageFormat::Png,
                )
                .unwrap();
            }
            drop(element);
            if case.starts_with("playlist") {
                let before = app.state.playlists.len();
                let label = app.settings_locale.text_static("r5-create");
                let messages = root_click(&app, &renderer, label);
                assert!(messages
                    .iter()
                    .any(|m| matches!(m, Message::CreatePlaylist { smart: false })));
                for message in messages {
                    let _ = update(&mut app, message);
                }
                assert_eq!(app.state.playlists.len(), before + 1);
                assert_eq!(app.state.active_playlist_id, None);
                assert!(
                    matches!(&app.state.playlists.last().unwrap().membership,PlaylistMembership::Explicit {wallpaper_ids} if wallpaper_ids.is_empty())
                );
                for key in ["r5-sequential", "r5-set-active"] {
                    let messages =
                        root_click(&app, &renderer, app.settings_locale.text_static(key));
                    assert!(messages.iter().any(|m| matches!(
                        m,
                        Message::TogglePlaylistOrder(_) | Message::SetActivePlaylist(..)
                    )));
                    for message in messages {
                        let _ = update(&mut app, message);
                    }
                }
                assert!(app.state.active_playlist_id.is_some());
                let messages = root_click(&app, &renderer, "×");
                assert!(messages
                    .iter()
                    .any(|m| matches!(m, Message::TogglePlaylistPanel)));
                for message in messages {
                    let _ = update(&mut app, message);
                }
                assert!(!app.playlist_panel_open);
            }
            if case == "engine" {
                let messages = root_click(&app, &renderer, "Same (2)");
                assert!(messages.iter().any(|m|matches!(m,Message::EnginePropertyValueChanged(key,value) if key=="mode" && *value==serde_json::json!(0.0625))));
                for message in messages {
                    let _ = update(&mut app, message);
                }
                assert_eq!(
                    app.engine_property_values["mode"],
                    serde_json::json!(0.0625)
                );
                let mut element = view(&app, window);
                let mut tree = Tree::new(&element);
                let node = element.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(size, size),
                );
                let mut probe = Probe::default();
                element.as_widget_mut().operate(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &mut probe,
                );
                let r = probe.texts.iter().find(|(text, _)| text == "R").unwrap().1;
                let number = probe
                    .texts
                    .iter()
                    .find(|(text, b)| text == "0.200" && (b.center_y() - r.center_y()).abs() < 10.0)
                    .unwrap()
                    .1;
                let messages = click(
                    &mut element,
                    &mut tree,
                    &node,
                    &renderer,
                    viewport,
                    iced::Point::new((r.x + r.width + number.x) * 0.5, r.center_y()),
                );
                drop(element);
                let raw = messages
                    .iter()
                    .find_map(|m| {
                        if let Message::EnginePropertyTextChanged(key, value) = m {
                            (key == "tint").then_some(value.clone())
                        } else {
                            None
                        }
                    })
                    .expect("actual RGB slider draft message");
                let rgb = engine_properties::color_preview_rgb(&raw).unwrap();
                assert!((rgb[0] - 0.5).abs() < 0.05);
                assert_eq!([rgb[1], rgb[2]], [0.4, 0.8]);
                for message in messages {
                    let _ = update(&mut app, message);
                }
                assert_eq!(
                    app.engine_property_values["invalid"],
                    serde_json::json!("#invalid")
                );
            }
            if case == "library" {
                let messages = root_click(
                    &app,
                    &renderer,
                    app.settings_locale.text_static("r5-back-settings"),
                );
                assert!(messages
                    .iter()
                    .any(|m| matches!(m, Message::SettingsWindowsClose { .. })));
                for message in messages {
                    let _ = update(&mut app, message);
                }
                assert_eq!(app.settings_workbench.windows_panel, None);
            }
            assert_eq!(submissions.load(Ordering::Acquire), 0);
            assert!(!app.state.slideshow_enabled);
            assert!(!app.apply_pending);
            receipts.push(serde_json::json!({"case":case,"width":width,"height":height,"scale":view::settings::bridge::effective_ui_scale(&app),"path":path,"texts":probe.texts.iter().map(|(text,b)|serde_json::json!({"text":text,"bounds":[b.x,b.y,b.width,b.height]})).collect::<Vec<_>>(),"providerSubmissions":0}));
        }
    }
    std::fs::write(
        output.join("E1-LAYOUTS-AND-INPUT.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "R3 owned production rail/menu layouts; explicit invocation only"]
fn rail_owned_composed_layout_draw_and_input() {
    use iced::advanced::{
        clipboard, layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout, Shell,
    };
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
        fonts.load_font(std::borrow::Cow::Borrowed(
            localization::Catalog::new("en-US", "en-US")
                .unwrap()
                .font_bytes(),
        ));
        fonts.load_font(std::borrow::Cow::Borrowed(
            include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
        ));
    }
    let mut captures = Vec::new();
    for (width, height) in [(1920u32, 1080u32), (2560, 1440), (640, 720), (640, 512)] {
        for scale in [1.0, 0.5] {
            for (mode, name, vertical, menu) in [
                (GalleryMode::Slices, "slices", false, false),
                (GalleryMode::Geometric, "hex", false, false),
                (GalleryMode::Wall, "wall", false, false),
                (GalleryMode::Depth, "depth", false, false),
                (GalleryMode::Slices, "folder-menu", false, true),
                (GalleryMode::Slices, "rtl", false, false),
                (GalleryMode::Slices, "visibility", false, false),
                (GalleryMode::Slices, "folder-overflow", false, true),
                (GalleryMode::Slices, "tooltip-sort", true, false),
                (GalleryMode::Slices, "tooltip-folder", true, false),
                (GalleryMode::Slices, "vertical-match-slices", true, false),
                (GalleryMode::Geometric, "vertical-match-hex", true, false),
                (GalleryMode::Wall, "vertical-match-wall", true, false),
                (GalleryMode::Depth, "vertical-match-depth", true, false),
            ] {
                if ["rtl", "visibility", "folder-overflow"].contains(&name)
                    && (width, height) != (640, 720)
                {
                    continue;
                }
                if name.starts_with("tooltip-") && (width, height) != (640, 512) {
                    continue;
                }
                if name.starts_with("vertical-match-") && (width, height) != (640, 512) {
                    continue;
                }
                let vertical = vertical || height == 512;
                let (mut app, _) = d3_application_fixture(2);
                app.viewport = [width as f32, height as f32];
                app.launch_policy = p1_preview_policy();
                app.config.ui.show_top_bar = true;
                app.config.ui.show_title = false;
                app.config.ui.show_monitor_tabs = true;
                app.config.ui.show_close_button = true;
                app.rail_reveal = 1.0;
                app.gallery_mode = mode;
                app.state.frontend_preferences.interface.ui_scale = Some(scale);
                app.state.frontend_preferences.filter_bar.always_visible = Some(true);
                app.folder_menu_open = menu;
                app.monitor_ids = vec!["INJECTED-physical-display-key".into()];
                app.active_monitor_id = Some(app.monitor_ids[0].clone());
                if vertical {
                    app.state.frontend_preferences.filter_bar.orientation = Some("vertical".into());
                    app.state.frontend_preferences.filter_bar.visual_style =
                        if name.starts_with("vertical-match-") {
                            None
                        } else {
                            Some("hex".into())
                        };
                    app.state.frontend_preferences.interface.ui_scale = Some(scale);
                }
                if name == "rtl" {
                    // Public locale selection intentionally remains en/zh; force only branch coverage.
                    app.settings_locale.language = localization::LANGUAGES
                        .iter()
                        .find(|l| l.tag == "ar-SA")
                        .unwrap();
                    assert!(app.settings_locale.language.rtl);
                }
                if name == "visibility" {
                    app.state.frontend_preferences.visibility.sort = Some(false);
                    app.state.frontend_preferences.visibility.resolution = Some(false);
                    app.filter_rule.minimum_width = 1920;
                }
                if name == "folder-overflow" {
                    app.config.catalog.local_folders = (0..40)
                        .map(|i| std::path::PathBuf::from(format!("J:/owned-folder-{i:02}/音楽")))
                        .collect();
                }
                if width <= 960 {
                    app.filter_rule
                        .folders
                        .insert(std::path::PathBuf::from(format!(
                            "J:/owned-fixture/{}",
                            "Long folder label 音楽 ".repeat(8),
                        )));
                }
                if name == "tooltip-folder" {
                    app.filter_rule.folders.insert(
                        format!("J:/first/{}", "Long root 音楽e\u{301} ".repeat(10)).into(),
                    );
                    app.filter_rule.folders.insert(
                        format!("J:/second/{}", "Long root 音楽e\u{301} ".repeat(10)).into(),
                    );
                }
                let mut fonts = iced::advanced::graphics::text::font_system()
                    .write()
                    .unwrap();
                fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                fonts.load_font(std::borrow::Cow::Borrowed(
                    include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
                ));
                drop(fonts);
                let model = gallery_rail_model(&app);
                if name.starts_with("vertical-match-") {
                    let expected = match mode {
                        GalleryMode::Slices | GalleryMode::Depth => donor_rail::RailMode::Slices,
                        GalleryMode::Geometric => donor_rail::RailMode::Geometric,
                        GalleryMode::Wall => donor_rail::RailMode::Wall,
                    };
                    assert_eq!(
                        model.mode, expected,
                        "default Match must exercise the actual mode skin"
                    );
                }
                let footprint = donor_rail::footprint(&model);
                let gallery_height = match mode {
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
                    GalleryMode::Slices => app.state.gallery.slices.height,
                };
                let at = view::gallery_rail_anchor(&app, &model, gallery_height);
                assert!(footprint[0] <= width as f32 && footprint[1] <= height as f32);
                assert!(
                    at[0] + footprint[0] <= width as f32 + 0.01
                        && at[1] + footprint[1] <= height as f32 + 0.01,
                    "final rail items must be inside the production viewport"
                );
                let window = app.window.unwrap();
                let theme = theme(&app, window);
                let size = iced::Size::new(width as f32, height as f32);
                let bounds = iced::Rectangle::with_size(size);
                renderer.reset(bounds);
                let mut element = view(&app, window);
                let mut tree = Tree::new(&element);
                let node = element.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(size, size),
                );
                assert_eq!(node.size(), size);
                // Hover the first retained type control, using actual production anchor.
                let local_hover = if name == "tooltip-sort" {
                    donor_rail::intent_center(
                        &model,
                        donor_rail::RailIntent::SetSort(SortOrder::ResolutionDescending),
                    )
                    .unwrap()
                } else if name == "tooltip-folder" {
                    donor_rail::intent_center(&model, donor_rail::RailIntent::ToggleFolder).unwrap()
                } else {
                    iced::Point::new(24.0, 12.0)
                };
                let cursor = if name.starts_with("tooltip-") {
                    iced::mouse::Cursor::Available(iced::Point::new(
                        at[0] + local_hover.x,
                        at[1] + local_hover.y,
                    ))
                } else {
                    iced::mouse::Cursor::Unavailable
                };
                element.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &theme,
                    &iced::advanced::renderer::Style {
                        text_color: iced::Color::WHITE,
                    },
                    Layout::new(&node),
                    cursor,
                    &bounds,
                );
                let pixels = Headless::screenshot(
                    &mut renderer,
                    iced::Size::new(width, height),
                    1.0,
                    iced::Color::from_rgb8(40, 42, 50),
                );
                let path = output.join(format!("{width}x{height}-{name}-scale{scale}.png"));
                image::save_buffer_with_format(
                    &path,
                    &pixels,
                    width,
                    height,
                    image::ColorType::Rgba8,
                    image::ImageFormat::Png,
                )
                .unwrap();
                if !menu {
                    for intent in [
                        donor_rail::RailIntent::Source(2),
                        donor_rail::RailIntent::CycleShape,
                        donor_rail::RailIntent::ToggleFolder,
                        donor_rail::RailIntent::ToggleFavorites,
                        donor_rail::RailIntent::HuePreset(0),
                        donor_rail::RailIntent::HuePreset(12),
                        donor_rail::RailIntent::SetSort(SortOrder::ColourPop),
                        donor_rail::RailIntent::SetSort(SortOrder::Newest),
                        donor_rail::RailIntent::OpenSearch,
                        donor_rail::RailIntent::TogglePlaylists,
                        donor_rail::RailIntent::ToggleEngineMute,
                        donor_rail::RailIntent::ToggleRandomRotation,
                        donor_rail::RailIntent::ToggleSettings,
                        donor_rail::RailIntent::Close,
                    ]
                    .into_iter()
                    .chain(
                        donor_rail::SORT_SHORTCUTS
                            .into_iter()
                            .map(|x| donor_rail::RailIntent::SetSort(x.0)),
                    )
                    .chain((0..13).map(donor_rail::RailIntent::HuePreset))
                    {
                        let Some(local) = donor_rail::intent_center(&model, intent) else {
                            assert_eq!(name, "visibility");
                            continue;
                        };
                        let point = iced::Point::new(at[0] + local.x, at[1] + local.y);
                        let mut messages = Vec::new();
                        let mut shell = Shell::new(&mut messages);
                        element.as_widget_mut().update(
                            &mut tree,
                            &iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
                                iced::mouse::Button::Left,
                            )),
                            Layout::new(&node),
                            iced::mouse::Cursor::Available(point),
                            &renderer,
                            &mut clipboard::Null,
                            &mut shell,
                            &bounds,
                        );
                        assert!(messages.iter().any(|message| matches!(message, Message::DonorRail(actual) if *actual == intent)), "{width}x{height} {name} missing {intent:?}: {messages:?}");
                    }
                }
                if name == "folder-overflow" {
                    let scale = scale as f32;
                    let popup_size = view::folder_popup_size(&app);
                    let popup_at =
                        donor_rail::folder_popup_anchor(&model, at, app.viewport, popup_size);
                    let cursor = iced::mouse::Cursor::Available(iced::Point::new(
                        popup_at[0] + popup_size[0] * 0.5,
                        popup_at[1] + popup_size[1] * 0.5,
                    ));
                    let mut messages = Vec::new();
                    element.as_widget_mut().update(
                        &mut tree,
                        &iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                            delta: iced::mouse::ScrollDelta::Lines { x: 0.0, y: -1000.0 },
                        }),
                        Layout::new(&node),
                        cursor,
                        &renderer,
                        &mut clipboard::Null,
                        &mut Shell::new(&mut messages),
                        &bounds,
                    );
                    let last = app.available_folders().last().unwrap().clone();
                    let point = iced::Point::new(
                        popup_at[0] + popup_size[0] * 0.5,
                        popup_at[1] + popup_size[1] - 6.0 * scale - (26.0 * scale).max(24.0) * 0.5,
                    );
                    element.as_widget_mut().update(
                        &mut tree,
                        &iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
                            iced::mouse::Button::Left,
                        )),
                        Layout::new(&node),
                        iced::mouse::Cursor::Available(point),
                        &renderer,
                        &mut clipboard::Null,
                        &mut Shell::new(&mut messages),
                        &bounds,
                    );
                    element.as_widget_mut().update(
                        &mut tree,
                        &iced::Event::Mouse(iced::mouse::Event::ButtonReleased(
                            iced::mouse::Button::Left,
                        )),
                        Layout::new(&node),
                        iced::mouse::Cursor::Available(point),
                        &renderer,
                        &mut clipboard::Null,
                        &mut Shell::new(&mut messages),
                        &bounds,
                    );
                    assert!(
                        messages.iter().any(
                            |m| matches!(m,Message::SetFolderFilter(Some(path)) if path==&last)
                        ),
                        "final popup row unreachable: {messages:?}"
                    );
                }
                captures.push(serde_json::json!({"path":path,"width":width,"height":height,"case":name,"rail_anchor":at,"rail_footprint":footprint,"rtl":model.rtl,"rail_mode":format!("{:?}",model.mode),"direction_evidence":if name=="rtl" {"forced RTL descriptor production-composition branch coverage; public en/zh unchanged"}else{"current fixture direction"},"fixture":"injected catalog/display; production view, no provider"}));
            }
        }
    }
    std::fs::write(
        output.join("CAPTURE-INDEX.json"),
        serde_json::to_vec_pretty(&captures).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "W6 owned changed Automation/history sort layouts; explicit invocation only"]
fn w6_owned_automation_and_history_1080p_1440p() {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout,
    };
    let output = std::path::PathBuf::from(
        std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned layout output"),
    );
    std::fs::create_dir_all(&output).unwrap();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .unwrap();
    let mut captures = Vec::new();
    for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
        for case in ["rotation", "startup", "history"] {
            let mut app = if case == "history" {
                d4a_fixture(4).0
            } else {
                p3_composed_editor_app(SettingsCategory::Automation)
            };
            app.viewport = [width as f32, height as f32];
            app.launch_policy = p1_preview_policy();
            if case == "history" {
                app.panel_state.settings_open = false;
                app.settings_workbench.active = false;
                app.panel_state.search_open = true;
                app.sort_order = SortOrder::Recent;
                let item = app.wallpapers[1].id.clone();
                app.state.local_library.usage.items.insert(
                    item,
                    candidate_state::local_library::ItemUsage {
                        confirmed_count: 7,
                        last_confirmed_unix: 1_700_000_000,
                        retained: Default::default(),
                    },
                );
                app.refresh_filter();
                assert_eq!(app.filtered_indices[0], 1);
            } else {
                p3_settings_intent(&mut app, SettingsMsg::SelectTab("automation".into()));
                p3_settings_intent(
                    &mut app,
                    SettingsMsg::SelectSection(if case == "startup" { 1 } else { 0 }),
                );
                app.state
                    .frontend_preferences
                    .desired_policy
                    .restore_on_startup = Some(true);
                app.state
                    .frontend_preferences
                    .desired_policy
                    .random_include_favourites = Some(true);
            }
            app.settings_workbench.message =
                "INJECTED fixture history/policy · no Apply or provider observation".into();
            app.colours.status =
                "INJECTED fixture history · no Apply · unseen last / stable item ID ties".into();
            app.status =
                Some("INJECTED fixture history · counts were not produced by Apply".into());
            app.settings_workbench.folio_entrance = None;
            app.settings_workbench.tab_reveal = None;
            {
                let mut fonts = iced::advanced::graphics::text::font_system()
                    .write()
                    .unwrap();
                fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                fonts.load_font(std::borrow::Cow::Borrowed(
                    include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
                ));
            }
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let background = application_style(&app, &theme).background_color;
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            assert_eq!(node.size(), size);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: application_style(&app, &theme).text_color,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let bytes = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                background,
            );
            let path = output.join(format!("{width}x{height}-w6-{case}.png"));
            ::image::save_buffer_with_format(
                &path,
                &bytes,
                width,
                height,
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
            captures.push(serde_json::json!({"path":path,"width":width,"height":height,"case":case,"injected":true,"providerTrial":false}));
        }
    }
    std::fs::write(
        output.join("w6-captures.json"),
        serde_json::to_vec_pretty(&captures).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "W3 owned production layouts; explicit invocation only"]
fn w3_owned_drawer_badges_and_library_1080p_1440p() {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout,
    };
    let output = std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").unwrap());
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .unwrap();
    let mut captures = vec![];
    for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
        for case in ["slices", "hex", "wall", "depth", "library", "studio"] {
            let (mut app, _) = d4a_fixture(3);
            app.panel_state.open_settings(SettingsCategory::Library);
            view::settings::controller::reconcile(&mut app);
            app.viewport = [width as f32, height as f32];
            app.settings_workbench.folio_entrance = None;
            app.settings_workbench.tab_reveal = None;
            app.config.catalog.local_folders = vec![app
                .state_path
                .with_file_name("Unavailable W3 labelled root")];
            if case == "library" {
                app.settings_workbench.windows_panel = Some(SettingsCategory::Library);
            } else if case == "studio" {
                app.panel_state.settings_category = SettingsCategory::Picker;
                view::settings::controller::reconcile(&mut app);
                p3_settings_intent(&mut app, SettingsMsg::SelectSection(1));
                app.settings_workbench.message = "Local preference saved.".into();
                app.settings_workbench.folio_entrance = None;
                app.settings_workbench.tab_reveal = None;
            } else {
                app.panel_state.settings_open = false;
                app.settings_workbench.active = false;
                app.panel_state.search_open = true;
                app.search_tags_open = true;
                app.gallery_mode = match case {
                    "hex" => GalleryMode::Geometric,
                    "wall" => GalleryMode::Wall,
                    "depth" => GalleryMode::Depth,
                    _ => GalleryMode::Slices,
                };
                app.state.gallery.mode = gallery_kind(app.gallery_mode);
                app.state.frontend_preferences.tag_panel.width =
                    Some(if width == 1920 { 540.0 } else { 760.0 });
                app.state.frontend_preferences.tag_panel.rows = Some(
                    match case {
                        "slices" => "1",
                        "wall" => "3",
                        _ => "2",
                    }
                    .into(),
                );
                app.state.frontend_preferences.tag_panel.offset_x = Some(20.0);
                app.state.frontend_preferences.tag_panel.offset_y = Some(-30.0);
                for (i, entry) in app.catalog.iter_mut().enumerate() {
                    entry.source_tags = (0..24).map(|n| format!("fixture-tag-{n:02}")).collect();
                    app.wallpapers[i].source = match i % 3 {
                        0 => WallpaperSource::LocalImage,
                        1 => WallpaperSource::LocalVideo,
                        _ => WallpaperSource::WallpaperEngine,
                    };
                    app.wallpapers[i].wallpaper_type = if i % 3 == 2 {
                        "scene"
                    } else if i % 3 == 1 {
                        "video"
                    } else {
                        "image"
                    }
                    .into();
                    let media = output.join("labelled-media");
                    std::fs::create_dir_all(&media).unwrap();
                    let path = media.join(format!("{width}-{case}-{i}.png"));
                    ::image::RgbaImage::from_pixel(
                        80,
                        60,
                        ::image::Rgba([40 + (i * 60) as u8, 100, 170 - (i * 30) as u8, 255]),
                    )
                    .save(&path)
                    .unwrap();
                    app.wallpapers[i].preview_path = Some(path);
                    entry.width = 80;
                    entry.height = 60;
                }
                app.project_local_tags();
                app.sync_gallery_presentation();
                if let Some(p) = &mut app.gallery_presentation {
                    p.settle();
                }
                if let Some(p) = &mut app.hex_presentation {
                    p.settle();
                }
                if let Some(p) = &mut app.wall_presentation {
                    p.settle();
                }
                let mut posters = Vec::new();
                for (index, item) in app.wallpapers.iter().enumerate() {
                    let source = poster_source_for_item(item);
                    posters.push(gpu_gallery::decode_poster(&source, index as u32).unwrap());
                    app.poster_slots.insert(
                        index,
                        PosterSlot {
                            layer: index as u32,
                            is_video: item.source == WallpaperSource::LocalVideo,
                            generation: app.poster_generation.load(Ordering::Acquire),
                            catalog_generation: app.catalog_generation.load(Ordering::Acquire),
                            item_id: item.id.clone(),
                            source,
                        },
                    );
                }
                app.posters = Arc::new(posters);
                app.rebuild_depth_frame();
            }
            {
                let mut fonts = iced::advanced::graphics::text::font_system()
                    .write()
                    .unwrap();
                fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                fonts.load_font(std::borrow::Cow::Borrowed(
                    include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
                ));
            }
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let style = application_style(&app, &theme);
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            renderer.reset(bounds);
            if case == "studio" {
                let mut element = view::settings::gateway::view(&app);
                let mut tree = Tree::new(&element);
                let node = element.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(size, size),
                );
                let root = Layout::new(&node);
                let footer = root.children().nth(1).expect("reserved footer").bounds();
                fn panels(layout: Layout<'_>, width: f32, bottoms: &mut Vec<f32>) {
                    let b = layout.bounds();
                    if (b.width - width).abs() < 0.1 && b.height > 360.0 {
                        bottoms.push(b.y + b.height);
                    }
                    for child in layout.children() {
                        panels(child, width, bottoms);
                    }
                }
                let mut bottoms = Vec::new();
                panels(
                    root.children().next().unwrap(),
                    crate::donor_folio::source_ui::picker_layout_studio_width(width as f32, 1.0),
                    &mut bottoms,
                );
                assert!(!bottoms.is_empty(), "actual compact studio panel found");
                assert!(
                    bottoms.into_iter().all(|bottom| bottom <= footer.y + 0.1),
                    "studio rows must scroll above the save-status footer"
                );
            }
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            fn finite(node: &layout::Node) {
                let b = node.bounds();
                assert!([b.x, b.y, b.width, b.height]
                    .into_iter()
                    .all(f32::is_finite));
                for child in node.children() {
                    finite(child);
                }
            }
            finite(&node);
            assert_eq!(node.size(), size);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: style.text_color,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let bytes = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                style.background_color,
            );
            let path = output.join(format!("{width}x{height}-w3-{case}.png"));
            ::image::save_buffer_with_format(
                &path,
                &bytes,
                width,
                height,
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
            captures
                .push(serde_json::json!({"path":path,"case":case,"width":width,"height":height}));
        }
    }
    std::fs::write(
        output.join("w3-captures.json"),
        serde_json::to_vec_pretty(&captures).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "Owned P7 dense real-image GPU growth/recreation; explicit invocation only"]
fn p7_dense_atlas_real_images_and_recreation_1080p_1440p() {
    dense_real_images_and_recreation(GalleryMode::Slices);
}

#[test]
#[ignore = "Owned P9 approved Wall growth/recreation with real images; explicit invocation only"]
fn p9_dense_wall_real_images_and_recreation_1080p_1440p() {
    dense_real_images_and_recreation(GalleryMode::Wall);
}

fn dense_real_images_and_recreation(mode: GalleryMode) {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout,
    };
    let output =
        std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned output"));
    let media = output.join("media");
    std::fs::create_dir_all(&media).unwrap();
    let (mut app, _) = d4a_fixture(100);
    app.gallery_mode = mode;
    app.depth_frame = None;
    app.selected = Some(50);
    let p = &mut app.state.gallery.slices;
    p.width = 32.0;
    p.expanded_width = 200.0;
    p.height = 520.0;
    p.spacing = 0.0;
    p.skew_distance = 0.0;
    p.visible_count = 64;
    p.round_corners = Some(false);
    p.shadows = Some(false);
    if mode == GalleryMode::Wall {
        let p = &mut app.state.gallery.wall;
        p.columns = 8;
        p.rows = 8;
        p.card_width = 80.0;
        p.card_height = 60.0;
        p.gap_x = 7.0;
        p.gap_y = 10.0;
        p.selected_scale = 1.0;
    }
    for i in 0..100 {
        let path = media.join(format!("dense-{i:03}.png"));
        let color = ::image::Rgba([
            (i * 37 % 220 + 20) as u8,
            (i * 71 % 220 + 20) as u8,
            (i * 113 % 220 + 20) as u8,
            255,
        ]);
        ::image::RgbaImage::from_pixel(80, 60, color)
            .save(&path)
            .unwrap();
        app.wallpapers[i].preview_path = Some(path);
    }
    let mut receipts = Vec::new();
    for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
        app.viewport = [width as f32, height as f32];
        app.gallery_presentation = None;
        app.wall_presentation = None;
        app.sync_gallery_presentation();
        if let Some(p) = &mut app.gallery_presentation {
            p.settle();
        }
        if let Some(p) = &mut app.wall_presentation {
            p.settle();
        }
        let indices = app.ordinary_poster_indices();
        assert!(indices.len() > 32);
        app.poster_slots.clear();
        let mut posters = Vec::new();
        for (layer, index) in indices.iter().enumerate() {
            let source = poster_source_for_item(&app.wallpapers[*index]);
            let poster = gpu_gallery::decode_poster(&source, layer as u32).unwrap();
            app.poster_slots.insert(
                *index,
                PosterSlot {
                    layer: layer as u32,
                    is_video: false,
                    generation: 1,
                    catalog_generation: app.catalog_generation.load(Ordering::Acquire),
                    item_id: app.wallpapers[*index].id.clone(),
                    source,
                },
            );
            posters.push(poster);
        }
        app.posters = Arc::new(posters);
        let entries = app.ordinary_frame();
        assert!(entries.iter().all(|e| e.card.layer.is_some()));
        let snapshot = Arc::new(GallerySnapshot {
            ordinary: Some(entries.clone()),
            cards: entries.iter().map(|e| e.card).collect(),
            posters: app.posters.clone(),
            colors: GalleryColors::default(),
            scene: DonorSceneState {
                stage: Default::default(),
                camera: 0.0,
                camera_target: 0.0,
                camera_velocity: 0.0,
                detail_progress: 0.0,
                external_detail: false,
                flip_options: Default::default(),
                detail_source: None,
                detail_card: None,
                viewport: app.viewport,
                time: 0.0,
                visibility: 1.0,
            },
            depth: None,
            wall_clip: (mode == GalleryMode::Wall).then(|| {
                gpu_gallery::wall_frame(app.viewport, gallery_layout_params_for_app(&app).wall).clip
            }),
        });
        let mut previous = None;
        for pass in 0..2 {
            let mut renderer = iced::futures::executor::block_on(
                <iced::Renderer as Headless>::new(UI_FONT, iced::Pixels(16.0), Some("wgpu")),
            )
            .unwrap();
            let mut element: Element<'_, GalleryMessage> = shader(GalleryProgram {
                snapshot: snapshot.clone(),
            })
            .width(Fill)
            .height(Fill)
            .into();
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            renderer.reset(bounds);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &iced::Theme::Dark,
                &iced::advanced::renderer::Style {
                    text_color: iced::Color::WHITE,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let bytes = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                iced::Color::BLACK,
            );
            let samples: HashSet<_> = entries
                .iter()
                .filter_map(|e| {
                    let x = e.card.center[0] as i32;
                    let y = e.card.center[1] as i32;
                    if x < 0 || x >= width as i32 || y < 0 || y >= height as i32 {
                        return None;
                    }
                    let at = (y as usize * width as usize + x as usize) * 4;
                    Some(bytes[at..at + 3].to_vec())
                })
                .collect();
            assert!(
                samples.len() > 32,
                "real image centers must survive growth: {} distinct",
                samples.len()
            );
            if mode == GalleryMode::Wall {
                assert_eq!(samples.len(), 64, "each visible Wall image must render");
            }
            assert!(
                app.posters.iter().all(|p| !p.has_pixels()),
                "GPU consumed the original decoded pixels"
            );
            if let Some(before) = previous {
                assert_eq!(
                    bytes, before,
                    "recreated GPU pipeline must recover identical image pixels"
                );
            }
            let path = output.join(format!("{width}x{height}-dense-recreation-{pass}.png"));
            ::image::save_buffer_with_format(
                &path,
                &bytes,
                width,
                height,
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
            receipts.push(serde_json::json!({"width":width,"height":height,"pass":pass,"posters":indices.len(),"distinctImageCenters":samples.len(),"path":path,"source":"production GalleryProgram/material/shader/atlas; real decoded PNGs; new headless device each pass"}));
            previous = Some(bytes);
        }
    }
    std::fs::write(
        output.join("dense-recovery.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "Owned P7 affected production Slices layouts; explicit invocation only"]
fn p7_owned_slices_layouts_1080p_1440p() {
    owned_gallery_layouts(GalleryMode::Slices);
}

#[test]
#[ignore = "Owned P8 affected production Hex layouts; explicit invocation only"]
fn p8_owned_hex_layouts_1080p_1440p() {
    owned_gallery_layouts(GalleryMode::Geometric);
}

#[test]
#[ignore = "Owned P9 affected production Wall layouts; explicit invocation only"]
fn p9_owned_wall_layouts_1080p_1440p() {
    owned_gallery_layouts(GalleryMode::Wall);
}

fn owned_gallery_layouts(mode: GalleryMode) {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout,
    };
    let output = std::path::PathBuf::from(
        std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned layout output required"),
    );
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .expect("headless WGPU adapter");
    let mut captures = Vec::new();
    for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
        let cases = if mode == GalleryMode::Slices {
            ["default", "wobble-mid", "layout-controls"]
        } else if mode == GalleryMode::Wall {
            ["default", "wall-scroll", "wall-rail"]
        } else {
            ["default", "edge-motion", "position-controls"]
        };
        for case in cases {
            let (mut app, _) = d4a_fixture(36);
            app.gallery_mode = mode;
            app.depth_frame = None;
            if mode == GalleryMode::Wall {
                app.config.ui.show_top_bar = true;
                app.rail_reveal = 1.0;
            }
            app.viewport = [width as f32, height as f32];
            app.selected = Some(12);
            app.state.gallery.slices.spacing = 0.0;
            app.state.gallery.slices.wobble = case == "wobble-mid";
            app.state.gallery.slices.parallax = Some(case == "wobble-mid");
            app.sync_gallery_presentation();
            if let Some(p) = &mut app.gallery_presentation {
                p.settle();
            }
            if let Some(p) = &mut app.hex_presentation {
                p.settle();
            }
            if let Some(p) = &mut app.wall_presentation {
                p.settle();
            }
            if case == "wall-scroll" {
                app.wall_presentation.as_mut().unwrap().wheel(177.0);
                app.wall_presentation.as_mut().unwrap().tick(0.05);
            }
            if case == "wall-rail" {
                app.state.frontend_preferences.interface.ui_scale = Some(2.0);
                app.state.frontend_preferences.filter_bar.orientation = Some("vertical".into());
                app.state.gallery.wall.layout = StateWallLayout::Cylinder;
                app.state.gallery.wall.round_corners = Some(false);
                app.sync_wall_presentation();
                app.wall_presentation.as_mut().unwrap().settle();
            }
            if case == "wobble-mid" {
                app.selected = Some(14);
                app.sync_gallery_presentation();
                app.gallery_presentation.as_mut().unwrap().tick(0.05);
            }
            if case == "layout-controls" {
                app.panel_state.open_settings(SettingsCategory::Picker);
                view::settings::controller::reconcile(&mut app);
                p3_settings_intent(&mut app, SettingsMsg::SelectSection(1));
            }
            if case == "edge-motion" {
                app.selected = Some(33);
                app.sync_gallery_presentation();
                app.hex_presentation.as_mut().unwrap().tick(0.05);
            }
            if case == "position-controls" {
                app.state.gallery.geometric.stage =
                    Some(candidate_state::mode_profiles::StageTransform {
                        x: 0.25,
                        y: -0.15,
                        scale: 1.0,
                        rotation_degrees: 0.0,
                        perspective: 0.0,
                        shear_x: 0.0,
                        shear_y: 0.0,
                        depth_angle_degrees: 0.0,
                    });
                app.sync_gallery_presentation();
                app.hex_presentation.as_mut().unwrap().settle();
                app.panel_state.open_settings(SettingsCategory::Position);
                view::settings::controller::reconcile(&mut app);
                p3_settings_intent(&mut app, SettingsMsg::SelectSection(1));
            }
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let style = application_style(&app, &theme);
            let mut element = view(&app, window);
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            assert_eq!(node.size(), size);
            renderer.reset(bounds);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: style.text_color,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let bytes = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                style.background_color,
            );
            assert_eq!(bytes.len(), width as usize * height as usize * 4);
            let mode_name = match mode {
                GalleryMode::Slices => "slices",
                GalleryMode::Wall => "wall",
                _ => "hex",
            };
            let path = output.join(format!("{width}x{height}-{mode_name}-{case}.png"));
            ::image::save_buffer_with_format(
                &path,
                &bytes,
                width,
                height,
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
            captures.push(serde_json::json!({"path":path,"width":width,"height":height,"case":case,"composition":"production app root","boundary":"Headless layout, not physical display/DPI/motion/human acceptance"}));
        }
    }
    std::fs::write(
        output.join("captures.json"),
        serde_json::to_vec_pretty(&captures).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "Owned P6 affected production form layouts; explicit invocation only"]
fn p6_owned_pending_layouts_1080p_1440p() {
    use crate::donor_folio;
    use iced::advanced::{
        clipboard, layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout, Shell,
    };
    use view::settings::pending_forms::{self, Action};
    let output = std::path::PathBuf::from(
        std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned layout output required"),
    );
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .expect("headless WGPU adapter");
    let mut captures = Vec::new();
    for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
        for case in ["pending-rules", "pending-browser", "pending-profile"] {
            let category = match case {
                "pending-rules" => SettingsCategory::Automation,
                "pending-browser" => SettingsCategory::Sources,
                _ => SettingsCategory::Theme,
            };
            let mut app = p3_composed_editor_app(category);
            app.viewport = [width as f32, height as f32];
            app.settings_workbench.windows_panel = Some(category);
            app.selected = Some(0);
            if case == "pending-rules" {
                pending_forms::dispatch(&mut app, Action::AddRule);
                pending_forms::dispatch(&mut app, Action::NewKind("Time window".into()));
                pending_forms::dispatch(&mut app, Action::AddNode(0, Vec::new(), None));
            } else if case == "pending-profile" {
                app.ensure_theme_designer();
                pending_forms::dispatch(&mut app, Action::SaveWallpaperProfile);
            }
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let background = application_style(&app, &theme).background_color;
            // New profile form uses the same production Folio page/sheet; the
            // long designer above it is already covered by P5 root layouts.
            let mut element = if case == "pending-profile" {
                let palette = donor_palette(&app);
                let page = donor_folio::page(
                    "Theme",
                    "Pending wallpaper profiles",
                    pending_forms::profiles_view(&app),
                    None,
                    None,
                    palette,
                    donor_folio::BlueprintKind::Settings,
                    view::settings::bridge::effective_ui_scale(&app),
                );
                donor_folio::sheet(
                    app.viewport,
                    None,
                    page,
                    palette,
                    Message::SheetBackdrop,
                    None,
                    view::settings::bridge::effective_ui_scale(&app),
                )
            } else {
                view(&app, window)
            };
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            assert_eq!(node.size(), size);
            let mut messages = Vec::new();
            if case == "pending-rules" {
                element.as_widget_mut().update(
                    &mut tree,
                    &iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                        delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: -480.0 },
                    }),
                    Layout::new(&node),
                    iced::mouse::Cursor::Available(iced::Point::new(
                        width as f32 * 0.62,
                        height as f32 * 0.6,
                    )),
                    &renderer,
                    &mut clipboard::Null,
                    &mut Shell::new(&mut messages),
                    &bounds,
                );
                assert!(messages.iter().all(|m| matches!(m, Message::SheetBackdrop)));
            }
            renderer.reset(bounds);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: application_style(&app, &theme).text_color,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let bytes = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                background,
            );
            assert_eq!(bytes.len(), width as usize * height as usize * 4);
            assert!(bytes.chunks_exact(4).any(|p| p != &bytes[..4]));
            let path = output.join(format!("{width}x{height}-{case}.png"));
            ::image::save_buffer_with_format(
                &path,
                &bytes,
                width,
                height,
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
            captures.push(serde_json::json!({"case":case,"width":width,"height":height,"path":path,"scaleFactor":1.0,"profile":app.state_path,"composition":if case=="pending-profile"{"production form in production Folio page/sheet"}else{"production app root"}}));
        }
    }
    assert_eq!(captures.len(), 8);
    std::fs::write(output.join("captures.json"),serde_json::to_vec_pretty(&serde_json::json!({"captures":captures,"boundary":"Affected production layouts only; not native input, physical display/DPI, motion, or human acceptance"})).unwrap()).unwrap();
}

#[test]
#[ignore = "Owned P5 WGPU modal keyboard regression; explicit exact invocation only"]
fn p5_theme_modal_owns_keyboard_over_previously_focused_name_and_hex() {
    fn editor_message(message: &Message) -> bool {
        match message {
            Message::SettingsLegacy { message, .. } => editor_message(message),
            Message::ThemeDesigner(crate::candidate_app::theme_workflow::Action::Name {
                ..
            })
            | Message::ThemeRoleTextChanged { .. }
            | Message::ThemeRoleSetColor { .. } => true,
            _ => false,
        }
    }
    use iced::advanced::{clipboard, layout, renderer::Headless, widget::Tree, Layout, Shell};
    let renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .expect("owned headless WGPU adapter");
    for hex in [false, true] {
        let mut app = super::theme_workflow::designer_app();
        app.launch_policy = p1_preview_policy();
        super::theme_workflow::designer_name(&mut app, "Unsaved keyboard draft");
        view::settings::controller::reconcile(&mut app);
        let window = app.window.unwrap();
        let mut tree = {
            let element = view(&app, window);
            Tree::new(element.as_widget())
        };
        let path = if hex {
            let mut path = entry_role_path(&app, &tree);
            let editor = theme_role_editor_content(&app);
            let subtree = Tree::new(editor.as_widget());
            path.extend(
                entry_paths(
                    &subtree,
                    iced::advanced::widget::tree::Tag::of::<EntryInputState>(),
                )
                .pop()
                .unwrap(),
            );
            path
        } else {
            entry_paths(
                &tree,
                iced::advanced::widget::tree::Tag::of::<EntryInputState>(),
            )
            .last()
            .unwrap()
            .clone()
        };
        path.iter()
            .fold(&mut tree, |node, &index| &mut node.children[index])
            .state
            .downcast_mut::<EntryInputState>()
            .focus();
        let event = iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Character("x".into()),
            modified_key: keyboard::Key::Character("x".into()),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: Some("x".into()),
            repeat: false,
        });
        let size = iced::Size::new(1920.0, 1080.0);
        let bounds = iced::Rectangle::with_size(size);
        let deliver = |element: &mut Element<'_, Message>, tree: &mut Tree, event: &iced::Event| {
            let node =
                element
                    .as_widget_mut()
                    .layout(tree, &renderer, &layout::Limits::new(size, size));
            let mut messages = Vec::new();
            let mut shell = Shell::new(&mut messages);
            element.as_widget_mut().update(
                tree,
                event,
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &renderer,
                &mut clipboard::Null,
                &mut shell,
                &bounds,
            );
            messages
        };
        let mut queued = {
            let mut element = view(&app, window);
            let positive = deliver(&mut element, &mut tree, &event);
            assert!(
                positive.iter().any(editor_message),
                "focused production input must receive the positive control: {positive:?}"
            );
            positive
        };
        let enter = iced::Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Enter),
            modified_key: keyboard::Key::Named(keyboard::key::Named::Enter),
            physical_key: keyboard::key::Physical::Unidentified(
                keyboard::key::NativeCode::Unidentified,
            ),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        });
        if hex {
            let mut element = view(&app, window);
            let positive = deliver(&mut element, &mut tree, &enter);
            assert!(
                positive.iter().any(editor_message),
                "hex Enter must submit before the decision: {positive:?}"
            );
            queued.extend(positive);
        }
        let (instance, generation, role, variant) = theme_editor_binding(&app);
        queued.push(Message::ThemeWheel {
            editor_instance: instance,
            selection_generation: generation,
            role,
            variant,
            action: crate::donor_folio::source_ui::wheel::WheelAction::Hue(0.75),
        });
        // No pointer click unfocuses the old field before native Close opens
        // the dirty decision, so the retained keyboard state is the risk.
        let _ = update(&mut app, Message::CloseRequested(window));
        let decision = app.theme_workflow.prompt.as_ref().unwrap().decision;
        let before = app.theme_session.clone();
        let mut element = view(&app, window);
        tree.diff(element.as_widget());
        let messages = deliver(&mut element, &mut tree, &event);
        assert!(
            !messages.iter().any(editor_message),
            "modal cannot type into the hidden prior field: {messages:?}"
        );
        let enter_messages = deliver(&mut element, &mut tree, &enter);
        assert!(
            !enter_messages.iter().any(editor_message),
            "modal cannot submit a hidden hex field: {enter_messages:?}"
        );
        drop(element);
        for message in messages.into_iter().chain(enter_messages).chain(queued) {
            let _ = update(&mut app, message);
        }
        assert_eq!(app.theme_session, before);
        assert_eq!(
            app.theme_workflow.prompt.as_ref().unwrap().decision,
            decision
        );
        d3_key(&mut app, keyboard::key::Named::Escape);
        assert!(app.theme_workflow.prompt.is_none());
        assert_eq!(
            app.theme_session.editor_document(),
            before.editor_document()
        );
        assert!(app.theme_session.designer_dirty() && app.visible);
        // Cancel restores an editable current tree, rather than leaving the
        // surviving draft behind a dead input binding.
        let mut element = view(&app, window);
        tree.diff(element.as_widget());
        let fresh_path = if hex {
            let mut path = entry_role_path(&app, &tree);
            let editor = theme_role_editor_content(&app);
            path.extend(
                entry_paths(
                    &Tree::new(editor.as_widget()),
                    iced::advanced::widget::tree::Tag::of::<EntryInputState>(),
                )
                .pop()
                .unwrap(),
            );
            path
        } else {
            entry_paths(
                &tree,
                iced::advanced::widget::tree::Tag::of::<EntryInputState>(),
            )
            .last()
            .unwrap()
            .clone()
        };
        fresh_path
            .iter()
            .fold(&mut tree, |node, &index| &mut node.children[index])
            .state
            .downcast_mut::<EntryInputState>()
            .focus();
        let current = deliver(&mut element, &mut tree, &event);
        assert!(
            current.iter().any(editor_message),
            "Cancel must restore usable input: {current:?}"
        );
        drop(element);
        let before_typing = app.theme_session.clone();
        for message in current {
            let _ = update(&mut app, message);
        }
        assert_ne!(app.theme_session, before_typing);
        assert!(app.theme_workflow.prompt.is_none() && !app.exit_requested);
    }
}

#[test]
#[ignore = "Owned P5 production layout capture; requires P5_LAYOUT_OUTPUT and explicit invocation"]
fn p5_owned_theme_layout_captures_1080p_1440p() {
    use super::theme_workflow::{
        designer_app, designer_color, designer_decide, designer_name, designer_save,
    };
    use crate::candidate_app::theme_workflow::{Action, Binding, Choice};
    use iced::advanced::{
        clipboard, layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout, Shell,
    };
    let output = std::path::PathBuf::from(
        std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned output required"),
    );
    assert!(output.is_absolute());
    std::fs::create_dir_all(&output).unwrap();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .expect("headless WGPU adapter required");
    let cases = [
        "designer-dark",
        "designer-light",
        "designer-edited",
        "invalid-hex",
        "filtered-role",
        "library",
        "library-scroll",
        "overwrite",
        "delete",
        "dirty-exit",
        "waiting-exit",
        "cancel-exit",
        "save-failure",
    ];
    let mut captures = Vec::new();
    for locale in ["en-US", "ar-SA"] {
        for [width, height] in [[1920u32, 1080u32], [2560, 1440]] {
            for case in cases {
                let mut app = designer_app();
                app.launch_policy = p1_preview_policy();
                app.viewport = [width as f32, height as f32];
                let next = localization::Catalog::new(locale, locale).unwrap();
                localization::reload_ui_font(&app.settings_locale, &next).unwrap();
                app.settings_locale = next;
                {
                    let mut fonts = iced::advanced::graphics::text::font_system()
                        .write()
                        .unwrap();
                    fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                    fonts.load_font(std::borrow::Cow::Borrowed(
                        include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf")
                            .as_slice(),
                    ));
                }
                view::settings::controller::reconcile(&mut app);
                match case {
                    "designer-light" => {
                        let i = app.theme_session.editor_instance().unwrap();
                        let _ = update(
                            &mut app,
                            Message::ThemeVariantSelected {
                                editor_instance: i,
                                variant: theme_roles::ColorVariant::Light,
                            },
                        );
                    }
                    "designer-edited" => designer_color(&mut app, "#cf345b"),
                    "invalid-hex" => {
                        let i = app.theme_session.editor_instance().unwrap();
                        let g = app.theme_session.editor_selection_generation().unwrap();
                        let _ = update(
                            &mut app,
                            Message::ThemeRoleTextChanged {
                                editor_instance: i,
                                selection_generation: g,
                                role: theme_roles::ThemeRole::Primary,
                                variant: theme_roles::ColorVariant::Dark,
                                text: "#not hex".into(),
                            },
                        );
                    }
                    "filtered-role" => {
                        let i = app.theme_session.editor_instance().unwrap();
                        let _ = update(
                            &mut app,
                            Message::ThemeRoleFilterChanged {
                                editor_instance: i,
                                text: "surface".into(),
                            },
                        );
                    }
                    "library" | "library-scroll" => {
                        for name in ["Nord", "Evening work", "Owned presentation theme"] {
                            designer_name(&mut app, name);
                            designer_save(&mut app, false);
                        }
                    }
                    "overwrite" => {
                        app.state
                            .theme_library
                            .save(None, "Existing", candidate_state::frontend::nord_document())
                            .unwrap();
                        designer_name(&mut app, "Existing");
                        designer_color(&mut app, "#cf345b");
                        designer_save(&mut app, false);
                        assert!(app.theme_workflow.prompt.is_some());
                    }
                    "delete" => {
                        designer_name(&mut app, "Applied custom Nord");
                        designer_save(&mut app, true);
                        let binding = Binding::current(&app).unwrap();
                        let id = app.state.theme_library.records[0].id;
                        let _ = update(
                            &mut app,
                            Message::ThemeDesigner(Action::Delete { binding, id }),
                        );
                    }
                    "dirty-exit" | "cancel-exit" => {
                        designer_name(&mut app, "Unsaved Theme draft");
                        let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
                        assert!(!app.exit_requested);
                        if case == "cancel-exit" {
                            designer_decide(&mut app, Choice::Cancel);
                        }
                    }
                    "waiting-exit" => {
                        // Owned source fixture holds the coordinator state only;
                        // no provider operation or accepted export is fabricated.
                        app.apply_pending = true;
                        let _ = update(&mut app, Message::Wake(Wake::Tray(TrayAction::Exit)));
                        assert!(app.exit_requested);
                    }
                    "save-failure" => {
                        std::fs::write(app.state_path.with_extension("toml.bak"), b"version=999\n")
                            .unwrap();
                        designer_name(&mut app, "Retained after save failure");
                        designer_save(&mut app, true);
                        assert!(app.save_failure.is_some());
                    }
                    _ => {}
                }
                let window = app.window.unwrap();
                let theme = theme(&app, window);
                let background = application_style(&app, &theme).background_color;
                let mut element = view(&app, window);
                let mut tree = Tree::new(&element);
                let size = iced::Size::new(width as f32, height as f32);
                let node = element.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(size, size),
                );
                assert_eq!(node.size(), size);
                fn finite(node: &layout::Node) {
                    let b = node.bounds();
                    assert!([b.x, b.y, b.width, b.height]
                        .into_iter()
                        .all(f32::is_finite));
                    for child in node.children() {
                        finite(child);
                    }
                }
                finite(&node);
                let bounds = iced::Rectangle::with_size(size);
                let mut scroll_messages = Vec::new();
                if case == "library-scroll" {
                    let point = iced::Point::new(width as f32 * 0.62, height as f32 * 0.5);
                    let mut shell = Shell::new(&mut scroll_messages);
                    element.as_widget_mut().update(
                        &mut tree,
                        &iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                            delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: -650.0 },
                        }),
                        Layout::new(&node),
                        iced::mouse::Cursor::Available(point),
                        &renderer,
                        &mut clipboard::Null,
                        &mut shell,
                        &bounds,
                    );
                    assert!(
                        scroll_messages
                            .iter()
                            .all(|message| matches!(message, Message::SheetBackdrop)),
                        "headless scroll must not dispatch execution"
                    );
                }
                renderer.reset(bounds);
                element.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &theme,
                    &iced::advanced::renderer::Style {
                        text_color: application_style(&app, &theme).text_color,
                    },
                    Layout::new(&node),
                    iced::mouse::Cursor::Unavailable,
                    &bounds,
                );
                let bytes = Headless::screenshot(
                    &mut renderer,
                    iced::Size::new(width, height),
                    1.0,
                    background,
                );
                assert_eq!(bytes.len(), width as usize * height as usize * 4);
                assert!(
                    bytes.chunks_exact(4).any(|pixel| pixel != &bytes[..4]),
                    "Blank {case}/{locale}/{width}"
                );
                let path = output.join(format!("{locale}-{width}x{height}-{case}.png"));
                ::image::save_buffer_with_format(
                    &path,
                    &bytes,
                    width,
                    height,
                    ::image::ColorType::Rgba8,
                    ::image::ImageFormat::Png,
                )
                .unwrap();
                captures.push(serde_json::json!({"case":case,"locale":locale,"width":width,"height":height,"scaleFactor":1.0,"path":path,"profile":app.state_path,"scrollPixels":if case=="library-scroll" {650}else{0},"scrollMessages":format!("{scroll_messages:?}"),"rootBounds":[node.bounds().x,node.bounds().y,node.bounds().width,node.bounds().height],"providerSubmission":false,"waitingExitFixtureOnly":case=="waiting-exit"}));
            }
        }
    }
    assert_eq!(captures.len(), 52);
    std::fs::write(output.join("captures.json"),serde_json::to_vec_pretty(&serde_json::json!({"boundary":"Owned headless production widgets and synthetic wheel event; not native input, physical display/DPI, motion or human acceptance","captures":captures})).unwrap()).unwrap();
}

fn contains(control: &Control, path: &str) -> bool {
    match control {
        Control::Number { path: key, .. }
        | Control::TextField { path: key, .. }
        | Control::Resolution { path: key, .. }
        | Control::KeyBinding { path: key, .. }
        | Control::Toggle { path: key, .. }
        | Control::Dropdown { path: key, .. }
        | Control::Chips { path: key, .. } => key == path,
        Control::Presets { .. } => path == "@profiles",
        Control::Details { rows, .. } | Control::StackBar { rows, .. } => {
            rows.iter().any(|row| contains(&row.control, path))
        }
        _ => false,
    }
}

fn expand(control: &Control, session: &mut view::settings::bridge::Session) {
    match control {
        Control::Details { id, rows, .. } => {
            session.navigation.expanded.insert(id.clone());
            for row in rows {
                expand(&row.control, session);
            }
        }
        Control::StackBar { id, rows, .. } => {
            session.bar_reveals.insert(
                id.clone(),
                crate::donor_folio::source_ui::motion::Tween::for_duration_ms(1.0, 100.0),
            );
            for row in rows {
                expand(&row.control, session);
            }
        }
        _ => {}
    }
}

#[test]
#[ignore = "Owned P4 WGPU layout capture; requires P4_LAYOUT_OUTPUT and explicit invocation"]
fn p4_owned_frontend_layout_captures_1080p_1440p() {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout,
    };
    use view::settings::composed::{keys, SettingsSource};
    let output = std::path::PathBuf::from(
        std::env::var_os("P4_LAYOUT_OUTPUT").expect("owned output required"),
    );
    assert!(output.is_absolute());
    std::fs::create_dir_all(&output).unwrap();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .expect("headless WGPU adapter required");
    let cases = [
        ("profiles-slices", "picker", "@profiles"),
        ("profiles-hex", "picker", "@profiles"),
        ("profiles-wall", "picker", "@profiles"),
        ("profiles-depth", "picker", "@profiles"),
        ("pending-valid", "playback", keys::playback::RESUME_DELAY),
        ("pending-invalid", "playback", keys::playback::RESUME_DELAY),
        ("process-picker", "playback", keys::playback::RESUME_DELAY),
        ("commands", "automation", "postProcessing.0.command"),
        ("integrations", "theme", "integrations.0.reload"),
        ("models", "filter", "semantic.models.0.manifest"),
        ("browser", "sources", keys::selector::WALLHAVEN_COLUMNS),
        ("recovery", "playback", keys::playback::RESUME_DELAY),
    ];
    let mut captures = Vec::new();
    for locale in ["en-US", "ar-SA"] {
        for [width, height] in [[1920u32, 1080u32], [2560, 1440]] {
            for (case, tab, path) in cases {
                let (mut app, submissions) = d3_application_fixture(2);
                app.state.gallery.variant = None;
                app.viewport = [width as f32, height as f32];
                let next = localization::Catalog::new(locale, locale).unwrap();
                localization::reload_ui_font(&app.settings_locale, &next).unwrap();
                app.settings_locale = next;
                {
                    let mut fonts = iced::advanced::graphics::text::font_system()
                        .write()
                        .unwrap();
                    fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                    fonts.load_font(std::borrow::Cow::Borrowed(
                        include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf")
                            .as_slice(),
                    ));
                }
                for mode in [
                    GalleryMode::Slices,
                    GalleryMode::Geometric,
                    GalleryMode::Wall,
                    GalleryMode::Depth,
                ] {
                    let _ = update(&mut app, Message::SetGalleryMode(mode));
                    app.save_gallery_preset().unwrap();
                }
                let mode = match case {
                    "profiles-slices" => GalleryMode::Slices,
                    "profiles-hex" => GalleryMode::Geometric,
                    "profiles-wall" => GalleryMode::Wall,
                    _ => GalleryMode::Depth,
                };
                let _ = update(&mut app, Message::SetGalleryMode(mode));
                app.panel_state.open_settings(SettingsCategory::Picker);
                view::settings::controller::reconcile(&mut app);
                p3_settings_intent(&mut app, SettingsMsg::SelectTab(tab.into()));
                match case {
                    "pending-valid" => {
                        p3_settings_intent(
                            &mut app,
                            SettingsMsg::Input(path.into(), "1.25".into()),
                        );
                        p3_settings_intent(&mut app, SettingsMsg::Commit);
                    }
                    "pending-invalid" => {
                        p3_settings_intent(&mut app, SettingsMsg::Input(path.into(), "-".into()))
                    }
                    "process-picker" => {
                        p3_settings_intent(
                            &mut app,
                            SettingsMsg::Run(composed::ActionId::ChooseRunningProcess),
                        );
                        p3_settings_intent(
                            &mut app,
                            SettingsMsg::ProcessPickerManualInput("owned-game.exe".into()),
                        );
                        p3_settings_intent(&mut app, SettingsMsg::ProcessPickerManualAdd);
                    }
                    "commands" => {
                        p3_settings_intent(
                            &mut app,
                            SettingsMsg::Run(composed::ActionId::AddPostCommand),
                        );
                        p3_settings_intent(
                            &mut app,
                            SettingsMsg::Input(path.into(), "literal pending command".into()),
                        );
                        p3_settings_intent(&mut app, SettingsMsg::Commit);
                    }
                    "integrations" => {
                        p3_settings_intent(
                            &mut app,
                            SettingsMsg::Run(composed::ActionId::AddIntegration),
                        );
                        p3_settings_intent(
                            &mut app,
                            SettingsMsg::Input(path.into(), "literal pending reload".into()),
                        );
                        p3_settings_intent(&mut app, SettingsMsg::Commit);
                    }
                    "models" => {
                        p3_settings_intent(
                            &mut app,
                            SettingsMsg::Run(composed::ActionId::AddSemanticModel),
                        );
                        p3_settings_intent(
                            &mut app,
                            SettingsMsg::Input(path.into(), "owned-missing-manifest.json".into()),
                        );
                        p3_settings_intent(&mut app, SettingsMsg::Commit);
                    }
                    "browser" => {
                        p3_settings_intent(
                            &mut app,
                            SettingsMsg::Input(path.into(), "6.25".into()),
                        );
                        p3_settings_intent(&mut app, SettingsMsg::Commit);
                    }
                    "recovery" => {
                        std::fs::write(
                            app.state_path.with_extension("toml.bak"),
                            format!("version={}\n", candidate_state::CURRENT_STATE_VERSION + 1),
                        )
                        .unwrap();
                        let next = !view::settings::bridge::Source { app: &app }
                            .flag(keys::playback::PROCESS_ENABLED);
                        p3_settings_intent(
                            &mut app,
                            SettingsMsg::Toggle(keys::playback::PROCESS_ENABLED.into(), next),
                        );
                        p3_settings_intent(
                            &mut app,
                            SettingsMsg::Input(path.into(), "2.75".into()),
                        );
                        assert!(app.local_state_recovery_available());
                    }
                    _ => {}
                }
                let cards = view::settings::controller::cards(&app);
                let section = cards
                    .iter()
                    .position(|(_, rows)| rows.iter().any(|row| contains(&row.control, path)))
                    .unwrap_or_else(|| panic!("No actual control for {case}/{path}"));
                app.settings_workbench.navigation.section = section;
                for row in &cards[section].1 {
                    expand(&row.control, &mut app.settings_workbench);
                }
                app.settings_workbench.last_frame = None;
                let origin = std::time::Instant::now();
                for frame in 0..=50 {
                    let window = app.window;
                    let generation = app.settings_workbench.generation;
                    let _ = update(
                        &mut app,
                        Message::SettingsIntent {
                            window,
                            generation,
                            intent: view::settings::folio::intent::Message::Frame(
                                origin + std::time::Duration::from_millis(frame * 50),
                            ),
                        },
                    );
                }
                let window = app.window.unwrap();
                let theme = theme(&app, window);
                let background = application_style(&app, &theme).background_color;
                let mut element = view(&app, window);
                let mut tree = Tree::new(&element);
                let size = iced::Size::new(width as f32, height as f32);
                let node = element.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(size, size),
                );
                assert_eq!(node.size(), size);
                fn finite(node: &layout::Node) {
                    let b = node.bounds();
                    assert!([b.x, b.y, b.width, b.height]
                        .into_iter()
                        .all(f32::is_finite));
                    for child in node.children() {
                        finite(child);
                    }
                }
                finite(&node);
                let bounds = iced::Rectangle::with_size(size);
                renderer.reset(bounds);
                element.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &theme,
                    &iced::advanced::renderer::Style::default(),
                    Layout::new(&node),
                    iced::mouse::Cursor::Unavailable,
                    &bounds,
                );
                let bytes = Headless::screenshot(
                    &mut renderer,
                    iced::Size::new(width, height),
                    1.0,
                    background,
                );
                assert_eq!(bytes.len(), width as usize * height as usize * 4);
                assert!(
                    bytes.chunks_exact(4).any(|pixel| pixel != &bytes[..4]),
                    "Blank {case}/{locale}/{width}"
                );
                let path = output.join(format!("{locale}-{width}x{height}-{case}.png"));
                ::image::save_buffer_with_format(
                    &path,
                    &bytes,
                    width,
                    height,
                    ::image::ColorType::Rgba8,
                    ::image::ImageFormat::Png,
                )
                .unwrap();
                captures.push(serde_json::json!({"case":case,"locale":locale,"width":width,"height":height,"scaleFactor":1.0,"path":path,"profile":app.state_path,"section":section,"rootBounds":[node.bounds().x,node.bounds().y,node.bounds().width,node.bounds().height]}));
                assert_eq!(submissions.load(Ordering::Acquire), 0);
            }
        }
    }
    assert_eq!(captures.len(), 56);
    std::fs::write(output.join("captures.json"),serde_json::to_vec_pretty(&serde_json::json!({"boundary":"Owned headless production layout, not native input/display/DPI or human acceptance","captures":captures})).unwrap()).unwrap();
}

#[test]
#[ignore = "Owned P10 four-mode detail and Depth production layouts; explicit invocation only"]
fn p10_owned_detail_layouts_1080p_1440p() {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout,
    };
    let output =
        std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned output"));
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .expect("headless WGPU");
    let mut captures = Vec::new();
    for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
        for mode in [
            GalleryMode::Slices,
            GalleryMode::Geometric,
            GalleryMode::Wall,
            GalleryMode::Depth,
        ] {
            let (mut app, _) = d4a_fixture(40);
            app.browse_position_restored = true;
            app.viewport = [width as f32, height as f32];
            app.set_gallery_mode(mode);
            app.selected = Some(3);
            app.config.ui.show_top_bar = true;
            app.rail_reveal = 1.0;
            if mode == GalleryMode::Depth {
                app.state.gallery.depth.width = 900.0;
                app.state.gallery.depth.height = 700.0;
                app.state.gallery.depth.visible_count = 3.0;
                app.reset_depth_transients();
                app.depth_presentation = Some(DepthPresentation::target(&app.state));
            }
            app.sync_gallery_presentation();
            if let Some(p) = &mut app.gallery_presentation {
                p.settle();
            }
            if let Some(p) = &mut app.hex_presentation {
                p.settle();
            }
            if let Some(p) = &mut app.wall_presentation {
                p.settle();
                p.entrance = GalleryMotionState::source(1.0, 1.0, 250.0, 1.0);
            }
            app.rebuild_depth_frame();
            app.start_detail(true, "p10-layout");
            app.detail_progress = 0.94;
            app.rebuild_depth_frame();
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let style = application_style(&app, &theme);
            let mut element = view(&app, window);
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            assert_eq!(node.size(), size);
            renderer.reset(bounds);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: style.text_color,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let bytes = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                style.background_color,
            );
            assert_eq!(bytes.len(), width as usize * height as usize * 4);
            let path = output.join(format!(
                "{width}x{height}-{}-detail.png",
                mode.label().to_lowercase()
            ));
            ::image::save_buffer_with_format(
                &path,
                &bytes,
                width,
                height,
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
            captures.push(serde_json::json!({"path":path,"mode":mode.label(),"width":width,"height":height,"detail_progress":0.94,"composition":"production root","boundary":"headless geometry; no native motion or human approval"}));
        }
    }
    std::fs::write(
        output.join("captures.json"),
        serde_json::to_vec_pretty(&captures).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "W1 owned production layouts; explicit invocation only"]
fn w1_owned_surfaces_1080p_1440p() {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout,
    };
    let output = std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").unwrap());
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .unwrap();
    let mut captures = Vec::new();
    for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
        for tab in [
            "picker",
            "picker-appearance",
            "filter",
            "library",
            "automation",
            "language",
            "library-child",
            "automation-child",
        ] {
            let mut app = p3_composed_editor_app(SettingsCategory::Picker);
            app.viewport = [width as f32, height as f32];
            p3_settings_intent(
                &mut app,
                SettingsMsg::SelectTab(tab.split('-').next().unwrap().into()),
            );
            if tab == "picker-appearance" {
                let model = view::settings::bridge::build(&app, "picker");
                let section = model
                    .iter()
                    .position(|(card, _)| card.title == "Appearance")
                    .unwrap();
                p3_settings_intent(&mut app, SettingsMsg::SelectSection(section));
            }
            if tab.ends_with("-child") {
                app.settings_workbench.windows_panel = Some(if tab == "library-child" {
                    SettingsCategory::Library
                } else {
                    SettingsCategory::Automation
                });
            }
            {
                let mut fonts = iced::advanced::graphics::text::font_system()
                    .write()
                    .unwrap();
                fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                fonts.load_font(std::borrow::Cow::Borrowed(
                    include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
                ));
            }
            app.settings_workbench.folio_entrance = None;
            app.settings_workbench.tab_reveal = None;
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let background = application_style(&app, &theme).background_color;
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            assert_eq!(node.size(), size);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: application_style(&app, &theme).text_color,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let bytes = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                background,
            );
            let path = output.join(format!("{width}x{height}-w1-{tab}.png"));
            ::image::save_buffer_with_format(
                &path,
                &bytes,
                width,
                height,
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
            captures.push(serde_json::json!({"path":path,"width":width,"height":height,"tab":tab}));
        }
    }
    std::fs::write(
        output.join("captures.json"),
        serde_json::to_vec_pretty(&captures).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "W4 changed Library production layouts; explicit invocation only"]
fn w4_owned_library_tools_1080p_1440p() {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout,
    };
    let output = std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").unwrap());
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .unwrap();
    let mut captures = Vec::new();
    for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
        for tab in ["library", "library-child"] {
            let mut app = p3_composed_editor_app(SettingsCategory::Picker);
            app.viewport = [width as f32, height as f32];
            p3_settings_intent(
                &mut app,
                SettingsMsg::SelectTab(tab.split('-').next().unwrap().into()),
            );
            if tab == "picker-appearance" {
                let model = view::settings::bridge::build(&app, "picker");
                let section = model
                    .iter()
                    .position(|(card, _)| card.title == "Appearance")
                    .unwrap();
                p3_settings_intent(&mut app, SettingsMsg::SelectSection(section));
            }
            if tab.ends_with("-child") {
                app.settings_workbench.windows_panel = Some(if tab == "library-child" {
                    SettingsCategory::Library
                } else {
                    SettingsCategory::Automation
                });
            }
            {
                let mut fonts = iced::advanced::graphics::text::font_system()
                    .write()
                    .unwrap();
                fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                fonts.load_font(std::borrow::Cow::Borrowed(
                    include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
                ));
            }
            app.settings_workbench.folio_entrance = None;
            app.settings_workbench.tab_reveal = None;
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let background = application_style(&app, &theme).background_color;
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            assert_eq!(node.size(), size);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: application_style(&app, &theme).text_color,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let bytes = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                background,
            );
            let path = output.join(format!("{width}x{height}-w4-{tab}.png"));
            ::image::save_buffer_with_format(
                &path,
                &bytes,
                width,
                height,
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
            captures.push(serde_json::json!({"path":path,"width":width,"height":height,"tab":tab}));
        }
    }
    std::fs::write(
        output.join("captures.json"),
        serde_json::to_vec_pretty(&captures).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "E1 production Search/Library layouts; explicit invocation only"]
fn e1_owned_search_library_1080p_1440p() {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout,
    };
    let output = std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").unwrap());
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .unwrap();
    let mut captures = Vec::new();
    for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
        for tab in ["search", "library-child"] {
            let mut app = if tab == "search" {
                d4a_fixture(3).0
            } else {
                p3_composed_editor_app(SettingsCategory::Picker)
            };
            app.viewport = [width as f32, height as f32];
            p3_settings_intent(
                &mut app,
                SettingsMsg::SelectTab(tab.split('-').next().unwrap().into()),
            );
            if tab == "picker-appearance" {
                let model = view::settings::bridge::build(&app, "picker");
                let section = model
                    .iter()
                    .position(|(card, _)| card.title == "Appearance")
                    .unwrap();
                p3_settings_intent(&mut app, SettingsMsg::SelectSection(section));
            }
            if tab.ends_with("-child") {
                app.settings_workbench.windows_panel = Some(if tab == "library-child" {
                    SettingsCategory::Library
                } else {
                    SettingsCategory::Automation
                });
            }
            if tab == "search" {
                app.panel_state.settings_open = false;
                app.settings_workbench.active = false;
                app.panel_state.search_open = true;
                app.search_tags_open = true;
                app.state.frontend_preferences.tag_panel.width = Some(760.0);
                app.colours.query.bucket = Some(crate::library::color_metadata::ColorBucket::Red);
                let media = output.join("labelled-media");
                std::fs::create_dir_all(&media).unwrap();
                let path = media.join(format!("{width}-red.png"));
                let sample =
                    ::image::RgbaImage::from_pixel(128, 128, ::image::Rgba([255, 0, 0, 255]));
                sample.save(&path).unwrap();
                app.wallpapers[0].file_target = path.clone();
                app.wallpapers[0].preview_path = Some(path.clone());
                app.wallpapers[0].source = WallpaperSource::LocalImage;
                app.wallpapers[0].wallpaper_type = "image".into();
                app.selected = Some(0);
                let still = crate::library::media_still::MediaStill::select(
                    &app.wallpapers[0],
                    app.catalog_generation.load(Ordering::Acquire),
                )
                .unwrap();
                let metadata = crate::library::color_metadata::analyse_sample(
                    &sample,
                    crate::library::color_metadata::hash_bytes(&std::fs::read(path).unwrap()),
                )
                .unwrap();
                app.colours
                    .metadata
                    .insert(still.item_id.clone(), metadata.clone());
                app.colours.entries.insert(
                    still.item_id.clone(),
                    crate::candidate_app::colours::Stored {
                        still: Some(still.clone()),
                        reason: None,
                        persistent: true,
                    },
                );
            }
            app.colours.status =
                "Owned layout fixture · known stills / Unknown poster · no native claim".into();
            {
                let mut fonts = iced::advanced::graphics::text::font_system()
                    .write()
                    .unwrap();
                fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                fonts.load_font(std::borrow::Cow::Borrowed(
                    include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
                ));
            }
            app.settings_workbench.folio_entrance = None;
            app.settings_workbench.tab_reveal = None;
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let background = application_style(&app, &theme).background_color;
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            assert_eq!(node.size(), size);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: application_style(&app, &theme).text_color,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let bytes = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                background,
            );
            let path = output.join(format!("{width}x{height}-w5-{tab}.png"));
            ::image::save_buffer_with_format(
                &path,
                &bytes,
                width,
                height,
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
            captures.push(serde_json::json!({"path":path,"width":width,"height":height,"tab":tab}));
        }
    }
    std::fs::write(
        output.join("w5-captures.json"),
        serde_json::to_vec_pretty(&captures).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "Effects retirement affected detail/settings/binding layouts; explicit invocation only"]
fn retirement_owned_detail_settings_bindings_1080p_1440p() {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::Tree,
        Layout,
    };
    let output = std::path::PathBuf::from(
        std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned layout output"),
    );
    std::fs::create_dir_all(&output).unwrap();
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .unwrap();
    let mut captures = Vec::new();
    for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
        for case in ["detail", "windows-settings", "bindings"] {
            let mut app = if case == "detail" {
                d4a_fixture(4).0
            } else {
                p3_composed_editor_app(SettingsCategory::Picker)
            };
            app.viewport = [width as f32, height as f32];
            app.launch_policy = p1_preview_policy();
            if case == "detail" {
                app.panel_state.settings_open = false;
                app.settings_workbench.active = false;
                app.selected = Some(0);
                app.detail_open = true;
                app.detail_progress = 1.0;
                assert!(view::detail_actions(&app, &app.wallpapers[0])
                    .iter()
                    .all(|a| !a.label.contains("Effects")));
            } else if case == "windows-settings" {
                let window = app.window;
                let generation = app.settings_workbench.generation;
                let _ = update(
                    &mut app,
                    Message::SettingsWindowsOpen {
                        window,
                        generation,
                        category: SettingsCategory::WallpapersDisplays,
                    },
                );
                assert_eq!(
                    app.settings_workbench.windows_panel,
                    Some(SettingsCategory::WallpapersDisplays)
                );
            } else {
                p3_settings_intent(&mut app, SettingsMsg::SelectTab("picker".into()));
                p3_settings_intent(
                    &mut app,
                    SettingsMsg::SelectSection(composed::PICKER_CONTROLS_SECTION),
                );
                assert_eq!(
                    app.settings_workbench.navigation.section,
                    composed::PICKER_CONTROLS_SECTION,
                );
                assert!(composed::KEY_BINDINGS
                    .iter()
                    .all(|b| b.path != "keys.studio"));
            }
            app.status = Some("Owned retirement layout fixture; no provider trial".into());
            app.settings_workbench.folio_entrance = None;
            app.settings_workbench.tab_reveal = None;
            {
                let mut fonts = iced::advanced::graphics::text::font_system()
                    .write()
                    .unwrap();
                fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                fonts.load_font(std::borrow::Cow::Borrowed(
                    include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
                ));
            }
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let background = application_style(&app, &theme).background_color;
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            renderer.reset(bounds);
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            assert_eq!(node.size(), size);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: application_style(&app, &theme).text_color,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let bytes = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                background,
            );
            let path = output.join(format!("{width}x{height}-retirement-{case}.png"));
            ::image::save_buffer_with_format(
                &path,
                &bytes,
                width,
                height,
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
            captures.push(serde_json::json!({"path":path,"width":width,"height":height,"case":case,"injected":true,"providerTrial":false}));
        }
    }
    std::fs::write(
        output.join("retirement-captures.json"),
        serde_json::to_vec_pretty(&captures).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "Search/tag replacement production bounds, scrolling/hits; explicit owned invocation"]
fn search_tags_owned_drawer_and_filter_layouts_1080p_1440p() {
    let long_zero_tag =
        "選択済み零件非常に長い中文标签 repeated long selected zero-count tag".repeat(3);
    use iced::advanced::{
        clipboard, layout,
        renderer::{Headless, Renderer as _},
        widget::{operation, Operation, Tree},
        Layout, Shell,
    };
    #[derive(Default)]
    struct Probe {
        offset: iced::Vector,
        pending: Option<iced::Vector>,
        texts: Vec<(String, iced::Rectangle)>,
        inputs: Vec<(Option<iced::widget::Id>, iced::Rectangle)>,
        containers: Vec<(Option<iced::widget::Id>, iced::Rectangle)>,
        vertical: Vec<(iced::Rectangle, iced::Rectangle)>,
        toolbar: Option<(iced::Rectangle, iced::Rectangle)>,
        filter_body: Option<iced::Rectangle>,
        filter_content_height: f32,
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
        fn scrollable(
            &mut self,
            id: Option<&iced::widget::Id>,
            bounds: iced::Rectangle,
            content: iced::Rectangle,
            translation: iced::Vector,
            _: &mut dyn operation::Scrollable,
        ) {
            if content.height > bounds.height + 1.0 {
                self.vertical.push((bounds - self.offset, content));
            }
            if id == Some(&iced::widget::Id::new("search-tag-toolbar-scroll")) {
                assert!(self.toolbar.is_none(), "exactly one Search toolbar");
                self.toolbar = Some((bounds - self.offset, content));
            }
            if id == Some(&iced::widget::Id::new("r5-search-body")) {
                self.filter_body = Some(bounds - self.offset);
                self.filter_content_height = content.height;
            }
            self.pending = Some(translation);
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
    let mut receipts = Vec::new();
    for (width, height, scale, locale, panel_width, tag_count) in [
        (1920u32, 1080u32, 1.0, "en-US", 760.0, 48),
        (2560, 1440, 1.0, "en-US", 760.0, 48),
        (1920, 1080, 1.25, "zh-CN", 760.0, 48),
        (1920, 1080, 1.0, "en-US", 360.0, 48),
        (1920, 1080, 1.0, "en-US", 540.0, 48),
        (1920, 1080, 1.0, "en-US", 760.0, 0),
        (1920, 1080, 1.0, "en-US", 760.0, 1),
        (1920, 1080, 1.0, "en-US", 360.0, 1),
        (1920, 1080, 0.5, "en-US", 540.0, 1),
        (1920, 1080, 0.5, "en-US", 760.0, 48),
        (1920, 1080, 0.5, "zh-CN", 760.0, 49),
        (1920, 1080, 1.0, "en-US", 540.0, 0),
    ] {
        for editor in [false, true] {
            if (panel_width < 760.0 || tag_count != 48) && editor {
                continue;
            }
            let (mut app, submissions) = d4a_fixture(3);
            app.viewport = [width as f32, height as f32];
            app.launch_policy = p1_preview_policy();
            app.state.frontend_preferences.interface.ui_scale = Some(scale);
            app.state.frontend_preferences.tag_panel.width = Some(panel_width);
            app.settings_locale = localization::Catalog::new(locale, locale).unwrap();
            app.panel_state.search_open = true;
            app.search_tags_open = true;
            app.wallpapers[0].title = include_str!("../../fixtures/e2-workshop-title.txt")
                .trim()
                .into();
            app.catalog[0].source_tags = (0..tag_count)
                .map(|i| format!("fixture-tag-{i:02}"))
                .collect();
            app.project_local_tags();
            if tag_count == 49 {
                app.filter_rule.tags.insert(long_zero_tag.clone());
                app.filter_rule.excluded_tags.insert("excluded-zero".into());
                app.refresh_filter();
                assert!(app.filtered_indices.is_empty());
            }
            if tag_count == 0 && panel_width == 540.0 {
                let _ = update(&mut app, Message::SearchChanged("no-sparse-match".into()));
                assert!(app.selected.is_none() && app.filtered_indices.is_empty());
            }
            if editor {
                let item = app.wallpapers[0].id.clone();
                let _ = update(&mut app, Message::OpenLocalTags(item));
            }
            {
                let mut fonts = iced::advanced::graphics::text::font_system()
                    .write()
                    .unwrap();
                fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
            }
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let style = application_style(&app, &theme);
            let size = iced::Size::new(width as f32, height as f32);
            let viewport = iced::Rectangle::with_size(size);
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            let mut first = Probe::default();
            element
                .as_widget_mut()
                .operate(&mut tree, Layout::new(&node), &renderer, &mut first);
            let id = if editor {
                "local-tag-editor"
            } else {
                "search-tag-panel"
            };
            let panel = first
                .containers
                .iter()
                .find(|(key, _)| key.as_ref() == Some(&iced::widget::Id::new(id)))
                .unwrap()
                .1;
            assert!(
                panel.x >= 0.0
                    && panel.y >= 0.0
                    && panel.x + panel.width <= width as f32 + 0.1
                    && panel.y + panel.height <= height as f32 + 0.1
            );
            let input_id = if editor {
                &app.local_tag_input_id
            } else {
                &app.search_input_id
            };
            let input = first
                .inputs
                .iter()
                .find(|(id, _)| id.as_ref() == Some(input_id))
                .unwrap()
                .1;
            assert!(
                panel.contains(input.position())
                    && input.y + input.height <= panel.y + panel.height
            );
            let mut messages = Vec::new();
            if !editor {
                renderer.reset(viewport);
                element.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &theme,
                    &iced::advanced::renderer::Style {
                        text_color: style.text_color,
                    },
                    Layout::new(&node),
                    iced::mouse::Cursor::Unavailable,
                    &viewport,
                );
                let bytes = Headless::screenshot(
                    &mut renderer,
                    iced::Size::new(width, height),
                    1.0,
                    style.background_color,
                );
                ::image::save_buffer_with_format(output.join(format!("tags-{width}x{height}-{scale}-{locale}-w{panel_width}-n{tag_count}-initial.png")), &bytes, width, height, ::image::ColorType::Rgba8, ::image::ImageFormat::Png).unwrap();
            }
            if editor {
                assert!(
                    !first
                        .inputs
                        .iter()
                        .any(|(id, _)| id.as_ref() == Some(&app.search_input_id)),
                    "editing is distinct from filtering"
                );
                let body = first.vertical[0].0;
                assert!(body.height > 40.0 && body.y >= panel.y && body.y + body.height < input.y);
                let mut reached = std::collections::BTreeSet::new();
                // Overlapping observations cannot skip rows at the minimum UI scale.
                let steps = (first.vertical[0].1.height / (body.height * 0.5)).ceil() as usize + 1;
                for step in 0..=steps {
                    let mut probe = Probe::default();
                    element.as_widget_mut().operate(
                        &mut tree,
                        Layout::new(&node),
                        &renderer,
                        &mut probe,
                    );
                    let fixed = probe
                        .inputs
                        .iter()
                        .find(|(id, _)| id.as_ref() == Some(&app.local_tag_input_id))
                        .unwrap()
                        .1;
                    assert_eq!(
                        fixed, input,
                        "Add field remains reachable while rows scroll"
                    );
                    for (label, bounds) in &probe.texts {
                        if label.starts_with("fixture-tag-") && body.contains(bounds.center()) {
                            reached.insert(label.clone());
                        }
                    }
                    if step == 0 {
                        let remove = probe
                            .texts
                            .iter()
                            .find(|(text, r)| text == "×" && body.contains(r.center()))
                            .unwrap()
                            .1
                            .center();
                        for event in [
                            iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),
                            iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left),
                        ] {
                            element.as_widget_mut().update(
                                &mut tree,
                                &iced::Event::Mouse(event),
                                Layout::new(&node),
                                iced::mouse::Cursor::Available(remove),
                                &renderer,
                                &mut clipboard::Null,
                                &mut Shell::new(&mut messages),
                                &viewport,
                            );
                        }
                    }
                    if step == 0 || step == steps {
                        renderer.reset(viewport);
                        element.as_widget().draw(
                            &tree,
                            &mut renderer,
                            &theme,
                            &iced::advanced::renderer::Style {
                                text_color: style.text_color,
                            },
                            Layout::new(&node),
                            iced::mouse::Cursor::Unavailable,
                            &viewport,
                        );
                        let bytes = Headless::screenshot(
                            &mut renderer,
                            iced::Size::new(width, height),
                            1.0,
                            style.background_color,
                        );
                        ::image::save_buffer_with_format(
                            output.join(format!(
                                "tags-{width}x{height}-{scale}-{locale}-w{panel_width}-n{tag_count}-editor-{step}.png"
                            )),
                            &bytes,
                            width,
                            height,
                            ::image::ColorType::Rgba8,
                            ::image::ImageFormat::Png,
                        )
                        .unwrap();
                    }
                    element.as_widget_mut().update(
                        &mut tree,
                        &iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                            delta: iced::mouse::ScrollDelta::Pixels {
                                x: 0.0,
                                y: -body.height * 0.5,
                            },
                        }),
                        Layout::new(&node),
                        iced::mouse::Cursor::Available(body.center()),
                        &renderer,
                        &mut clipboard::Null,
                        &mut Shell::new(&mut messages),
                        &viewport,
                    );
                }
                assert_eq!(reached.len(), 48, "every removal row reachable");
                assert!(messages.iter().any(|m|matches!(m,Message::LocalTag{edit:crate::candidate_app::local_library::TagEdit::Remove(tag),..} if tag=="fixture-tag-00")));
                receipts.push(serde_json::json!({"viewport":[width,height],"scale":scale,"locale":locale,"editor":editor,"panel":format!("{panel:?}"),"fixed_input":format!("{input:?}"),"reached_tags":reached.len(),"remove_hit":true}));
            } else {
                if tag_count <= 1 {
                    let body = first.filter_body.expect("actual filter body");
                    let row = first
                        .texts
                        .iter()
                        .find(|(label, _)| {
                            if tag_count == 0 {
                                label == app.settings_locale.text_static("r5-panel-copy-10")
                            } else {
                                label.starts_with("fixture-tag-00")
                            }
                        })
                        .expect("sparse tag/empty row")
                        .1;
                    assert!(row.x>=body.x-0.1 && row.y>=body.y-0.1 && row.x+row.width<=body.x+body.width+0.1 && row.y+row.height<=body.y+body.height+0.1,"sparse tag must be fully visible without scrolling: {row:?}, body {body:?}");
                    if tag_count == 1 {
                        for event in [
                            iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),
                            iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left),
                        ] {
                            element.as_widget_mut().update(
                                &mut tree,
                                &iced::Event::Mouse(event),
                                Layout::new(&node),
                                iced::mouse::Cursor::Available(row.center()),
                                &renderer,
                                &mut clipboard::Null,
                                &mut Shell::new(&mut messages),
                                &viewport,
                            );
                        }
                        assert!(messages.iter().any(|m| matches!(m,Message::SearchTag(tag,false) if tag=="fixture-tag-00")),"fully visible sparse chip must publish its filter action");
                    }
                    receipts.push(serde_json::json!({"panel_width":panel_width,"tag_count":tag_count,"sparse_row":format!("{row:?}"),"body":format!("{body:?}"),"no_scroll_required":true,"tag_hit":tag_count==1,"selected":app.selected}));
                }
                for label in [
                    app.settings_locale.text_static("tags-match"),
                    app.settings_locale.text_static("tags-order"),
                ] {
                    assert!(
                        first.texts.iter().any(|(t, _)| t == label),
                        "retained control {label}"
                    );
                }
                {
                    let (toolbar, content) = first
                        .toolbar
                        .expect("actual named Search toolbar must exist");
                    assert!(
                        toolbar.x >= panel.x
                            && toolbar.y >= panel.y
                            && toolbar.x + toolbar.width <= panel.x + panel.width + 0.1
                            && toolbar.y + toolbar.height <= input.y,
                        "toolbar must be inside panel above stable Search field"
                    );
                    let required = [
                        app.settings_locale.text_static("tags-match"),
                        app.settings_locale.text_static("tags-order"),
                        app.settings_locale.text_static("r5-panel-copy-06"),
                        app.settings_locale.text_static("r5-panel-copy-07"),
                        app.settings_locale.text_static("r5-panel-copy-09"),
                        "A–Z",
                        app.settings_locale.text_static("r5-panel-copy-04"),
                        "ESC",
                    ];
                    let mut reached = std::collections::BTreeSet::new();
                    for _ in 0..=12 {
                        let mut probe = Probe::default();
                        element.as_widget_mut().operate(
                            &mut tree,
                            Layout::new(&node),
                            &renderer,
                            &mut probe,
                        );
                        assert_eq!(probe.toolbar.unwrap().0, toolbar);
                        assert_eq!(
                            probe
                                .inputs
                                .iter()
                                .find(|(id, _)| id.as_ref() == Some(&app.search_input_id))
                                .unwrap()
                                .1,
                            input
                        );
                        for (label, bounds) in &probe.texts {
                            if toolbar.contains(bounds.center())
                                && bounds.x >= toolbar.x - 0.1
                                && bounds.x + bounds.width <= toolbar.x + toolbar.width + 0.1
                                && bounds.y >= toolbar.y - 0.1
                                && bounds.y + bounds.height <= toolbar.y + toolbar.height + 0.1
                            {
                                reached.insert(label.clone());
                                if label == "A–Z"
                                    || label == app.settings_locale.text_static("search-clear-tags")
                                {
                                    for event in [
                                        iced::mouse::Event::ButtonPressed(
                                            iced::mouse::Button::Left,
                                        ),
                                        iced::mouse::Event::ButtonReleased(
                                            iced::mouse::Button::Left,
                                        ),
                                    ] {
                                        element.as_widget_mut().update(
                                            &mut tree,
                                            &iced::Event::Mouse(event),
                                            Layout::new(&node),
                                            iced::mouse::Cursor::Available(bounds.center()),
                                            &renderer,
                                            &mut clipboard::Null,
                                            &mut Shell::new(&mut messages),
                                            &viewport,
                                        );
                                    }
                                }
                            }
                        }
                        element.as_widget_mut().update(
                            &mut tree,
                            &iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                                delta: iced::mouse::ScrollDelta::Pixels { x: -100.0, y: 0.0 },
                            }),
                            Layout::new(&node),
                            iced::mouse::Cursor::Available(toolbar.center()),
                            &renderer,
                            &mut clipboard::Null,
                            &mut Shell::new(&mut messages),
                            &viewport,
                        );
                    }
                    for label in required {
                        assert!(reached.contains(label),"toolbar action {label} unreachable inside {panel_width}px panel; reached {reached:?}");
                    }
                    assert!(
                        messages.iter().any(|m| matches!(m, Message::ToggleTagSort)),
                        "distinct A–Z control dispatches the existing sort reducer"
                    );
                    assert_eq!(
                        messages
                            .iter()
                            .any(|m| matches!(m, Message::ClearSearchTags)),
                        tag_count == 49,
                        "conditional Clear tags retains its real action"
                    );
                    receipts.push(serde_json::json!({"viewport":[width,height],"scale":scale,"locale":locale,"panel_width":panel_width,"toolbar_reached":reached,"toolbar_bounds":format!("{toolbar:?}"),"content_bounds":format!("{content:?}")}));
                }
                let body = first.filter_body.unwrap();
                assert!(
                    input.width > panel.width * 0.50,
                    "integrated query fills the strip after Tags and magnifier"
                );
                let clear = app.settings_locale.text_static("search-clear-tags");
                assert_eq!(
                    first.texts.iter().any(|(label, _)| label == clear),
                    tag_count == 49,
                    "Clear tags appears only for an active tag constraint"
                );
                if tag_count >= 48 {
                    let mut reached = std::collections::BTreeSet::new();
                    let steps =
                        (first.filter_content_height / (body.height * 0.5)).ceil() as usize + 1;
                    for _ in 0..=steps {
                        let mut probe = Probe::default();
                        element.as_widget_mut().operate(
                            &mut tree,
                            Layout::new(&node),
                            &renderer,
                            &mut probe,
                        );
                        assert_eq!(
                            probe
                                .inputs
                                .iter()
                                .find(|(id, _)| id.as_ref() == Some(&app.search_input_id))
                                .unwrap()
                                .1,
                            input
                        );
                        for (label, bounds) in &probe.texts {
                            if body.contains(bounds.center()) {
                                reached.insert(label.clone());
                                if tag_count == 49 && label.starts_with("選択済み零件") {
                                    assert!(
                                        label.contains('…') && label.ends_with("  0"),
                                        "elide only the long name; preserve its visible count"
                                    );
                                    assert!(bounds.x >= body.x - 0.1 && bounds.x + bounds.width <= body.x + body.width - 10.0 + 0.1 && bounds.y >= body.y - 0.1 && bounds.y + bounds.height <= body.y + body.height + 0.1, "entire shaped long-chip label/count must fit: {bounds:?}, {body:?}");
                                    for event in [
                                        iced::mouse::Event::ButtonPressed(
                                            iced::mouse::Button::Left,
                                        ),
                                        iced::mouse::Event::ButtonReleased(
                                            iced::mouse::Button::Left,
                                        ),
                                    ] {
                                        element.as_widget_mut().update(
                                            &mut tree,
                                            &iced::Event::Mouse(event),
                                            Layout::new(&node),
                                            iced::mouse::Cursor::Available(bounds.center()),
                                            &renderer,
                                            &mut clipboard::Null,
                                            &mut Shell::new(&mut messages),
                                            &viewport,
                                        );
                                    }
                                }
                            }
                        }
                        element.as_widget_mut().update(
                            &mut tree,
                            &iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                                delta: iced::mouse::ScrollDelta::Pixels {
                                    x: 0.0,
                                    y: -body.height * 0.5,
                                },
                            }),
                            Layout::new(&node),
                            iced::mouse::Cursor::Available(body.center()),
                            &renderer,
                            &mut clipboard::Null,
                            &mut Shell::new(&mut messages),
                            &viewport,
                        );
                    }
                    assert_eq!(
                        reached
                            .iter()
                            .filter(|label| label.starts_with("fixture-tag-"))
                            .count(),
                        if tag_count == 49 { 0 } else { tag_count },
                        "all current co-occurring tags remain independently reachable"
                    );
                    if tag_count == 49 {
                        assert!(reached.iter().any(
                            |label| label.starts_with("選択済み零件") && label.ends_with("  0")
                        ));
                        assert!(messages.iter().any(|m| matches!(m, Message::SearchTag(tag, false) if tag == &long_zero_tag)), "fitted chip dispatches the complete original tag");
                        assert!(reached.contains("− excluded-zero  0"));
                    }
                    receipts.push(serde_json::json!({"scale":scale,"locale":locale,"panel_width":panel_width,"tag_count":tag_count,"tag_scroll_reached":reached,"query_fixed":true}));
                }
                renderer.reset(viewport);
                element.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &theme,
                    &iced::advanced::renderer::Style {
                        text_color: style.text_color,
                    },
                    Layout::new(&node),
                    iced::mouse::Cursor::Unavailable,
                    &viewport,
                );
                let bytes = Headless::screenshot(
                    &mut renderer,
                    iced::Size::new(width, height),
                    1.0,
                    style.background_color,
                );
                ::image::save_buffer_with_format(
                    output.join(format!(
                        "tags-{width}x{height}-{scale}-{locale}-w{panel_width}-n{tag_count}-filter.png"
                    )),
                    &bytes,
                    width,
                    height,
                    ::image::ColorType::Rgba8,
                    ::image::ImageFormat::Png,
                )
                .unwrap();
            }
            drop(element);
            if tag_count == 1 && !editor {
                for message in messages {
                    if matches!(message, Message::SearchTag(_, false)) {
                        let _ = update(&mut app, message);
                    }
                }
                assert_eq!(
                    app.filtered_indices,
                    vec![0],
                    "real sparse chip filters the fixture catalogue"
                );
            }
            assert_eq!(submissions.load(Ordering::Acquire), 0);
        }
    }
    std::fs::write(
        output.join("search-tags-layout-receipt.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
}

#[test]
fn r4_theme_compactness_uses_reading_width_and_localized_all50_roles() {
    assert!(!view::settings::theme_designer::compact(
        [1920.0, 1080.0],
        0.9
    ));
    assert!(!view::settings::theme_designer::compact(
        [2560.0, 1440.0],
        0.9
    ));
    assert!(
        view::settings::theme_designer::compact([1040.0, 1080.0], 0.9),
        "a window wider than 780 still has a compact reading area"
    );
    assert!(
        view::settings::theme_designer::compact([640.0, 600.0], 0.5),
        "minimum-scale text needs the single-column reading layout"
    );
    for locale in ["en-US", "zh-CN"] {
        let mut app = super::theme_workflow::designer_app();
        app.settings_locale = localization::Catalog::new(locale, locale).unwrap();
        let applied = app.state.applied_theme.clone();
        for variant in theme_roles::ColorVariant::ALL {
            for role in theme_roles::ThemeRole::ALL {
                let i = app.theme_session.editor_instance().unwrap();
                let _ = update(
                    &mut app,
                    Message::ThemeVariantSelected {
                        editor_instance: i,
                        variant,
                    },
                );
                let _ = update(
                    &mut app,
                    Message::ThemeRoleSelected {
                        editor_instance: i,
                        role,
                    },
                );
                let element = view::settings::gateway::view(&app);
                let _ = iced::advanced::widget::Tree::new(element.as_widget());
                assert_eq!(app.theme_session.editor_selected_role(), Some(role));
                assert_eq!(app.theme_session.editor_selected_variant(), Some(variant));
            }
        }
        assert_eq!(app.state.applied_theme, applied);
    }
}

#[test]
#[ignore = "PK-10 owned production Theme and Settings composition; explicit invocation only"]
fn r4_owned_theme_designer_layouts_and_reachability() {
    use crate::candidate_app::theme_workflow::{Action, Binding, Load};
    use iced::advanced::{
        clipboard, layout,
        renderer::{Headless, Renderer as _},
        widget::{operation, Operation, Tree},
        Layout, Shell,
    };
    #[derive(Default)]
    struct Probe {
        offset: iced::Vector,
        owner: Option<iced::widget::Id>,
        pending: Option<(iced::Vector, Option<iced::widget::Id>)>,
        scrolls: Vec<(
            iced::widget::Id,
            iced::Rectangle,
            iced::Rectangle,
            iced::Vector,
        )>,
        texts: Vec<(Option<iced::widget::Id>, String, iced::Rectangle)>,
        close_bounds: Option<iced::Rectangle>,
        starter_bounds: Option<iced::Rectangle>,
        proof_bounds: std::collections::BTreeMap<&'static str, iced::Rectangle>,
        inputs: Vec<(Option<iced::widget::Id>, iced::Rectangle)>,
    }
    impl Operation for Probe {
        fn container(&mut self, id: Option<&iced::widget::Id>, bounds: iced::Rectangle) {
            if id == Some(&iced::widget::Id::new("folio-close")) {
                self.close_bounds = Some(bounds - self.offset);
            }
            if id
                == Some(&iced::widget::Id::new(
                    view::settings::theme_designer::STARTER_PICKER,
                ))
            {
                self.starter_bounds = Some(bounds - self.offset);
            }
            for name in [
                "theme-proof-miniature",
                "theme-proof-selected",
                "theme-proof-secondary",
                "theme-proof-status",
            ] {
                if id == Some(&iced::widget::Id::new(name)) {
                    self.proof_bounds.insert(name, bounds - self.offset);
                }
            }
        }
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
            let offset = self.offset;
            let owner = self.owner.clone();
            if let Some((delta, next)) = self.pending.take() {
                self.offset += delta;
                self.owner = next;
            }
            visit(self);
            self.offset = offset;
            self.owner = owner;
        }
        fn scrollable(
            &mut self,
            id: Option<&iced::widget::Id>,
            bounds: iced::Rectangle,
            content: iced::Rectangle,
            translation: iced::Vector,
            _: &mut dyn operation::Scrollable,
        ) {
            if let Some(id) = id {
                self.scrolls
                    .push((id.clone(), bounds - self.offset, content, translation));
            }
            self.pending = Some((translation, id.cloned()));
        }
        fn text(&mut self, _: Option<&iced::widget::Id>, bounds: iced::Rectangle, value: &str) {
            self.texts
                .push((self.owner.clone(), value.into(), bounds - self.offset));
        }
        fn text_input(
            &mut self,
            id: Option<&iced::widget::Id>,
            bounds: iced::Rectangle,
            _: &mut dyn operation::TextInput,
        ) {
            self.inputs.push((id.cloned(), bounds - self.offset));
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
    fn deliver(
        element: &mut Element<'_, Message>,
        tree: &mut Tree,
        node: &layout::Node,
        renderer: &iced::Renderer,
        bounds: iced::Rectangle,
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
            &bounds,
        );
        messages
    }
    fn click(
        element: &mut Element<'_, Message>,
        tree: &mut Tree,
        node: &layout::Node,
        renderer: &iced::Renderer,
        bounds: iced::Rectangle,
        point: iced::Point,
    ) -> Vec<Message> {
        let mut messages = Vec::new();
        for event in [
            iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),
            iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left),
        ] {
            messages.extend(deliver(element, tree, node, renderer, bounds, point, event));
        }
        messages
    }
    fn unbind(message: Message) -> Message {
        match message {
            Message::SettingsLegacy { message, .. } => unbind(*message),
            m => m,
        }
    }
    fn fits(outer: iced::Rectangle, inner: iced::Rectangle) -> bool {
        inner.x >= outer.x - 0.1
            && inner.y >= outer.y - 0.1
            && inner.x + inner.width <= outer.x + outer.width + 0.1
            && inner.y + inner.height <= outer.y + outer.height + 0.1
    }
    fn menu_bounds(layout: Layout<'_>, picker: iced::Rectangle) -> Option<iced::Rectangle> {
        let bounds = layout.bounds();
        if (bounds.width - picker.width).abs() < 1.0 && bounds.x == picker.x {
            return Some(bounds);
        }
        layout
            .children()
            .find_map(|child| menu_bounds(child, picker))
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
    let mut receipts = Vec::new();
    for locale in ["en-US", "zh-CN"] {
        for (width, height, scale) in [
            (1920u32, 1080u32, 1.0f64),
            (2560, 1440, 1.0),
            (1920, 1080, 0.5),
            (1040, 768, 1.0),
            (640, 600, 1.0),
            (640, 600, 0.5),
        ] {
            let mut app = super::theme_workflow::designer_app();
            app.viewport = [width as f32, height as f32];
            app.state.frontend_preferences.interface.ui_scale = Some(scale);
            app.launch_policy = p1_preview_policy();
            let next = localization::Catalog::new(locale, locale).unwrap();
            localization::reload_ui_font(&app.settings_locale, &next).unwrap();
            app.settings_locale = next;
            let final_name = "Owned final library record with a long descriptive name 23";
            for n in 0..24 {
                let name = if n == 23 {
                    final_name.to_string()
                } else {
                    format!("Owned named Theme {n:02}")
                };
                app.state
                    .theme_library
                    .save(None, &name, candidate_state::frontend::nord_document())
                    .unwrap();
            }
            super::theme_workflow::designer_name(
                &mut app,
                "Owned long Theme name for source style presentation and persistence",
            );
            view::settings::controller::reconcile(&mut app);
            {
                let mut fonts = iced::advanced::graphics::text::font_system()
                    .write()
                    .unwrap();
                fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                fonts.load_font(std::borrow::Cow::Borrowed(
                    include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
                ));
            }
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let style = application_style(&app, &theme);
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            let initial = probe(&mut element, &mut tree, &node, &renderer);
            assert_eq!(
                initial.scrolls.len(),
                2,
                "only independent index and reading scrollers"
            );
            let capture = |case: &str,
                           element: &mut Element<'_, Message>,
                           tree: &mut Tree,
                           node: &layout::Node,
                           renderer: &mut iced::Renderer| {
                renderer.reset(bounds);
                element.as_widget().draw(
                    tree,
                    renderer,
                    &theme,
                    &iced::advanced::renderer::Style {
                        text_color: style.text_color,
                    },
                    Layout::new(node),
                    iced::mouse::Cursor::Unavailable,
                    &bounds,
                );
                if let Some(overlay) = element.as_widget_mut().overlay(
                    tree,
                    Layout::new(node),
                    renderer,
                    &bounds,
                    iced::Vector::ZERO,
                ) {
                    let mut overlay = iced::advanced::overlay::Nested::new(overlay);
                    let overlay_node = overlay.layout(renderer, size);
                    overlay.draw(
                        renderer,
                        &theme,
                        &iced::advanced::renderer::Style {
                            text_color: style.text_color,
                        },
                        Layout::new(&overlay_node),
                        iced::mouse::Cursor::Unavailable,
                    );
                }
                let bytes = Headless::screenshot(
                    renderer,
                    iced::Size::new(width, height),
                    1.0,
                    style.background_color,
                );
                let path =
                    output.join(format!("pk10-{locale}-{width}x{height}-{scale}-{case}.png"));
                ::image::save_buffer_with_format(
                    &path,
                    &bytes,
                    width,
                    height,
                    ::image::ColorType::Rgba8,
                    ::image::ImageFormat::Png,
                )
                .unwrap();
                path
            };
            let index_heading = app.settings_locale.text("theme-designer-index-title");
            let index_heading_bounds = initial
                .texts
                .iter()
                .find(|(_, text, _)| text == &index_heading)
                .expect("Theme index heading")
                .2;
            assert!(
                index_heading_bounds.height >= 20.0 * scale.max(0.95) as f32,
                "Theme index uses the pinned legible type scale: {index_heading_bounds:?}"
            );
            let miniature = initial.proof_bounds["theme-proof-miniature"];
            for (key, container_id) in [
                ("theme-designer-proof-selected", "theme-proof-selected"),
                ("theme-designer-proof-secondary", "theme-proof-secondary"),
                ("theme-designer-proof-status", "theme-proof-status"),
                ("theme-designer-proof-roles", "theme-proof-status"),
            ] {
                let caption = app.settings_locale.text(key);
                let text = initial
                    .texts
                    .iter()
                    .find(|(owner, text, _)| {
                        owner.as_ref()
                            == Some(&iced::widget::Id::new(
                                view::settings::theme_designer::READING_SCROLL,
                            ))
                            && text == &caption
                    })
                    .expect("proof caption")
                    .2;
                let container = initial.proof_bounds[container_id];
                assert!(
                    fits(miniature, container) && fits(container, text),
                    "{locale} {width}×{height} at {scale}: {caption} {text:?} in {container:?} inside {miniature:?}"
                );
            }
            let close = initial.close_bounds.expect("current close container");
            assert!(
                (close.width - close.height).abs() < 0.1,
                "square close: {close:?}"
            );
            let glyph = initial.texts.iter().find(|(_, s, _)| s == "×").unwrap().2;
            assert!((glyph.center().x - close.center().x).abs() < 1.0);
            assert!((glyph.center().y - close.center().y).abs() < 1.0);
            let close_click = click(
                &mut element,
                &mut tree,
                &node,
                &renderer,
                bounds,
                iced::Point::new(close.x + 2.0, close.y + 2.0),
            );
            assert!(
                close_click
                    .into_iter()
                    .map(unbind)
                    .any(|message| matches!(message, Message::SettingsWindowsClose { .. })),
                "Theme Close shares the visible corner hit"
            );
            assert!(!initial
                .texts
                .iter()
                .any(|(_, s, _)| s == "Background appearance" || s == "Live desktop-through"));
            for (_, body, _, _) in &initial.scrolls {
                for y in [10000.0, -10000.0, -10000.0, 10000.0, 10000.0] {
                    let messages = deliver(
                        &mut element,
                        &mut tree,
                        &node,
                        &renderer,
                        bounds,
                        body.center(),
                        iced::mouse::Event::WheelScrolled {
                            delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y },
                        },
                    );
                    assert!(messages.into_iter().map(unbind).all(|m| matches!(m, Message::SheetBackdrop)),
                        "production Theme wheel must never emit Close, including at both boundaries");
                }
            }
            // Start the reachability pass at the top after boundary exhaustion.
            let top = capture("designer", &mut element, &mut tree, &node, &mut renderer);
            let mut reached = std::collections::BTreeSet::new();
            let mut last = initial;
            let mut paths = vec![top];
            let mut role_messages = Vec::new();
            let mut load_messages = Vec::new();
            let mut name_captured = false;
            let mut proof_captured = false;
            let mut starter_reached = Vec::new();
            for (id, case) in [
                (view::settings::theme_designer::INDEX_SCROLL, "index-end"),
                (
                    view::settings::theme_designer::READING_SCROLL,
                    "library-end",
                ),
            ] {
                let id = iced::widget::Id::new(id);
                let body = last.scrolls.iter().find(|s| s.0 == id).unwrap().1;
                assert!(
                    body.x >= 0.0
                        && body.y >= 0.0
                        && body.x + body.width <= size.width
                        && body.y + body.height <= size.height
                );
                let steps = (last.scrolls.iter().find(|s| s.0 == id).unwrap().2.height / 100.0)
                    .ceil() as usize
                    + 2;
                for _ in 0..steps {
                    for (owner, text, rect) in &last.texts {
                        if owner.as_ref() == Some(&id)
                            && rect.y >= body.y - 0.1
                            && rect.y + rect.height <= body.y + body.height + 0.1
                        {
                            assert!(
                                rect.x >= body.x - 0.1
                                    && rect.x + rect.width <= body.x + body.width + 0.1,
                                "horizontal glyph bounds {text}: {rect:?} outside {body:?}"
                            );
                            reached.insert(text.clone());
                        }
                    }
                    let emitted = deliver(
                        &mut element,
                        &mut tree,
                        &node,
                        &renderer,
                        bounds,
                        body.center(),
                        iced::mouse::Event::WheelScrolled {
                            delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: -100.0 },
                        },
                    );
                    assert!(
                        emitted
                            .into_iter()
                            .map(unbind)
                            .all(|m| matches!(m, Message::SheetBackdrop)),
                        "wheel boundaries cannot close the designer or execute a Theme action"
                    );
                    last = probe(&mut element, &mut tree, &node, &renderer);
                    if case == "library-end"
                        && !proof_captured
                        && fits(body, last.proof_bounds["theme-proof-miniature"])
                    {
                        paths.push(capture(
                            "proof",
                            &mut element,
                            &mut tree,
                            &node,
                            &mut renderer,
                        ));
                        proof_captured = true;
                    }
                    if case == "library-end"
                        && !name_captured
                        && last.inputs.iter().any(|(id, rect)| {
                            id.as_ref() == Some(&iced::widget::Id::new("theme-name"))
                                && fits(body, *rect)
                        })
                    {
                        paths.push(capture(
                            "named-save",
                            &mut element,
                            &mut tree,
                            &node,
                            &mut renderer,
                        ));
                        name_captured = true;
                    }
                    let picker = last.starter_bounds.filter(|rect| fits(body, *rect));
                    if let Some(picker) =
                        picker.filter(|_| case == "library-end" && starter_reached.is_empty())
                    {
                        let options = candidate_state::frontend::starter_palettes();
                        let option_height = 12.0
                            * crate::donor_folio::source_ui::legible_type_scale(scale as f32)
                            * 1.3
                            + 18.0 * scale as f32;
                        for (index, option) in options.iter().enumerate() {
                            let _ = deliver(
                                &mut element,
                                &mut tree,
                                &node,
                                &renderer,
                                bounds,
                                picker.center(),
                                iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),
                            );
                            let mut root = element
                                .as_widget_mut()
                                .overlay(
                                    &mut tree,
                                    Layout::new(&node),
                                    &renderer,
                                    &bounds,
                                    iced::Vector::ZERO,
                                )
                                .expect("production starter picker overlay");
                            let overlay_node = root.as_overlay_mut().layout(&renderer, size);
                            let overlay_layout = Layout::new(&overlay_node);
                            let menu =
                                menu_bounds(overlay_layout, picker).expect("bounded starter menu");
                            assert!(fits(bounds, menu));
                            assert!(menu.height <= 224.0 * scale as f32 + 0.1);
                            let max_scroll = options.len() as f32 * option_height - menu.height;
                            let offset = (index as f32 * option_height - menu.height * 0.5)
                                .clamp(0.0, max_scroll);
                            let mut messages = Vec::new();
                            for y in [10_000.0, -offset] {
                                root.as_overlay_mut().update(
                                    &iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                                        delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y },
                                    }),
                                    overlay_layout,
                                    iced::mouse::Cursor::Available(menu.center()),
                                    &renderer,
                                    &mut clipboard::Null,
                                    &mut Shell::new(&mut messages),
                                );
                            }
                            assert!(
                                messages.is_empty(),
                                "menu scroll must not dispatch a selection"
                            );
                            let point = iced::Point::new(
                                menu.center().x,
                                menu.y + (index as f32 + 0.5) * option_height - offset,
                            );
                            assert!(menu.contains(point), "{} visible", option.name);
                            drop(root);
                            if index == 0 || index + 1 == options.len() {
                                let case = if index == 0 {
                                    "starter-first"
                                } else {
                                    "starter-end"
                                };
                                paths.push(capture(
                                    case,
                                    &mut element,
                                    &mut tree,
                                    &node,
                                    &mut renderer,
                                ));
                            }
                            let mut overlay = element
                                .as_widget_mut()
                                .overlay(
                                    &mut tree,
                                    Layout::new(&node),
                                    &renderer,
                                    &bounds,
                                    iced::Vector::ZERO,
                                )
                                .expect("starter menu remains open");
                            let overlay_node = overlay.as_overlay_mut().layout(&renderer, size);
                            for event in [
                                iced::mouse::Event::CursorMoved { position: point },
                                iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),
                            ] {
                                overlay.as_overlay_mut().update(
                                    &iced::Event::Mouse(event),
                                    Layout::new(&overlay_node),
                                    iced::mouse::Cursor::Available(point),
                                    &renderer,
                                    &mut clipboard::Null,
                                    &mut Shell::new(&mut messages),
                                );
                            }
                            assert!(messages.into_iter().map(unbind).any(|message| matches!(
                                message, Message::ThemeDesigner(Action::Load { target: Load::Starter(id), .. })
                                    if id == option.id
                            )), "actual starter hit for {}", option.name);
                            starter_reached.push(option.name.as_str());
                        }
                    }
                }
                paths.push(capture(case, &mut element, &mut tree, &node, &mut renderer));
                let expected = if case == "index-end" {
                    app.settings_locale.text("theme-designer-role-source-color")
                } else {
                    final_name.into()
                };
                let point = last
                    .texts
                    .iter()
                    .find(|(owner, text, rect)| {
                        owner.as_ref() == Some(&id)
                            && text == &expected
                            && body.contains(rect.center())
                    })
                    .expect("final item visible")
                    .2
                    .center();
                let messages = click(&mut element, &mut tree, &node, &renderer, bounds, point);
                if case == "index-end" {
                    role_messages = messages;
                } else {
                    load_messages = messages;
                }
                assert!(
                    last.scrolls.iter().find(|s| s.0 == id).unwrap().3.y > 0.0,
                    "real scroll translation"
                );
            }
            assert!(reached.contains(final_name));
            assert!(name_captured, "named Save field reachable");
            assert_eq!(starter_reached.len(), 26);
            drop(element);
            assert!(
                role_messages.iter().cloned().map(unbind).any(|m| matches!(
                    m,
                    Message::ThemeRoleSelected {
                        role: theme_roles::ThemeRole::SourceColor,
                        ..
                    }
                )),
                "final role actual hit"
            );
            for m in role_messages {
                let _ = update(&mut app, m);
            }
            assert_eq!(
                app.theme_session.editor_selected_role(),
                Some(theme_roles::ThemeRole::SourceColor)
            );
            assert!(
                load_messages
                    .iter()
                    .cloned()
                    .map(unbind)
                    .any(|m| matches!(m, Message::ThemeDesigner(Action::Load { .. }))),
                "final library actual Load hit"
            );
            // Return to the top and select the actual Light tab through its widget.
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            let p = probe(&mut element, &mut tree, &node, &renderer);
            let light = app.settings_locale.text("theme-profile-light");
            let point = p
                .texts
                .iter()
                .find(|(_, s, _)| s == &light)
                .unwrap()
                .2
                .center();
            let messages = click(&mut element, &mut tree, &node, &renderer, bounds, point);
            drop(element);
            assert!(
                messages.iter().cloned().map(unbind).any(|m| matches!(
                    m,
                    Message::ThemeVariantSelected {
                        variant: theme_roles::ColorVariant::Light,
                        ..
                    }
                )),
                "actual Light hit"
            );
            for m in messages {
                let _ = update(&mut app, m);
            }
            assert_eq!(
                app.theme_session.editor_selected_variant(),
                Some(theme_roles::ColorVariant::Light)
            );
            // Capture a root-owned confirmation with the same long name.
            let binding = Binding::current(&app).unwrap();
            let id = app.state.theme_library.records.last().unwrap().id;
            let _ = update(
                &mut app,
                Message::ThemeDesigner(Action::Delete { binding, id }),
            );
            assert!(app.theme_workflow.prompt.is_some());
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            paths.push(capture(
                "confirmation",
                &mut element,
                &mut tree,
                &node,
                &mut renderer,
            ));
            drop(element);
            let mut settings = p3_composed_editor_app(SettingsCategory::Library);
            settings.viewport = [width as f32, height as f32];
            settings.launch_policy = p1_preview_policy();
            settings.state.frontend_preferences.interface.ui_scale = Some(scale);
            settings.settings_locale = localization::Catalog::new(locale, locale).unwrap();
            view::settings::controller::reconcile(&mut settings);
            settings.settings_workbench.folio_entrance = None;
            settings.settings_workbench.tab_reveal = None;
            let mut settings_element = view(&settings, settings.window.unwrap());
            let mut settings_tree = Tree::new(&settings_element);
            let settings_node = settings_element.as_widget_mut().layout(
                &mut settings_tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            let current = probe(
                &mut settings_element,
                &mut settings_tree,
                &settings_node,
                &renderer,
            );
            let settings_close = current.close_bounds.expect("Settings reading Close");
            let settings_glyph = current.texts.iter().find(|(_, s, _)| s == "×").unwrap().2;
            assert_eq!(close.size(), settings_close.size());
            assert_eq!(glyph.size(), settings_glyph.size());
            let click_settings = click(
                &mut settings_element,
                &mut settings_tree,
                &settings_node,
                &renderer,
                bounds,
                iced::Point::new(settings_close.x + 2.0, settings_close.y + 2.0),
            );
            assert!(
                click_settings.into_iter().any(|message| matches!(
                    message,
                    Message::SettingsIntent {
                        intent: view::settings::folio::intent::Message::ToggleSettings,
                        ..
                    }
                )),
                "Settings Close shares the visible corner hit"
            );
            paths.push(capture(
                "settings-close",
                &mut settings_element,
                &mut settings_tree,
                &settings_node,
                &mut renderer,
            ));
            receipts.push(serde_json::json!({"locale":locale,"width":width,"height":height,"scale":scale,"paths":paths,"reached":reached,"starterHits":starter_reached,"closeSize":[close.width,close.height],"closeGlyphSize":[glyph.width,glyph.height],"indexFinalHit":true,"libraryFinalLoadHit":true,"lightActualHit":true,"profile":app.state_path,"providerSubmission":false}));
        }
    }
    std::fs::write(
        output.join("PK10-LAYOUTS.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
}
#[test]
fn r5_library_reading_action_uses_existing_generation_gateway_and_back() {
    use view::settings::{
        composed::ActionId,
        folio::intent::{Message as Intent, SettingsMsg},
    };
    let mut app = p3_composed_editor_app(SettingsCategory::Library);
    let cards = view::settings::bridge::build(&app, "library");
    assert!(cards.iter().flat_map(|(_, rows)| rows).any(|row| matches!(
        row.control,
        Control::ActionBtn {
            id: ActionId::OpenLibraryTools,
            ..
        }
    )));
    let saved = app.state.clone();
    let config = toml::to_string(&app.config).unwrap();
    let window = app.window;
    let generation = app.settings_workbench.generation;
    let callback = Message::SettingsIntent {
        window,
        generation,
        intent: Intent::Settings(SettingsMsg::Run(ActionId::OpenLibraryTools)),
    };
    let _ = update(&mut app, callback.clone());
    assert_eq!(
        app.settings_workbench.windows_panel,
        Some(SettingsCategory::Library)
    );
    let child_generation = app.settings_workbench.generation;
    assert_ne!(generation, child_generation);
    let _ = update(&mut app, callback);
    assert_eq!(
        app.settings_workbench.generation, child_generation,
        "stale callback is rejected"
    );
    let _ = update(
        &mut app,
        Message::SettingsWindowsClose {
            window,
            generation: child_generation,
        },
    );
    assert_eq!(app.settings_workbench.windows_panel, None);
    assert_eq!(app.settings_workbench.tab, "library");
    assert_eq!(app.state, saved);
    assert_eq!(toml::to_string(&app.config).unwrap(), config);
}

#[test]
fn r5_playlist_activation_membership_order_and_reload_keep_real_intent_without_apply() {
    let (mut app, submissions) = d4a_fixture(3);
    app.launch_policy = p1_preview_policy();
    let _ = update(
        &mut app,
        Message::PlaylistNameChanged("R5 owned explicit".into()),
    );
    let _ = update(&mut app, Message::CreatePlaylist { smart: false });
    let id = app.state.playlists.last().unwrap().id.clone();
    let generation = app.automation_generation.load(Ordering::Acquire);
    let _ = update(&mut app, Message::SetActivePlaylist(id.clone(), generation));
    assert_eq!(app.state.active_playlist_id.as_deref(), Some(id.as_str()));
    assert!(
        app.playlist_playing(),
        "explicit Start begins the timed owner without Apply"
    );
    app.selected = Some(1);
    let _ = update(&mut app, Message::TogglePlaylistMembership(id.clone()));
    let _ = update(&mut app, Message::TogglePlaylistOrder(id.clone()));
    let saved = CandidateState::load(&app.state_path);
    assert_eq!(saved.active_playlist_id, app.state.active_playlist_id);
    assert_eq!(saved.playlists, app.state.playlists);
    assert!(matches!(
        saved.playlists.last().unwrap().order,
        PlaybackOrder::Shuffle
    ));
    assert_eq!(
        library::evaluate_playlist(
            saved.playlists.last().unwrap(),
            &app.catalog,
            &saved.favorites
        )
        .iter()
        .map(|e| e.id.as_str())
        .collect::<Vec<_>>(),
        vec![app.wallpapers[0].id.as_str(), app.wallpapers[1].id.as_str()]
    );
    assert_eq!(submissions.load(Ordering::Acquire), 0);
    assert!(!app.state.slideshow_enabled);
}

fn r5_schema_fixture(app: &mut GpuCandidate) {
    let path = app.state_path.with_file_name("r5-read-only-project.json");
    std::fs::write(&path, r##"{"general":{"properties":{
        "enabled":{"type":"bool","text":"Enabled / 启用","value":true,"order":1},
        "speed":{"type":"slider","text":"Speed with a deliberately long descriptive label / 播放速度长标签","value":0.6,"min":0,"max":1,"step":0.1,"order":2},
        "mode":{"type":"combo","text":"Mode / 模式","value":1,"options":[{"label":"First / 第一项","value":1},{"label":"Second / 第二项","value":2}],"order":3},
        "tint":{"type":"color","text":"Tint / 色调","value":"0.20 0.40 0.80","order":4},
        "invalid":{"type":"color","text":"Unavailable colour preview / 颜色预览不可用","value":"#invalid","order":5}
    }}}"##).unwrap();
    app.engine_properties = engine_properties::read_supported(&path).properties;
    assert_eq!(app.engine_properties.len(), 5);
    app.engine_property_values = app
        .engine_properties
        .iter()
        .map(|p| (p.key.clone(), p.default_value.clone()))
        .collect();
    app.engine_properties_project = Some(("r5-owned".into(), path));
    app.engine_properties_diagnostic = Some(
        "Read-only owned schema fixture · provider confirmation unavailable / 提供程序确认不可用"
            .into(),
    );
}

#[test]
#[ignore = "R5 owned production panel layouts/hits; explicit invocation only"]
fn r5_owned_panel_layouts() {
    use iced::advanced::{
        clipboard, layout,
        renderer::{Headless, Renderer as _},
        widget::{operation, Operation, Tree},
        Layout, Shell,
    };
    #[derive(Default)]
    struct Probe {
        offset: iced::Vector,
        pending: Option<iced::Vector>,
        texts: Vec<(String, iced::Rectangle)>,
        scrolls: Vec<(iced::Rectangle, iced::Rectangle, iced::Vector)>,
    }
    impl Operation for Probe {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
            let old = self.offset;
            if let Some(v) = self.pending.take() {
                self.offset += v;
            }
            visit(self);
            self.offset = old;
        }
        fn text(&mut self, _: Option<&iced::widget::Id>, b: iced::Rectangle, value: &str) {
            self.texts.push((value.into(), b - self.offset));
        }
        fn scrollable(
            &mut self,
            _: Option<&iced::widget::Id>,
            b: iced::Rectangle,
            c: iced::Rectangle,
            v: iced::Vector,
            _: &mut dyn operation::Scrollable,
        ) {
            self.scrolls.push((b - self.offset, c, v));
            self.pending = Some(v);
        }
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
    let mut receipts = Vec::new();
    for locale in ["en-US", "zh-CN"] {
        for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
            for case in [
                "search",
                "playlist-empty",
                "playlist-populated",
                "library",
                "automation",
                "playback",
                "settings-search",
                "engine",
                "folder",
            ] {
                if locale == "zh-CN"
                    && matches!(
                        case,
                        "playlist-empty" | "playback" | "settings-search" | "folder"
                    )
                {
                    continue;
                }
                let (mut app, submissions) = d4a_fixture(3);
                app.launch_policy = p1_preview_policy();
                app.viewport = [width as f32, height as f32];
                app.selected = Some(0);
                let next = localization::Catalog::new(locale, locale).unwrap();
                localization::reload_ui_font(&app.settings_locale, &next).unwrap();
                app.settings_locale = next;
                {
                    let mut fonts = iced::advanced::graphics::text::font_system()
                        .write()
                        .unwrap();
                    fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                    fonts.load_font(std::borrow::Cow::Borrowed(
                        include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf")
                            .as_slice(),
                    ));
                }
                if case == "search" {
                    app.panel_state.search_open = true;
                    app.search_tags_open = true;
                    app.state.frontend_preferences.tag_panel.width = Some(760.0);
                    app.catalog[0].source_tags = (0..36)
                        .map(|n| format!("R5 标签 landscape {n:02}"))
                        .collect();
                    app.project_local_tags();
                    app.reconcile_tag_editor();
                    app.colours.query.bucket =
                        Some(crate::library::color_metadata::ColorBucket::Red);
                    app.colours.status="Owned deterministic status: analysis unavailable / 分析不可用; no provider".into();
                } else if case.starts_with("playlist") {
                    app.playlist_panel_open = true;
                    if case.ends_with("populated") {
                        app.state.playlists.push(Playlist {
                            id: "r5-owned".into(),
                            name: "R5 owned sequence / 本地长名称播放列表".into(),
                            order: PlaybackOrder::Sequential,
                            membership: PlaylistMembership::Explicit {
                                wallpaper_ids: app
                                    .wallpapers
                                    .iter()
                                    .rev()
                                    .map(|w| w.id.clone())
                                    .collect(),
                            },
                        });
                        app.state.active_playlist_id = Some("r5-owned".into());
                    } else {
                        app.state.playlists.clear();
                        app.state.active_playlist_id = None;
                    }
                } else if case == "engine" {
                    app.engine_properties_open = true;
                    r5_schema_fixture(&mut app);
                } else if case == "folder" {
                    app.folder_menu_open = true;
                } else {
                    let _ = update(&mut app, Message::ToggleSettingsPanel);
                    p3_settings_intent(
                        &mut app,
                        SettingsMsg::SelectTab(
                            if case == "settings-search" {
                                "library"
                            } else {
                                case
                            }
                            .into(),
                        ),
                    );
                    if case == "settings-search" {
                        p3_settings_intent(&mut app, SettingsMsg::SearchOpen);
                        p3_settings_intent(&mut app, SettingsMsg::SearchInput("library".into()));
                    } else {
                        app.settings_workbench.windows_panel = Some(match case {
                            "library" => SettingsCategory::Library,
                            "automation" => SettingsCategory::Automation,
                            _ => SettingsCategory::Playback,
                        });
                        app.cache_status="Owned fixture: cache operation refused; ownership unavailable / 缓存所有权不可用".into();
                        app.monitor_ids =
                            vec!["R5 owned display with a long physical identifier".into()];
                    }
                    app.settings_workbench.folio_entrance = None;
                    app.settings_workbench.tab_reveal = None;
                }
                let window = app.window.unwrap();
                let theme = theme(&app, window);
                let style = application_style(&app, &theme);
                let size = iced::Size::new(width as f32, height as f32);
                let viewport = iced::Rectangle::with_size(size);
                let mut element = view(&app, window);
                let mut tree = Tree::new(&element);
                let node = element.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(size, size),
                );
                let mut top = Probe::default();
                element
                    .as_widget_mut()
                    .operate(&mut tree, Layout::new(&node), &renderer, &mut top);
                let overflow = top
                    .scrolls
                    .iter()
                    .filter(|(b, c, _)| c.height > b.height + 1.0)
                    .max_by(|(a, _, _), (b, _, _)| a.width.total_cmp(&b.width))
                    .map(|(b, _, _)| *b);
                let mut captured = Vec::new();
                for end in [false, true] {
                    if end {
                        let Some(body) = overflow else { continue };
                        for _ in 0..30 {
                            let mut messages = Vec::new();
                            element.as_widget_mut().update(
                                &mut tree,
                                &iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                                    delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: -150.0 },
                                }),
                                Layout::new(&node),
                                iced::mouse::Cursor::Available(body.center()),
                                &renderer,
                                &mut clipboard::Null,
                                &mut Shell::new(&mut messages),
                                &viewport,
                            );
                        }
                    }
                    let mut probe = Probe::default();
                    element.as_widget_mut().operate(
                        &mut tree,
                        Layout::new(&node),
                        &renderer,
                        &mut probe,
                    );
                    assert!(probe
                        .texts
                        .iter()
                        .all(|(_, r)| r.width.is_finite() && r.height.is_finite()));
                    if end {
                        assert!(
                            probe.scrolls.iter().any(|(_, _, v)| v.y > 0.0),
                            "wheel must actually scroll {case}"
                        );
                    }
                    renderer.reset(viewport);
                    element.as_widget().draw(
                        &tree,
                        &mut renderer,
                        &theme,
                        &iced::advanced::renderer::Style {
                            text_color: style.text_color,
                        },
                        Layout::new(&node),
                        iced::mouse::Cursor::Unavailable,
                        &viewport,
                    );
                    let bytes = Headless::screenshot(
                        &mut renderer,
                        iced::Size::new(width, height),
                        1.0,
                        style.background_color,
                    );
                    let path = output.join(format!(
                        "r5-{locale}-{width}x{height}-{case}-{}.png",
                        if end { "end" } else { "top" }
                    ));
                    ::image::save_buffer_with_format(
                        &path,
                        &bytes,
                        width,
                        height,
                        ::image::ColorType::Rgba8,
                        ::image::ImageFormat::Png,
                    )
                    .unwrap();
                    captured.push(serde_json::json!({"path":path,"end":end,"scrolls":format!("{:?}",probe.scrolls),"texts":probe.texts.iter().filter(|(_,r)|viewport.intersects(r)).map(|(s,r)|serde_json::json!({"text":s,"bounds":[r.x,r.y,r.width,r.height]})).collect::<Vec<_>>()}));
                }
                drop(element);
                assert_eq!(submissions.load(Ordering::Acquire), 0);
                assert!(!app.state.slideshow_enabled);
                receipts.push(serde_json::json!({"locale":locale,"width":width,"height":height,"case":case,"captures":captured,"provider_submissions":0,"boundary":"production headless widgets; not native or human acceptance"}));
            }
        }
    }
    std::fs::write(
        output.join("R5-LAYOUTS.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
}

#[test]
fn feedback_playlist_browsing_never_changes_rotation_until_explicit_action() {
    let (mut app, submissions) = d4a_fixture(3);
    app.launch_policy = p1_preview_policy();
    for name in ["First rotation", "Second browsing"] {
        let _ = update(&mut app, Message::PlaylistNameChanged(name.into()));
        let _ = update(&mut app, Message::CreatePlaylist { smart: false });
    }
    let first = app.state.playlists[0].id.clone();
    let second = app.state.playlists[1].id.clone();
    let generation = app.automation_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::SetActivePlaylist(first.clone(), generation),
    );
    let before = app.state.clone();
    let disk = std::fs::read(&app.state_path).unwrap();
    let _ = update(&mut app, Message::BrowsePlaylist(second.clone()));
    assert_eq!(app.playlist_browsing_id.as_deref(), Some(second.as_str()));
    assert_eq!(app.state, before);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), disk);
    let _ = update(&mut app, Message::BrowsePlaylist("missing".into()));
    assert_eq!(app.playlist_browsing_id.as_deref(), Some(second.as_str()));
    let generation = app.automation_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::SetActivePlaylist(second.clone(), generation),
    );
    assert_eq!(
        CandidateState::load(&app.state_path).active_playlist_id,
        Some(second)
    );
    assert_eq!(submissions.load(Ordering::Acquire), 0);
    assert!(!app.state.slideshow_enabled);
    assert!(app.playlist_playing());
}

#[test]
fn feedback_rail_visibility_roundtrips_to_actual_layout_and_no_badge_authoring() {
    use view::settings::{composed::draft::Value, preference_projection};
    let (mut app, _) = d4a_fixture(3);
    let settings = [
        "colors", "theme", "tagcloud", "sort", "mute", "search", "settings", "display", "close",
    ];
    for name in settings {
        let path = format!("filterBar.show.{name}");
        assert!(preference_projection::write(
            &mut app.state.frontend_preferences,
            &path,
            &Value::Boolean(false)
        )
        .unwrap());
    }
    let encoded = toml::to_string(&app.state).unwrap();
    app.state = toml::from_str(&encoded).unwrap();
    for name in settings {
        assert_eq!(
            preference_projection::read(
                &app.state.frontend_preferences,
                &format!("filterBar.show.{name}")
            ),
            Some(Value::Boolean(false))
        );
    }
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
        GalleryMode::Depth,
    ] {
        app.gallery_mode = mode;
        let model = view::gallery_rail_model(&app);
        assert!(model.title.is_none());
        assert!(!model.show_close);
        assert!(!model.visibility.colors && !model.visibility.tagcloud);
        assert_eq!(app.state.frontend_preferences.visibility.theme, Some(false));
        assert!(
            !model.visibility.sort
                && !model.visibility.mute
                && !model.visibility.search
                && !model.visibility.settings
        );
    }
    // Old saved badge intent remains readable but cannot re-enable execution or authoring.
    app.state.frontend_preferences.picker.show_type_badges = Some(true);
    assert_eq!(
        preference_projection::read(
            &app.state.frontend_preferences,
            "components.wallpaperSelector.showTypeBadges"
        ),
        None
    );
    assert!(!preference_projection::write(
        &mut app.state.frontend_preferences,
        "components.wallpaperSelector.showTypeBadges",
        &Value::Boolean(false)
    )
    .unwrap());
    assert_eq!(
        app.state.frontend_preferences.picker.show_type_badges,
        Some(true)
    );
}

#[test]
#[ignore = "E2 affected production details, recovery absence and current fonts; explicit owned capture"]
fn e2_owned_detail_titles_and_recovery_absence_1080p_1440p() {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::{Operation, Tree},
        Layout,
    };
    #[derive(Default)]
    struct Texts(Vec<String>);
    impl Operation for Texts {
        fn traverse(&mut self, f: &mut dyn FnMut(&mut dyn Operation)) {
            f(self);
        }
        fn text(&mut self, _: Option<&iced::widget::Id>, _: iced::Rectangle, value: &str) {
            self.0.push(value.into());
        }
    }
    let output = std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").unwrap());
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .unwrap();
    let mut captures = Vec::new();
    for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
        for (mode, name) in [
            (GalleryMode::Slices, "slices"),
            (GalleryMode::Geometric, "hex"),
            (GalleryMode::Wall, "wall"),
            (GalleryMode::Depth, "depth"),
        ] {
            for scale in if width == 1920 && mode == GalleryMode::Geometric {
                vec![1.0, 1.25]
            } else {
                vec![1.0]
            } {
                let (mut app, submissions) = d4a_fixture(3);
                app.viewport = [width as f32, height as f32];
                app.launch_policy = p1_preview_policy();
                app.state.frontend_preferences.interface.ui_scale = Some(scale);
                app.set_gallery_mode(mode);
                app.selected = Some(0);
                app.rail_reveal = 1.0;
                app.wallpapers[0].title =
                    include_str!("../../fixtures/e2-workshop-title.txt").into();
                app.wallpapers[0].source = WallpaperSource::WallpaperEngine;
                app.wallpapers[0].wallpaper_type = "scene".into();
                app.sync_gallery_presentation();
                if let Some(p) = &mut app.gallery_presentation {
                    p.settle();
                }
                if let Some(p) = &mut app.hex_presentation {
                    p.settle();
                }
                if let Some(p) = &mut app.wall_presentation {
                    p.settle();
                    p.entrance = GalleryMotionState::source(1.0, 1.0, 250.0, 1.0);
                }
                {
                    let mut fonts = iced::advanced::graphics::text::font_system()
                        .write()
                        .unwrap();
                    fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                    fonts.load_font(std::borrow::Cow::Borrowed(include_bytes!(
                        "../../../assets/fonts/SymbolsNerdFont-Regular.ttf"
                    )));
                }
                app.rebuild_depth_frame();
                app.start_detail(true, "e2-layout");
                app.detail_progress = 1.0;
                app.depth_detail = Some(GalleryMotionState::source(1.0, 1.0, 250.0, 1.0));
                app.resolve_depth_detail();
                app.rebuild_depth_frame();
                let window = app.window.unwrap();
                let theme = theme(&app, window);
                let style = application_style(&app, &theme);
                let size = iced::Size::new(width as f32, height as f32);
                let bounds = iced::Rectangle::with_size(size);
                {
                    let mut element = view(&app, window);
                    let mut tree = Tree::new(&element);
                    let node = element.as_widget_mut().layout(
                        &mut tree,
                        &renderer,
                        &layout::Limits::new(size, size),
                    );
                    renderer.reset(bounds);
                    element.as_widget().draw(
                        &tree,
                        &mut renderer,
                        &theme,
                        &iced::advanced::renderer::Style {
                            text_color: style.text_color,
                        },
                        Layout::new(&node),
                        iced::mouse::Cursor::Unavailable,
                        &bounds,
                    );
                    let bytes = Headless::screenshot(
                        &mut renderer,
                        iced::Size::new(width, height),
                        1.0,
                        style.background_color,
                    );
                    let path = output.join(format!("{width}x{height}-{name}-scale{scale}.png"));
                    ::image::save_buffer_with_format(
                        &path,
                        &bytes,
                        width,
                        height,
                        ::image::ColorType::Rgba8,
                        ::image::ImageFormat::Png,
                    )
                    .unwrap();
                    captures.push(serde_json::json!({"path":path,"mode":name,"scale":scale,"title":app.wallpapers[0].title,"depth_panel":app.depth_panel_open()}));
                }
                let mut recovery = test_provider_recovery(20261027, "display-e2", "e2-unknown");
                recovery.phase = coordinator::RecoveryPhase::DispatchedOrUnknown;
                app.provider_recoveries = vec![recovery.clone()];
                app.provider_state_unknown = true;
                app.status =
                    Some("Provider state Unknown; further provider changes remain blocked.".into());
                let records = app.provider_recoveries.clone();
                for policy in [runtime::LaunchPolicy::default(), p1_preview_policy()] {
                    app.launch_policy = policy;
                    for detail in [false, true] {
                        app.detail_open = detail;
                        let mut element = view(&app, window);
                        let mut tree = Tree::new(&element);
                        let node = element.as_widget_mut().layout(
                            &mut tree,
                            &renderer,
                            &layout::Limits::new(size, size),
                        );
                        let mut texts = Texts::default();
                        element.as_widget_mut().operate(
                            &mut tree,
                            Layout::new(&node),
                            &renderer,
                            &mut texts,
                        );
                        assert!(texts
                            .0
                            .iter()
                            .any(|s| s.contains("Unknown") && s.contains("blocked")));
                        for retired in [
                            "Retry observation",
                            "Cancel failed request",
                            "Reconcile provider",
                            "click selects",
                            "click again",
                            "Apply stays explicit",
                        ] {
                            assert!(
                                texts.0.iter().all(|s| !s.contains(retired)),
                                "retired {retired}"
                            );
                        }
                        assert!(detail_actions(&app, &app.wallpapers[0])
                            .iter()
                            .all(|a| a.label != "Reconcile provider"));
                        let apply = detail_actions(&app, &app.wallpapers[0])
                            .into_iter()
                            .find(|a| a.intent == donor_back_layout::DetailIntent::Apply)
                            .unwrap();
                        assert!(!apply.enabled);
                    }
                }
                assert_eq!(app.provider_recoveries, records);
                assert!(app.provider_state_unknown);
                assert_eq!(submissions.load(Ordering::Acquire), 0);
            }
        }
    }
    std::fs::write(
        output.join("captures.json"),
        serde_json::to_vec_pretty(&captures).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "E3 owned affected Settings production layouts; explicit invocation only"]
fn e3_owned_motion_settings_1080p_1440p_and_scale() {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::{Operation, Tree},
        Layout,
    };
    #[derive(Default)]
    struct Texts(Vec<String>);
    impl Operation for Texts {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
            visit(self);
        }
        fn text(&mut self, _: Option<&iced::widget::Id>, _: iced::Rectangle, value: &str) {
            self.0.push(value.into());
        }
    }
    let output =
        std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned output"));
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .unwrap();
    let mut captures = Vec::new();
    for (width, height, scale) in [
        (1920u32, 1080u32, 1.0),
        (2560, 1440, 1.0),
        (1920, 1080, 1.25),
    ] {
        for section in 0..3 {
            let (mut app, submissions) = d4a_fixture(3);
            app.launch_policy = p1_preview_policy();
            app.viewport = [width as f32, height as f32];
            app.state.frontend_preferences.interface.ui_scale = Some(scale);
            e3_seed_inert_source_motion(&mut app);
            let _ = update(&mut app, Message::ToggleSettingsPanel);
            p3_settings_intent(&mut app, SettingsMsg::SelectTab("motion".into()));
            p3_settings_intent(&mut app, SettingsMsg::SelectSection(section));
            app.settings_workbench.folio_entrance = None;
            app.settings_workbench.tab_reveal = None;
            {
                let mut fonts = iced::advanced::graphics::text::font_system()
                    .write()
                    .unwrap();
                fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
                fonts.load_font(std::borrow::Cow::Borrowed(
                    include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
                ));
            }
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let style = application_style(&app, &theme);
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            let mut texts = Texts::default();
            element
                .as_widget_mut()
                .operate(&mut tree, Layout::new(&node), &renderer, &mut texts);
            assert!(texts.0.iter().any(|s| s == "Motion"));
            for retired in [
                "Wallpaper transitions",
                "Transitions",
                "Preview",
                "Sand",
                "Transition preview unavailable in this build.",
            ] {
                assert!(
                    texts.0.iter().all(|s| s != retired),
                    "retired public label {retired}"
                );
            }
            renderer.reset(bounds);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: style.text_color,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let bytes = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                style.background_color,
            );
            let path = output.join(format!(
                "{width}x{height}-motion-{section}-scale{scale}.png"
            ));
            ::image::save_buffer_with_format(
                &path,
                &bytes,
                width,
                height,
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
            captures.push(serde_json::json!({"path":path,"width":width,"height":height,"scale":scale,"section":section,"texts":texts.0}));
            assert_eq!(submissions.load(Ordering::Acquire), 0);
        }
    }
    std::fs::write(
        output.join("captures.json"),
        serde_json::to_vec_pretty(&captures).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "Owned actual public preset studio layouts; explicit invocation"]
fn preset_name_public_studio_bounds_and_captured_identity() {
    use iced::advanced::{
        layout,
        renderer::{Headless, Renderer as _},
        widget::{Operation, Tree},
        Layout,
    };
    #[derive(Default)]
    struct Bounds {
        text: Vec<(String, iced::Rectangle)>,
        inputs: Vec<iced::Rectangle>,
    }
    impl Operation for Bounds {
        fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
            visit(self);
        }
        fn text(&mut self, _: Option<&iced::widget::Id>, bounds: iced::Rectangle, value: &str) {
            self.text.push((value.into(), bounds));
        }
        fn text_input(
            &mut self,
            _: Option<&iced::widget::Id>,
            bounds: iced::Rectangle,
            _: &mut dyn iced::advanced::widget::operation::TextInput,
        ) {
            self.inputs.push(bounds);
        }
    }
    let output =
        std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned output"));
    let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
        UI_FONT,
        iced::Pixels(16.0),
        Some("wgpu"),
    ))
    .unwrap();
    let mut receipts = Vec::new();
    for (width, height, scale, locale, name) in [
        (1920u32, 1080u32, 1.0, "en-US", "Evening"),
        (1920, 1080, 0.5, "en-US", "夜晚海風森林城市晨光花園星空山水"),
        (
            640,
            720,
            1.0,
            "en-US",
            "Evening 海風 café — a very long preset name for the narrow layout studio saved button",
        ),
        (
            960,
            720,
            0.5,
            "zh-CN",
            "Evening 海風 café — a very long preset name for the narrow layout studio saved button",
        ),
    ] {
        let mut app = p3_composed_editor_app(SettingsCategory::Picker);
        app.viewport = [width as f32, height as f32];
        app.launch_policy = p1_preview_policy();
        app.state.gallery.mode = GalleryKind::Slices;
        app.gallery_mode = GalleryMode::Slices;
        app.state.frontend_preferences.interface.ui_scale = Some(scale as f64);
        app.settings_locale = localization::Catalog::new(locale, locale).unwrap();
        app.save_gallery_preset().unwrap();
        let id = app
            .state
            .mode_profiles
            .selected_id(GalleryKind::Slices)
            .unwrap();
        let snapshot = app
            .state
            .mode_profiles
            .snapshot(GalleryKind::Slices, id)
            .unwrap();
        assert!(app
            .state
            .mode_profiles
            .rename_selected(GalleryKind::Slices, id, name));
        view::settings::controller::reconcile(&mut app);
        p3_settings_intent(&mut app, SettingsMsg::SelectSection(1));
        app.settings_workbench.folio_entrance = None;
        app.settings_workbench.tab_reveal = None;
        {
            let mut fonts = iced::advanced::graphics::text::font_system()
                .write()
                .unwrap();
            fonts.load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
        }
        let size = iced::Size::new(width as f32, height as f32);
        let viewport = iced::Rectangle::with_size(size);
        let mut element = view::settings::gateway::view(&app);
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let mut bounds = Bounds::default();
        element
            .as_widget_mut()
            .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
        let panel_width =
            crate::donor_folio::source_ui::picker_layout_studio_width(width as f32, scale);
        let right = 18.0 * scale + panel_width;
        let shown = bounds
            .text
            .iter()
            .find(|(text, _)| text == name || (text.starts_with("Evening") && text.ends_with('…')))
            .expect("actual saved entry label");
        assert!(shown.1.x >= 0.0 && shown.1.x + shown.1.width <= right + 0.1);
        assert!(shown.1.height < 20.0, "saved label is one line");
        if name != "Evening" && name.starts_with("Evening") {
            assert!(shown.0.ends_with('…'));
        } else {
            assert_eq!(shown.0, name);
        }
        let title = app
            .settings_locale
            .text_static("settings-selector-preset-name-label");
        assert!(bounds
            .text
            .iter()
            .any(|(text, b)| text == title && b.x >= 0.0 && b.x + b.width <= right + 0.1));
        assert!(bounds
            .inputs
            .iter()
            .any(|b| b.x >= 0.0 && b.x + b.width <= right + 0.1 && b.y < shown.1.y));
        let mut messages = Vec::new();
        for event in [
            iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left),
            iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left),
        ] {
            element.as_widget_mut().update(
                &mut tree,
                &iced::Event::Mouse(event),
                Layout::new(&node),
                iced::mouse::Cursor::Available(shown.1.center()),
                &mut renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &viewport,
            );
        }
        assert!(messages.iter().any(|m|matches!(m,Message::SettingsIntent { intent:view::settings::folio::intent::Message::ApplyPreset(GalleryKind::Slices,target),..} if *target==id)),"public label hit keeps captured ID");
        renderer.reset(viewport);
        element.as_widget().draw(
            &tree,
            &mut renderer,
            &iced::Theme::Dark,
            &iced::advanced::renderer::Style {
                text_color: iced::Color::WHITE,
            },
            Layout::new(&node),
            iced::mouse::Cursor::Unavailable,
            &viewport,
        );
        let pixels = iced::Size::new(width, height);
        let bytes = Headless::screenshot(&mut renderer, pixels, 1.0, iced::Color::BLACK);
        let path = output.join(format!("public-preset-{width}-scale{scale}-{locale}.png"));
        ::image::save_buffer_with_format(
            &path,
            &bytes,
            width,
            height,
            ::image::ColorType::Rgba8,
            ::image::ImageFormat::Png,
        )
        .unwrap();
        drop(element);
        assert_eq!(
            app.state.mode_profiles.selected_id(GalleryKind::Slices),
            Some(id)
        );
        assert_eq!(
            app.state
                .mode_profiles
                .snapshot(GalleryKind::Slices, id)
                .unwrap(),
            snapshot
        );
        receipts.push(serde_json::json!({"width":width,"height":height,"scale":scale,"locale":locale,"name":name,"shown":shown.0,"preset_id":id,"studio_width":panel_width,"path":path}));
    }
    std::fs::write(
        output.join("public-preset-layouts.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
}

#[test]
fn ret03_direct_sort_intents_preserve_selection_and_keyboard_option_flags() {
    let (mut app, _) = d4a_fixture(3);
    app.navigate_selection(1);
    assert_eq!(app.selected, Some(1));
    assert!(app.gallery_engaged);
    let selected = app.selected.map(|i| app.wallpapers[i].id.clone());
    for (order, _, _, _) in donor_rail::SORT_SHORTCUTS {
        let _ = update(
            &mut app,
            Message::DonorRail(donor_rail::RailIntent::SetSort(order)),
        );
        assert_eq!(app.sort_order, order);
        assert_eq!(app.selected.map(|i| app.wallpapers[i].id.clone()), selected);
        let filtered = app.filtered_indices.clone();
        let generation = app.poster_generation.load(Ordering::Acquire);
        let _ = update(
            &mut app,
            Message::DonorRail(donor_rail::RailIntent::SetSort(order)),
        );
        assert_eq!(app.filtered_indices, filtered);
        assert_eq!(
            app.poster_generation.load(Ordering::Acquire),
            generation,
            "reclick must not schedule posters"
        );
    }
    app.state.frontend_preferences.visibility.sort_recent = Some(false);
    app.state.frontend_preferences.visibility.sort_pop = Some(false);
    app.sort_order = SortOrder::Minimalist;
    let _ = update(&mut app, Message::CycleSort);
    assert_eq!(app.sort_order, SortOrder::ResolutionDescending);
    let _ = update(&mut app, Message::CycleSortBack);
    assert_eq!(app.sort_order, SortOrder::Minimalist);
    let model = gallery_rail_model(&app);
    assert!(!model.sort_visibility[1] && !model.sort_visibility[3]);
    for order in [SortOrder::Recent, SortOrder::ColourPop] {
        assert!(
            donor_rail::intent_center(&model, donor_rail::RailIntent::SetSort(order)).is_none()
        );
    }
    app.state.frontend_preferences.visibility.sort = Some(false);
    let model = gallery_rail_model(&app);
    for (order, _, _, _) in donor_rail::SORT_SHORTCUTS {
        assert!(
            donor_rail::intent_center(&model, donor_rail::RailIntent::SetSort(order)).is_none()
        );
    }
}
#[test]
fn pk01_all_seven_legacy_visibility_flags_drive_live_controls_and_safe_cycle() {
    let (mut app, submissions) = d4a_fixture(3);
    assert_eq!(app.sort_order, SortOrder::Colour);
    assert_eq!(
        donor_rail::SORT_SHORTCUTS.map(|(order, _, _, _)| order),
        SortOrder::ALL
    );
    for (index, key) in [
        "date",
        "recent",
        "color",
        "pop",
        "richness",
        "minimalist",
        "res",
    ]
    .into_iter()
    .enumerate()
    {
        let path = format!("filterBar.show.sort.{key}");
        view::settings::preference_projection::write(
            &mut app.state.frontend_preferences,
            &path,
            &view::settings::composed::draft::Value::Boolean(false),
        )
        .unwrap();
        let model = gallery_rail_model(&app);
        assert!(!model.sort_visibility[index]);
        assert!(donor_rail::intent_center(
            &model,
            donor_rail::RailIntent::SetSort(SortOrder::ALL[index])
        )
        .is_none());
    }
    let before = app.sort_order;
    let _ = update(&mut app, Message::CycleSort);
    let _ = update(&mut app, Message::CycleSortBack);
    assert_eq!(app.sort_order, before);
    assert_eq!(submissions.load(Ordering::Acquire), 0);
}

#[test]
fn ret03_folder_and_size_projection_keeps_full_active_scope() {
    let (mut app, _) = d3_application_fixture(0);
    app.filter_rule.folders.clear();
    assert_eq!(gallery_rail_model(&app).folder_label, "ALL WALLPAPERS");
    let root = std::path::PathBuf::from("J:/");
    app.filter_rule.folders.insert(root.clone());
    let model = gallery_rail_model(&app);
    assert_eq!(model.folder_label, root.display().to_string());
    assert!(model.folder_active && model.visibility.folder);
    let long =
        std::path::PathBuf::from(format!("J:/missing/{}", "音楽e\u{301}長い名前".repeat(30)));
    app.filter_rule.folders.insert(long.clone());
    let model = gallery_rail_model(&app);
    assert_eq!(model.folder_label, "2 FOLDERS");
    let chooser = view::folder_popup_model(&app);
    assert!(chooser
        .identity
        .concat()
        .contains(&long.display().to_string()));
    assert!(chooser
        .rows
        .iter()
        .any(|row| row.path.as_ref() == Some(&long) && row.unavailable && row.selected));
    assert!(!gallery_rail_model(&app).visibility.resolution);
    app.filter_rule.minimum_width = 0;
    app.filter_rule.minimum_height = 1080;
    assert!(gallery_rail_model(&app).visibility.resolution);
    assert_eq!(gallery_rail_model(&app).resolution_label, "≥0×1080");
    let _ = update(&mut app, Message::CycleResolution);
    assert_eq!(
        (
            app.filter_rule.minimum_width,
            app.filter_rule.minimum_height
        ),
        (0, 0)
    );
}

#[test]
fn ret03_conditional_size_and_mute_projection_follow_existing_gates() {
    let (mut app, _) = d3_application_fixture(0);
    for (width, height, visible) in [
        (0, 0, false),
        (1920, 0, true),
        (0, 1080, true),
        (1920, 1080, true),
    ] {
        app.filter_rule.minimum_width = width;
        app.filter_rule.minimum_height = height;
        assert_eq!(gallery_rail_model(&app).visibility.resolution, visible);
    }
    app.state.frontend_preferences.visibility.resolution = Some(false);
    assert!(
        gallery_rail_model(&app).visibility.resolution,
        "PK-02 active constraints override idle visibility"
    );
    let _ = update(&mut app, Message::ClearResolution);
    assert!(!gallery_rail_model(&app).visibility.resolution);
    app.launch_policy = p1_preview_policy();
    let prior_mute = app.state.global_muted;
    let _ = update(
        &mut app,
        Message::DonorRail(donor_rail::RailIntent::ToggleEngineMute),
    );
    assert_eq!(app.state.global_muted, prior_mute);
    assert_eq!(
        app.status.as_deref(),
        Some("Provider execution is unavailable in --preview.")
    );
}

#[test]
fn ret03_shortcut_comparators_keep_existing_discriminating_meanings() {
    use crate::library::color_metadata::{ColorMetadata, MetadataView, ALGORITHM_VERSION};
    let (app, _) = d4a_fixture(4);
    let mut catalog = app.catalog.clone();
    for (index, (title, kind, folder, width, height)) in [
        ("Title 10", "Z", "J:/b", 10, 10),
        ("Title 2", "a", "J:/A", 20, 20),
        ("Title 3", "B", "J:/c", 30, 30),
        ("Title 1", "c", "J:/d", 40, 40),
    ]
    .into_iter()
    .enumerate()
    {
        let item = &mut catalog[index];
        item.title = title.into();
        item.kind = kind.into();
        item.folder = folder.into();
        item.width = width;
        item.height = height;
        item.modified_unix = index as u64;
    }
    let mut metadata = MetadataView::new();
    for (index, hue, richness) in [(0, Some(240.0), 0.2), (1, Some(0.0), 0.8), (2, None, 0.5)] {
        metadata.insert(
            catalog[index].id.clone(),
            ColorMetadata {
                version: ALGORITHM_VERSION,
                fingerprint: "A".repeat(64),
                dominant_rgb: [0, 0, 0],
                hue,
                saturation: 0.5,
                brightness: 0.5,
                richness,
            },
        );
    }
    let mut usage = crate::candidate_state::local_library::UsageHistory::default();
    for (index, count, time) in [(0, 8, 10), (1, 2, 30), (2, 4, 20)] {
        usage.items.insert(
            catalog[index].id.clone(),
            crate::candidate_state::local_library::ItemUsage {
                confirmed_count: count,
                last_confirmed_unix: time,
                retained: Default::default(),
            },
        );
    }
    let sorting = crate::library::sort_metrics::MetadataView::from([
        (
            catalog[0].id.clone(),
            crate::library::sort_metrics::SortMetrics {
                version: 1,
                hue: 8,
                saturation: 80,
                richness: 200,
            },
        ),
        (
            catalog[1].id.clone(),
            crate::library::sort_metrics::SortMetrics {
                version: 1,
                hue: 0,
                saturation: 100,
                richness: 800,
            },
        ),
        (
            catalog[2].id.clone(),
            crate::library::sort_metrics::SortMetrics {
                version: 1,
                hue: 99,
                saturation: 0,
                richness: 500,
            },
        ),
    ]);
    for (order, expected) in [
        (SortOrder::Newest, [3, 2, 1, 0]),
        (SortOrder::ResolutionDescending, [3, 2, 1, 0]),
        (SortOrder::Colour, [1, 0, 2, 3]),
        (SortOrder::ColourPop, [0, 1, 2, 3]),
        (SortOrder::Richness, [1, 2, 0, 3]),
        (SortOrder::Minimalist, [0, 2, 1, 3]),
        (SortOrder::Recent, [1, 2, 0, 3]),
    ] {
        let result = library::query_catalog(
            &catalog,
            &Default::default(),
            &Default::default(),
            order,
            &Default::default(),
            &metadata,
            &sorting,
            &usage,
        );
        assert_eq!(
            result
                .iter()
                .map(|entry| entry.id.clone())
                .collect::<Vec<_>>(),
            expected.map(|index| catalog[index].id.clone()),
            "{order:?}"
        );
    }
}

#[test]
fn ret03_final_anchor_clamps_offsets_without_changing_preferences() {
    let (mut app, _) = d4a_fixture(2);
    app.viewport = [640.0, 512.0];
    for vertical in [false, true] {
        for locale in ["en-US", "ar-SA"] {
            app.settings_locale = localization::Catalog::new("en-US", "en-US").unwrap();
            app.settings_locale.language = localization::LANGUAGES
                .iter()
                .find(|l| l.tag == locale)
                .unwrap();
            assert_eq!(app.settings_locale.language.rtl, locale == "ar-SA");
            app.state.frontend_preferences.filter_bar.orientation =
                Some(if vertical { "vertical" } else { "horizontal" }.into());
            for offset in [-10000.0, 10000.0] {
                app.state.frontend_preferences.filter_bar.offset_x = Some(offset);
                app.state.frontend_preferences.filter_bar.offset_y = Some(offset);
                let model = gallery_rail_model(&app);
                let size = donor_rail::footprint(&model);
                let at = view::gallery_rail_anchor(&app, &model, 520.0);
                assert!(
                    at[0] >= 0.0
                        && at[1] >= 0.0
                        && at[0] + size[0] <= 640.01
                        && at[1] + size[1] <= 512.01
                );
                assert_eq!(
                    app.state.frontend_preferences.filter_bar.offset_x,
                    Some(offset)
                );
                assert_eq!(
                    app.state.frontend_preferences.filter_bar.offset_y,
                    Some(offset)
                );
            }
        }
    }
}

#[test]
fn ret03_resolution_cycle_preserves_existing_folder_and_empty_result_semantics() {
    let (mut app, _) = d4a_fixture(3);
    let baseline = app.filtered_indices.len();
    assert_eq!(baseline, 3);
    use crate::candidate_state::preferences::{Orientation, ResolutionPreset};
    app.state.frontend_preferences.resolution_presets = [(1920, 1080), (2560, 1440), (3840, 2160)]
        .map(|from| ResolutionPreset {
            retained: Default::default(),
            label: format!("{}", from.0),
            orientation: Orientation::Wide,
            from,
            to: None,
        })
        .into();
    for expected in [
        Some((1920, 1080)),
        Some((2560, 1440)),
        Some((3840, 2160)),
        None,
    ] {
        let _ = update(&mut app, Message::CycleResolution);
        assert_eq!(
            app.filter_rule
                .resolution_preset
                .as_ref()
                .map(|preset| preset.from),
            expected
        );
        assert_eq!(
            (
                app.filter_rule.minimum_width,
                app.filter_rule.minimum_height
            ),
            (0, 0)
        );
        assert!(
            gallery_rail_model(&app).visibility.resolution,
            "applicable saved presets expose idle control too"
        );
        if expected.is_none() {
            assert_eq!(app.filtered_indices.len(), baseline);
        } else {
            assert!(app.filtered_indices.is_empty());
        }
    }
    for sticky in [false, true] {
        app.state.frontend_preferences.filter_bar.sticky = Some(sticky);
        app.filter_rule
            .folders
            .insert("J:/owned-unavailable".into());
        let _ = update(&mut app, Message::CycleResolution);
        assert_eq!(!app.filter_rule.folders.is_empty(), sticky);
        app.filter_rule.minimum_width = 0;
        app.filter_rule.minimum_height = 0;
        app.filter_rule.resolution_preset = None;
        app.filter_rule.folders.clear();
    }
}

#[test]
#[ignore = "PK-01 owned eight-frame production sort composition; explicit invocation"]
fn pk01_seven_sorts_owned_production_compositions_1080p_1440p() {
    use iced::advanced::Renderer as _;
    use iced::advanced::{clipboard, layout, renderer::Headless, widget::Tree, Layout, Shell};
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
        fonts.load_font(std::borrow::Cow::Borrowed(
            localization::Catalog::new("en-US", "en-US")
                .unwrap()
                .font_bytes(),
        ));
        fonts.load_font(std::borrow::Cow::Borrowed(
            include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
        ));
    }
    let mut captures = Vec::new();
    for (width, height) in [(1920u32, 1080u32), (2560, 1440)] {
        for mode in [
            GalleryMode::Slices,
            GalleryMode::Geometric,
            GalleryMode::Wall,
            GalleryMode::Depth,
        ] {
            let (mut app, submissions) = d4a_fixture(2);
            app.viewport = [width as f32, height as f32];
            app.launch_policy = p1_preview_policy();
            app.config.ui.show_top_bar = true;
            app.config.ui.show_title = false;
            app.config.ui.show_close_button = true;
            app.rail_reveal = 1.0;
            app.gallery_mode = mode;
            app.state.frontend_preferences.interface.ui_scale = Some(1.0);
            app.state.frontend_preferences.filter_bar.always_visible = Some(true);
            let model = gallery_rail_model(&app);
            let gallery_height = match mode {
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
                GalleryMode::Slices => app.state.gallery.slices.height,
            };
            let at = view::gallery_rail_anchor(&app, &model, gallery_height);
            let footprint = donor_rail::footprint(&model);
            assert!(
                at[0] + footprint[0] <= width as f32 + 0.01
                    && at[1] + footprint[1] <= height as f32 + 0.01
            );
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            renderer.reset(bounds);
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            assert_eq!(node.size(), size);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: iced::Color::WHITE,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let pixels = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                iced::Color::from_rgb8(40, 42, 50),
            );
            let path = output.join(format!("{width}x{height}-{mode:?}.png"));
            image::save_buffer_with_format(
                &path,
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            )
            .unwrap();
            for order in SortOrder::ALL {
                let intent = donor_rail::RailIntent::SetSort(order);
                let local = donor_rail::intent_center(&model, intent)
                    .expect("all seven sort controls are visible");
                let point = iced::Point::new(at[0] + local.x, at[1] + local.y);
                let mut messages = Vec::new();
                element.as_widget_mut().update(
                    &mut tree,
                    &iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
                        iced::mouse::Button::Left,
                    )),
                    Layout::new(&node),
                    iced::mouse::Cursor::Available(point),
                    &renderer,
                    &mut clipboard::Null,
                    &mut Shell::new(&mut messages),
                    &bounds,
                );
                assert!(
                    messages
                        .iter()
                        .any(|m| matches!(m,Message::DonorRail(actual) if *actual==intent)),
                    "{mode:?} {order:?} hit"
                );
            }
            assert_eq!(submissions.load(Ordering::Acquire), 0);
            captures.push(serde_json::json!({"path":path,"viewport":[width,height],"mode":format!("{mode:?}"),"sort_intents":7,"fixture":"injected catalog; actual production view; no provider"}));
        }
    }
    assert_eq!(captures.len(), 8);
    std::fs::write(
        output.join("CAPTURE-INDEX.json"),
        serde_json::to_vec_pretty(&captures).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "PK-02 owned production editor/rail composition and hit checks; explicit invocation"]
fn pk02_resolution_editor_and_rail_production_compositions() {
    use crate::candidate_state::preferences::{Orientation, ResolutionPreset};
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
            Message::SettingsLegacy { message, .. } => action(message, expected),
            Message::SettingsIntent {
                intent: Intent::Settings(SettingsMsg::Run(actual)),
                ..
            } => *actual == expected,
            _ => false,
        }
    }
    fn closes(message: &Message) -> bool {
        match message {
            Message::SettingsLegacy { message, .. } => closes(message),
            Message::SettingsIntent {
                intent: Intent::ToggleSettings,
                ..
            } => true,
            _ => false,
        }
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
    iced::advanced::graphics::text::font_system()
        .write()
        .unwrap()
        .load_font(std::borrow::Cow::Borrowed(
            include_bytes!("../../../assets/fonts/SymbolsNerdFont-Regular.ttf").as_slice(),
        ));
    let mut receipts = Vec::new();
    let preset = ResolutionPreset {
        retained: Default::default(),
        label: "Custom / 長い名前の解像度範囲".repeat(3),
        orientation: Orientation::Wide,
        from: (1920, 1080),
        to: Some((2559, 1439)),
    };
    for (n, width, height, scale, locale, populated) in [
        (0, 1920u32, 1080u32, 1.0, "en-US", false),
        (1, 1920, 1080, 1.0, "en-US", true),
        (2, 1920, 1080, 0.5, "zh-CN", true),
        (3, 900, 720, 1.0, "en-US", true),
        (4, 640, 720, 0.5, "zh-CN", true),
        (5, 640, 720, 1.0, "en-US", false),
        (6, 2560, 1440, 1.0, "zh-CN", true),
        (7, 960, 720, 0.5, "en-US", false),
    ] {
        let (mut app, submissions) = d4a_fixture(2);
        app.viewport = [width as f32, height as f32];
        app.launch_policy = p1_preview_policy();
        app.settings_locale = localization::Catalog::new(locale, locale).unwrap();
        iced::advanced::graphics::text::font_system()
            .write()
            .unwrap()
            .load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
        app.state.frontend_preferences.interface.ui_scale = Some(scale);
        if populated {
            app.state.frontend_preferences.resolution_presets = vec![
                preset.clone(),
                ResolutionPreset {
                    retained: Default::default(),
                    label: "Tall 8K+".into(),
                    orientation: Orientation::Tall,
                    from: (4320, 7680),
                    to: None,
                },
            ];
        }
        app.settings_workbench = view::settings::bridge::Session::from_state(&app.state);
        app.panel_state
            .open_settings(SettingsCategory::CatalogFilters);
        view::settings::controller::reconcile(&mut app);
        p3_settings_intent(&mut app, SettingsMsg::SelectTab("filter".into()));
        p3_settings_intent(&mut app, SettingsMsg::SelectSection(2));
        if populated {
            app.settings_workbench
                .navigation
                .expanded
                .insert(format!("{}.0", keys::filter_bar::RESOLUTION_PRESETS));
            app.settings_workbench
                .navigation
                .expanded
                .insert(format!("{}.0.custom", keys::filter_bar::RESOLUTION_PRESETS));
            app.filter_rule.resolution_preset = Some(preset.clone());
        }
        app.settings_workbench.folio_entrance = None;
        app.settings_workbench.tab_reveal = None;
        let window = app.window.unwrap();
        let theme = theme(&app, window);
        let size = iced::Size::new(width as f32, height as f32);
        let bounds = iced::Rectangle::with_size(size);
        renderer.reset(bounds);
        let mut element = view(&app, window);
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let mut p = probe(&mut element, &mut tree, &node, &renderer);
        let close_glyph = p
            .texts
            .iter()
            .find(|(text, _)| text == "×")
            .expect("header Close glyph")
            .1;
        let minimum = 30.0 * (scale as f32).max(1.0);
        let close = p
            .containers
            .iter()
            .map(|(_, rect)| *rect)
            .filter(|rect| {
                rect.contains(close_glyph.center())
                    && rect.width >= minimum - 0.1
                    && rect.height >= minimum - 0.1
                    && (rect.width - rect.height).abs() < 0.1
            })
            .min_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)))
            .expect("full square Close target");
        assert!(
            close.x >= 0.0
                && close.y >= 0.0
                && close.x + close.width <= bounds.width + 0.01
                && close.y + close.height <= bounds.height + 0.01,
            "{n} full Close target must fit"
        );
        let close_messages = click(
            &mut element,
            &mut tree,
            &node,
            &renderer,
            bounds,
            close.center(),
        );
        assert!(
            close_messages.iter().any(closes),
            "{n} actual Close hit: {close_messages:?}"
        );
        if width == 640 && scale == 1.0 && locale == "en-US" {
            let title = app
                .settings_locale
                .text("settings-section-resolution-presets");
            let title_bounds = p
                .texts
                .iter()
                .filter(|(text, _)| text == &title)
                .max_by(|(_, a), (_, b)| a.height.total_cmp(&b.height))
                .expect("actual reading-header section title")
                .1;
            assert!(title_bounds.height > 2.0 * 46.0, "640/default long word must wrap onto its allocated lines instead of painting over actions: {title_bounds:?}");
            assert!(
                title_bounds.x + title_bounds.width <= close.x + 0.01,
                "title allocation reserves header actions"
            );
        }
        let mut hits = vec!["Close Settings".to_owned()];
        for (label, expected) in [
            (
                app.settings_locale
                    .text("settings-filter-resolution-band-fhd"),
                ActionId::CreateResolutionPresetBand("fhd"),
            ),
            (
                app.settings_locale
                    .text("settings-filter-resolution-band-custom"),
                ActionId::CreateResolutionPresetBand("custom"),
            ),
            (
                app.settings_locale.text("pk02-next-resolution"),
                ActionId::CycleResolution,
            ),
            (
                app.settings_locale.text("pk02-clear-resolution"),
                ActionId::ClearResolution,
            ),
        ] {
            let rect = p
                .texts
                .iter()
                .find(|(text, rect)| text == &label && bounds.contains(rect.center()))
                .unwrap_or_else(|| panic!("{n} mounted action missing {label}; {:?}", p.texts))
                .1;
            let messages = click(
                &mut element,
                &mut tree,
                &node,
                &renderer,
                bounds,
                rect.center(),
            );
            assert!(
                messages.iter().any(|m| action(m, expected)),
                "{n} actual action hit {label}: {messages:?}"
            );
            hits.push(label);
        }
        if populated {
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: iced::Color::WHITE,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let pixels = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                iced::Color::from_rgb8(40, 42, 50),
            );
            let path = output.join(format!(
                "editor-top-{n}-{width}x{height}-{scale}-{locale}.png"
            ));
            image::save_buffer_with_format(
                &path,
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            )
            .unwrap();
            receipts.push(serde_json::json!({"path":path,"surface":"production Settings resolution editor top","viewport":[width,height],"scale":scale,"locale":locale,"populated":true,"native":false}));
            let remove = app.settings_locale.text("settings-filter-remove-action");
            let mut reached = false;
            for _ in 0..18 {
                p = probe(&mut element, &mut tree, &node, &renderer);
                let clip = p
                    .scrolls
                    .iter()
                    .filter(|(_, rect, _, _)| rect.width > 200.0 * scale as f32)
                    .last()
                    .map(|x| x.1)
                    .unwrap_or(bounds);
                if let Some((_, rect)) = p.texts.iter().find(|(text, rect)| {
                    text == &remove
                        && clip.contains(rect.center())
                        && bounds.contains(rect.center())
                }) {
                    let messages = click(
                        &mut element,
                        &mut tree,
                        &node,
                        &renderer,
                        bounds,
                        rect.center(),
                    );
                    if messages
                        .iter()
                        .any(|m| action(m, ActionId::RemoveResolutionPreset(0)))
                    {
                        reached = true;
                        break;
                    }
                }
                let point = clip.center();
                let mut messages = Vec::new();
                element.as_widget_mut().update(
                    &mut tree,
                    &iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                        delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: -160.0 },
                    }),
                    Layout::new(&node),
                    iced::mouse::Cursor::Available(point),
                    &renderer,
                    &mut clipboard::Null,
                    &mut Shell::new(&mut messages),
                    &bounds,
                );
            }
            assert!(
                reached,
                "{n} remove must remain reachable through production scroll"
            );
            hits.push(remove);
        }
        renderer.reset(bounds);
        element.as_widget().draw(
            &tree,
            &mut renderer,
            &theme,
            &iced::advanced::renderer::Style {
                text_color: iced::Color::WHITE,
            },
            Layout::new(&node),
            iced::mouse::Cursor::Unavailable,
            &bounds,
        );
        let pixels = Headless::screenshot(
            &mut renderer,
            iced::Size::new(width, height),
            1.0,
            iced::Color::from_rgb8(40, 42, 50),
        );
        let path = output.join(format!("editor-{n}-{width}x{height}-{scale}-{locale}.png"));
        image::save_buffer_with_format(
            &path,
            &pixels,
            width,
            height,
            image::ColorType::Rgba8,
            image::ImageFormat::Png,
        )
        .unwrap();
        assert_eq!(submissions.load(Ordering::Acquire), 0);
        receipts.push(serde_json::json!({"path":path,"surface":"production Settings resolution editor","viewport":[width,height],"scale":scale,"locale":locale,"populated":populated,"actual_headless_action_hits":hits,"native":false}));
    }
    for (mode, n) in [
        (GalleryMode::Slices, 0),
        (GalleryMode::Geometric, 1),
        (GalleryMode::Wall, 2),
        (GalleryMode::Depth, 3),
    ] {
        for (width, height, scale, legacy) in [
            (1920u32, 1080u32, 1.0, false),
            (640, 720, 0.5, true),
            (900, 720, 1.0, false),
        ] {
            let (mut app, submissions) = d4a_fixture(2);
            app.viewport = [width as f32, height as f32];
            app.launch_policy = p1_preview_policy();
            app.gallery_mode = mode;
            app.rail_reveal = 1.0;
            app.state.frontend_preferences.interface.ui_scale = Some(scale);
            app.state.frontend_preferences.filter_bar.always_visible = Some(true);
            if legacy {
                app.filter_rule.minimum_width = 1920;
                app.filter_rule.minimum_height = 1080;
            } else {
                app.filter_rule.resolution_preset = Some(preset.clone());
                app.filter_rule.orientation = OrientationFilter::Portrait;
            }
            app.state.frontend_preferences.visibility.resolution = Some(false);
            if width == 900 {
                app.filter_rule.resolution_preset = None;
                app.filter_rule.orientation = OrientationFilter::Any;
                app.state.frontend_preferences.resolution_presets = vec![preset.clone()];
                app.state.frontend_preferences.visibility.resolution = None;
            }
            let model = gallery_rail_model(&app);
            assert!(model.visibility.resolution);
            let footprint = donor_rail::footprint(&model);
            let anchor = view::gallery_rail_anchor(&app, &model, 500.0);
            assert!(
                anchor[0] >= 0.0
                    && anchor[1] >= 0.0
                    && anchor[0] + footprint[0] <= width as f32 + 0.01
                    && anchor[1] + footprint[1] <= height as f32 + 0.01,
                "{mode:?} constrained rail fits"
            );
            let local =
                donor_rail::intent_center(&model, donor_rail::RailIntent::CycleResolution).unwrap();
            // Use actual gallery height to find the root-composed rail, matching production owner.
            let gh = match mode {
                GalleryMode::Slices => app.state.gallery.slices.height,
                GalleryMode::Depth => app.state.gallery.depth.height,
                GalleryMode::Wall => {
                    app.state.gallery.wall.card_height * app.state.gallery.wall.rows as f32
                        + app.state.gallery.wall.gap_y
                            * app.state.gallery.wall.rows.saturating_sub(1) as f32
                }
                GalleryMode::Geometric => donor_rail::geometric_height(
                    app.state.gallery.geometric.radius,
                    app.state.gallery.geometric.rows,
                    app.state.gallery.geometric.gap_y,
                    app.state.gallery.geometric.stagger,
                    donor_layout::HexShape::Hexagon,
                ),
            };
            let actual_anchor = view::gallery_rail_anchor(&app, &model, gh);
            let point = iced::Point::new(actual_anchor[0] + local.x, actual_anchor[1] + local.y);
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            renderer.reset(bounds);
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            let messages = click(&mut element, &mut tree, &node, &renderer, bounds, point);
            assert!(
                messages.iter().any(|m| matches!(
                    m,
                    Message::DonorRail(donor_rail::RailIntent::CycleResolution)
                )),
                "{mode:?} rail actual hit"
            );
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: iced::Color::WHITE,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let pixels = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                iced::Color::from_rgb8(40, 42, 50),
            );
            let path = output.join(format!("rail-{n}-{width}x{height}-{scale}.png"));
            image::save_buffer_with_format(
                &path,
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            )
            .unwrap();
            assert_eq!(submissions.load(Ordering::Acquire), 0);
            receipts.push(serde_json::json!({"path":path,"surface":"production gallery rail","mode":format!("{mode:?}"),"viewport":[width,height],"scale":scale,"legacy":legacy,"active_unmatched_snapshot":!legacy && width != 900,"idle_applicable":width == 900,"resolution_hit":true,"native":false}));
        }
    }
    std::fs::write(
        output.join("CAPTURE-INDEX.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
}

#[test]
#[ignore = "PK-03 owned production state/control compositions; explicit headless invocation"]
fn pk03_rotation_settings_and_playlist_production_compositions() {
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
            Message::SettingsLegacy { message, .. } => action(message, expected),
            Message::SettingsIntent {
                intent: Intent::Settings(SettingsMsg::Run(actual)),
                ..
            } => *actual == expected,
            _ => false,
        }
    }
    fn closes(message: &Message) -> bool {
        match message {
            Message::SettingsLegacy { message, .. } => closes(message),
            Message::SettingsIntent {
                intent: Intent::ToggleSettings,
                ..
            } => true,
            _ => false,
        }
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
    fn random_toggle(message: &Message) -> bool {
        match message {
            Message::SettingsLegacy { message, .. } => random_toggle(message),
            Message::SettingsIntent {
                intent: Intent::Settings(SettingsMsg::Toggle(path, _)),
                ..
            } => path == keys::general::RANDOM_ROTATE,
            _ => false,
        }
    }
    let mut receipts = Vec::new();
    for (width, height, scale, locale) in [
        (1920u32, 1080u32, 1.0, "en-US"),
        (2560, 1440, 0.5, "zh-CN"),
        (900, 720, 1.0, "en-US"),
    ] {
        for case in [
            "off",
            "on",
            "paused",
            "stopped",
            "manager-start",
            "manager-stop",
        ] {
            let (mut app, submissions) = d4a_fixture(2);
            app.viewport = [width as f32, height as f32];
            app.launch_policy = p1_preview_policy();
            app.settings_locale = localization::Catalog::new(locale, locale).unwrap();
            iced::advanced::graphics::text::font_system()
                .write()
                .unwrap()
                .load_font(std::borrow::Cow::Borrowed(app.settings_locale.font_bytes()));
            app.state.frontend_preferences.interface.ui_scale = Some(scale);
            app.state.playlists = vec![candidate_state::Playlist {
                id: "owned".into(),
                name: "Owned playlist / 長い名前".into(),
                order: candidate_state::PlaybackOrder::Sequential,
                membership: candidate_state::PlaylistMembership::Explicit {
                    wallpaper_ids: vec![app.wallpapers[0].id.clone()],
                },
            }];
            app.state.active_playlist_id = Some("owned".into());
            app.state.automation_intent = Some(candidate_state::AutomationIntent {
                random_enabled: !matches!(case, "off" | "manager-start"),
                playlist_playing: matches!(case, "paused" | "manager-stop"),
                safely_stopped: case == "stopped",
                ..Default::default()
            });
            let manager = case.starts_with("manager");
            if manager {
                app.panel_state.settings_open = false;
                app.playlist_panel_open = true;
                app.playlist_picker = false;
                app.playlist_browsing_id = Some("owned".into());
            } else {
                app.settings_workbench = view::settings::bridge::Session::from_state(&app.state);
                app.panel_state.open_settings(SettingsCategory::Automation);
                view::settings::controller::reconcile(&mut app);
                p3_settings_intent(&mut app, SettingsMsg::SelectSection(0));
                app.settings_workbench.folio_entrance = None;
                app.settings_workbench.tab_reveal = None;
            }
            let expected_state = app.random_rotation_state();
            let window = app.window.unwrap();
            let theme = theme(&app, window);
            let size = iced::Size::new(width as f32, height as f32);
            let bounds = iced::Rectangle::with_size(size);
            renderer.reset(bounds);
            let mut element = view(&app, window);
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style {
                    text_color: iced::Color::WHITE,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &bounds,
            );
            let top_pixels = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.0,
                iced::Color::from_rgb8(40, 42, 50),
            );
            let top_path = output.join(format!(
                "pk03-top-{case}-{width}x{height}-{scale}-{locale}.png"
            ));
            image::save_buffer_with_format(
                &top_path,
                &top_pixels,
                width,
                height,
                image::ColorType::Rgba8,
                image::ImageFormat::Png,
            )
            .unwrap();
            let mut scrolled = false;
            let mut received = None;
            let mut seen_status = false;
            let mut hits = Vec::new();
            for _ in 0..18 {
                let p = probe(&mut element, &mut tree, &node, &renderer);
                seen_status |= p
                    .texts
                    .iter()
                    .any(|(text, _)| text.contains(expected_state.label()));
                for (text, rect) in &p.texts {
                    if !bounds.contains(rect.center()) {
                        continue;
                    }
                    let candidate = match case {
                        "off" => {
                            text == app.settings_locale.text_static("settings-control-disabled")
                        }
                        "on" => text == app.settings_locale.text_static("settings-control-enabled"),
                        "stopped" => text == "Resume random",
                        "manager-start" => text == app.settings_locale.text_static("r5-set-active"),
                        "manager-stop" => {
                            text == app.settings_locale.text_static("playlists-stop-edition")
                        }
                        _ => text == "Stop playlist",
                    };
                    if !candidate {
                        continue;
                    }
                    let messages = click(
                        &mut element,
                        &mut tree,
                        &node,
                        &renderer,
                        bounds,
                        rect.center(),
                    );
                    let selected = messages.into_iter().find(|m| match case {
                        "off" | "on" => random_toggle(m),
                        "paused" => action(m, ActionId::StopPlaylist),
                        "stopped" => action(m, ActionId::ResumeRandomRotation),
                        "manager-start" => {
                            matches!(m,Message::SetActivePlaylist(id, _) if id == "owned")
                        }
                        _ => matches!(m, Message::StopPlaylist { .. }),
                    });
                    if let Some(message) = selected {
                        received = Some(message);
                        hits.push(text.clone());
                        break;
                    }
                }
                if received.is_some() {
                    break;
                }
                // Manager sequence and definition have independent owners; use the actual control ancestor.
                let intended_scroll = if manager {
                    p.scrolls
                        .iter()
                        .find(|(id, ..)| *id == Some(iced::widget::Id::new("playlist-definition")))
                        .or_else(|| {
                            p.scrolls.iter().find(|(id, ..)| {
                                *id == Some(iced::widget::Id::new("playlist-reading"))
                            })
                        })
                } else {
                    p.scrolls
                        .iter()
                        .filter(|(_, rect, _, _)| rect.width > 200.0 * scale as f32)
                        .last()
                };
                let point = intended_scroll
                    .map(|(_, r, _, _)| r.center())
                    .unwrap_or(bounds.center());
                scrolled = true;
                let mut messages = Vec::new();
                element.as_widget_mut().update(
                    &mut tree,
                    &iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                        delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: -140.0 },
                    }),
                    Layout::new(&node),
                    iced::mouse::Cursor::Available(point),
                    &renderer,
                    &mut clipboard::Null,
                    &mut Shell::new(&mut messages),
                    &bounds,
                );
            }
            let path = if scrolled {
                output.join(format!(
                    "pk03-hit-{case}-{width}x{height}-{scale}-{locale}.png"
                ))
            } else {
                top_path.clone()
            };
            if scrolled {
                renderer.reset(bounds);
                element.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &theme,
                    &iced::advanced::renderer::Style {
                        text_color: iced::Color::WHITE,
                    },
                    Layout::new(&node),
                    iced::mouse::Cursor::Unavailable,
                    &bounds,
                );
                let pixels = Headless::screenshot(
                    &mut renderer,
                    iced::Size::new(width, height),
                    1.0,
                    iced::Color::from_rgb8(40, 42, 50),
                );
                image::save_buffer_with_format(
                    &path,
                    &pixels,
                    width,
                    height,
                    image::ColorType::Rgba8,
                    image::ImageFormat::Png,
                )
                .unwrap();
            }
            let diagnostic = probe(&mut element, &mut tree, &node, &renderer);
            std::fs::write(output.join(format!("probe-{case}-{width}x{height}-{scale}-{locale}.json")),
                serde_json::to_vec_pretty(&serde_json::json!({
                    "state":expected_state.label(),"hits":hits,
                    "texts":diagnostic.texts.iter().map(|(text,r)|serde_json::json!({"text":text,"bounds":[r.x,r.y,r.width,r.height]})).collect::<Vec<_>>(),
                    "scrolls":diagnostic.scrolls.iter().map(|(id,r,content,translation)|serde_json::json!({"id":format!("{id:?}"),"bounds":[r.x,r.y,r.width,r.height],"content":[content.x,content.y,content.width,content.height],"translation":[translation.x,translation.y]})).collect::<Vec<_>>()
                })).unwrap()).unwrap();
            assert!(seen_status, "mounted state missing {case} {width}/{scale}");
            let message = received.unwrap_or_else(|| {
                panic!("actual production control hit missing {case} {width}/{scale}")
            });
            drop(element);
            let before_generation = app.automation_generation.load(Ordering::Acquire);
            let before_time = unix_now();
            let _ = update(&mut app, message);
            match case {
                "off" | "stopped" => assert_eq!(
                    app.random_rotation_state(),
                    candidate_state::RandomRotationState::On
                ),
                "on" => assert_eq!(
                    app.random_rotation_state(),
                    candidate_state::RandomRotationState::Off
                ),
                "paused" | "manager-stop" => {
                    assert!(!app.playlist_playing());
                    assert_eq!(
                        app.random_rotation_state(),
                        candidate_state::RandomRotationState::On
                    );
                }
                _ => assert!(app.playlist_playing()),
            }
            assert!(app.automation_generation.load(Ordering::Acquire) > before_generation);
            assert!(app.next_slideshow_unix >= before_time + app.state.slideshow_interval_seconds);
            assert!(!app.apply_pending);
            assert_eq!(submissions.load(Ordering::Acquire), 0);
            assert_eq!(
                CandidateState::load(&app.state_path).automation_intent(),
                app.state.automation_intent()
            );
            receipts.push(serde_json::json!({"path":path,"top_path":top_path,"separate_hit_frame":scrolled,"viewport":[width,height],"scale":scale,"locale":locale,"case":case,"visible_state":expected_state.label(),"actual_headless_action_hits":hits,"saved_action_readback":true,"provider_submissions":0,"native":false}));
        }
    }
    assert_eq!(receipts.len(), 18);
    std::fs::write(
        output.join("PK03-CAPTURE-INDEX.json"),
        serde_json::to_vec_pretty(&receipts).unwrap(),
    )
    .unwrap();
}

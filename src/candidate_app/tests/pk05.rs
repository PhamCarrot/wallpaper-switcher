//! PK-05 browse chooser, existing query and distinct saved-owner contracts.
use super::*;
use crate::candidate_app::playlists::{Browse, Edit};

fn fixture() -> (GpuCandidate, Arc<std::sync::atomic::AtomicUsize>) {
    let (mut app, calls) = d3_application_fixture(5);
    app.launch_policy = p1_preview_policy();
    app.gallery_mode = GalleryMode::Slices;
    app.sort_order = SortOrder::Newest;
    app.state.playlists.clear();
    app.state.playlists.push(Playlist {
        id: "manual-42".into(),
        name: "Duplicate / 音楽".into(),
        order: PlaybackOrder::Shuffle,
        membership: PlaylistMembership::Explicit {
            wallpaper_ids: vec![
                app.wallpapers[3].id.clone(),
                app.wallpapers[1].id.clone(),
                "missing".into(),
            ],
        },
    });
    app.state.playlists.push(Playlist {
        id: "smart-27".into(),
        name: "Duplicate / 音楽".into(),
        order: PlaybackOrder::Sequential,
        membership: PlaylistMembership::Smart {
            rule: SmartRule {
                tags: ["blue".into()].into(),
                ..Default::default()
            },
        },
    });
    app.state.playlists.push(Playlist {
        id: "empty".into(),
        name: "Empty existing list".into(),
        order: PlaybackOrder::Sequential,
        membership: PlaylistMembership::Explicit {
            wallpaper_ids: Vec::new(),
        },
    });
    for (i, entry) in app.catalog.iter_mut().enumerate() {
        entry.title = format!("Wallpaper {i}");
        entry.width = if i % 2 == 1 { 1920 } else { 900 };
        entry.height = 1080;
        entry.modified_unix = i as u64;
        entry.tags = [if i % 2 == 1 { "blue" } else { "green" }.into()].into();
        entry.source_tags = entry.tags.clone();
        entry.folder = app
            .state_path
            .with_file_name(if i % 2 == 1 { "a" } else { "b" });
    }
    app.state.active_playlist_id = Some("smart-27".into());
    app.state.automation_intent = Some(candidate_state::AutomationIntent {
        random_enabled: true,
        playlist_playing: true,
        safely_stopped: false,
        ..Default::default()
    });
    app.state.schedules.push(candidate_state::Schedule {
        id: "schedule-preserved".into(),
        name: "Existing schedule".into(),
        enabled: true,
        days: 42,
        minute_of_day: 321,
        priority: 7,
        creation_order: 9,
        playlist_id: "smart-27".into(),
        monitor_ids: vec!["STABLE-A".into()],
    });
    app.state.local_library.usage.items.insert(
        "item-0001".into(),
        candidate_state::local_library::ItemUsage {
            confirmed_count: 4,
            last_confirmed_unix: 123,
            ..Default::default()
        },
    );
    app.next_slideshow_unix = 123456;
    app.last_automation_minute = Some((2, 234));
    app.state.save(&app.state_path).unwrap();
    (app, calls)
}
fn open(app: &mut GpuCandidate) {
    let _ = update(
        app,
        Message::DonorRail(donor_rail::RailIntent::TogglePlaylists),
    );
    assert!(app.playlist_panel_open && app.playlist_chooser && !app.playlist_picker);
}
fn browse(app: &mut GpuCandidate, action: Browse) {
    let owner = app.playlist_owner().unwrap();
    let _ = update(app, Message::PlaylistBrowse(owner, action));
}
fn choose(app: &mut GpuCandidate, id: Option<&str>) {
    open(app);
    browse(app, Browse::Choose(id.map(str::to_string)));
    assert!(!app.playlist_panel_open);
}

#[test]
fn pk05_three_routes_and_mode_bound_stale_callbacks_preserve_existing_binding() {
    let (mut app, calls) = fixture();
    app.selected = None;
    open(&mut app);
    let stale = app.playlist_owner().unwrap();
    browse(&mut app, Browse::Edit);
    assert!(app.playlist_panel_open && !app.playlist_chooser && !app.playlist_picker);
    let _ = update(
        &mut app,
        Message::PlaylistBrowse(stale, Browse::Choose(Some("manual-42".into()))),
    );
    assert!(app.browse_playlist_id.is_none());
    let _ = update(&mut app, Message::TogglePlaylistPanel);
    app.state.frontend_preferences.bindings.insert(
        candidate_state::preferences::BindingAction::Playlists,
        "p".into(),
    );
    let event_window = app.window.unwrap();
    let _ = update(
        &mut app,
        Message::KeyPressed {
            key: keyboard::Key::Character("p".into()),
            modifiers: keyboard::Modifiers::empty(),
            event: NavigationKeyEvent {
                event_window,
                physical_key: keyboard::key::Physical::Unidentified(
                    keyboard::key::NativeCode::Unidentified,
                ),
                location: keyboard::Location::Standard,
                repeat: false,
            },
        },
    );
    assert!(app.playlist_panel_open && !app.playlist_chooser);
    let _ = update(&mut app, Message::TogglePlaylistPanel);
    app.gallery_mode = GalleryMode::Geometric;
    app.selected = Some(1);
    app.detail_open = true;
    let id = app.wallpapers[1].id.clone();
    let _ = update(
        &mut app,
        Message::DonorDetail(id, donor_back_layout::DetailIntent::Playlists),
    );
    assert!(app.playlist_panel_open && app.playlist_picker && !app.playlist_chooser);
    let owner = app.playlist_owner().unwrap();
    let _ = update(
        &mut app,
        Message::PlaylistBrowse(owner, Browse::Choose(Some("manual-42".into()))),
    );
    assert!(app.browse_playlist_id.is_none());
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

#[test]
fn pk05_browse_and_all_never_publish_or_change_playback_schedule_history_due_or_provider() {
    let (mut app, calls) = fixture();
    let state = app.state.clone();
    let bytes = std::fs::read(&app.state_path).unwrap();
    let timer = (
        app.automation_generation.load(Ordering::Acquire),
        app.next_slideshow_unix,
        app.last_automation_minute,
        app.automation_suspended,
    );
    app.selected = Some(3);
    choose(&mut app, Some("manual-42"));
    assert_eq!(app.filtered_indices, [3, 1]);
    assert_eq!(app.selected, Some(3));
    assert_eq!(app.playlist_browsing_id, None);
    open(&mut app);
    browse(&mut app, Browse::Query("MANUAL-42".into()));
    browse(&mut app, Browse::Close);
    choose(&mut app, None);
    assert_eq!(app.filtered_indices.len(), 5);
    assert_eq!(app.state, state);
    assert_eq!(std::fs::read(&app.state_path).unwrap(), bytes);
    assert_eq!(
        (
            app.automation_generation.load(Ordering::Acquire),
            app.next_slideshow_unix,
            app.last_automation_minute,
            app.automation_suspended
        ),
        timer
    );
    assert!(app.active_operation.is_none() && !app.apply_pending);
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

#[test]
fn pk05_manual_and_smart_intersect_filters_keep_public_sort_and_stable_selection() {
    let (mut app, _) = fixture();
    app.sort_order = SortOrder::Newest;
    app.selected = Some(1);
    assert!(!app.browse_position_restored && !app.gallery_engaged);
    choose(&mut app, Some("manual-42"));
    assert_eq!(app.filtered_indices, [3, 1]);
    assert_eq!(app.selected, Some(1));
    choose(&mut app, Some("smart-27"));
    assert_eq!(app.filtered_indices, [3, 1]);
    let private =
        library::evaluate_playlist(&app.state.playlists[1], &app.catalog, &app.state.favorites);
    assert_eq!(
        private.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
        ["item-0001", "item-0003"]
    );
    for rule in [
        SmartRule {
            text: "Wallpaper 3".into(),
            ..Default::default()
        },
        SmartRule {
            tags: ["blue".into()].into(),
            excluded_tags: ["blocked".into()].into(),
            ..Default::default()
        },
        SmartRule {
            kinds: ["image".into()].into(),
            orientation: OrientationFilter::Landscape,
            minimum_width: 1900,
            minimum_height: 1000,
            ..Default::default()
        },
        SmartRule {
            folders: [app.catalog[3].folder.clone()].into(),
            ..Default::default()
        },
    ] {
        app.filter_rule = rule;
        app.refresh_filter();
        assert!(!app.filtered_indices.is_empty());
        assert!(app.filtered_indices.iter().all(|i| [1, 3].contains(i)));
    }
    app.filter_rule = SmartRule {
        resolution_preset: Some(candidate_state::preferences::ResolutionPreset {
            label: "FHD only".into(),
            orientation: candidate_state::preferences::Orientation::Wide,
            from: (1920, 1080),
            to: Some((1920, 1080)),
            retained: Default::default(),
        }),
        ..Default::default()
    };
    app.refresh_filter();
    assert_eq!(app.filtered_indices, [3, 1]);
    // Real stamped metadata remains owned by the existing still validator.
    for (i, rgb) in [(1, [255, 0, 0, 255]), (3, [0, 0, 255, 255])] {
        let sample = image::RgbaImage::from_pixel(2, 2, image::Rgba(rgb));
        let path = app
            .state_path
            .with_file_name(format!("pk05-colour-{i}.png"));
        sample.save(&path).unwrap();
        app.wallpapers[i].file_target = path.clone();
        app.wallpapers[i].preview_path = Some(path.clone());
        let still = library::media_still::MediaStill::select(
            &app.wallpapers[i],
            app.catalog_generation.load(Ordering::Acquire),
        )
        .unwrap();
        let metadata = library::color_metadata::analyse_sample(
            &sample,
            library::color_metadata::hash_bytes(&std::fs::read(&path).unwrap()),
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
    app.colours.query.bucket = Some(library::color_metadata::ColorBucket::Blue);
    app.refresh_filter();
    assert_eq!(app.filtered_indices, [3]);
    app.colours.query = Default::default();
    app.filter_rule = SmartRule {
        favorites_only: true,
        ..Default::default()
    };
    app.state.favorites = [app.catalog[3].id.clone()].into();
    app.refresh_filter();
    assert_eq!(app.filtered_indices, [3]);
    app.filter_rule = Default::default();
    app.monitor_ids = vec!["STABLE-A".into()];
    app.active_monitor_id = Some("STABLE-A".into());
    app.config
        .excluded_wallpapers
        .insert("Monitor0".into(), vec![app.catalog[3].id.clone()]);
    app.refresh_filter();
    assert_eq!(app.filtered_indices, [1]);
    app.show_excluded = true;
    app.config
        .disabled_wallpapers
        .insert("Monitor0".into(), vec![app.catalog[1].id.clone()]);
    app.refresh_filter();
    assert_eq!(app.filtered_indices, [3]);
    app.source_filter = SourceFilter::WallpaperEngine;
    app.refresh_filter();
    assert!(app.filtered_indices.is_empty());
}

#[test]
fn pk05_lifetime_search_tags_manager_hide_keep_folder_and_missing_clear_only_predicate() {
    let (mut app, _) = fixture();
    choose(&mut app, Some("manual-42"));
    let _ = update(&mut app, Message::SearchChanged("Wallpaper".into()));
    let _ = update(&mut app, Message::SearchTag("blue".into(), false));
    assert_eq!(app.browse_playlist_id.as_deref(), Some("manual-42"));
    let _ = update(&mut app, Message::TogglePlaylistPanel);
    let _ = update(&mut app, Message::BrowsePlaylist("smart-27".into()));
    let _ = update(&mut app, Message::TogglePlaylistPanel);
    assert_eq!(app.browse_playlist_id.as_deref(), Some("manual-42"));
    app.launch_policy = runtime::parse_launch_policy(&["candidate.exe".into()]).unwrap();
    let _ = update(&mut app, Message::HideGallery);
    assert!(!app.visible && !app.exit_requested);
    assert_eq!(app.browse_playlist_id.as_deref(), Some("manual-42"));
    app.visible = true;
    let _ = update(&mut app, Message::SetFolderFilter(None));
    assert!(app.browse_playlist_id.is_none());
    assert_eq!(app.filter_rule.text, "Wallpaper");
    assert!(app.filter_rule.tags.contains("blue"));
    choose(&mut app, Some("empty"));
    assert!(app.filtered_indices.is_empty());
    assert_eq!(app.browse_playlist_id.as_deref(), Some("empty"));
    app.state.playlists.retain(|p| p.id != "empty");
    app.refresh_filter();
    assert!(app.browse_playlist_id.is_none() && app.browse_playlist_unavailable);
    assert_eq!(app.filter_rule.text, "Wallpaper");
    assert!(app.filter_rule.tags.contains("blue"));
    choose(&mut app, None);
    assert!(!app.browse_playlist_unavailable);
    let loaded = CandidateState::load(&app.state_path);
    let (mut restarted, _) = d3_application_fixture(0);
    restarted.state = loaded;
    assert!(restarted.browse_playlist_id.is_none());
}

#[test]
fn pk05_catalog_membership_and_smart_tag_refresh_follow_current_ids_not_indices() {
    let (mut app, calls) = fixture();
    app.selected = Some(1);
    choose(&mut app, Some("manual-42"));
    let _ = update(&mut app, Message::TogglePlaylistPanel);
    app.set_playlist_membership("manual-42", "item-0003", false);
    assert_eq!(app.filtered_indices, [1]);
    assert_eq!(app.selected, Some(1));
    let _ = update(&mut app, Message::TogglePlaylistPanel);
    let mut entries = app.catalog.clone();
    entries.reverse();
    let mut wallpapers = app.wallpapers.clone();
    wallpapers.reverse();
    let generation = app.catalog_scan_generation.load(Ordering::Acquire);
    let _ = update(
        &mut app,
        Message::Wake(Wake::CatalogLoaded {
            generation,
            wallpapers,
            entries,
        }),
    );
    assert_eq!(app.filtered_indices, [3]);
    assert_eq!(
        app.selected.map(|i| app.wallpapers[i].id.as_str()),
        Some("item-0001")
    );
    choose(&mut app, Some("smart-27"));
    app.catalog[1].tags.clear();
    app.refresh_filter();
    assert_eq!(app.filtered_indices, [3]);
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

#[test]
fn pk05_captured_chooser_callbacks_reject_search_close_mode_window_visibility_catalog_changes() {
    for change in 0..7 {
        let (mut app, _) = fixture();
        open(&mut app);
        let owner = app.playlist_owner().unwrap();
        match change {
            0 => browse(&mut app, Browse::Query("smart".into())),
            1 => browse(&mut app, Browse::Close),
            2 => browse(&mut app, Browse::Edit),
            3 => app.window = Some(window::Id::unique()),
            4 => app.visibility_generation += 1,
            5 => {
                app.catalog_generation.fetch_add(1, Ordering::AcqRel);
            }
            _ => app.visible = false,
        }
        let _ = update(
            &mut app,
            Message::PlaylistBrowse(owner.clone(), Browse::Choose(Some("manual-42".into()))),
        );
        let _ = update(&mut app, Message::PlaylistEdit(owner, Edit::Create));
        assert!(app.browse_playlist_id.is_none(), "change {change}");
        assert_eq!(app.state.playlists.len(), 3);
    }
}

#[test]
fn pk05_escape_and_navigation_are_owned_and_missing_clicked_id_is_visible() {
    let (mut app, _) = fixture();
    open(&mut app);
    let selected = app.selected;
    for key in [
        keyboard::key::Named::ArrowRight,
        keyboard::key::Named::ArrowDown,
    ] {
        d3_key(&mut app, key);
    }
    assert_eq!(app.selected, selected);
    d3_key(&mut app, keyboard::key::Named::Escape);
    assert!(!app.playlist_panel_open && app.visible);
    open(&mut app);
    app.state.playlists.retain(|p| p.id != "manual-42");
    browse(&mut app, Browse::Choose(Some("manual-42".into())));
    assert!(app.browse_playlist_id.is_none() && app.browse_playlist_unavailable);
}

#[test]
fn pk05_actual_favourite_toggle_requeries_smart_browse_without_gallery_favourites_filter() {
    let (mut app, calls) = fixture();
    app.state.active_playlist_id = Some("manual-42".into());
    let PlaylistMembership::Smart { rule } = &mut app.state.playlists[1].membership else {
        panic!("smart fixture")
    };
    rule.favorites_only = true;
    app.state.favorites = ["item-0001".into(), "item-0003".into()].into();
    app.selected = Some(3);
    choose(&mut app, Some("smart-27"));
    assert!(!app.filter_rule.favorites_only);
    assert_eq!(app.filtered_indices, [3, 1]);
    let schedules = app.state.schedules.clone();
    let history = app.state.local_library.usage.clone();
    let timer = (
        app.next_slideshow_unix,
        app.automation_generation.load(Ordering::Acquire),
    );
    let _ = update(&mut app, Message::ToggleFavorite);
    assert!(!app.state.favorites.contains("item-0003"));
    assert_eq!(app.filtered_indices, [1]);
    assert_eq!(app.selected, Some(1));
    assert_eq!(app.browse_playlist_id.as_deref(), Some("smart-27"));
    assert_eq!(app.state.active_playlist_id.as_deref(), Some("manual-42"));
    assert!(!CandidateState::load(&app.state_path)
        .favorites
        .contains("item-0003"));
    let _ = update(&mut app, Message::ToggleFavorite);
    assert!(app.filtered_indices.is_empty());
    assert_eq!(app.browse_playlist_id.as_deref(), Some("smart-27"));
    assert_eq!(app.state.schedules, schedules);
    assert_eq!(app.state.local_library.usage, history);
    assert_eq!(
        (
            app.next_slideshow_unix,
            app.automation_generation.load(Ordering::Acquire)
        ),
        timer
    );
    assert_eq!(calls.load(Ordering::Acquire), 0);
}

#[test]
#[ignore = "PK05 actual production chooser/routes; explicit invocation"]
fn pk05_production_compositions_and_public_routes() {
    use iced::advanced::{
        clipboard, layout,
        renderer::{Headless, Renderer as _},
        widget::{operation, Operation, Tree},
        Layout, Shell,
    };
    use view::settings::folio::intent::{Message as Intent, SettingsMsg};
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

    fn save(
        element: &mut Element<'_, Message>,
        tree: &mut Tree,
        node: &layout::Node,
        renderer: &mut iced::Renderer,
        viewport: iced::Rectangle,
        cursor: iced::mouse::Cursor,
        path: &std::path::Path,
    ) {
        // Settle the shared Folio selected-state paint before taking a static frame.
        let now = iced::time::Instant::now();
        for offset in [0, 1_000] {
            let mut messages = Vec::new();
            element.as_widget_mut().update(
                tree,
                &iced::Event::Window(window::Event::RedrawRequested(
                    now + std::time::Duration::from_millis(offset),
                )),
                Layout::new(node),
                cursor,
                renderer,
                &mut clipboard::Null,
                &mut Shell::new(&mut messages),
                &viewport,
            );
        }
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
    for scenario in ["default", "empty", "minimum"] {
        let path = output
            .parent()
            .unwrap()
            .join("candidate/profile-seeds")
            .join(scenario)
            .join("candidate-state.toml");
        let seed: CandidateState = toml::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(
            seed.playlists.len(),
            if scenario == "empty" { 0 } else { 40 }
        );
        assert!(
            !seed.automation_intent().random_enabled
                && !seed.automation_intent().playlist_playing
                && seed.schedules.is_empty()
        );
    }

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
    }
    fn rail_point(app: &GpuCandidate, intent: donor_rail::RailIntent) -> iced::Point {
        let model = gallery_rail_model(app);
        let gh = match app.gallery_mode {
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
                    donor_layout::HexShape::Hexagon,
                )
            }
        };
        let at = view::gallery_rail_anchor(app, &model, gh);
        let local = donor_rail::intent_center(&model, intent).unwrap();
        iced::Point::new(at[0] + local.x, at[1] + local.y)
    }
    fn root_point(
        app: &GpuCandidate,
        renderer: &iced::Renderer,
        point: iced::Point,
    ) -> Vec<Message> {
        let size = iced::Size::new(app.viewport[0], app.viewport[1]);
        let bounds = iced::Rectangle::with_size(size);
        let mut element = view(app, app.window.unwrap());
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, renderer, &layout::Limits::new(size, size));
        click(&mut element, &mut tree, &node, renderer, bounds, point)
    }
    fn root_label(
        app: &GpuCandidate,
        renderer: &iced::Renderer,
        label: &str,
        accepts: impl Fn(&Message) -> bool,
    ) -> Vec<Message> {
        let size = iced::Size::new(app.viewport[0], app.viewport[1]);
        let bounds = iced::Rectangle::with_size(size);
        let mut element = view(app, app.window.unwrap());
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, renderer, &layout::Limits::new(size, size));
        for _ in 0..20 {
            let p = probe(&mut element, &mut tree, &node, renderer);
            let mut scroll_target = None;
            for (_, r) in p.texts.iter().rev().filter(|(t, _)| t == label) {
                let point = r.center();
                if bounds.contains(point) {
                    let messages = click(&mut element, &mut tree, &node, renderer, bounds, point);
                    if messages.iter().any(&accepts) {
                        return messages;
                    }
                }
                // The index and legacy sheet keep clipped children in the layout tree.
                // Wheel inside the owning viewport to bring the actual action into view.
                if let Some((_, pane, _, _)) =
                    p.scrolls.iter().rev().find(|(_, pane, content, _)| {
                        content.height > pane.height + 1.0
                            && bounds.contains(pane.center())
                            && (pane.x..pane.x + pane.width).contains(&point.x)
                            && (point.y < pane.y || point.y > pane.y + pane.height)
                    })
                {
                    let direction = if point.y < pane.y { 1.0 } else { -1.0 };
                    scroll_target = Some((pane.center(), direction * pane.height * 0.55));
                }
            }
            let Some((point, dy)) = scroll_target else {
                panic!(
                    "No mounted hit for {label}; labels: {:?}; scrolls: {:?}",
                    p.texts, p.scrolls
                );
            };
            let _ = send(
                &mut element,
                &mut tree,
                &node,
                renderer,
                bounds,
                point,
                iced::mouse::Event::WheelScrolled {
                    delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: dy },
                },
            );
        }
        panic!("No mounted hit for {label} after scrolling");
    }

    fn is_manager(message: &Message) -> bool {
        match message {
            Message::TogglePlaylistPanel => true,
            Message::SettingsLegacy { message, .. } => is_manager(message),
            _ => false,
        }
    }
    fn snapshot(app: &GpuCandidate, renderer: &mut iced::Renderer, path: &std::path::Path) {
        let size = iced::Size::new(app.viewport[0], app.viewport[1]);
        let bounds = iced::Rectangle::with_size(size);
        let mut element = view(app, app.window.unwrap());
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, renderer, &layout::Limits::new(size, size));
        save(
            &mut element,
            &mut tree,
            &node,
            renderer,
            bounds,
            iced::mouse::Cursor::Unavailable,
            path,
        );
    }
    let mut index = Vec::new();
    let mut cases = Vec::new();
    for mode in [
        GalleryMode::Slices,
        GalleryMode::Geometric,
        GalleryMode::Wall,
        GalleryMode::Depth,
    ] {
        for scale in [1.0, 0.5] {
            cases.push((
                format!("{mode:?}-{scale}"),
                1920.0,
                1080.0,
                scale,
                mode,
                3,
                "en-US",
                false,
                false,
            ));
        }
    }
    cases.extend([
        (
            "many".into(),
            900.0,
            480.0,
            1.0,
            GalleryMode::Wall,
            40,
            "en-US",
            false,
            false,
        ),
        (
            "many-min".into(),
            640.0,
            512.0,
            0.5,
            GalleryMode::Geometric,
            40,
            "en-US",
            false,
            false,
        ),
        (
            "empty".into(),
            1920.0,
            1080.0,
            1.0,
            GalleryMode::Slices,
            0,
            "en-US",
            true,
            false,
        ),
        (
            "no-selection".into(),
            900.0,
            720.0,
            1.0,
            GalleryMode::Depth,
            3,
            "en-US",
            true,
            false,
        ),
        (
            "no-match".into(),
            640.0,
            512.0,
            0.5,
            GalleryMode::Wall,
            3,
            "en-US",
            true,
            true,
        ),
        (
            "cjk-narrow".into(),
            320.0,
            512.0,
            1.0,
            GalleryMode::Slices,
            12,
            "zh-CN",
            false,
            false,
        ),
        (
            "cjk-min".into(),
            2560.0,
            1440.0,
            0.5,
            GalleryMode::Depth,
            12,
            "zh-CN",
            false,
            false,
        ),
    ]);
    for (name, width, height, scale, mode, count, locale, no_selection, no_match) in cases {
        let (mut app, calls) = fixture();
        app.viewport = [width, height];
        app.gallery_mode = mode;
        app.state.gallery.mode = match mode {
            GalleryMode::Slices => GalleryKind::Slices,
            GalleryMode::Geometric => GalleryKind::Geometric,
            GalleryMode::Wall => GalleryKind::Wall,
            GalleryMode::Depth => GalleryKind::Depth,
        };
        app.settings_locale = localization::Catalog::new(locale, locale).unwrap();
        app.config.ui.show_top_bar = true;
        app.state.frontend_preferences.filter_bar.always_visible = Some(true);
        app.state.frontend_preferences.interface.ui_scale = Some(scale);
        if no_selection {
            app.selected = None;
        }
        if count == 0 {
            app.state.playlists.clear();
        }
        if count > 3 {
            for i in 3..count {
                app.state.playlists.push(Playlist {
                    id: format!("saved-{i:02}"),
                    name: format!("保存したプレイリスト {i} / 音楽 and a long descriptive title"),
                    order: PlaybackOrder::Sequential,
                    membership: PlaylistMembership::Explicit {
                        wallpaper_ids: Vec::new(),
                    },
                });
            }
        }
        app.state.save(&app.state_path).unwrap();
        let state = app.state.clone();
        let bytes = std::fs::read(&app.state_path).unwrap();
        let timer = (
            app.automation_generation.load(Ordering::Acquire),
            app.next_slideshow_unix,
            app.last_automation_minute,
        );
        let messages = root_point(
            &app,
            &renderer,
            rail_point(&app, donor_rail::RailIntent::TogglePlaylists),
        );
        assert!(messages.iter().any(|m| matches!(
            m,
            Message::DonorRail(donor_rail::RailIntent::TogglePlaylists)
        )));
        for m in messages {
            let _ = update(&mut app, m);
        }
        assert!(app.playlist_chooser && app.playlist_panel_open);
        if no_match {
            browse(&mut app, Browse::Query("missing-query-42".into()));
        }
        let size = iced::Size::new(width, height);
        let bounds = iced::Rectangle::with_size(size);
        let mut element = view(&app, app.window.unwrap());
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let p = probe(&mut element, &mut tree, &node, &renderer);
        let panel = p
            .containers
            .iter()
            .find(|(id, _)| {
                id.as_ref()
                    .is_some_and(|id| id == &iced::widget::Id::new("playlist-browse-panel"))
            })
            .expect("panel")
            .1;
        assert!((panel.width - (440.0 * scale as f32).min(width - 32.0)).abs() < 0.1);
        assert!((panel.height - (480.0 * scale as f32).min(height - 32.0)).abs() < 0.1);
        let edit = p
            .texts
            .iter()
            .rev()
            .find(|(t, _)| t == app.settings_locale.text_static("playlists-edit"))
            .unwrap()
            .1;
        assert!(
            panel.contains(edit.position()) && edit.y + edit.height <= panel.y + panel.height + 0.5,
            "{name} footer {edit:?}/{panel:?}"
        );
        let input = p.inputs.last().copied().expect("chooser query");
        assert!(
            input.y >= panel.y && input.y + input.height <= edit.y,
            "{name} query bounds"
        );
        let scroll = p
            .scrolls
            .iter()
            .find(|(id, _, _, _)| {
                id.as_ref()
                    .is_some_and(|id| id == &iced::widget::Id::new("playlist-browse-lists"))
            })
            .expect("list scroll");
        assert!(
            scroll.1.height > 0.0 && scroll.1.y + scroll.1.height <= edit.y + 0.5,
            "{name} scrolling/footer"
        );
        let path = output.join(format!("chooser-{name}-top.png"));
        save(
            &mut element,
            &mut tree,
            &node,
            &mut renderer,
            bounds,
            iced::mouse::Cursor::Unavailable,
            &path,
        );
        let footer_before = edit;
        let mut last_hit = false;
        let mut row_hits = Vec::new();
        let mut visited = std::collections::BTreeSet::new();
        let step = (scroll.1.height * 0.65).max(1.0);
        let steps = ((scroll.2.height / step).ceil() as usize + 3).min(120);
        for _ in 0..steps {
            let p = probe(&mut element, &mut tree, &node, &renderer);
            for (t, r) in &p.texts {
                if !scroll.1.contains(r.center()) {
                    continue;
                }
                let messages = click(
                    &mut element,
                    &mut tree,
                    &node,
                    &renderer,
                    bounds,
                    r.center(),
                );
                for m in messages {
                    if let Message::PlaylistBrowse(_, Browse::Choose(Some(id))) = m {
                        visited.insert(id.clone());
                        row_hits.push((t.clone(), id.clone()));
                        if app.state.playlists.last().is_some_and(|p| p.id == id) {
                            last_hit = true;
                        }
                    }
                }
            }
            if count > 3 {
                let _ = send(
                    &mut element,
                    &mut tree,
                    &node,
                    &renderer,
                    bounds,
                    scroll.1.center(),
                    iced::mouse::Event::WheelScrolled {
                        delta: iced::mouse::ScrollDelta::Pixels { x: 0.0, y: -step },
                    },
                );
            }
        }
        let p_end = probe(&mut element, &mut tree, &node, &renderer);
        let footer_end = p_end
            .texts
            .iter()
            .rev()
            .find(|(t, _)| t == app.settings_locale.text_static("playlists-edit"))
            .unwrap()
            .1;
        assert_eq!(footer_before, footer_end, "fixed footer");
        if count > 3 {
            assert!(last_hit, "{name} final row reachable: {visited:?}");
            assert_eq!(
                visited.len(),
                app.state.playlists.len(),
                "{name} every row hit"
            );
            let end = output.join(format!("chooser-{name}-end.png"));
            save(
                &mut element,
                &mut tree,
                &node,
                &mut renderer,
                bounds,
                iced::mouse::Cursor::Unavailable,
                &end,
            );
            index.push(serde_json::json!({"path":end,"case":name,"position":"end","mounted_rail":true,"row_hits":row_hits,"fixed_footer":true}));
        }
        let close = p_end.texts.iter().rev().find(|(t, _)| t == "×").unwrap().1;
        let closed = click(
            &mut element,
            &mut tree,
            &node,
            &renderer,
            bounds,
            close.center(),
        );
        assert!(
            closed
                .iter()
                .any(|m| matches!(m, Message::PlaylistBrowse(_, Browse::Close))),
            "close hit {name}"
        );
        let edited = click(
            &mut element,
            &mut tree,
            &node,
            &renderer,
            bounds,
            footer_end.center(),
        );
        assert!(
            edited
                .iter()
                .any(|m| matches!(m, Message::PlaylistBrowse(_, Browse::Edit))),
            "footer hit {name}"
        );
        drop(element);
        for m in edited {
            let _ = update(&mut app, m);
        }
        assert!(app.playlist_panel_open && !app.playlist_chooser && !app.playlist_picker);
        assert_eq!(app.state, state);
        assert_eq!(std::fs::read(&app.state_path).unwrap(), bytes);
        assert_eq!(
            (
                app.automation_generation.load(Ordering::Acquire),
                app.next_slideshow_unix,
                app.last_automation_minute
            ),
            timer
        );
        assert_eq!(calls.load(Ordering::Acquire), 0);
        index.push(serde_json::json!({"path":path,"case":name,"viewport":[width,height],"scale":scale,"panel":[panel.x,panel.y,panel.width,panel.height],"mode":format!("{mode:?}"),"mounted_rail":true,"close_hit":true,"edit_hit":true,"rows_hit":visited.len(),"no_publication":true}));
    }
    {
        let (mut app, calls) = fixture();
        open(&mut app);
        let size = iced::Size::new(app.viewport[0], app.viewport[1]);
        let bounds = iced::Rectangle::with_size(size);
        let mut element = view(&app, app.window.unwrap());
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        // Drive the same chained first-focusable operation that the runtime consumes.
        let mut focus: Box<dyn Operation> = Box::new(operation::focusable::focus_next::<()>());
        loop {
            element.as_widget_mut().operate(
                &mut tree,
                Layout::new(&node),
                &renderer,
                focus.as_mut(),
            );
            match focus.finish() {
                operation::Outcome::Chain(next) => focus = next,
                _ => break,
            }
        }
        drop(element);
        let mut expected = String::new();
        for key in "manual-42".chars() {
            expected.push(key);
            let mut element = view(&app, app.window.unwrap());
            tree.diff(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            let key = key.to_string();
            let mut typed = Vec::new();
            element.as_widget_mut().update(
                &mut tree,
                &iced::Event::Keyboard(keyboard::Event::KeyPressed {
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
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &renderer,
                &mut clipboard::Null,
                &mut Shell::new(&mut typed),
                &bounds,
            );
            assert!(typed.iter().any(|m|matches!(m,Message::PlaylistBrowse(_,Browse::Query(value)) if value==&expected)),"mounted autofocus typing: {typed:?}");
            drop(element);
            for m in typed {
                let _ = update(&mut app, m);
            }
            assert_eq!(app.playlist_query, expected);
        }
        assert_eq!(app.playlist_query, "manual-42");
        let query = output.join("chooser-query-id.png");
        snapshot(&app, &mut renderer, &query);
        let label = format!(
            "{} · {}",
            app.state.playlists[0].name, app.state.playlists[0].id
        );
        let messages = root_label(
            &app,
            &renderer,
            &label,
            |m| matches!(m,Message::PlaylistBrowse(_,Browse::Choose(Some(id))) if id=="manual-42"),
        );
        for m in messages {
            let _ = update(&mut app, m);
        }
        assert_eq!(app.browse_playlist_id.as_deref(), Some("manual-42"));
        assert!(!app.playlist_panel_open);
        open(&mut app);
        let label = app.settings_locale.text_static("playlists-browse-all");
        let messages = root_label(&app, &renderer, label, |m| {
            matches!(m, Message::PlaylistBrowse(_, Browse::Choose(None)))
        });
        for m in messages {
            let _ = update(&mut app, m);
        }
        assert!(app.browse_playlist_id.is_none());
        open(&mut app);
        let messages = root_point(&app, &renderer, iced::Point::new(1.0, 1.0));
        assert!(messages
            .iter()
            .any(|m| matches!(m, Message::PlaylistBrowse(_, Browse::Close))));
        for m in messages {
            let _ = update(&mut app, m);
        }
        assert!(!app.playlist_panel_open && app.visible);
        assert_eq!(calls.load(Ordering::Acquire), 0);
        index.push(serde_json::json!({"path":query,"route":"mounted first-focusable operation -> actual query typing -> manual row -> All row -> outside click","focus":"headless widget operation, not native","actual_hits":true}));
    }
    {
        let (mut app, _) = fixture();
        app.viewport = [900.0, 720.0];
        let _ = update(&mut app, Message::SearchChanged("Wallpaper".into()));
        choose(&mut app, Some("manual-42"));
        let _ = update(&mut app, Message::TogglePlaylistPanel);
        let owner = app.playlist_owner().unwrap();
        let _ = update(
            &mut app,
            Message::PlaylistEdit(owner, Edit::DeleteRequest("manual-42".into())),
        );
        let owner = app.playlist_owner().unwrap();
        let _ = update(&mut app, Message::PlaylistEdit(owner, Edit::DeleteConfirm));
        assert!(!app.state.playlists.iter().any(|p| p.id == "manual-42"));
        assert_eq!(app.state.active_playlist_id.as_deref(), Some("smart-27"));
        let _ = update(&mut app, Message::TogglePlaylistPanel);
        assert!(app.browse_playlist_id.is_none());
        assert_eq!(app.filter_rule.text, "Wallpaper");
        let size = iced::Size::new(app.viewport[0], app.viewport[1]);
        let mut element = view(&app, app.window.unwrap());
        let mut tree = Tree::new(&element);
        let node =
            element
                .as_widget_mut()
                .layout(&mut tree, &renderer, &layout::Limits::new(size, size));
        let layout = probe(&mut element, &mut tree, &node, &renderer);
        let notice = app
            .settings_locale
            .text_static("playlist-browse-unavailable");
        let viewport = iced::Rectangle::with_size(size);
        assert!(layout.texts.iter().any(|(label, r)| label == notice
            && r.width > 0.0
            && r.height > 0.0
            && viewport.contains(r.center())));
        let unavailable = output.join("chooser-unavailable-notice.png");
        snapshot(&app, &mut renderer, &unavailable);
        index.push(serde_json::json!({"path":unavailable,"route":"confirmed manager deletion -> refresh clears browse and renders notice","retained_filter":"Wallpaper"}));
    }
    // Public Settings -> Playback -> Windows controls -> existing manager, exact wrapped hit messages.
    for (width, height, scale) in [
        (1920.0, 1080.0, 1.0),
        (2560.0, 1440.0, 0.5),
        (900.0, 720.0, 1.0),
    ] {
        let (mut app, calls) = fixture();
        app.viewport = [width, height];
        app.gallery_mode = GalleryMode::Slices;
        app.config.ui.show_top_bar = true;
        app.state.frontend_preferences.filter_bar.always_visible = Some(true);
        app.state.frontend_preferences.interface.ui_scale = Some(scale);
        let messages = root_point(
            &app,
            &renderer,
            rail_point(&app, donor_rail::RailIntent::ToggleSettings),
        );
        for m in messages {
            let _ = update(&mut app, m);
        }
        assert!(app.panel_state.settings_open);
        app.settings_workbench.folio_entrance = None;
        app.settings_workbench.tab_reveal = None;
        let label = app.settings_locale.text_static("settings-tab-playback");
        let messages = root_label(
            &app,
            &renderer,
            label,
            |m| matches!(m,Message::SettingsIntent{intent:Intent::Settings(SettingsMsg::SelectTab(tab)),..} if tab=="playback"),
        );
        for m in messages {
            let _ = update(&mut app, m);
        }
        app.settings_workbench.folio_entrance = None;
        app.settings_workbench.tab_reveal = None;
        let label = app.settings_locale.text_static("r5-windows-controls");
        let messages = root_label(&app, &renderer, label, |m| {
            matches!(
                m,
                Message::SettingsWindowsOpen {
                    category: SettingsCategory::Playback,
                    ..
                }
            )
        });
        for m in messages {
            let _ = update(&mut app, m);
        }
        assert_eq!(
            app.settings_workbench.windows_panel,
            Some(SettingsCategory::Playback)
        );
        let entry = output.join(format!("settings-playback-entry-{width}-{scale}.png"));
        snapshot(&app, &mut renderer, &entry);
        let label = app.settings_locale.text_static("r5-playlists");
        let messages = root_label(&app, &renderer, label, is_manager);
        for m in messages {
            let _ = update(&mut app, m);
        }
        assert!(app.playlist_panel_open && !app.playlist_picker && !app.playlist_chooser);
        let manager = output.join(format!("settings-manager-{width}-{scale}.png"));
        snapshot(&app, &mut renderer, &manager);
        index.push(serde_json::json!({"path":entry,"route":"rail Settings -> Playback tab -> Windows controls -> wrapped Playlists","manager_path":manager,"actual_hits":true,"scale":scale}));
        assert_eq!(calls.load(Ordering::Acquire), 0);
    }
    // Real detail action -> captured membership picker; no chooser predicate mutation.
    for scale in [1.0, 0.5] {
        let (mut app, calls) = fixture();
        app.viewport = [1920.0, 1080.0];
        app.gallery_mode = GalleryMode::Geometric;
        app.state.frontend_preferences.interface.ui_scale = Some(scale);
        app.selected = Some(1);
        choose(&mut app, Some("manual-42"));
        assert_eq!(app.selected, Some(1));
        // The fixture has no UI frame delivery. Settle the incoming catalogue
        // roll before opening the detail owner, as required for a hit-ready card.
        app.ordinary_roll = None;
        app.deterministic_animation = true;
        app.start_detail(true, "pk05-headless-detail");
        app.detail_progress = 1.0;
        assert!(
            app.detail_source.is_some(),
            "detail must capture presented card geometry"
        );
        assert!(view::detail_controls_active(&app));
        // Detail actions are painted and hit-tested by the back canvas.
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
        let action = actions
            .iter()
            .position(|a| a.intent == donor_back_layout::DetailIntent::Playlists)
            .unwrap();
        let center = detail.actions[action].center();
        let [x, y] = card.point([center.x, center.y]);
        let messages = root_point(&app, &renderer, iced::Point::new(x, y));
        assert!(messages.iter().any(|m| matches!(
            m, Message::DonorDetail(target, donor_back_layout::DetailIntent::Playlists) if target == &id
        )), "actual detail canvas hit {scale}: {messages:?}");
        for m in messages {
            let _ = update(&mut app, m);
        }
        assert!(app.playlist_picker && app.playlist_panel_open && !app.playlist_chooser);
        let path = output.join(format!("detail-captured-membership-{scale}.png"));
        snapshot(&app, &mut renderer, &path);
        let owner = app.playlist_owner().unwrap();
        assert_eq!(app.browse_playlist_id.as_deref(), Some("manual-42"));
        app.selected = Some(3);
        let saved = app.state.clone();
        let _ = update(
            &mut app,
            Message::PlaylistEdit(
                owner,
                Edit::Membership {
                    playlist: "manual-42".into(),
                    included: false,
                },
            ),
        );
        assert_eq!(app.state, saved);
        assert_eq!(calls.load(Ordering::Acquire), 0);
        index.push(serde_json::json!({"path":path,"route":"actual detail Playlists -> captured membership picker","scale":scale,"stale_capture_refused":true}));
    }
    std::fs::write(
        output.join("PK05-CAPTURE-INDEX.json"),
        serde_json::to_vec_pretty(&index).unwrap(),
    )
    .unwrap();
}

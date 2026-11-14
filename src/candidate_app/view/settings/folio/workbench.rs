use crate::donor_folio::source_ui::flow;

use std::collections::{HashMap, HashSet};

use iced::widget::canvas::{self, Frame, Path, Stroke};
use iced::widget::{
    button, column, container, image, mouse_area, scrollable, stack, text, text_input,
};
use iced::{Alignment, Background, Color, Element, Length, Padding, Point, Rectangle};

use crate::candidate_app::view::settings::composed::SettingsSource;
use crate::candidate_app::view::settings::folio::intent::Message;
use crate::donor_folio::source_ui::motion::{MotionProfile, Tween};
use crate::donor_folio::source_ui::with_alpha;
use crate::donor_folio::source_ui::{
    folio_horizontal_rule, label, legible_type_scale, sentence_case, FOLIO_INDEX_WIDTH,
    FOLIO_RULE_ALPHA, TYPE_SMALL,
};
use crate::donor_ui::Palette;

use super::control;
use crate::candidate_app::localization::Catalog;
use crate::candidate_app::view::settings::composed::search::SettingsSearchResult;
use crate::candidate_app::view::settings::composed::{
    build_tab_with_runtime_status, visible_tabs, ActionId, Card, Control, Row, SettingsFocus,
};
use crate::candidate_app::view::settings::folio::intent::SettingsMsg;

const GLYPH_FADE: f32 = 1.0;

fn category_note<'a>(catalog: &Catalog, tab: &str) -> &'static str {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    tr(match tab {
        "picker" => "settings-tab-note-picker",
        "filter" => "settings-tab-note-filter",
        "position" => "settings-tab-note-position",
        "displays" => "settings-tab-note-displays",
        "motion" => "settings-tab-note-motion",
        "playback" => "settings-tab-note-playback",
        "performance" => "settings-tab-note-performance",
        "library" => "settings-tab-note-library",
        "sources" => "settings-tab-note-sources",
        "automation" => "settings-tab-note-automation",
        "theme" => "settings-tab-note-theme",
        "integrations" => "settings-tab-note-integrations",
        "language" => "settings-tab-note-language",
        _ => "settings-tab-note-default",
    })
}

pub struct SourceCtx<'a, 's> {
    pub config: &'s dyn SettingsSource,
    pub values: &'a HashMap<String, String>,
    pub themes: &'a [String],
    pub folders: &'a [String],
    pub backends: &'a [String],
    pub outputs: &'a [String],
    pub output_statuses:
        &'a [crate::candidate_app::view::settings::composed::status::OutputStatus],
    pub output_previews: &'a HashMap<String, String>,
    pub library_watch:
        Option<&'a crate::candidate_app::view::settings::composed::status::LibraryWatchStatus>,
    pub app_themes:
        Option<&'a crate::candidate_app::view::settings::composed::status::AppThemesResult>,
    pub playback:
        Option<&'a crate::candidate_app::view::settings::composed::status::PlaybackStatus>,
    pub thumbnail_task:
        Option<&'a crate::candidate_app::view::settings::composed::status::TaskStatus>,
    pub analysis: String,
}

impl SourceCtx<'_, '_> {
    fn motion(&self) -> MotionProfile {
        MotionProfile::new(
            self.config.motion_fast_ms(),
            self.config.motion_standard_ms(),
            self.config.motion_slow_ms(),
        )
    }

    fn cards(&self, tab: &str) -> Vec<(Card, Vec<Row>)> {
        let catalog = self.config.catalog();
        let tr = |key| catalog.text_static(key);
        let mut cards = build_tab_with_runtime_status(
            tab,
            self.config,
            self.themes,
            self.folders,
            &self.analysis,
            self.backends,
            self.outputs,
            self.output_statuses,
            self.output_previews,
            self.library_watch,
            self.playback,
            self.app_themes,
        );
        if let Some(task) = self.thumbnail_task {
            for row in cards.iter_mut().flat_map(|(_, rows)| rows) {
                if let crate::candidate_app::view::settings::composed::Control::ActionBtn {
                    id: ActionId::CaptureWeThumbnails,
                    label,
                } = &mut row.control
                {
                    let state = match &task.state {
                        crate::candidate_app::view::settings::composed::status::TaskState::Running => {
                            tr("filter-bar-state-running")
                        }
                        crate::candidate_app::view::settings::composed::status::TaskState::Paused => {
                            tr("filter-bar-state-paused")
                        }
                        crate::candidate_app::view::settings::composed::status::TaskState::Completed => {
                            tr("filter-bar-state-done")
                        }
                        crate::candidate_app::view::settings::composed::status::TaskState::Failed => {
                            tr("filter-bar-state-failed")
                        }
                        crate::candidate_app::view::settings::composed::status::TaskState::Cancelled => {
                            tr("filter-bar-state-stopped")
                        }
                        crate::candidate_app::view::settings::composed::status::TaskState::Other(state) => state.as_str(),
                    };
                    row.desc = format!(
                        "{state} · {} / {} · {}",
                        task.progress, task.total, task.detail
                    );
                    if task.state.is_active() {
                        *label = tr("settings-performance-capture-we-stop").into();
                    }
                }
            }
        }
        cards
    }
}

#[derive(Clone, Copy)]
pub struct FocusCtx<'a> {
    pub keyboard_focus: SettingsFocus,
    pub focused_control: usize,
    pub focused_choice: Option<usize>,
    pub expanded_details: &'a HashSet<String>,
    pub bar_reveals: &'a HashMap<String, Tween>,
    pub active_input: Option<&'a str>,
    pub armed: Option<ActionId>,
}

impl FocusCtx<'_> {
    fn on(self, area: SettingsFocus, index: usize) -> (bool, Option<usize>) {
        let focused = self.keyboard_focus == area && self.focused_control == index;
        (focused, if focused { self.focused_choice } else { None })
    }

    fn bar(self, id: &str) -> (bool, f32) {
        self.bar_reveals
            .get(id)
            .map_or((false, 0.0), |reveal| (reveal.target > 0.5, reveal.x))
    }
}

#[derive(Clone, Copy)]
pub struct ChromeCtx<'a> {
    pub viewport: (f32, f32),
    pub scale: f32,
    pub entrance: f32,
    pub palette: &'a Palette,
}

#[derive(Clone)]
pub struct KeybindCaptureView {
    pub title: String,
    pub binding: String,
    pub edited: bool,
    pub conflict: Option<String>,
}

#[derive(Clone, Default)]
pub struct ProcessPickerView {
    pub targets: Vec<String>,
    pub running: Vec<String>,
    pub query: String,
    pub manual: String,
    pub scroll: f32,
}

pub struct WorkbenchInput<'a, 's> {
    pub source: SourceCtx<'a, 's>,
    pub focus: FocusCtx<'a>,
    pub chrome: ChromeCtx<'a>,
    pub preview_image: Option<&'a image::Handle>,
    pub tab_transition: f32,
    pub tab: &'a str,
    pub selected_section: usize,
    pub search_open: bool,
    pub search_query: &'a str,
    pub search_results: &'a [SettingsSearchResult],
    pub keybind_capture: Option<KeybindCaptureView>,
    pub process_picker: Option<ProcessPickerView>,
    pub status: &'a str,
}

pub fn settings_workbench<'a>(input: WorkbenchInput<'a, '_>) -> Element<'a, Message> {
    let catalog = input.source.config.catalog();
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let WorkbenchInput {
        source,
        focus,
        chrome,
        preview_image,
        tab_transition,
        tab,
        selected_section,
        search_open,
        search_query,
        search_results,
        keybind_capture,
        process_picker,
        status,
    } = input;
    let ChromeCtx {
        viewport,
        scale,
        entrance,
        palette,
    } = chrome;
    let motion = source.motion();
    let cards = source.cards(tab);
    let selected_section = selected_section.min(cards.len().saturating_sub(1));
    let sections: Vec<(usize, &'static str)> = cards
        .iter()
        .enumerate()
        .map(|(index, (card, _))| (index, card.title))
        .collect();
    let (card, settings) = cards.into_iter().nth(selected_section).unwrap_or_else(|| {
        (
            Card {
                title: tr("settings-section-general"),
                subtitle: "",
            },
            Vec::new(),
        )
    });
    let has_selected_preset = settings.iter().any(|setting| {
        matches!(&setting.control, Control::Presets { items, .. } if items.iter().any(|entry| entry.selected))
    });
    let reveal = crate::donor_folio::source_ui::motion::smoothstep(entrance.clamp(0.0, 1.0));
    let branch_reveal =
        crate::donor_folio::source_ui::motion::smoothstep(tab_transition.clamp(0.0, 1.0));
    let (panel_width, panel_height) =
        crate::donor_folio::source_ui::folio_sheet_dims(viewport, scale);
    let tabs = visible_tabs(source.config);
    let active_tab = tabs.iter().position(|(key, _)| *key == tab).unwrap_or(0);

    let index = navigation(
        catalog,
        &tabs,
        active_tab,
        tab,
        &sections,
        selected_section,
        focus.keyboard_focus == SettingsFocus::Index,
        focus.keyboard_focus == SettingsFocus::Sections,
        scale,
        palette,
        GLYPH_FADE,
        branch_reveal,
        search_open,
        search_query,
        search_results,
    );
    let reading = reading_surface(
        catalog,
        ReadingInput {
            tab,
            stash_available: source.config.stash_available(),
            card,
            settings,
            values: source.values,
            has_selected_preset,
            available_width: panel_width - FOLIO_INDEX_WIDTH * scale.max(0.9),
            scale,
            palette,
            fade: GLYPH_FADE,
            motion,
        },
        focus,
    );
    let atmosphere: Element<'a, Message> = if let Some(preview) = preview_image {
        image(preview.clone())
            .width(Length::Fill)
            .height(Length::Fill)
            .content_fit(iced::ContentFit::Cover)
            .opacity(0.045 * reveal)
            .into()
    } else {
        iced::widget::Space::new()
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };
    let page = container(stack![
        atmosphere,
        iced::widget::canvas(Blueprint {
            rtl,
            palette: *palette,
            fade: reveal
        })
        .width(Length::Fill)
        .height(Length::Fill),
        column![reading, status_footer(status, scale, palette)].height(Length::Fill),
    ])
    .width(Length::Fill)
    .height(Length::Fill)
    .style(move |_| {
        crate::donor_folio::source_ui::bg_style(Background::Color(with_alpha(
            palette.surface,
            0.965 * reveal,
        )))
    });
    let body = flow![rtl;
        container(index).width(Length::Fixed(FOLIO_INDEX_WIDTH * scale.max(0.9))),
        container(page).width(Length::Fill),
    ]
    .spacing(0.0)
    .height(Length::Fill);
    let panel = container(body)
        .width(Length::Fixed(panel_width))
        .height(Length::Fixed(panel_height))
        .clip(true)
        .style(move |_| crate::donor_folio::source_ui::folio_sheet_panel_style(palette, reveal));
    let mut layers = stack![
        mouse_area(
            container(text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(move |_| crate::donor_folio::source_ui::folio_scrim_style(reveal)),
        )
        .on_press(Message::ToggleSettings)
        .on_scroll(|_| Message::Noop),
        container(crate::donor_folio::sheet_panel(
            iced::widget::opaque(panel),
            Message::Noop
        ))
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill),
    ];
    if let Some(capture) = keybind_capture {
        layers = layers.push(keybind_capture_layer(
            catalog, capture, scale, palette, reveal,
        ));
    }
    if let Some(picker) = process_picker {
        layers = layers.push(process_picker_layer(
            catalog, picker, scale, palette, reveal, status,
        ));
    }
    layers.into()
}

fn status_footer<'a>(status: &'a str, scale: f32, palette: &'a Palette) -> Element<'a, Message> {
    if status.is_empty() {
        return iced::widget::Space::new().height(0.0).into();
    }
    container(scrollable(
        text(status)
            .font(crate::donor_folio::source_ui::UI_FONT)
            .size(12.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
            .color(palette.surface_text)
            .width(Length::Fill),
    ))
    .height(Length::Fixed(60.0 * scale))
    .width(Length::Fill)
    .padding([8.0 * scale, 14.0 * scale])
    .style(move |_| {
        crate::donor_folio::source_ui::bg_style(Background::Color(palette.surface_container))
    })
    .into()
}

fn process_picker_layer<'a>(
    catalog: &Catalog,
    picker: ProcessPickerView,
    scale: f32,
    palette: &'a Palette,
    reveal: f32,
    status: &'a str,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let ProcessPickerView {
        targets,
        running,
        query,
        manual,
        scroll,
    } = picker;
    let rail_count = running.len().max(2);
    let rail_active = (scroll.clamp(0.0, 1.0) * (rail_count - 1) as f32).round() as usize;
    let target_rows: Element<'a, Message> = if targets.is_empty() {
        container(label(
            tr("settings-playback-targets-empty"),
            TYPE_SMALL,
            scale,
            with_alpha(palette.surface_text, 0.58),
        ))
        .width(Length::Fill)
        .padding([12.0 * scale, 14.0 * scale])
        .style(move |_| {
            crate::donor_folio::source_ui::box_style(
                with_alpha(palette.surface_container, 0.46),
                with_alpha(palette.outline, 0.3),
            )
        })
        .into()
    } else {
        targets
            .into_iter()
            .fold(column![].spacing(6.0 * scale), |rows, target| {
                let remove = crate::donor_folio::source_ui::folio_destructive_action(
                    rtl,
                    tr("settings-playback-target-remove"),
                    false,
                    Some(Message::Settings(SettingsMsg::ProcessPickerRemove(
                        target.clone(),
                    ))),
                    Length::Fixed(80.0 * scale),
                    scale * 0.76,
                    palette,
                );
                rows.push(
                    container(
                        flow![rtl;
                            label(
                                target,
                                14.0,
                                scale,
                                with_alpha(palette.surface_text, GLYPH_FADE)
                            ),
                            container(text("")).width(Length::Fill),
                            remove,
                        ]
                        .align_y(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .padding([8.0 * scale, 10.0 * scale])
                    .style(move |_| {
                        crate::donor_folio::source_ui::box_style(
                            with_alpha(palette.surface_container, 0.7),
                            with_alpha(palette.primary, 0.46),
                        )
                    }),
                )
            })
            .into()
    };
    let running_rows: Element<'a, Message> = if running.is_empty() {
        container(label(
            tr("settings-playback-running-empty"),
            TYPE_SMALL,
            scale,
            with_alpha(palette.surface_text, 0.58),
        ))
        .width(Length::Fill)
        .padding([12.0 * scale, 14.0 * scale])
        .style(move |_| {
            crate::donor_folio::source_ui::box_style(
                with_alpha(palette.surface_container, 0.46),
                with_alpha(palette.outline, 0.3),
            )
        })
        .into()
    } else {
        running
            .into_iter()
            .fold(column![].spacing(6.0 * scale), |rows, process| {
                let add = crate::donor_folio::source_ui::folio_action(
                    rtl,
                    tr("settings-playback-target-add"),
                    true,
                    Some(Message::Settings(SettingsMsg::ProcessPickerAdd(
                        process.clone(),
                    ))),
                    Length::Fixed(68.0 * scale),
                    scale * 0.76,
                    palette,
                );
                rows.push(
                    container(
                        flow![rtl;
                            label(
                                process,
                                14.0,
                                scale,
                                with_alpha(palette.surface_text, GLYPH_FADE)
                            ),
                            container(text("")).width(Length::Fill),
                            add,
                        ]
                        .align_y(Alignment::Center),
                    )
                    .width(Length::Fill)
                    .padding([8.0 * scale, 10.0 * scale])
                    .style(move |_| {
                        crate::donor_folio::source_ui::box_style(
                            with_alpha(palette.surface_container, 0.7),
                            with_alpha(palette.outline, 0.38),
                        )
                    }),
                )
            })
            .into()
    };
    let close = crate::donor_folio::source_ui::folio_action(
        rtl,
        tr("settings-playback-target-close"),
        true,
        Some(Message::Settings(SettingsMsg::ProcessPickerClose)),
        Length::Fixed(76.0 * scale),
        scale * 0.82,
        palette,
    );
    let body = column![
        flow![rtl;
            label(
                tr("settings-playback-target-dialog-title"),
                20.0,
                scale,
                with_alpha(palette.surface_text, GLYPH_FADE),
            ),
            container(text("")).width(Length::Fill),
            label(
                tr("settings-playback-process-enabled"),
                TYPE_SMALL,
                scale,
                with_alpha(palette.primary, GLYPH_FADE),
            ),
        ]
        .align_y(Alignment::Center),
        label(
            tr("settings-playback-target-dialog-desc"),
            TYPE_SMALL,
            scale,
            with_alpha(palette.surface_text, 0.65),
        )
        .line_height(iced::widget::text::LineHeight::Relative(1.35)),
        crate::donor_folio::source_ui::folio_horizontal_rule(with_alpha(palette.outline, 0.46)),
        label(
            tr("settings-playback-targets"),
            12.0,
            scale,
            with_alpha(palette.primary, GLYPH_FADE)
        ),
        target_rows,
        status_footer(status, scale, palette),
        label(
            tr("settings-playback-running"),
            12.0,
            scale,
            with_alpha(palette.primary, GLYPH_FADE)
        ),
        flow![rtl;
            label("⌕", 15.0, scale, with_alpha(palette.primary, GLYPH_FADE)),
            text_input(tr("browser-search-placeholder"), &query)
                .font(crate::donor_folio::source_ui::UI_FONT)
                .on_input(|query| Message::Settings(SettingsMsg::ProcessPickerSearch(query)))
                .size(12.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                .padding([8.0 * scale, 9.0 * scale])
                .width(Length::Fill)
                .style(move |_theme, _status| {
                    crate::donor_folio::source_ui::workbench_input_style(
                        palette.surface_text,
                        palette.primary,
                    )
                }),
        ]
        .spacing(8.0 * scale)
        .align_y(Alignment::Center),
        flow![rtl;
            text_input(tr("settings-playback-target-manual-placeholder"), &manual)
                .font(crate::donor_folio::source_ui::UI_FONT)
                .on_input(|process| Message::Settings(SettingsMsg::ProcessPickerManualInput(
                    process
                )))
                .on_submit(Message::Settings(SettingsMsg::ProcessPickerManualAdd))
                .size(12.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                .padding([8.0 * scale, 9.0 * scale])
                .width(Length::Fill)
                .style(move |_theme, _status| {
                    crate::donor_folio::source_ui::workbench_input_style(
                        palette.surface_text,
                        palette.primary,
                    )
                }),
            crate::donor_folio::source_ui::folio_action(rtl,
                tr("settings-playback-target-add"),
                true,
                Some(Message::Settings(SettingsMsg::ProcessPickerManualAdd)),
                Length::Fixed(68.0 * scale),
                scale * 0.76,
                palette,
            ),
        ]
        .spacing(8.0 * scale)
        .align_y(Alignment::Center),
        flow![rtl;
            scrollable(running_rows)
                .height(Length::Fixed(142.0 * scale))
                .width(Length::Fill)
                .direction(iced::widget::scrollable::Direction::Vertical(
                    iced::widget::scrollable::Scrollbar::new().width(0.0).scroller_width(0.0),
                ))
                .on_scroll(|viewport| {
                    Message::Settings(SettingsMsg::ProcessPickerScroll(
                        viewport.relative_offset().y,
                    ))
                }),
            iced::widget::canvas(IndexRail {
                active: rail_active,
                count: rail_count,
                notches: false,
                palette: *palette,
                fade: GLYPH_FADE,
            })
            .width(Length::Fixed(16.0 * scale))
            .height(Length::Fixed(142.0 * scale)),
        ]
        .spacing(8.0 * scale),
        crate::donor_folio::source_ui::folio_horizontal_rule(with_alpha(palette.outline, 0.46)),
        flow![rtl;container(text("")).width(Length::Fill), close].align_y(Alignment::Center),
    ]
    .spacing(12.0 * scale);
    let panel = mouse_area(
        container(body)
            .width(Length::Fixed(500.0 * scale.max(0.9)))
            .padding([20.0 * scale, 22.0 * scale])
            .style(move |_| {
                crate::donor_folio::source_ui::folio_sheet_panel_style(palette, reveal)
            }),
    )
    .on_press(Message::Noop)
    .on_scroll(|_| Message::Noop);
    mouse_area(
        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(move |_| crate::donor_folio::source_ui::folio_scrim_style(0.6 * reveal)),
    )
    .on_press(Message::Settings(SettingsMsg::ProcessPickerClose))
    .into()
}

fn keybind_capture_layer<'a>(
    catalog: &Catalog,
    capture: KeybindCaptureView,
    scale: f32,
    palette: &'a Palette,
    reveal: f32,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let KeybindCaptureView {
        title,
        binding,
        edited,
        conflict,
    } = capture;
    let action_scale = scale * 0.82;
    let action = |label_key: &'static str, active: bool, message: Message| {
        crate::donor_folio::source_ui::folio_action(
            rtl,
            tr(label_key),
            active,
            Some(message),
            Length::Fixed(crate::donor_folio::source_ui::folio_action_width(
                tr(label_key),
                action_scale,
            )),
            action_scale,
            palette,
        )
    };
    let pad = mouse_area(
        container(label(
            binding,
            22.0,
            scale,
            with_alpha(
                if edited {
                    palette.primary
                } else {
                    palette.surface_text
                },
                GLYPH_FADE,
            ),
        ))
        .width(Length::Fill)
        .center_x(Length::Fill)
        .padding([20.0 * scale, 12.0 * scale])
        .style(move |_| {
            crate::donor_folio::source_ui::box_style(
                with_alpha(palette.surface_container, 0.55),
                with_alpha(
                    if edited {
                        palette.primary
                    } else {
                        palette.outline
                    },
                    0.75,
                ),
            )
        }),
    )
    .on_press(Message::Settings(SettingsMsg::KeybindCaptureClick(
        crate::candidate_app::view::settings::composed::input::MouseButton::Left,
    )))
    .on_right_press(Message::Settings(SettingsMsg::KeybindCaptureClick(
        crate::candidate_app::view::settings::composed::input::MouseButton::Right,
    )))
    .on_middle_press(Message::Settings(SettingsMsg::KeybindCaptureClick(
        crate::candidate_app::view::settings::composed::input::MouseButton::Middle,
    )));
    let mut body = column![
        label(
            title,
            14.0,
            scale,
            with_alpha(palette.surface_text, GLYPH_FADE)
        ),
        pad,
        label(
            tr("settings-keybind-capture-hint"),
            TYPE_SMALL,
            scale,
            with_alpha(palette.surface_text, 0.58),
        )
        .line_height(iced::widget::text::LineHeight::Relative(1.35)),
    ]
    .spacing(10.0 * scale);
    if let Some(conflict) = conflict {
        body = body.push(label(
            conflict,
            TYPE_SMALL,
            scale,
            with_alpha(palette.tertiary, GLYPH_FADE),
        ));
    }
    body = body.push(
        flow![rtl;
            action("settings-keybind-capture-default", false, {
                Message::Settings(SettingsMsg::KeybindCaptureDefault)
            }),
            action("settings-keybind-capture-unbind", false, {
                Message::Settings(SettingsMsg::KeybindCaptureUnbind)
            }),
            container(text("")).width(Length::Fill),
            action("settings-keybind-capture-cancel", false, {
                Message::Settings(SettingsMsg::KeybindCaptureCancel)
            }),
            action("settings-keybind-capture-ok", true, {
                Message::Settings(SettingsMsg::KeybindCaptureApply)
            }),
        ]
        .spacing(8.0 * scale)
        .align_y(Alignment::Center),
    );
    let panel = mouse_area(
        container(body)
            .width(Length::Fixed(390.0 * scale.max(0.9)))
            .padding([18.0 * scale, 20.0 * scale])
            .style(move |_| {
                crate::donor_folio::source_ui::folio_sheet_panel_style(palette, reveal)
            }),
    )
    .on_press(Message::Noop);
    mouse_area(
        container(panel)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(move |_| crate::donor_folio::source_ui::folio_scrim_style(0.6 * reveal)),
    )
    .on_press(Message::Settings(SettingsMsg::KeybindCaptureCancel))
    .into()
}

pub fn picker_layout_workbench<'a>(
    source: &SourceCtx<'a, '_>,
    focus: FocusCtx<'a>,
    chrome: ChromeCtx<'a>,
) -> Element<'a, Message> {
    let catalog = source.config.catalog();
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let ChromeCtx {
        viewport,
        scale,
        entrance,
        palette,
    } = chrome;
    let motion = source.motion();
    let rows = source
        .cards("picker")
        .into_iter()
        .nth(super::super::composed::PICKER_LAYOUT_SECTION)
        .map_or_else(Vec::new, |(_, rows)| rows);
    let has_selected_preset = rows.iter().any(|row| {
        matches!(&row.control, Control::Presets { items, .. } if items.iter().any(|entry| entry.selected))
    });
    let reveal = crate::donor_folio::source_ui::motion::smoothstep(entrance.clamp(0.0, 1.0));
    let panel_width = crate::donor_folio::source_ui::picker_layout_studio_width(viewport.0, scale);
    let panel_height = (viewport.1 - 36.0 * scale).max(360.0);

    let mut pinned = column![].spacing(10.0 * scale);
    let mut controls = column![].spacing(13.0 * scale);
    for (index, setting) in rows.into_iter().enumerate() {
        let is_pinned = setting.title == tr("settings-selector-display-mode-label")
            || matches!(
                &setting.control,
                Control::TextField { key, .. } if key == crate::candidate_app::view::settings::composed::PRESET_NAME_KEY
            )
            || matches!(&setting.control, Control::Presets { .. });
        let field = picker_layout_field(
            catalog,
            setting,
            index,
            source.values,
            has_selected_preset,
            focus,
            panel_width - 42.0 * scale,
            scale,
            palette,
            GLYPH_FADE,
            motion,
        );
        if is_pinned {
            pinned = pinned.push(field);
        } else {
            controls = controls.push(field);
        }
    }

    let mode = tr(match source.config.display_mode().as_str() {
        "grid" | "wall" => "settings-selector-mode-wall",
        "hex" => "settings-selector-mode-hex",
        "sandy" | "nova" => "settings-selector-mode-sandy",
        "hand" => "settings-selector-mode-hand",
        "depth" => "settings-selector-mode-depth",
        "collection" => "settings-selector-mode-collection",
        _ => "settings-selector-mode-slices",
    });
    let page_actions = flow![rtl;
        crate::donor_folio::source_ui::folio_action(rtl,
            tr("settings-studio-back"),
            false,
            Some(Message::Settings(SettingsMsg::LeaveLayoutStudio)),
            Length::Fixed(104.0 * scale.max(1.0)),
            scale,
            palette,
        ),
        container(text("")).width(Length::Fill),
        crate::donor_folio::source_ui::folio_action(rtl,
            "×",
            false,
            Some(Message::ToggleSettings),
            Length::Shrink,
            scale,
            palette,
        ),
    ]
    .align_y(Alignment::Center);
    let intro = column![
        page_actions,
        label(
            tr_args!(catalog, "settings-studio-title", mode => mode),
            31.0,
            scale,
            with_alpha(palette.surface_text, GLYPH_FADE)
        ),
        label(
            tr("settings-studio-intro"),
            TYPE_SMALL,
            scale,
            with_alpha(palette.surface_text, 0.58),
        )
        .line_height(iced::widget::text::LineHeight::Relative(1.35)),
    ]
    .spacing(7.0 * scale);
    let scroll = scrollable(controls.padding(Padding {
        top: 1.0,
        right: 10.0 * scale,
        bottom: 20.0 * scale,
        left: 0.0,
    }))
    .direction(crate::donor_folio::source_ui::thin_vbar())
    .style(crate::donor_folio::source_ui::scroll_style(with_alpha(
        palette.outline,
        0.72,
    )))
    .height(Length::Fill);
    let page = column![intro, pinned, scroll]
        .spacing(15.0 * scale)
        .height(Length::Fill)
        .padding([17.0 * scale, 19.0 * scale]);
    let surface = stack![
        container(text(""))
            .width(Length::Fill)
            .height(Length::Fill)
            .style(move |_| {
                crate::donor_folio::source_ui::bg_style(Background::Color(with_alpha(
                    palette.surface,
                    0.985 * reveal,
                )))
            }),
        iced::widget::canvas(Blueprint {
            rtl,
            palette: *palette,
            fade: 0.55 * reveal
        })
        .width(Length::Fill)
        .height(Length::Fill),
        page,
    ]
    .width(Length::Fill)
    .height(Length::Fill);
    let panel = mouse_area(
        container(surface)
            .width(Length::Fixed(panel_width))
            .height(Length::Fixed(panel_height))
            .clip(true)
            .style(move |_| {
                let mut style = crate::donor_folio::source_ui::box_style(
                    with_alpha(palette.surface, 0.99 * reveal),
                    with_alpha(palette.outline, 0.72 * reveal),
                );
                style.shadow = iced::Shadow {
                    color: with_alpha(Color::BLACK, 0.4 * reveal),
                    offset: iced::Vector::new(8.0, 14.0),
                    blur_radius: 36.0,
                };
                style
            }),
    )
    .on_press(Message::Noop)
    .on_scroll(|_| Message::Noop);
    container(panel)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(crate::donor_folio::source_ui::direction::start_for(rtl))
        .align_y(iced::alignment::Vertical::Center)
        .padding(
            crate::donor_folio::source_ui::direction::logical_padding_for(
                rtl,
                0.0,
                0.0,
                0.0,
                18.0 * scale,
            ),
        )
        .into()
}

fn picker_layout_field<'a>(
    catalog: &Catalog,
    setting: Row,
    index: usize,
    values: &'a HashMap<String, String>,
    has_selected_preset: bool,
    focus: FocusCtx<'_>,
    available_width: f32,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    motion: MotionProfile,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    if let Control::AppTheme { app } = &setting.control {
        return super::app_theme::view(
            catalog,
            app.clone(),
            index,
            focus
                .expanded_details
                .contains(&format!("app-theme:{}", app.id)),
            focus.on(SettingsFocus::Controls, index).0,
            scale,
            palette,
        );
    }
    if setting.control.is_inline_editor() {
        let (focused, focused_choice) = focus.on(SettingsFocus::Controls, index);
        return compact_inline_field(
            catalog,
            setting,
            index,
            values,
            has_selected_preset,
            focused,
            focused_choice,
            available_width,
            scale,
            palette,
            fade,
            motion,
        );
    }
    if setting.control.is_group_editor() {
        return compact_group_field(
            catalog,
            setting,
            index,
            values,
            has_selected_preset,
            focus,
            available_width,
            scale,
            palette,
            fade,
            motion,
        );
    }
    let (focused, focused_choice) = focus.on(SettingsFocus::Controls, index);
    if setting.control.is_compact_action() {
        return compact_action_field(
            catalog,
            setting,
            index,
            focused,
            available_width,
            scale,
            palette,
            fade,
        );
    }
    let locked = focus
        .active_input
        .is_some_and(|key| match &setting.control {
            Control::Number {
                key: control_key, ..
            }
            | Control::TextField {
                key: control_key, ..
            }
            | Control::Resolution {
                key: control_key, ..
            } => control_key == key,
            _ => false,
        });
    let title = if setting.title.is_empty() {
        tr("settings-selector-saved-styles-label").to_string()
    } else {
        setting.title
    };
    let widget = control::widget(
        catalog,
        setting.control,
        values,
        has_selected_preset,
        focused,
        focused_choice,
        available_width,
        scale * 0.94,
        palette,
        fade,
        focus.armed,
        motion,
        Some(focus.bar_reveals),
    );
    let line = if focused || locked {
        palette.primary
    } else {
        palette.outline
    };
    mouse_area(
        container(
            column![
                container(text(""))
                    .width(Length::Fill)
                    .height(Length::Fixed(2.0))
                    .style(
                        move |_| crate::donor_folio::source_ui::bg_style(Background::Color(
                            with_alpha(line, if focused || locked { 0.9 } else { 0.42 },)
                        ))
                    ),
                label(title, 13.0, scale, with_alpha(palette.surface_text, fade)),
                label(
                    setting.desc,
                    TYPE_SMALL,
                    scale,
                    with_alpha(palette.surface_text, 0.54)
                )
                .line_height(iced::widget::text::LineHeight::Relative(1.3)),
                container(widget).padding(Padding {
                    top: 4.0 * scale,
                    ..Padding::ZERO
                }),
            ]
            .spacing(7.0 * scale),
        )
        .width(Length::Fill)
        .padding([0.0, 3.0 * scale]),
    )
    .on_press(Message::Settings(SettingsMsg::FocusControl(index)))
    .into()
}

fn navigation<'a>(
    catalog: &Catalog,
    tabs: &[(&'static str, &'static str)],
    active_tab: usize,
    selected_tab: &str,
    sections: &[(usize, &'static str)],
    selected_section: usize,
    index_focused: bool,
    section_focused: bool,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    branch_reveal: f32,
    search_open: bool,
    search_query: &'a str,
    search_results: &'a [SettingsSearchResult],
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let search = search_control(
        catalog,
        search_open,
        search_query,
        search_results,
        scale,
        palette,
        fade,
    );
    let content: Element<'a, Message> = if search_open {
        search_result_list(catalog, search_query, search_results, scale, palette, fade)
    } else {
        let mut tree = column![].spacing(1.0 * scale);
        for (key, label) in tabs.iter().copied() {
            let active = key == selected_tab;
            let category = button(
                flow![rtl;
                    text(if active {
                        "▾"
                    } else if rtl {
                        "◂"
                    } else {
                        "▸"
                    })
                    .font(crate::donor_folio::source_ui::UI_FONT)
                    .size(10.0 * legible_type_scale(scale)),
                    text(sentence_case(label))
                        .font(crate::donor_folio::source_ui::UI_FONT)
                        .size(12.5 * legible_type_scale(scale)),
                    container(text("")).width(Length::Fill),
                ]
                .spacing(9.0 * scale)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([8.0 * scale, 8.0 * scale])
            .on_press(Message::Settings(SettingsMsg::SelectTab(key.to_string())))
            .style(move |_theme, status| {
                line_button_style(active, index_focused && active, palette, fade, status)
            });
            tree = tree.push(category);
            if active {
                tree = tree.push(section_branch(
                    catalog,
                    sections,
                    selected_section,
                    section_focused,
                    scale,
                    palette,
                    fade,
                    branch_reveal,
                ));
            }
        }
        tree.into()
    };
    let tree = scrollable(content)
        .direction(iced::widget::scrollable::Direction::Vertical(
            iced::widget::scrollable::Scrollbar::hidden(),
        ))
        .height(Length::Fill);
    let tracker: Element<'a, Message> = if search_open {
        container(text(""))
            .width(Length::Fixed(14.0 * scale))
            .into()
    } else {
        iced::widget::canvas(IndexRail {
            active: active_tab,
            count: tabs.len(),
            notches: true,
            palette: *palette,
            fade,
        })
        .width(Length::Fixed(14.0 * scale.max(0.9)))
        .height(Length::Fill)
        .into()
    };
    crate::donor_folio::source_ui::folio_index_shell(
        rtl,
        tr("settings-nav-heading"),
        category_note(catalog, selected_tab),
        vec![
            search,
            flow![rtl;tree, tracker]
                .spacing(9.0 * scale)
                .height(Length::Fill)
                .into(),
        ],
        17.0,
        scale,
        palette,
    )
}

fn search_control<'a>(
    catalog: &Catalog,
    open: bool,
    query: &'a str,
    results: &[SettingsSearchResult],
    scale: f32,
    palette: &'a Palette,
    fade: f32,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    if open {
        let submit = results.first().map_or(Message::Noop, |result| {
            Message::Settings(SettingsMsg::OpenSearchResult(
                result.tab.clone(),
                result.section,
                result.row,
            ))
        });
        flow![rtl;
            label("⌕", 15.0, scale, with_alpha(palette.primary, fade)),
            text_input(tr("settings-nav-search"), query)
                .id(crate::candidate_app::view::settings::composed::settings_search_input_id())
                .font(crate::donor_folio::source_ui::UI_FONT)
                .on_input(|value| Message::Settings(SettingsMsg::SearchInput(value)))
                .on_submit(submit)
                .size(12.0 * legible_type_scale(scale))
                .padding([7.0 * scale, 6.0 * scale])
                .width(Length::Fill)
                .style(move |_theme, _status| {
                    crate::donor_folio::source_ui::workbench_input_style(
                        with_alpha(palette.surface_text, fade),
                        with_alpha(palette.primary, fade),
                    )
                }),
            button(
                text("×").font(crate::donor_folio::source_ui::UI_FONT).size(14.0 * legible_type_scale(scale))
            )
            .padding([5.0 * scale, 8.0 * scale])
            .on_press(Message::Settings(SettingsMsg::SearchClose))
            .style(move |_theme, status| {
                line_button_style(false, false, palette, fade, status)
            }),
        ]
        .spacing(7.0 * scale)
        .align_y(Alignment::Center)
        .into()
    } else {
        button(
            flow![rtl;
                label("⌕", 15.0, scale, with_alpha(palette.primary, fade)),
                text(tr("settings-nav-search"))
                    .font(crate::donor_folio::source_ui::UI_FONT)
                    .size(11.5 * legible_type_scale(scale)),
                container(text("")).width(Length::Fill),
                label(
                    tr("settings-nav-search-shortcut"),
                    9.0,
                    scale,
                    with_alpha(palette.surface_text, 0.42 * fade)
                ),
            ]
            .spacing(8.0 * scale)
            .align_y(Alignment::Center),
        )
        .width(Length::Fill)
        .padding([8.0 * scale, 8.0 * scale])
        .on_press(Message::Settings(SettingsMsg::SearchOpen))
        .style(move |_theme, status| line_button_style(false, false, palette, fade, status))
        .into()
    }
}

fn search_result_list<'a>(
    catalog: &Catalog,
    query: &str,
    results: &'a [SettingsSearchResult],
    scale: f32,
    palette: &'a Palette,
    fade: f32,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    if results.is_empty() {
        return container(label(
            if query.trim().is_empty() {
                tr("settings-nav-search-empty-hint")
            } else {
                tr("settings-nav-search-no-results")
            },
            10.5,
            scale,
            with_alpha(palette.surface_text, 0.5 * fade),
        ))
        .padding([12.0 * scale, 8.0 * scale])
        .into();
    }
    let mut list = column![].spacing(1.0 * scale);
    for result in results {
        list = list.push(
            button(
                column![
                    text(&result.title)
                        .font(crate::donor_folio::source_ui::UI_FONT)
                        .size(12.0 * legible_type_scale(scale)),
                    label(
                        format!(
                            "{}  /  {}",
                            sentence_case(&result.tab_label),
                            result.section_title,
                        ),
                        9.0,
                        scale,
                        with_alpha(palette.primary, 0.72 * fade),
                    ),
                ]
                .spacing(3.0 * scale),
            )
            .width(Length::Fill)
            .padding([9.0 * scale, 8.0 * scale])
            .on_press(Message::Settings(SettingsMsg::OpenSearchResult(
                result.tab.clone(),
                result.section,
                result.row,
            )))
            .style(move |_theme, status| line_button_style(false, false, palette, fade, status)),
        );
    }
    list.into()
}

fn section_branch<'a>(
    catalog: &Catalog,
    sections: &[(usize, &'static str)],
    selected: usize,
    keyboard_focused: bool,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    reveal: f32,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let mut children = column![].spacing(1.0 * scale);
    for (index, title) in sections.iter().copied() {
        let active = index == selected;
        children = children.push(
            button(
                flow![rtl;
                    label(
                        if active { "◆" } else { "◇" },
                        8.0,
                        scale,
                        with_alpha(palette.primary, if active { fade } else { 0.44 * fade }),
                    ),
                    text(title)
                        .font(crate::donor_folio::source_ui::UI_FONT)
                        .size(11.5 * legible_type_scale(scale)),
                    container(text("")).width(Length::Fill),
                ]
                .spacing(9.0 * scale)
                .align_y(Alignment::Center),
            )
            .width(Length::Fill)
            .padding([7.0 * scale, 7.0 * scale])
            .on_press(Message::Settings(SettingsMsg::SelectSection(index)))
            .style(move |_theme, status| {
                line_button_style(active, keyboard_focused && active, palette, fade, status)
            }),
        );
    }
    let guide = container(text(""))
        .width(Length::Fixed(1.0))
        .height(Length::Fill)
        .style(move |_| {
            crate::donor_folio::source_ui::bg_style(Background::Color(with_alpha(
                palette.primary,
                0.42 * fade * reveal,
            )))
        });
    container(flow![rtl;guide, children].spacing(9.0 * scale))
        .width(Length::Fill)
        .height(Length::Fixed(branch_height(sections.len(), scale, reveal)))
        .padding(
            crate::donor_folio::source_ui::direction::logical_padding_for(
                rtl,
                2.0 * scale,
                0.0,
                4.0 * scale,
                17.0 * scale,
            ),
        )
        .clip(true)
        .into()
}

fn branch_height(section_count: usize, scale: f32, reveal: f32) -> f32 {
    (section_count as f32 * 36.0 * scale.max(1.0) + 6.0 * scale) * reveal.clamp(0.0, 1.0)
}

struct ReadingInput<'a> {
    tab: &'a str,
    stash_available: bool,
    card: Card,
    settings: Vec<Row>,
    values: &'a HashMap<String, String>,
    has_selected_preset: bool,
    available_width: f32,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    motion: MotionProfile,
}

fn reading_columns(available_width: f32, scale: f32) -> (f32, f32, bool) {
    let content_width = (available_width - 76.0 * scale).max(1.0);
    let column_width = ((content_width - 34.0 * scale) * 0.5).max(1.0);
    (content_width, column_width, column_width < 260.0 * scale)
}

fn reading_surface<'a>(
    catalog: &Catalog,
    input: ReadingInput<'a>,
    focus: FocusCtx<'_>,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let ReadingInput {
        tab,
        stash_available,
        card,
        settings,
        values,
        has_selected_preset,
        available_width,
        scale,
        palette,
        fade,
        motion,
    } = input;
    let (section_title, section_subtitle) = (card.title, card.subtitle);
    let header = column![
        flow![rtl;
            text(section_title)
                .font(crate::donor_folio::source_ui::UI_FONT)
                .size(46.0 * scale.clamp(0.88, 1.08))
                .line_height(iced::widget::text::LineHeight::Relative(1.0))
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                .width(Length::Fill)
                .color(with_alpha(palette.surface_text, fade)),
            iced::widget::tooltip(
                crate::donor_folio::source_ui::folio_icon_action(rtl,
                    crate::donor_folio::source_ui::EYE_CLOSED,
                    stash_available.then_some(Message::Stash),
                    scale,
                    palette,
                ),
                text(if stash_available { "Hide picker; reopen from the tray or hotkey" } else { "Hide unavailable in preview" }).size(12),
                iced::widget::tooltip::Position::Bottom,
            ),
            crate::donor_folio::source_ui::folio_close(rtl, Message::ToggleSettings, scale, palette),
        ]
        .spacing(8.0 * scale)
        .align_y(Alignment::Start),
        container(
            label(
                if section_subtitle.is_empty() {
                    category_note(catalog, tab)
                } else {
                    section_subtitle
                },
                TYPE_SMALL,
                scale,
                with_alpha(palette.surface_text, 0.56 * fade),
            )
            .line_height(iced::widget::text::LineHeight::Relative(1.4)),
        )
        .width(Length::Fill),
        folio_horizontal_rule(with_alpha(palette.outline, FOLIO_RULE_ALPHA * fade)),
    ]
    .spacing(12.0 * scale);
    let (_, column_width, single_column) = reading_columns(available_width, scale);
    let narrow_bindings = section_title == tr("settings-section-controls")
        && column_width < 280.0 * legible_type_scale(scale);
    let controls = field_grid(
        catalog,
        settings,
        tab == "sources" || single_column || narrow_bindings,
        values,
        has_selected_preset,
        focus,
        column_width,
        scale,
        palette,
        fade,
        motion,
    );
    let page = column![header, controls].spacing(28.0 * scale);
    scrollable(container(page).width(Length::Fill).padding(Padding {
        top: 34.0 * scale,
        right: 38.0 * scale,
        bottom: 48.0 * scale,
        left: 38.0 * scale,
    }))
    .direction(iced::widget::scrollable::Direction::Vertical(
        iced::widget::scrollable::Scrollbar::hidden(),
    ))
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn field_grid<'a>(
    catalog: &Catalog,
    settings: Vec<Row>,
    full_width: bool,
    values: &'a HashMap<String, String>,
    has_selected_preset: bool,
    focus: FocusCtx<'_>,
    column_width: f32,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    motion: MotionProfile,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    if settings.is_empty() {
        return label(
            tr("settings-section-empty"),
            12.0,
            scale,
            with_alpha(palette.surface_text, 0.56 * fade),
        )
        .into();
    }

    let mut remaining: Vec<_> = settings.into_iter().enumerate().collect();
    let mut page = column![].spacing(30.0 * scale);
    while !remaining.is_empty() {
        if full_width || remaining[0].1.control.is_wide_field() {
            let (index, setting) = remaining.remove(0);
            page = page.push(field(
                catalog,
                setting,
                index,
                values,
                has_selected_preset,
                focus,
                column_width * 2.0 + 34.0 * scale,
                scale,
                palette,
                fade,
                motion,
            ));
            continue;
        }
        let compact = remaining[0].1.control.is_compact_field();
        let count = remaining
            .iter()
            .take_while(|(_, row)| {
                !matches!(row.control, Control::Segment)
                    && row.control.is_compact_field() == compact
            })
            .count();
        let group: Vec<_> = remaining.drain(..count).collect();
        page = page.push(if compact {
            compact_field_grid(
                catalog,
                group,
                values,
                has_selected_preset,
                focus,
                column_width,
                scale,
                palette,
                fade,
                motion,
            )
        } else {
            regular_field_grid(
                catalog,
                group,
                values,
                has_selected_preset,
                focus,
                column_width,
                scale,
                palette,
                fade,
                motion,
            )
        });
    }
    page.into()
}

#[allow(clippy::too_many_arguments)]
fn regular_field_grid<'a>(
    catalog: &Catalog,
    settings: Vec<(usize, Row)>,
    values: &'a HashMap<String, String>,
    has_selected_preset: bool,
    focus: FocusCtx<'_>,
    column_width: f32,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    motion: MotionProfile,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let mut grid = column![].spacing(30.0 * scale);
    let mut settings = settings.into_iter();
    while let Some((left_index, left)) = settings.next() {
        let left = field(
            catalog,
            left,
            left_index,
            values,
            has_selected_preset,
            focus,
            column_width,
            scale,
            palette,
            fade,
            motion,
        );
        let pair: Element<'a, Message> = if let Some((right_index, right)) = settings.next() {
            let right = field(
                catalog,
                right,
                right_index,
                values,
                has_selected_preset,
                focus,
                column_width,
                scale,
                palette,
                fade,
                motion,
            );
            flow![rtl;
                container(left).width(Length::FillPortion(1)),
                container(text("")).width(Length::Fixed(1.0)).height(Length::Fill).style(
                    move |_| crate::donor_folio::source_ui::bg_style(Background::Color(with_alpha(
                        palette.outline,
                        0.34 * fade,
                    )))
                ),
                container(right).width(Length::FillPortion(1)),
            ]
            .spacing(17.0 * scale)
            .align_y(Alignment::Start)
            .into()
        } else {
            flow![rtl;
                container(left).width(Length::FillPortion(1)),
                container(text("")).width(Length::FillPortion(1)),
            ]
            .spacing(35.0 * scale)
            .align_y(Alignment::Start)
            .into()
        };
        grid = grid.push(pair);
    }
    grid.into()
}

#[allow(clippy::too_many_arguments)]
fn compact_field_grid<'a>(
    catalog: &Catalog,
    mut settings: Vec<(usize, Row)>,
    values: &'a HashMap<String, String>,
    has_selected_preset: bool,
    focus: FocusCtx<'_>,
    column_width: f32,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    motion: MotionProfile,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    if settings.len() == 1 {
        let (index, setting) = settings.remove(0);
        return container(
            container(field(
                catalog,
                setting,
                index,
                values,
                has_selected_preset,
                focus,
                column_width,
                scale,
                palette,
                fade,
                motion,
            ))
            .width(Length::Fixed(column_width)),
        )
        .width(Length::Fill)
        .align_x(if rtl {
            Alignment::End
        } else {
            Alignment::Start
        })
        .into();
    }

    let right = settings.split_off(settings.len().div_ceil(2));
    let mut left_column = column![].spacing(0.0);
    for (index, setting) in settings {
        left_column = left_column.push(field(
            catalog,
            setting,
            index,
            values,
            has_selected_preset,
            focus,
            column_width,
            scale,
            palette,
            fade,
            motion,
        ));
    }
    let mut right_column = column![].spacing(0.0);
    for (index, setting) in right {
        right_column = right_column.push(field(
            catalog,
            setting,
            index,
            values,
            has_selected_preset,
            focus,
            column_width,
            scale,
            palette,
            fade,
            motion,
        ));
    }
    flow![rtl;
        container(left_column).width(Length::FillPortion(1)),
        container(text("")).width(Length::Fixed(1.0)).height(Length::Fill).style(move |_| {
            crate::donor_folio::source_ui::bg_style(Background::Color(with_alpha(
                palette.outline,
                0.34 * fade,
            )))
        }),
        container(right_column).width(Length::FillPortion(1)),
    ]
    .spacing(17.0 * scale)
    .align_y(Alignment::Start)
    .into()
}

fn compact_field_copy(
    title: &str,
    desc: &str,
    copy_width: f32,
    scale: f32,
    palette: &Palette,
    fade: f32,
) -> Element<'static, Message> {
    let title = label(
        title.trim().to_owned(),
        12.0,
        scale,
        with_alpha(palette.surface_text, fade),
    )
    .width(Length::Fixed(copy_width))
    .wrapping(iced::widget::text::Wrapping::WordOrGlyph);
    if desc.trim().is_empty() {
        title.into()
    } else {
        column![
            title,
            label(
                desc.trim().to_owned(),
                TYPE_SMALL,
                scale,
                with_alpha(palette.surface_text, 0.52 * fade)
            )
            .width(Length::Fixed(copy_width))
            .line_height(iced::widget::text::LineHeight::Relative(1.12))
            .wrapping(iced::widget::text::Wrapping::WordOrGlyph),
        ]
        .spacing(2.0 * scale)
        .into()
    }
}

#[allow(clippy::too_many_arguments)]
fn compact_inline_field<'a>(
    catalog: &Catalog,
    setting: Row,
    index: usize,
    values: &'a HashMap<String, String>,
    has_selected_preset: bool,
    focused: bool,
    focused_choice: Option<usize>,
    available_width: f32,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    motion: MotionProfile,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let Row {
        title,
        desc,
        control,
    } = setting;
    let editor_width = match &control {
        Control::Number { .. } => (available_width * 0.34).clamp(118.0 * scale, 176.0 * scale),
        Control::TextField { .. } => (available_width * 0.42).clamp(142.0 * scale, 248.0 * scale),
        Control::KeyBinding { .. } => (available_width * 0.5).clamp(176.0 * scale, 300.0 * scale),
        _ => unreachable!("compact inline field requires one scalar editor"),
    };
    let copy_width = (available_width - editor_width - 44.0 * scale).max(72.0 * scale);
    let copy = compact_field_copy(&title, &desc, copy_width, scale, palette, fade);
    let editor = control::widget(
        catalog,
        control,
        values,
        has_selected_preset,
        focused,
        focused_choice,
        editor_width,
        scale * 0.78,
        palette,
        fade,
        None,
        motion,
        None,
    );
    crate::donor_folio::source_ui::folio_inline_bar(
        rtl,
        copy,
        focused,
        Message::Settings(SettingsMsg::FocusControl(index)),
        container(editor).width(Length::Fixed(editor_width)).into(),
        scale,
        palette,
        fade,
    )
}

#[allow(clippy::too_many_arguments)]
fn compact_action_field<'a>(
    catalog: &Catalog,
    setting: Row,
    index: usize,
    focused: bool,
    available_width: f32,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let Row {
        title,
        desc,
        control,
    } = setting;
    let (value, message, enabled, disabled) = match control {
        Control::Toggle { path, value } => (
            value,
            SettingsMsg::Toggle(path, !value),
            tr("settings-control-enabled"),
            tr("settings-control-disabled"),
        ),
        Control::ToggleAction { id, value } => (
            value,
            SettingsMsg::Run(id),
            tr("settings-app-themes-on"),
            tr("settings-app-themes-off"),
        ),
        _ => unreachable!("compact action field requires a toggle"),
    };
    let action_scale = scale * 0.82;
    let action_width =
        crate::donor_folio::source_ui::folio_action_width(enabled, action_scale).max(
            crate::donor_folio::source_ui::folio_action_width(disabled, action_scale),
        );
    let copy_width = (available_width - action_width - 44.0 * scale).max(72.0 * scale);
    let copy = compact_field_copy(&title, &desc, copy_width, scale, palette, fade);
    let action = crate::donor_folio::source_ui::folio_action(
        rtl,
        if value { enabled } else { disabled },
        value,
        Some(Message::Settings(message)),
        Length::Fixed(action_width),
        action_scale,
        palette,
    );
    crate::donor_folio::source_ui::folio_action_bar(
        rtl,
        copy,
        focused,
        Message::Settings(SettingsMsg::FocusControl(index)),
        action,
        scale,
        palette,
        fade,
    )
}

#[allow(clippy::too_many_arguments)]
fn compact_group_field<'a>(
    catalog: &Catalog,
    setting: Row,
    index: usize,
    values: &'a HashMap<String, String>,
    has_selected_preset: bool,
    focus: FocusCtx<'_>,
    available_width: f32,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    motion: MotionProfile,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let Row {
        title,
        desc,
        control,
    } = setting;
    let title = if title.is_empty() {
        tr("settings-selector-saved-styles-label").to_string()
    } else {
        title
    };
    let id = control
        .compact_bar_id()
        .expect("compact group requires a bar identity");
    let (expanded, reveal) = focus.bar(&id);
    let (focused, _) = focus.on(SettingsFocus::Controls, index);
    let is_stack_bar = matches!(&control, Control::StackBar { .. });
    let description_in_tray = !is_stack_bar;
    let summary = match &control {
        Control::MotionWeights { weights } => {
            weights
                .iter()
                .map(|(_, key, _)| values.get(key).map_or("0", String::as_str))
                .collect::<Vec<_>>()
                .join(" / ")
                + " ms"
        }
        Control::StackBar { summary, .. } => summary.clone(),
        _ => unreachable!("compact group requires multiple editors"),
    };
    let content: Element<'a, Message> = if let Control::StackBar { preview, .. } = &control {
        let preview_width = 86.0 * scale;
        let preview_height = 48.0 * scale;
        let identity_width = (available_width * 0.24).clamp(110.0 * scale, 185.0 * scale);
        let wallpaper_width =
            (available_width - preview_width - identity_width - 96.0 * scale).max(100.0 * scale);
        let preview: Element<'a, Message> = preview.as_ref().map_or_else(
            || {
                container(label(
                    "◇",
                    11.0,
                    scale,
                    with_alpha(palette.surface_text, 0.36 * fade),
                ))
                .width(Length::Fixed(preview_width))
                .height(Length::Fixed(preview_height))
                .center(Length::Fill)
                .style(move |_| {
                    crate::donor_folio::source_ui::box_style(
                        with_alpha(palette.surface_container, 0.32 * fade),
                        with_alpha(palette.outline, 0.34 * fade),
                    )
                })
                .into()
            },
            |path| {
                container(
                    image(image::Handle::from_path(path))
                        .width(Length::Fill)
                        .height(Length::Fill)
                        .content_fit(iced::ContentFit::Cover),
                )
                .width(Length::Fixed(preview_width))
                .height(Length::Fixed(preview_height))
                .clip(true)
                .style(move |_| {
                    crate::donor_folio::source_ui::box_style(
                        with_alpha(palette.surface_container, 0.32 * fade),
                        with_alpha(palette.outline, 0.52 * fade),
                    )
                })
                .into()
            },
        );
        let title = crate::donor_folio::source_ui::ellipsize_text(
            title.trim(),
            12.5 * legible_type_scale(scale),
            identity_width,
        );
        let desc = crate::donor_folio::source_ui::ellipsize_text(
            desc.trim(),
            TYPE_SMALL * legible_type_scale(scale),
            identity_width,
        );
        let summary = crate::donor_folio::source_ui::ellipsize_text(
            summary.trim(),
            TYPE_SMALL * legible_type_scale(scale),
            wallpaper_width,
        );
        flow![rtl;
            preview,
            column![
                label(title, 12.5, scale, with_alpha(palette.surface_text, fade)),
                label(desc, TYPE_SMALL, scale, with_alpha(palette.surface_text, 0.52 * fade)),
            ]
            .width(Length::Fixed(identity_width))
            .spacing(1.0 * scale),
            container(text("")).width(Length::Fill),
            label(summary, TYPE_SMALL, scale, with_alpha(palette.primary, fade))
                .width(Length::Fixed(wallpaper_width))
                .wrapping(iced::widget::text::Wrapping::None)
                .align_x(crate::donor_folio::source_ui::direction::end_for(rtl)),
        ]
        .spacing(12.0 * scale)
        .align_y(Alignment::Center)
        .into()
    } else {
        flow![rtl;
            label(title, 12.5, scale, with_alpha(palette.surface_text, fade)),
            container(text("")).width(Length::Fill),
            label(summary, TYPE_SMALL, scale, with_alpha(palette.primary, fade),)
                .width(Length::Fixed((available_width * 0.42).max(90.0 * scale)))
                .wrapping(iced::widget::text::Wrapping::None)
                .align_x(crate::donor_folio::source_ui::direction::end_for(rtl)),
        ]
        .spacing(10.0 * scale)
        .align_y(Alignment::Center)
        .into()
    };
    let (editor, options_height) = match control {
        Control::StackBar { rows, .. } => {
            let live_height = rows
                .iter()
                .filter(|row| matches!(row.control, Control::Display(_)))
                .count() as f32
                * 48.0;
            let background_height: f32 = rows
                .iter()
                .filter_map(|row| {
                    let Control::Chips {
                        path,
                        current,
                        background: Some(background),
                        ..
                    } = &row.control
                    else {
                        return None;
                    };
                    let reveal = focus.bar_reveals.get(&format!("background:{path}")).map_or(
                        if matches!(current.as_str(), "center" | "fit") {
                            1.0
                        } else {
                            0.0
                        },
                        |value| value.x,
                    );
                    let picker = focus
                        .bar_reveals
                        .get(&background.picker_id())
                        .map_or(0.0, |value| value.x);
                    Some(
                        (52.0 + 192.0 * picker)
                            * crate::donor_folio::source_ui::motion::smoothstep(reveal),
                    )
                })
                .sum();
            (
                detail_rows(
                    catalog,
                    rows,
                    index,
                    values,
                    has_selected_preset,
                    available_width - 18.0 * scale,
                    scale,
                    palette,
                    fade,
                    focus,
                    motion,
                ),
                176.0 + background_height + live_height,
            )
        }
        control => (
            control::widget(
                catalog,
                control,
                values,
                has_selected_preset,
                focused,
                None,
                available_width - 42.0 * scale,
                scale,
                palette,
                fade,
                focus.armed,
                motion,
                Some(focus.bar_reveals),
            ),
            110.0,
        ),
    };
    let options: Element<'a, Message> = if desc.trim().is_empty() || !description_in_tray {
        editor
    } else {
        column![
            label(
                desc,
                TYPE_SMALL,
                scale,
                with_alpha(palette.surface_text, 0.58 * fade),
            ),
            editor,
        ]
        .spacing(6.0 * scale)
        .into()
    };
    crate::donor_folio::source_ui::folio_stack_bar(
        rtl,
        content,
        expanded,
        expanded,
        reveal,
        Message::Settings(SettingsMsg::ToggleBar(id, index)),
        None,
        options,
        if is_stack_bar { 68.0 } else { 42.0 },
        options_height,
        scale,
        palette,
    )
}

fn field<'a>(
    catalog: &Catalog,
    setting: Row,
    index: usize,
    values: &'a HashMap<String, String>,
    has_selected_preset: bool,
    focus: FocusCtx<'_>,
    available_width: f32,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    motion: MotionProfile,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    if let Control::AppTheme { app } = &setting.control {
        return super::app_theme::view(
            catalog,
            app.clone(),
            index,
            focus
                .expanded_details
                .contains(&format!("app-theme:{}", app.id)),
            focus.on(SettingsFocus::Controls, index).0,
            scale,
            palette,
        );
    }
    if setting.control.is_inline_editor() {
        let (focused, focused_choice) = focus.on(SettingsFocus::Controls, index);
        return compact_inline_field(
            catalog,
            setting,
            index,
            values,
            has_selected_preset,
            focused,
            focused_choice,
            available_width,
            scale,
            palette,
            fade,
            motion,
        );
    }
    if setting.control.is_group_editor() {
        return compact_group_field(
            catalog,
            setting,
            index,
            values,
            has_selected_preset,
            focus,
            available_width,
            scale,
            palette,
            fade,
            motion,
        );
    }
    let (focused, focused_choice) = focus.on(SettingsFocus::Controls, index);
    if setting.control.is_compact_action() {
        return compact_action_field(
            catalog,
            setting,
            index,
            focused,
            available_width,
            scale,
            palette,
            fade,
        );
    }
    let Row {
        title,
        desc,
        control,
    } = setting;
    if matches!(control, Control::Segment) {
        return column![
            text(title)
                .font(crate::donor_folio::source_ui::UI_FONT)
                .size(22.0 * legible_type_scale(scale))
                .color(with_alpha(palette.surface_text, fade)),
            label(
                desc,
                TYPE_SMALL,
                scale,
                with_alpha(palette.surface_text, 0.56 * fade)
            )
            .line_height(iced::widget::text::LineHeight::Relative(1.38)),
            folio_horizontal_rule(with_alpha(palette.outline, FOLIO_RULE_ALPHA * fade)),
        ]
        .spacing(9.0 * scale)
        .padding(Padding {
            top: 10.0 * scale,
            ..Padding::ZERO
        })
        .into();
    }
    if let Control::Details { id, summary, rows } = control {
        let expanded = focus.expanded_details.contains(&id);
        let body = detail_rows(
            catalog,
            rows,
            index,
            values,
            has_selected_preset,
            available_width,
            scale,
            palette,
            fade,
            focus,
            motion,
        );
        return crate::donor_folio::source_ui::folio_details(
            rtl,
            title,
            summary,
            desc,
            expanded,
            focused,
            Message::Settings(SettingsMsg::ToggleDetails(id, index)),
            body,
            scale,
            palette,
        );
    }
    let locked = focus.active_input.is_some_and(|key| match &control {
        Control::Number {
            key: control_key, ..
        }
        | Control::TextField {
            key: control_key, ..
        }
        | Control::Resolution {
            key: control_key, ..
        } => control_key == key,
        Control::MotionWeights { weights } => {
            weights.iter().any(|(_, control_key, _)| control_key == key)
        }
        _ => false,
    }) || focused && focused_choice.is_some();
    let title = if title.is_empty() {
        tr("settings-selector-saved-styles-label").to_string()
    } else {
        title
    };
    let widget = control::widget(
        catalog,
        control,
        values,
        has_selected_preset,
        focused,
        focused_choice,
        available_width,
        scale,
        palette,
        fade,
        focus.armed,
        motion,
        Some(focus.bar_reveals),
    );
    let body = column![
        container(text(""))
            .width(Length::Fill)
            .height(Length::Fixed(2.0))
            .style(
                move |_| crate::donor_folio::source_ui::bg_style(Background::Color(
                    if focused || locked {
                        with_alpha(palette.primary, fade)
                    } else {
                        with_alpha(palette.outline, 0.48 * fade)
                    }
                ))
            ),
        label(title, 14.0, scale, with_alpha(palette.surface_text, fade)),
        label(
            desc,
            TYPE_SMALL,
            scale,
            with_alpha(palette.surface_text, 0.56 * fade)
        )
        .line_height(iced::widget::text::LineHeight::Relative(1.38)),
        container(widget).width(Length::Fill).padding(Padding {
            top: 7.0 * scale,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
        }),
    ]
    .spacing(8.0 * scale);
    mouse_area(container(body).width(Length::Fill).padding(Padding {
        top: 0.0,
        right: 6.0 * scale,
        bottom: 4.0 * scale,
        left: 6.0 * scale,
    }))
    .on_press(Message::Settings(SettingsMsg::FocusControl(index)))
    .into()
}

#[allow(clippy::too_many_arguments)]
fn detail_rows<'a>(
    catalog: &Catalog,
    rows: Vec<Row>,
    parent_index: usize,
    values: &'a HashMap<String, String>,
    has_selected_preset: bool,
    available_width: f32,
    scale: f32,
    palette: &'a Palette,
    fade: f32,
    focus: FocusCtx<'_>,
    motion: MotionProfile,
) -> Element<'a, Message> {
    let tr = |key| catalog.text_static(key);
    let rtl = catalog.language.rtl;

    let mut content = column![].spacing(15.0 * scale);
    for row in rows {
        if let Control::Display(control) = row.control {
            content = content.push(crate::candidate_app::view::settings::folio::display_view(
                catalog,
                control,
                scale * 0.94,
                palette,
            ));
            continue;
        }
        if let Control::Details { id, summary, rows } = row.control {
            let expanded = focus.expanded_details.contains(&id);
            let body = detail_rows(
                catalog,
                rows,
                parent_index,
                values,
                has_selected_preset,
                (available_width - 24.0 * scale).max(160.0 * scale),
                scale * 0.94,
                palette,
                fade,
                focus,
                motion,
            );
            content = content.push(crate::donor_folio::source_ui::folio_details(
                rtl,
                row.title,
                summary,
                row.desc,
                expanded,
                false,
                Message::Settings(SettingsMsg::ToggleDetails(id, parent_index)),
                body,
                scale * 0.94,
                palette,
            ));
            continue;
        }
        if row.control.is_inline_editor() {
            content = content.push(compact_inline_field(
                catalog,
                row,
                parent_index,
                values,
                has_selected_preset,
                false,
                None,
                (available_width - 24.0 * scale).max(160.0 * scale),
                scale * 0.94,
                palette,
                fade,
                motion,
            ));
            continue;
        }
        if row.control.is_group_editor() {
            let nested_focus = FocusCtx {
                focused_control: usize::MAX,
                focused_choice: None,
                ..focus
            };
            content = content.push(compact_group_field(
                catalog,
                row,
                parent_index,
                values,
                has_selected_preset,
                nested_focus,
                (available_width - 24.0 * scale).max(160.0 * scale),
                scale * 0.94,
                palette,
                fade,
                motion,
            ));
            continue;
        }
        if row.control.is_compact_action() {
            content = content.push(compact_action_field(
                catalog,
                row,
                parent_index,
                false,
                (available_width - 24.0 * scale).max(160.0 * scale),
                scale * 0.94,
                palette,
                fade,
            ));
            continue;
        }
        let Row {
            title,
            desc,
            control,
        } = row;
        let widget = control::widget(
            catalog,
            control,
            values,
            has_selected_preset,
            false,
            None,
            (available_width - 24.0 * scale).max(160.0 * scale),
            scale * 0.94,
            palette,
            fade,
            focus.armed,
            motion,
            Some(focus.bar_reveals),
        );
        content = content.push(
            column![
                label(title, 12.0, scale, with_alpha(palette.surface_text, fade)),
                label(
                    desc,
                    TYPE_SMALL,
                    scale,
                    with_alpha(palette.surface_text, 0.5 * fade)
                )
                .line_height(iced::widget::text::LineHeight::Relative(1.32)),
                container(widget).width(Length::Fill).padding(Padding {
                    top: 3.0 * scale,
                    ..Padding::ZERO
                }),
            ]
            .spacing(6.0 * scale),
        );
    }
    content.into()
}

fn line_button_style(
    active: bool,
    focused: bool,
    palette: &Palette,
    fade: f32,
    status: button::Status,
) -> button::Style {
    crate::donor_folio::source_ui::folio_line_button_style(active, focused, palette, fade, status)
}

fn tracker_position(active: usize, count: usize) -> f32 {
    if count <= 1 {
        0.0
    } else {
        active.min(count - 1) as f32 / (count - 1) as f32
    }
}

struct IndexRail {
    active: usize,
    count: usize,
    notches: bool,
    palette: Palette,
    fade: f32,
}

impl canvas::Program<Message> for IndexRail {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        let x = bounds.width * 0.5;
        let top = 8.0;
        let bottom = (bounds.height - 8.0).max(top);
        frame.stroke(
            &Path::line(Point::new(x, top), Point::new(x, bottom)),
            Stroke::default()
                .with_color(with_alpha(self.palette.outline, 0.48 * self.fade))
                .with_width(1.0),
        );
        if self.notches {
            for index in 0..self.count {
                let y = top + (bottom - top) * tracker_position(index, self.count);
                frame.stroke(
                    &Path::line(Point::new(x - 2.5, y), Point::new(x + 2.5, y)),
                    Stroke::default()
                        .with_color(with_alpha(self.palette.outline, 0.58 * self.fade))
                        .with_width(1.0),
                );
            }
        }
        let y = top + (bottom - top) * tracker_position(self.active, self.count);
        let radius = 5.0;
        let diamond = Path::new(|builder| {
            builder.move_to(Point::new(x, y - radius));
            builder.line_to(Point::new(x + radius, y));
            builder.line_to(Point::new(x, y + radius));
            builder.line_to(Point::new(x - radius, y));
            builder.close();
        });
        frame.fill(&diamond, with_alpha(self.palette.background, self.fade));
        frame.stroke(
            &diamond,
            Stroke::default()
                .with_color(with_alpha(self.palette.primary, self.fade))
                .with_width(1.5),
        );
        vec![frame.into_geometry()]
    }
}

struct Blueprint {
    rtl: bool,
    palette: Palette,
    fade: f32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate_app::view::settings::composed::test_source::FakeSettingsSource;
    fn palette() -> Palette {
        Palette::from_rgb([0.1; 3], [0.2; 3], [0.9; 3], [0.3, 0.6, 0.8])
    }
    #[test]
    #[ignore = "Owned Settings compact geometry/click/capture contract; explicit invocation"]
    fn settings_cleanup_compact_group_layouts() {
        use iced::advanced::{
            layout,
            renderer::{Headless, Renderer as _},
            widget::{Operation, Tree},
            Layout,
        };
        #[derive(Default)]
        struct Bounds(Vec<(String, Rectangle)>, Vec<Rectangle>);
        impl Operation for Bounds {
            fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
                visit(self);
            }
            fn text(&mut self, _: Option<&iced::widget::Id>, bounds: Rectangle, value: &str) {
                self.0.push((value.into(), bounds));
            }
            fn text_input(
                &mut self,
                _: Option<&iced::widget::Id>,
                bounds: Rectangle,
                _: &mut dyn iced::advanced::widget::operation::TextInput,
            ) {
                self.1.push(bounds);
            }
        }
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            crate::candidate_app::UI_FONT,
            iced::Pixels(16.0),
            Some("wgpu"),
        ))
        .unwrap();
        let output =
            std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned output"));
        std::fs::create_dir_all(&output).unwrap();
        let mut receipts = Vec::new();
        for scale in [1.0, 0.5] {
            for n in [1usize, 2, 3, 5] {
                for rtl in [false, true] {
                    let mut catalog = Catalog::default();
                    catalog.language = crate::candidate_app::localization::LANGUAGES
                        .iter()
                        .find(|language| language.tag == if rtl { "ar-SA" } else { "en-US" })
                        .unwrap();
                    assert_eq!(
                        catalog.language.rtl, rtl,
                        "fixture must exercise requested direction"
                    );
                    let palette = palette();
                    let values = HashMap::new();
                    let expanded = HashSet::new();
                    let reveals = HashMap::new();
                    let column_width = 300.0 * scale;
                    let size = iced::Size::new(column_width * 2.0 + 34.0 * scale, 450.0);
                    let viewport = Rectangle::with_size(size);
                    let mut focus_bounds = Vec::new();
                    for focused in [false, true] {
                        let focus = FocusCtx {
                            keyboard_focus: SettingsFocus::Controls,
                            focused_control: if focused { 11 } else { 99 },
                            focused_choice: None,
                            expanded_details: &expanded,
                            bar_reveals: &reveals,
                            active_input: None,
                            armed: None,
                        };
                        let rows = (0..n)
                            .map(|i| {
                                (
                                    11 + i,
                                    Row {
                                        title: format!("Field {i}"),
                                        desc: String::new(),
                                        control: Control::Toggle {
                                            path: format!("test.{i}"),
                                            value: false,
                                        },
                                    },
                                )
                            })
                            .collect();
                        let mut element = compact_field_grid(
                            &catalog,
                            rows,
                            &values,
                            false,
                            focus,
                            column_width,
                            scale,
                            &palette,
                            1.0,
                            MotionProfile::default(),
                        );
                        let mut tree = Tree::new(&element);
                        let node = element.as_widget_mut().layout(
                            &mut tree,
                            &renderer,
                            &layout::Limits::new(iced::Size::ZERO, size),
                        );
                        if n == 1 {
                            assert_eq!(node.children().len(), 1, "no separator or empty companion");
                            assert!(
                                (node.children()[0].size().width - column_width).abs() < 0.1,
                                "singleton retains half-width"
                            );
                        } else {
                            assert_eq!(
                                node.children().len(),
                                3,
                                "existing two columns + separator"
                            );
                        }
                        let mut bounds = Bounds::default();
                        element.as_widget_mut().operate(
                            &mut tree,
                            Layout::new(&node),
                            &renderer,
                            &mut bounds,
                        );
                        let fields = (0..n)
                            .map(|i| {
                                bounds
                                    .0
                                    .iter()
                                    .find(|(text, _)| text == &format!("Field {i}"))
                                    .unwrap()
                                    .1
                            })
                            .collect::<Vec<_>>();
                        for b in &fields {
                            assert!(
                                b.width > 0.0 && viewport.contains(b.center()),
                                "label reachable"
                            );
                        }
                        let split = n.div_ceil(2);
                        for i in 1..n {
                            if i != split {
                                assert!(fields[i].y > fields[i - 1].y, "column-major order");
                            }
                        }
                        if n > 1 {
                            assert!((fields[0].y - fields[split].y).abs() < 0.1);
                            assert_eq!(fields[0].x > fields[split].x, rtl);
                        }
                        let mut messages = Vec::new();
                        element.as_widget_mut().update(
                            &mut tree,
                            &iced::Event::Mouse(iced::mouse::Event::ButtonPressed(
                                iced::mouse::Button::Left,
                            )),
                            Layout::new(&node),
                            iced::mouse::Cursor::Available(fields[0].center()),
                            &renderer,
                            &mut iced::advanced::clipboard::Null,
                            &mut iced::advanced::Shell::new(&mut messages),
                            &viewport,
                        );
                        assert!(
                            messages.iter().any(|message| matches!(
                                message,
                                Message::Settings(SettingsMsg::FocusControl(11))
                            )),
                            "original control index retained"
                        );
                        focus_bounds.push(fields.clone());
                        if !focused {
                            renderer.reset(viewport);
                            element.as_widget().draw(
                                &tree,
                                &mut renderer,
                                &iced::Theme::Dark,
                                &iced::advanced::renderer::Style {
                                    text_color: palette.surface_text,
                                },
                                Layout::new(&node),
                                iced::mouse::Cursor::Unavailable,
                                &viewport,
                            );
                            let pixels =
                                iced::Size::new(size.width.ceil() as u32, size.height as u32);
                            let bytes =
                                Headless::screenshot(&mut renderer, pixels, 1.0, Color::BLACK);
                            let path =
                                output.join(format!("compact-n{n}-scale{scale}-rtl{rtl}.png"));
                            ::image::save_buffer_with_format(
                                &path,
                                &bytes,
                                pixels.width,
                                pixels.height,
                                ::image::ColorType::Rgba8,
                                ::image::ImageFormat::Png,
                            )
                            .unwrap();
                            receipts.push(serde_json::json!({"n":n,"scale":scale,"rtl":rtl,"path":path,"fields":fields.iter().map(|b|[b.x,b.y,b.width,b.height]).collect::<Vec<_>>() }));
                        }
                    }
                    assert_eq!(
                        focus_bounds[0], focus_bounds[1],
                        "focus cannot shift layout"
                    );
                }
            }
        }
        // Exercise the public Performance composition through the real narrow-column decision.
        for (width, scale, full_width) in [(600.0, 1.0, true), (640.0, 0.5, false)] {
            let config = FakeSettingsSource::default();
            let palette = palette();
            let values = HashMap::from([(String::from("general.maxFps"), String::from("120"))]);
            let expanded = HashSet::new();
            let reveals = HashMap::new();
            let focus = FocusCtx {
                keyboard_focus: SettingsFocus::Controls,
                focused_control: 0,
                focused_choice: None,
                expanded_details: &expanded,
                bar_reveals: &reveals,
                active_input: None,
                armed: None,
            };
            assert_eq!(reading_columns(width, scale).2, full_width);
            let (card, settings) = crate::candidate_app::view::settings::composed::build_tab(
                "performance",
                &config,
                &[],
                &[],
                "",
                &[],
            )
            .remove(0);
            assert_eq!(settings.len(), 1);
            let title = settings[0].title.clone();
            let size = iced::Size::new(width, 450.0);
            let viewport = Rectangle::with_size(size);
            let mut element = reading_surface(
                config.catalog(),
                ReadingInput {
                    tab: "performance",
                    stash_available: false,
                    card,
                    settings,
                    values: &values,
                    has_selected_preset: false,
                    available_width: width,
                    scale,
                    palette: &palette,
                    fade: 1.0,
                    motion: MotionProfile::default(),
                },
                focus,
            );
            let mut tree = Tree::new(&element);
            let node = element.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(size, size),
            );
            let mut bounds = Bounds::default();
            element
                .as_widget_mut()
                .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
            let labels = bounds
                .0
                .iter()
                .filter(|(text, _)| text == &title)
                .map(|(_, b)| *b)
                .collect::<Vec<_>>();
            assert!(labels.len() >= 2, "actual header and control label");
            assert_eq!(bounds.1.len(), 1, "actual remaining Max FPS editor");
            for b in labels.iter().chain(bounds.1.iter()) {
                assert!(b.width > 0.0 && b.height > 0.0);
                assert!(
                    b.x >= 0.0
                        && b.y >= 0.0
                        && b.x + b.width <= width + 0.1
                        && b.y + b.height <= size.height + 0.1,
                    "label/editor reachable: {b:?}"
                );
            }
            let mut messages = Vec::new();
            element.as_widget_mut().update(
                &mut tree,
                &iced::Event::Mouse(iced::mouse::Event::ButtonPressed(iced::mouse::Button::Left)),
                Layout::new(&node),
                iced::mouse::Cursor::Available(labels.last().unwrap().center()),
                &renderer,
                &mut iced::advanced::clipboard::Null,
                &mut iced::advanced::Shell::new(&mut messages),
                &viewport,
            );
            assert!(
                messages.iter().any(|message| matches!(
                    message,
                    Message::Settings(SettingsMsg::FocusControl(0))
                )),
                "actual public control index"
            );
            renderer.reset(viewport);
            element.as_widget().draw(
                &tree,
                &mut renderer,
                &iced::Theme::Dark,
                &iced::advanced::renderer::Style {
                    text_color: palette.surface_text,
                },
                Layout::new(&node),
                iced::mouse::Cursor::Unavailable,
                &viewport,
            );
            let pixels = iced::Size::new(width as u32, size.height as u32);
            let bytes = Headless::screenshot(&mut renderer, pixels, 1.0, Color::BLACK);
            let path = output.join(format!("performance-reading-width{width}-scale{scale}.png"));
            ::image::save_buffer_with_format(
                &path,
                &bytes,
                pixels.width,
                pixels.height,
                ::image::ColorType::Rgba8,
                ::image::ImageFormat::Png,
            )
            .unwrap();
            receipts.push(serde_json::json!({"production_reading_surface":true,"width":width,"scale":scale,"full_width":full_width,"path":path,"labels":labels.iter().map(|b|[b.x,b.y,b.width,b.height]).collect::<Vec<_>>(),"editors":bounds.1.iter().map(|b|[b.x,b.y,b.width,b.height]).collect::<Vec<_>>()}));
        }
        std::fs::write(
            output.join("COMPACT-GEOMETRY.json"),
            serde_json::to_vec_pretty(&receipts).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn actual_workbench_constructs_all_categories_and_live_layout_studio_for_each_locale() {
        for locale in ["en-US", "ja-JP", "ar-SA"] {
            let config = FakeSettingsSource::default().with_locale(locale);
            let catalog = config.catalog();
            let palette = palette();
            let values = HashMap::new();
            let expanded = HashSet::new();
            let reveals = HashMap::new();
            let previews = HashMap::new();
            for (tab, _) in visible_tabs(&config) {
                let source = SourceCtx {
                    config: &config,
                    values: &values,
                    themes: &[],
                    folders: &[],
                    backends: &[],
                    outputs: &[],
                    output_statuses: &[],
                    output_previews: &previews,
                    library_watch: None,
                    app_themes: None,
                    playback: None,
                    thumbnail_task: None,
                    analysis: String::new(),
                };
                let focus = FocusCtx {
                    keyboard_focus: SettingsFocus::Sections,
                    focused_control: 0,
                    focused_choice: None,
                    expanded_details: &expanded,
                    bar_reveals: &reveals,
                    active_input: None,
                    armed: None,
                };
                let chrome = ChromeCtx {
                    viewport: (1920.0, 1080.0),
                    scale: 1.0,
                    entrance: 1.0,
                    palette: &palette,
                };
                let element = settings_workbench(WorkbenchInput {
                    source,
                    focus,
                    chrome,
                    preview_image: None,
                    tab_transition: 1.0,
                    tab,
                    selected_section: 0,
                    search_open: false,
                    search_query: "",
                    search_results: &[],
                    keybind_capture: None,
                    process_picker: None,
                    status: "Saved pending. This operation is unavailable in this build.",
                });
                assert!(
                    !iced::advanced::widget::Tree::new(&element)
                        .children
                        .is_empty(),
                    "{locale}/{tab}"
                );
            }
            let source = SourceCtx {
                config: &config,
                values: &values,
                themes: &[],
                folders: &[],
                backends: &[],
                outputs: &[],
                output_statuses: &[],
                output_previews: &previews,
                library_watch: None,
                app_themes: None,
                playback: None,
                thumbnail_task: None,
                analysis: String::new(),
            };
            let focus = FocusCtx {
                keyboard_focus: SettingsFocus::Controls,
                focused_control: 0,
                focused_choice: None,
                expanded_details: &expanded,
                bar_reveals: &reveals,
                active_input: None,
                armed: None,
            };
            let chrome = ChromeCtx {
                viewport: (2560.0, 1440.0),
                scale: 1.0,
                entrance: 1.0,
                palette: &palette,
            };
            let element = picker_layout_workbench(&source, focus, chrome);
            assert!(!iced::advanced::widget::Tree::new(&element)
                .children
                .is_empty());
        }
    }
    #[test]
    fn production_reading_geometry_and_branch_reveal_match_source_at_required_sizes() {
        for viewport in [(1920.0, 1080.0), (2560.0, 1440.0)] {
            let (sheet, _) = crate::donor_folio::source_ui::folio_sheet_dims(viewport, 1.0);
            let (content, column, single) = reading_columns(sheet - 318.0, 1.0);
            assert_eq!(content, 996.0);
            assert_eq!(column, 481.0);
            assert!(!single);
            let studio = crate::donor_folio::source_ui::picker_layout_studio_width(viewport.0, 1.0);
            assert_eq!(studio, 440.0);
            assert!(studio < viewport.0 * 0.5);
        }
        assert!(reading_columns(500.0, 1.0).2);
        assert_eq!(branch_height(5, 1.0, 0.0), 0.0);
        assert_eq!(branch_height(5, 1.0, 1.0), 186.0);
    }
    #[test]
    #[ignore = "Owned focused/unfocused production field geometry and captures; explicit invocation"]
    fn feedback_owned_layouts() {
        use iced::advanced::{
            layout,
            renderer::{Headless, Renderer as _},
            widget::{Operation, Tree},
            Layout,
        };
        #[derive(Default)]
        struct TextBounds(Vec<(String, Rectangle)>);
        impl Operation for TextBounds {
            fn traverse(&mut self, visit: &mut dyn FnMut(&mut dyn Operation)) {
                visit(self);
            }
            fn text(&mut self, _: Option<&iced::widget::Id>, b: Rectangle, value: &str) {
                self.0.push((value.into(), b));
            }
        }
        let config = FakeSettingsSource::default();
        let catalog = config.catalog();
        let palette = palette();
        let values = HashMap::new();
        let expanded = HashSet::new();
        let reveals = HashMap::new();
        let previews = HashMap::new();
        let source = SourceCtx {
            config: &config,
            values: &values,
            themes: &[],
            folders: &[],
            backends: &[],
            outputs: &[],
            output_statuses: &[],
            output_previews: &previews,
            library_watch: None,
            app_themes: None,
            playback: None,
            thumbnail_task: None,
            analysis: String::new(),
        };
        let row = source.cards("filter").into_iter().flat_map(|(_,rows)| rows)
            .find(|row| matches!(&row.control, Control::Dropdown { path, .. } if path == "filterBar.visualStyle")).unwrap();
        let mut renderer = iced::futures::executor::block_on(<iced::Renderer as Headless>::new(
            crate::candidate_app::UI_FONT,
            iced::Pixels(16.0),
            Some("wgpu"),
        ))
        .unwrap();
        let size = iced::Size::new(620.0, 360.0);
        let output =
            std::path::PathBuf::from(std::env::var_os("P5_LAYOUT_OUTPUT").expect("owned output"));
        std::fs::create_dir_all(&output).unwrap();
        let mut receipts = Vec::new();
        for picker in [false, true] {
            let mut positions = Vec::new();
            for focused in [false, true] {
                let focus = FocusCtx {
                    keyboard_focus: SettingsFocus::Controls,
                    focused_control: if focused { 0 } else { 1 },
                    focused_choice: None,
                    expanded_details: &expanded,
                    bar_reveals: &reveals,
                    active_input: None,
                    armed: None,
                };
                let mut element = if picker {
                    picker_layout_field(
                        catalog,
                        row.clone(),
                        0,
                        &values,
                        false,
                        focus,
                        620.0,
                        0.9,
                        &palette,
                        1.0,
                        source.motion(),
                    )
                } else {
                    field(
                        catalog,
                        row.clone(),
                        0,
                        &values,
                        false,
                        focus,
                        620.0,
                        0.9,
                        &palette,
                        1.0,
                        source.motion(),
                    )
                };
                let mut tree = Tree::new(&element);
                let node = element.as_widget_mut().layout(
                    &mut tree,
                    &renderer,
                    &layout::Limits::new(size, size),
                );
                let mut bounds = TextBounds::default();
                element.as_widget_mut().operate(
                    &mut tree,
                    Layout::new(&node),
                    &renderer,
                    &mut bounds,
                );
                assert!(!bounds.0.is_empty());
                positions.push(bounds.0.clone());
                let viewport = Rectangle::with_size(size);
                renderer.reset(viewport);
                element.as_widget().draw(
                    &tree,
                    &mut renderer,
                    &iced::Theme::Dark,
                    &iced::advanced::renderer::Style {
                        text_color: palette.surface_text,
                    },
                    Layout::new(&node),
                    iced::mouse::Cursor::Unavailable,
                    &viewport,
                );
                let bytes = Headless::screenshot(
                    &mut renderer,
                    iced::Size::new(620u32, 360u32),
                    1.0,
                    Color::BLACK,
                );
                let path = output.join(format!(
                    "feedback-field-picker{picker}-focused{focused}.png"
                ));
                ::image::save_buffer_with_format(
                    &path,
                    &bytes,
                    620,
                    360,
                    ::image::ColorType::Rgba8,
                    ::image::ImageFormat::Png,
                )
                .unwrap();
                receipts.push(serde_json::json!({"picker":picker,"focused":focused,"path":path,"texts":bounds.0.iter().map(|(s,b)|serde_json::json!({"text":s,"bounds":[b.x,b.y,b.width,b.height]})).collect::<Vec<_>>()}));
            }
            assert_eq!(
                positions[0], positions[1],
                "focus must not change actual title/description/choice geometry"
            );
        }
        std::fs::write(
            output.join("FEEDBACK-FOCUS-GEOMETRY.json"),
            serde_json::to_vec_pretty(&receipts).unwrap(),
        )
        .unwrap();
    }
}

impl canvas::Program<Message> for Blueprint {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: iced::mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let rtl = self.rtl;

        let mut frame = Frame::new(renderer, bounds.size());
        let line = with_alpha(self.palette.outline, 0.075 * self.fade);
        let accent = with_alpha(self.palette.primary, 0.09 * self.fade);
        let centre = Point::new(
            crate::donor_folio::source_ui::direction::mirror_x_for(
                rtl,
                bounds.width * 0.72,
                0.0,
                bounds.width,
            ),
            bounds.height * 0.56,
        );
        for radius in [90.0, 176.0, 264.0] {
            frame.stroke(
                &Path::circle(centre, radius),
                Stroke::default().with_color(line).with_width(1.0),
            );
        }
        frame.stroke(
            &Path::line(
                Point::new(centre.x, 0.0),
                Point::new(centre.x, bounds.height),
            ),
            Stroke::default().with_color(line).with_width(1.0),
        );
        frame.stroke(
            &Path::line(
                Point::new(0.0, centre.y),
                Point::new(bounds.width, centre.y),
            ),
            Stroke::default().with_color(line).with_width(1.0),
        );
        let marker = Path::new(|builder| {
            builder.move_to(Point::new(centre.x, centre.y - 7.0));
            builder.line_to(Point::new(centre.x + 7.0, centre.y));
            builder.line_to(Point::new(centre.x, centre.y + 7.0));
            builder.line_to(Point::new(centre.x - 7.0, centre.y));
            builder.close();
        });
        frame.stroke(
            &marker,
            Stroke::default().with_color(accent).with_width(1.3),
        );
        vec![frame.into_geometry()]
    }
}

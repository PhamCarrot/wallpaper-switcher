use super::super::*;
use super::donor_palette;
use crate::{
    donor_back_layout, donor_folio, donor_search, donor_ui, engine_properties, gpu_gallery, library,
};
use iced::widget::{column, image, mouse_area};
use iced::{Alignment, Length};

// R5 panels use the adopted Folio label/action metrics. Natural shaped-text
// width is intentional: character-count estimates clip CJK and long captions.
pub(super) fn panel_text<'a>(
    value: impl iced::widget::text::IntoFragment<'a>,
    scale: f32,
) -> iced::widget::Text<'a> {
    text(value)
        .font(UI_FONT)
        .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
        .shaping(iced::widget::text::Shaping::Advanced)
}
pub(super) fn panel_button<'a>(
    content: impl Into<Element<'a, Message>>,
    palette: donor_ui::Palette,
    scale: f32,
) -> iced::widget::Button<'a, Message> {
    button(content)
        .padding([7.0 * scale, 12.0 * scale])
        .style(move |_, status| {
            donor_folio::source_ui::folio_button_style(false, false, &palette, status)
        })
}

// Keep the retained slider's keyboard/pointer interaction. Its 2px rail and
// 10px outlined marker follow Folio; the native circular marker is retained.
pub(super) fn panel_slider_style(palette: donor_ui::Palette) -> iced::widget::slider::Style {
    use iced::widget::slider::{Handle, HandleShape, Rail, Style};
    Style {
        rail: Rail {
            backgrounds: (
                palette.primary.into(),
                donor_ui::with_alpha_color(palette.outline, 0.5).into(),
            ),
            width: 2.0,
            border: iced::Border::default(),
        },
        handle: Handle {
            shape: HandleShape::Circle { radius: 5.0 },
            background: palette.surface.into(),
            border_width: 1.0,
            border_color: palette.primary,
        },
    }
}

pub(super) fn engine_property_sheet(
    app: &GpuCandidate,
    palette: donor_ui::Palette,
) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    let tr = |key| app.settings_locale.text_static(key);
    let mut property_panel = column![].spacing(14.0 * scale);
    for property in &app.engine_properties {
        let value = app
            .engine_property_values
            .get(&property.key)
            .unwrap_or(&property.default_value);
        let key = property.key.clone();
        let control: Element<'_, Message> = match &property.kind {
            engine_properties::EnginePropertyKind::Bool => {
                let enabled = value.as_bool().unwrap_or(false);
                panel_button(
                    panel_text(if enabled { tr("r5-on") } else { tr("r5-off") }, scale),
                    palette,
                    scale,
                )
                .on_press(Message::EnginePropertyValueChanged(
                    key,
                    serde_json::Value::Bool(!enabled),
                ))
                .style(move |_, status| donor_ui::action_style(palette, enabled, false, status))
                .into()
            }
            engine_properties::EnginePropertyKind::Slider { min, max, step } => {
                let current = value.as_f64().unwrap_or(*min).clamp(*min, *max);
                let fallback = property.default_value.clone();
                row![
                    container(
                        slider(*min..=*max, current, move |number| {
                            let value = serde_json::Number::from_f64(number)
                                .map(serde_json::Value::Number)
                                .unwrap_or_else(|| fallback.clone());
                            Message::EnginePropertyValueChanged(key.clone(), value)
                        })
                        .step(*step)
                        .height(26.0 * scale)
                        .style(move |_, _| panel_slider_style(palette))
                    )
                    .width(Fill),
                    panel_text(format!("{current:.2}"), scale)
                ]
                .spacing(10.0 * scale)
                .align_y(iced::Alignment::Center)
                .into()
            }
            engine_properties::EnginePropertyKind::Combo { options } => {
                let labels = options
                    .iter()
                    .enumerate()
                    .map(|(index, option)| {
                        if options
                            .iter()
                            .filter(|candidate| candidate.label == option.label)
                            .count()
                            > 1
                        {
                            format!("{} ({})", option.label, index + 1)
                        } else {
                            option.label.clone()
                        }
                    })
                    .collect::<Vec<_>>();
                let mut choices = row![].spacing(6.0 * scale);
                for (option, label) in options.iter().zip(labels) {
                    let active = option.value == *value;
                    choices = choices.push(
                        panel_button(panel_text(label, scale), palette, scale)
                            .on_press(Message::EnginePropertyValueChanged(
                                key.clone(),
                                option.value.clone(),
                            ))
                            .style(move |_, status| {
                                donor_ui::action_style(palette, active, false, status)
                            }),
                    );
                }
                choices.wrap().into()
            }
            engine_properties::EnginePropertyKind::Color => {
                let color = value
                    .as_str()
                    .and_then(engine_properties::color_preview_rgb);
                let mut editor = column![
                    row![
                        container(text(if color.is_some() { "" } else { "?" }))
                            .width(38.0 * scale)
                            .height(20.0 * scale)
                            .style(move |_| {
                                container::Style {
                                    background: color.map(|[r, g, b]| {
                                        Background::Color(Color::from_rgb(r, g, b))
                                    }),
                                    border: iced::Border {
                                        color: Color::from_rgba(1.0, 1.0, 1.0, 0.45),
                                        width: 1.0,
                                        radius: 4.0.into(),
                                    },
                                    ..Default::default()
                                }
                            }),
                        text_input("#RRGGBB or R G B (0–1)", value.as_str().unwrap_or_default())
                            .padding([7.0 * scale, 10.0 * scale])
                            .font(UI_FONT)
                            .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                            .style(move |_, _| donor_ui::ghost_input_style(palette))
                            .on_input({
                                let key = key.clone();
                                move |value| Message::EnginePropertyTextChanged(key.clone(), value)
                            })
                            .width(Fill),
                        text(if color.is_some() {
                            ""
                        } else {
                            tr("r5-preview-unavailable")
                        })
                        .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
                    ]
                    .spacing(8.0 * scale),
                    panel_text(tr("r5-colour-format"), scale)
                ]
                .spacing(4.0 * scale);
                if let Some(rgb) = color {
                    for (channel, caption) in ["R", "G", "B"].into_iter().enumerate() {
                        let channel_key = key.clone();
                        editor = editor.push(
                            row![
                                panel_text(caption, scale).width(24.0 * scale),
                                slider(0.0..=1.0, rgb[channel], move |next| {
                                    let mut updated = rgb;
                                    updated[channel] = next;
                                    Message::EnginePropertyTextChanged(
                                        channel_key.clone(),
                                        format!("{} {} {}", updated[0], updated[1], updated[2]),
                                    )
                                })
                                .step(0.001_f32)
                                .style(move |_, _| panel_slider_style(palette)),
                                panel_text(format!("{:.3}", rgb[channel]), scale)
                                    .width(48.0 * scale)
                            ]
                            .spacing(8.0 * scale)
                            .align_y(iced::Alignment::Center),
                        );
                    }
                }
                editor.into()
            }
        };
        let title = panel_text(&property.label, scale)
            .size(13.0 * crate::donor_folio::source_ui::legible_type_scale(scale));
        let compact =
            super::settings::theme_designer::reading_width(app.viewport, scale) < 780.0 * scale;
        let field: Element<'_, Message> = if compact {
            column![title, control].spacing(9.0 * scale).into()
        } else {
            row![
                container(title).width(218.0 * scale),
                container(control).width(Fill)
            ]
            .spacing(20.0 * scale)
            .into()
        };
        property_panel =
            property_panel.push(column![donor_folio::rule(palette), field].spacing(12.0 * scale));
    }
    if let Some(diagnostic) = &app.engine_properties_diagnostic {
        property_panel = property_panel.push(
            panel_text(diagnostic.clone(), scale)
                .size(12.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                .color(palette.tertiary),
        );
    } else if app.engine_properties.is_empty() {
        property_panel = property_panel.push(
            panel_text(tr("r5-engine-empty"), scale)
                .size(12.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
        );
    }
    let footer = row![
        text(if app.launch_policy.is_preview() {
            tr("r5-engine-preview")
        } else {
            tr("r5-engine-scope")
        })
        .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
        .width(Fill),
        panel_button(
            panel_text(
                if app.apply_pending {
                    tr("r5-engine-applying")
                } else {
                    tr("r5-engine-apply")
                },
                scale
            ),
            palette,
            scale
        )
        .on_press_maybe(
            (app.apply_enabled && !app.apply_pending && app.engine_properties_project.is_some())
                .then_some(Message::ApplyEngineProperties)
        )
        .style(move |_, status| donor_ui::action_style(palette, true, false, status))
    ]
    .spacing(12.0 * scale);
    let index = donor_folio::index(
        "Playback",
        "Wallpaper Engine / properties",
        text(format!("{} supported fields", app.engine_properties.len()))
            .size(13.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
            .into(),
        palette,
        scale,
    );
    let page = donor_folio::page(
        tr("r5-engine-title"),
        tr("r5-engine-note"),
        property_panel.into(),
        None,
        Some(footer.into()),
        palette,
        donor_folio::BlueprintKind::Folio,
        scale,
    );
    donor_folio::sheet(
        app.viewport,
        Some(index),
        page,
        palette,
        Message::SheetBackdrop,
        Some((
            "Engine / properties".into(),
            Message::ToggleEngineProperties,
        )),
        scale,
    )
}
pub(in crate::candidate_app) fn search_filter_content(app: &GpuCandidate) -> Element<'_, Message> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    let palette = donor_palette(app);
    let input = column![
        panel_text(app.settings_locale.text_static("r5-panel-copy-54"), scale)
            .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
        text_input(
            app.settings_locale.text_static("r5-panel-copy-53"),
            &app.filter_rule.text
        )
        .font(UI_FONT)
        .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
        .padding([7.0 * scale, 10.0 * scale])
        .style(move |_, _| donor_ui::ghost_input_style(palette))
        .id(app.search_input_id.clone())
        .on_input(Message::SearchChanged)
        .padding([9.0 * scale, 10.0 * scale])
        .size(14.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
        .width(Fill)
        .style(move |_, _| donor_ui::ghost_input_style(palette)),
        panel_text(
            format!(
                "{} matches in the {} item catalog",
                app.filtered_indices.len(),
                app.wallpapers.len()
            ),
            scale
        )
        .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
        .color(palette.primary)
    ]
    .spacing(9.0 * scale);
    let filters = column![
        row![
            panel_text(app.settings_locale.text_static("r5-panel-copy-47"), scale).width(Fill),
            panel_button(
                panel_text(sort_label(app.sort_order), scale),
                palette,
                scale
            )
            .on_press(Message::CycleSort)
            .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
            panel_button(
                panel_text(
                    if app.filter_rule.favorites_only {
                        "✓ Favorites"
                    } else {
                        app.settings_locale.text_static("r5-panel-copy-00")
                    },
                    scale
                ),
                palette,
                scale
            )
            .on_press(Message::ToggleFavoritesFilter)
            .style(move |_, status| donor_ui::action_style(
                palette,
                app.filter_rule.favorites_only,
                false,
                status
            ))
        ]
        .spacing(8.0 * scale),
        row![
            panel_text(app.settings_locale.text_static("r5-panel-copy-48"), scale).width(Fill),
            panel_button(
                panel_text(
                    if app.show_excluded {
                        app.settings_locale.text_static("r5-panel-copy-55")
                    } else {
                        app.settings_locale.text_static("r5-panel-copy-56")
                    },
                    scale
                ),
                palette,
                scale
            )
            .on_press(Message::ToggleShowExcluded)
            .style(move |_, status| donor_ui::action_style(
                palette,
                app.show_excluded,
                false,
                status
            ))
        ]
        .spacing(8.0 * scale),
        row![
            panel_text(app.settings_locale.text_static("r5-panel-copy-49"), scale).width(Fill),
            panel_button(panel_text(app.resolution_label(), scale), palette, scale)
                .on_press(Message::CycleResolution)
                .style(move |_, status| donor_ui::action_style(palette, false, false, status)),
            panel_button(
                panel_text(orientation_label(app.filter_rule.orientation), scale),
                palette,
                scale
            )
            .on_press(Message::CycleOrientation)
            .style(move |_, status| donor_ui::action_style(
                palette,
                app.filter_rule.orientation != OrientationFilter::Any,
                false,
                status
            ))
        ]
        .spacing(8.0 * scale),
    ]
    .spacing(14.0 * scale);
    column![
        donor_folio::section(
            app.settings_locale.text_static("r5-panel-copy-50"),
            "",
            input.into(),
            palette,
            scale
        ),
        donor_folio::section(
            app.settings_locale.text_static("r5-panel-copy-51"),
            app.settings_locale.text_static("r5-panel-copy-52"),
            column![filters].spacing(12.0 * scale).into(),
            palette,
            scale
        )
    ]
    .spacing(28.0 * scale)
    .into()
}
// One measured popup model drives draw, hit, scroll, footprint and rail anchoring.
pub(in crate::candidate_app) fn folder_popup_model(
    app: &GpuCandidate,
) -> crate::donor_folder::Model {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    let palette = donor_palette(app);
    let rail = super::gallery_rail_model(app);
    let roots = app
        .config
        .catalog
        .local_folders
        .iter()
        .cloned()
        .map(crate::candidate_app::catalog::folder_filter_path)
        .collect::<Vec<_>>();
    crate::donor_folder::Model::new(
        &app.available_folders(),
        &roots,
        &app.filter_rule.folders.iter().cloned().collect::<Vec<_>>(),
        app.viewport,
        scale,
        rail.mode,
        rail.rtl,
        palette.surface,
        palette.primary,
        palette.surface_text,
        UI_FONT,
        NERD_FONT,
    )
}
#[cfg(test)]
pub(in crate::candidate_app) fn folder_popup_size(app: &GpuCandidate) -> [f32; 2] {
    let model = folder_popup_model(app);
    [model.width, model.height]
}

pub(in crate::candidate_app) fn folder_popup_view(
    model: crate::donor_folder::Model,
) -> Element<'static, Message> {
    crate::donor_folder::view(model).map(Message::SetFolderFilter)
}

pub(super) fn search_panel_view(
    app: &GpuCandidate,
    palette: donor_ui::Palette,
) -> Element<'_, Message> {
    use iced::widget::{opaque, text::Wrapping};
    if app.tag_drawer_item.is_some() {
        return local_tag_editor_view(app, palette);
    }
    let tr = |key| app.settings_locale.text_static(key);
    let scale = view::settings::bridge::effective_ui_scale(app);
    let type_scale = donor_folio::source_ui::legible_type_scale(scale);
    let width = app.search_panel_width();
    let inset = 18.0 * scale;
    let inner_width = (width - 2.0 * inset).max(1.0);
    let gap = 8.0 * scale;
    let toolbar_gap = 7.0 * scale;
    let toolbar_height = 27.0 * scale.max(0.8);
    let toolbar_track = 6.0 * scale.max(1.0);
    let header_height = toolbar_height + toolbar_track;
    let control_font = iced::Font::MONOSPACE;
    let control_size = 9.5 * type_scale;
    let control_width = |label| {
        donor_back_layout::measured_width(label, control_size, control_font).ceil()
            + 20.0 * scale
            + 1.0
    };
    let control = |label, active, destructive, message: Option<Message>| {
        let control = panel_button(
            container(
                text(label)
                    .font(control_font)
                    .size(control_size)
                    .shaping(iced::widget::text::Shaping::Advanced)
                    .wrapping(Wrapping::None),
            )
            .center_x(Fill)
            .center_y(Fill),
            palette,
            scale,
        )
        .width(control_width(label))
        .height(toolbar_height)
        .padding([0.0, 10.0 * scale])
        .style(move |_, status| {
            donor_ui::action_style(
                palette,
                active,
                destructive,
                if active && matches!(status, button::Status::Disabled) {
                    button::Status::Active
                } else {
                    status
                },
            )
        });
        match message {
            Some(message) => control.on_press(message),
            None => control,
        }
    };
    let dim = donor_ui::with_alpha_color(palette.surface_text, 0.62);
    let caption = |label| {
        text(label)
            .font(UI_FONT)
            .size(control_size)
            .shaping(iced::widget::text::Shaping::Advanced)
            .wrapping(Wrapping::None)
            .color(dim)
    };
    let match_label = tr("tags-match");
    let order_label = tr("tags-order");
    let all = tr("r5-panel-copy-06");
    let any = tr("r5-panel-copy-07");
    let popular = tr("r5-panel-copy-09");
    let az = "A–Z";
    let toggle = if app.search_tags_open {
        tr("r5-panel-copy-04")
    } else {
        tr("r5-panel-copy-05")
    };
    let clear = tr("search-clear-tags");
    let mut actions = row![
        caption(match_label),
        control(
            all,
            !app.filter_rule.tags_match_any,
            false,
            Some(Message::SetTagMatchAny(false))
        ),
        control(
            any,
            app.filter_rule.tags_match_any,
            false,
            Some(Message::SetTagMatchAny(true))
        ),
        caption(order_label),
        control(
            popular,
            !app.search_tags_sort_az,
            false,
            app.search_tags_sort_az.then_some(Message::ToggleTagSort)
        ),
        control(
            az,
            app.search_tags_sort_az,
            false,
            (!app.search_tags_sort_az).then_some(Message::ToggleTagSort)
        ),
    ];
    let action_widths = [all, any, popular, az, toggle, "ESC"].map(control_width);
    let mut widest_action = action_widths.into_iter().fold(0.0_f32, f32::max);
    let mut toolbar_width = action_widths.into_iter().sum::<f32>()
        + [match_label, order_label]
            .into_iter()
            .map(|label| {
                donor_back_layout::measured_width(label, control_size, UI_FONT).ceil() + 1.0
            })
            .sum::<f32>()
        + 7.0 * toolbar_gap;
    if !app.filter_rule.tags.is_empty() || !app.filter_rule.excluded_tags.is_empty() {
        actions = actions.push(control(clear, false, true, Some(Message::ClearSearchTags)));
        toolbar_width += control_width(clear) + toolbar_gap;
        widest_action = widest_action.max(control_width(clear));
    }
    actions = actions
        .push(control(
            toggle,
            app.search_tags_open,
            false,
            Some(Message::ToggleSearchTags),
        ))
        .push(
            mouse_area(
                container(caption("ESC"))
                    .center_x(control_width("ESC"))
                    .center_y(toolbar_height),
            )
            .on_press(Message::CloseSearchPanel),
        )
        .spacing(toolbar_gap)
        .align_y(Alignment::Center);

    let title_size = 10.5 * type_scale;
    let title_gap = 4.0 * scale;
    let heading_width = |value: &str| {
        donor_back_layout::measured_width(value, title_size, control_font).ceil() + 1.0
    };
    let count = format!("· {}", app.filtered_indices.len());
    let count_width = heading_width(&count);
    // Reserve the full count and a whole toolbar action before fitting the title.
    let title_max_width =
        (inner_width - widest_action - count_width - title_gap - 2.0 * toolbar_gap).max(1.0);
    let title = donor_back_layout::measured_lines(
        tr("tags-filter-title"),
        title_size,
        title_max_width,
        control_font,
        1,
    )
    .into_iter()
    .next()
    .unwrap_or_default();
    let title_width = heading_width(&title).min(title_max_width);
    let heading = |label, width| {
        text(label)
            .font(control_font)
            .size(title_size)
            .width(width)
            .shaping(iced::widget::text::Shaping::Advanced)
            .wrapping(Wrapping::None)
            .color(donor_ui::with_alpha_color(palette.surface_text, 0.82))
    };
    let toolbar = row![
        row![heading(title, title_width), heading(count, count_width)].spacing(title_gap),
        Space::new().width(Fill),
        scrollable(actions)
            .id("search-tag-toolbar-scroll")
            .direction(iced::widget::scrollable::Direction::Horizontal(
                iced::widget::scrollable::Scrollbar::default()
                    .width(toolbar_track)
                    .scroller_width(toolbar_track),
            ))
            .width(toolbar_width.min(
                (inner_width - title_width - count_width - title_gap - 2.0 * toolbar_gap).max(1.0)
            ))
            .height(header_height),
    ]
    .spacing(toolbar_gap)
    .align_y(Alignment::Center)
    .width(Fill)
    .height(header_height);

    let query_size = 13.0 * type_scale;
    let line_height = 1.25;
    let query_height = (46.0 * scale).max(query_size * line_height + 14.0 * scale);
    let field = container(
        row![
            control(
                tr("tags-search-mode-tags"),
                true,
                false,
                Some(Message::CloseSearchPanel)
            ),
            text("\u{f0349}")
                .font(NERD_FONT)
                .size(16.0 * scale)
                .color(donor_ui::with_alpha_color(palette.surface_text, 0.82)),
            text_input(tr("r5-search-placeholder"), &app.filter_rule.text)
                .id(app.search_input_id.clone())
                .on_input(Message::SearchChanged)
                .font(UI_FONT)
                .size(query_size)
                .line_height(line_height)
                .padding([7.0 * scale, 3.0 * scale])
                .width(Fill)
                .style(move |_, _| donor_ui::ghost_input_style(palette)),
        ]
        .spacing(toolbar_gap)
        .align_y(Alignment::Center)
        .height(Fill),
    )
    .id("search-tag-field")
    .width(Fill)
    .height(query_height)
    .padding([0.0, 12.0 * scale])
    .style(move |_| {
        donor_ui::box_style(
            donor_ui::with_alpha_color(palette.surface_variant, 0.94),
            donor_ui::with_alpha_color(palette.primary, 0.5),
        )
    });

    let help_size = 9.0 * type_scale;
    let help = tr("search-tag-counts-help");
    let help_height = donor_search::wrapped_height(help, help_size, inner_width, UI_FONT);
    let help = text(help)
        .font(UI_FONT)
        .size(help_size)
        .line_height(line_height)
        .shaping(iced::widget::text::Shaping::Advanced)
        .wrapping(Wrapping::WordOrGlyph)
        .width(Fill)
        .height(help_height)
        .color(dim);
    let mut height = 20.0 * scale + header_height + query_height + help_height + 2.0 * gap;
    let mut content = column![toolbar, field, help].spacing(gap).height(Fill);
    if app.search_tags_open {
        let scrollbar = 10.0;
        let body_width = (inner_width - scrollbar).max(1.0);
        let chip_size = 11.0 * type_scale;
        let chip_padding = 12.0 * scale;
        let chip_height = (chip_size * line_height + 14.0 * scale).ceil();
        let chip_gap = 6.0 * scale;
        let empty = tr("r5-panel-copy-10");
        let mut tag_rows = column![].spacing(chip_gap).padding(iced::Padding {
            right: scrollbar,
            ..iced::Padding::ZERO
        });
        let mut chips = Vec::new();
        let mut widths = Vec::new();
        for (tag, count) in app.search_tag_entries() {
            let excluded = app.filter_rule.excluded_tags.contains(tag);
            let active = excluded || app.filter_rule.tags.contains(tag);
            let prefix = if excluded { "− " } else { "" };
            let suffix = format!("  {count}");
            let max_label_width = (body_width - 2.0 * chip_padding - 1.0).max(0.0);
            let display_tag = if tag.contains(['\n', '\r', '\t']) {
                std::borrow::Cow::Owned(tag.replace(['\n', '\r', '\t'], " "))
            } else {
                std::borrow::Cow::Borrowed(tag)
            };
            let full = format!("{prefix}{display_tag}{suffix}");
            let measure =
                |value: &str| donor_back_layout::measured_width(value, chip_size, UI_FONT);
            let full_width = measure(full.as_str());
            let clipped = full_width.ceil() > max_label_width;
            let (label, label_width) = if clipped {
                let reserved = measure(&format!("{prefix}{suffix}"));
                let shown = donor_back_layout::measured_lines(
                    tag,
                    chip_size,
                    (max_label_width - reserved - 1.0).max(0.0),
                    UI_FONT,
                    1,
                )
                .into_iter()
                .next()
                .unwrap_or_default();
                let label = format!("{prefix}{shown}{suffix}");
                let width = measure(&label);
                (label, width)
            } else {
                (full, full_width)
            };
            let button_width = (label_width.ceil() + 2.0 * chip_padding + 1.0).min(body_width);
            let tag_button = panel_button(
                panel_text(label, scale)
                    .line_height(line_height)
                    .wrapping(Wrapping::None),
                palette,
                scale,
            )
            .width(button_width)
            .height(chip_height)
            .on_press(Message::SearchTag(tag.to_owned(), false))
            .style(move |_, status| donor_ui::action_style(palette, active, excluded, status));
            let tag_button: Element<'_, Message> = mouse_area(tag_button)
                .on_right_press(Message::SearchTag(tag.to_owned(), true))
                .into();
            chips.push(if !clipped && display_tag == tag {
                tag_button
            } else {
                iced::widget::tooltip(
                    tag_button,
                    container(panel_text(tag, scale)).max_width(body_width),
                    iced::widget::tooltip::Position::Top,
                )
                .into()
            });
            widths.push(button_width);
        }
        let body_height = if chips.is_empty() {
            let h = donor_search::wrapped_height(empty, chip_size, body_width, UI_FONT)
                .max(chip_height);
            tag_rows = tag_rows.push(
                panel_text(empty, scale)
                    .line_height(line_height)
                    .wrapping(Wrapping::WordOrGlyph)
                    .width(body_width)
                    .height(h),
            );
            h
        } else {
            let rows = donor_search::chip_rows(&widths, body_width, chip_gap);
            let visible = rows.len().min(app.search_panel_row_limit()) as f32;
            let mut chips = chips.into_iter();
            for row_len in rows {
                tag_rows = tag_rows.push(
                    row(chips.by_ref().take(row_len))
                        .spacing(chip_gap)
                        .height(chip_height),
                );
            }
            visible * chip_height + (visible - 1.0) * chip_gap
        };
        height += gap + body_height;
        content = content.push(
            scrollable(tag_rows)
                .id("r5-search-body")
                .width(Fill)
                .height(Fill)
                .direction(iced::widget::scrollable::Direction::Vertical(
                    iced::widget::scrollable::Scrollbar::default()
                        .width(scrollbar)
                        .scroller_width(scrollbar),
                )),
        );
    }
    let [left, top, width, height] = app.tag_panel_bounds(width, height);
    let panel = container(content)
        .id("search-tag-panel")
        .width(width)
        .height(height)
        .padding([10.0 * scale, inset])
        .style(move |_| {
            donor_ui::box_style(
                donor_ui::with_alpha_color(palette.surface, 0.97),
                donor_ui::with_alpha_color(palette.primary, 0.48),
            )
        });
    container(opaque(
        mouse_area(panel).on_scroll(|_| Message::SheetBackdrop),
    ))
    .padding(iced::Padding {
        top,
        left,
        ..iced::Padding::ZERO
    })
    .into()
}
pub(super) fn local_tag_editor_view(
    app: &GpuCandidate,
    palette: donor_ui::Palette,
) -> Element<'_, Message> {
    use super::super::local_library::TagEdit;
    let tr = |key| app.settings_locale.text_static(key);
    let scale = view::settings::bridge::effective_ui_scale(app);
    let Some(owner) = app.tag_owner() else {
        return Space::new().into();
    };
    let entry = app.catalog.iter().find(|e| e.id == owner.item_id());
    let wallpaper = app.wallpapers.iter().find(|w| w.id == owner.item_id());
    let width = (520.0 * scale).min((app.viewport[0] - 40.0).max(1.0));
    let height = (400.0 * scale).min((app.viewport[1] - 80.0).max(160.0));
    let [left, top, width, height] = app.tag_panel_bounds(width, height);
    let add = Message::LocalTag {
        owner: owner.clone(),
        edit: TagEdit::Add,
    };
    let text_owner = owner.clone();
    let mut entries = column![].spacing(5.0 * scale);
    if let Some(entry) = entry {
        for (index, tag) in entry.tags.iter().enumerate() {
            let line = row![
                text(format!("{:02}", index + 1))
                    .font(iced::Font::MONOSPACE)
                    .size(9.5 * crate::donor_folio::source_ui::legible_type_scale(scale))
                    .color(palette.primary),
                panel_text(tag.clone(), scale).width(Fill),
                panel_button(panel_text("×", scale), palette, scale).on_press(Message::LocalTag {
                    owner: owner.clone(),
                    edit: TagEdit::Remove(tag.clone())
                }),
            ]
            .spacing(10.0 * scale)
            .align_y(iced::Alignment::Center);
            entries = entries.push(
                container(line)
                    .padding([5.0 * scale, 8.0 * scale])
                    .width(Fill)
                    .style(move |_| {
                        donor_ui::box_style(
                            donor_ui::with_alpha_color(palette.surface_variant, 0.34),
                            donor_ui::with_alpha_color(palette.outline, 0.28),
                        )
                    }),
            );
        }
        if entry.tags.is_empty() {
            entries = entries.push(panel_text(tr("tags-empty"), scale));
        }
    } else {
        entries = entries.push(panel_text(tr("tags-unavailable"), scale));
    }
    let title = wallpaper.map_or(tr("tags-unavailable"), |w| w.title.as_str());
    let title_size = 11.0 * crate::donor_folio::source_ui::legible_type_scale(scale);
    let title_lines = crate::donor_back_layout::measured_lines(
        title,
        title_size,
        (width - 24.0 * scale).max(80.0),
        UI_FONT,
        2,
    );
    let content = column![
        row![
            text(tr("tags-local-title"))
                .font(iced::Font::MONOSPACE)
                .size(10.5 * crate::donor_folio::source_ui::legible_type_scale(scale))
                .color(palette.primary),
            Space::new().width(Fill),
            panel_button(panel_text(tr("tags-back"), scale), palette, scale)
                .on_press(Message::BackToSearchFilters),
            panel_button(panel_text("×", scale), palette, scale)
                .on_press(Message::CloseSearchPanel)
        ]
        .spacing(8.0 * scale)
        .align_y(iced::Alignment::Center),
        panel_text(title_lines.join("\n"), scale),
        panel_text(format!("ID · {}", owner.item_id()), scale),
        scrollable(entries).height(Fill),
        row![
            text_input(
                app.settings_locale.text_static("r5-tag-placeholder"),
                &app.local_tag_input
            )
            .id(app.local_tag_input_id.clone())
            .font(UI_FONT)
            .size(13.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
            .padding([7.0 * scale, 8.0 * scale])
            .style(move |_, _| donor_ui::ghost_input_style(palette))
            .on_input(move |raw| Message::LocalTag {
                owner: text_owner.clone(),
                edit: TagEdit::Text(raw)
            })
            .on_submit(add.clone())
            .width(Fill),
            panel_button(
                panel_text(app.settings_locale.text_static("r5-add"), scale),
                palette,
                scale
            )
            .on_press(add)
        ]
        .spacing(11.0 * scale),
        panel_text(tr("tags-local-help"), scale),
    ]
    .spacing(12.0 * scale)
    .height(Fill);
    let panel = container(content)
        .id("local-tag-editor")
        .width(width)
        .height(height)
        .padding(12.0 * scale)
        .style(move |_| {
            donor_ui::box_style(
                donor_ui::with_alpha_color(palette.surface, 0.97),
                donor_ui::with_alpha_color(palette.primary, 0.72),
            )
        });
    container(iced::widget::opaque(
        iced::widget::mouse_area(panel).on_press(Message::SheetBackdrop),
    ))
    .padding(iced::Padding {
        top,
        left,
        ..iced::Padding::ZERO
    })
    .into()
}

/// Rail browsing is transient and never shares the captured membership picker.
pub(super) fn playlist_chooser_view<'a>(
    app: &'a GpuCandidate,
    retiring: Option<&'a playlists::RetiringPlaylist>,
    opacity: f32,
) -> Element<'a, Message> {
    use super::super::playlists::Browse;
    let Some(owner) = retiring.map_or_else(|| app.playlist_owner(), |old| old.owner.clone()) else {
        return Space::new().into();
    };
    let palette = donor_palette(app).opacity(opacity);
    let browse_id = retiring.map_or(app.browse_playlist_id.as_deref(), |old| {
        old.browse_id.as_deref()
    });
    let query_input = retiring.map_or(app.playlist_query.as_str(), |old| old.query.as_str());
    let scale = view::settings::bridge::effective_ui_scale(app);
    let rtl = app.settings_locale.language.rtl;
    let type_scale = donor_folio::source_ui::legible_type_scale(scale);
    let tr = |key| app.settings_locale.text_static(key);
    let message = |action| Message::PlaylistBrowse(owner.clone(), action);
    let query = query_input.trim().to_lowercase();
    let mut choices = column![donor_folio::source_ui::folio_action(
        rtl,
        tr("playlists-browse-all"),
        browse_id.is_none(),
        Some(message(Browse::Choose(None))),
        Fill,
        scale,
        &palette,
    )]
    .spacing(5.0 * scale);
    let mut matches = 0;
    for playlist in &app.state.playlists {
        if !query.is_empty()
            && !playlist.name.to_lowercase().contains(&query)
            && !playlist.id.to_lowercase().contains(&query)
        {
            continue;
        }
        matches += 1;
        let active = browse_id == Some(&playlist.id);
        let name = if playlist.name.is_empty() {
            tr("playlists-unnamed")
        } else {
            &playlist.name
        };
        choices = choices.push(donor_folio::source_ui::folio_action(
            rtl,
            format!("{name} · {}", playlist.id),
            active,
            Some(message(Browse::Choose(Some(playlist.id.clone())))),
            Fill,
            scale,
            &palette,
        ));
    }
    if matches == 0 {
        choices = choices.push(
            panel_text(
                tr(if app.state.playlists.is_empty() {
                    "playlists-index-empty-title"
                } else {
                    "playlists-picker-no-match"
                }),
                scale,
            )
            .width(Fill),
        );
    }
    let input_owner = owner.clone();
    let content = column![
        row![
            panel_text(tr("playlists-choose"), scale)
                .size(16.0 * type_scale)
                .color(palette.primary)
                .width(Fill),
            donor_folio::source_ui::folio_action(
                rtl,
                "×",
                false,
                Some(message(Browse::Close)),
                Length::Shrink,
                scale,
                &palette,
            ),
        ]
        .align_y(Alignment::Center)
        .spacing(12.0 * scale),
        panel_text(tr("playlists-picker-hint"), scale).width(Fill),
        donor_folio::source_ui::folio_ghost_field(
            query_input,
            tr("playlists-picker-search"),
            move |value| Message::PlaylistBrowse(input_owner.clone(), Browse::Query(value)),
            Message::SheetBackdrop,
            Fill,
            scale,
            &palette,
        )
        .id(PLAYLIST_CHOOSER_QUERY_ID),
        scrollable(container(choices).width(Fill).padding(iced::Padding {
            right: 12.0,
            ..iced::Padding::ZERO
        }))
        .id("playlist-browse-lists")
        .height(Fill)
        .width(Fill)
        .direction(donor_folio::source_ui::thin_vbar())
        .style(donor_folio::source_ui::scroll_style(palette.primary)),
        donor_folio::source_ui::folio_action(
            rtl,
            tr("playlists-edit"),
            false,
            Some(message(Browse::Edit)),
            Fill,
            scale,
            &palette,
        ),
    ]
    .spacing(12.0 * scale);
    let panel = container(content)
        .id("playlist-browse-panel")
        .width((440.0 * scale).min((app.viewport[0] - 32.0).max(1.0)))
        .height((480.0 * scale).min((app.viewport[1] - 32.0).max(1.0)))
        .padding(18.0 * scale)
        .style(move |_| donor_ui::box_style(palette.surface, palette.outline));
    let scrim = container(mouse_area(panel).on_press(Message::SheetBackdrop))
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill)
        .style(move |_| donor_folio::source_ui::folio_scrim_style(opacity));
    mouse_area(scrim).on_press(message(Browse::Close)).into()
}

pub(super) fn playlist_picker_view<'a>(
    app: &'a GpuCandidate,
    retiring: Option<&'a playlists::RetiringPlaylist>,
    opacity: f32,
) -> Element<'a, Message> {
    use super::super::playlists::Edit;
    let Some(owner) = retiring.map_or_else(|| app.playlist_owner(), |old| old.owner.clone()) else {
        return Space::new().into();
    };
    let palette = donor_palette(app).opacity(opacity);
    let name = retiring.map_or(app.playlist_name.as_str(), |old| old.name.as_str());
    let scale = view::settings::bridge::effective_ui_scale(app);
    let type_scale = donor_folio::source_ui::legible_type_scale(scale);
    let tr = |key| app.settings_locale.text_static(key);
    let message = |edit| Message::PlaylistEdit(owner.clone(), edit);
    let selected = retiring.map_or_else(
        || app.selected.and_then(|i| app.wallpapers.get(i)),
        |old| {
            app.wallpapers
                .iter()
                .find(|item| Some(item.id.as_str()) == old.selected_id.as_deref())
        },
    );
    let header = row![
        panel_text(tr("playlists-card-add-title"), scale)
            .size(16.0 * type_scale)
            .width(Fill),
        panel_button(panel_text("×", scale), palette, scale).on_press(message(Edit::Close)),
    ]
    .spacing(12.0 * scale)
    .align_y(Alignment::Center);
    let mut memberships = column![].spacing(6.0 * scale);
    let mut manual = 0;
    for playlist in &app.state.playlists {
        let PlaylistMembership::Explicit { wallpaper_ids } = &playlist.membership else {
            continue;
        };
        manual += 1;
        let included = selected.is_some_and(|w| wallpaper_ids.contains(&w.id));
        memberships = memberships.push(
            panel_button(
                row![
                    panel_text(if included { "✓" } else { "＋" }, scale).width(18.0 * type_scale),
                    panel_text(&playlist.name, scale).width(Fill),
                ]
                .spacing(8.0 * scale)
                .align_y(Alignment::Center),
                palette,
                scale,
            )
            .width(Fill)
            .on_press(message(Edit::Membership {
                playlist: playlist.id.clone(),
                included: !included,
            }))
            .style(move |_, status| donor_ui::action_style(palette, included, false, status)),
        );
    }
    if manual == 0 {
        memberships = memberships.push(panel_text(tr("playlist-picker-empty"), scale));
    }
    let input_owner = owner.clone();
    let create = column![
        donor_folio::rule(palette),
        panel_text(tr("r5-new-playlist"), scale).color(palette.primary),
        row![
            text_input(tr("playlists-new-name"), name)
                .on_input(move |name| Message::PlaylistEdit(input_owner.clone(), Edit::Name(name)))
                .on_submit(message(Edit::Create))
                .font(UI_FONT)
                .size(12.0 * type_scale)
                .padding([7.0 * scale, 10.0 * scale])
                .style(move |_, _| donor_ui::field_input_style(palette))
                .width(Fill),
            panel_button(panel_text(tr("r5-create"), scale), palette, scale)
                .on_press_maybe((!name.trim().is_empty()).then(|| message(Edit::Create))),
        ]
        .spacing(8.0 * scale)
        .align_y(Alignment::Center),
    ]
    .spacing(10.0 * scale);
    // Keep dismissal and creation reachable while long membership lists scroll independently.
    let content = column![
        header,
        donor_folio::rule(palette),
        container(
            scrollable(
                container(
                    column![
                        panel_text(tr("r5-selected-wallpaper"), scale)
                            .color(donor_ui::with_alpha_color(palette.surface_text, 0.65)),
                        panel_text(selected.map_or("", |w| w.title.as_str()), scale)
                            .size(15.0 * type_scale),
                        memberships,
                    ]
                    .spacing(12.0 * scale)
                )
                .width(Fill)
                .padding(iced::Padding {
                    right: 12.0,
                    ..iced::Padding::ZERO
                })
            )
            .id("playlist-picker-memberships")
            .height(Length::Shrink)
            .width(Fill)
        )
        .max_height(((app.viewport[1] * 0.7).clamp(240.0, 560.0) - 180.0 * type_scale).max(80.0)),
        create,
    ]
    .spacing(14.0 * scale);
    let panel = container(content)
        .width(Length::Fixed((app.viewport[0] - 32.0).clamp(240.0, 420.0)))
        .max_height((app.viewport[1] * 0.7).clamp(240.0, 560.0))
        .padding(18.0 * scale)
        .style(move |_| donor_ui::panel_style(palette, 1.0));
    let scrim = container(mouse_area(panel).on_press(Message::SheetBackdrop))
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill)
        .style(move |_| donor_ui::scrim_style(0.55 * opacity));
    mouse_area(scrim).on_press(message(Edit::Close)).into()
}

fn playlist_delete_view<'a>(
    app: &'a GpuCandidate,
    target: &'a playlists::Deletion,
    owner: Option<playlists::Owner>,
    opacity: f32,
) -> Element<'a, Message> {
    use super::super::playlists::Edit;
    let Some(owner) = owner else {
        return Space::new().into();
    };
    let palette = donor_palette(app).opacity(opacity);
    let scale = view::settings::bridge::effective_ui_scale(app);
    let tr = |key| app.settings_locale.text_static(key);
    let message = |edit| Message::PlaylistEdit(owner.clone(), edit);
    let type_scale = donor_folio::source_ui::legible_type_scale(scale);
    let content = column![
        row![
            panel_text(tr("playlist-delete-action"), scale)
                .size(16.0 * type_scale)
                .width(Fill),
            panel_button(panel_text("×", scale), palette, scale)
                .on_press(message(Edit::DeleteCancel)),
        ]
        .spacing(12.0 * scale)
        .align_y(Alignment::Center),
        donor_folio::rule(palette),
        container(
            scrollable(
                column![
                    panel_text(&target.playlist.name, scale).size(22.0 * type_scale),
                    panel_text(tr("playlist-delete-record-only"), scale),
                    panel_text(
                        if target.active {
                            tr("playlist-delete-active")
                        } else {
                            tr("playlist-delete-other")
                        },
                        scale
                    ),
                    panel_text(
                        format!(
                            "{}: {}",
                            tr("playlist-delete-schedules"),
                            target.schedules.len()
                        ),
                        scale
                    ),
                ]
                .spacing(14.0 * scale)
            )
            .id("playlist-delete-consequences")
            .height(Length::Shrink)
        )
        .max_height((app.viewport[1] - 200.0 * type_scale).max(80.0)),
        donor_folio::rule(palette),
        row![
            panel_button(
                panel_text(tr("playlist-delete-cancel"), scale),
                palette,
                scale
            )
            .on_press(message(Edit::DeleteCancel)),
            Space::new().width(Fill),
            panel_button(
                panel_text(tr("playlist-delete-action"), scale),
                palette,
                scale
            )
            .on_press(message(Edit::DeleteConfirm))
            .style(move |_, status| donor_ui::action_style(palette, false, true, status)),
        ]
        .spacing(12.0 * scale)
        .align_y(Alignment::Center),
    ]
    .spacing(14.0 * scale);
    let panel = container(content)
        .width(Length::Fixed((app.viewport[0] - 32.0).clamp(240.0, 500.0)))
        .max_height((app.viewport[1] - 40.0).max(180.0))
        .padding(18.0 * scale)
        .style(move |_| donor_ui::panel_style(palette, 1.0));
    let scrim = container(mouse_area(panel).on_press(Message::SheetBackdrop))
        .width(Fill)
        .height(Fill)
        .center_x(Fill)
        .center_y(Fill)
        .style(move |_| donor_ui::scrim_style(0.55 * opacity));
    mouse_area(scrim)
        .on_press(message(Edit::DeleteCancel))
        .into()
}

pub(super) fn playlist_panel_view<'a>(
    app: &'a GpuCandidate,
    selected: Option<(usize, &'a WallpaperItem)>,
    retiring: Option<&'a playlists::RetiringPlaylist>,
    opacity: f32,
) -> Element<'a, Message> {
    let owner = || retiring.map_or_else(|| app.playlist_owner(), |old| old.owner.clone());
    if let Some(target) = retiring.map_or(app.playlist_delete.as_ref(), |old| old.deletion.as_ref())
    {
        return playlist_delete_view(app, target, owner(), opacity);
    }
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    let palette = donor_palette(app).opacity(opacity);
    let name = retiring.map_or(app.playlist_name.as_str(), |old| old.name.as_str());
    let type_scale = donor_folio::source_ui::legible_type_scale(scale);
    let tr = |key| app.settings_locale.text_static(key);
    let browsing_id = retiring
        .map_or(app.playlist_browsing_id.as_deref(), |old| {
            old.editor_id.as_deref()
        })
        .filter(|id| app.state.playlists.iter().any(|p| p.id == *id))
        .or(app.state.active_playlist_id.as_deref())
        .or_else(|| app.state.playlists.first().map(|p| p.id.as_str()));
    let [sheet_width, _, index_width] = donor_folio::dimensions(app.viewport, scale);
    let page_width = sheet_width - index_width - 64.0 * scale;
    let stacked = page_width < 560.0 * type_scale;
    let sequence_width = 380.0 * scale.max(0.9);
    let fixed_sequence =
        !stacked && page_width >= sequence_width + 360.0 * type_scale + 28.0 * scale + 1.0;
    // A one-pixel rule needs zero border; box_style's border consumes its fill.
    let rule = || {
        container(Space::new())
            .width(Fill)
            .height(1)
            .style(move |_| iced::widget::container::Style {
                background: Some(donor_ui::with_alpha_color(palette.outline, 0.34).into()),
                ..Default::default()
            })
    };
    let mut lists = column![].spacing(4.0 * scale);
    let mut browsed = None;
    if app.state.playlists.is_empty() {
        lists = lists
            .push(
                panel_text(tr("r5-playlist-empty"), scale)
                    .size(13.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
            )
            .push(panel_text(tr("r5-playlist-create-hint"), scale));
    }
    for playlist in &app.state.playlists {
        let active =
            app.state.active_playlist_id.as_deref() == Some(&playlist.id) && app.playlist_playing();
        let browsing = browsing_id == Some(playlist.id.as_str());
        let entries = library::evaluate_playlist(playlist, &app.catalog, &app.state.favorites);
        let count = entries.len();
        if browsing && browsed.is_none() {
            browsed = Some((playlist, entries));
        }
        let kind = if matches!(playlist.membership, PlaylistMembership::Smart { .. }) {
            tr("r5-smart")
        } else {
            tr("r5-explicit")
        };
        let label = donor_back_layout::measured_lines(
            &playlist.name,
            12.0 * type_scale,
            (index_width - 69.0 * scale - 12.0 - 11.0 * type_scale).max(20.0),
            UI_FONT,
            2,
        )
        .join("\n");
        lists = lists.push(
            iced::widget::tooltip(
                button(
                    row![
                        panel_text(if browsing { "◆" } else { "◇" }, scale).color(palette.primary),
                        column![
                            panel_text(label, scale)
                                .size(12.0 * type_scale)
                                .wrapping(iced::widget::text::Wrapping::None),
                            panel_text(format!("{kind} · {count:02}"), scale)
                                .size(
                                    10.0 * crate::donor_folio::source_ui::legible_type_scale(scale)
                                )
                                .color(donor_ui::with_alpha_color(palette.surface_text, 0.56)),
                            panel_text(if active { tr("r5-active-playback") } else { "" }, scale)
                                .size(10.0 * type_scale)
                                .color(palette.primary)
                        ]
                        .spacing(3.0 * scale)
                        .width(Fill)
                    ]
                    .spacing(9.0 * scale)
                    .align_y(iced::Alignment::Center),
                )
                .width(Fill)
                .padding([9.0 * scale, 10.0 * scale])
                .on_press(Message::BrowsePlaylist(playlist.id.clone()))
                .style(move |_, status| donor_ui::line_style(palette, browsing, false, status)),
                container(panel_text(&playlist.name, scale))
                    .max_width((app.viewport[0] - 40.0).min(640.0)),
                iced::widget::tooltip::Position::Right,
            )
            .style(move |_| donor_ui::box_style(palette.background, palette.outline)),
        );
    }
    let create = column![
        rule(),
        row![
            panel_text(tr("r5-new-playlist"), scale).color(palette.primary),
            Space::new().width(Fill),
            panel_text(format!("{:02}", app.state.playlists.len()), scale)
        ]
        .align_y(iced::Alignment::Center),
        text_input(tr("r5-playlist-name"), name)
            .on_input(Message::PlaylistNameChanged)
            .font(UI_FONT)
            .size(11.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
            .padding([7.0 * scale, 10.0 * scale])
            .width(Fill)
            .style(move |_, _| donor_ui::ghost_input_style(palette)),
        row![
            container(
                panel_button(panel_text(tr("r5-create"), scale), palette, scale).on_press_maybe(
                    (!name.trim().is_empty()).then_some(Message::CreatePlaylist { smart: false })
                )
            )
            .id(iced::widget::Id::new("playlist-create-manual-action")),
            container(
                panel_button(panel_text(tr("r5-create-smart"), scale), palette, scale)
                    .on_press_maybe(
                        (!name.trim().is_empty())
                            .then_some(Message::CreatePlaylist { smart: true })
                    )
            )
            .id(iced::widget::Id::new("playlist-create-smart-action"))
        ]
        .spacing(6.0 * scale)
        .wrap(),
    ]
    .spacing(9.0 * scale);
    let navigation = column![
        scrollable(container(lists).padding(iced::Padding {
            right: 12.0,
            ..iced::Padding::ZERO
        }))
        .id("playlist-navigation")
        .height(Fill),
        create
    ]
    .spacing(14.0 * scale);
    let list_index = container(
        column![
            panel_text(tr("r5-playlists"), scale).size(20.0 * type_scale),
            panel_text(tr("r5-playlist-index-note"), scale)
                .color(donor_ui::with_alpha_color(palette.surface_text, 0.6)),
            navigation,
        ]
        .spacing(14.0 * scale),
    )
    .padding([20.0 * scale, 20.0 * scale])
    .height(Fill)
    .width(Fill)
    .id(iced::widget::Id::new("playlist-index-region"))
    .style(move |_| donor_ui::index_style(palette))
    .into();

    let mut reading: Element<'a, Message> = panel_text(tr("r5-playlist-reading-empty"), scale)
        .size(22.0 * type_scale)
        .into();
    if let Some((playlist, entries)) = browsed {
        let editable = matches!(playlist.membership, PlaylistMembership::Explicit { .. });
        let member = selected.is_some_and(|(index, wallpaper)| match &playlist.membership {
            PlaylistMembership::Explicit { wallpaper_ids } => wallpaper_ids.contains(&wallpaper.id),
            PlaylistMembership::Smart { rule } => app.catalog.get(index).is_some_and(|entry| {
                entry.id == wallpaper.id && library::matches_rule(entry, rule, &app.state.favorites)
            }),
        });
        let active =
            app.state.active_playlist_id.as_deref() == Some(&playlist.id) && app.playlist_playing();
        let kind = if editable {
            tr("r5-explicit")
        } else {
            tr("r5-smart")
        };
        let title_size = 42.0 * scale.clamp(0.88, 1.08);
        let definition_width = if stacked {
            page_width
        } else if fixed_sequence {
            page_width - sequence_width - 28.0 * scale - 1.0
        } else {
            (page_width - 28.0 * scale - 1.0) * 0.6
        };
        let title = donor_back_layout::measured_lines(
            &playlist.name,
            title_size,
            (definition_width - 12.0).max(20.0),
            UI_FONT,
            2,
        )
        .join("\n");
        let identity = column![
            iced::widget::tooltip(
                container(
                    panel_text(title, scale)
                        .size(title_size)
                        .line_height(iced::widget::text::LineHeight::Relative(0.98))
                        .wrapping(iced::widget::text::Wrapping::None)
                        .width(Fill)
                )
                .id(iced::widget::Id::new("playlist-selected-title")),
                container(panel_text(&playlist.name, scale))
                    .max_width((app.viewport[0] - 40.0).min(640.0)),
                iced::widget::tooltip::Position::Bottom,
            )
            .style(move |_| donor_ui::box_style(palette.background, palette.outline)),
            panel_text(&playlist.id, scale)
                .color(donor_ui::with_alpha_color(palette.surface_text, 0.58)),
            panel_text(format!("{kind} · {:02}", entries.len()), scale).color(palette.primary),
            row![
                container(
                    panel_button(panel_text(tr("r5-set-active"), scale), palette, scale)
                        .on_press_maybe((!active).then(|| Message::SetActivePlaylist(
                            playlist.id.clone(),
                            app.automation_generation.load(Ordering::Acquire)
                        )))
                )
                .id(iced::widget::Id::new("playlist-start-action")),
                container(
                    panel_button(
                        panel_text(tr("playlists-stop-edition"), scale),
                        palette,
                        scale
                    )
                    .on_press_maybe(active.then_some(Message::StopPlaylist {
                        generation: app.automation_generation.load(Ordering::Acquire)
                    }))
                )
                .id(iced::widget::Id::new("playlist-stop-action")),
            ]
            .spacing(6.0 * scale)
            .wrap(),
        ]
        .spacing(10.0 * scale)
        .width(Fill);
        let mut details = column![
            panel_text(format!("01  {}", tr("repair-playlist-definition")), scale)
                .size(14.0 * type_scale)
                .color(palette.primary),
            rule(),
            panel_text(
                if editable {
                    tr("r5-explicit-hint")
                } else {
                    tr("r5-smart-hint")
                },
                scale
            ),
        ]
        .spacing(12.0 * scale)
        .width(Fill);
        if let Some((_, wallpaper)) = selected {
            details = details.push(
                column![
                    rule(),
                    panel_text(
                        format!("{} · {}", tr("r5-selected-wallpaper"), wallpaper.title),
                        scale
                    )
                    .width(Fill),
                    container(
                        panel_button(
                            panel_text(
                                if member {
                                    tr("r5-included")
                                } else {
                                    tr("r5-add-list")
                                },
                                scale
                            ),
                            palette,
                            scale
                        )
                        .on_press_maybe(
                            editable
                                .then(|| Message::TogglePlaylistMembership(playlist.id.clone()))
                        )
                        .style(move |_, status| donor_ui::action_style(
                            palette, member, false, status
                        ))
                    )
                    .id(iced::widget::Id::new("playlist-membership-action"))
                ]
                .spacing(9.0 * scale),
            );
        }

        if let Some(owner) = owner() {
            use super::super::playlists::Edit;
            let message = |edit| Message::PlaylistEdit(owner.clone(), edit);
            details = details.push(rule());
            details = details.push(
                container(
                    panel_button(
                        panel_text(tr("playlist-delete-action"), scale),
                        palette,
                        scale,
                    )
                    .on_press(message(Edit::DeleteRequest(playlist.id.clone())))
                    .style(move |_, status| donor_ui::action_style(palette, false, true, status)),
                )
                .id(iced::widget::Id::new("playlist-delete-action")),
            );
        }
        let playback = column![
            panel_text(format!("02  {}", tr("playlists-playback-title")), scale)
                .size(14.0 * type_scale)
                .color(palette.primary),
            rule(),
            container(
                panel_button(
                    panel_text(
                        match playlist.order {
                            PlaybackOrder::Sequential => tr("r5-sequential"),
                            PlaybackOrder::Shuffle => tr("r5-shuffle"),
                        },
                        scale
                    ),
                    palette,
                    scale
                )
                .on_press(Message::TogglePlaylistOrder(playlist.id.clone()))
            )
            .id(iced::widget::Id::new("playlist-order-action")),
            panel_text(app.random_rotation_state().label(), scale),
            panel_text(
                format!(
                    "{} {} s",
                    tr("playlists-every"),
                    app.state.slideshow_interval_seconds
                ),
                scale
            ),
            panel_text(
                tr(
                    if app.automation_suspended || app.state.automation_intent().safely_stopped {
                        "playlist-manager-safe-stop"
                    } else if active {
                        "playlist-manager-playing"
                    } else {
                        "playlist-manager-browse"
                    }
                ),
                scale
            ),
            panel_text(
                if app.launch_policy.allows_automatic_provider() {
                    ""
                } else {
                    tr("playlist-manager-preview")
                },
                scale
            )
            .color(donor_ui::with_alpha_color(palette.surface_text, 0.65)),
            panel_text(
                if active {
                    tr("r5-active-playback")
                } else {
                    tr("repair-playlist-browse")
                },
                scale
            )
            .color(donor_ui::with_alpha_color(palette.surface_text, 0.65)),
        ]
        .spacing(12.0 * scale)
        .width(Fill);
        let sections: Element<'a, Message> = if !stacked && definition_width > 450.0 * type_scale {
            row![
                container(details).width(Length::FillPortion(1)),
                container(Space::new())
                    .width(1)
                    .height(Fill)
                    .style(move |_| iced::widget::container::Style {
                        background: Some(donor_ui::with_alpha_color(palette.outline, 0.34).into()),
                        ..Default::default()
                    }),
                container(playback).width(Length::FillPortion(1))
            ]
            .spacing(10.0 * scale)
            .into()
        } else {
            column![details, playback].spacing(24.0 * scale).into()
        };
        let controls = column![identity, rule(), sections]
            .spacing(24.0 * scale)
            .width(Fill);
        let sequence_heading = column![
            row![
                panel_text(tr("r5-sequence"), scale)
                    .size(14.0 * type_scale)
                    .color(palette.primary),
                Space::new().width(Fill),
                panel_text(format!("{:02}", entries.len()), scale)
            ],
            rule()
        ]
        .spacing(10.0 * scale);
        let mut sequence = column![].spacing(10.0 * scale).width(Fill);
        if entries.is_empty() {
            sequence = sequence.push(
                container(
                    column![
                        panel_text(tr("r5-sequence-empty"), scale)
                            .size(13.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
                        panel_text(tr("playlist-sequence-empty-hint"), scale)
                    ]
                    .spacing(6.0 * scale),
                )
                .padding([24.0 * scale, 0.0 * scale]),
            );
        }
        for (n, entry) in entries.iter().enumerate() {
            // Existing explicit ID order or current smart-rule order. Shuffle
            // remains playback intent; this sequence never invents a live cursor.
            let art: Element<'a, Message> = app
                .wallpapers
                .iter()
                .find(|w| w.id == entry.id)
                .and_then(|w| w.preview_path.as_ref())
                .filter(|p| p.is_file())
                .map(|p| {
                    image(image::Handle::from_path(p))
                        .opacity(opacity)
                        .content_fit(iced::ContentFit::Cover)
                        .width(Fill)
                        .height(158.0 * scale * (2.0 / 3.0))
                        .into()
                })
                .unwrap_or_else(|| {
                    container(
                        panel_text("W", scale)
                            .size(22.0 * crate::donor_folio::source_ui::legible_type_scale(scale)),
                    )
                    .width(Fill)
                    .height(158.0 * scale * (2.0 / 3.0))
                    .align_x(iced::alignment::Horizontal::Center)
                    .align_y(iced::alignment::Vertical::Center)
                    .style(move |_| donor_ui::box_style(palette.background, palette.outline))
                    .into()
                });
            sequence = sequence.push(
                column![
                    row![
                        panel_text(format!("{:02}", n + 1), scale)
                            .size(14.0 * crate::donor_folio::source_ui::legible_type_scale(scale))
                            .color(palette.primary),
                        column![
                            panel_text(&entry.title, scale)
                                .size(12.0 * type_scale)
                                .width(Fill),
                            // Scale both axes equally; retain the original cover aspect and source crop.
                            row![
                                container(art)
                                    .id(iced::widget::Id::new("playlist-art"))
                                    .width(Length::FillPortion(2)),
                                Space::new().width(Length::FillPortion(1)),
                            ]
                            .spacing(0)
                            .width(Fill),
                        ]
                        .spacing(6.0 * scale)
                        .width(Fill)
                    ]
                    .spacing(10.0 * scale)
                    .align_y(iced::Alignment::Center),
                    rule()
                ]
                .spacing(10.0 * scale),
            );
        }
        let definition = container(
            scrollable(container(controls).padding(iced::Padding {
                right: 12.0,
                bottom: 8.0 * scale,
                ..iced::Padding::ZERO
            }))
            .id("playlist-definition")
            .height(Fill),
        )
        .id(iced::widget::Id::new("playlist-details-region"))
        .width(Fill)
        .height(Fill);
        let sequence = container(
            column![
                sequence_heading,
                scrollable(container(sequence).padding(iced::Padding {
                    right: 12.0,
                    bottom: 8.0 * scale,
                    ..iced::Padding::ZERO
                }))
                .id("playlist-sequence")
                .height(Fill)
            ]
            .spacing(12.0 * scale)
            .height(Fill),
        )
        .id(iced::widget::Id::new("playlist-sequence-region"))
        .width(Fill)
        .height(Fill);
        reading = if stacked {
            column![
                container(definition).height(Length::FillPortion(3)),
                container(sequence).height(Length::FillPortion(2))
            ]
            .spacing(18.0 * scale)
            .height(Fill)
            .into()
        } else {
            row![
                container(definition).width(if fixed_sequence {
                    Fill
                } else {
                    Length::FillPortion(3)
                }),
                container(Space::new())
                    .width(1)
                    .height(Fill)
                    .style(move |_| iced::widget::container::Style {
                        background: Some(donor_ui::with_alpha_color(palette.outline, 0.42).into()),
                        ..Default::default()
                    }),
                container(sequence).width(if fixed_sequence {
                    Length::Fixed(sequence_width)
                } else {
                    Length::FillPortion(2)
                })
            ]
            .spacing(14.0 * scale)
            .height(Fill)
            .into()
        };
    }
    let page = container(reading)
        .padding(iced::Padding {
            top: 30.0 * scale,
            right: 30.0 * scale,
            bottom: 44.0 * scale,
            left: 34.0 * scale,
        })
        .height(Fill)
        .width(Fill)
        .style(move |_| {
            donor_ui::box_style(
                donor_ui::with_alpha_color(palette.surface, 0.965),
                iced::Color::TRANSPARENT,
            )
        })
        .into();
    let selected_name = browsing_id
        .and_then(|id| app.state.playlists.iter().find(|p| p.id == id))
        .map(|p| p.name.as_str())
        .unwrap_or(tr("playlists-masthead-fallback"));
    let masthead = donor_back_layout::measured_lines(
        &format!("{} / {selected_name}", tr("r5-playlists")),
        10.0 * scale,
        (sheet_width - 320.0 * scale).max(20.0),
        UI_FONT,
        1,
    )
    .into_iter()
    .next()
    .unwrap_or_default();
    donor_folio::sheet(
        app.viewport,
        Some(list_index),
        page,
        palette,
        Message::SheetBackdrop,
        Some((masthead, Message::TogglePlaylistPanel)),
        scale,
    )
}

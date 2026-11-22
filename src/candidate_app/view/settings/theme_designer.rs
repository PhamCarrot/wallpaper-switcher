//! Theme presentation adapted from pinned skwd-wall 1.0.0-beta.26
//! frontend/theme_designer/view.rs. See docs/third_party/SKWD-WALL-NOTICE.md.
//! ThemeSession and theme_workflow retain every mutation/publication decision.
use super::*;
use crate::candidate_app::theme_workflow::{Action, Binding, Choice, Load, PromptKind};
use crate::donor_folio::source_ui::{
    self, flow, folio_rule, label, legible_type_scale, with_alpha, TYPE_SMALL, UI_FONT,
};
use iced::widget::column;
use iced::{Alignment, Background, Length, Padding};

const COLOUR_FIELD_SIZE: f32 = 184.0;
const READING_SECTION_SPACING: f32 = 28.0;
pub(in crate::candidate_app) const INDEX_SCROLL: &str = "theme-designer-index";
pub(in crate::candidate_app) const READING_SCROLL: &str = "theme-designer-reading";

// The shell and the compact decision use the same available reading width.
pub(in crate::candidate_app) fn reading_width(viewport: [f32; 2], scale: f32) -> f32 {
    let [width, _, index] = donor_folio::dimensions(viewport, scale);
    (width - index - 76.0 * scale).max(0.0)
}
pub(in crate::candidate_app) fn compact(viewport: [f32; 2], scale: f32) -> bool {
    reading_width(viewport, scale) < 780.0 * legible_type_scale(scale)
}

fn role_key(role: theme_roles::ThemeRole) -> String {
    let suffix = match role.index() {
        0 => "accent".into(),
        1 => "accent-text".into(),
        2 => "second-accent".into(),
        3 => "panel".into(),
        4 => "panel-text".into(),
        5 => "panel-alt".into(),
        6 => "panel-raised".into(),
        7 => "backdrop".into(),
        _ => role.name().replace('_', "-"),
    };
    format!("theme-designer-role-{suffix}")
}
fn role_label(app: &GpuCandidate, role: theme_roles::ThemeRole) -> String {
    app.settings_locale.text(&role_key(role))
}

fn swatch<'a>(hex: &str, width: f32, height: f32) -> Element<'a, Message> {
    let rgb = gpu_gallery::parse_hex_rgb(hex, [0.5; 3]);
    let color = Color::from_rgb(rgb[0], rgb[1], rgb[2]);
    container(Space::new())
        .width(width)
        .height(height)
        .style(move |_| donor_ui::box_style(color, with_alpha(Color::BLACK, 0.38)))
        .into()
}

fn designer_label<'a>(
    value: impl Into<String>,
    size: f32,
    scale: f32,
    color: Color,
) -> iced::widget::Text<'a> {
    label(value.into(), size, scale, color)
}

// Reference button metrics with the already-adopted Folio width/style owners.
fn action<'a>(
    caption: String,
    active: bool,
    destructive: bool,
    message: Option<Message>,
    palette: donor_ui::Palette,
    scale: f32,
) -> Element<'a, Message> {
    button(
        text(caption)
            .font(UI_FONT)
            .size(11.0 * legible_type_scale(scale)),
    )
    .padding([7.0 * scale, 12.0 * scale])
    .on_press_maybe(message)
    .style(move |_, status| source_ui::folio_button_style(active, destructive, &palette, status))
    .into()
}

pub(in crate::candidate_app) fn palette_index(app: &GpuCandidate) -> Element<'_, Message> {
    let session = &app.theme_session;
    let palette = donor_palette(app);
    let scale = super::bridge::effective_ui_scale(app);
    let tr = |key: &str| app.settings_locale.text(key);
    let instance = session.editor_instance().unwrap();
    let selected = session.editor_selected_role().unwrap();
    let variant = session.editor_selected_variant().unwrap();
    let document = session.editor_document().unwrap();
    let filter = session.editor_filter().unwrap_or_default();
    let needle = filter.trim().to_lowercase();
    let mut roles = column![].spacing(1.0 * scale);
    for (group, key) in theme_roles::RoleGroup::ALL.into_iter().zip([
        "wall", "accents", "tertiary", "surfaces", "error", "effects",
    ]) {
        let entries: Vec<_> = theme_roles::ROLE_DESCRIPTORS
            .iter()
            .filter(|entry| {
                entry.group == group
                    && (needle.is_empty()
                        || role_label(app, entry.role).to_lowercase().contains(&needle)
                        || entry.role.name().contains(&needle)
                        || entry.label.to_lowercase().contains(&needle))
            })
            .collect();
        if entries.is_empty() {
            continue;
        }
        roles = roles.push(
            container(designer_label(
                tr(&format!("theme-designer-group-{key}")),
                10.0,
                scale,
                palette.primary,
            ))
            .padding([12.0 * scale, 7.0 * scale]),
        );
        for entry in entries {
            let role = entry.role;
            let active = role == selected;
            let hex = document.color(role, variant);
            roles = roles.push(
                button(
                    row![
                        designer_label(
                            if active { "◆" } else { "◇" },
                            8.0,
                            scale,
                            with_alpha(palette.primary, if active { 1.0 } else { 0.44 })
                        ),
                        swatch(hex, 24.0 * scale, 16.0 * scale),
                        designer_label(role_label(app, role), 11.5, scale, palette.surface_text)
                            .width(Fill),
                        designer_label(hex, 9.0, scale, with_alpha(palette.surface_text, 0.46)),
                    ]
                    .spacing(8.0 * scale)
                    .align_y(Alignment::Center),
                )
                .width(Fill)
                .padding([8.0 * scale, 7.0 * scale])
                .on_press(Message::ThemeRoleSelected {
                    editor_instance: instance,
                    role,
                })
                .style(move |_, status| {
                    source_ui::folio_line_button_style(active, false, &palette, 1.0, status)
                }),
            );
        }
    }
    let footer = column![
        folio_rule(&palette),
        row![
            swatch(
                document.color(selected, variant),
                24.0 * scale,
                16.0 * scale
            ),
            designer_label(role_label(app, selected), 10.0, scale, palette.primary)
        ]
        .spacing(8.0 * scale),
        designer_label(selected.name(), 9.5, scale, palette.surface_text),
        designer_label(
            tr(if variant == theme_roles::ColorVariant::Dark {
                "theme-profile-dark"
            } else {
                "theme-profile-light"
            }),
            9.5,
            scale,
            palette.primary
        ),
        designer_label(
            tr(&format!("{}-desc", role_key(selected))),
            11.0,
            scale,
            with_alpha(palette.surface_text, 0.5)
        ),
        designer_label(
            tr(if theme_roles::role_descriptor(selected).has_consumer {
                "theme-designer-mapped"
            } else {
                "theme-designer-retained"
            }),
            9.5,
            scale,
            with_alpha(palette.surface_text, 0.5)
        ),
    ]
    .spacing(8.0 * scale);
    source_ui::folio_index_shell(
        app.settings_locale.language.rtl,
        tr("theme-designer-index-title"),
        tr("theme-designer--index-subtitle"),
        vec![
            text_input(&tr("theme-designer-role-search"), filter)
                .id("theme-role-filter")
                .font(UI_FONT)
                .size(12.0 * legible_type_scale(scale))
                .padding([7.0 * scale, 9.0 * scale])
                .on_input(move |text| Message::ThemeRoleFilterChanged {
                    editor_instance: instance,
                    text,
                })
                .style(move |_, _| {
                    source_ui::workbench_input_style(palette.surface_text, palette.primary)
                })
                .into(),
            scrollable(roles.padding(Padding {
                right: 12.0 * scale,
                ..Padding::ZERO
            }))
            .id(INDEX_SCROLL)
            .direction(source_ui::thin_vbar())
            .style(source_ui::scroll_style(palette.primary))
            .height(Fill)
            .into(),
            footer.into(),
        ],
        18.0,
        scale,
        &palette,
    )
}

pub(super) fn wheel_editor(app: &GpuCandidate) -> Element<'_, Message> {
    let palette = donor_palette(app);
    let session = &app.theme_session;
    let scale = super::bridge::effective_ui_scale(app);
    let tr = |key: &str| app.settings_locale.text(key);
    let instance = session.editor_instance().unwrap_or_default();
    let generation = session.editor_selection_generation().unwrap_or_default();
    let role = session
        .editor_selected_role()
        .unwrap_or(theme_roles::ThemeRole::Primary);
    let variant = session
        .editor_selected_variant()
        .unwrap_or(theme_roles::ColorVariant::Dark);
    let current = session
        .editor_document()
        .map(|doc| doc.color(role, variant))
        .unwrap_or_default();
    let loaded = session
        .editor_loaded_color(role, variant)
        .unwrap_or_default();
    let buffer = session.editor_buffer().unwrap_or_default();
    let set = Message::ThemeRoleSetColor {
        editor_instance: instance,
        selection_generation: generation,
        role,
        variant,
    };
    let wheel: Element<'_, source_ui::wheel::WheelAction> =
        iced::widget::canvas(source_ui::wheel::ColorWheel {
            hsv: session.designer_hsv(),
            binding: Some((instance, generation)),
        })
        .width(COLOUR_FIELD_SIZE * scale)
        .height(COLOUR_FIELD_SIZE * scale)
        .into();
    let wheel = wheel.map(move |action| Message::ThemeWheel {
        editor_instance: instance,
        selection_generation: generation,
        role,
        variant,
        action,
    });
    let mut recents = row![].spacing(6.0 * scale).align_y(Alignment::Center);
    if session.designer_recent().is_empty() {
        recents = recents.push(designer_label(
            tr("theme-designer-recent-empty"),
            9.5,
            scale,
            with_alpha(palette.surface_text, 0.42),
        ));
    }
    for color in session.designer_recent().iter().rev() {
        recents = recents.push(
            button(swatch(color, 28.0 * scale, 20.0 * scale))
                .padding(0.0 * scale)
                .on_press(Message::ThemeRecent {
                    editor_instance: instance,
                    selection_generation: generation,
                    role,
                    variant,
                    color: color.clone(),
                }),
        );
    }
    let mut variants = row![
        designer_label(
            tr("theme-designer-colour-field-title"),
            14.0,
            scale,
            palette.surface_text
        ),
        Space::new().width(Fill)
    ]
    .spacing(7.0 * scale)
    .align_y(Alignment::Center);
    for (value, key) in [
        (theme_roles::ColorVariant::Dark, "theme-profile-dark"),
        (theme_roles::ColorVariant::Light, "theme-profile-light"),
    ] {
        variants = variants.push(action(
            tr(key),
            variant == value,
            false,
            Some(Message::ThemeVariantSelected {
                editor_instance: instance,
                variant: value,
            }),
            palette,
            scale,
        ));
    }
    let editor = column![
        variants,
        designer_label(
            tr("theme-designer-colour-field-desc"),
            10.5,
            scale,
            with_alpha(palette.surface_text, 0.56)
        ),
        container(wheel).center_x(Fill),
        container(
            row![
                swatch(current, 42.0 * scale, 30.0 * scale),
                text_input("#RRGGBB", buffer)
                    .id("theme-role-hex")
                    .font(UI_FONT)
                    .on_input(move |text| Message::ThemeRoleTextChanged {
                        editor_instance: instance,
                        selection_generation: generation,
                        role,
                        variant,
                        text
                    })
                    .on_submit(set.clone())
                    .width(138.0 * scale)
                    .padding([7.0 * scale, 9.0 * scale])
                    .size(12.0 * legible_type_scale(scale))
                    .style(move |_, _| source_ui::workbench_input_style(
                        palette.surface_text,
                        palette.primary
                    )),
                action(
                    tr("theme-designer-set-colour"),
                    true,
                    false,
                    Some(set),
                    palette,
                    scale
                )
            ]
            .spacing(8.0 * scale)
            .align_y(Alignment::Center)
        )
        .center_x(Fill),
        container(
            row![
                action(
                    tr("theme-designer-reset-colour"),
                    false,
                    false,
                    (current != loaded || session.editor_pending()).then_some(
                        Message::ThemeRoleResetColor {
                            editor_instance: instance,
                            selection_generation: generation,
                            role,
                            variant
                        }
                    ),
                    palette,
                    scale
                ),
                designer_label(
                    tr("theme-designer-loaded-colour"),
                    9.5,
                    scale,
                    with_alpha(palette.surface_text, 0.56)
                ),
                swatch(loaded, 18.0 * scale, 14.0 * scale),
                designer_label(loaded, 9.5, scale, with_alpha(palette.surface_text, 0.56)),
            ]
            .spacing(8.0 * scale)
            .align_y(Alignment::Center)
            .wrap()
        )
        .center_x(Fill),
        designer_label(
            if session.editor_pending() {
                tr("theme-designer-unset")
            } else {
                String::new()
            },
            9.5,
            scale,
            palette.primary
        ),
        designer_label(
            session
                .editor_field_diagnostic()
                .map(|d| d.to_string())
                .unwrap_or_default(),
            9.5,
            scale,
            palette.surface_text
        ),
        folio_rule(&palette),
        designer_label(tr("theme-designer-recent"), 9.0, scale, palette.primary),
        recents,
    ]
    .spacing(10.0 * scale);
    container(editor).width(Length::FillPortion(5)).into()
}

pub(in crate::candidate_app) const STARTER_PICKER: &str = "theme-designer-starter";

fn starting_palette(app: &GpuCandidate, binding: Binding) -> Element<'_, Message> {
    let palette = donor_palette(app);
    let scale = super::bridge::effective_ui_scale(app);
    let tr = |key: &str| app.settings_locale.text(key);
    let starters = candidate_state::frontend::starter_palettes();
    let selected = starters
        .iter()
        .find(|starter| Some(starter.id.as_str()) == app.theme_workflow.starter)
        .map(|starter| starter.name.as_str());
    let picker = iced::widget::pick_list(
        starters
            .iter()
            .map(|starter| starter.name.as_str())
            .collect::<Vec<_>>(),
        selected,
        move |name| {
            let starter = starters
                .iter()
                .find(|starter| starter.name == name)
                .expect("pick list only returns bundled starters");
            Message::ThemeDesigner(Action::Load {
                binding,
                target: Load::Starter(&starter.id),
            })
        },
    )
    .placeholder(tr("theme-designer-preset-placeholder"))
    .menu_height(Length::Fixed(224.0 * scale))
    .font(UI_FONT)
    .text_size(12.0 * legible_type_scale(scale))
    .padding([9.0 * scale, 12.0 * scale])
    .width(Fill)
    .style(move |_, status| iced::widget::pick_list::Style {
        text_color: palette.surface_text,
        placeholder_color: with_alpha(palette.surface_text, 0.56),
        handle_color: palette.primary,
        background: Background::Color(palette.surface),
        border: iced::Border {
            color: if matches!(status, iced::widget::pick_list::Status::Active) {
                palette.outline
            } else {
                palette.primary
            },
            width: 1.0,
            radius: 0.0.into(),
        },
    })
    .menu_style(move |theme| iced::widget::overlay::menu::Style {
        background: Background::Color(palette.background),
        border: iced::Border {
            color: palette.outline,
            width: 1.0,
            radius: 0.0.into(),
        },
        text_color: palette.surface_text,
        selected_text_color: palette.primary_text,
        selected_background: Background::Color(palette.primary),
        ..iced::widget::overlay::menu::default(theme)
    });
    column![
        folio_rule(&palette),
        designer_label(
            tr("theme-designer-start-title"),
            14.0,
            scale,
            palette.surface_text
        ),
        designer_label(
            tr("theme-designer-start-desc"),
            10.5,
            scale,
            with_alpha(palette.surface_text, 0.56)
        ),
        container(picker).id(STARTER_PICKER),
    ]
    .spacing(12.0 * scale)
    .width(Fill)
    .into()
}

fn save_document(app: &GpuCandidate, binding: Binding) -> Element<'_, Message> {
    let palette = donor_palette(app);
    let scale = super::bridge::effective_ui_scale(app);
    let tr = |key: &str| app.settings_locale.text(key);
    let instance = app.theme_session.editor_instance().unwrap();
    let has_name = !app.theme_session.designer_name().trim().is_empty();
    let mut actions = row![
        action(
            tr("theme-designer-save"),
            false,
            false,
            has_name.then_some(Message::ThemeDesigner(Action::Save {
                binding,
                apply: false
            })),
            palette,
            scale
        ),
        action(
            tr("theme-designer-save-apply"),
            has_name,
            false,
            has_name.then_some(Message::ThemeDesigner(Action::Save {
                binding,
                apply: true
            })),
            palette,
            scale
        ),
    ]
    .spacing(7.0 * scale);
    actions = actions.push(action(
        tr("theme-designer-reset"),
        false,
        true,
        app.theme_session
            .designer_dirty()
            .then_some(Message::ThemeDesigner(Action::Reset(binding))),
        palette,
        scale,
    ));
    let mut body = column![
        folio_rule(&palette),
        designer_label(
            tr("theme-designer-publish-title"),
            14.0,
            scale,
            palette.surface_text
        ),
        designer_label(
            tr("theme-designer-publish-desc"),
            10.5,
            scale,
            with_alpha(palette.surface_text, 0.56)
        ),
        text_input(
            &tr("theme-designer-name-placeholder"),
            app.theme_session.designer_name()
        )
        .id("theme-name")
        .font(UI_FONT)
        .on_input(move |text| Message::ThemeDesigner(Action::Name { instance, text }))
        .on_submit(Message::ThemeDesigner(Action::Save {
            binding,
            apply: false
        }))
        .padding([7.0 * scale, 9.0 * scale])
        .size(12.0 * legible_type_scale(scale))
        .width(Fill)
        .style(move |_, _| source_ui::workbench_input_style(palette.surface_text, palette.primary)),
        actions.wrap(),
        designer_label(
            tr(if app.theme_session.designer_dirty() {
                "theme-designer-dirty"
            } else {
                "theme-designer-clean"
            }),
            10.0,
            scale,
            palette.primary
        ),
        designer_label(
            &app.theme_workflow.message,
            10.5,
            scale,
            palette.surface_text
        ),
    ]
    .spacing(12.0 * scale)
    .width(Fill);
    if app.theme_workflow.probe_armed {
        body = body.push(action(
            "Probe hidden Exit (once)".into(),
            false,
            false,
            Some(Message::ThemeExitProbe),
            palette,
            scale,
        ));
    }
    if app.local_state_recovery_available() {
        body = body
            .push(action(
                tr("theme-designer-reload"),
                false,
                false,
                Some(Message::SettingsReloadSavedState {
                    window: app.window,
                    generation: app.settings_workbench.generation,
                }),
                palette,
                scale,
            ))
            .push(designer_label(
                &app.settings_workbench.message,
                10.5,
                scale,
                palette.surface_text,
            ));
    }
    body.into()
}

fn saved_library(app: &GpuCandidate, binding: Binding) -> Element<'_, Message> {
    let palette = donor_palette(app);
    let scale = super::bridge::effective_ui_scale(app);
    let tr = |key: &str| app.settings_locale.text(key);
    let variant = app.theme_session.editor_selected_variant().unwrap();
    let mut library = column![].spacing(1.0 * scale);
    for record in &app.state.theme_library.records {
        let active = app.theme_session.designer_loaded_id() == Some(record.id);
        let mut colors = row![].spacing(2.0 * scale).align_y(Alignment::Center);
        for role in theme_roles::ThemeRole::ALL.into_iter().take(9) {
            colors = colors.push(swatch(
                record.document.color(role, variant),
                10.0 * scale,
                15.0 * scale,
            ));
        }
        library = library.push(
            row![
                button(
                    column![
                        row![
                            designer_label(
                                if active { "◆" } else { "◇" },
                                8.0,
                                scale,
                                palette.primary
                            ),
                            designer_label(&record.name, 10.5, scale, palette.surface_text)
                                .width(Fill)
                        ]
                        .spacing(8.0 * scale),
                        row![
                            designer_label(
                                tr("theme-designer-load"),
                                9.0,
                                scale,
                                with_alpha(palette.surface_text, 0.56)
                            ),
                            Space::new().width(Fill),
                            colors
                        ]
                        .align_y(Alignment::Center),
                    ]
                    .spacing(5.0 * scale)
                )
                .width(Fill)
                .padding([6.0 * scale, 8.0 * scale])
                .on_press(Message::ThemeDesigner(Action::Load {
                    binding,
                    target: Load::Saved(record.id)
                }))
                .style(move |_, status| source_ui::folio_button_style(
                    active, false, &palette, status
                )),
                action(
                    tr("theme-designer-delete"),
                    false,
                    true,
                    Some(Message::ThemeDesigner(Action::Delete {
                        binding,
                        id: record.id
                    })),
                    palette,
                    scale
                ),
            ]
            .spacing(5.0 * scale)
            .align_y(Alignment::Center),
        );
    }
    if app.state.theme_library.records.is_empty() {
        library = library.push(designer_label(
            tr("theme-designer-saved-empty"),
            10.0,
            scale,
            with_alpha(palette.surface_text, 0.48),
        ));
    }
    column![
        folio_rule(&palette),
        row![
            designer_label(
                tr("theme-designer-saved-title"),
                14.0,
                scale,
                palette.surface_text
            ),
            Space::new().width(Fill),
            designer_label(
                app.state.theme_library.records.len().to_string(),
                9.0,
                scale,
                with_alpha(palette.surface_text, 0.44)
            )
        ],
        designer_label(
            tr("theme-designer-saved-desc"),
            9.5,
            scale,
            with_alpha(palette.surface_text, 0.56)
        ),
        library,
    ]
    .spacing(8.0 * scale)
    .width(Fill)
    .into()
}

pub(super) fn content(app: &GpuCandidate) -> Element<'_, Message> {
    let palette = donor_palette(app);
    let scale = super::bridge::effective_ui_scale(app);
    let tr = |key: &str| app.settings_locale.text(key);
    let Some(binding) = Binding::current(app) else {
        return Space::new().into();
    };
    let header = column![
        flow![app.settings_locale.language.rtl;
            text(tr("theme-designer-title"))
                .font(UI_FONT)
                .size(46.0 * scale.clamp(0.88, 1.08))
                .line_height(iced::widget::text::LineHeight::Relative(1.0))
                .wrapping(iced::widget::text::Wrapping::WordOrGlyph)
                .color(palette.surface_text)
                .width(Fill),
            source_ui::folio_close(
                app.settings_locale.language.rtl,
                Message::SettingsThemeClose {
                    window: app.window,
                    generation: app.settings_workbench.generation
                },
                scale,
                &palette
            )
        ]
        .spacing(8.0 * scale)
        .align_y(Alignment::Start),
        designer_label(
            tr("theme-designer-description"),
            TYPE_SMALL,
            scale,
            with_alpha(palette.surface_text, 0.56)
        ),
        folio_rule(&palette),
    ]
    .spacing(12.0 * scale);
    let editor = theme_role_editor_content(app);
    let compact = compact(app.viewport, scale);
    let reading_width = reading_width(app.viewport, scale);
    let proof_width = if compact {
        reading_width
    } else {
        (reading_width - 34.0 * scale) * 7.0 / 12.0
    };
    let proof = draft_preview(app, proof_width, scale);
    let left = column![starting_palette(app, binding), save_document(app, binding)]
        .spacing(28.0 * scale)
        .width(Length::FillPortion(1));
    let right = container(saved_library(app, binding)).width(Length::FillPortion(1));
    let (workspace, management): (Element<'_, Message>, Element<'_, Message>) = if compact {
        (
            column![editor, proof].spacing(28.0 * scale).into(),
            column![left, right].spacing(28.0 * scale).into(),
        )
    } else {
        (
            row![editor, proof]
                .spacing(34.0 * scale)
                .align_y(Alignment::Start)
                .into(),
            row![left, right]
                .spacing(34.0 * scale)
                .align_y(Alignment::Start)
                .into(),
        )
    };
    scrollable(
        container(column![header, workspace, management].spacing(READING_SECTION_SPACING * scale))
            .width(Fill)
            .padding(Padding {
                top: 34.0 * scale,
                right: 38.0 * scale,
                bottom: 48.0 * scale,
                left: 38.0 * scale,
            }),
    )
    .id(READING_SCROLL)
    .direction(source_ui::thin_vbar())
    .style(source_ui::scroll_style(palette.primary))
    .width(Fill)
    .height(Fill)
    .into()
}

fn draft_preview(app: &GpuCandidate, width: f32, scale: f32) -> Element<'_, Message> {
    let type_scale = legible_type_scale(scale);
    let narrow = width < 420.0 * type_scale;
    let stack_navigation = width < 275.0 * type_scale;
    // At low scales, the type floor can require more height even in the wide layout.
    let fit_height = narrow || scale < type_scale;
    let pane_height = if fit_height {
        Length::Shrink
    } else {
        Length::Fill
    };
    let pal = donor_palette(app);
    let tr = |key: &str| app.settings_locale.text(key);
    let session = &app.theme_session;
    let document = session.editor_document().expect("active designer");
    let variant = session.editor_selected_variant().unwrap();
    let candidate =
        appearance_theme::EffectiveThemeColors::from_document(document, variant).palette();
    let selected_role = session.editor_selected_role().unwrap();

    let active_chip = container(label(
        tr("theme-designer-proof-selected"),
        8.0,
        scale,
        candidate.primary_text,
    ))
    .padding([5.0 * scale, 12.0 * scale])
    .id("theme-proof-selected")
    .style(move |_| source_ui::box_style(candidate.primary, with_alpha(candidate.primary, 0.8)));
    let secondary_chip = container(label(
        tr("theme-designer-proof-secondary"),
        8.0,
        scale,
        candidate.tertiary,
    ))
    .padding([4.0 * scale, 9.0 * scale])
    .id("theme-proof-secondary")
    .style(move |_| {
        source_ui::box_style(
            candidate.surface_container,
            with_alpha(candidate.tertiary, 0.76),
        )
    });

    let search = container(
        row![
            label("⌕", 8.0, scale, candidate.primary),
            label(
                tr("theme-designer-proof-search"),
                7.5,
                scale,
                with_alpha(candidate.surface_text, 0.58)
            ),
        ]
        .spacing(6.0 * scale)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([5.0 * scale, 6.0 * scale])
    .style(move |_| {
        source_ui::box_style(
            candidate.surface_container,
            with_alpha(candidate.outline, 0.68),
        )
    });
    let active_nav = container(
        row![
            label("◆", 6.5, scale, candidate.primary),
            label(
                tr("theme-designer-proof-nav-active"),
                8.0,
                scale,
                candidate.surface_text
            ),
        ]
        .spacing(6.0 * scale)
        .align_y(Alignment::Center),
    )
    .width(Length::Fill)
    .padding([5.0 * scale, 6.0 * scale])
    .style(move |_| {
        source_ui::box_style(
            candidate.surface_variant,
            with_alpha(candidate.primary, 0.74),
        )
    });
    let navigator = container(
        column![
            label(
                tr("theme-designer-proof-nav-title"),
                10.0,
                scale,
                candidate.surface_text
            ),
            search,
            active_nav,
            label(
                tr("theme-designer-proof-nav-motion"),
                7.5,
                scale,
                with_alpha(candidate.surface_text, 0.68)
            ),
            label(
                tr("theme-designer-proof-nav-playback"),
                7.5,
                scale,
                with_alpha(candidate.surface_text, 0.68)
            ),
            label(
                tr("theme-designer-proof-nav-theme"),
                7.5,
                scale,
                with_alpha(candidate.surface_text, 0.68)
            ),
            container(text("")).height(Length::Fill),
            label(
                tr("theme-designer-proof-nav-footer"),
                6.5,
                scale,
                with_alpha(candidate.surface_text, 0.4)
            ),
        ]
        .spacing(8.0 * scale),
    )
    .width(if stack_navigation {
        Length::Fill
    } else {
        Length::Fixed(126.0 * scale)
    })
    .height(pane_height)
    .padding(9.0 * scale)
    .style(move |_| {
        source_ui::box_style(candidate.background, with_alpha(candidate.outline, 0.58))
    });

    let raised_card = container(
        column![
            row![
                label(
                    tr("theme-designer-proof-primary-title"),
                    9.5,
                    scale,
                    candidate.surface_text
                )
                .width(Length::Fill),
                active_chip,
            ]
            .spacing(6.0 * scale)
            .align_y(Alignment::Center),
            label(
                tr("theme-designer-proof-primary-desc"),
                7.5,
                scale,
                with_alpha(candidate.surface_text, 0.58),
            ),
        ]
        .spacing(6.0 * scale),
    )
    .padding(9.0 * scale)
    .width(Length::FillPortion(1))
    .style(move |_| {
        source_ui::box_style(
            candidate.surface_container,
            with_alpha(candidate.outline, 0.66),
        )
    });
    let alternate_card = container(
        column![
            row![
                label(
                    tr("theme-designer-proof-alternate-title"),
                    9.5,
                    scale,
                    candidate.surface_text
                )
                .width(Length::Fill),
                secondary_chip,
            ]
            .spacing(6.0 * scale)
            .align_y(Alignment::Center),
            label(
                tr("theme-designer-proof-alternate-desc"),
                7.5,
                scale,
                with_alpha(candidate.surface_text, 0.58),
            ),
        ]
        .spacing(6.0 * scale),
    )
    .padding(9.0 * scale)
    .width(Length::FillPortion(1))
    .style(move |_| {
        source_ui::box_style(
            candidate.surface_variant,
            with_alpha(candidate.outline, 0.66),
        )
    });
    let cards: Element<'_, Message> = if narrow {
        column![raised_card, alternate_card]
            .spacing(7.0 * scale)
            .into()
    } else {
        row![raised_card, alternate_card]
            .spacing(7.0 * scale)
            .into()
    };
    let filter_field = container(
        row![
            label(
                tr("theme-designer-proof-filter"),
                8.0,
                scale,
                with_alpha(candidate.surface_text, 0.58)
            ),
            container(text("")).width(Length::Fill),
            label(
                tr("settings-nav-search-shortcut"),
                7.0,
                scale,
                candidate.tertiary
            ),
        ]
        .align_y(Alignment::Center),
    )
    .padding([6.0 * scale, 8.0 * scale])
    .width(Length::Fill)
    .style(move |_| source_ui::box_style(candidate.surface, with_alpha(candidate.outline, 0.72)));
    let status_bar = container(
        row![
            label("◆", 6.5, scale, candidate.tertiary),
            label(
                tr("theme-designer-proof-status"),
                7.5,
                scale,
                candidate.surface_text,
            )
            .width(Length::Fill),
            label(
                tr("theme-designer-proof-roles"),
                7.0,
                scale,
                with_alpha(candidate.surface_text, 0.5)
            ),
        ]
        .spacing(6.0 * scale)
        .align_y(Alignment::Center),
    )
    .padding([6.0 * scale, 8.0 * scale])
    .width(Length::Fill)
    .id("theme-proof-status")
    .style(move |_| {
        source_ui::box_style(
            candidate.surface_container,
            with_alpha(candidate.tertiary, 0.72),
        )
    });
    let reading = container(
        column![
            row![
                column![
                    label(
                        tr("theme-designer-proof-eyebrow"),
                        7.0,
                        scale,
                        candidate.primary
                    ),
                    text(tr("theme-designer-proof-title"))
                        .font(UI_FONT)
                        .size(15.0 * scale.clamp(0.9, 1.05))
                        .color(candidate.surface_text),
                ]
                .spacing(3.0 * scale),
                container(text("")).width(Length::Fill),
                label(
                    role_label(app, selected_role),
                    7.0,
                    scale,
                    with_alpha(candidate.surface_text, 0.48),
                ),
            ]
            .align_y(Alignment::End),
            folio_rule(&candidate),
            cards,
            filter_field,
            status_bar,
        ]
        .spacing(7.0 * scale),
    )
    .height(pane_height)
    .width(Length::Fill)
    .padding(11.0 * scale)
    .style(move |_| source_ui::bg_style(Background::Color(candidate.surface)));
    let miniature: Element<'_, Message> = if stack_navigation {
        column![navigator, reading].into()
    } else {
        row![navigator, reading]
            .height(if fit_height {
                Length::Shrink
            } else {
                Length::Fixed(224.0 * scale)
            })
            .into()
    };
    let miniature = container(miniature)
        .width(Length::Fill)
        .id("theme-proof-miniature")
        .style(move |_| {
            source_ui::box_style(candidate.surface, with_alpha(candidate.outline, 0.76))
        });

    container(
        column![
            label(
                tr("theme-designer-proof-section-title"),
                14.0,
                scale,
                pal.surface_text
            ),
            label(
                tr("theme-designer-proof-section-desc"),
                10.5,
                scale,
                with_alpha(pal.surface_text, 0.56),
            ),
            miniature,
        ]
        .spacing(9.0 * scale),
    )
    .width(Length::FillPortion(7))
    .padding(0.0 * scale)
    .into()
}

pub(in crate::candidate_app) fn prompt(app: &GpuCandidate) -> Option<Element<'_, Message>> {
    let scale = crate::candidate_app::view::settings::bridge::effective_ui_scale(app);
    let prompt = app.theme_workflow.prompt.as_ref()?;
    let binding = prompt.binding;
    let decision = prompt.decision;
    let palette = donor_palette(app);
    let decide = move |choice| {
        Message::ThemeDesigner(Action::Decide {
            binding,
            decision,
            choice,
        })
    };
    let cancel = decide(Choice::Cancel);
    let tr = |key: &str| app.settings_locale.text(key);
    let (title, note, choices): (String, String, Vec<(&str, Choice)>) = match &prompt.kind {
        PromptKind::Departure(_) => (
            tr("theme-designer-departure-title"),
            tr("theme-designer-departure-note"),
            vec![
                ("theme-designer-save", Choice::Save),
                ("theme-designer-discard", Choice::Discard),
                ("theme-designer-cancel", Choice::Cancel),
            ],
        ),
        PromptKind::ExitWaiting => (
            tr("theme-designer-wait-title"),
            tr("theme-designer-wait-note"),
            vec![("theme-designer-cancel-exit", Choice::Cancel)],
        ),
        PromptKind::Overwrite { target, .. } => (
            tr("theme-designer-overwrite-title"),
            format!("{}\n{}", tr("theme-designer-overwrite-note"), target.name),
            vec![
                ("theme-designer-overwrite", Choice::Confirm),
                ("theme-designer-cancel", Choice::Cancel),
            ],
        ),
        PromptKind::Delete(target) => (
            tr("theme-designer-delete-title"),
            format!("{}\n{}", tr("theme-designer-delete-note"), target.name),
            vec![
                ("theme-designer-delete", Choice::Confirm),
                ("theme-designer-cancel", Choice::Cancel),
            ],
        ),
    };
    let mut buttons = row![].spacing(7.0 * scale);
    for (key, choice) in choices {
        buttons = buttons.push(action(
            tr(key),
            choice == Choice::Confirm || choice == Choice::Save,
            choice == Choice::Discard
                || matches!(&prompt.kind, PromptKind::Delete(_)) && choice == Choice::Confirm,
            Some(decide(choice)),
            palette,
            scale,
        ));
    }
    let body = column![
        label(note, 11.0, scale, palette.surface_text),
        label(
            &app.theme_workflow.message,
            10.5,
            scale,
            palette.surface_text
        ),
        buttons
    ]
    .spacing(16.0 * scale)
    .into();
    let page = donor_folio::page(
        title,
        tr("theme-designer-decision"),
        body,
        Some(cancel.clone()),
        None,
        palette,
        donor_folio::BlueprintKind::Settings,
        scale,
    );
    Some(donor_folio::sheet(
        app.viewport,
        None,
        page,
        palette,
        cancel,
        None,
        scale,
    ))
}

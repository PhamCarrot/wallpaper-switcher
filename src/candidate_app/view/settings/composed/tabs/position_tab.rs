//! Source-derived pure control projection; explicit locale and -owned consumers.
use crate::candidate_app::view::settings::composed::keys;

use super::Builder;

pub(super) fn tab_position(builder: &mut Builder<'_>) {
    let catalog = builder.cfg.catalog();
    let tr = |key| catalog.text_static(key);

    picker_position(
        builder,
        tr("settings-position-slices-card"),
        keys::selector::SLICE_STAGE_X,
        keys::selector::SLICE_STAGE_Y,
    );
    picker_position(
        builder,
        tr("settings-position-hex-card"),
        keys::selector::HEX_STAGE_X,
        keys::selector::HEX_STAGE_Y,
    );
    picker_position(
        builder,
        tr("settings-position-wall-card"),
        keys::selector::GRID_STAGE_X,
        keys::selector::GRID_STAGE_Y,
    );
    picker_position(
        builder,
        tr("settings-position-depth-card"),
        keys::selector::DEPTH_STAGE_X,
        keys::selector::DEPTH_STAGE_Y,
    );
    builder.card(
        tr("settings-filter-position-card"),
        tr("settings-filter-position-card-desc"),
    );
    builder.num_setting(
        tr("settings-filter-offset-x-label"),
        tr("settings-filter-offset-x-desc"),
        crate::candidate_app::view::settings::composed::schema::setting::filter_bar::OFFSET_X,
        "px",
    );
    builder.num_setting(
        tr("settings-filter-offset-y-label"),
        tr("settings-filter-offset-y-desc"),
        crate::candidate_app::view::settings::composed::schema::setting::filter_bar::OFFSET_Y,
        "px",
    );
}

fn picker_position(builder: &mut Builder<'_>, title: &'static str, x: &str, y: &str) {
    let catalog = builder.cfg.catalog();
    let tr = |key| catalog.text_static(key);

    builder.card(title, tr("settings-position-picker-card-desc"));
    builder.num(
        tr("settings-position-horizontal-label"),
        tr("settings-position-horizontal-desc"),
        x,
        "%",
    );
    builder.num(
        tr("settings-position-vertical-label"),
        tr("settings-position-vertical-desc"),
        y,
        "%",
    );
}

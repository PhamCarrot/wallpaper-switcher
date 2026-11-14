//! Source-derived pure control projection; explicit locale and -owned consumers.
use crate::candidate_app::view::settings::composed::status::AppThemesResult;

use super::{ActionId, Control, Row};

pub(super) fn rows(
    catalog: &crate::candidate_app::localization::Catalog,
    result: Option<&AppThemesResult>,
) -> Vec<Row> {
    let tr = |key| catalog.text_static(key);

    let mut rows: Vec<_> = result.map_or_else(Vec::new, |result| {
        result
            .apps
            .iter()
            .map(|app| Row {
                title: app.name.clone(),
                desc: tr("settings-app-themes-desc").into(),
                control: Control::AppTheme { app: app.clone() },
            })
            .collect()
    });
    rows.push(Row {
        title: tr("settings-app-themes-scan").into(),
        desc: if result.is_some() {
            tr("settings-app-themes-scan-desc").into()
        } else {
            "External application theme discovery is unavailable on Windows.".into()
        },
        control: Control::ActionBtn {
            id: ActionId::RefreshAppThemes,
            label: tr("settings-app-themes-refresh").into(),
        },
    });
    rows
}

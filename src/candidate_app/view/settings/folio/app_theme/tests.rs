use super::state_label;
use crate::candidate_app::localization::Catalog;

#[test]
fn custom_outputs_are_named_without_mislabeling_other_theme_tools() {
    let catalog = Catalog::default();
    let tr = |key| catalog.text_static(key);
    assert_eq!(
        state_label(&catalog, "conflict", "custom-output"),
        tr("settings-app-themes-custom-output-status")
    );
    assert_eq!(
        state_label(&catalog, "conflict", "noctalia"),
        tr("settings-app-themes-conflict")
    );
    assert_eq!(
        state_label(&catalog, "changed", "custom-output"),
        tr("settings-app-themes-changed")
    );
}

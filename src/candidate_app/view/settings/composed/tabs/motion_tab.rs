//! Source-derived pure control projection; explicit locale and -owned consumers.
#[allow(clippy::wildcard_imports)]
use super::*;

pub(super) fn tab_motion(builder: &mut Builder<'_>) {
    let catalog = builder.cfg.catalog();
    let tr = |key| catalog.text_static(key);

    builder.card(
        tr("settings-motion-motion-card"),
        tr("settings-motion-motion-card-desc"),
    );
    builder.motion_weights();
}

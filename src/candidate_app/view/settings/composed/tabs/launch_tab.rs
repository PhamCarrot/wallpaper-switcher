//! Source-derived pure control projection; explicit locale and -owned consumers.
#[allow(clippy::wildcard_imports)]
use super::*;

pub(super) fn tab_launch(builder: &mut Builder<'_>) {
    let catalog = builder.cfg.catalog();
    let tr = |key| catalog.text_static(key);

    let cfg = builder.cfg;
    builder.card(
        tr("settings-launch-launch-card"),
        tr("settings-launch-launch-card-desc"),
    );
    let anim = cfg.launch_animation();
    builder.dropdown_cur(
        tr("settings-launch-animation-label"),
        tr("settings-launch-animation-desc"),
        keys::launch::ANIMATION,
        &[
            ("none", tr("settings-launch-animation-none")),
            ("fade", tr("settings-launch-animation-fade")),
            ("rise", tr("settings-launch-animation-rise")),
            ("zoom", tr("settings-launch-animation-zoom")),
        ],
        anim,
    );
    let speed = match cfg.text(keys::motion::LAUNCH_SPEED).as_str() {
        "fast" | "standard" | "slow" => cfg.text(keys::motion::LAUNCH_SPEED),
        _ => String::from("standard"),
    };
    builder.dropdown_cur(
        tr("settings-launch-motion-label"),
        tr("settings-launch-motion-desc"),
        keys::motion::LAUNCH_SPEED,
        &motion_speed_options(catalog),
        speed,
    );
    builder.num(
        tr("settings-launch-fade-from-label"),
        tr("settings-launch-fade-from-desc"),
        keys::general::OPEN_FADE_FROM,
        "%",
    );
}

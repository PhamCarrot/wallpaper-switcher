//! Settings projection for the native desktop wallpaper transition owner.
use crate::candidate_app::view::settings::composed::{keys, transitions, Control};
use crate::transition::TransitionEffect;

use super::Builder;

fn family_label_key(family: transitions::Family) -> &'static str {
    match family {
        transitions::Family::Random => "settings-transitions-family-random",
        transitions::Family::Fade => "settings-transitions-family-fade",
        transitions::Family::Wipe => "settings-transitions-family-wipe",
        transitions::Family::Warp => "settings-transitions-family-warp",
        transitions::Family::Break => "settings-transitions-family-break",
        transitions::Family::Sand => "settings-transitions-family-sand",
    }
}

fn family_desc_key(family: transitions::Family) -> &'static str {
    match family {
        transitions::Family::Random => "settings-transitions-family-random-desc",
        transitions::Family::Fade => "settings-transitions-family-fade-desc",
        transitions::Family::Wipe => "settings-transitions-family-wipe-desc",
        transitions::Family::Warp => "settings-transitions-family-warp-desc",
        transitions::Family::Break => "settings-transitions-family-break-desc",
        transitions::Family::Sand => "settings-transitions-family-sand-desc",
    }
}

fn current_effect(builder: &Builder<'_>) -> TransitionEffect {
    let saved = builder.cfg.text(keys::wallpaper_transition::EFFECT);
    TransitionEffect::parse_saved(&saved)
        .map(|parsed| parsed.effect)
        .unwrap_or(TransitionEffect::Crossfade)
}

pub(super) fn tab_wallpaper_transition(builder: &mut Builder<'_>) {
    let catalog = builder.cfg.catalog();
    let tr = |key| catalog.text_static(key);
    let effect = current_effect(builder);
    let family = transitions::native_family_of(effect);

    builder.card(
        tr("settings-transitions-transitions-card"),
        tr("settings-transitions-transitions-card-desc"),
    );
    builder.toggle(
        tr("settings-transitions-enable-label"),
        tr("settings-transitions-enable-desc"),
        keys::wallpaper_transition::ENABLED,
    );
    builder.num(
        tr("settings-transitions-duration-label"),
        tr("settings-transitions-duration-desc"),
        keys::wallpaper_transition::DURATION_MS,
        "ms",
    );

    let family_options: Vec<(String, String)> = transitions::FAMILIES
        .iter()
        .filter(|family| **family != transitions::Family::Sand)
        .map(|family| {
            (
                family.key().to_string(),
                tr(family_label_key(*family)).to_string(),
            )
        })
        .collect();
    builder.row(
        tr("settings-transitions-family-label"),
        tr(family_desc_key(family)),
        Control::Chips {
            path: keys::wallpaper_transition::FAMILY.to_string(),
            options: family_options,
            current: family.key().to_string(),
            disabled: Vec::new(),
            background: None,
        },
    );

    if family != transitions::Family::Random {
        let options: Vec<(String, String)> = transitions::native_effects_in_family(family)
            .map(|effect| (effect.key().to_string(), effect.display_name().to_string()))
            .collect();
        builder.dynamic_dropdown(
            tr("settings-transitions-shader-label"),
            tr("settings-transitions-shader-desc"),
            keys::wallpaper_transition::EFFECT,
            options,
        );
    }
}

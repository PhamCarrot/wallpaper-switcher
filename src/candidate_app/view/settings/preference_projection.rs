//! Exact composed source-key adapter for typed saved frontend preferences.
//! There is no dynamic registry, default generator or execution capability here.
use super::composed::draft::Value;
use crate::candidate_state::preferences::{BindingAction, FrontendPreferences};

fn binding(path: &str) -> Option<BindingAction> {
    Some(match path {
        "keys.select" => BindingAction::Select,
        "keys.apply" => BindingAction::Apply,
        "keys.randomApply" => BindingAction::RandomApply,
        "keys.flip" => BindingAction::Flip,
        "keys.reveal" => BindingAction::Reveal,
        "keys.favourite" => BindingAction::Favourite,
        "keys.effects" => BindingAction::Effects,
        "keys.sceneProperties" => BindingAction::SceneProperties,
        "keys.playlists" => BindingAction::Playlists,
        "keys.settings" => BindingAction::Settings,
        "keys.help" => BindingAction::Help,
        "keys.themePanel" => BindingAction::ThemePanel,
        "keys.tagCloud" => BindingAction::TagCloud,
        "keys.tagMode" => BindingAction::TagMode,
        "keys.filterBar" => BindingAction::FilterBar,
        "keys.folderPrev" => BindingAction::FolderPrev,
        "keys.folderNext" => BindingAction::FolderNext,
        "keys.folderToggle" => BindingAction::FolderToggle,
        "keys.hiddenFolders" => BindingAction::HiddenFolders,
        "keys.colorPrev" => BindingAction::ColorPrev,
        "keys.colorNext" => BindingAction::ColorNext,
        "keys.navLeft" => BindingAction::NavLeft,
        "keys.navRight" => BindingAction::NavRight,
        "keys.navUp" => BindingAction::NavUp,
        "keys.navDown" => BindingAction::NavDown,
        "keys.pageBack" => BindingAction::PageBack,
        "keys.pageForward" => BindingAction::PageForward,
        "keys.jumpFirst" => BindingAction::JumpFirst,
        "keys.jumpLast" => BindingAction::JumpLast,
        "keys.modePrev" => BindingAction::ModePrev,
        "keys.modeNext" => BindingAction::ModeNext,
        "keys.presetNext" => BindingAction::PresetNext,
        "keys.presetPrev" => BindingAction::PresetPrev,
        "keys.autocomplete" => BindingAction::Autocomplete,
        "keys.typePrev" => BindingAction::TypePrev,
        "keys.typeNext" => BindingAction::TypeNext,
        "keys.sortPrev" => BindingAction::SortPrev,
        "keys.sortNext" => BindingAction::SortNext,
        "keys.randomRotate" => BindingAction::RandomRotate,
        "keys.searchMode" => BindingAction::SearchMode,
        "keys.downloads" => BindingAction::Downloads,
        "keys.sourceWallhaven" => BindingAction::SourceWallhaven,
        "keys.sourceSteam" => BindingAction::SourceSteam,
        "keys.sourceUnsplash" => BindingAction::SourceUnsplash,
        "keys.sourcePexels" => BindingAction::SourcePexels,
        "keys.sourceYoutube" => BindingAction::SourceYoutube,
        "keys.sourceBing" => BindingAction::SourceBing,
        _ => return None,
    })
}
pub(in crate::candidate_app) fn read(saved: &FrontendPreferences, path: &str) -> Option<Value> {
    if let Some(action) = binding(path) {
        return saved.bindings.get(&action).cloned().map(Value::Text);
    }
    match path {
        "transition.enabled" => saved.source_motion.enabled.clone().map(Value::Boolean),
        "transition.shader" => saved.source_motion.shader.clone().map(Value::Text),
        "transition.durationMs" => saved.source_motion.duration_ms.clone().map(Value::Number),
        "transition.fps" => saved.source_motion.fps.clone().map(Value::Number),
        "transition.preview" => saved.source_motion.preview.clone().map(Value::Boolean),
        "transition.previewFps" => saved.source_motion.preview_fps.clone().map(Value::Number),
        "features.steam" => saved.source_browsers.steam_visible.map(Value::Boolean),
        "components.wallpaperSelector.wallhavenColumns" => {
            saved.source_browsers.wallhaven.columns.map(Value::Number)
        }
        "components.wallpaperSelector.wallhavenRows" => {
            saved.source_browsers.wallhaven.rows.map(Value::Number)
        }
        "components.wallpaperSelector.wallhavenThumbWidth" => saved
            .source_browsers
            .wallhaven
            .thumb_width
            .map(Value::Number),
        "components.wallpaperSelector.wallhavenThumbHeight" => saved
            .source_browsers
            .wallhaven
            .thumb_height
            .map(Value::Number),
        "components.wallpaperSelector.steamColumns" => {
            saved.source_browsers.steam.columns.map(Value::Number)
        }
        "components.wallpaperSelector.steamRows" => {
            saved.source_browsers.steam.rows.map(Value::Number)
        }
        "components.wallpaperSelector.steamThumbWidth" => {
            saved.source_browsers.steam.thumb_width.map(Value::Number)
        }
        "components.wallpaperSelector.steamThumbHeight" => {
            saved.source_browsers.steam.thumb_height.map(Value::Number)
        }
        "general.language" => saved.interface.language.clone().map(Value::Text),
        "general.uiScale" => saved.interface.ui_scale.clone().map(Value::Number),
        "general.maxFps" => saved.interface.max_fps.clone().map(Value::Number),
        "motion.fastMs" => saved.interface.fast_ms.clone().map(Value::Number),
        "motion.standardMs" => saved.interface.standard_ms.clone().map(Value::Number),
        "motion.slowMs" => saved.interface.slow_ms.clone().map(Value::Number),
        "general.applyOnPickerMonitor" => saved
            .picker
            .apply_on_picker_monitor
            .clone()
            .map(Value::Boolean),
        "general.closeOnSelection" => saved.picker.close_on_selection.clone().map(Value::Boolean),
        "general.hoverSelects" => saved.picker.hover_selects.clone().map(Value::Boolean),
        "components.wallpaperSelector.startPosition" => {
            saved.picker.start_position.clone().map(Value::Text)
        }
        "monitor" => saved.picker.desired_monitor_id.clone().map(Value::Text),
        "launch.animation" => saved.launch.animation.clone().map(Value::Text),
        "motion.launchSpeed" => saved.launch.speed.clone().map(Value::Text),
        "general.openFadeFrom" => saved.launch.fade_from.clone().map(Value::Number),
        "filterBar.visualStyle" => saved.filter_bar.visual_style.clone().map(Value::Text),
        "filterBar.orientation" => saved.filter_bar.orientation.clone().map(Value::Text),
        "filterBar.defaultFolder" => saved.filter_bar.default_folder.clone().map(Value::Text),
        "filterBar.sticky" => saved.filter_bar.sticky.clone().map(Value::Boolean),
        "filterBar.offsetX" => saved.filter_bar.offset_x.clone().map(Value::Number),
        "filterBar.offsetY" => saved.filter_bar.offset_y.clone().map(Value::Number),
        "general.filterBarAlwaysVisible" => {
            saved.filter_bar.always_visible.clone().map(Value::Boolean)
        }
        "filterBar.show.type.all" => saved.visibility.type_all.clone().map(Value::Boolean),
        "filterBar.show.type.static" => saved.visibility.type_static.clone().map(Value::Boolean),
        "filterBar.show.type.video" => saved.visibility.type_video.clone().map(Value::Boolean),
        "filterBar.show.type.we" => saved.visibility.type_we.clone().map(Value::Boolean),
        "filterBar.show.sort.date" => saved.visibility.sort_date.clone().map(Value::Boolean),
        "filterBar.show.sort.recent" => saved.visibility.sort_recent.clone().map(Value::Boolean),
        "filterBar.show.sort.color" => saved.visibility.sort_color.clone().map(Value::Boolean),
        "filterBar.show.sort.pop" => saved.visibility.sort_pop.clone().map(Value::Boolean),
        "filterBar.show.sort.richness" => {
            saved.visibility.sort_richness.clone().map(Value::Boolean)
        }
        "filterBar.show.sort.minimalist" => {
            saved.visibility.sort_minimalist.clone().map(Value::Boolean)
        }
        "filterBar.show.sort.res" => saved.visibility.sort_res.clone().map(Value::Boolean),
        "filterBar.show.sort" => saved.visibility.sort.clone().map(Value::Boolean),
        "filterBar.show.mute" => saved.visibility.mute.clone().map(Value::Boolean),
        "filterBar.show.search" => Some(Value::Boolean(
            saved.visibility.search.unwrap_or(true) || saved.visibility.tagcloud.unwrap_or(true),
        )),
        "filterBar.show.settings" => saved.visibility.settings.clone().map(Value::Boolean),
        "filterBar.show.display" => saved.visibility.display.clone().map(Value::Boolean),
        "filterBar.show.close" => saved.visibility.close.clone().map(Value::Boolean),
        "filterBar.show.folder" => saved.visibility.folder.clone().map(Value::Boolean),
        "filterBar.show.favourites" => saved.visibility.favourites.clone().map(Value::Boolean),
        "filterBar.show.random" => saved.visibility.random.clone().map(Value::Boolean),
        "filterBar.show.colors" => saved.visibility.colors.clone().map(Value::Boolean),
        "filterBar.show.theme" => saved.visibility.theme.clone().map(Value::Boolean),
        "filterBar.show.tagcloud" => saved.visibility.tagcloud.clone().map(Value::Boolean),
        "filterBar.show.orient" => saved.visibility.orient.clone().map(Value::Boolean),
        "filterBar.show.playlists" => saved.visibility.playlists.clone().map(Value::Boolean),
        "filterBar.show.download" => saved.visibility.download.clone().map(Value::Boolean),
        "filterBar.show.resolution" => saved.visibility.resolution.clone().map(Value::Boolean),
        "general.notifyOnWallpaperChange" => saved
            .desired_policy
            .notify_on_change
            .clone()
            .map(Value::Boolean),
        "restoreOnStartup" => saved
            .desired_policy
            .restore_on_startup
            .clone()
            .map(Value::Boolean),
        "general.randomIncludeStatic" => saved
            .desired_policy
            .random_include_static
            .clone()
            .map(Value::Boolean),
        "general.randomIncludeVideo" => saved
            .desired_policy
            .random_include_video
            .clone()
            .map(Value::Boolean),
        "general.randomIncludeWE" => saved
            .desired_policy
            .random_include_we
            .clone()
            .map(Value::Boolean),
        "general.randomIncludeFavourites" => saved
            .desired_policy
            .random_include_favourites
            .clone()
            .map(Value::Boolean),
        _ => None,
    }
}
/// Returns false only when this exact path belongs to another owner.
pub(in crate::candidate_app) fn write(
    saved: &mut FrontendPreferences,
    path: &str,
    value: &Value,
) -> Result<bool, String> {
    let mut proposed = saved.clone();
    if let Some(action) = binding(path) {
        let Value::Text(value) = value else {
            return Err("Expected a binding string.".into());
        };
        proposed.bindings.insert(action, value.clone());
    } else {
        match (path, value) {
            ("transition.enabled", Value::Boolean(value)) => {
                proposed.source_motion.enabled = Some(value.clone());
            }
            ("transition.shader", Value::Text(value)) => {
                if !super::composed::SHADERS.contains(&value.as_str()) {
                    return Err("Choose an exact listed source shader.".into());
                }
                proposed.source_motion.shader = Some(value.clone());
            }
            ("transition.durationMs", Value::Number(value)) => {
                proposed.source_motion.duration_ms = Some(value.clone());
            }
            ("transition.fps", Value::Number(value)) => {
                proposed.source_motion.fps = Some(value.clone());
            }
            ("transition.preview", Value::Boolean(value)) => {
                proposed.source_motion.preview = Some(value.clone());
            }
            ("transition.previewFps", Value::Number(value)) => {
                proposed.source_motion.preview_fps = Some(value.clone());
            }
            ("features.steam", Value::Boolean(value)) => {
                proposed.source_browsers.steam_visible = Some(*value)
            }
            ("components.wallpaperSelector.wallhavenColumns", Value::Number(value)) => {
                proposed.source_browsers.wallhaven.columns = Some(*value)
            }
            ("components.wallpaperSelector.wallhavenRows", Value::Number(value)) => {
                proposed.source_browsers.wallhaven.rows = Some(*value)
            }
            ("components.wallpaperSelector.wallhavenThumbWidth", Value::Number(value)) => {
                proposed.source_browsers.wallhaven.thumb_width = Some(*value)
            }
            ("components.wallpaperSelector.wallhavenThumbHeight", Value::Number(value)) => {
                proposed.source_browsers.wallhaven.thumb_height = Some(*value)
            }
            ("components.wallpaperSelector.steamColumns", Value::Number(value)) => {
                proposed.source_browsers.steam.columns = Some(*value)
            }
            ("components.wallpaperSelector.steamRows", Value::Number(value)) => {
                proposed.source_browsers.steam.rows = Some(*value)
            }
            ("components.wallpaperSelector.steamThumbWidth", Value::Number(value)) => {
                proposed.source_browsers.steam.thumb_width = Some(*value)
            }
            ("components.wallpaperSelector.steamThumbHeight", Value::Number(value)) => {
                proposed.source_browsers.steam.thumb_height = Some(*value)
            }
            ("general.language", Value::Text(value)) => {
                proposed.interface.language = Some(value.clone())
            }
            ("general.uiScale", Value::Number(value)) => {
                proposed.interface.ui_scale = Some(value.clone())
            }
            ("general.maxFps", Value::Number(value)) => {
                proposed.interface.max_fps = Some(value.clone())
            }
            ("motion.fastMs", Value::Number(value)) => {
                proposed.interface.fast_ms = Some(value.clone())
            }
            ("motion.standardMs", Value::Number(value)) => {
                proposed.interface.standard_ms = Some(value.clone())
            }
            ("motion.slowMs", Value::Number(value)) => {
                proposed.interface.slow_ms = Some(value.clone())
            }
            ("general.applyOnPickerMonitor", Value::Boolean(value)) => {
                proposed.picker.apply_on_picker_monitor = Some(value.clone())
            }
            ("general.closeOnSelection", Value::Boolean(value)) => {
                proposed.picker.close_on_selection = Some(value.clone())
            }
            ("general.hoverSelects", Value::Boolean(value)) => {
                proposed.picker.hover_selects = Some(value.clone())
            }
            ("components.wallpaperSelector.startPosition", Value::Text(value)) => {
                proposed.picker.start_position = Some(value.clone())
            }
            ("monitor", Value::Text(value)) => {
                proposed.picker.desired_monitor_id = Some(value.clone())
            }
            ("launch.animation", Value::Text(value)) => {
                proposed.launch.animation = Some(value.clone())
            }
            ("motion.launchSpeed", Value::Text(value)) => {
                proposed.launch.speed = Some(value.clone())
            }
            ("general.openFadeFrom", Value::Number(value)) => {
                proposed.launch.fade_from = Some(value.clone())
            }
            ("filterBar.visualStyle", Value::Text(value)) => {
                proposed.filter_bar.visual_style = Some(value.clone())
            }
            ("filterBar.orientation", Value::Text(value)) => {
                proposed.filter_bar.orientation = Some(value.clone())
            }
            ("filterBar.defaultFolder", Value::Text(value)) => {
                proposed.filter_bar.default_folder = Some(value.clone())
            }
            ("filterBar.sticky", Value::Boolean(value)) => {
                proposed.filter_bar.sticky = Some(value.clone())
            }
            ("filterBar.offsetX", Value::Number(value)) => {
                proposed.filter_bar.offset_x = Some(value.clone())
            }
            ("filterBar.offsetY", Value::Number(value)) => {
                proposed.filter_bar.offset_y = Some(value.clone())
            }
            ("general.filterBarAlwaysVisible", Value::Boolean(value)) => {
                proposed.filter_bar.always_visible = Some(value.clone())
            }
            ("filterBar.show.type.all", Value::Boolean(value)) => {
                proposed.visibility.type_all = Some(value.clone())
            }
            ("filterBar.show.type.static", Value::Boolean(value)) => {
                proposed.visibility.type_static = Some(value.clone())
            }
            ("filterBar.show.type.video", Value::Boolean(value)) => {
                proposed.visibility.type_video = Some(value.clone())
            }
            ("filterBar.show.type.we", Value::Boolean(value)) => {
                proposed.visibility.type_we = Some(value.clone())
            }
            ("filterBar.show.sort.date", Value::Boolean(value)) => {
                proposed.visibility.sort_date = Some(value.clone())
            }
            ("filterBar.show.sort.recent", Value::Boolean(value)) => {
                proposed.visibility.sort_recent = Some(value.clone())
            }
            ("filterBar.show.sort.color", Value::Boolean(value)) => {
                proposed.visibility.sort_color = Some(value.clone())
            }
            ("filterBar.show.sort.pop", Value::Boolean(value)) => {
                proposed.visibility.sort_pop = Some(value.clone())
            }
            ("filterBar.show.sort.richness", Value::Boolean(value)) => {
                proposed.visibility.sort_richness = Some(value.clone())
            }
            ("filterBar.show.sort.minimalist", Value::Boolean(value)) => {
                proposed.visibility.sort_minimalist = Some(value.clone())
            }
            ("filterBar.show.sort.res", Value::Boolean(value)) => {
                proposed.visibility.sort_res = Some(value.clone())
            }
            ("filterBar.show.sort", Value::Boolean(value)) => {
                proposed.visibility.sort = Some(value.clone())
            }
            ("filterBar.show.mute", Value::Boolean(value)) => {
                proposed.visibility.mute = Some(value.clone())
            }
            ("filterBar.show.search", Value::Boolean(value)) => {
                proposed.visibility.search = Some(*value);
                proposed.visibility.tagcloud = Some(*value);
            }
            ("filterBar.show.settings", Value::Boolean(value)) => {
                proposed.visibility.settings = Some(value.clone())
            }
            ("filterBar.show.display", Value::Boolean(value)) => {
                proposed.visibility.display = Some(value.clone())
            }
            ("filterBar.show.close", Value::Boolean(value)) => {
                proposed.visibility.close = Some(value.clone())
            }
            ("filterBar.show.folder", Value::Boolean(value)) => {
                proposed.visibility.folder = Some(value.clone())
            }
            ("filterBar.show.favourites", Value::Boolean(value)) => {
                proposed.visibility.favourites = Some(value.clone())
            }
            ("filterBar.show.random", Value::Boolean(value)) => {
                proposed.visibility.random = Some(value.clone())
            }
            ("filterBar.show.colors", Value::Boolean(value)) => {
                proposed.visibility.colors = Some(value.clone())
            }
            ("filterBar.show.theme", Value::Boolean(value)) => {
                proposed.visibility.theme = Some(value.clone())
            }
            ("filterBar.show.tagcloud", Value::Boolean(value)) => {
                proposed.visibility.tagcloud = Some(value.clone())
            }
            ("filterBar.show.orient", Value::Boolean(value)) => {
                proposed.visibility.orient = Some(value.clone())
            }
            ("filterBar.show.playlists", Value::Boolean(value)) => {
                proposed.visibility.playlists = Some(value.clone())
            }
            ("filterBar.show.download", Value::Boolean(value)) => {
                proposed.visibility.download = Some(value.clone())
            }
            ("filterBar.show.resolution", Value::Boolean(value)) => {
                proposed.visibility.resolution = Some(value.clone())
            }
            ("general.notifyOnWallpaperChange", Value::Boolean(value)) => {
                proposed.desired_policy.notify_on_change = Some(value.clone())
            }
            ("restoreOnStartup", Value::Boolean(value)) => {
                proposed.desired_policy.restore_on_startup = Some(value.clone())
            }
            ("general.randomIncludeStatic", Value::Boolean(value)) => {
                proposed.desired_policy.random_include_static = Some(value.clone())
            }
            ("general.randomIncludeVideo", Value::Boolean(value)) => {
                proposed.desired_policy.random_include_video = Some(value.clone())
            }
            ("general.randomIncludeWE", Value::Boolean(value)) => {
                proposed.desired_policy.random_include_we = Some(value.clone())
            }
            ("general.randomIncludeFavourites", Value::Boolean(value)) => {
                proposed.desired_policy.random_include_favourites = Some(value.clone())
            }
            _ => return Ok(false),
        }
    }
    proposed.validate()?;
    *saved = proposed;
    Ok(true)
}

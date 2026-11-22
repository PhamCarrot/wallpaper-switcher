//! Narrow source presentation data; no config IO, daemon, mutation, or worker.
use super::input::InputAction;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct KeyBindingDescriptor {
    pub action: InputAction,
    pub path: &'static str,
    pub title_key: &'static str,
    pub group: KeyBindingGroup,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyBindingGroup {
    Wallpaper,
    Panels,
    Filters,
    Folders,
    Navigation,
    Downloads,
    Global,
}

impl KeyBindingGroup {
    pub const ALL: [Self; 7] = [
        Self::Wallpaper,
        Self::Panels,
        Self::Filters,
        Self::Folders,
        Self::Navigation,
        Self::Downloads,
        Self::Global,
    ];

    pub const fn title_key(self) -> &'static str {
        match self {
            Self::Wallpaper => "settings-keybinds-wallpaper-card",
            Self::Panels => "settings-keybinds-panels-card",
            Self::Filters => "settings-keybinds-filters-card",
            Self::Folders => "settings-keybinds-folders-card",
            Self::Navigation => "settings-keybinds-navigation-card",
            Self::Downloads => "settings-keybinds-downloads-card",
            Self::Global => "settings-keybinds-global-card",
        }
    }

    pub const fn description_key(self) -> &'static str {
        match self {
            Self::Wallpaper => "settings-keybinds-wallpaper-card-desc",
            Self::Panels => "settings-keybinds-panels-card-desc",
            Self::Filters => "settings-keybinds-filters-card-desc",
            Self::Folders => "settings-keybinds-folders-card-desc",
            Self::Navigation => "settings-keybinds-navigation-card-desc",
            Self::Downloads => "settings-keybinds-downloads-card-desc",
            Self::Global => "settings-keybinds-global-card-desc",
        }
    }
}

pub const KEY_BINDINGS: [KeyBindingDescriptor; 42] = [
    KeyBindingDescriptor {
        action: InputAction::TagCloud,
        path: super::keys::keybind::TAG_CLOUD,
        title_key: "keybind-tag-cloud",
        group: KeyBindingGroup::Filters,
    },
    KeyBindingDescriptor {
        action: InputAction::Select,
        path: super::keys::keybind::SELECT,
        title_key: "keybind-select",
        group: KeyBindingGroup::Wallpaper,
    },
    KeyBindingDescriptor {
        action: InputAction::Apply,
        path: super::keys::keybind::APPLY,
        title_key: "keybind-apply",
        group: KeyBindingGroup::Wallpaper,
    },
    KeyBindingDescriptor {
        action: InputAction::RandomApply,
        path: super::keys::keybind::RANDOM_APPLY,
        title_key: "keybind-random-apply",
        group: KeyBindingGroup::Wallpaper,
    },
    KeyBindingDescriptor {
        action: InputAction::Flip,
        path: super::keys::keybind::FLIP,
        title_key: "keybind-flip",
        group: KeyBindingGroup::Wallpaper,
    },
    KeyBindingDescriptor {
        action: InputAction::Reveal,
        path: super::keys::keybind::REVEAL,
        title_key: "keybind-reveal",
        group: KeyBindingGroup::Wallpaper,
    },
    KeyBindingDescriptor {
        action: InputAction::Favourite,
        path: super::keys::keybind::FAVOURITE,
        title_key: "keybind-favourite",
        group: KeyBindingGroup::Wallpaper,
    },
    KeyBindingDescriptor {
        action: InputAction::Effects,
        path: super::keys::keybind::EFFECTS,
        title_key: "keybind-effects",
        group: KeyBindingGroup::Wallpaper,
    },
    KeyBindingDescriptor {
        action: InputAction::SceneProperties,
        path: super::keys::keybind::SCENE_PROPERTIES,
        title_key: "keybind-scene-properties",
        group: KeyBindingGroup::Wallpaper,
    },
    KeyBindingDescriptor {
        action: InputAction::Playlists,
        path: super::keys::keybind::PLAYLISTS,
        title_key: "keybind-playlists",
        group: KeyBindingGroup::Panels,
    },
    KeyBindingDescriptor {
        action: InputAction::Settings,
        path: super::keys::keybind::SETTINGS,
        title_key: "keybind-settings",
        group: KeyBindingGroup::Panels,
    },
    KeyBindingDescriptor {
        action: InputAction::Help,
        path: super::keys::keybind::HELP,
        title_key: "keybind-help",
        group: KeyBindingGroup::Panels,
    },
    KeyBindingDescriptor {
        action: InputAction::FolderPrev,
        path: super::keys::keybind::FOLDER_PREV,
        title_key: "keybind-folder-prev",
        group: KeyBindingGroup::Folders,
    },
    KeyBindingDescriptor {
        action: InputAction::FolderNext,
        path: super::keys::keybind::FOLDER_NEXT,
        title_key: "keybind-folder-next",
        group: KeyBindingGroup::Folders,
    },
    KeyBindingDescriptor {
        action: InputAction::FolderToggle,
        path: super::keys::keybind::FOLDER_TOGGLE,
        title_key: "keybind-folder-toggle",
        group: KeyBindingGroup::Folders,
    },
    KeyBindingDescriptor {
        action: InputAction::HiddenFolders,
        path: super::keys::keybind::HIDDEN_FOLDERS,
        title_key: "keybind-hidden-folders",
        group: KeyBindingGroup::Folders,
    },
    KeyBindingDescriptor {
        action: InputAction::NavLeft,
        path: super::keys::keybind::NAV_LEFT,
        title_key: "keybind-nav-left",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::NavRight,
        path: super::keys::keybind::NAV_RIGHT,
        title_key: "keybind-nav-right",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::NavUp,
        path: super::keys::keybind::NAV_UP,
        title_key: "keybind-nav-up",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::NavDown,
        path: super::keys::keybind::NAV_DOWN,
        title_key: "keybind-nav-down",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::PageBack,
        path: super::keys::keybind::PAGE_BACK,
        title_key: "keybind-page-back",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::PageForward,
        path: super::keys::keybind::PAGE_FORWARD,
        title_key: "keybind-page-forward",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::JumpFirst,
        path: super::keys::keybind::JUMP_FIRST,
        title_key: "keybind-jump-first",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::JumpLast,
        path: super::keys::keybind::JUMP_LAST,
        title_key: "keybind-jump-last",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::ModePrev,
        path: super::keys::keybind::MODE_PREV,
        title_key: "keybind-mode-prev",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::ModeNext,
        path: super::keys::keybind::MODE_NEXT,
        title_key: "keybind-mode-next",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::Autocomplete,
        path: super::keys::keybind::AUTOCOMPLETE,
        title_key: "keybind-autocomplete",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::PresetNext,
        path: super::keys::keybind::PRESET_NEXT,
        title_key: "keybind-preset-next",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::PresetPrev,
        path: super::keys::keybind::PRESET_PREV,
        title_key: "keybind-preset-prev",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::FilterBar,
        path: super::keys::keybind::FILTER_BAR,
        title_key: "keybind-show-toolbar",
        group: KeyBindingGroup::Navigation,
    },
    KeyBindingDescriptor {
        action: InputAction::TypePrev,
        path: super::keys::keybind::TYPE_PREV,
        title_key: "keybind-type-prev",
        group: KeyBindingGroup::Filters,
    },
    KeyBindingDescriptor {
        action: InputAction::TypeNext,
        path: super::keys::keybind::TYPE_NEXT,
        title_key: "keybind-type-next",
        group: KeyBindingGroup::Filters,
    },
    KeyBindingDescriptor {
        action: InputAction::SortPrev,
        path: super::keys::keybind::SORT_PREV,
        title_key: "keybind-sort-prev",
        group: KeyBindingGroup::Filters,
    },
    KeyBindingDescriptor {
        action: InputAction::SortNext,
        path: super::keys::keybind::SORT_NEXT,
        title_key: "keybind-sort-next",
        group: KeyBindingGroup::Filters,
    },
    KeyBindingDescriptor {
        action: InputAction::RandomRotate,
        path: super::keys::keybind::RANDOM_ROTATE,
        title_key: "keybind-random-rotate",
        group: KeyBindingGroup::Filters,
    },
    KeyBindingDescriptor {
        action: InputAction::ConfigApplyToAll,
        path: super::keys::keybind::CONFIG_APPLY_TO_ALL,
        title_key: "keybind-apply-to-all",
        group: KeyBindingGroup::Global,
    },
    KeyBindingDescriptor {
        action: InputAction::ConfigClose,
        path: super::keys::keybind::CONFIG_CLOSE,
        title_key: "keybind-close",
        group: KeyBindingGroup::Global,
    },
    KeyBindingDescriptor {
        action: InputAction::ConfigSwitchMonitor,
        path: super::keys::keybind::CONFIG_SWITCH_MONITOR,
        title_key: "keybind-switch-monitor",
        group: KeyBindingGroup::Global,
    },
    KeyBindingDescriptor {
        action: InputAction::ConfigToggleExclude,
        path: super::keys::keybind::CONFIG_TOGGLE_EXCLUDE,
        title_key: "keybind-toggle-exclude",
        group: KeyBindingGroup::Global,
    },
    KeyBindingDescriptor {
        action: InputAction::ConfigToggleHidden,
        path: super::keys::keybind::CONFIG_TOGGLE_HIDDEN,
        title_key: "keybind-toggle-hidden",
        group: KeyBindingGroup::Global,
    },
    KeyBindingDescriptor {
        action: InputAction::ConfigOpenExplorer,
        path: super::keys::keybind::CONFIG_OPEN_EXPLORER,
        title_key: "keybind-open-explorer",
        group: KeyBindingGroup::Global,
    },
    KeyBindingDescriptor {
        action: InputAction::ConfigReload,
        path: super::keys::keybind::CONFIG_RELOAD,
        title_key: "keybind-reload-config",
        group: KeyBindingGroup::Global,
    },
];

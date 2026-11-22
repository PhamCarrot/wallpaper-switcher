//! Narrow source presentation data; no config IO, daemon, mutation, or worker.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActionId {
    SetThemeTarget(&'static str, bool),
    RefreshAppThemes,
    SetAppTheme(u8, bool),
    AdoptAppTheme(u8),
    RefreshAppTheme(u8),
    CustomizeAppTheme(u8, &'static str),
    CopyAppThemeTemplate(u8),
    ClearCache,
    CaptureWeThumbnails,
    RecomputeColors,
    OptimizeImages,
    RefreshBackdrop,
    CycleBackgroundOpacity,
    CopyLayerRule,
    AddPostCommand,
    RemovePostCommand(u16),
    AddIntegration,
    RemoveIntegration(u16),
    ImportSemanticModel,
    AddSemanticModel,
    RemoveSemanticModel(u16),
    DeleteSemanticModel(u16),
    DeleteActiveSemanticModel,
    CycleResolution,
    ClearResolution,
    AddResolutionPreset,
    CreateResolutionPresetBand(&'static str),
    RemoveResolutionPreset(u16),
    OpenDisplays,
    ResumeRandomRotation,
    StopPlaylist,
    OpenThemeDesigner,
    ShowExcluded,
    ChooseRunningProcess,
    GenerateBugReport,
    ResetMotionFast,
    ResetMotionStandard,
    ResetMotionSlow,
    ResetKeybinds,
    SetNavKeys(super::input::NavKeys),
}

impl ActionId {
    pub const fn is_destructive(self) -> bool {
        matches!(
            self,
            Self::ClearCache
                | Self::OptimizeImages
                | Self::ResetKeybinds
                | Self::DeleteSemanticModel(_)
                | Self::DeleteActiveSemanticModel
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsFocus {
    Index,
    Sections,
    Controls,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsKey {
    FocusNext { backwards: bool },
    CategoryNext { backwards: bool },
    Search,
    Previous,
    Next,
    Up,
    Down,
    Activate,
    Cancel,
}

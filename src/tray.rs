use tray_icon::menu::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{Icon, TrayIcon, TrayIconBuilder};

const OPEN_ID: &str = "wallpaper-switcher.open";
const RANDOM_ID: &str = "wallpaper-switcher.random";
const SLIDESHOW_ID: &str = "wallpaper-switcher.slideshow";
const STOP_PLAYLIST_ID: &str = "wallpaper-switcher.stop-playlist";
const RESUME_RANDOM_ID: &str = "wallpaper-switcher.resume-random";
const CONFIG_ID: &str = "wallpaper-switcher.config";
const EXIT_ID: &str = "wallpaper-switcher.exit";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayAction {
    Open,
    Random,
    ToggleRandomRotation,
    StopPlaylist,
    ResumeRandomRotation,
    OpenConfig,
    Exit,
}

pub struct TrayState {
    _icon: TrayIcon,
    slideshow: CheckMenuItem,
    stop_playlist: MenuItem,
    resume_random: MenuItem,
}

impl TrayState {
    pub fn set_rotation_state(
        &self,
        state: crate::candidate_state::RandomRotationState,
        playlist_playing: bool,
    ) {
        use crate::candidate_state::RandomRotationState;
        self.slideshow
            .set_checked(state != RandomRotationState::Off);
        self.slideshow.set_text(state.label());
        self.stop_playlist.set_enabled(playlist_playing);
        self.resume_random
            .set_enabled(state == RandomRotationState::SafelyStopped);
    }
}

pub fn set_event_handler<F>(handler: F)
where
    F: Fn(TrayAction) + Send + Sync + 'static,
{
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let action = match event.id().as_ref() {
            OPEN_ID => Some(TrayAction::Open),
            RANDOM_ID => Some(TrayAction::Random),
            SLIDESHOW_ID => Some(TrayAction::ToggleRandomRotation),
            STOP_PLAYLIST_ID => Some(TrayAction::StopPlaylist),
            RESUME_RANDOM_ID => Some(TrayAction::ResumeRandomRotation),
            CONFIG_ID => Some(TrayAction::OpenConfig),
            EXIT_ID => Some(TrayAction::Exit),
            _ => None,
        };
        if let Some(action) = action {
            handler(action);
        }
    }));
}

fn tray_icon() -> Result<Icon, String> {
    let width = 32u32;
    let height = 32u32;
    let mut rgba = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        for x in 0..width {
            let dx = x as i32 - 16;
            let dy = y as i32 - 16;
            let inside = dx * dx + dy * dy <= 13 * 13;
            let (red, green, blue, alpha) = if inside {
                (235, 111, 146, 255)
            } else {
                (0, 0, 0, 0)
            };
            rgba.extend_from_slice(&[red, green, blue, alpha]);
        }
    }
    Icon::from_rgba(rgba, width, height)
        .map_err(|error| format!("Could not create tray icon: {error}"))
}

fn create_with_identity(
    slideshow_enabled: bool,
    guid: u128,
    tooltip: &str,
) -> Result<TrayState, String> {
    let open = MenuItem::with_id(OPEN_ID, "Open", true, None);
    let random = MenuItem::with_id(RANDOM_ID, "Random Wallpaper", true, None);
    let slideshow = CheckMenuItem::with_id(
        SLIDESHOW_ID,
        "Random rotation",
        true,
        slideshow_enabled,
        None,
    );
    let stop_playlist = MenuItem::with_id(STOP_PLAYLIST_ID, "Stop playlist", false, None);
    let resume_random = MenuItem::with_id(RESUME_RANDOM_ID, "Resume random rotation", false, None);
    let config = MenuItem::with_id(CONFIG_ID, "Open Config", true, None);
    let exit = MenuItem::with_id(EXIT_ID, "Exit", true, None);
    let separator = PredefinedMenuItem::separator();
    let menu = Menu::with_items(&[
        &open,
        &random,
        &slideshow,
        &stop_playlist,
        &resume_random,
        &config,
        &separator,
        &exit,
    ])
    .map_err(|error| format!("Could not create tray menu: {error}"))?;
    let icon = TrayIconBuilder::new()
        .with_guid(guid)
        .with_tooltip(tooltip)
        .with_icon(tray_icon()?)
        .with_menu(Box::new(menu))
        .build()
        .map_err(|error| format!("Could not create tray icon: {error}"))?;
    Ok(TrayState {
        _icon: icon,
        slideshow,
        stop_playlist,
        resume_random,
    })
}

pub fn create(slideshow_enabled: bool) -> Result<TrayState, String> {
    create_with_identity(
        slideshow_enabled,
        0x6f54b05b_e4a3_4ec4_9dc4_f71d641ec9c1,
        "Wallpaper Switcher",
    )
}

pub fn create_gpu_candidate(slideshow_enabled: bool) -> Result<TrayState, String> {
    create_with_identity(
        slideshow_enabled,
        0x49d3dc70_a670_46e7_a7ab_99ad646f6f5a,
        "Wallpaper Switcher GPU Candidate",
    )
}

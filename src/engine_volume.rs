use std::path::Path;

use windows::core::Interface;
use windows::Win32::Media::Audio::{
    eRender, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
    MMDeviceEnumerator, DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_ALL, COINIT_MULTITHREADED,
};

struct ComApartment;

impl ComApartment {
    fn initialize() -> Result<Self, String> {
        unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
            .ok()
            .map_err(|error| format!("Core Audio COM initialization failed: {error}"))?;
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

fn process_image_path(process_id: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };

    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, process_id) };
    if process.is_null() {
        return None;
    }
    let mut buffer = vec![0u16; 32_768];
    let mut length = buffer.len() as u32;
    let success = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            buffer.as_mut_ptr(),
            &mut length,
        )
    } != 0;
    unsafe { CloseHandle(process) };
    success.then(|| String::from_utf16_lossy(&buffer[..length as usize]))
}

fn is_engine_renderer_path(install_dir: &Path, candidate: &Path) -> bool {
    let root = install_dir
        .canonicalize()
        .unwrap_or_else(|_| install_dir.to_path_buf())
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase();
    let candidate = candidate
        .canonicalize()
        .unwrap_or_else(|_| candidate.to_path_buf())
        .to_string_lossy()
        .replace('/', "\\")
        .to_ascii_lowercase();
    if !candidate.starts_with(&format!("{root}\\")) {
        return false;
    }
    matches!(
        Path::new(&candidate)
            .file_stem()
            .and_then(|name| name.to_str()),
        Some("wallpaper32" | "wallpaper64" | "webwallpaper32" | "webwallpaper64")
    )
}

pub fn set_wallpaper_engine_volume(volume: u8) -> Result<(), String> {
    let install_dir = crate::scanner::wallpaper_engine_install_dir()
        .ok_or_else(|| "Wallpaper Engine installation was not found".to_string())?;
    let _apartment = ComApartment::initialize()?;
    let device_enumerator: IMMDeviceEnumerator = unsafe {
        CoCreateInstance(
            &MMDeviceEnumerator,
            None::<&windows::core::IUnknown>,
            CLSCTX_ALL,
        )
    }
    .map_err(|error| format!("Core Audio device enumeration is unavailable: {error}"))?;
    let devices = unsafe { device_enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }
        .map_err(|error| format!("Could not enumerate active audio outputs: {error}"))?;
    let count = unsafe { devices.GetCount() }
        .map_err(|error| format!("Could not count active audio outputs: {error}"))?;
    let level = f32::from(volume.min(100)) / 100.0;
    let mut matched = 0usize;
    let mut updated = 0usize;
    let mut errors = Vec::new();

    for device_index in 0..count {
        let result = (|| -> windows::core::Result<()> {
            let device = unsafe { devices.Item(device_index) }?;
            let manager: IAudioSessionManager2 = unsafe { device.Activate(CLSCTX_ALL, None) }?;
            let sessions = unsafe { manager.GetSessionEnumerator() }?;
            let session_count = unsafe { sessions.GetCount() }?;
            for session_index in 0..session_count {
                let session = unsafe { sessions.GetSession(session_index) }?;
                let control: IAudioSessionControl2 = session.cast()?;
                let process_id = unsafe { control.GetProcessId() }?;
                let Some(path) = process_image_path(process_id) else {
                    continue;
                };
                if !is_engine_renderer_path(&install_dir, Path::new(&path)) {
                    continue;
                }
                matched += 1;
                let volume_control: ISimpleAudioVolume = session.cast()?;
                unsafe { volume_control.SetMasterVolume(level, std::ptr::null()) }?;
                updated += 1;
            }
            Ok(())
        })();
        if let Err(error) = result {
            errors.push(error.to_string());
        }
    }

    if matched == 0 {
        return Err("No active Wallpaper Engine renderer audio session was found".to_string());
    }
    if updated != matched || !errors.is_empty() {
        return Err(format!(
            "Updated {updated} of {matched} Wallpaper Engine audio sessions{}",
            if errors.is_empty() {
                String::new()
            } else {
                format!(": {}", errors.join("; "))
            }
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_volume_scope_rejects_other_processes_and_directories() {
        let root = Path::new(r"C:\Steam\wallpaper_engine");
        assert!(is_engine_renderer_path(
            root,
            Path::new(r"C:\Steam\wallpaper_engine\wallpaper64.exe")
        ));
        assert!(is_engine_renderer_path(
            root,
            Path::new(r"C:\Steam\wallpaper_engine\bin\webwallpaper32.exe")
        ));
        assert!(!is_engine_renderer_path(
            root,
            Path::new(r"C:\Other\wallpaper64.exe")
        ));
        assert!(!is_engine_renderer_path(
            root,
            Path::new(r"C:\Steam\wallpaper_engine\notepad.exe")
        ));
    }
}

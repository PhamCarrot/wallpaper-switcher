use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Component, Path, PathBuf, Prefix};
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, Ordering};

use iced::Task;
use windows::core::PCWSTR;
use windows::Win32::System::Com::{
    CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::Common::ITEMIDLIST;
use windows::Win32::UI::Shell::{
    SHGetNameFromIDList, SHOpenFolderAndSelectItems, SHParseDisplayName, SIGDN_FILESYSPATH,
};
use windows_sys::Win32::UI::Shell::{
    ShellExecuteExW, SEE_MASK_IDLIST, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

use super::{GpuCandidate, Message};
use crate::scanner::WallpaperItem;

fn windows_spelling(path: &Path) -> PathBuf {
    let wide: Vec<_> = path
        .as_os_str()
        .encode_wide()
        .map(|unit| {
            if unit == u16::from(b'/') {
                u16::from(b'\\')
            } else {
                unit
            }
        })
        .collect();
    OsString::from_wide(&wide).into()
}

pub(super) fn copy_absolute_path(path: &Path) -> Option<String> {
    let path = windows_spelling(path);
    path.is_absolute().then(|| path.display().to_string())
}

#[derive(Debug, PartialEq, Eq)]
enum SourceFolder {
    SelectFile(PathBuf),
    OpenDirectory(PathBuf),
}

fn folder_action(file: &Path, folder: &Path) -> Option<SourceFolder> {
    let file = windows_spelling(file);
    if let Ok(metadata) = file.metadata() {
        if metadata.is_file() {
            return Some(SourceFolder::SelectFile(file));
        }
        if metadata.is_dir() {
            return Some(SourceFolder::OpenDirectory(file));
        }
    }
    let folder = windows_spelling(folder);
    folder
        .is_dir()
        .then_some(SourceFolder::OpenDirectory(folder))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RequestOwner {
    item: String,
    visibility: u64,
    window: Option<iced::window::Id>,
    status: Option<String>,
}

impl RequestOwner {
    pub(super) fn current(&self, app: &GpuCandidate) -> bool {
        app.visible
            && !app.apply_pending
            && app.window == self.window
            && app.visibility_generation == self.visibility
            && app.status == self.status
            && app
                .selected
                .and_then(|index| app.wallpapers.get(index))
                .is_some_and(|item| item.id == self.item)
    }
}

static REVEAL_BUSY: AtomicBool = AtomicBool::new(false);
struct RevealGuard;
impl Drop for RevealGuard {
    fn drop(&mut self) {
        REVEAL_BUSY.store(false, Ordering::Release);
    }
}

pub(super) fn show_source_folder(app: &GpuCandidate, item: &WallpaperItem) -> Task<Message> {
    if REVEAL_BUSY.swap(true, Ordering::AcqRel) {
        return Task::none();
    }
    let guard = RevealGuard;
    let owner = RequestOwner {
        item: item.id.clone(),
        visibility: app.visibility_generation,
        window: app.window,
        status: app.status.clone(),
    };
    let file = item.file_target.clone();
    let folder = item.folder_path.clone();
    let (send, receive) = futures_channel::oneshot::channel();
    let thread = std::thread::Builder::new()
        .name("show-source-folder".into())
        .spawn(move || {
            let _guard = guard;
            let result = folder_action(&file, &folder)
                .ok_or_else(|| "Source file and folder are missing.".to_owned())
                .and_then(invoke_shell);
            let _ = send.send(result);
        });
    if let Err(error) = thread {
        return Task::done(Message::SourceFolderError(owner, error.to_string()));
    }
    Task::future(receive).then(move |result| match result {
        Ok(Ok(())) => Task::none(),
        Ok(Err(error)) => Task::done(Message::SourceFolderError(owner.clone(), error)),
        Err(error) => Task::done(Message::SourceFolderError(owner.clone(), error.to_string())),
    })
}

fn invoke_shell(action: SourceFolder) -> Result<(), String> {
    let path = match &action {
        SourceFolder::SelectFile(path) | SourceFolder::OpenDirectory(path) => path,
    };
    with_shell_item(path, |pidl| unsafe {
        match &action {
            SourceFolder::SelectFile(_) => {
                SHOpenFolderAndSelectItems(pidl, None, 0).map_err(|error| error.to_string())
            }
            SourceFolder::OpenDirectory(_) => {
                let mut info = SHELLEXECUTEINFOW {
                    cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
                    fMask: SEE_MASK_IDLIST | SEE_MASK_NOASYNC,
                    lpIDList: pidl.cast(),
                    nShow: SW_SHOWNORMAL,
                    ..std::mem::zeroed()
                };
                if ShellExecuteExW(&mut info) == 0 {
                    Err(std::io::Error::last_os_error().to_string())
                } else {
                    Ok(())
                }
            }
        }
    })
}

fn with_shell_item<T>(
    path: &Path,
    use_item: impl FnOnce(*mut ITEMIDLIST) -> Result<T, String>,
) -> Result<T, String> {
    // Each background request owns the apartment, absolute PIDL and its names.
    let ordinary = ordinary_shell_path(path);
    let shell_path = ordinary.as_deref().unwrap_or(path);
    let mut wide: Vec<_> = shell_path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return Err("Invalid source path.".into());
    }
    wide.push(0);
    unsafe {
        CoInitializeEx(None, COINIT_APARTMENTTHREADED)
            .ok()
            .map_err(|error| error.to_string())?;
        let mut pidl = null_mut();
        let result = match SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None) {
            Ok(()) if !pidl.is_null() => {
                // Ordinary shell parsing may normalize a verbatim component. It
                // is safe to use its PIDL only if it resolved to the original file.
                if ordinary.is_some() {
                    shell_item_path(pidl).and_then(|resolved| {
                        let expected = path.canonicalize().map_err(|e| e.to_string())?;
                        if resolved.canonicalize().ok().as_ref() != Some(&expected) {
                            return Err("Windows resolved a different source path.".into());
                        }
                        use_item(pidl)
                    })
                } else {
                    use_item(pidl)
                }
            }
            Ok(()) => Err("Windows did not resolve the source path.".into()),
            Err(error) => Err(error.to_string()),
        };
        CoTaskMemFree(Some(pidl.cast()));
        CoUninitialize();
        result
    }
}

fn shell_item_path(pidl: *const ITEMIDLIST) -> Result<PathBuf, String> {
    unsafe {
        let name =
            SHGetNameFromIDList(pidl, SIGDN_FILESYSPATH).map_err(|error| error.to_string())?;
        if name.is_null() {
            return Err("Windows did not resolve the source path.".into());
        }
        let path = OsString::from_wide(name.as_wide()).into();
        CoTaskMemFree(Some(name.0.cast()));
        Ok(path)
    }
}

fn ordinary_shell_path(path: &Path) -> Option<PathBuf> {
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return None;
    };
    // The shell parses ordinary drive and UNC names, including long names.
    // Preserve the verbatim spelling everywhere outside Shell parsing.
    let units = path.as_os_str().encode_wide();
    let ordinary: Vec<_> = match prefix.kind() {
        Prefix::VerbatimDisk(_) => units.skip(4).collect(),
        Prefix::VerbatimUNC(_, _) => [b'\\' as u16; 2].into_iter().chain(units.skip(8)).collect(),
        _ => return None,
    };
    Some(OsString::from_wide(&ordinary).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_file_paths_keep_windows_case_unicode_and_roots() {
        for (input, expected) in [
            (
                r"D:/Steam/壁紙, café/project.json",
                r"D:\Steam\壁紙, café\project.json",
            ),
            (
                r"//server/Share/壁紙/project.json",
                r"\\server\Share\壁紙\project.json",
            ),
            (
                r"\\?\J:\Long Name\project.json",
                r"\\?\J:\Long Name\project.json",
            ),
            (r"\\?\UNC\server\Share\壁紙", r"\\?\UNC\server\Share\壁紙"),
            (r"J:/", r"J:\"),
        ] {
            assert_eq!(
                copy_absolute_path(Path::new(input)).as_deref(),
                Some(expected)
            );
        }
        assert!(copy_absolute_path(Path::new(r"J:project.json")).is_none());
    }

    #[test]
    fn detail_file_shell_resolves_the_exact_extended_path_without_opening_explorer() {
        let root = std::env::temp_dir().join(format!("detail-shell-{}", std::process::id()));
        let mut folder = root.clone();
        for part in 0..6 {
            folder = folder.join(format!("{part}-{}", "long-name".repeat(6)));
        }
        for folder in [root, folder] {
            std::fs::create_dir_all(&folder).unwrap();
            let file = folder.join("file name, 壁紙.json");
            std::fs::write(&file, b"{}").unwrap();
            let expected = file.canonicalize().unwrap();
            assert_ne!(
                ordinary_shell_path(&expected).as_deref(),
                Some(expected.as_path())
            );
            for spelling in [&file, &expected] {
                let resolved = with_shell_item(spelling, |pidl| shell_item_path(pidl)).unwrap();
                assert_eq!(resolved.canonicalize().unwrap(), expected);
            }
        }
        assert_eq!(
            ordinary_shell_path(Path::new(r"\\?\UNC\server\Share\壁紙")).as_deref(),
            Some(Path::new(r"\\server\Share\壁紙")),
        );
    }

    #[test]
    fn detail_file_action_selects_only_files_and_opens_existing_directories() {
        let root = std::env::temp_dir().join(format!("detail-file-action-{}", std::process::id()));
        let folder = root.join("壁紙, café");
        std::fs::create_dir_all(&folder).unwrap();
        let file = folder.join("file name, 壁紙.json");
        std::fs::write(&file, b"{}").unwrap();
        assert_eq!(
            folder_action(&file, &folder),
            Some(SourceFolder::SelectFile(windows_spelling(&file)))
        );
        let missing = folder.join("missing");
        assert_eq!(
            folder_action(&missing, &folder),
            Some(SourceFolder::OpenDirectory(windows_spelling(&folder)))
        );
        assert_eq!(
            folder_action(&folder, &missing),
            Some(SourceFolder::OpenDirectory(windows_spelling(&folder)))
        );
        assert_eq!(folder_action(&missing, &missing), None);
    }
}

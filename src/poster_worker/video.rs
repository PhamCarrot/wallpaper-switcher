//! Synchronous, software-only frame extraction on the existing poster lanes.
use std::{ffi::CStr, os::windows::ffi::OsStrExt, path::Path, ptr::null_mut};
use windows::{
    core::{Interface, HRESULT, PCWSTR},
    Win32::{
        Media::MediaFoundation::*,
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
    },
};
use windows_sys::Win32::{
    Foundation::{FreeLibrary, HMODULE},
    System::LibraryLoader::{GetProcAddress, LoadLibraryExW, LOAD_LIBRARY_SEARCH_SYSTEM32},
};

struct Library(HMODULE);

impl Library {
    fn load(name: PCWSTR) -> Option<Self> {
        // Media Foundation can be absent on Windows N. Load only system DLLs,
        // so missing components are a per-item fallback even at app startup.
        let module = unsafe { LoadLibraryExW(name.0, null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) };
        (!module.is_null()).then_some(Self(module))
    }

    fn symbol(&self, name: &CStr) -> Option<unsafe extern "system" fn() -> isize> {
        unsafe { GetProcAddress(self.0, name.as_ptr().cast()) }
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { FreeLibrary(self.0) };
        }
    }
}

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

struct Platform(unsafe extern "system" fn() -> HRESULT);
impl Drop for Platform {
    fn drop(&mut self) {
        let _ = unsafe { (self.0)() };
    }
}

// Limit the source surface before negotiating RGB32 and recheck the actual
// output type. A synchronous codec call can still block inside Windows.
fn dimensions(media: &IMFMediaType) -> Option<(u32, u32)> {
    let size = unsafe { media.GetUINT64(&MF_MT_FRAME_SIZE) }.ok()?;
    let (width, height) = ((size >> 32) as u32, size as u32);
    (width > 0
        && height > 0
        && width <= 8192
        && height <= 8192
        && u64::from(width) * u64::from(height) <= 4096 * 2304)
        .then_some((width, height))
}

fn display_area(media: &IMFMediaType, surface: (u32, u32)) -> ((u32, u32), (u32, u32)) {
    // The RGB converter may align the surface to codec block boundaries.
    // Only the display aperture contains valid picture pixels.
    [MF_MT_MINIMUM_DISPLAY_APERTURE, MF_MT_GEOMETRIC_APERTURE]
        .iter()
        .find_map(|key| {
            let mut bytes = [0; std::mem::size_of::<MFVideoArea>()];
            let mut length = 0;
            unsafe { media.GetBlob(key, &mut bytes, Some(&mut length)) }.ok()?;
            if length as usize != bytes.len() {
                return None;
            }
            // MFVideoArea contains only integers and describes this fixed-size blob.
            let area = unsafe { bytes.as_ptr().cast::<MFVideoArea>().read_unaligned() };
            let x = u32::try_from(area.OffsetX.value).ok()?;
            let y = u32::try_from(area.OffsetY.value).ok()?;
            let width = u32::try_from(area.Area.cx).ok()?;
            let height = u32::try_from(area.Area.cy).ok()?;
            (width > 0
                && height > 0
                && x.checked_add(width)? <= surface.0
                && y.checked_add(height)? <= surface.1)
                .then_some(((x, y), (width, height)))
        })
        .unwrap_or(((0, 0), surface))
}

pub(crate) fn with_frame<T>(
    path: &Path,
    wanted: &dyn Fn() -> bool,
    render: impl FnOnce(&Frame<'_>) -> Option<T>,
) -> Option<T> {
    if !wanted() || !path.is_absolute() {
        return None;
    }
    let path: Vec<_> = path.as_os_str().encode_wide().chain([0]).collect();
    let platform_dll = Library::load(windows::core::w!("mfplat.dll"))?;
    let reader_dll = Library::load(windows::core::w!("mfreadwrite.dll"))?;
    // These exports use the signatures in mfapi.h and mfreadwrite.h. All
    // COM objects and the startup guard drop before either DLL lease.
    unsafe {
        let startup: unsafe extern "system" fn(u32, u32) -> HRESULT =
            std::mem::transmute(platform_dll.symbol(c"MFStartup")?);
        let shutdown = std::mem::transmute(platform_dll.symbol(c"MFShutdown")?);
        let open_file: unsafe extern "system" fn(
            MF_FILE_ACCESSMODE,
            MF_FILE_OPENMODE,
            MF_FILE_FLAGS,
            PCWSTR,
            *mut Option<IMFByteStream>,
        ) -> HRESULT = std::mem::transmute(platform_dll.symbol(c"MFCreateFile")?);
        let create_attributes: unsafe extern "system" fn(
            *mut Option<IMFAttributes>,
            u32,
        ) -> HRESULT = std::mem::transmute(platform_dll.symbol(c"MFCreateAttributes")?);
        let create_type: unsafe extern "system" fn(*mut Option<IMFMediaType>) -> HRESULT =
            std::mem::transmute(platform_dll.symbol(c"MFCreateMediaType")?);
        let create_reader: unsafe extern "system" fn(
            *mut core::ffi::c_void,
            *mut core::ffi::c_void,
            *mut Option<IMFSourceReader>,
        ) -> HRESULT =
            std::mem::transmute(reader_dll.symbol(c"MFCreateSourceReaderFromByteStream")?);

        CoInitializeEx(None, COINIT_MULTITHREADED).ok().ok()?;
        let _apartment = Apartment;
        startup(MF_VERSION, MFSTARTUP_NOSOCKET).ok().ok()?;
        let _platform = Platform(shutdown);
        let mut stream = None;
        open_file(
            MF_ACCESSMODE_READ,
            MF_OPENMODE_FAIL_IF_NOT_EXIST,
            MF_FILEFLAGS_NONE,
            PCWSTR(path.as_ptr()),
            &mut stream,
        )
        .ok()
        .ok()?;
        let stream = stream?;
        let mut attributes = None;
        create_attributes(&mut attributes, 1).ok().ok()?;
        let attributes = attributes?;
        attributes
            .SetUINT32(&MF_SOURCE_READER_ENABLE_VIDEO_PROCESSING, 1)
            .ok()?;
        let mut reader = None;
        create_reader(stream.as_raw(), attributes.as_raw(), &mut reader)
            .ok()
            .ok()?;
        let reader = reader?;
        let video = MF_SOURCE_READER_FIRST_VIDEO_STREAM.0 as u32;
        reader
            .SetStreamSelection(MF_SOURCE_READER_ALL_STREAMS.0 as u32, false)
            .ok()?;
        reader.SetStreamSelection(video, true).ok()?;
        dimensions(&reader.GetNativeMediaType(video, 0).ok()?)?;
        if !wanted() {
            return None;
        }
        let mut media = None;
        create_type(&mut media).ok().ok()?;
        let media = media?;
        media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ok()?;
        media.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).ok()?;
        reader.SetCurrentMediaType(video, None, &media).ok()?;
        for _ in 0..12 {
            if !wanted() {
                return None;
            }
            let mut flags = 0;
            let mut sample = None;
            reader
                .ReadSample(video, 0, None, Some(&mut flags), None, Some(&mut sample))
                .ok()?;
            if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 || !wanted() {
                return None;
            }
            if let Some(sample) = sample {
                let actual = reader.GetCurrentMediaType(video).ok()?;
                let size = dimensions(&actual)?;
                if actual.GetGUID(&MF_MT_SUBTYPE).ok()? != MFVideoFormat_RGB32 {
                    return None;
                }
                let buffer = sample.ConvertToContiguousBuffer().ok()?;
                return render_buffer(&buffer, &actual, size, render);
            }
            if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
                break;
            }
        }
    }
    None
}

enum Unlock<'a> {
    Surface(&'a IMF2DBuffer2),
    Linear(&'a IMFMediaBuffer),
}
impl Drop for Unlock<'_> {
    fn drop(&mut self) {
        let _ = unsafe {
            match self {
                Self::Surface(buffer) => buffer.Unlock2D(),
                Self::Linear(buffer) => buffer.Unlock(),
            }
        };
    }
}

pub(crate) struct Frame<'a> {
    size: (u32, u32),
    first_row: *const u8,
    pitch: isize,
    _lock: Unlock<'a>,
}

impl Frame<'_> {
    pub(crate) fn size(&self) -> (u32, u32) {
        self.size
    }

    pub(crate) fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        assert!(x < self.size.0 as usize && y < self.size.1 as usize);
        // Frame construction verified that every row's active pixels fit
        // inside the locked allocation, including bottom-up surfaces.
        unsafe {
            let pixel = self.first_row.offset(y as isize * self.pitch).add(x * 4);
            [*pixel.add(2), *pixel.add(1), *pixel, 255]
        }
    }
}

fn render_buffer<T>(
    buffer: &IMFMediaBuffer,
    media: &IMFMediaType,
    size: (u32, u32),
    render: impl FnOnce(&Frame<'_>) -> Option<T>,
) -> Option<T> {
    let ((x, y), visible_size) = display_area(media, size);
    let surface = buffer.cast::<IMF2DBuffer2>().ok();
    let (mut start, mut first, mut pitch, mut length) = (null_mut(), null_mut(), 0, 0);
    // Both successful lock calls are paired with Unlock even on unwind.
    let lock = unsafe {
        if let Some(surface) = &surface {
            surface
                .Lock2DSize(
                    MF2DBuffer_LockFlags_Read,
                    &mut first,
                    &mut pitch,
                    &mut start,
                    &mut length,
                )
                .ok()?;
            Unlock::Surface(surface)
        } else {
            pitch = media
                .GetUINT32(&MF_MT_DEFAULT_STRIDE)
                .map_or(-(size.0 as i32 * 4), |stride| stride as i32);
            buffer.Lock(&mut start, None, Some(&mut length)).ok()?;
            first = if pitch < 0 {
                start.wrapping_add(pitch.unsigned_abs() as usize * (size.1 as usize - 1))
            } else {
                start
            };
            Unlock::Linear(buffer)
        }
    };
    let row_bytes = size.0 as usize * 4;
    let offset = first.addr().checked_sub(start.addr())?;
    let distance = (pitch.unsigned_abs() as usize).checked_mul(size.1 as usize - 1)?;
    let last = offset.checked_add_signed(if pitch < 0 {
        -(isize::try_from(distance).ok()?)
    } else {
        isize::try_from(distance).ok()?
    })?;
    if start.is_null()
        || (pitch.unsigned_abs() as usize) < row_bytes
        || offset.max(last).checked_add(row_bytes)? > length as usize
    {
        return None;
    }
    let frame = Frame {
        size: visible_size,
        first_row: first
            .wrapping_offset(y as isize * pitch as isize)
            .wrapping_add(x as usize * 4),
        pitch: pitch as isize,
        _lock: lock,
    };
    render(&frame)
}

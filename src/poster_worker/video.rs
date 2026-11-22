//! Synchronous, software-only frame extraction on the existing poster lanes.
use std::time::{Duration, Instant};
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

#[derive(Debug, Clone, Copy)]
struct DisplayArea {
    origin: [f64; 2],
    span: [f64; 2],
}

impl DisplayArea {
    fn full((width, height): (u32, u32)) -> Self {
        Self {
            origin: [0.0; 2],
            span: [width.into(), height.into()],
        }
    }

    fn crop(area: MFVideoArea, surface: (u32, u32)) -> Option<Self> {
        let offset = |value: MFOffset| f64::from(value.value) + f64::from(value.fract) / 65536.0;
        let raw = [offset(area.OffsetX), offset(area.OffsetY)];
        let limit = [f64::from(surface.0), f64::from(surface.1)];
        let dimensions = [f64::from(area.Area.cx), f64::from(area.Area.cy)];
        let origin = std::array::from_fn(|axis| raw[axis].clamp(0.0, limit[axis]));
        let span = std::array::from_fn(|axis| {
            (raw[axis] + dimensions[axis]).clamp(0.0, limit[axis]) - origin[axis]
        });
        span.iter()
            .all(|value| *value >= 1.0)
            .then_some(Self { origin, span })
    }

    fn size(self) -> (u32, u32) {
        (self.span[0].round() as u32, self.span[1].round() as u32)
    }

    fn fractional_pixel(
        self,
        surface: (u32, u32),
        (x, y): (usize, usize),
        pixel: impl Fn(usize, usize) -> [u8; 4],
    ) -> [u8; 4] {
        let (width, height) = self.size();
        let bounds = |axis: usize, limit: u32| {
            let first = (self.origin[axis] - 0.5).ceil().max(0.0);
            let last = ((self.origin[axis] + self.span[axis] - 0.5).ceil() - 1.0)
                .min(f64::from(limit - 1));
            (first, last.max(first))
        };
        let at = |axis: usize, number: usize, count: u32, limit: u32| {
            let (first, last) = bounds(axis, limit);
            (self.origin[axis] + (number as f64 + 0.5) * self.span[axis] / f64::from(count) - 0.5)
                .clamp(first, last)
        };
        let (x, y) = (at(0, x, width, surface.0), at(1, y, height, surface.1));
        let (left, top) = (x.floor() as usize, y.floor() as usize);
        let (fx, fy) = (x.fract(), y.fract());
        let a = pixel(left, top);
        if fx == 0.0 && fy == 0.0 {
            return a;
        }
        let right = (left + 1).min(bounds(0, surface.0).1 as usize);
        let bottom = (top + 1).min(bounds(1, surface.1).1 as usize);
        let b = pixel(right, top);
        let c = pixel(left, bottom);
        let d = pixel(right, bottom);
        std::array::from_fn(|channel| {
            let blend = |a: u8, b: u8, t| f64::from(a) + (f64::from(b) - f64::from(a)) * t;
            let first = blend(a[channel], b[channel], fx);
            let second = blend(c[channel], d[channel], fx);
            (first + (second - first) * fy).round() as u8
        })
    }
}

fn display_area(media: &IMFMediaType, surface: (u32, u32)) -> Option<DisplayArea> {
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
            DisplayArea::crop(area, surface)
        })
}

fn pixel_aspect(media: &IMFMediaType) -> Option<f64> {
    let ratio = unsafe { media.GetUINT64(&MF_MT_PIXEL_ASPECT_RATIO) }.ok()?;
    let (num, den) = ((ratio >> 32) as u32, ratio as u32);
    (num > 0 && den > 0).then(|| f64::from(num) / f64::from(den))
}

fn rotation(media: &IMFMediaType) -> Option<u32> {
    unsafe { media.GetUINT32(&MF_MT_VIDEO_ROTATION) }
        .ok()
        .filter(|degrees| matches!(degrees, 90 | 180 | 270))
        // The attribute describes how the source pixels are oriented. Undo
        // that rotation to display an upright frame.
        .map(|degrees| 360 - degrees)
}

fn first_sample<T>(
    wanted: &dyn Fn() -> bool,
    mut read: impl FnMut() -> Option<(u32, Option<T>)>,
) -> Option<T> {
    let started = Instant::now();
    for _ in 0..128 {
        if !wanted() || started.elapsed() >= Duration::from_secs(3) {
            break;
        }
        let (flags, sample) = read()?;
        if flags & MF_SOURCE_READERF_ERROR.0 as u32 != 0 || !wanted() {
            return None;
        }
        if sample.is_some() {
            return sample;
        }
        if flags & MF_SOURCE_READERF_ENDOFSTREAM.0 as u32 != 0 {
            break;
        }
    }
    None
}

#[cfg(test)]
thread_local! {
    static READ_BOUNDARY: std::cell::RefCell<Option<Box<dyn FnOnce()>>> = const { std::cell::RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn hold_next_read(gate: impl FnOnce() + 'static) {
    READ_BOUNDARY.with(|slot| *slot.borrow_mut() = Some(Box::new(gate)));
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
        let native = reader.GetNativeMediaType(video, 0).ok()?;
        let native_size = dimensions(&native)?;
        if !wanted() {
            return None;
        }
        let mut media = None;
        create_type(&mut media).ok().ok()?;
        let media = media?;
        media.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video).ok()?;
        media.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_RGB32).ok()?;
        reader.SetCurrentMediaType(video, None, &media).ok()?;
        let sample = first_sample(wanted, || {
            // An in-flight synchronous read belongs to this worker. Generation
            // and epoch invalidation stay prompt; cache clear waits for return.
            if !wanted() {
                return None;
            }
            let mut flags = 0;
            let mut sample = None;
            reader
                .ReadSample(video, 0, None, Some(&mut flags), None, Some(&mut sample))
                .ok()?;
            let sample = sample.filter(|sample| sample.GetTotalLength().is_ok_and(|size| size > 0));
            #[cfg(test)]
            if sample.is_some() {
                if let Some(gate) = READ_BOUNDARY.with(|slot| slot.borrow_mut().take()) {
                    gate();
                }
            }
            Some((flags, sample))
        })?;
        let actual = reader.GetCurrentMediaType(video).ok()?;
        let size = dimensions(&actual)?;
        if actual.GetGUID(&MF_MT_SUBTYPE).ok()? != MFVideoFormat_RGB32 {
            return None;
        }
        let area = display_area(&actual, size)
            .or_else(|| display_area(&native, size))
            .unwrap_or_else(|| DisplayArea::full(size));
        let aspect = pixel_aspect(&native)
            .or_else(|| pixel_aspect(&actual))
            .unwrap_or(1.0);
        let rotation = rotation(&native).or_else(|| rotation(&actual)).unwrap_or(0);
        crate::apply_trace::event_lazy("poster-video-format", || {
            format!(
            "native={native_size:?} surface={size:?} aperture={area:?} par={aspect} rotation={rotation}"
        )
        });
        let buffer = sample.ConvertToContiguousBuffer().ok()?;
        render_buffer(&buffer, &actual, size, area, aspect, rotation, render)
    }
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
    surface: (u32, u32),
    area: DisplayArea,
    pixel_aspect: f64,
    rotation: u32,
    integer_origin: Option<[usize; 2]>,
    first_row: *const u8,
    pitch: isize,
    _lock: Unlock<'a>,
}

impl Frame<'_> {
    pub(crate) fn size(&self) -> (u32, u32) {
        let (width, height) = self.area.size();
        if matches!(self.rotation, 90 | 270) {
            (height, width)
        } else {
            (width, height)
        }
    }

    pub(crate) fn display_extent(&self) -> (f64, f64) {
        let (width, height) = (self.area.span[0] * self.pixel_aspect, self.area.span[1]);
        let (width, height) = if matches!(self.rotation, 90 | 270) {
            (height, width)
        } else {
            (width, height)
        };
        let aspect = width / height;
        if (1.0 / 8192.0..=8192.0).contains(&aspect) {
            (width, height)
        } else {
            (aspect.clamp(1.0 / 8192.0, 8192.0), 1.0)
        }
    }

    pub(crate) fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let (oriented_w, oriented_h) = self.size();
        assert!(x < oriented_w as usize && y < oriented_h as usize);
        let (width, height) = self.area.size();
        let (x, y) = match self.rotation {
            90 => (width as usize - 1 - y, x),
            180 => (width as usize - 1 - x, height as usize - 1 - y),
            270 => (y, height as usize - 1 - x),
            _ => (x, y),
        };
        if let Some([left, top]) = self.integer_origin {
            return self.surface_pixel(left + x, top + y);
        }
        self.area
            .fractional_pixel(self.surface, (x, y), |x, y| self.surface_pixel(x, y))
    }

    fn surface_pixel(&self, x: usize, y: usize) -> [u8; 4] {
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
    area: DisplayArea,
    pixel_aspect: f64,
    rotation: u32,
    render: impl FnOnce(&Frame<'_>) -> Option<T>,
) -> Option<T> {
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
    let integer_origin = area
        .origin
        .iter()
        .chain(&area.span)
        .all(|value| value.fract() == 0.0)
        .then(|| area.origin.map(|value| value as usize));
    let frame = Frame {
        surface: size,
        area,
        pixel_aspect,
        rotation,
        integer_origin,
        first_row: first,
        pitch: pitch as isize,
        _lock: lock,
    };
    render(&frame)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::SIZE;

    #[test]
    fn video_signed_fractional_aperture_samples_expected_rgb_columns() {
        let surface = (4, 4);
        let metadata = MFVideoArea {
            OffsetX: MFOffset {
                value: 0,
                fract: 32768,
            },
            OffsetY: MFOffset {
                value: -1,
                fract: 32768,
            },
            Area: SIZE { cx: 3, cy: 3 },
        };
        let crop = DisplayArea::crop(metadata, surface).unwrap();
        assert_eq!(crop.origin, [0.5, 0.0]);
        assert_eq!(crop.span, [3.0, 2.5]);
        let rgb = |x: usize, y: usize| [(x * 80) as u8, (y * 60) as u8, 0, 255];
        assert_eq!(crop.fractional_pixel(surface, (0, 0), rgb), [40, 0, 0, 255]);
        assert_eq!(
            crop.fractional_pixel(surface, (1, 1), rgb),
            [120, 45, 0, 255]
        );
        assert_eq!(
            crop.fractional_pixel(surface, (2, 2), rgb),
            [160, 60, 0, 255],
            "crop must not mix in the padding row or column"
        );
    }

    #[test]
    fn video_sample_loop_skips_empty_ticks_and_bounds_eof_error_and_cancellation() {
        let tick = MF_SOURCE_READERF_STREAMTICK.0 as u32;
        let eof = MF_SOURCE_READERF_ENDOFSTREAM.0 as u32;
        let mut reads = 0;
        let late = first_sample(&|| true, || {
            reads += 1;
            Some(if reads == 25 {
                (0, Some(123))
            } else {
                (tick, None)
            })
        });
        assert_eq!((late, reads), (Some(123), 25));
        let mut reads = 0;
        let empty = first_sample::<()>(&|| true, || {
            reads += 1;
            Some((eof, None))
        });
        assert!(empty.is_none());
        assert_eq!(reads, 1);
        assert!(first_sample(&|| true, || Some((
            MF_SOURCE_READERF_ERROR.0 as u32,
            Some(1)
        )))
        .is_none());
        let cancel = std::cell::Cell::new(false);
        assert!(first_sample(&|| !cancel.get(), || {
            cancel.set(true);
            Some((0, Some(1)))
        })
        .is_none());
        let mut reads = 0;
        assert!(first_sample::<()>(&|| true, || {
            reads += 1;
            Some((tick, None))
        })
        .is_none());
        assert_eq!(reads, 128);
    }
}

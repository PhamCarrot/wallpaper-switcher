use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, Weak};

use iced::widget::shader::{self, Action, Pipeline, Primitive, Viewport};
use iced::{mouse, Event, Point, Rectangle, Size};

use crate::apply_trace;
use crate::donor_layout::{
    self, GridLayout as DonorGridLayout, GridParams as DonorGridParams, HexCurve as DonorHexCurve,
    HexParams as DonorHexParams, HexShape as DonorHexShape, StageParams,
};
use crate::donor_scene::{scene_instances, DonorSceneState, InstanceRaw};
use crate::library::media_still::{is_gif, Fingerprint};
use crate::poster_worker::buffer_pool::{BufferPool, PosterBytes};
use crate::poster_worker::cache::{self, PosterCache, ReadBatch, WriteBatch};
use crate::poster_worker::{video, PosterKind, PosterSource, PosterStamp};
use crate::scanner::VideoStamp;
use crate::upstream_slices_adapter::{build_slice_layout, slice_edge_tilt, slice_opacity};

// The gallery keeps a bounded poster set, but selected cards and detail enlarge
// these textures substantially. 512px preserves that bounded cache while
// avoiding the severe magnification blur of the former 256px tier.
pub const POSTER_WIDTH: u32 = 512;
pub const POSTER_HEIGHT: u32 = 512;
pub(crate) const POSTER_MATTE_RGB: [u8; 3] = [30, 30, 46];
pub(crate) const POSTER_BYTES: usize = (POSTER_WIDTH * POSTER_HEIGHT * 4) as usize;
const POSTER_UPLOAD_BYTES_PER_FRAME: usize = 8 * 1024 * 1024;
const POSTER_GROWTH_LAYERS: u32 = 8;
// Stop at 16px, as in the reference near tier. Every BC level is block aligned.
const NEAR_MIP_COUNT: u32 = 6;
pub const MAX_POSTERS: usize = 32;
/// Maximum number of logical poster slots shared by drawing and demand.
/// Depth's retained outgoing/live resource contract remains MAX_POSTERS.
pub const MAX_ORDINARY_POSTERS: usize = 256;
static POSTER_LAYER_LIMIT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(MAX_ORDINARY_POSTERS);
pub(crate) fn ordinary_poster_limit() -> usize {
    POSTER_LAYER_LIMIT.load(std::sync::atomic::Ordering::Relaxed)
}

const SLICE_VARIANTS: [&str; 3] = ["CLASSIC", "DENSE", "CINEMA"];
const GEOMETRIC_VARIANTS: [&str; 4] = ["HEX ARC", "TRI WAVE", "SHOWCASE DIAMOND", "RHOMBUS"];
const WALL_VARIANTS: [&str; 6] = [
    "UNIFORM",
    "BRICK",
    "MASONRY",
    "JUSTIFIED",
    "EDITORIAL",
    "CYLINDER",
];
const DEFAULT_POSTER_ASPECT: f32 = 16.0 / 9.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GalleryMode {
    Slices,
    Geometric,
    Wall,
    Depth,
}

impl GalleryMode {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Slices => "Slices",
            Self::Geometric => "Geometric",
            Self::Wall => "Wall",
            Self::Depth => "Depth",
        }
    }

    pub const fn variant_labels(self) -> &'static [&'static str] {
        match self {
            Self::Slices => &SLICE_VARIANTS,
            Self::Geometric => &GEOMETRIC_VARIANTS,
            Self::Wall => &WALL_VARIANTS,
            Self::Depth => &[],
        }
    }

    pub fn variant_label(self, index: usize) -> &'static str {
        let labels = self.variant_labels();
        if labels.is_empty() {
            "Custom"
        } else {
            labels[index % labels.len()]
        }
    }
}

#[derive(Debug, Clone)]
pub struct Poster {
    pub layer: u32,
    pub source: PathBuf,
    /// Width / height of the decoded image content, before it is fitted to the square texture.
    pub aspect_ratio: f32,
    pixels: Arc<Mutex<PosterPixels>>,
    far_pixels: Arc<Mutex<PosterPixels>>,
    recovery: PosterRecovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PosterTier {
    Near,
    Far,
}

#[derive(Debug)]
pub(crate) enum PosterDecodeError {
    Cancelled,
    MissingStill,
    SourceChanged,
    Dimensions(image::ImageError),
    Image(image::ImageError),
    InvalidPixels,
}

impl PosterDecodeError {
    pub(crate) fn is_file_lock(&self) -> bool {
        match self {
            Self::Dimensions(image::ImageError::IoError(io))
            | Self::Image(image::ImageError::IoError(io)) => matches!(
                io.raw_os_error().map(|code| code as u32),
                Some(
                    windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION
                        | windows_sys::Win32::Foundation::ERROR_LOCK_VIOLATION
                )
            ),
            _ => false,
        }
    }
}

impl std::fmt::Display for PosterDecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("poster request was cancelled"),
            Self::MissingStill => f.write_str("no usable still was captured"),
            Self::SourceChanged => f.write_str("poster source changed or became unreadable"),
            Self::Dimensions(error) => write!(f, "could not read image dimensions: {error}"),
            Self::Image(error) => write!(f, "could not decode image pixels: {error}"),
            Self::InvalidPixels => f.write_str("invalid poster pixels or dimensions"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PosterEncoding {
    // Canonical cache/pool canvas; never a packed GPU upload.
    NearRgba,
    NearRgbaMips,
    NearBc7,
    FarRgba,
    FarBc1,
}

impl PosterEncoding {
    pub(crate) fn is_near(self) -> bool {
        self.tier() == PosterTier::Near
    }

    fn tier(self) -> PosterTier {
        match self {
            Self::NearRgba | Self::NearRgbaMips | Self::NearBc7 => PosterTier::Near,
            Self::FarRgba | Self::FarBc1 => PosterTier::Far,
        }
    }

    fn for_texture(tier: PosterTier, bc: bool) -> Self {
        match (tier, bc) {
            (PosterTier::Near, false) => Self::NearRgbaMips,
            (PosterTier::Near, true) => Self::NearBc7,
            (PosterTier::Far, false) => Self::FarRgba,
            (PosterTier::Far, true) => Self::FarBc1,
        }
    }

    fn texture_format(self) -> wgpu::TextureFormat {
        match self {
            Self::NearRgba | Self::NearRgbaMips | Self::FarRgba => {
                wgpu::TextureFormat::Rgba8UnormSrgb
            }
            Self::NearBc7 => wgpu::TextureFormat::Bc7RgbaUnormSrgb,
            Self::FarBc1 => wgpu::TextureFormat::Bc1RgbaUnormSrgb,
        }
    }

    // Called on the poster worker (or by the isolated test fixtures).
    fn encode(self, rgba: Arc<PosterBytes>, aspect: f32) -> Option<Arc<PosterBytes>> {
        if rgba.len() != POSTER_BYTES {
            return None;
        }
        match self {
            Self::NearRgba => Some(rgba),
            Self::NearRgbaMips | Self::NearBc7 => {
                let mut level = rgba.to_vec();
                let mut packed = Vec::with_capacity(near_bytes(self.texture_format()));
                for mip in 0..NEAR_MIP_COUNT {
                    let extent = near_extent(mip);
                    if self == Self::NearBc7 {
                        packed.extend(intel_tex_2::bc7::compress_blocks(
                            &intel_tex_2::bc7::opaque_fast_settings(),
                            &intel_tex_2::RgbaSurface {
                                width: extent.width,
                                height: extent.height,
                                stride: extent.width * 4,
                                data: &level,
                            },
                        ));
                    } else {
                        packed.extend_from_slice(&level);
                    }
                    if mip + 1 < NEAR_MIP_COUNT {
                        level = downsample_near(&level, extent.width);
                    }
                }
                Some(Arc::new(packed.into()))
            }
            Self::FarRgba | Self::FarBc1 => self
                .encode_far(&rgba, aspect)
                .map(|bytes| Arc::new(bytes.into())),
        }
    }

    fn encode_far(self, rgba: &[u8], aspect: f32) -> Option<Vec<u8>> {
        let tile = far_tile_pixels(rgba, aspect)?;
        if self == Self::FarBc1 {
            return Some(pack_far_bc1(&tile));
        }
        Some(tile)
    }
}

fn pack_far_bc1(tile: &[u8]) -> Vec<u8> {
    intel_tex_2::bc1::compress_blocks(&intel_tex_2::RgbaSurface {
        width: FAR_WIDTH,
        height: FAR_HEIGHT,
        stride: FAR_WIDTH * 4,
        data: tile,
    })
}

#[derive(Debug)]
enum PosterPixels {
    Ready(PosterEncoding, Arc<PosterBytes>),
    Consumed,
    Restoring,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum PosterRecovery {
    Image {
        fingerprint: Fingerprint,
        cache_path: Option<PathBuf>,
    },
    Video {
        stamp: VideoStamp,
        cache_path: Option<PathBuf>,
    },
    Placeholder {
        kind: PosterKind,
    },
}

impl Poster {
    pub(crate) fn reusable_stamp(&self, cache_root: &Path) -> Option<PosterStamp> {
        let (stamp, cache_path) = match &self.recovery {
            PosterRecovery::Image {
                fingerprint,
                cache_path,
            } => (PosterStamp::Still(fingerprint.clone()), cache_path),
            PosterRecovery::Video { stamp, cache_path } if stamp.cacheable() => {
                (PosterStamp::Video(*stamp), cache_path)
            }
            _ => return None,
        };
        cache_path
            .as_ref()
            .is_none_or(|path| path.parent() == Some(cache_root))
            .then_some(stamp)
    }

    pub(crate) fn same_resource(&self, other: &Self) -> bool {
        // A canonical cache hit creates a fresh Poster value while retaining
        // the same stamped recovery identity. Treat that as the same GPU
        // resource so an existing Far/Near atlas slot can survive a demand
        // refresh without forcing a redundant upload.
        let same_recovery = match (&self.recovery, &other.recovery) {
            // Synthetic placeholders have no source stamp, so pointer identity
            // remains the only exact identity available for them.
            (PosterRecovery::Placeholder { .. }, PosterRecovery::Placeholder { .. }) => {
                Arc::ptr_eq(&self.pixels, &other.pixels)
            }
            _ => self.recovery == other.recovery,
        };
        self.layer == other.layer && self.source == other.source && same_recovery
    }

    pub(crate) fn matches_source(&self, kind: PosterKind, source: Option<PosterStamp>) -> bool {
        match &self.recovery {
            PosterRecovery::Image { fingerprint, .. } => {
                kind == PosterKind::Still
                    && matches!(source, Some(PosterStamp::Still(stamp)) if stamp == *fingerprint)
            }
            PosterRecovery::Video { stamp, .. } => {
                kind == PosterKind::Video
                    && stamp.cacheable()
                    && source == Some(PosterStamp::Video(*stamp))
            }
            PosterRecovery::Placeholder { kind: captured } => {
                kind == PosterKind::Placeholder && kind == *captured
            }
        }
    }

    fn current_cached_thumbnail(&self) -> Option<(Vec<u8>, f32)> {
        let PosterRecovery::Image {
            fingerprint,
            cache_path: Some(cache_path),
        } = &self.recovery
        else {
            return None;
        };
        if !fingerprint.matches(&self.source) {
            return None;
        }
        let pixels = match &*self.pixels.lock().ok()? {
            PosterPixels::Ready(PosterEncoding::NearRgba, pixels) => Some(pixels.to_vec()),
            _ => None,
        };
        let pixels = pixels.or_else(|| cache::read_poster(cache_path))?;
        Some((pixels, self.aspect_ratio))
    }

    fn pixels_for(&self, tier: PosterTier) -> &Mutex<PosterPixels> {
        match tier {
            PosterTier::Near => &self.pixels,
            PosterTier::Far => &self.far_pixels,
        }
    }

    #[cfg(test)]
    pub fn has_pixels(&self) -> bool {
        [PosterTier::Near, PosterTier::Far].into_iter().any(|tier| {
            self.pixels_for(tier)
                .lock()
                .is_ok_and(|state| matches!(*state, PosterPixels::Ready(..)))
        })
    }

    fn has_encoded_pixels(&self, encoding: PosterEncoding) -> bool {
        self.pixels_for(encoding.tier()).lock().is_ok_and(
            |state| matches!(*state, PosterPixels::Ready(current, _) if encoding == current),
        )
    }

    fn upload_unavailable(&self, tier: PosterTier) -> bool {
        self.pixels_for(tier)
            .lock()
            .is_ok_and(|state| matches!(*state, PosterPixels::Unavailable))
    }

    fn take_pixels(&self, encoding: PosterEncoding) -> Option<Arc<PosterBytes>> {
        let mut state = self.pixels_for(encoding.tier()).lock().ok()?;
        if !matches!(*state, PosterPixels::Ready(current, _) if encoding == current) {
            return None;
        }
        let PosterPixels::Ready(_, bytes) = std::mem::replace(&mut *state, PosterPixels::Consumed)
        else {
            unreachable!()
        };
        Some(bytes)
    }

    fn discard_near_pixels(&self) {
        if let Ok(mut state) = self.pixels.lock() {
            if matches!(*state, PosterPixels::Ready(..)) {
                *state = PosterPixels::Consumed;
            }
        }
    }

    fn needs_restore(&self, encoding: PosterEncoding) -> bool {
        self.pixels_for(encoding.tier()).lock().is_ok_and(|state| {
            matches!(*state, PosterPixels::Consumed)
                || matches!(*state, PosterPixels::Ready(current, _) if current != encoding)
        })
    }

    fn near_restore_pending(&self) -> bool {
        self.pixels
            .lock()
            .is_ok_and(|state| matches!(*state, PosterPixels::Restoring))
    }

    pub(crate) fn request_restore(&self, encoding: PosterEncoding) -> Option<PosterRestore> {
        let mut state = self.pixels_for(encoding.tier()).lock().ok()?;
        match *state {
            PosterPixels::Consumed => {}
            PosterPixels::Ready(current, _) if current != encoding => {}
            _ => return None,
        }
        let previous = std::mem::replace(&mut *state, PosterPixels::Restoring);
        Some(PosterRestore {
            poster: self.clone(),
            encoding,
            previous,
            published: false,
        })
    }
}

/// Holds the exact resource through the worker queue, even for a transition pin.
#[derive(Debug)]
pub(crate) struct PosterRestore {
    poster: Poster,
    encoding: PosterEncoding,
    previous: PosterPixels,
    published: bool,
}

impl PosterRestore {
    pub(crate) fn shares_source_with(&self, other: &Self) -> bool {
        self.poster.source == other.poster.source && self.poster.recovery == other.poster.recovery
    }

    /// The captured tile already has its final matte, crop and dimensions.
    /// Other states still need the source-grouped recovery below.
    fn publish_staged_far(&mut self, current_epoch: &impl Fn() -> bool) -> bool {
        let PosterPixels::Ready(PosterEncoding::FarRgba, tile) = &self.previous else {
            return false;
        };
        if self.encoding != PosterEncoding::FarBc1
            || tile.len() != (FAR_WIDTH * FAR_HEIGHT * 4) as usize
            || !current_epoch()
        {
            return false;
        }
        let _timing = apply_trace::cpu_span("depth-staged-far-pack", || {
            format!(
                "source={:?} layer={}",
                self.poster.source, self.poster.layer
            )
        });
        let packed = pack_far_bc1(tile);
        if let Ok(mut slot) = self.poster.far_pixels.lock() {
            if current_epoch() && matches!(*slot, PosterPixels::Restoring) {
                *slot = PosterPixels::Ready(self.encoding, Arc::new(packed.into()));
                self.published = true;
                return true;
            }
        }
        false
    }

    pub(crate) fn poster(&self) -> &Poster {
        &self.poster
    }

    /// Called only by the poster worker. A failure is cached for this resource;
    /// a new source fingerprint or a cache reset creates a fresh resource.
    #[cfg(test)]
    fn restore(self, current_epoch: impl Fn() -> bool) {
        self.restore_with(None, &mut ReadBatch::default(), &mut None, current_epoch);
    }

    fn restore_with(
        mut self,
        decoder: Option<&PosterDecoder>,
        cache: &mut ReadBatch,
        canonical: &mut Option<Option<Arc<PosterBytes>>>,
        current_epoch: impl Fn() -> bool,
    ) {
        if !current_epoch() {
            return;
        }
        if canonical.is_none()
            && !matches!(
                &self.previous,
                PosterPixels::Ready(PosterEncoding::NearRgba, _)
            )
            && decoder.is_some_and(|decoder| !decoder.make_staging_room())
        {
            // Dropping the request restores its previous state for the next draw.
            return;
        }
        let poster = &self.poster;
        let _timing = apply_trace::cpu_span("poster-restore-tier", || {
            format!(
                "source={:?} layer={} encoding={:?}",
                poster.source, poster.layer, self.encoding
            )
        });
        let rgba = canonical.get_or_insert_with(|| {
            if let PosterPixels::Ready(PosterEncoding::NearRgba, bytes) = &self.previous {
                return Some(Arc::clone(bytes));
            }
            let mut pixels = poster_buffer(decoder);
            match &poster.recovery {
                PosterRecovery::Image {
                    fingerprint,
                    cache_path,
                } => {
                    let cached = cache_path
                        .as_deref()
                        .and_then(|path| cache.read_into(path, &mut pixels));
                    cached
                        .or_else(|| {
                            if !fingerprint.matches(&poster.source) {
                                return None;
                            }
                            decode_poster_into(&poster.source, &mut pixels, &current_epoch).ok()?;
                            fingerprint.matches(&poster.source).then_some(())
                        })
                        .map(|()| Arc::new(pixels))
                }
                PosterRecovery::Video { stamp, cache_path } => {
                    let cached = cache_path
                        .as_deref()
                        .and_then(|path| cache.read_video_into(path, &mut pixels));
                    cached
                        .filter(|aspect| *aspect == poster.aspect_ratio)
                        .or_else(|| {
                            if !stamp.cacheable() || !stamp.matches(&poster.source) {
                                return None;
                            }
                            let aspect =
                                decode_video_into(&poster.source, &mut pixels, &current_epoch)?;
                            stamp
                                .matches(&poster.source)
                                .then_some(aspect)
                                .filter(|aspect| *aspect == poster.aspect_ratio)
                        })
                        .map(|_| Arc::new(pixels))
                }
                PosterRecovery::Placeholder { .. } => {
                    fill_video_placeholder(&poster.source, &mut pixels);
                    Some(Arc::new(pixels))
                }
            }
        });
        if self.encoding == PosterEncoding::NearRgba
            && rgba.is_some()
            && decoder.is_some_and(|decoder| !decoder.pool.can_keep_scratch(decoder.producers))
        {
            return;
        }
        if self.encoding.tier() == PosterTier::Near && self.encoding != PosterEncoding::NearRgba {
            if let Some(decoder) = decoder {
                if !decoder.make_upload_staging_room(near_bytes(self.encoding.texture_format())) {
                    return;
                }
            }
        }
        let pixels = rgba
            .as_ref()
            .and_then(|bytes| self.encoding.encode(Arc::clone(bytes), poster.aspect_ratio));
        if self.encoding.is_near() && pixels.is_some() {
            apply_trace::event_lazy("poster-near-encoded", || {
                format!(
                    "source={:?} layer={} encoding={:?}",
                    poster.source, poster.layer, self.encoding
                )
            });
        }
        let mut staged = false;
        if let Ok(mut state) = poster.pixels_for(self.encoding.tier()).lock() {
            if current_epoch() {
                staged = pixels.is_some() && self.encoding.tier() == PosterTier::Near;
                *state = pixels.map_or(PosterPixels::Unavailable, |bytes| {
                    PosterPixels::Ready(self.encoding, bytes)
                });
                self.published = true;
            }
        }
        if let Some(decoder) = decoder.filter(|_| staged) {
            decoder.track_staging(&poster.pixels);
        }
    }
}

impl Drop for PosterRestore {
    fn drop(&mut self) {
        if self.published {
            return;
        }
        if let Ok(mut pixels) = self.poster.pixels_for(self.encoding.tier()).lock() {
            if matches!(*pixels, PosterPixels::Restoring) {
                *pixels = std::mem::replace(&mut self.previous, PosterPixels::Consumed);
            }
        }
    }
}

/// The worker owns the staging order, while posters own their leases until
/// upload, discard or cancellation. Weak references never keep posters alive.
pub(crate) struct PosterDecoder {
    pool: Arc<BufferPool>,
    producers: usize,
    upload_staging_bytes: usize,
    staging: Mutex<PosterStaging>,
    video_failures: Mutex<HashMap<PathBuf, VideoFailure>>,
}

struct VideoFailure {
    stamp: VideoStamp,
    attempts: u8,
    last: std::time::Instant,
}

#[derive(Default)]
struct PosterStaging {
    staged: VecDeque<Weak<Mutex<PosterPixels>>>,
    prior_staged: usize,
}

impl PosterDecoder {
    #[cfg(test)]
    pub(crate) fn weak_pool(&self) -> Weak<BufferPool> {
        Arc::downgrade(&self.pool)
    }

    #[cfg(test)]
    pub(crate) fn new(capacity: usize) -> Self {
        Self::with_producers(capacity, 1)
    }

    pub(crate) fn with_producers(capacity: usize, producers: usize) -> Self {
        assert!(producers < capacity);
        // Reserve part of the existing RAM budget for packed upload mips.
        // Tiny pool-only fixtures keep their original two-canvas minimum.
        let reserved = if capacity >= 4 {
            (capacity / 4)
                .max(2)
                .min(capacity.saturating_sub(producers + 1))
        } else {
            0
        };
        Self {
            pool: BufferPool::new(capacity - reserved),
            producers,
            upload_staging_bytes: (reserved * POSTER_BYTES)
                .max(near_bytes(wgpu::TextureFormat::Rgba8UnormSrgb)),
            staging: Mutex::default(),
            video_failures: Mutex::default(),
        }
    }

    pub(crate) fn begin_work(&self) {
        let mut staging = self.staging.lock().unwrap();
        staging.staged.retain(|pixels| {
            pixels.upgrade().is_some_and(|pixels| {
                pixels.lock().is_ok_and(|state| {
                    matches!(&*state, PosterPixels::Ready(encoding, _) if encoding.tier() == PosterTier::Near)
                        || matches!(*state, PosterPixels::Restoring)
                })
            })
        });
        staging.prior_staged = staging.staged.len();
    }

    pub(crate) fn trace(&self) {
        self.pool.trace();
    }

    pub(crate) fn clear_video_failures(&self) {
        self.video_failures.lock().unwrap().clear();
    }

    fn can_retry_video(&self, path: &Path, stamp: VideoStamp) -> bool {
        self.video_failures
            .lock()
            .unwrap()
            .get(path)
            .is_none_or(|prior| {
                prior.stamp != stamp
                    || (prior.attempts < 2
                        && prior.last.elapsed() >= std::time::Duration::from_secs(2))
            })
    }

    fn video_outcome(&self, path: &Path, stamp: VideoStamp, usable: bool) {
        let mut failures = self.video_failures.lock().unwrap();
        if usable {
            failures.remove(path);
            return;
        }
        if failures.len() == MAX_ORDINARY_POSTERS && !failures.contains_key(path) {
            if let Some(oldest) = failures
                .iter()
                .min_by_key(|(_, failure)| failure.last)
                .map(|(path, _)| path.clone())
            {
                failures.remove(&oldest);
            }
        }
        let attempts = failures
            .get(path)
            .filter(|prior| prior.stamp == stamp)
            .map_or(1, |prior| prior.attempts.saturating_add(1));
        failures.insert(
            path.to_owned(),
            VideoFailure {
                stamp,
                attempts,
                last: std::time::Instant::now(),
            },
        );
    }

    fn make_staging_room(&self) -> bool {
        // Old staging can still be pinned by a snapshot that will never draw
        // again. Reclaim it under pressure and leave new results for the next
        // redraw to consume. Restore remains available for the reclaimed slot.
        let mut staging = self.staging.lock().unwrap();
        while !self.pool.has_staging_room(self.producers) && staging.prior_staged != 0 {
            staging.prior_staged -= 1;
            let Some(weak) = staging.staged.pop_front() else {
                break;
            };
            if let Some(pixels) = weak.upgrade() {
                if let Ok(mut state) = pixels.lock() {
                    if matches!(&*state, PosterPixels::Ready(encoding, _) if encoding.tier() == PosterTier::Near)
                    {
                        *state = PosterPixels::Consumed;
                    } else if matches!(*state, PosterPixels::Restoring) {
                        staging.staged.push_back(weak);
                    }
                }
            }
        }
        self.pool.has_staging_room(self.producers)
    }

    // Restore batches encode sequentially on the existing poster worker.
    // Retained snapshots/pins may outlive a draw; reclaim their packed payload
    // under the same staging owner, leaving resource identity/restoration intact.
    fn make_upload_staging_room(&self, wanted: usize) -> bool {
        let mut staging = self.staging.lock().unwrap();
        let packed_bytes = |staging: &PosterStaging| {
            staging
                .staged
                .iter()
                .filter_map(Weak::upgrade)
                .map(|pixels| {
                    pixels.lock().map_or(0, |state| match &*state {
                        PosterPixels::Ready(encoding, bytes)
                            if encoding.tier() == PosterTier::Near
                                && *encoding != PosterEncoding::NearRgba =>
                        {
                            bytes.len()
                        }
                        _ => 0,
                    })
                })
                .sum::<usize>()
        };
        while packed_bytes(&staging) + wanted > self.upload_staging_bytes
            && staging.prior_staged != 0
        {
            staging.prior_staged -= 1;
            let Some(weak) = staging.staged.pop_front() else {
                break;
            };
            if let Some(pixels) = weak.upgrade() {
                if let Ok(mut state) = pixels.lock() {
                    if matches!(&*state, PosterPixels::Ready(encoding, _) if encoding.tier() == PosterTier::Near && *encoding != PosterEncoding::NearRgba)
                    {
                        *state = PosterPixels::Consumed;
                    } else {
                        staging.staged.push_back(weak);
                    }
                }
            }
        }
        packed_bytes(&staging) + wanted <= self.upload_staging_bytes
    }

    #[cfg(test)]
    fn restore(
        &self,
        request: PosterRestore,
        cache: &mut ReadBatch,
        current_epoch: impl Fn() -> bool,
    ) {
        request.restore_with(Some(self), cache, &mut None, current_epoch);
    }

    pub(crate) fn restore_batch(
        &self,
        requests: Vec<PosterRestore>,
        cache: &mut ReadBatch,
        current_epoch: impl Fn() -> bool,
        depth_redraw: Option<&dyn Fn()>,
    ) -> usize {
        let mut by_source = HashMap::new();
        let mut groups: Vec<Vec<PosterRestore>> = Vec::new();
        for request in requests {
            let poster = &request.poster;
            // An outgoing pin can read its captured cache key even after the
            // source changes. Only identical recovery paths and stamps share.
            let group = *by_source
                .entry((poster.source.clone(), poster.recovery.clone()))
                .or_insert_with(|| {
                    groups.push(Vec::new());
                    groups.len() - 1
                });
            groups[group].push(request);
        }
        let count = groups.len();
        if let Some(redraw) = depth_redraw {
            let mut published = 0;
            for group in &mut groups {
                group.retain_mut(|request| {
                    let ready = request.publish_staged_far(&current_epoch);
                    published += usize::from(ready);
                    !ready
                });
            }
            if published != 0 && groups.iter().any(|group| !group.is_empty()) && current_epoch() {
                redraw();
            }
        }
        for group in groups {
            let mut canonical = group.iter().find_map(|request| match &request.previous {
                PosterPixels::Ready(PosterEncoding::NearRgba, bytes) => {
                    Some(Some(Arc::clone(bytes)))
                }
                _ => None,
            });
            for request in group {
                request.restore_with(Some(self), cache, &mut canonical, &current_epoch);
            }
        }
        count
    }

    pub(crate) fn fork_slot(&self, poster: &Poster, layer: u32) -> Poster {
        fn fork(pixels: &Mutex<PosterPixels>) -> Arc<Mutex<PosterPixels>> {
            let state = match &*pixels.lock().unwrap() {
                PosterPixels::Ready(encoding, bytes) => {
                    PosterPixels::Ready(*encoding, Arc::clone(bytes))
                }
                PosterPixels::Unavailable => PosterPixels::Unavailable,
                PosterPixels::Consumed | PosterPixels::Restoring => PosterPixels::Consumed,
            };
            Arc::new(Mutex::new(state))
        }
        let pixels = fork(&poster.pixels);
        if pixels.lock().is_ok_and(|state| {
            matches!(&*state, PosterPixels::Ready(encoding, _) if encoding.tier() == PosterTier::Near)
        }) {
            self.track_staging(&pixels);
        }
        Poster {
            layer,
            pixels,
            far_pixels: fork(&poster.far_pixels),
            ..poster.clone()
        }
    }

    fn track_staging(&self, pixels: &Arc<Mutex<PosterPixels>>) {
        let mut staging = self.staging.lock().unwrap();
        let weak = Arc::downgrade(pixels);
        if let Some(index) = staging.staged.iter().position(|other| other.ptr_eq(&weak)) {
            staging.staged.remove(index);
            staging.prior_staged -= usize::from(index < staging.prior_staged);
        }
        staging.staged.push_back(weak);
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SliceParams {
    pub offset_x: f32,
    pub offset_y: f32,
    pub width: f32,
    pub expanded_width: f32,
    pub height: f32,
    pub spacing: f32,
    /// Horizontal distance between the top and bottom edges, in logical pixels.
    pub skew_distance: f32,
    /// Vertical distance between the left and right edges, in logical pixels.
    pub edge_tilt_distance: f32,
    pub visible_count: usize,
    pub corner_radius: f32,
    pub wobble: bool,
    pub wobble_strength: f32,
    /// Transient signed navigation displacement. Zero means fully settled.
    pub motion: f32,
}

impl Default for SliceParams {
    fn default() -> Self {
        Self {
            offset_x: 0.0,
            offset_y: 0.0,
            width: 250.0,
            expanded_width: 420.0,
            height: 420.0,
            spacing: -72.0,
            skew_distance: 48.0,
            edge_tilt_distance: 0.0,
            visible_count: 11,
            corner_radius: 12.0,
            wobble: false,
            wobble_strength: 1.0,
            motion: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GeometricShape {
    #[default]
    Hexagon,
    Circle,
    Triangle,
    Diamond,
    Rhombus,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GeometricCurve {
    Flat,
    #[default]
    Arc,
    Wave,
    S,
    Cylinder,
}

#[derive(Debug, Clone, Copy)]
pub struct GeometricParams {
    pub radius: f32,
    pub rows: usize,
    pub columns: usize,
    pub shape: GeometricShape,
    pub curve: GeometricCurve,
    pub curve_strength: f32,
    pub curve_frequency: f32,
    pub gap_x: f32,
    pub gap_y: f32,
    pub aspect: f32,
    pub stagger: f32,
    pub lens: f32,
    pub orbit: f32,
    /// A non-positive value keeps the viewport-derived cylinder radius.
    pub orbit_radius: f32,
    pub twist: f32,
    pub scatter: f32,
    /// Transient signed navigation displacement. Zero means fully settled.
    pub motion: f32,
    /// Donor Stage state applied after geometric deformation.
    pub stage: StageParams,
}

impl Default for GeometricParams {
    fn default() -> Self {
        Self {
            radius: 140.0,
            rows: 3,
            columns: 7,
            shape: GeometricShape::Hexagon,
            curve: GeometricCurve::Arc,
            curve_strength: 1.2,
            curve_frequency: 1.0,
            gap_x: 6.0,
            gap_y: 6.0,
            aspect: 1.0,
            stagger: 0.5,
            lens: 0.0,
            orbit: 0.0,
            orbit_radius: 0.0,
            twist: 0.0,
            scatter: 0.0,
            motion: 0.0,
            stage: StageParams::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum WallLayout {
    Uniform,
    #[default]
    Brick,
    Masonry,
    Justified,
    Editorial,
    Cylinder,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WallParams {
    pub columns: usize,
    pub rows: usize,
    pub card_width: f32,
    pub card_height: f32,
    pub gap_x: f32,
    pub gap_y: f32,
    pub corner_radius: f32,
    pub layout: WallLayout,
    pub stagger: f32,
    pub selected_scale: f32,
    pub flow_wave: f32,
    pub flow_frequency: f32,
    pub scatter: f32,
    pub scale_variance: f32,
    pub cylinder_bend: f32,
    pub cylinder_radius: f32,
    /// Persistent absolute Wall camera position in content coordinates.
    pub scroll_offset: f32,
    /// Transient signed navigation displacement. Zero means fully settled.
    pub motion: f32,
    /// Donor Stage state applied after Grid placement and cylinder projection.
    pub stage: StageParams,
    /// Actual horizontal rail footprint; None means no rail in the composition.
    pub rail_height: Option<f32>,
    pub rail_width: Option<f32>,
    pub rail_rtl: bool,
    pub rail_gap: f32,
    pub border_width: f32,
    pub user_engaged: bool,
}

impl Default for WallParams {
    fn default() -> Self {
        Self {
            columns: 6,
            rows: 3,
            card_width: 300.0,
            card_height: 169.0,
            gap_x: 8.0,
            gap_y: 8.0,
            corner_radius: 12.0,
            layout: WallLayout::Uniform,
            stagger: 0.5,
            selected_scale: 1.0,
            flow_wave: 0.0,
            flow_frequency: 1.0,
            scatter: 0.0,
            scale_variance: 0.0,
            cylinder_bend: 0.65,
            cylinder_radius: 720.0,
            scroll_offset: 0.0,
            motion: 0.0,
            stage: StageParams::default(),
            rail_height: Some(24.0),
            rail_width: None,
            rail_rtl: false,
            rail_gap: 8.0,
            border_width: 2.0,
            user_engaged: false,
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GalleryLayoutParams {
    pub slices: SliceParams,
    pub geometric: GeometricParams,
    pub wall: WallParams,
    pub depth: donor_layout::DepthParams,
    pub depth_camera: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SliceCard {
    pub index: usize,
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub layer: Option<u32>,
    pub selected: bool,
    pub opacity: f32,
    pub skew_distance: f32,
    pub edge_tilt_distance: f32,
    pub rotation: f32,
    /// Triangle mask direction, independent of poster/world rotation (0 up, 1 down).
    pub triangle_direction: u8,
    pub corner_radius: f32,
    /// 0 = sheared card, 1 = circle, 2 = hexagon, 3 = diamond, 4 = triangle.
    pub shape: u32,
    /// Width / height of the original poster before cover-cropping to this card.
    pub poster_aspect_ratio: f32,
}

#[derive(Debug, Clone)]
pub struct GallerySnapshot {
    pub ordinary: Option<Vec<crate::donor_scene::CardPresentation>>,
    pub(crate) defer_near_restore: bool,
    pub cards: Vec<SliceCard>,
    pub posters: Arc<Vec<Poster>>,
    pub colors: GalleryColors,
    /// Donor scene concepts retained at the  adapter boundary. The
    /// renderer consumes the projected cards, while this state keeps Stage,
    /// camera and detail ownership explicit for later controls/motion work.
    pub scene: DonorSceneState,
    pub depth: Option<DepthSnapshot>,
    /// Logical Wall frame shared by shader clipping and input hit gating.
    pub wall_clip: Option<[f32; 4]>,
}

impl GallerySnapshot {
    fn hit(&self, x: f32, y: f32) -> Option<usize> {
        if self.wall_clip.is_some_and(|clip| !inside_clip(clip, x, y)) {
            return None;
        }
        if let Some(entries) = &self.ordinary {
            if entries.iter().any(|entry| entry.source_style) {
                return entries
                    .iter()
                    .rev()
                    .find(|entry| {
                        crate::donor_scene::presented_slice_contains(
                            entry,
                            &self.colors,
                            self.scene.viewport,
                            [x, y],
                        )
                    })
                    .map(|entry| entry.card.index);
            }
        }
        hit_test(&self.cards, x, y)
    }

    fn physical_clip(&self, scale: f32) -> [f32; 4] {
        self.wall_clip
            .map_or([-1.0e9, -1.0e9, 1.0e9, 1.0e9], |clip| {
                clip.map(|v| v * scale)
            })
    }

    /// Every ordinary consumer, including an off-frame external detail, uses
    /// the exact uploaded poster resource rather than a stale atlas layer.
    #[cfg(test)]
    pub(crate) fn ordinary_resident_instances(
        &self,
        resident: &[Poster],
        scale: f32,
    ) -> Vec<InstanceRaw> {
        self.ordinary_instances_with_residency(
            |p| resident.iter().any(|r| r.same_resource(p)),
            scale,
        )
    }

    fn ordinary_instances_with_residency(
        &self,
        is_resident: impl Fn(&Poster) -> bool,
        scale: f32,
    ) -> Vec<InstanceRaw> {
        let mut matching_layers = [false; MAX_ORDINARY_POSTERS];
        for poster in self.posters.iter().filter(|poster| is_resident(poster)) {
            if let Some(matching) = matching_layers.get_mut(poster.layer as usize) {
                *matching = true;
            }
        }
        let layer_resident = |layer: u32| {
            matching_layers
                .get(layer as usize)
                .copied()
                .unwrap_or(false)
        };
        let mut snapshot = self.clone();
        if let Some(entries) = &mut snapshot.ordinary {
            entries.retain(|e| e.retained.as_ref().is_none_or(&is_resident));
            for entry in entries {
                if entry.card.layer.is_some_and(|layer| !layer_resident(layer)) {
                    entry.card.layer = None;
                    entry.readiness = crate::donor_scene::CardReadiness::Unavailable;
                }
            }
        } else {
            for card in &mut snapshot.cards {
                if card.layer.is_some_and(|layer| !layer_resident(layer)) {
                    card.layer = None;
                }
            }
        }
        if let Some(card) = &mut snapshot.scene.detail_card {
            if card.layer.is_some_and(|layer| !layer_resident(layer)) {
                card.layer = None;
            }
        }
        snapshot.ordinary_instances(scale)
    }

    fn ordinary_instances(&self, scale: f32) -> Vec<InstanceRaw> {
        if let Some(entries) = &self.ordinary {
            let raw =
                crate::donor_scene::presented_instances(entries, &self.colors, scale, &self.scene);
            return raw;
        }
        let mut raw = scene_instances(&self.cards, &self.colors, scale, &self.scene);
        if self.wall_clip.is_some() {
            // scene_instances emits gallery bodies first, then separate detail
            // scrim/shadow/body. Only gallery bodies use the Wall frame clip.
            for body in raw.iter_mut().take(self.cards.len()) {
                body.misc[2] = 1;
            }
        }
        raw
    }
}

fn inside_clip(clip: [f32; 4], x: f32, y: f32) -> bool {
    x >= clip[0] && y >= clip[1] && x <= clip[2] && y <= clip[3]
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct WallFrame {
    pub center: [f32; 2],
    pub top: f32,
    pub left: f32,
    pub clip: [f32; 4],
}

/// beta.24 SceneCore::wall_composition_center, with actual rail orientation.
/// Geometry keeps the full configured frame; clipping intersects the viewport.
pub(crate) fn wall_frame(viewport: [f32; 2], p: WallParams) -> WallFrame {
    let width = (p.card_width.max(1.0) * p.columns.max(1) as f32
        + p.gap_x * p.columns.max(1).saturating_sub(1) as f32)
        .max(1.0);
    let height = (p.card_height.max(1.0) * p.rows.max(1) as f32
        + p.gap_y * p.rows.max(1).saturating_sub(1) as f32)
        .max(1.0);
    let shift_y = p
        .rail_height
        .map_or(0.0, |height| (height + p.rail_gap) * 0.5);
    let shift_x = p.rail_width.map_or(0.0, |width| (width + p.rail_gap) * 0.5)
        * if p.rail_rtl { -1.0 } else { 1.0 };
    let center = [viewport[0] * 0.5 + shift_x, viewport[1] * 0.5 + shift_y];
    let left = center[0] - width * 0.5;
    let top = center[1] - height * 0.5;
    WallFrame {
        center,
        left,
        top,
        clip: [
            left.max(0.0),
            top.max(0.0),
            (left + width).min(viewport[0]),
            (top + height).min(viewport[1]),
        ],
    }
}

#[derive(Debug, Clone, Copy)]
pub struct GalleryColors {
    pub surface: [f32; 3],
    pub accent: [f32; 3],
    pub unselected_brightness: f32,
    pub selection_border_width: f32,
    pub show_selection_border: bool,
    pub border_glow: bool,
    pub border_glow_radius: f32,
}

impl Default for GalleryColors {
    fn default() -> Self {
        Self {
            surface: [0.16, 0.15, 0.22],
            accent: [0.80, 0.65, 0.95],
            unselected_brightness: 0.68,
            selection_border_width: 2.0,
            show_selection_border: true,
            border_glow: false,
            border_glow_radius: 0.0,
        }
    }
}

pub fn parse_hex_rgb(value: &str, fallback: [f32; 3]) -> [f32; 3] {
    let Some([red, green, blue]) = crate::rgb_color::parse_hex_rgb_bytes(value) else {
        return fallback;
    };
    [
        red as f32 / 255.0,
        green as f32 / 255.0,
        blue as f32 / 255.0,
    ]
}

#[derive(Debug, Clone)]
pub enum GalleryMessage {
    Pointer {
        index: Option<usize>,
        id: Option<String>,
        source: Option<PathBuf>,
        revision: Option<u64>,
        presented_catalog_generation: Option<u64>,
        button: crate::candidate_state::binding_codec::MouseButton,
        modifiers: crate::candidate_state::binding_codec::Mods,
        sequence: Option<u64>,
    },
    Hovered(Option<usize>),
    #[cfg(test)]
    PresentedClick {
        index: usize,
        id: String,
        source: PathBuf,
        catalog_generation: u64,
        sequence: Option<u64>,
    },
    #[cfg(test)]
    Clicked(usize, Option<u64>),
    DepthPointer {
        revision: u64,
        item_id: Option<String>,
        right: bool,
        hover: bool,
    },
    BackdropClicked(Option<u64>),
    Wheel(f32, Option<u64>),
    BoundWheel {
        amount: f32,
        vertical: f32,
        modifiers: crate::candidate_state::binding_codec::Mods,
        sequence: Option<u64>,
    },
}

#[derive(Debug, Clone)]
pub(crate) enum PosterFeedback {
    Restore {
        posters: Vec<(Poster, PosterEncoding)>,
        near_order: Vec<Poster>,
        depth: bool,
    },
    Redraw,
    MaintenanceAfter(std::time::Duration),
    Prewarmed {
        key: AtlasPrewarmKey,
        posters: Arc<Vec<Poster>>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AtlasPrewarmKey {
    pub generation: u64,
    pub motion_generation: u64,
    pub selected: Option<usize>,
    pub viewport: [u32; 2],
}

pub struct GalleryProgram {
    pub(crate) prewarm: Option<(AtlasPrewarmKey, bool)>,
    pub snapshot: Arc<GallerySnapshot>,
    pub(crate) poster_feedback: Option<fn(PosterFeedback)>,
}

impl shader::Program<GalleryMessage> for GalleryProgram {
    type State = iced::keyboard::Modifiers;
    type Primitive = GalleryPrimitive;

    fn update(
        &self,
        state: &mut Self::State,
        event: &Event,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Option<Action<GalleryMessage>> {
        if let Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) = event {
            *state = *modifiers;
            return None;
        }
        // Mode entry is hidden. A held Colour roll still displays outgoing
        // art, so let the wheel interrupt it without treating a click as a
        // backdrop hit or restarting its motion from pointer hover.
        if self.prewarm.is_some()
            && (self.snapshot.scene.visibility == 0.0
                || !matches!(event, Event::Mouse(mouse::Event::WheelScrolled { .. })))
        {
            return None;
        }
        if let Event::Mouse(mouse::Event::ButtonPressed(button)) = event {
            use crate::candidate_state::binding_codec::{Mods, MouseButton as B};
            let button = match button {
                mouse::Button::Left => B::Left,
                mouse::Button::Right => B::Right,
                mouse::Button::Middle => B::Middle,
                _ => return None,
            };
            let point = cursor.position_in(bounds)?;
            let (index, id, source, revision, presented_catalog_generation) =
                if let Some(frame) = &self.snapshot.depth {
                    let hit = frame.hit(point.x, point.y);
                    (
                        hit.map(|c| c.card.index),
                        hit.map(|c| c.id.clone()),
                        None,
                        Some(frame.revision),
                        None,
                    )
                } else {
                    let index = self.snapshot.hit(point.x, point.y);
                    let entry = index.and_then(|i| {
                        self.snapshot
                            .ordinary
                            .as_ref()?
                            .iter()
                            .find(|e| e.card.index == i)
                    });
                    (
                        index,
                        entry.map(|e| e.id.clone()),
                        entry.map(|e| e.source.clone()),
                        None,
                        entry.map(|e| e.catalog_generation),
                    )
                };
            let sequence = widget_mouse_sequence();
            let name = match button {
                B::Left => "left",
                B::Right => "right",
                B::Middle => "middle",
            };
            trace_mouse_receipt(
                sequence,
                "binding-pointer",
                name,
                "none",
                None,
                cursor,
                bounds,
            );
            let message = GalleryMessage::Pointer {
                index,
                id,
                source,
                revision,
                presented_catalog_generation,
                button,
                modifiers: Mods {
                    ctrl: state.control(),
                    alt: state.alt(),
                    shift: state.shift(),
                    logo: state.logo(),
                },
                sequence,
            };
            trace_mouse_publication(sequence, "binding-pointer", &message);
            return Some(Action::publish(message).and_capture());
        }
        if let Some(frame) = &self.snapshot.depth {
            if matches!(
                event,
                Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft)
            ) {
                let item_id = cursor
                    .position_in(bounds)
                    .and_then(|p| frame.hit(p.x, p.y))
                    .map(|card| card.id.clone());
                return Some(Action::publish(GalleryMessage::DepthPointer {
                    revision: frame.revision,
                    item_id,
                    right: false,
                    hover: true,
                }));
            }
        }
        match event {
            Event::Mouse(mouse::Event::CursorMoved { .. } | mouse::Event::CursorLeft) => {
                self.snapshot.ordinary.as_ref()?;
                Some(Action::publish(GalleryMessage::Hovered(
                    cursor
                        .position_in(bounds)
                        .and_then(|p| self.snapshot.hit(p.x, p.y)),
                )))
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
                let (amount, delta_kind, delta_x, delta_y) = match delta {
                    mouse::ScrollDelta::Lines { y, x } => {
                        let amount = if *y == 0.0 { *x } else { *y };
                        (amount, "lines", *x, *y)
                    }
                    mouse::ScrollDelta::Pixels { y, x } => {
                        let amount = if *y == 0.0 { *x / 60.0 } else { *y / 60.0 };
                        (amount, "pixels", *x, *y)
                    }
                };
                let sequence = widget_mouse_sequence();
                trace_mouse_receipt(
                    sequence,
                    "wheel",
                    "none",
                    delta_kind,
                    Some((delta_x, delta_y, amount)),
                    cursor,
                    bounds,
                );
                if amount == 0.0 {
                    trace_mouse_ignored(sequence, "normalized_wheel_delta_zero");
                    return None;
                }
                let message = GalleryMessage::BoundWheel {
                    amount,
                    vertical: if delta_kind == "lines" {
                        delta_y
                    } else {
                        delta_y / 60.0
                    },
                    modifiers: crate::candidate_state::binding_codec::Mods {
                        ctrl: state.control(),
                        alt: state.alt(),
                        shift: state.shift(),
                        logo: state.logo(),
                    },
                    sequence,
                };
                trace_mouse_publication(sequence, "wheel", &message);
                Some(Action::publish(message).and_capture())
            }
            _ => None,
        }
    }

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        GalleryPrimitive {
            frame_sequence: apply_trace::enabled().then(apply_trace::next_gallery_frame_sequence),
            snapshot: self.snapshot.clone(),
            poster_feedback: self.poster_feedback,
            prewarm: self.prewarm,
        }
    }
}

fn widget_mouse_sequence() -> Option<u64> {
    apply_trace::enabled().then(apply_trace::next_widget_mouse_sequence)
}

fn trace_mouse_receipt(
    sequence: Option<u64>,
    event_kind: &str,
    button: &str,
    delta_kind: &str,
    wheel: Option<(f32, f32, f32)>,
    cursor: mouse::Cursor,
    bounds: Rectangle,
) {
    let Some(sequence) = sequence else {
        return;
    };
    apply_trace::event_lazy("iced-widget-mouse-receipt", || {
        let cursor = cursor.position();
        let (wheel_x, wheel_y, normalized_amount) = wheel.unwrap_or((0.0, 0.0, 0.0));
        format!(
            "sequence={sequence} receipt=iced_widget event_kind={event_kind} button={button} delta_kind={delta_kind} wheel_x={wheel_x} wheel_y={wheel_y} normalized_amount={normalized_amount} cursor_available={} cursor_x={} cursor_y={} bounds_x={} bounds_y={} bounds_width={} bounds_height={}",
            cursor.is_some(),
            cursor.map_or(0.0, |point| point.x),
            cursor.map_or(0.0, |point| point.y),
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
        )
    });
}

fn trace_mouse_publication(sequence: Option<u64>, event_kind: &str, message: &GalleryMessage) {
    let Some(sequence) = sequence else {
        return;
    };
    let (message_kind, index, amount) = match message {
        GalleryMessage::Pointer { index, .. } => ("binding-pointer", *index, None),
        GalleryMessage::Hovered(index) => ("hovered", *index, None),
        #[cfg(test)]
        GalleryMessage::PresentedClick { index, .. } => ("presented-click", Some(*index), None),
        GalleryMessage::DepthPointer { .. } => ("depth-pointer", None, None),
        #[cfg(test)]
        GalleryMessage::Clicked(index, _) => ("clicked", Some(*index), None),
        GalleryMessage::BackdropClicked(_) => ("backdrop-clicked", None, None),
        GalleryMessage::Wheel(amount, _) | GalleryMessage::BoundWheel { amount, .. } => {
            ("wheel", None, Some(*amount))
        }
    };
    apply_trace::event_lazy("iced-widget-mouse-publication", || {
        format!(
            "sequence={sequence} receipt=iced_widget event_kind={event_kind} message_kind={message_kind} card_index={index:?} wheel_amount={amount:?} captured=true"
        )
    });
}

fn trace_mouse_ignored(sequence: Option<u64>, reason: &str) {
    let Some(sequence) = sequence else {
        return;
    };
    apply_trace::event_lazy("iced-widget-mouse-ignored", || {
        format!("sequence={sequence} receipt=iced_widget reason={reason}")
    });
}

pub fn layout_slices(
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: SliceParams,
) -> Vec<SliceCard> {
    if item_count == 0 {
        return Vec::new();
    }
    let Some(layout) = build_slice_layout(
        item_count,
        selected,
        viewport,
        params.offset_x,
        params.offset_y,
        params.width,
        params.expanded_width,
        params.spacing,
        params.skew_distance,
        params.visible_count,
        params.motion,
    ) else {
        return Vec::new();
    };
    let selected = selected.min(item_count - 1);
    let width_of = |index: usize| {
        if index == selected {
            params.expanded_width.max(1.0)
        } else {
            params.width.max(1.0)
        }
    };
    let mut cards: Vec<_> = (layout.start..layout.end)
        .map(|index| {
            let selected_card = index == selected;
            let fade = slice_opacity(
                layout.centers[index - layout.start],
                layout.reference_center_x,
                layout.half_view,
                layout.expanded_layout_width,
                layout.stride,
            );
            SliceCard {
                triangle_direction: 0,
                index,
                center: [layout.centers[index - layout.start], layout.center_y],
                size: [width_of(index), params.height.max(1.0)],
                layer: (index < MAX_POSTERS).then_some(index as u32),
                selected: selected_card,
                opacity: fade.clamp(0.0, 1.0),
                skew_distance: params.skew_distance,
                edge_tilt_distance: slice_edge_tilt(
                    params.edge_tilt_distance,
                    params.motion,
                    params.wobble,
                    params.wobble_strength,
                ),
                rotation: 0.0,
                corner_radius: params.corner_radius.max(0.0),
                shape: 0,
                poster_aspect_ratio: DEFAULT_POSTER_ASPECT,
            }
        })
        .collect();
    selected_last(&mut cards, selected);
    cards
}

pub fn layout_gallery(
    mode: GalleryMode,
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: GalleryLayoutParams,
) -> Vec<SliceCard> {
    layout_gallery_with_aspects(mode, item_count, selected, viewport, params, &[])
}

pub fn layout_gallery_with_aspects(
    mode: GalleryMode,
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: GalleryLayoutParams,
    poster_aspects: &[f32],
) -> Vec<SliceCard> {
    let mut cards = match mode {
        GalleryMode::Slices => layout_slices(item_count, selected, viewport, params.slices),
        GalleryMode::Geometric => {
            layout_geometric(item_count, selected, viewport, params.geometric)
        }
        GalleryMode::Depth => donor_layout::depth_layout(
            item_count,
            params.depth_camera,
            viewport,
            [viewport[0] * 0.5, viewport[0]],
            params.depth,
            1.0,
        )
        .cards
        .into_iter()
        .map(|p| SliceCard {
            triangle_direction: 0,
            index: p.filtered_index,
            center: p.center,
            size: p.size,
            layer: None,
            selected: p.filtered_index == selected,
            opacity: p.opacity,
            skew_distance: p.skew,
            edge_tilt_distance: 0.0,
            rotation: 0.0,
            corner_radius: p.radii[0],
            shape: 0,
            poster_aspect_ratio: valid_aspect(poster_aspects.get(p.filtered_index).copied()),
        })
        .collect(),
        GalleryMode::Wall => {
            layout_wall_with_aspects(item_count, selected, viewport, params.wall, poster_aspects)
        }
    };
    for card in &mut cards {
        card.poster_aspect_ratio = valid_aspect(poster_aspects.get(card.index).copied());
    }
    cards
}

fn layout_geometric(
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: GeometricParams,
) -> Vec<SliceCard> {
    if item_count == 0 {
        return Vec::new();
    }
    let selected = selected.min(item_count - 1);
    if params.shape == GeometricShape::Circle {
        return layout_geometric_circle(item_count, selected, viewport, params);
    }

    let shape = match params.shape {
        GeometricShape::Hexagon => DonorHexShape::Hexagon,
        GeometricShape::Triangle => DonorHexShape::Triangle,
        GeometricShape::Diamond => DonorHexShape::Diamond,
        GeometricShape::Rhombus => DonorHexShape::Rhombus,
        GeometricShape::Circle => unreachable!(),
    };
    let curve = match params.curve {
        GeometricCurve::Flat => DonorHexCurve::Flat,
        GeometricCurve::Arc => DonorHexCurve::Arc,
        GeometricCurve::Wave => DonorHexCurve::Wave,
        GeometricCurve::S => DonorHexCurve::S,
        GeometricCurve::Cylinder => DonorHexCurve::Cylinder,
    };
    let orbit_radius = if params.orbit_radius.is_finite() && params.orbit_radius > 0.0 {
        params.orbit_radius
    } else {
        viewport[0].min(viewport[1]).max(1.0) * 0.55
    };
    let donor = DonorHexParams {
        r: params.radius.max(donor_layout::MIN_HEX_R),
        rows: params.rows.max(1),
        cols: params.columns.max(1),
        scroll_step: 1,
        curve,
        shape,
        curve_strength: params.curve_strength,
        curve_frequency: params.curve_frequency,
        gap_x: params.gap_x,
        gap_y: params.gap_y,
        aspect: params.aspect.max(0.05),
        stagger: params.stagger,
        lens: params.lens,
        lens_radius: orbit_radius,
        orbit: params.orbit,
        orbit_radius,
        twist: params.twist,
        scatter: params.scatter,
        stage: params.stage,
    };
    let mut cards: Vec<_> =
        donor_layout::hex_placements(item_count, selected, viewport, &donor, params.motion)
            .into_iter()
            .map(|placement| SliceCard {
                triangle_direction: placement.triangle_direction,
                index: placement.index,
                center: [placement.x, placement.y],
                size: [placement.width, placement.height],
                layer: (placement.index < MAX_POSTERS).then_some(placement.index as u32),
                selected: placement.index == selected,
                opacity: placement.opacity,
                skew_distance: 0.0,
                edge_tilt_distance: 0.0,
                rotation: 0.0,
                corner_radius: 0.0,
                shape: match placement.shape {
                    DonorHexShape::Hexagon => 2,
                    DonorHexShape::Diamond | DonorHexShape::Rhombus => 3,
                    DonorHexShape::Triangle => 4,
                },
                poster_aspect_ratio: DEFAULT_POSTER_ASPECT,
            })
            .collect();
    selected_last(&mut cards, selected);
    cards
}

/// Circle is a  compatibility shape not present in the pinned donor Hex
/// family. Keep its bounded layout while applying the donor Stage transform
/// and the same camera displacement contract.
fn layout_geometric_circle(
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: GeometricParams,
) -> Vec<SliceCard> {
    let rows = params.rows.max(1);
    let total_columns = item_count.div_ceil(rows);
    let visible_columns = params.columns.max(1).min(total_columns);
    let selected_column = selected / rows;
    let start_column = selected_column
        .saturating_sub(visible_columns / 2)
        .min(total_columns.saturating_sub(visible_columns));
    let end_column = (start_column + visible_columns).min(total_columns);
    let radius = params.radius.max(1.0);
    let aspect = params.aspect.max(0.05);
    let step_x = radius * 2.0 * aspect + params.gap_x;
    let step_y = radius * 2.0 + params.gap_y;
    let origin = (
        viewport[0] * 0.5 + params.motion * step_x,
        viewport[1] * 0.5 + 24.0,
    );
    let stage_origin = (viewport[0] * 0.5, viewport[1] * 0.5 + 24.0);
    let mut cards = Vec::with_capacity(visible_columns * rows);
    for column in start_column..end_column {
        for row in 0..rows {
            let index = column * rows + row;
            if index >= item_count {
                break;
            }
            let x = origin.0 + (column as isize - selected_column as isize) as f32 * step_x;
            let y = origin.1 + (row as f32 - (rows.saturating_sub(1) as f32 * 0.5)) * step_y;
            let (x, y, stage_scale) =
                params
                    .stage
                    .transform(x, y, stage_origin, (viewport[0], viewport[1]));
            cards.push(SliceCard {
                triangle_direction: 0,
                index,
                center: [x, y],
                size: [
                    radius * 2.0 * aspect * stage_scale,
                    radius * 2.0 * stage_scale,
                ],
                layer: (index < MAX_POSTERS).then_some(index as u32),
                selected: index == selected,
                opacity: 1.0,
                skew_distance: 0.0,
                edge_tilt_distance: 0.0,
                rotation: 0.0,
                corner_radius: 0.0,
                shape: 1,
                poster_aspect_ratio: DEFAULT_POSTER_ASPECT,
            });
        }
    }
    selected_last(&mut cards, selected);
    cards
}
fn layout_wall(
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: WallParams,
) -> Vec<SliceCard> {
    layout_wall_with_aspects(item_count, selected, viewport, params, &[])
}

pub(crate) fn wall_placements(
    item_count: usize,
    params: WallParams,
    poster_aspects: &[f32],
) -> (DonorGridParams, Vec<donor_layout::GridPlacement>) {
    let layout = match params.layout {
        WallLayout::Uniform => DonorGridLayout::Uniform,
        WallLayout::Brick => DonorGridLayout::Brick,
        WallLayout::Masonry => DonorGridLayout::Masonry,
        WallLayout::Justified => DonorGridLayout::Justified,
        WallLayout::Editorial => DonorGridLayout::Editorial,
        WallLayout::Cylinder => DonorGridLayout::Cylinder,
    };
    let donor = DonorGridParams {
        cols: params.columns.max(1),
        rows: params.rows.max(1),
        thumb_w: params.card_width.max(1.0),
        thumb_h: params.card_height.max(1.0),
        gap_x: params.gap_x,
        gap_y: params.gap_y,
        corner_radius: params.corner_radius.max(0.0),
        border_width: params.border_width,
        layout,
        stagger: params.stagger,
        selected_scale: params.selected_scale,
        flow_wave: params.flow_wave,
        flow_frequency: params.flow_frequency,
        scatter: params.scatter,
        scale_variance: params.scale_variance,
        cylinder_bend: params.cylinder_bend,
        cylinder_radius: params.cylinder_radius,
        stage: params.stage,
    };
    let placements = donor_layout::grid_placements(item_count, &donor, poster_aspects);
    (donor, placements)
}

/// Immutable content geometry plus an ordered projected-y index. Camera, hover,
/// material and resource identity remain presentation/frame owned.
#[derive(Debug)]
pub(crate) struct WallLayoutWindow {
    key: WallParams,
    pub donor: DonorGridParams,
    pub placements: Vec<donor_layout::GridPlacement>,
    pub max_scroll: f32,
    draw_max_scroll: f32,
    y_order: Vec<(f32, usize)>,
    half_height: f32,
    camera_scale: f32,
    epsilon: f32,
}

impl WallLayoutWindow {
    fn key(mut p: WallParams) -> WallParams {
        p.scroll_offset = 0.0;
        p.motion = 0.0;
        p.user_engaged = false;
        p.corner_radius = 0.0;
        p.border_width = 0.0;
        p.selected_scale = 1.0;
        p.rail_height = None;
        p.rail_width = None;
        p.rail_gap = 0.0;
        p.rail_rtl = false;
        p
    }

    pub fn matches(&self, p: WallParams) -> bool {
        self.key == Self::key(p)
    }

    pub fn new(count: usize, params: WallParams, aspects: &[f32]) -> Self {
        let (donor, placements) = wall_placements(count, params, aspects);
        let bottom = placements
            .iter()
            .map(|p| p.y + p.height)
            .fold(0.0, f32::max);
        let mut half_height = 0.0_f32;
        let mut magnitude = 0.0_f32;
        let mut y_order: Vec<_> = placements
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let x = p.x + p.width * 0.5 - donor.total_w().max(1.0) * 0.5;
                let y = p.y + p.height * 0.5 - donor.total_h().max(1.0) * 0.5;
                let (x, y, scale) = donor.cylinder_transform(x, y, (0.0, 0.0));
                let y = donor.stage.transform(x, y, (0.0, 0.0), (0.0, 0.0)).1;
                // Stage perspective is clamped to 2. Dynamic selection focus
                // expands this bound at query time; exact clipping follows.
                half_height = half_height.max(p.height * scale.abs() * donor.stage.scale.abs());
                magnitude = magnitude.max(y.abs());
                (y, i)
            })
            .collect();
        y_order.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        let (sin, cos) = donor.stage.rotation.to_radians().sin_cos();
        let camera_scale = donor.stage.scale * (donor.stage.shear_x * sin + cos);
        Self {
            key: Self::key(params),
            max_scroll: (bottom - donor.total_h()).max(0.0),
            // Presentation camera bounds and the historical draw clamp differ
            // for supported negative gaps. Preserve both contracts.
            draw_max_scroll: (bottom - donor.total_h().max(1.0)).max(0.0),
            donor,
            placements,
            y_order,
            half_height,
            camera_scale,
            epsilon: magnitude * f32::EPSILON * 32.0 + 0.01,
        }
    }

    pub fn cards(
        &self,
        selected: usize,
        viewport: [f32; 2],
        params: WallParams,
        aspects: &[f32],
    ) -> Vec<SliceCard> {
        let frame = wall_frame(viewport, params);
        let camera = params.scroll_offset.clamp(0.0, self.draw_max_scroll);
        let shift = frame.center[1]
            + params.stage.offset_y * viewport[1] * 0.5
            + (params.motion * self.donor.cell_h() - camera) * self.camera_scale;
        let padding = self.half_height * params.selected_scale.max(1.0)
            + self.epsilon
            + shift.abs() * f32::EPSILON * 32.0;
        let low = frame.clip[1] - shift - padding;
        let high = frame.clip[3] - shift + padding;
        let start = self.y_order.partition_point(|p| p.0 < low);
        let end = self.y_order.partition_point(|p| p.0 <= high);
        if start >= end {
            return Vec::new();
        }
        let mut indices: Vec<_> = self.y_order[start..end].iter().map(|p| p.1).collect();
        indices.sort_unstable();
        donor_layout::layout_work(2, indices.len());
        wall_cards_from_placements(
            selected,
            viewport,
            params,
            aspects,
            &self.donor,
            camera,
            indices.into_iter().map(|i| &self.placements[i]),
        )
    }
}

fn layout_wall_with_aspects(
    item_count: usize,
    selected: usize,
    viewport: [f32; 2],
    params: WallParams,
    poster_aspects: &[f32],
) -> Vec<SliceCard> {
    if item_count == 0 {
        return Vec::new();
    }
    let selected = selected.min(item_count - 1);
    let (donor, placements) = wall_placements(item_count, params, poster_aspects);
    let frame_height = donor.total_h().max(1.0);
    let content_bottom = placements
        .iter()
        .map(|placement| placement.y + placement.height)
        .fold(0.0_f32, f32::max);
    let max_scroll = (content_bottom - frame_height).max(0.0);
    let camera = params.scroll_offset.clamp(0.0, max_scroll);
    wall_cards_from_placements(
        selected,
        viewport,
        params,
        poster_aspects,
        &donor,
        camera,
        placements.iter(),
    )
}

fn wall_cards_from_placements<'a>(
    selected: usize,
    viewport: [f32; 2],
    params: WallParams,
    poster_aspects: &[f32],
    donor: &DonorGridParams,
    camera: f32,
    placements: impl Iterator<Item = &'a donor_layout::GridPlacement>,
) -> Vec<SliceCard> {
    let frame = wall_frame(viewport, params);
    let frame_center = frame.center;
    let left = frame.left;
    let cards: Vec<_> = placements
        .filter_map(|placement| {
            let raw_x = left + placement.x + placement.width * 0.5;
            let raw_y =
                frame.top + params.motion * donor.cell_h() + placement.y + placement.height * 0.5
                    - camera;
            let (surface_x, surface_y, surface_scale) =
                donor.cylinder_transform(raw_x, raw_y, (frame_center[0], frame_center[1]));
            let (center_x, center_y, stage_scale) = donor.stage.transform(
                surface_x,
                surface_y,
                (frame_center[0], frame_center[1]),
                (viewport[0], viewport[1]),
            );
            let focus = if placement.index == selected && params.user_engaged {
                params.selected_scale.max(0.1)
            } else {
                1.0
            };
            let draw_width = placement.width * surface_scale * stage_scale * focus;
            let draw_height = placement.height * surface_scale * stage_scale * focus;
            if center_x + draw_width * 0.5 <= frame.clip[0]
                || center_x - draw_width * 0.5 >= frame.clip[2]
                || center_y + draw_height * 0.5 <= frame.clip[1]
                || center_y - draw_height * 0.5 >= frame.clip[3]
            {
                return None;
            }
            Some(SliceCard {
                triangle_direction: 0,
                index: placement.index,
                center: [center_x, center_y],
                size: [draw_width, draw_height],
                layer: (placement.index < MAX_POSTERS).then_some(placement.index as u32),
                selected: placement.index == selected,
                opacity: 1.0,
                skew_distance: 0.0,
                edge_tilt_distance: 0.0,
                rotation: 0.0,
                corner_radius: params.corner_radius.max(0.0),
                shape: 0,
                poster_aspect_ratio: valid_aspect(poster_aspects.get(placement.index).copied()),
            })
        })
        .collect();
    cards
}

fn valid_aspect(aspect: Option<f32>) -> f32 {
    aspect
        .filter(|aspect| aspect.is_finite() && *aspect > 0.0)
        .unwrap_or(DEFAULT_POSTER_ASPECT)
        .clamp(0.01, 100.0)
}

fn selected_last(cards: &mut [SliceCard], selected: usize) {
    cards.sort_by_key(|card| {
        if card.index == selected {
            usize::MAX
        } else {
            usize::MAX - card.index.abs_diff(selected)
        }
    });
}

pub fn hit_test(cards: &[SliceCard], x: f32, y: f32) -> Option<usize> {
    cards
        .iter()
        .rev()
        .find(|card| card.opacity > 0.0 && contains_point(card, x, y))
        .map(|card| card.index)
}

fn contains_point(card: &SliceCard, x: f32, y: f32) -> bool {
    let world_x = x - card.center[0];
    let world_y = y - card.center[1];
    let (sin, cos) = (-card.rotation).sin_cos();
    let point_x = world_x * cos - world_y * sin;
    let point_y = world_x * sin + world_y * cos;
    let half_x = card.size[0] * 0.5;
    let half_y = card.size[1] * 0.5;
    // The shader rotates placement, leaving shape coordinates local. Use the
    // donor's independently scaled flat-top polygon for every Hex rotation,
    // including tiny rotations and sub-pixel positive extents.
    if card.shape == 2 {
        return donor_layout::Hit {
            index: card.index,
            cx: 0.0,
            cy: 0.0,
            hw: half_x,
            hh: half_y,
            skew: 0.0,
            edge_tilt: 0.0,
            hex: true,
            hex_shape: DonorHexShape::Hexagon,
            triangle_direction: 0,
        }
        .contains(point_x, point_y);
    }
    if card.rotation.abs() < 0.0001 {
        match card.shape {
            0 => {
                return {
                    donor_layout::Hit {
                        index: card.index,
                        cx: card.center[0],
                        cy: card.center[1],
                        hw: half_x,
                        hh: half_y,
                        skew: card.skew_distance,
                        edge_tilt: card.edge_tilt_distance,
                        hex: false,
                        hex_shape: DonorHexShape::Hexagon,
                        triangle_direction: 0,
                    }
                    .contains(x, y)
                }
            }
            3 | 4 => {
                let donor_shape = match card.shape {
                    3 => DonorHexShape::Diamond,
                    _ => DonorHexShape::Triangle,
                };
                return donor_layout::Hit {
                    index: card.index,
                    cx: card.center[0],
                    cy: card.center[1],
                    hw: half_x,
                    hh: half_y,
                    skew: 0.0,
                    edge_tilt: 0.0,
                    hex: true,
                    hex_shape: donor_shape,
                    triangle_direction: card.triangle_direction.min(1),
                }
                .contains(x, y);
            }
            _ => {}
        }
    }
    let normalized_x = point_x / half_x.max(1.0);
    let normalized_y = point_y / half_y.max(1.0);
    match card.shape {
        1 => return normalized_x * normalized_x + normalized_y * normalized_y <= 1.0,
        3 => return normalized_x.abs() + normalized_y.abs() <= 1.0,
        4 => {
            let y = if card.triangle_direction == 0 {
                normalized_y
            } else {
                -normalized_y
            };
            return ((2.0 * normalized_x.abs() - y - 1.0) / 2.236_068).max(y - 1.0) <= 0.0;
        }
        _ => {}
    }
    let skew_half = card.skew_distance * 0.5;
    let tilt_half = card.edge_tilt_distance * 0.5;
    let body_x = (half_x - skew_half.abs()).max(1.0);
    let body_y = (half_y - tilt_half.abs()).max(1.0);
    let determinant = body_x * body_y - skew_half * tilt_half;
    if determinant.abs() < 1.0 {
        return false;
    }
    let transformed_x = (body_y * point_x + skew_half * point_y) / determinant * body_x;
    let transformed_y = (tilt_half * point_x + body_x * point_y) / determinant * body_y;
    transformed_x.abs() <= body_x && transformed_y.abs() <= body_y
}

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    resolution: [f32; 2],
    time: f32,
    visibility: f32,
    clip: [f32; 4],
}

#[derive(Debug)]
pub struct GalleryPrimitive {
    frame_sequence: Option<u64>,
    prewarm: Option<(AtlasPrewarmKey, bool)>,
    snapshot: Arc<GallerySnapshot>,
    poster_feedback: Option<fn(PosterFeedback)>,
}

pub struct GalleryPipeline {
    pipeline: wgpu::RenderPipeline,
    globals: wgpu::Buffer,
    instances: wgpu::Buffer,
    instance_capacity: usize,
    instance_count: u32,
    bc: bool,
    posters: wgpu::Texture,
    far: wgpu::Texture,
    near_slots: Vec<(Poster, u32)>,
    far_residents: Vec<(Poster, u32)>,
    near_idle: Option<std::time::Instant>,
    far_idle: Option<std::time::Instant>,
    maintenance_due: Option<std::time::Instant>,
    prewarm_wake_due: Option<std::time::Instant>,
    prewarm_receipt: Option<(
        AtlasPrewarmKey,
        Weak<Vec<Poster>>,
        Arc<std::sync::atomic::AtomicBool>,
    )>,
    preview: wgpu::Texture,
    bind_group: wgpu::BindGroup,
    bind_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    wanted_posters: Arc<Vec<Poster>>,
    reported_near_order: Vec<Poster>,
}

impl Pipeline for GalleryPipeline {
    fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        POSTER_LAYER_LIMIT.store(
            (device.limits().max_texture_array_layers as usize).min(MAX_ORDINARY_POSTERS),
            std::sync::atomic::Ordering::Relaxed,
        );
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("skwd donor item presentation"),
            source: wgpu::ShaderSource::Wgsl(include_str!("../shaders/skwd_item.wgsl").into()),
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("skwd donor scene globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bc = device
            .features()
            .contains(wgpu::Features::TEXTURE_COMPRESSION_BC);
        PREFER_BC_FAR.store(bc, std::sync::atomic::Ordering::Relaxed);
        let posters = poster_atlas(
            device,
            1,
            PosterEncoding::for_texture(PosterTier::Near, bc).texture_format(),
        );
        let far = far_atlas(
            device,
            FAR_HEIGHT,
            PosterEncoding::for_texture(PosterTier::Far, bc).texture_format(),
        );
        apply_trace::event_lazy("gallery-atlas-allocated", || {
            format!(
                "near_bytes={} far_bytes={} near_format={:?} far_format={:?}",
                near_bytes(posters.format()),
                texture_layout(far.format(), far.size()).2,
                posters.format(),
                far.format()
            )
        });
        let preview = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("skwd donor preview fallback"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &preview,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &[0, 0, 0, 255],
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(4),
                rows_per_image: Some(1),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("skwd donor scene sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let bind_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("skwd donor scene bind layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let bind_group = poster_bind_group(
            device,
            &bind_layout,
            &globals,
            &posters,
            &far,
            &preview,
            &sampler,
        );
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("skwd donor scene pipeline layout"),
            bind_group_layouts: &[&bind_layout],
            push_constant_ranges: &[],
        });
        let instance_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<InstanceRaw>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![
                0 => Float32x4,
                1 => Float32x4,
                2 => Float32x4,
                3 => Float32x4,
                4 => Float32x4,
                5 => Float32x4,
                6 => Float32x4,
                7 => Float32x4,
                8 => Uint32x4,
                9 => Float32x4,
                10 => Float32x4,
            ],
        };
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("skwd donor item render pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[instance_layout],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview: None,
            cache: None,
        });
        let instance_capacity = 16usize;
        let instances = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("skwd donor scene instances"),
            size: (instance_capacity * std::mem::size_of::<InstanceRaw>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Self {
            pipeline,
            globals,
            instances,
            instance_capacity,
            instance_count: 0,
            bc,
            posters,
            far,
            near_slots: Vec::new(),
            far_residents: Vec::new(),
            near_idle: None,
            far_idle: None,
            maintenance_due: None,
            prewarm_wake_due: None,
            prewarm_receipt: None,
            preview,
            bind_group,
            bind_layout,
            sampler,
            wanted_posters: Arc::new(Vec::new()),
            reported_near_order: Vec::new(),
        }
    }
}

fn poster_atlas(device: &wgpu::Device, layers: u32, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gallery poster array"),
        size: wgpu::Extent3d {
            width: POSTER_WIDTH,
            height: POSTER_HEIGHT,
            depth_or_array_layers: layers,
        },
        mip_level_count: NEAR_MIP_COUNT,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn far_atlas(device: &wgpu::Device, height: u32, format: wgpu::TextureFormat) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("gallery far tiles"),
        size: wgpu::Extent3d {
            width: FAR_ATLAS_SIZE,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

fn poster_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    posters: &wgpu::Texture,
    far: &wgpu::Texture,
    preview: &wgpu::Texture,
    sampler: &wgpu::Sampler,
) -> wgpu::BindGroup {
    let posters = posters.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let far = far.create_view(&wgpu::TextureViewDescriptor {
        dimension: Some(wgpu::TextureViewDimension::D2Array),
        ..Default::default()
    });
    let preview = preview.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("gallery poster binding"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(&posters),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::TextureView(&far),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(&preview),
            },
        ],
    })
}

fn poster_layer(texture: &wgpu::Texture, layer: u32) -> wgpu::TexelCopyTextureInfo<'_> {
    wgpu::TexelCopyTextureInfo {
        texture,
        mip_level: 0,
        origin: wgpu::Origin3d {
            x: 0,
            y: 0,
            z: layer,
        },
        aspect: wgpu::TextureAspect::All,
    }
}

static PREFER_BC_FAR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
const FAR_ATLAS_SIZE: u32 = 2048;
const FAR_WIDTH: u32 = 160;
const FAR_HEIGHT: u32 = 88;
const FAR_COLUMNS: u32 = FAR_ATLAS_SIZE / FAR_WIDTH;
const FAR_EXTENT: wgpu::Extent3d = wgpu::Extent3d {
    width: FAR_WIDTH,
    height: FAR_HEIGHT,
    depth_or_array_layers: 1,
};
const _: () = assert!(FAR_COLUMNS * (FAR_ATLAS_SIZE / FAR_HEIGHT) >= MAX_ORDINARY_POSTERS as u32);

fn far_tile_origin(layer: u32) -> wgpu::Origin3d {
    wgpu::Origin3d {
        x: layer % FAR_COLUMNS * FAR_WIDTH,
        y: layer / FAR_COLUMNS * FAR_HEIGHT,
        z: 0,
    }
}

fn poster_content_size(aspect: f32) -> (u32, u32) {
    let width = (POSTER_HEIGHT as f32 * aspect)
        .round()
        .clamp(1.0, POSTER_WIDTH as f32) as u32;
    let height = (POSTER_WIDTH as f32 / aspect)
        .round()
        .clamp(1.0, POSTER_HEIGHT as f32) as u32;
    (width, height)
}

fn far_tile_pixels(rgba: &[u8], aspect: f32) -> Option<Vec<u8>> {
    let canvas =
        image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(POSTER_WIDTH, POSTER_HEIGHT, rgba)?;
    let (width, height) = poster_content_size(aspect);
    let content = image::imageops::crop_imm(
        &canvas,
        (POSTER_WIDTH - width) / 2,
        (POSTER_HEIGHT - height) / 2,
        width,
        height,
    );
    Some(
        image::imageops::resize(
            &*content,
            FAR_WIDTH,
            FAR_HEIGHT,
            image::imageops::FilterType::Triangle,
        )
        .into_raw(),
    )
}

fn initial_far_pixels(rgba: &[u8], aspect: f32) -> Arc<Mutex<PosterPixels>> {
    let prefer_bc = PREFER_BC_FAR.load(std::sync::atomic::Ordering::Relaxed);
    let encoding = PosterEncoding::for_texture(PosterTier::Far, prefer_bc);
    Arc::new(Mutex::new(
        encoding
            .encode_far(rgba, aspect)
            .map_or(PosterPixels::Unavailable, |bytes| {
                PosterPixels::Ready(encoding, Arc::new(bytes.into()))
            }),
    ))
}

fn map_far_instance(instance: &mut InstanceRaw, poster: &Poster, slot: u32, height: u32) {
    let origin = far_tile_origin(slot);
    let (content_width, content_height) = poster_content_size(poster.aspect_ratio);
    let content_origins = [
        (POSTER_WIDTH - content_width) / 2,
        (POSTER_HEIGHT - content_height) / 2,
    ];
    for (axis, (canvas, content)) in [
        (POSTER_WIDTH, content_width),
        (POSTER_HEIGHT, content_height),
    ]
    .into_iter()
    .enumerate()
    {
        let offset = |uv: f32| {
            ((uv * canvas as f32 - content_origins[axis] as f32) / content as f32).clamp(0.0, 1.0)
        };
        let start = offset(instance.crop[axis]);
        let end = offset(instance.crop[axis] + instance.crop[axis + 2]);
        instance.crop[axis] = start;
        instance.crop[axis + 2] = end - start;
    }
    // Keep linear filtering within the tile at both edges.
    let atlas = FAR_ATLAS_SIZE as f32;
    instance.uv = [
        (origin.x as f32 + 0.5) / atlas,
        (origin.y as f32 + 0.5) / height as f32,
        (FAR_WIDTH - 1) as f32 / atlas,
        (FAR_HEIGHT - 1) as f32 / height as f32,
    ];
    instance.misc[0] = 2;
    instance.misc[1] = 0;
}

const POSTER_EXTENT: wgpu::Extent3d = wgpu::Extent3d {
    width: POSTER_WIDTH,
    height: POSTER_HEIGHT,
    depth_or_array_layers: 1,
};

fn near_extent(mip: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width: POSTER_WIDTH >> mip,
        height: POSTER_HEIGHT >> mip,
        depth_or_array_layers: 1,
    }
}

fn near_bytes(format: wgpu::TextureFormat) -> usize {
    (0..NEAR_MIP_COUNT)
        .map(|mip| texture_layout(format, near_extent(mip)).2)
        .sum()
}

// The canonical opaque, matted canvas is sRGB. Average light, not sRGB bytes.
// Worker-only; neither prepare nor atlas resize performs pixel processing.
fn downsample_near(source: &[u8], width: u32) -> Vec<u8> {
    let linear: [f32; 256] = std::array::from_fn(|i| {
        let v = i as f32 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    });
    let next = width as usize / 2;
    let mut result = vec![255; next * next * 4];
    for y in 0..next {
        for x in 0..next {
            let offsets = [
                (y * 2 * width as usize + x * 2) * 4,
                (y * 2 * width as usize + x * 2 + 1) * 4,
                ((y * 2 + 1) * width as usize + x * 2) * 4,
                ((y * 2 + 1) * width as usize + x * 2 + 1) * 4,
            ];
            for channel in 0..3 {
                let v = offsets
                    .iter()
                    .map(|i| linear[source[i + channel] as usize])
                    .sum::<f32>()
                    * 0.25;
                let srgb = if v <= 0.0031308 {
                    v * 12.92
                } else {
                    1.055 * v.powf(1.0 / 2.4) - 0.055
                };
                result[(y * next + x) * 4 + channel] = (srgb * 255.0).round() as u8;
            }
        }
    }
    result
}

fn texture_layout(format: wgpu::TextureFormat, extent: wgpu::Extent3d) -> (u32, u32, usize) {
    let (block_width, block_height) = format.block_dimensions();
    let bytes_per_row = extent.width.div_ceil(block_width) * format.block_copy_size(None).unwrap();
    let rows_per_image = extent.height.div_ceil(block_height);
    (
        bytes_per_row,
        rows_per_image,
        (bytes_per_row * rows_per_image) as usize,
    )
}

impl GalleryPipeline {
    fn grow_atlas(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, wanted: u32) {
        let grown = poster_atlas(device, wanted, self.posters.format());
        let slot_bytes = near_bytes(grown.format());
        let copied_layers = self.near_slots.len();
        if copied_layers != 0 {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("preserve resident gallery posters"),
            });
            for (_, slot) in &self.near_slots {
                for mip in 0..NEAR_MIP_COUNT {
                    encoder.copy_texture_to_texture(
                        wgpu::TexelCopyTextureInfo {
                            mip_level: mip,
                            ..poster_layer(&self.posters, *slot)
                        },
                        wgpu::TexelCopyTextureInfo {
                            mip_level: mip,
                            ..poster_layer(&grown, *slot)
                        },
                        near_extent(mip),
                    );
                }
            }
            // The copy must precede both new writes and drawing on the new binding.
            queue.submit([encoder.finish()]);
        }
        self.bind_group = poster_bind_group(
            device,
            &self.bind_layout,
            &self.globals,
            &grown,
            &self.far,
            &self.preview,
            &self.sampler,
        );
        self.posters = grown;
        apply_trace::event_lazy("gallery-atlas-growth", || {
            format!(
                "layers={wanted} bytes={} copied_layers={copied_layers} copied_bytes={} device_limit={}",
                wanted as usize * slot_bytes,
                copied_layers * slot_bytes,
                device.limits().max_texture_array_layers,
            )
        });
    }

    fn shrink_near(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, wanted: u32) {
        let smaller = poster_atlas(device, wanted, self.posters.format());
        let before =
            near_bytes(self.posters.format()) * self.posters.depth_or_array_layers() as usize;
        let mut encoder = device.create_command_encoder(&Default::default());
        for (new_slot, (_, old_slot)) in self.near_slots.iter_mut().enumerate() {
            for mip in 0..NEAR_MIP_COUNT {
                encoder.copy_texture_to_texture(
                    wgpu::TexelCopyTextureInfo {
                        mip_level: mip,
                        ..poster_layer(&self.posters, *old_slot)
                    },
                    wgpu::TexelCopyTextureInfo {
                        mip_level: mip,
                        ..poster_layer(&smaller, new_slot as u32)
                    },
                    near_extent(mip),
                );
            }
            *old_slot = new_slot as u32;
        }
        queue.submit([encoder.finish()]);
        self.posters = smaller;
        self.rebind(device);
        apply_trace::event_lazy("gallery-atlas-shrink", || {
            format!(
                "tier=near layers={wanted} before_bytes={before} after_bytes={} survivors={}",
                near_bytes(self.posters.format()) * wanted as usize,
                self.near_slots.len()
            )
        });
    }

    fn resize_far(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, height: u32) {
        let resized = far_atlas(device, height, self.far.format());
        let before = texture_layout(self.far.format(), self.far.size()).2;
        let old_height = self.far.height();
        let mut encoder = device.create_command_encoder(&Default::default());
        for (new_slot, (_, old_slot)) in self.far_residents.iter_mut().enumerate() {
            encoder.copy_texture_to_texture(
                wgpu::TexelCopyTextureInfo {
                    origin: far_tile_origin(*old_slot),
                    ..poster_layer(&self.far, 0)
                },
                wgpu::TexelCopyTextureInfo {
                    origin: far_tile_origin(new_slot as u32),
                    ..poster_layer(&resized, 0)
                },
                FAR_EXTENT,
            );
            *old_slot = new_slot as u32;
        }
        queue.submit([encoder.finish()]);
        self.far = resized;
        self.rebind(device);
        apply_trace::event_lazy("gallery-far-resize", || {
            format!("height={height} old_height={old_height} before_bytes={before} after_bytes={} survivors={}", texture_layout(self.far.format(), self.far.size()).2, self.far_residents.len())
        });
    }

    fn rebind(&mut self, device: &wgpu::Device) {
        self.bind_group = poster_bind_group(
            device,
            &self.bind_layout,
            &self.globals,
            &self.posters,
            &self.far,
            &self.preview,
            &self.sampler,
        );
    }

    fn retune_atlases(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        near: u32,
        far: u32,
        now: std::time::Instant,
        feedback: Option<fn(PosterFeedback)>,
    ) {
        let near_target = if near <= 1 {
            1
        } else {
            near.div_ceil(POSTER_GROWTH_LAYERS) * POSTER_GROWTH_LAYERS
        }
        .min(device.limits().max_texture_array_layers)
        .min(MAX_POSTERS as u32);
        let far_target = far_height(far);
        if far_target > self.far.height() {
            self.resize_far(device, queue, far_target);
        }
        if shrink_due(
            self.posters.depth_or_array_layers(),
            near_target,
            &mut self.near_idle,
            now,
        ) {
            // The caller orders the required residents first. Spare warm slots
            // can be evicted before the copy into the smaller texture.
            self.near_slots.truncate(near_target as usize);
            self.shrink_near(device, queue, near_target);
        }
        if shrink_due(self.far.height(), far_target, &mut self.far_idle, now) {
            self.resize_far(device, queue, far_target);
        }
        // One outstanding wake per pipeline; renewed demand cancels the low
        // watermark without creating a timer/polling stream.
        if self.maintenance_due.is_some_and(|due| now >= due) {
            self.maintenance_due = None;
        }
        if self.maintenance_due.is_none() {
            if let Some(due) = self
                .near_idle
                .into_iter()
                .chain(self.far_idle)
                .map(|start| start + ATLAS_SHRINK_DELAY)
                .min()
            {
                if let Some(feedback) = feedback {
                    self.maintenance_due = Some(due);
                    feedback(PosterFeedback::MaintenanceAfter(
                        due.saturating_duration_since(now),
                    ));
                }
            }
        }
    }
}

const ATLAS_SHRINK_DELAY: std::time::Duration = std::time::Duration::from_secs(2);

fn far_height(required: u32) -> u32 {
    if required <= FAR_COLUMNS {
        FAR_HEIGHT
    } else {
        (required.div_ceil(FAR_COLUMNS).div_ceil(4) * 4 * FAR_HEIGHT).min(FAR_ATLAS_SIZE)
    }
}

fn shrink_due(
    allocated: u32,
    target: u32,
    idle: &mut Option<std::time::Instant>,
    now: std::time::Instant,
) -> bool {
    if target * 2 > allocated {
        *idle = None;
        return false;
    }
    let start = idle.get_or_insert(now);
    if now.saturating_duration_since(*start) >= ATLAS_SHRINK_DELAY {
        *idle = None;
        true
    } else {
        false
    }
}

impl GallerySnapshot {
    fn upload_order(&self, physical_scale: f32, near_limit: u32) -> Vec<(&Poster, bool)> {
        let mut priority = [(3, false, f32::INFINITY); MAX_ORDINARY_POSTERS];
        let visible =
            self.wall_clip
                .unwrap_or([0.0, 0.0, self.scene.viewport[0], self.scene.viewport[1]]);
        for card in self.cards.iter().filter(|_| self.ordinary.is_none()) {
            if !card.selected
                && (card.center[0] + card.size[0] * 0.5 < visible[0]
                    || card.center[1] + card.size[1] * 0.5 < visible[1]
                    || card.center[0] - card.size[0] * 0.5 > visible[2]
                    || card.center[1] - card.size[1] * 0.5 > visible[3])
            {
                continue;
            }
            let Some(rank) = card
                .layer
                .and_then(|layer| priority.get_mut(layer as usize))
            else {
                continue;
            };
            let dx = card.center[0] - self.scene.viewport[0] * 0.5;
            let dy = card.center[1] - self.scene.viewport[1] * 0.5;
            let hovered = self.depth.as_ref().is_some_and(|depth| {
                depth
                    .cards
                    .iter()
                    .any(|entry| entry.card.index == card.index && entry.hovered)
            });
            let candidate = (
                if card.selected {
                    0
                } else if hovered {
                    1
                } else {
                    2
                },
                false,
                dx * dx + dy * dy,
            );
            if candidate < *rank {
                *rank = candidate;
            }
        }
        let mut posters_by_layer = [None; MAX_ORDINARY_POSTERS];
        for poster in self.posters.iter() {
            if let Some(slot) = posters_by_layer.get_mut(poster.layer as usize) {
                *slot = Some(poster);
            }
        }
        for entry in self.ordinary.iter().flatten() {
            let card = &entry.card;
            if card.opacity <= f32::EPSILON {
                continue;
            }
            let Some(layer) = card.layer.map(|layer| layer as usize) else {
                continue;
            };
            let Some(Some(poster)) = posters_by_layer.get(layer) else {
                continue;
            };
            let left = (card.center[0] - card.size[0] * 0.5).max(visible[0]);
            let top = (card.center[1] - card.size[1] * 0.5).max(visible[1]);
            let width = (card.center[0] + card.size[0] * 0.5).min(visible[2]) - left;
            let height = (card.center[1] + card.size[1] * 0.5).min(visible[3]) - top;
            if width <= 0.0 || height <= 0.0 {
                continue;
            }
            let (content_w, content_h) = poster_content_size(poster.aspect_ratio);
            let sampled_far_width = FAR_WIDTH as f32
                * (entry.crop[2] * POSTER_WIDTH as f32 / content_w as f32).clamp(0.0, 1.0);
            let sampled_far_height = FAR_HEIGHT as f32
                * (entry.crop[3] * POSTER_HEIGHT as f32 / content_h as f32).clamp(0.0, 1.0);
            let near = card.size[0] * physical_scale > sampled_far_width
                || card.size[1] * physical_scale > sampled_far_height;
            let tier = if card.selected {
                0
            } else if near {
                1
            } else {
                2
            };
            let candidate = (tier, !entry.hovered, -width * height * card.opacity);
            if candidate < priority[layer] {
                priority[layer] = candidate;
            }
        }
        if let Some(layer) = self.scene.detail_card.and_then(|card| card.layer) {
            if let Some(rank) = priority.get_mut(layer as usize) {
                *rank = (0, false, f32::NEG_INFINITY);
            }
        }
        let mut order: Vec<_> = self.posters.iter().take(MAX_ORDINARY_POSTERS).collect();
        order.sort_by(|a, b| {
            let a = priority
                .get(a.layer as usize)
                .copied()
                .unwrap_or((3, false, f32::INFINITY));
            let b = priority
                .get(b.layer as usize)
                .copied()
                .unwrap_or((3, false, f32::INFINITY));
            a.0.cmp(&b.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.total_cmp(&b.2))
        });
        let mut remaining_near = near_limit;
        order
            .into_iter()
            .map(|p| {
                let near = remaining_near > 0
                    && priority
                        .get(p.layer as usize)
                        .is_some_and(|rank| rank.0 <= 1);
                remaining_near -= u32::from(near);
                (p, near)
            })
            .collect()
    }
}

impl Primitive for GalleryPrimitive {
    type Pipeline = GalleryPipeline;

    fn prepare(
        &self,
        pipeline: &mut GalleryPipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        viewport: &Viewport,
    ) {
        let scale = viewport.scale_factor();
        let frame_sequence = self.frame_sequence;
        crate::apply_trace::event_lazy("gallery-frame-prepare", || {
            format!(
                "frame={} depth={} cards={} posters={} scene_time={:.4}",
                frame_sequence.map_or_else(|| "none".into(), |id| id.to_string()),
                self.snapshot.depth.is_some(),
                self.snapshot.cards.len(),
                self.snapshot.posters.len(),
                self.snapshot.scene.time
            )
        });
        let _timing = crate::apply_trace::cpu_span("detail-gpu-prepare", || {
            format!("posters={}", self.snapshot.posters.len())
        });
        let globals = Globals {
            resolution: [
                viewport.physical_width() as f32,
                viewport.physical_height() as f32,
            ],
            time: self.snapshot.scene.time,
            visibility: self.snapshot.scene.visibility.clamp(0.0, 1.0),
            clip: self.snapshot.physical_clip(scale),
        };
        let snapshot = &self.snapshot;
        let limit = device
            .limits()
            .max_texture_array_layers
            .min(MAX_POSTERS as u32);
        let order = snapshot.upload_order(scale, limit);
        let defer_near = snapshot.defer_near_restore && self.prewarm.is_none();
        let mut allow_near_restore = [!defer_near
            && (snapshot.ordinary.is_none() || snapshot.depth.is_some());
            MAX_ORDINARY_POSTERS];
        let [width, height] = snapshot.scene.viewport;
        let [left, top, right, bottom] = snapshot.wall_clip.unwrap_or([0.0, 0.0, width, height]);
        for entry in snapshot.ordinary.iter().flatten().filter(|_| !defer_near) {
            let card = &entry.card;
            let [x, y] = card.center;
            if card.opacity >= 0.5
                && x >= left.max(0.0)
                && x < right.min(width)
                && y >= top.max(0.0)
                && y < bottom.min(height)
            {
                if let Some(slot) = card
                    .layer
                    .and_then(|layer| allow_near_restore.get_mut(layer as usize))
                {
                    *slot = true;
                }
            }
        }
        if let Some(slot) = snapshot
            .scene
            .detail_card
            .and_then(|card| card.layer)
            .and_then(|layer| allow_near_restore.get_mut(layer as usize))
        {
            *slot = true;
        }
        let wanted_near = |poster: &Poster| {
            order
                .iter()
                .any(|(p, near)| *near && p.same_resource(poster))
        };
        if !Arc::ptr_eq(&pipeline.wanted_posters, &snapshot.posters) {
            pipeline.far_residents.retain(|(resident, _)| {
                snapshot
                    .posters
                    .iter()
                    .any(|wanted| wanted.same_resource(resident))
            });
            pipeline.wanted_posters = snapshot.posters.clone();
        }
        let required = order.iter().filter(|(_, near)| *near).count() as u32;
        if required.min(limit) > pipeline.posters.size().depth_or_array_layers {
            let wanted = required.div_ceil(POSTER_GROWTH_LAYERS) * POSTER_GROWTH_LAYERS;
            pipeline.grow_atlas(device, queue, wanted.min(limit));
        }
        let mut spare = pipeline
            .posters
            .size()
            .depth_or_array_layers
            .saturating_sub(required);
        pipeline.near_slots.retain(|(resident, _)| {
            if wanted_near(resident) {
                return true;
            }
            if spare == 0
                || snapshot.depth.is_some()
                || order.is_empty()
                || order.iter().any(|(poster, _)| {
                    poster.layer == resident.layer && !poster.same_resource(resident)
                })
            {
                return false;
            }
            spare -= 1;
            true
        });
        pipeline
            .near_slots
            .sort_by_key(|(poster, _)| !wanted_near(poster));
        let required_far = order.len() as u32;
        pipeline.retune_atlases(
            device,
            queue,
            required.min(limit),
            required_far,
            std::time::Instant::now(),
            self.poster_feedback,
        );
        queue.write_buffer(&pipeline.globals, 0, bytemuck::bytes_of(&globals));

        let mut uploaded_bytes = 0;
        let mut recycled_rgba = 0;
        let mut deferred = 0;
        let mut restore = Vec::new();
        let mut near_slots = [None; MAX_ORDINARY_POSTERS];
        let mut used_near = [false; MAX_POSTERS];
        for (poster, slot) in &pipeline.near_slots {
            if order.iter().any(|(wanted, _)| wanted.same_resource(poster)) {
                near_slots[poster.layer as usize] = Some(*slot);
            }
            used_near[*slot as usize] = true;
        }
        let mut far_slots = [false; MAX_ORDINARY_POSTERS];
        let mut used_far = [false; MAX_ORDINARY_POSTERS + FAR_COLUMNS as usize];
        for (poster, slot) in &pipeline.far_residents {
            far_slots[poster.layer as usize] = true;
            used_far[*slot as usize] = true;
        }
        // Publish the small tiles first so a cold viewport gets useful images
        // while the larger uploads share the remaining per-frame budget.
        for tier in [PosterTier::Far, PosterTier::Near] {
            for &(poster, near) in &order {
                let layer = poster.layer as usize;
                if layer >= MAX_ORDINARY_POSTERS {
                    continue;
                }
                let (texture, origin, extent) = match tier {
                    PosterTier::Near if near && near_slots[layer].is_none() => {
                        let Some(slot) = used_near
                            [..pipeline.posters.size().depth_or_array_layers as usize]
                            .iter()
                            .position(|used| !used)
                        else {
                            continue;
                        };
                        (
                            &pipeline.posters,
                            wgpu::Origin3d {
                                z: slot as u32,
                                ..Default::default()
                            },
                            POSTER_EXTENT,
                        )
                    }
                    PosterTier::Far => {
                        if far_slots[layer] {
                            if !near {
                                poster.discard_near_pixels();
                            }
                            continue;
                        }
                        {
                            let capacity =
                                ((pipeline.far.height() / FAR_HEIGHT) * FAR_COLUMNS) as usize;
                            let Some(slot) = used_far[..capacity.min(used_far.len())]
                                .iter()
                                .position(|used| !used)
                            else {
                                continue;
                            };
                            (&pipeline.far, far_tile_origin(slot as u32), FAR_EXTENT)
                        }
                    }
                    _ => continue,
                };
                let encoding = PosterEncoding::for_texture(tier, pipeline.bc);
                let mip_count = if tier == PosterTier::Near {
                    NEAR_MIP_COUNT
                } else {
                    1
                };
                let byte_count = if tier == PosterTier::Near {
                    near_bytes(texture.format())
                } else {
                    texture_layout(texture.format(), extent).2
                };
                if uploaded_bytes + byte_count > POSTER_UPLOAD_BYTES_PER_FRAME {
                    if poster.has_encoded_pixels(encoding) {
                        deferred += 1;
                        continue;
                    }
                } else if let Some(bytes) = poster.take_pixels(encoding) {
                    if bytes.len() != byte_count {
                        continue;
                    }
                    let mut offset = 0;
                    for mip in 0..mip_count {
                        let extent = if tier == PosterTier::Near {
                            near_extent(mip)
                        } else {
                            extent
                        };
                        let (bytes_per_row, rows_per_image, count) =
                            texture_layout(texture.format(), extent);
                        queue.write_texture(
                            wgpu::TexelCopyTextureInfo {
                                origin,
                                mip_level: mip,
                                ..poster_layer(texture, 0)
                            },
                            &bytes[offset..offset + count],
                            wgpu::TexelCopyBufferLayout {
                                offset: 0,
                                bytes_per_row: Some(bytes_per_row),
                                rows_per_image: Some(rows_per_image),
                            },
                            extent,
                        );
                        offset += count;
                    }
                    recycled_rgba += usize::from(bytes.is_pooled());
                    drop(bytes);
                    uploaded_bytes += byte_count;
                    match tier {
                        PosterTier::Near => {
                            used_near[origin.z as usize] = true;
                            near_slots[layer] = Some(origin.z);
                            pipeline.near_slots.push((poster.clone(), origin.z));
                        }
                        PosterTier::Far => {
                            far_slots[layer] = true;
                            let slot = origin.y / FAR_HEIGHT * FAR_COLUMNS + origin.x / FAR_WIDTH;
                            used_far[slot as usize] = true;
                            pipeline.far_residents.push((poster.clone(), slot));
                            if !near {
                                poster.discard_near_pixels();
                            }
                        }
                    }
                    continue;
                }
                if self.poster_feedback.is_some()
                    && (tier == PosterTier::Far || allow_near_restore[layer])
                    && poster.needs_restore(encoding)
                {
                    restore.push((poster.clone(), encoding));
                }
            }
        }
        if uploaded_bytes != 0 || !restore.is_empty() {
            let resident = order
                .iter()
                .filter(|(p, _)| {
                    let layer = p.layer as usize;
                    near_slots.get(layer).is_some_and(Option::is_some)
                        || far_slots.get(layer).copied().unwrap_or(false)
                })
                .count();
            apply_trace::event_lazy("gallery-poster-uploads", || {
                format!(
                    "new_bytes={uploaded_bytes} deferred={deferred} recover_requested={} resident={} wanted={} far_resident={} near_resident={} near_wanted={required} recycled_rgba={recycled_rgba}",
                    restore.len(), resident, order.len(), pipeline.far_residents.len(), pipeline.near_slots.len(),
                )
            });
        }
        if let Some(feedback) = self.poster_feedback {
            let near_order: Vec<_> = order
                .iter()
                .filter_map(|(poster, near)| {
                    (*near && allow_near_restore.get(poster.layer as usize) == Some(&true))
                        .then_some(*poster)
                })
                .collect();
            let previous = &pipeline.reported_near_order;
            let changed = previous.len() != near_order.len()
                || previous
                    .iter()
                    .zip(&near_order)
                    .any(|(old, new)| !old.same_resource(new));
            let near_encoding = PosterEncoding::for_texture(PosterTier::Near, pipeline.bc);
            let waiting_and_changed = changed
                && previous
                    .iter()
                    .chain(near_order.iter().copied())
                    .any(|poster| {
                        poster.near_restore_pending()
                            || (poster.needs_restore(near_encoding)
                                && !pipeline
                                    .near_slots
                                    .iter()
                                    .any(|(resident, _)| resident.same_resource(poster)))
                    });
            if !restore.is_empty() || waiting_and_changed {
                let near_order: Vec<_> = near_order.into_iter().cloned().collect();
                pipeline.reported_near_order.clone_from(&near_order);
                feedback(PosterFeedback::Restore {
                    posters: restore,
                    near_order,
                    depth: snapshot.depth.is_some(),
                });
            } else if changed {
                pipeline.reported_near_order.clear();
            }
            if deferred != 0 {
                feedback(PosterFeedback::Redraw);
            }
            if let Some((key, true)) = self.prewarm {
                let already_sent =
                    pipeline
                        .prewarm_receipt
                        .as_ref()
                        .is_some_and(|(prior, posters, _)| {
                            *prior == key && posters.ptr_eq(&Arc::downgrade(&snapshot.posters))
                        });
                let settled = order.iter().all(|(poster, near)| {
                    let layer = poster.layer as usize;
                    (!near
                        || !allow_near_restore[layer]
                        || near_slots[layer].is_some()
                        || poster.upload_unavailable(PosterTier::Near))
                        && (far_slots[layer] || poster.upload_unavailable(PosterTier::Far))
                });
                if settled && !already_sent {
                    pipeline.prewarm_receipt = Some((
                        key,
                        Arc::downgrade(&snapshot.posters),
                        Arc::new(std::sync::atomic::AtomicBool::new(true)),
                    ));
                    let posters = snapshot.posters.clone();
                    let pending = pipeline.prewarm_receipt.as_ref().unwrap().2.clone();
                    // Flush queued writes and wait for their actual GPU completion.
                    // This receipt establishes pre-entry residency, not presentation.
                    queue.submit([]);
                    queue.on_submitted_work_done(move || {
                        pending.store(false, std::sync::atomic::Ordering::Release);
                        feedback(PosterFeedback::Prewarmed { key, posters })
                    });
                }
            }
        }

        if pipeline
            .prewarm_receipt
            .as_ref()
            .is_some_and(|(_, _, pending)| pending.load(std::sync::atomic::Ordering::Acquire))
        {
            queue.submit([]);
            let now = std::time::Instant::now();
            if pipeline.prewarm_wake_due.is_none_or(|due| now >= due) {
                if let Some(feedback) = self.poster_feedback {
                    let delay = std::time::Duration::from_millis(16);
                    pipeline.prewarm_wake_due = Some(now + delay);
                    feedback(PosterFeedback::MaintenanceAfter(delay));
                }
            }
        }

        let mut far_posters = [None; MAX_ORDINARY_POSTERS];
        let mut far_locations = [None; MAX_ORDINARY_POSTERS];
        for (poster, slot) in &pipeline.far_residents {
            far_posters[poster.layer as usize] = Some(poster);
            far_locations[poster.layer as usize] = Some(*slot);
        }
        let mut raw = if let Some(depth) = &snapshot.depth {
            let residents: Vec<_> = pipeline
                .near_slots
                .iter()
                .map(|(p, _)| p.clone())
                .chain(pipeline.far_residents.iter().map(|(p, _)| p.clone()))
                .collect();
            depth_resident_instances(
                depth,
                &snapshot.posters,
                &residents,
                &snapshot.colors,
                scale,
                &snapshot.scene,
            )
        } else {
            let mut posters = far_posters;
            for (poster, _) in &pipeline.near_slots {
                posters[poster.layer as usize] = Some(poster);
            }
            let is_resident = |poster: &Poster| {
                posters
                    .get(poster.layer as usize)
                    .and_then(|resident| *resident)
                    .is_some_and(|resident| poster.same_resource(resident))
            };
            snapshot.ordinary_instances_with_residency(is_resident, scale)
        };
        crate::apply_trace::event_lazy("gallery-frame-residency", || {
            let missing = order
                .iter()
                .filter(|(poster, _)| {
                    let layer = poster.layer as usize;
                    !near_slots.get(layer).is_some_and(Option::is_some)
                        && !far_slots.get(layer).copied().unwrap_or(false)
                })
                .count();
            format!(
                "frame={} depth={} wanted={} resident={} missing={} far_resident={} near_resident={} near_wanted={} deferred={} instances={}",
                frame_sequence.map_or_else(|| "none".into(), |id| id.to_string()),
                snapshot.depth.is_some(),
                order.len(),
                order.len().saturating_sub(missing),
                missing,
                pipeline.far_residents.len(),
                pipeline.near_slots.len(),
                required,
                deferred,
                raw.len(),
            )
        });
        let mut draw_bindings = frame_sequence.map(|_| Vec::new());
        for instance in &mut raw {
            if !matches!(instance.misc[0], 1 | 2) {
                continue;
            }
            let layer = instance.misc[1] as usize;
            if let Some(slot) = near_slots.get(layer).copied().flatten() {
                instance.misc[0] = 4;
                instance.misc[1] = slot;
            } else if let Some(poster) = far_posters.get(layer).and_then(|p| *p) {
                map_far_instance(
                    instance,
                    poster,
                    far_locations[layer].unwrap(),
                    pipeline.far.height(),
                );
            } else {
                continue;
            }
            if let Some(bindings) = &mut draw_bindings {
                if let Some(poster) = snapshot.posters.iter().find(|p| p.layer as usize == layer) {
                    let tier = if instance.misc[0] == 4 { "near" } else { "far" };
                    bindings.push((&poster.source, tier, instance.rect, instance.params[2]));
                }
            }
        }
        if let (Some(frame), Some(bindings)) = (frame_sequence, draw_bindings) {
            apply_trace::event_lazy("gallery-depth-draw-bindings", || {
                format!(
                    "frame={frame} visibility={} bindings={}",
                    globals.visibility,
                    serde_json::to_string(&bindings).unwrap_or_default()
                )
            });
        }
        if raw.len() > pipeline.instance_capacity {
            pipeline.instance_capacity = raw.len().next_power_of_two();
            pipeline.instances = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("skwd donor scene instances"),
                size: (pipeline.instance_capacity * std::mem::size_of::<InstanceRaw>()) as u64,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !raw.is_empty() {
            queue.write_buffer(&pipeline.instances, 0, bytemuck::cast_slice(&raw));
        }
        pipeline.instance_count = raw.len() as u32;
    }

    fn draw(&self, _pipeline: &GalleryPipeline, _render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        false
    }

    fn render(
        &self,
        pipeline: &GalleryPipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        let render_started = self.frame_sequence.map(|_| std::time::Instant::now());
        if pipeline.instance_count == 0 {
            crate::apply_trace::event_lazy("gallery-frame-render", || {
                format!(
                    "frame={} instances=0 skipped=true",
                    self.frame_sequence
                        .map_or_else(|| "none".into(), |id| id.to_string())
                )
            });
            return;
        }
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Slices pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );
        pass.set_pipeline(&pipeline.pipeline);
        pass.set_bind_group(0, &pipeline.bind_group, &[]);
        pass.set_vertex_buffer(0, pipeline.instances.slice(..));
        pass.draw(0..6, 0..pipeline.instance_count);
        drop(pass);
        if let (Some(frame_sequence), Some(started)) = (self.frame_sequence, render_started) {
            // Attach to this command buffer, not the queue's previous submission.
            // Callback observation includes polling latency and is not presentation.
            encoder.on_submitted_work_done(move || {
                crate::apply_trace::event_lazy("gallery-frame-gpu-complete", || {
                    format!(
                        "frame={frame_sequence} command_buffer_complete=true render_to_callback_us={}",
                        started.elapsed().as_micros()
                    )
                });
            });
        }
        crate::apply_trace::event_lazy("gallery-frame-render", || {
            format!(
                "frame={} instances={} skipped=false",
                self.frame_sequence
                    .map_or_else(|| "none".into(), |id| id.to_string()),
                pipeline.instance_count
            )
        });
    }
}

#[cfg(test)]
pub fn decode_poster(path: &Path, layer: u32) -> Option<Poster> {
    decode_poster_uncached(path, layer, None, &|| true).ok()
}

fn poster_buffer(decoder: Option<&PosterDecoder>) -> PosterBytes {
    match decoder {
        Some(decoder) => decoder.pool.take(),
        None => vec![0; POSTER_BYTES].into(),
    }
}

fn poster_from_pixels(
    path: &Path,
    layer: u32,
    aspect_ratio: f32,
    rgba: PosterBytes,
    recovery: PosterRecovery,
    decoder: Option<&PosterDecoder>,
    wanted: &dyn Fn() -> bool,
) -> Option<Poster> {
    let far_pixels = initial_far_pixels(&rgba, aspect_ratio);
    if !wanted() {
        return None;
    }
    let stage_near = decoder
        .as_ref()
        .is_none_or(|decoder| decoder.pool.can_keep_scratch(decoder.producers));
    let near = if stage_near {
        PosterPixels::Ready(PosterEncoding::NearRgba, Arc::new(rgba))
    } else {
        PosterPixels::Consumed
    };
    let pixels = Arc::new(Mutex::new(near));
    if let Some(decoder) = decoder.filter(|_| stage_near) {
        decoder.track_staging(&pixels);
    }
    Some(Poster {
        layer,
        source: path.to_path_buf(),
        aspect_ratio,
        far_pixels,
        pixels,
        recovery,
    })
}

fn decode_poster_uncached(
    path: &Path,
    layer: u32,
    decoder: Option<&PosterDecoder>,
    wanted: &dyn Fn() -> bool,
) -> Result<Poster, PosterDecodeError> {
    let fingerprint = Fingerprint::capture(path).ok_or(PosterDecodeError::SourceChanged)?;
    let mut rgba = poster_buffer(decoder);
    let aspect_ratio = decode_poster_into(path, &mut rgba, wanted)?;
    if !wanted() {
        return Err(PosterDecodeError::Cancelled);
    }
    if !fingerprint.matches(path) {
        return Err(PosterDecodeError::SourceChanged);
    }
    poster_from_pixels(
        path,
        layer,
        aspect_ratio,
        rgba,
        PosterRecovery::Image {
            fingerprint,
            cache_path: None,
        },
        decoder,
        wanted,
    )
    .ok_or(PosterDecodeError::Cancelled)
}

/// Settings uses the existing poster pixels without their atlas padding.
pub(crate) fn still_thumbnail(
    path: &std::path::Path,
    resident: Option<&Poster>,
) -> Option<iced::widget::image::Handle> {
    let (rgba, aspect) = resident
        .and_then(Poster::current_cached_thumbnail)
        .or_else(|| decode_poster_pixels_with_aspect(path))?;
    let canvas = image::RgbaImage::from_raw(POSTER_WIDTH, POSTER_HEIGHT, rgba)?;
    let (width, height) = poster_content_size(aspect);
    let crop = image::imageops::crop_imm(
        &canvas,
        (POSTER_WIDTH - width) / 2,
        (POSTER_HEIGHT - height) / 2,
        width,
        height,
    );
    Some(iced::widget::image::Handle::from_rgba(
        width,
        height,
        crop.to_image().into_raw(),
    ))
}

fn decode_poster_pixels_with_aspect(path: &std::path::Path) -> Option<(Vec<u8>, f32)> {
    let mut rgba = vec![0; POSTER_BYTES];
    let aspect = decode_poster_into(path, &mut rgba, &|| true).ok()?;
    Some((rgba, aspect))
}

fn decode_poster_into(
    path: &Path,
    rgba: &mut [u8],
    wanted: &dyn Fn() -> bool,
) -> Result<f32, PosterDecodeError> {
    if !wanted() {
        return Err(PosterDecodeError::Cancelled);
    }
    let source = if is_gif(path) {
        decode_gif_cover(path, wanted)?
    } else {
        image::open(path)
            .map_err(PosterDecodeError::Image)?
            .into_rgba8()
    };
    if !wanted() {
        return Err(PosterDecodeError::Cancelled);
    }
    render_poster_into(&source, rgba, wanted).ok_or_else(|| {
        if wanted() {
            PosterDecodeError::InvalidPixels
        } else {
            PosterDecodeError::Cancelled
        }
    })
}

const GIF_COVER_MAX_FRAMES: u64 = 12;
const GIF_COVER_MAX_PIXELS: u64 = 24 * 1024 * 1024;
const GIF_COVER_RGBA_BYTES: u64 = 64 * 1024 * 1024;

fn decode_gif_cover(
    path: &Path,
    wanted: &dyn Fn() -> bool,
) -> Result<image::RgbaImage, PosterDecodeError> {
    use image::{AnimationDecoder, ImageDecoder};

    let file = std::fs::File::open(path).map_err(|error| PosterDecodeError::Image(error.into()))?;
    let mut decoder = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(file))
        .map_err(PosterDecodeError::Image)?;
    let (width, height) = decoder.dimensions();
    let pixels = u64::from(width) * u64::from(height);
    let bytes = pixels * 4;
    if pixels == 0 || bytes > GIF_COVER_RGBA_BYTES {
        return Err(PosterDecodeError::InvalidPixels);
    }
    if !wanted() {
        return Err(PosterDecodeError::Cancelled);
    }
    let mut limits = image::Limits::default();
    // An animated read can hold the first frame, disposal canvas, decoded patch
    // and current composite. Large GIFs retain their first frame within the same cap.
    // The underlying codec independently limits index conversion and metadata.
    if bytes * 4 > GIF_COVER_RGBA_BYTES {
        limits.max_alloc = Some(GIF_COVER_RGBA_BYTES - bytes);
        decoder
            .set_limits(limits)
            .map_err(PosterDecodeError::Image)?;
        let mut frame = image::RgbaImage::new(width, height);
        decoder
            .read_image(&mut frame)
            .map_err(PosterDecodeError::Image)?;
        return Ok(frame);
    }

    limits.max_alloc = Some(bytes * 3);
    decoder
        .set_limits(limits)
        .map_err(PosterDecodeError::Image)?;
    let mut frames = decoder.into_frames();
    let first = frames
        .next()
        .ok_or(PosterDecodeError::InvalidPixels)?
        .map_err(PosterDecodeError::Image)?
        .into_buffer();
    if !gif_cover_frame_is_empty(&first) {
        return Ok(first);
    }
    // Charge the largest allowed patch plus the composited canvas on every step,
    // including the first frame. A broken later frame leaves the first intact.
    let frame_limit = (GIF_COVER_MAX_PIXELS / (pixels * 3)).clamp(1, GIF_COVER_MAX_FRAMES);
    for _ in 1..frame_limit {
        if !wanted() {
            return Err(PosterDecodeError::Cancelled);
        }
        let Some(Ok(frame)) = frames.next() else {
            break;
        };
        let frame = frame.into_buffer();
        if !gif_cover_frame_is_empty(&frame) {
            return Ok(frame);
        }
    }
    Ok(first)
}

fn gif_cover_frame_is_empty(frame: &image::RgbaImage) -> bool {
    let (pixels, _) = frame.as_raw().as_chunks::<4>();
    if pixels.first().is_none_or(|pixel| pixel[3] == 0) {
        pixels.iter().all(|pixel| pixel[3] == 0)
    } else {
        pixels.iter().all(|pixel| *pixel == [0, 0, 0, 255])
    }
}

fn render_poster_into(
    source: &image::RgbaImage,
    rgba: &mut [u8],
    wanted: &dyn Fn() -> bool,
) -> Option<f32> {
    let (width, height) = source.dimensions();
    if width == 0 || height == 0 || !wanted() {
        return None;
    }
    let (fitted_width, fitted_height) = fitted_poster_size(width, height);
    let left = (POSTER_WIDTH - fitted_width) / 2;
    let top = (POSTER_HEIGHT - fitted_height) / 2;
    fill_poster_matte(rgba);
    // Keep the row and horizontal filter table bounded for ordinary sources.
    const MAX_ROW_WIDTH: u32 = 8192;
    if (width, height) == (fitted_width, fitted_height) || width > MAX_ROW_WIDTH {
        let mut canvas =
            image::ImageBuffer::<image::Rgba<u8>, _>::from_raw(POSTER_WIDTH, POSTER_HEIGHT, rgba)?;
        if (width, height) == (fitted_width, fitted_height) {
            image::imageops::overlay(&mut canvas, source, left.into(), top.into());
        } else {
            let fitted = image::imageops::resize(
                source,
                fitted_width,
                fitted_height,
                image::imageops::FilterType::Triangle,
            );
            image::imageops::overlay(&mut canvas, &fitted, left.into(), top.into());
        }
    } else {
        let (pixels, _) = source.as_raw().as_chunks::<4>();
        resize_poster_rows(
            (width, height),
            |x, y| pixels[y * width as usize + x],
            fitted_poster_rows(rgba, fitted_width, fitted_height),
            fitted_width,
            fitted_height,
            wanted,
        )?;
    }
    wanted().then_some(width as f32 / height as f32)
}

fn fitted_poster_size(width: u32, height: u32) -> (u32, u32) {
    fitted_poster_extent(f64::from(width), f64::from(height))
}

fn fitted_poster_extent(width: f64, height: f64) -> (u32, u32) {
    // Match DynamicImage::resize's f64 fit, including rounding at half pixels.
    let scale = (f64::from(POSTER_WIDTH) / width).min(f64::from(POSTER_HEIGHT) / height);
    (
        ((width * scale).round() as u32).max(1),
        ((height * scale).round() as u32).max(1),
    )
}

fn fill_poster_matte(rgba: &mut [u8]) {
    let [r, g, b] = POSTER_MATTE_RGB;
    for pixel in rgba.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[r, g, b, 255]);
    }
}

fn fitted_poster_rows(rgba: &mut [u8], width: u32, height: u32) -> impl Iterator<Item = &mut [u8]> {
    let stride = POSTER_WIDTH as usize * 4;
    let start = ((POSTER_HEIGHT - height) / 2) as usize * stride;
    let left = ((POSTER_WIDTH - width) / 2) as usize * 4;
    rgba[start..start + height as usize * stride]
        .chunks_exact_mut(stride)
        .map(move |row| &mut row[left..left + width as usize * 4])
}

// Adapted from image 0.25.10's imageops/sample.rs (MIT OR Apache-2.0).
// Preserve vertical-then-horizontal accumulation and rounding for v3 cache bytes.
struct TriangleAxis {
    len: usize,
    ratio: f32,
}

impl TriangleAxis {
    fn append_weights(
        &self,
        output: usize,
        weights: &mut Vec<f32>,
    ) -> (usize, std::ops::Range<usize>) {
        let support = self.ratio.max(1.0);
        let center = (output as f32 + 0.5) * self.ratio;
        let first = ((center - support).floor() as usize).min(self.len - 1);
        let end = ((center + support).ceil() as usize).clamp(first + 1, self.len);
        let center = center - 0.5;
        let start = weights.len();
        let mut sum = 0.0;
        for input in first..end {
            let weight = (1.0 - ((input as f32 - center) / support).abs()).max(0.0);
            weights.push(weight);
            sum += weight;
        }
        for weight in &mut weights[start..] {
            *weight /= sum;
        }
        (first, start..weights.len())
    }
}

fn resize_poster_rows<'a>(
    (source_width, source_height): (u32, u32),
    sample_pixel: impl Fn(usize, usize) -> [u8; 4],
    rows: impl Iterator<Item = &'a mut [u8]>,
    width: u32,
    height: u32,
    wanted: &dyn Fn() -> bool,
) -> Option<()> {
    use image::Pixel;

    let horizontal = TriangleAxis {
        len: source_width as usize,
        ratio: source_width as f32 / width as f32,
    };
    let vertical = TriangleAxis {
        len: source_height as usize,
        ratio: source_height as f32 / height as f32,
    };
    let mut x_weights = Vec::new();
    let columns: Vec<_> = (0..width as usize)
        .map(|x| horizontal.append_weights(x, &mut x_weights))
        .collect();
    let mut y_weights = Vec::new();
    let mut scratch = vec![[0.0; 4]; source_width as usize];
    for (y, row) in rows.enumerate() {
        if !wanted() {
            return None;
        }
        y_weights.clear();
        let (first, _) = vertical.append_weights(y, &mut y_weights);
        for (x, sample) in scratch.iter_mut().enumerate() {
            let mut sum = [0.0; 4];
            for (y, weight) in (first..).zip(&y_weights) {
                let pixel = sample_pixel(x, y);
                for channel in 0..4 {
                    sum[channel] += f32::from(pixel[channel]) * weight;
                }
            }
            *sample = sum;
        }
        for (pixel, (first, weights)) in row.chunks_exact_mut(4).zip(&columns) {
            let mut sum = [0.0; 4];
            for (source_pixel, weight) in scratch[*first..].iter().zip(&x_weights[weights.clone()])
            {
                for channel in 0..4 {
                    sum[channel] += source_pixel[channel] * weight;
                }
            }
            let fitted = image::Rgba(sum.map(|sample| sample.clamp(0.0, 255.0).round() as u8));
            let matte = image::Rgba::from_slice_mut(pixel);
            matte.blend(&fitted);
        }
    }
    Some(())
}

pub(crate) fn decode_poster_cached(
    path: &Path,
    layer: u32,
    cache: &WriteBatch,
    decoder: Option<&PosterDecoder>,
    wanted: &dyn Fn() -> bool,
) -> Result<Poster, PosterDecodeError> {
    let fingerprint = Fingerprint::capture(path).ok_or(PosterDecodeError::SourceChanged)?;
    let (source_width, source_height) =
        image::image_dimensions(path).map_err(PosterDecodeError::Dimensions)?;
    if source_width == 0 || source_height == 0 {
        return Err(PosterDecodeError::InvalidPixels);
    }
    let aspect_ratio = source_width as f32 / source_height as f32;
    let Some(cache) = cache.owner() else {
        return decode_poster_uncached(path, layer, decoder, wanted);
    };
    let poster = decode_poster_at(
        path,
        layer,
        cache,
        aspect_ratio,
        fingerprint.clone(),
        decoder,
        wanted,
    )?;
    fingerprint
        .matches(path)
        .then_some(poster)
        .ok_or(PosterDecodeError::SourceChanged)
}

pub fn poster_cache_path(path: &std::path::Path, cache_dir: &std::path::Path) -> Option<PathBuf> {
    let fingerprint = Fingerprint::capture(path)?;
    Some(poster_cache_path_for_stamp(path, cache_dir, &fingerprint))
}

pub(crate) fn video_poster_cache_path(path: &Path, cache_dir: &Path) -> Option<PathBuf> {
    let stamp = VideoStamp::capture(path).filter(VideoStamp::cacheable)?;
    Some(video_cache_path_for_stamp(path, cache_dir, &stamp))
}

fn video_cache_path_for_stamp(path: &Path, root: &Path, stamp: &VideoStamp) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    (
        path.to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase(),
        stamp,
        POSTER_WIDTH,
        POSTER_HEIGHT,
    )
        .hash(&mut hash);
    root.join(format!("video-poster-v3-{:016x}.rgba", hash.finish()))
}

fn poster_cache_path_for_stamp(
    path: &Path,
    cache_dir: &Path,
    fingerprint: &Fingerprint,
) -> PathBuf {
    // Keep non-GIF records reusable and retain the existing cache allowlist.
    let recipe = if is_gif(path) { 4 } else { 3 };
    poster_cache_path_for_recipe(path, cache_dir, fingerprint, recipe)
}

fn poster_cache_path_for_recipe(
    path: &Path,
    cache_dir: &Path,
    fingerprint: &Fingerprint,
    recipe: u8,
) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    recipe.hash(&mut hasher);
    path.to_string_lossy()
        .replace('\\', "/")
        .to_ascii_lowercase()
        .hash(&mut hasher);
    fingerprint.bytes.hash(&mut hasher);
    Some(fingerprint.modified).hash(&mut hasher);
    POSTER_WIDTH.hash(&mut hasher);
    POSTER_HEIGHT.hash(&mut hasher);
    cache_dir.join(format!("poster-{:016x}.rgba", hasher.finish()))
}

static POSTER_TEMP_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn publish_poster_cache(
    cache_path: &std::path::Path,
    header: &[u8],
    pixels: &[u8],
    nonce: u64,
    wanted: &dyn Fn() -> bool,
) -> bool {
    let temporary = cache_path.with_extension(format!("rgba.{nonce}.tmp"));
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
    {
        use std::io::Write;
        let written = file
            .write_all(header)
            .and_then(|()| file.write_all(pixels))
            .is_ok();
        drop(file);
        // Atomic create-only publication leaves no two-link window for guarded readers.
        let published = written
            && wanted()
            && crate::file_publication::publish_staged_file_create_only(&temporary, cache_path)
                .is_ok();
        let _ = std::fs::remove_file(&temporary);
        return published;
    }
    false
}

/// Fill the existing disk record without creating a slot, staging pixels, or caching a fallback.
pub(crate) fn pregenerate_poster(
    path: &Path,
    stamp: &PosterStamp,
    cache: &PosterCache,
    decoder: &PosterDecoder,
    rgba: &mut [u8],
    wanted: &dyn Fn() -> bool,
) -> bool {
    let current = || wanted() && stamp.matches(path);
    if !current() {
        return false;
    }
    match stamp {
        PosterStamp::Still(fingerprint) => {
            let cache_path = poster_cache_path_for_stamp(path, cache.root(), fingerprint);
            !cache.contains(&cache_path, POSTER_BYTES as u64)
                && decode_poster_into(path, rgba, wanted).is_ok()
                && publish_poster_cache(
                    &cache_path,
                    &[],
                    rgba,
                    POSTER_TEMP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
                    &current,
                )
        }
        PosterStamp::Video(stamp) if stamp.cacheable() => {
            let cache_path = video_cache_path_for_stamp(path, cache.root(), stamp);
            if cache.contains_video(&cache_path) {
                decoder.video_outcome(path, *stamp, true);
                return false;
            }
            if !current() || !decoder.can_retry_video(path, *stamp) {
                return false;
            }
            let aspect = decode_video_into(path, rgba, wanted);
            if !current() {
                return false;
            }
            decoder.video_outcome(path, *stamp, aspect.is_some());
            aspect.is_some_and(|aspect| {
                publish_video_cache(cache, &cache_path, aspect, rgba, &current)
            })
        }
        PosterStamp::Video(_) => false,
    }
}

fn publish_video_cache(
    cache: &PosterCache,
    path: &Path,
    aspect: f32,
    rgba: &[u8],
    current: &dyn Fn() -> bool,
) -> bool {
    if !current() {
        return false;
    }
    cache.discard_invalid_video(path);
    let header = cache::video_header(path, aspect, rgba);
    publish_poster_cache(
        path,
        &header,
        rgba,
        POSTER_TEMP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        current,
    )
}

fn decode_poster_at(
    path: &std::path::Path,
    layer: u32,
    cache: &PosterCache,
    aspect_ratio: f32,
    fingerprint: Fingerprint,
    decoder: Option<&PosterDecoder>,
    wanted: &dyn Fn() -> bool,
) -> Result<Poster, PosterDecodeError> {
    let cache_path = poster_cache_path_for_stamp(path, cache.root(), &fingerprint);
    let mut rgba = poster_buffer(decoder);
    if !wanted() {
        return Err(PosterDecodeError::Cancelled);
    }
    let cache_hit = cache.read_into(&cache_path, &mut rgba).is_some();
    apply_trace::event_lazy("poster-still-canonical", || {
        format!("source={path:?} hit={cache_hit}")
    });
    if !cache_hit {
        if !wanted() {
            return Err(PosterDecodeError::Cancelled);
        }
        decode_poster_into(path, &mut rgba, wanted)?;
        if !wanted() {
            return Err(PosterDecodeError::Cancelled);
        }
        if !fingerprint.matches(path) {
            return Err(PosterDecodeError::SourceChanged);
        }
        publish_poster_cache(
            &cache_path,
            &[],
            &rgba,
            POSTER_TEMP_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            &|| wanted() && fingerprint.matches(path),
        );
    }
    if !wanted() {
        return Err(PosterDecodeError::Cancelled);
    }
    if !fingerprint.matches(path) {
        return Err(PosterDecodeError::SourceChanged);
    }
    poster_from_pixels(
        path,
        layer,
        aspect_ratio,
        rgba,
        PosterRecovery::Image {
            fingerprint,
            cache_path: Some(cache_path),
        },
        decoder,
        wanted,
    )
    .ok_or(PosterDecodeError::Cancelled)
}

fn decode_video_into(path: &Path, rgba: &mut [u8], wanted: &dyn Fn() -> bool) -> Option<f32> {
    video::with_frame(path, wanted, |frame| {
        let (width, height) = frame.size();
        let (display_width, display_height) = frame.display_extent();
        let (fitted_width, fitted_height) = fitted_poster_extent(display_width, display_height);
        fill_poster_matte(rgba);
        resize_poster_rows(
            (width, height),
            |x, y| frame.pixel(x, y),
            fitted_poster_rows(rgba, fitted_width, fitted_height),
            fitted_width,
            fitted_height,
            wanted,
        )?;
        wanted().then_some((display_width / display_height) as f32)
    })
}

pub(crate) fn decode_video_poster(
    source: &PosterSource,
    cache: &WriteBatch,
    decoder: &PosterDecoder,
    wanted: &dyn Fn() -> bool,
) -> Result<Poster, PosterDecodeError> {
    let check = || {
        if !wanted() {
            return Err(PosterDecodeError::Cancelled);
        }
        source
            .video_is_current()
            .then_some(())
            .ok_or(PosterDecodeError::SourceChanged)
    };
    check()?;
    let stamp = source.video_stamp.ok_or(PosterDecodeError::SourceChanged)?;
    let mut rgba = poster_buffer(Some(decoder));
    let owner = stamp.cacheable().then(|| cache.owner()).flatten();
    let cache_path =
        owner.map(|owner| video_cache_path_for_stamp(&source.path, owner.root(), &stamp));
    let cached = owner
        .zip(cache_path.as_deref())
        .and_then(|(owner, path)| owner.read_video_into(path, &mut rgba));
    apply_trace::event_lazy("poster-video-canonical", || {
        format!("source={:?} hit={}", source.path, cached.is_some())
    });
    let aspect = if cached.is_some() {
        decoder.video_outcome(&source.path, stamp, true);
        cached
    } else if decoder.can_retry_video(&source.path, stamp) {
        let aspect = decode_video_into(&source.path, &mut rgba, wanted);
        check()?;
        decoder.video_outcome(&source.path, stamp, aspect.is_some());
        if let (Some(aspect), Some(owner), Some(path)) = (aspect, owner, cache_path.as_deref()) {
            publish_video_cache(owner, path, aspect, &rgba, &|| check().is_ok());
        }
        apply_trace::event_lazy("poster-video-frame", || {
            format!(
                "source={:?} outcome={}",
                source.path,
                if aspect.is_some() {
                    "frame"
                } else {
                    "fallback"
                }
            )
        });
        aspect
    } else {
        None
    };
    let real_frame = aspect.map(|aspect| (aspect, PosterRecovery::Video { stamp, cache_path }));
    let (aspect, recovery) = real_frame.unwrap_or_else(|| {
        fill_video_placeholder(&source.path, &mut rgba);
        (
            DEFAULT_POSTER_ASPECT,
            PosterRecovery::Placeholder { kind: source.kind },
        )
    });
    check()?;
    let poster = poster_from_pixels(
        &source.path,
        source.layer,
        aspect,
        rgba,
        recovery,
        Some(decoder),
        wanted,
    )
    .ok_or(PosterDecodeError::Cancelled)?;
    check()?;
    Ok(poster)
}

#[cfg(test)]
pub fn video_placeholder_poster(path: &Path, layer: u32) -> Poster {
    video_placeholder_with(path, layer, None)
}

#[cfg(test)]
fn video_placeholder_with(path: &Path, layer: u32, decoder: Option<&PosterDecoder>) -> Poster {
    placeholder_if_wanted(path, layer, PosterKind::Video, decoder, &|| true)
        .expect("an unconditional video placeholder cannot be cancelled")
}

pub(crate) fn placeholder_if_wanted(
    path: &Path,
    layer: u32,
    kind: PosterKind,
    decoder: Option<&PosterDecoder>,
    wanted: &dyn Fn() -> bool,
) -> Option<Poster> {
    let mut rgba = poster_buffer(decoder);
    if !wanted() {
        return None;
    }
    fill_video_placeholder(path, &mut rgba);
    if !wanted() {
        return None;
    }
    poster_from_pixels(
        path,
        layer,
        DEFAULT_POSTER_ASPECT,
        rgba,
        PosterRecovery::Placeholder { kind },
        decoder,
        wanted,
    )
}

fn fill_video_placeholder(path: &Path, rgba: &mut [u8]) {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.to_string_lossy()
        .to_ascii_lowercase()
        .hash(&mut hasher);
    let sample = hasher.finish();
    let base = [
        45 + (sample & 0x3f) as u8,
        42 + ((sample >> 8) & 0x3f) as u8,
        64 + ((sample >> 16) & 0x4f) as u8,
    ];
    for y in 0..POSTER_HEIGHT {
        let shade = (y * 36 / POSTER_HEIGHT) as u8;
        for x in 0..POSTER_WIDTH {
            let offset = ((y * POSTER_WIDTH + x) * 4) as usize;
            rgba[offset] = base[0].saturating_add(shade);
            rgba[offset + 1] = base[1].saturating_add(shade);
            rgba[offset + 2] = base[2].saturating_add(shade);
            rgba[offset + 3] = 255;
        }
    }
}

#[cfg(test)]
mod tests {
    fn gif_cover_fixture(path: &Path, frames: impl IntoIterator<Item = image::RgbaImage>) {
        let file = std::fs::File::create(path).unwrap();
        image::codecs::gif::GifEncoder::new(file)
            .encode_frames(frames.into_iter().map(image::Frame::new))
            .unwrap();
    }

    #[test]
    fn gif_cover_uses_first_nonempty_composite_and_keeps_a_detailed_opening() {
        use image::{Rgba, RgbaImage};
        let (profile, _) = cache::tests::fixture();
        let path = profile.join("preview.gif");
        let black = RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 255]));
        let red_patch = RgbaImage::from_fn(4, 4, |x, y| {
            if x == 1 && y == 2 {
                Rgba([240, 0, 0, 255])
            } else {
                Rgba([0, 0, 0, 0])
            }
        });
        let blue = RgbaImage::from_pixel(4, 4, Rgba([0, 0, 255, 255]));
        // The encoder writes Background disposal. The red frame must clear the
        // black frame outside its patch, and win over a later solid frame.
        gif_cover_fixture(&path, [black.clone(), red_patch.clone(), blue.clone()]);
        assert_eq!(decode_gif_cover(&path, &|| true).unwrap(), red_patch);
        gif_cover_fixture(&path, [RgbaImage::new(4, 4), blue.clone()]);
        assert_eq!(decode_gif_cover(&path, &|| true).unwrap(), blue);
        let mut dark = black.clone();
        dark.put_pixel(2, 2, Rgba([2, 1, 3, 255]));
        gif_cover_fixture(&path, [dark.clone(), blue.clone()]);
        assert_eq!(decode_gif_cover(&path, &|| true).unwrap(), dark);
        gif_cover_fixture(&path, [black.clone()]);
        assert_eq!(decode_gif_cover(&path, &|| true).unwrap(), black);
        gif_cover_fixture(
            &path,
            std::iter::repeat_n(black.clone(), GIF_COVER_MAX_FRAMES as usize).chain([blue]),
        );
        assert_eq!(decode_gif_cover(&path, &|| true).unwrap(), black);
        let polls = std::cell::Cell::new(0);
        let result = decode_gif_cover(&path, &|| {
            polls.set(polls.get() + 1);
            polls.get() < 3
        });
        assert!(matches!(result, Err(PosterDecodeError::Cancelled)));
    }

    #[test]
    fn gif_cover_replaces_legacy_record_and_restores_generated_artwork() {
        let (profile, root) = cache::tests::fixture();
        let path = profile.join("preview.gif");
        let black = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]));
        let cover = [230, 93, 35, 255];
        let artwork = image::RgbaImage::from_pixel(4, 4, image::Rgba(cover));
        gif_cover_fixture(&path, [black, artwork]);
        let stamp = Fingerprint::capture(&path).unwrap();
        let legacy = poster_cache_path_for_recipe(&path, &root, &stamp, 3);
        let current = poster_cache_path(&path, &root).unwrap();
        assert_ne!(legacy, current);
        let mut old_pixels = vec![0; POSTER_BYTES];
        for pixel in old_pixels.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        std::fs::write(&legacy, &old_pixels).unwrap();
        let owner = PosterCache::open(&profile, &root, true).unwrap();
        let cold = decode_poster_at(&path, 0, &owner, 1.0, stamp.clone(), None, &|| true).unwrap();
        let near = cold.take_pixels(PosterEncoding::NearRgba).unwrap();
        let far = cold.take_pixels(PosterEncoding::FarRgba).unwrap();
        assert_eq!(&near[..4], &cover);
        assert_eq!(std::fs::read(&legacy).unwrap(), old_pixels);
        assert_eq!(&std::fs::read(&current).unwrap()[..], &near[..]);

        let decoder = PosterDecoder::new(2);
        let mut rgba = vec![0; POSTER_BYTES];
        assert!(!pregenerate_poster(
            &path,
            &PosterStamp::Still(stamp.clone()),
            &owner,
            &decoder,
            &mut rgba,
            &|| true
        ));
        std::fs::remove_file(&current).unwrap();
        assert!(pregenerate_poster(
            &path,
            &PosterStamp::Still(stamp.clone()),
            &owner,
            &decoder,
            &mut rgba,
            &|| true
        ));
        assert_eq!(&std::fs::read(&current).unwrap()[..], &near[..]);
        use std::os::windows::fs::OpenOptionsExt;
        let locked_source = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();
        assert!(
            std::fs::File::open(&path).is_err(),
            "warm path must read the cache"
        );
        let warm = decode_poster_at(&path, 1, &owner, 1.0, stamp, None, &|| true).unwrap();
        drop(locked_source);
        assert_eq!(
            &warm.take_pixels(PosterEncoding::NearRgba).unwrap()[..],
            &near[..]
        );
        assert_eq!(
            &warm.take_pixels(PosterEncoding::FarRgba).unwrap()[..],
            &far[..]
        );
        let iced::widget::image::Handle::Rgba { pixels, .. } =
            still_thumbnail(&path, None).unwrap()
        else {
            panic!("thumbnail fallback must supply decoded pixels");
        };
        assert_eq!(&pixels[..4], &cover);
        drop(owner);
        std::fs::remove_file(&path).unwrap();
        for (encoding, expected) in [
            (PosterEncoding::FarRgba, far),
            (PosterEncoding::NearRgba, near),
        ] {
            warm.request_restore(encoding).unwrap().restore(|| true);
            assert_eq!(&warm.take_pixels(encoding).unwrap()[..], &expected[..]);
        }
    }

    #[test]
    #[ignore = "Requires the two supplied Workshop GIF paths (read-only)"]
    fn gif_cover_workshop_previews_read_only() {
        use image::AnimationDecoder;
        let paths =
            std::env::var_os("WALLPAPER_SWITCHER_TEST_GIF_COVERS").expect("supplied previews");
        let paths: Vec<_> = std::env::split_paths(&paths).collect();
        assert_eq!(paths.len(), 2);
        for path in paths {
            let before = Fingerprint::capture(&path).unwrap();
            let first = image::open(&path).unwrap().into_rgba8();
            let file = std::fs::File::open(&path).unwrap();
            let second = image::codecs::gif::GifDecoder::new(std::io::BufReader::new(file))
                .unwrap()
                .into_frames()
                .nth(1)
                .unwrap()
                .unwrap()
                .into_buffer();
            assert_eq!(first.dimensions(), (160, 160));
            assert!(gif_cover_frame_is_empty(&first));
            assert!(!gif_cover_frame_is_empty(&second));
            assert_eq!(decode_gif_cover(&path, &|| true).unwrap(), second);
            assert!(before.matches(&path));
            eprintln!("{}: selected composited frame 1", path.display());
        }
    }

    #[test]
    #[ignore = "Requires owned orientation, anamorphic and delayed-keyframe Windows fixtures"]
    fn native_video_metadata_and_delayed_keyframe_match_independent_pixels() {
        let root = PathBuf::from(
            std::env::var_os("WALLPAPER_SWITCHER_VIDEO_FIXTURES").expect("owned video fixtures"),
        );
        let mut rgba = vec![0; POSTER_BYTES];
        let at = |pixels: &[u8], x: usize, y: usize| -> [u8; 4] {
            let i = (y * POSTER_WIDTH as usize + x) * 4;
            pixels[i..i + 4].try_into().unwrap()
        };
        let red = |p: [u8; 4]| p[0] > 220 && p[1] < 32 && p[2] < 32 && p[3] == 255;
        let blue = |p: [u8; 4]| p[2] > 220 && p[1] < 32 && p[0] < 32 && p[3] == 255;
        let matte = [
            POSTER_MATTE_RGB[0],
            POSTER_MATTE_RGB[1],
            POSTER_MATTE_RGB[2],
            255,
        ];
        let oriented = decode_video_into(&root.join("rotate.mp4"), &mut rgba, &|| true);
        eprintln!(
            "rotation: aspect={oriented:?} left={:?} right={:?}",
            at(&rgba, 150, 256),
            at(&rgba, 360, 256)
        );
        assert_eq!(oriented, Some(2.0 / 3.0));
        assert!(red(at(&rgba, 150, 256)) && blue(at(&rgba, 360, 256)));
        assert_eq!(at(&rgba, 50, 256), matte);

        let wide = decode_video_into(&root.join("anamorphic.mp4"), &mut rgba, &|| true);
        eprintln!(
            "anamorphic: aspect={wide:?} top={:?} bottom={:?}",
            at(&rgba, 256, 155),
            at(&rgba, 256, 355)
        );
        assert_eq!(wide, Some(2.0));
        assert_eq!(at(&rgba, 256, 110), matte);
        assert!(red(at(&rgba, 256, 155)) && blue(at(&rgba, 256, 355)));
        assert_eq!(at(&rgba, 256, 401), matte);

        let delayed = decode_video_into(&root.join("delayed.mp4"), &mut rgba, &|| true);
        eprintln!(
            "delayed: aspect={delayed:?} top={:?} bottom={:?}",
            at(&rgba, 256, 155),
            at(&rgba, 256, 355)
        );
        assert_eq!(delayed, Some(1.5));
        assert!(red(at(&rgba, 256, 155)) && blue(at(&rgba, 256, 355)));
    }

    #[test]
    fn video_cache_repairs_bad_payload_and_header_with_create_only_race() {
        use std::sync::Barrier;
        let (profile, root) = cache::tests::fixture();
        let owner = PosterCache::open(&profile, &root, false).unwrap();
        let path = root.join("video-poster-v3-0123456789abcdef.rgba");
        let pixels = vec![53; POSTER_BYTES];
        let mut actual = vec![0; POSTER_BYTES];
        assert!(publish_video_cache(&owner, &path, 1.5, &pixels, &|| true));
        let initial = std::fs::read(&path).unwrap();
        assert_eq!(initial.len() as u64, cache::VIDEO_ENTRY_BYTES);
        let legacy = root.join("video-poster-0123456789abcdef.rgba");
        let mut legacy_record = 1.5_f32.to_le_bytes().to_vec();
        legacy_record.extend_from_slice(&pixels);
        std::fs::write(&legacy, &legacy_record).unwrap();
        assert!(owner.read_video_into(&legacy, &mut actual).is_none());
        owner.discard_invalid_video(&legacy);
        assert_eq!(std::fs::read(&legacy).unwrap(), legacy_record);
        for byte in [0, 6, 48] {
            let mut damaged = initial.clone();
            damaged[byte] ^= 0xff;
            std::fs::write(&path, damaged).unwrap();
            assert!(owner.read_video_into(&path, &mut actual).is_none());
            let start = Barrier::new(2);
            let published = std::thread::scope(|scope| {
                let race = || {
                    start.wait();
                    publish_video_cache(&owner, &path, 1.5, &pixels, &|| true)
                };
                let first = scope.spawn(race);
                let second = scope.spawn(race);
                usize::from(first.join().unwrap()) + usize::from(second.join().unwrap())
            });
            assert_eq!(published, 1);
            assert_eq!(owner.read_video_into(&path, &mut actual), Some(1.5));
            assert_eq!(actual, pixels);
            owner.discard_invalid_video(&path);
            assert_eq!(std::fs::read(&path).unwrap(), initial);
        }
        let alias = root.join("video-poster-v3-1111111111111111.rgba");
        std::fs::hard_link(&path, &alias).unwrap();
        owner.discard_invalid_video(&alias);
        assert!(alias.exists(), "repair refuses hard-linked records");
    }

    #[test]
    fn corrupt_video_attempt_budget_resets_on_revision_and_epoch() {
        let (profile, _) = cache::tests::fixture();
        let path = profile.join("temporary.mp4");
        std::fs::write(&path, b"corrupt").unwrap();
        let stamp = VideoStamp::capture(&path).unwrap();
        let decoder = PosterDecoder::new(4);
        assert!(decoder.can_retry_video(&path, stamp));
        for attempts in 1..=2 {
            decoder.video_outcome(&path, stamp, false);
            assert!(!decoder.can_retry_video(&path, stamp));
            decoder
                .video_failures
                .lock()
                .unwrap()
                .get_mut(&path)
                .unwrap()
                .last -= std::time::Duration::from_secs(3);
            assert_eq!(decoder.can_retry_video(&path, stamp), attempts < 2);
        }
        std::fs::write(&path, b"revised video").unwrap();
        let revised = VideoStamp::capture(&path).unwrap();
        assert!(decoder.can_retry_video(&path, revised));
        decoder.clear_video_failures();
        assert!(decoder.can_retry_video(&path, stamp));
    }

    #[test]
    #[ignore = "Needs owned H.264 red/blue and equal-size green Windows fixtures"]
    fn video_equal_mtime_replacement_and_outgoing_restore_keep_exact_pixels() {
        let baseline = PathBuf::from(
            std::env::var_os("WALLPAPER_SWITCHER_P6_FIXTURE").expect("owned red-blue clip"),
        );
        let green = baseline.with_file_name("green.mp4");
        let (profile, root) = cache::tests::fixture();
        let path = profile.join("active.mp4");
        let control = profile.join("control.mp4");
        std::fs::copy(&baseline, &path).unwrap();
        let original_metadata = path.metadata().unwrap();
        assert_eq!(original_metadata.len(), green.metadata().unwrap().len());
        let mut source = PosterSource {
            index: 0,
            layer: 0,
            item_id: "video".into(),
            kind: PosterKind::Video,
            still: None,
            video_stamp: VideoStamp::capture(&path),
            path: path.clone(),
        };
        assert!(source.video_stamp.unwrap().cacheable());
        let decoder = PosterDecoder::new(5);
        let cache = WriteBatch::from_test_root(&profile, &root);
        let outgoing = decode_video_poster(&source, &cache, &decoder, &|| true).unwrap();
        let prior_bytes = outgoing.take_pixels(PosterEncoding::NearRgba).unwrap();
        let old_record = video_poster_cache_path(&path, &root).unwrap();
        std::fs::copy(&green, &path).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new().set_modified(original_metadata.modified().unwrap()),
            )
            .unwrap();
        std::fs::copy(&green, &control).unwrap();
        assert_eq!(
            path.metadata().unwrap().modified().unwrap(),
            original_metadata.modified().unwrap()
        );
        assert!(matches!(
            decode_video_poster(&source, &cache, &decoder, &|| true),
            Err(PosterDecodeError::SourceChanged)
        ));

        drop(cache);
        drop(decoder);
        let decoder = PosterDecoder::new(5);
        let cache = WriteBatch::from_test_root(&profile, &root);
        source.video_stamp = VideoStamp::capture(&path);
        let fresh = decode_video_poster(&source, &cache, &decoder, &|| true)
            .unwrap()
            .take_pixels(PosterEncoding::NearRgba)
            .unwrap();
        assert!(
            old_record.exists(),
            "the obsolete cache was present after reopening the worker"
        );
        let active_record = video_poster_cache_path(&path, &root).unwrap();
        assert_ne!(old_record, active_record);
        let valid_record = std::fs::read(&active_record).unwrap();
        for index in [5, 50] {
            let mut damaged = valid_record.clone();
            damaged[index] ^= 0xff;
            std::fs::write(&active_record, damaged).unwrap();
            let repaired = decode_video_poster(&source, &cache, &decoder, &|| true)
                .unwrap()
                .take_pixels(PosterEncoding::NearRgba)
                .unwrap();
            assert_eq!(&fresh[..], &repaired[..]);
            assert_eq!(std::fs::read(&active_record).unwrap(), valid_record);
        }

        outgoing
            .request_restore(PosterEncoding::NearRgba)
            .unwrap()
            .restore(|| true);
        let restored = outgoing.take_pixels(PosterEncoding::NearRgba).unwrap();
        assert_eq!(&restored[..], &prior_bytes[..]);
        drop(restored);
        drop(prior_bytes);
        std::fs::remove_file(old_record).unwrap();
        outgoing
            .request_restore(PosterEncoding::NearRgba)
            .unwrap()
            .restore(|| true);
        assert!(outgoing.upload_unavailable(PosterTier::Near));

        source.path = control.clone();
        source.video_stamp = VideoStamp::capture(&control);
        let twin = decode_video_poster(&source, &cache, &decoder, &|| true)
            .unwrap()
            .take_pixels(PosterEncoding::NearRgba)
            .unwrap();
        assert_eq!(&fresh[..], &twin[..]);
        let pixel = (256 * POSTER_WIDTH as usize + 256) * 4;
        assert!(fresh[pixel + 1] > 220 && fresh[pixel] < 32 && fresh[pixel + 2] < 32);

        std::fs::remove_file(&control).unwrap();
        source.video_stamp = VideoStamp::capture(&control);
        assert!(matches!(
            decode_video_poster(&source, &cache, &decoder, &|| true),
            Err(PosterDecodeError::SourceChanged)
        ));
        std::fs::copy(&green, &control).unwrap();
        source.video_stamp = VideoStamp::capture(&control);
        assert_eq!(
            decode_video_poster(&source, &cache, &decoder, &|| true)
                .unwrap()
                .aspect_ratio,
            16.0 / 9.0
        );
    }

    #[test]
    #[ignore = "requires a Media Foundation H.264 decoder and WALLPAPER_SWITCHER_P6_FIXTURE"]
    fn p61_native_video_frame_orientation_fallback_and_restore() {
        use super::*;

        let input = PathBuf::from(std::env::var_os("WALLPAPER_SWITCHER_P6_FIXTURE").unwrap());
        let (profile, root) = cache::tests::fixture();
        let path = profile.join("frame.mp4");
        std::fs::copy(&input, &path).unwrap();
        let path = path.canonicalize().unwrap();
        let mut source = PosterSource {
            index: 0,
            layer: 0,
            item_id: "owned-video".into(),
            kind: PosterKind::Video,
            still: None,
            video_stamp: VideoStamp::capture(&path),
            path: path.clone(),
        };
        let cache = WriteBatch::new(&root);
        let decoder = PosterDecoder::new(4);
        let poster = decode_video_poster(&source, &cache, &decoder, &|| true).unwrap();
        assert_eq!(poster.aspect_ratio, 1.5);
        let original = poster.take_pixels(PosterEncoding::NearRgba).unwrap();
        let pixel = |x, y| {
            let offset = (y * POSTER_WIDTH as usize + x) * 4;
            &original[offset..offset + 4]
        };
        let top = pixel(256, 150);
        let bottom = pixel(256, 360);
        assert!(
            top[0] > 220 && top[1] < 32 && top[2] < 32 && top[3] == 255,
            "{top:?}"
        );
        assert!(
            bottom[0] < 32 && bottom[1] < 32 && bottom[2] > 220 && bottom[3] == 255,
            "{bottom:?}"
        );
        poster
            .request_restore(PosterEncoding::NearRgba)
            .unwrap()
            .restore(|| true);
        let restored = poster.take_pixels(PosterEncoding::NearRgba).unwrap();
        assert_eq!(&original[..], &restored[..]);
        drop(original);
        drop(restored);

        std::fs::write(&path, b"owned corrupt MP4").unwrap();
        assert!(matches!(
            decode_video_poster(&source, &cache, &decoder, &|| true),
            Err(PosterDecodeError::SourceChanged)
        ));
        poster
            .request_restore(PosterEncoding::NearRgba)
            .unwrap()
            .restore(|| true);
        assert!(poster.upload_unavailable(PosterTier::Near));
        source.video_stamp = VideoStamp::capture(&path);
        assert!(matches!(
            decode_video_poster(&source, &cache, &decoder, &|| false),
            Err(PosterDecodeError::Cancelled)
        ));
        let fallback = decode_video_poster(&source, &cache, &decoder, &|| true).unwrap();
        assert!(!fallback.matches_source(source.kind, source.stamp()));
        let expected = video_placeholder_poster(&path, 0)
            .take_pixels(PosterEncoding::NearRgba)
            .unwrap();
        let actual = fallback.take_pixels(PosterEncoding::NearRgba).unwrap();
        assert_eq!(&expected[..], &actual[..]);
        drop(actual);

        // This H.264 frame negotiates a 128x80 RGB surface on the test host.
        // The bottom eight rows are outside the 128x72 display aperture.
        std::fs::copy(input.with_file_name("green.mp4"), &path).unwrap();
        source.video_stamp = VideoStamp::capture(&path);
        let cropped = decode_video_poster(&source, &cache, &decoder, &|| true).unwrap();
        assert_eq!(cropped.aspect_ratio, 16.0 / 9.0);
        let pixels = cropped.take_pixels(PosterEncoding::NearRgba).unwrap();
        let offset = (395 * POSTER_WIDTH as usize + 256) * 4;
        let bottom = &pixels[offset..offset + 4];
        assert!(
            bottom[0] < 32 && bottom[1] > 220 && bottom[2] < 32 && bottom[3] == 255,
            "{bottom:?}"
        );
        assert!(std::fs::read_dir(root).unwrap().next().is_none());
    }

    use super::*;

    #[test]
    fn pk08_thumbnail_reloads_an_uncached_resident_at_the_same_path() {
        let path =
            std::env::temp_dir().join(format!("pk08-still-replacement-{}.png", std::process::id()));
        let write = |w, h, rgba| {
            image::RgbaImage::from_pixel(w, h, image::Rgba(rgba))
                .save(&path)
                .unwrap()
        };
        write(16, 8, [240, 0, 0, 255]);
        let resident = decode_poster(&path, 0).unwrap();
        write(8, 16, [0, 0, 240, 255]);
        let thumbnail = still_thumbnail(&path, Some(&resident)).unwrap();
        std::fs::remove_file(path).unwrap();
        assert!(
            matches!(thumbnail, iced::widget::image::Handle::Rgba {width: 256, height: 512, pixels, ..}
            if pixels[..4] == [0, 0, 240, 255])
        );
    }

    // An independent edge/cross-product oracle, expressed as six vertices.
    // Distances are to finite segments, not the implementation's half planes.
    fn r8_hex_oracle(p: [f32; 2], half: [f32; 2]) -> (bool, f32) {
        let vertices = [
            [half[0], 0.0],
            [half[0] * 0.5, half[1]],
            [-half[0] * 0.5, half[1]],
            [-half[0], 0.0],
            [-half[0] * 0.5, -half[1]],
            [half[0] * 0.5, -half[1]],
        ];
        let mut inside = true;
        let mut distance = f32::INFINITY;
        for i in 0..6 {
            let a = vertices[i];
            let b = vertices[(i + 1) % 6];
            let edge = [b[0] - a[0], b[1] - a[1]];
            let q = [p[0] - a[0], p[1] - a[1]];
            inside &= edge[0] * q[1] - edge[1] * q[0] >= 0.0;
            let t = ((q[0] * edge[0] + q[1] * edge[1]) / (edge[0] * edge[0] + edge[1] * edge[1]))
                .clamp(0.0, 1.0);
            distance = distance.min((q[0] - t * edge[0]).hypot(q[1] - t * edge[1]));
        }
        (inside, distance)
    }

    fn r8_world(card: &SliceCard, local: [f32; 2]) -> [f32; 2] {
        let (sin, cos) = card.rotation.sin_cos();
        [
            card.center[0] + cos * local[0] - sin * local[1],
            card.center[1] + sin * local[0] + cos * local[1],
        ]
    }

    #[test]
    fn r8_hex_polygon_contract() {
        for aspect in [0.05, 0.5, 1.0, 2.0, 3.75, 10.0] {
            let cards = layout_geometric(
                1,
                0,
                [1200.0, 800.0],
                GeometricParams {
                    radius: 100.0,
                    aspect,
                    ..Default::default()
                },
            );
            let mut card = cards[0].clone();
            let half = [card.size[0] * 0.5, card.size[1] * 0.5];
            for rotation in [0.0, 0.00005, 0.37, -1.1, std::f32::consts::PI] {
                card.rotation = rotation;
                for ix in -24..=24 {
                    for iy in -24..=24 {
                        let local = [ix as f32 / 20.0 * half[0], iy as f32 / 20.0 * half[1]];
                        let (inside, edge_distance) = r8_hex_oracle(local, half);
                        // A bounded roundoff band, not fractional-alpha equivalence.
                        if edge_distance < 0.002 {
                            continue;
                        }
                        let p = r8_world(&card, local);
                        assert_eq!(
                            hit_test(&[card.clone()], p[0], p[1]),
                            inside.then_some(card.index),
                            "aspect={aspect}, rotation={rotation}, local={local:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn r8_recorded_points_vertices_and_edge_band() {
        for aspect in [0.05, 0.5, 1.0, 2.0, 10.0] {
            let mut card = layout_geometric(
                1,
                0,
                [1200.0, 800.0],
                GeometricParams {
                    radius: 100.0,
                    aspect,
                    ..Default::default()
                },
            )[0]
            .clone();
            let half = [100.0 * aspect, 87.0];
            assert_eq!(card.size, [half[0] * 2.0, half[1] * 2.0]);
            // Original review point: the old aspect-2 shader was visibly inside.
            // The opposite mismatch: this stretched interior was masked away
            // by the old aspect-0.5 shader despite being clickable.
            for rotation in [0.0, 0.63, -1.2] {
                card.rotation = rotation;
                for sx in [-1.0, 1.0] {
                    for sy in [-1.0, 1.0] {
                        for (normalized, expected) in [([0.7, 0.9], false), ([0.4, 0.8], true)] {
                            let p = r8_world(
                                &card,
                                [sx * normalized[0] * half[0], sy * normalized[1] * half[1]],
                            );
                            assert_eq!(
                                hit_test(&[card.clone()], p[0], p[1]),
                                expected.then_some(0)
                            );
                        }
                        // Top edge, slope midpoint, top corner and side vertex.
                        // Exact fractional alpha/hits within 0.25 logical pixels
                        // of the edge are intentionally not equated.
                        for edge in [[0.0, 1.0], [0.25, 1.0], [0.5, 1.0], [0.75, 0.5], [1.0, 0.0]] {
                            for offset in [-0.25, 0.25] {
                                let local = [sx * edge[0] * half[0], sy * edge[1] * half[1]];
                                let length = local[0].hypot(local[1]);
                                let p = [
                                    local[0] * (1.0 + offset / length),
                                    local[1] * (1.0 + offset / length),
                                ];
                                let (inside, distance) = r8_hex_oracle(p, half);
                                assert!(distance > 0.002);
                                let world = r8_world(&card, p);
                                assert_eq!(
                                    hit_test(&[card.clone()], world[0], world[1]),
                                    inside.then_some(0)
                                );
                            }
                        }
                    }
                }
            }
            // Exactly representable local boundary coordinates, with no
            // translation/inverse-rotation rounding at this check.
            card.center = [0.0; 2];
            card.rotation = 0.0;
            for point in [
                [half[0], 0.0],
                [-half[0], 0.0],
                [half[0] * 0.5, half[1]],
                [-half[0] * 0.5, half[1]],
                [half[0] * 0.5, -half[1]],
                [-half[0] * 0.5, -half[1]],
                [0.0, half[1]],
                [0.0, -half[1]],
            ] {
                assert_eq!(hit_test(&[card.clone()], point[0], point[1]), Some(0));
            }
        }
    }

    #[test]
    fn r8_hex_deformed_layout_and_instance_contract() {
        for curve in [
            GeometricCurve::Flat,
            GeometricCurve::Arc,
            GeometricCurve::Wave,
            GeometricCurve::S,
            GeometricCurve::Cylinder,
        ] {
            let params = GeometricParams {
                aspect: 2.0,
                radius: 35.0,
                rows: 3,
                columns: 5,
                curve,
                curve_strength: 0.3,
                curve_frequency: 0.8,
                lens: 0.4,
                orbit: 0.2,
                twist: 0.25,
                scatter: 0.2,
                motion: 0.3,
                stage: StageParams {
                    offset_x: 0.1,
                    offset_y: -0.1,
                    scale: 0.8,
                    rotation: 15.0,
                    shear_x: 0.1,
                    shear_y: -0.15,
                    perspective: 0.2,
                    depth_angle: 20.0,
                },
                ..Default::default()
            };
            let cards = layout_geometric(15, 7, [1000.0, 700.0], params);
            assert!(!cards.is_empty());
            assert_eq!(cards.last().unwrap().index, 7);
            for card in cards {
                for scale in [0.75, 1.0, 2.0] {
                    let inst = crate::donor_scene::instance_for_card(
                        &card,
                        &GalleryColors::default(),
                        scale,
                        0.0,
                    );
                    assert_eq!(
                        inst.rect,
                        [
                            card.center[0] * scale,
                            card.center[1] * scale,
                            card.size[0] * 0.5 * scale,
                            card.size[1] * 0.5 * scale
                        ]
                    );
                    assert_eq!(inst.misc[3] & 1, 1);
                    assert_eq!((inst.misc[3] >> 8) & 15, 0); // Hex fallback only
                    assert_eq!(inst.shape[1], card.rotation);
                }
                let half = [card.size[0] * 0.5, card.size[1] * 0.5];
                for normalized in [[0.0, 0.0], [0.4, 0.8], [-0.8, -0.7], [1.1, 0.0]] {
                    let local = [normalized[0] * half[0], normalized[1] * half[1]];
                    let (inside, _) = r8_hex_oracle(local, half);
                    let p = r8_world(&card, local);
                    assert_eq!(
                        hit_test(&[card.clone()], p[0], p[1]),
                        inside.then_some(card.index)
                    );
                }
            }
        }
    }

    #[test]
    fn r8_hex_invalid_and_small_extents() {
        let template = layout_geometric(
            1,
            0,
            [200.0, 200.0],
            GeometricParams {
                radius: 40.0,
                rows: 1,
                columns: 1,
                ..Default::default()
            },
        )[0]
        .clone();
        for rotation in [0.0, 0.00005, 0.6] {
            for invalid in [0.0, -1.0, f32::NAN, f32::INFINITY] {
                for axis in 0..2 {
                    let mut card = template.clone();
                    card.rotation = rotation;
                    card.size[axis] = invalid;
                    assert_eq!(
                        hit_test(&[card.clone()], card.center[0], card.center[1]),
                        None
                    );
                }
            }
            let mut card = template.clone();
            card.center = [2.0, -3.0];
            card.rotation = rotation;
            card.size = [0.5, 0.8];
            for (local, inside) in [([0.1, 0.3], true), ([0.22, 0.3], false)] {
                let p = r8_world(&card, local);
                assert_eq!(
                    hit_test(&[card.clone()], p[0], p[1]),
                    inside.then_some(card.index)
                );
            }
            assert_eq!(hit_test(&[card.clone()], f32::NAN, 0.0), None);
            assert_eq!(hit_test(&[card], 0.0, f32::INFINITY), None);
        }
    }

    #[test]
    fn r8_hex_widget_routes_frontmost_selection_and_background() {
        let mut front = layout_geometric(
            1,
            0,
            [800.0, 600.0],
            GeometricParams {
                radius: 100.0,
                aspect: 2.0,
                ..Default::default()
            },
        )[0]
        .clone();
        front.index = 7;
        front.rotation = 0.43;
        let mut back = front.clone();
        back.index = 3;
        back.size = [700.0, 400.0];
        back.selected = false;
        let mut cards = vec![front.clone(), back];
        selected_last(&mut cards, 7);
        let program = GalleryProgram {
            prewarm: None,
            poster_feedback: None,
            snapshot: Arc::new(GallerySnapshot {
                defer_near_restore: false,
                ordinary: None,
                wall_clip: None,
                depth: None,
                cards,
                posters: Arc::new(Vec::new()),
                colors: GalleryColors::default(),
                scene: DonorSceneState {
                    stage: StageParams::default(),
                    camera: 0.0,
                    camera_target: 0.0,
                    camera_velocity: 0.0,
                    detail_progress: 0.0,
                    external_detail: false,
                    flip_options: Default::default(),
                    detail_source: None,
                    detail_card: None,
                    viewport: [800.0, 600.0],
                    time: 0.0,
                    visibility: 1.0,
                },
            }),
        };
        let bounds = Rectangle::new(Point::new(19.0, 27.0), Size::new(800.0, 600.0));
        for (local, expected) in [
            ([0.0, 0.0], Some(7)),
            ([80.0, 69.6], Some(7)),
            ([140.0, 78.3], Some(3)),
            ([300.0, 140.0], None),
        ] {
            let p = r8_world(&front, local);
            let action = <GalleryProgram as shader::Program<GalleryMessage>>::update(
                &program,
                &mut Default::default(),
                &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                bounds,
                mouse::Cursor::Available(Point::new(p[0] + bounds.x, p[1] + bounds.y)),
            )
            .expect("in-bounds click must route");
            let (message, _, status) = action.into_inner();
            assert_eq!(status, iced::event::Status::Captured);
            match (message, expected) {
                (
                    Some(GalleryMessage::Pointer {
                        index: Some(actual),
                        ..
                    }),
                    Some(expected),
                ) => {
                    assert_eq!(actual, expected)
                }
                (Some(GalleryMessage::Pointer { index: None, .. }), None) => {}
                other => panic!("wrong gallery route: {other:?}"),
            }
        }
        // Later draws win even when selected flags alone do not dictate order.
        let mut reversed = program.snapshot.cards.clone();
        reversed.reverse();
        assert_eq!(
            hit_test(&reversed, front.center[0], front.center[1]),
            Some(3)
        );
    }

    #[test]
    fn r8_shared_non_hex_hits_remain_unchanged() {
        for (shape, inside, outside) in [
            (GeometricShape::Circle, [0.6, 0.6], [0.9, 0.9]),
            (GeometricShape::Triangle, [0.0, 0.0], [0.8, -0.8]),
            (GeometricShape::Diamond, [0.3, 0.3], [0.8, 0.8]),
            (GeometricShape::Rhombus, [0.3, 0.3], [0.8, 0.8]),
        ] {
            let mut card = layout_geometric(
                1,
                0,
                [500.0, 500.0],
                GeometricParams {
                    shape,
                    radius: 80.0,
                    rows: 1,
                    columns: 1,
                    ..Default::default()
                },
            )[0]
            .clone();
            for rotation in [0.0, 0.7] {
                card.rotation = rotation;
                for (p, expected) in [(inside, true), (outside, false)] {
                    let world = r8_world(
                        &card,
                        [p[0] * card.size[0] * 0.5, p[1] * card.size[1] * 0.5],
                    );
                    assert_eq!(
                        hit_test(&[card.clone()], world[0], world[1]),
                        expected.then_some(card.index),
                        "{shape:?}"
                    );
                }
            }
        }
        let mut card = layout_slices(1, 0, [1000.0, 800.0], SliceParams::default())[0].clone();
        card.skew_distance = 0.0;
        card.edge_tilt_distance = 0.0;
        for rotation in [0.0, 0.7] {
            card.rotation = rotation;
            let center = r8_world(&card, [0.0, 0.0]);
            let outside = r8_world(&card, [card.size[0], 0.0]);
            assert_eq!(
                hit_test(&[card.clone()], center[0], center[1]),
                Some(card.index)
            );
            assert_eq!(hit_test(&[card.clone()], outside[0], outside[1]), None);
        }
    }

    #[test]
    fn r8_production_wgsl_parses_and_validates() {
        let module = wgpu::naga::front::wgsl::parse_str(include_str!("../shaders/skwd_item.wgsl"))
            .expect("production WGSL must parse");
        wgpu::naga::valid::Validator::new(
            wgpu::naga::valid::ValidationFlags::all(),
            wgpu::naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .expect("production WGSL must validate");
        assert_eq!(module.entry_points.len(), 2);
        // This is semantic validation of the entire production shader, not
        // execution, pipeline creation or a rendered-mask/CPU equivalence test.
    }

    #[test]
    fn curated_variant_labels_match_the_thirteen_supported_arrangements() {
        assert_eq!(
            GalleryMode::Slices.variant_labels(),
            &["CLASSIC", "DENSE", "CINEMA"]
        );
        assert_eq!(
            GalleryMode::Geometric.variant_labels(),
            &["HEX ARC", "TRI WAVE", "SHOWCASE DIAMOND", "RHOMBUS"]
        );
        assert_eq!(
            GalleryMode::Wall.variant_labels(),
            &[
                "UNIFORM",
                "BRICK",
                "MASONRY",
                "JUSTIFIED",
                "EDITORIAL",
                "CYLINDER"
            ]
        );
    }

    #[test]
    fn renderer_slice_defaults_disable_wobble() {
        assert!(!SliceParams::default().wobble);
    }

    #[test]
    fn slice_distances_remain_pixel_units() {
        let params = SliceParams {
            skew_distance: 72.0,
            edge_tilt_distance: 18.0,
            ..Default::default()
        };
        let cards = layout_slices(3, 1, [1920.0, 1080.0], params);
        let selected = cards.iter().find(|card| card.selected).unwrap();

        assert_eq!(selected.skew_distance, 72.0);
        assert_eq!(selected.edge_tilt_distance, 18.0);
    }

    #[test]
    fn slices_honor_persisted_geometry_and_visible_count() {
        let params = SliceParams {
            offset_x: 0.1,
            offset_y: -0.2,
            width: 120.0,
            expanded_width: 300.0,
            height: 240.0,
            visible_count: 5,
            corner_radius: 27.0,
            ..Default::default()
        };
        let cards = layout_slices(20, 10, [1000.0, 800.0], params);
        let selected = cards.iter().find(|card| card.selected).unwrap();

        assert_eq!(cards.len(), 5);
        assert_eq!(selected.size, [300.0, 240.0]);
        assert_eq!(selected.center, [550.0, 320.0]);
        assert_eq!(selected.corner_radius, 27.0);
    }

    #[test]
    fn slices_motion_moves_cards_but_fades_against_the_fixed_viewport() {
        let params = SliceParams {
            width: 120.0,
            expanded_width: 300.0,
            spacing: 10.0,
            visible_count: 5,
            motion: 0.75,
            ..Default::default()
        };
        let moving = layout_slices(8, 4, [1000.0, 800.0], params);
        let settled = layout_slices(
            8,
            4,
            [1000.0, 800.0],
            SliceParams {
                motion: 0.0,
                ..params
            },
        );
        let moving_card = moving.iter().find(|card| card.index == 2).unwrap();
        let settled_card = settled.iter().find(|card| card.index == 2).unwrap();
        let expected_stride = (params.width - params.skew_distance.abs()).max(1.0) + params.spacing;

        assert!(
            (moving_card.center[0] - settled_card.center[0] - params.motion * expected_stride)
                .abs()
                < 0.001
        );
        let expected_opacity =
            slice_opacity(moving_card.center[0], 500.0, 290.0, 252.0, expected_stride);
        let translated_reference_opacity = slice_opacity(
            moving_card.center[0],
            moving_card.center[0],
            290.0,
            252.0,
            expected_stride,
        );
        assert!((moving_card.opacity - expected_opacity).abs() < 0.001);
        assert_ne!(expected_opacity, translated_reference_opacity);
        assert_ne!(moving_card.opacity, settled_card.opacity);
    }

    #[test]
    fn slices_measure_spacing_on_the_sheared_midline() {
        let params = SliceParams {
            width: 200.0,
            expanded_width: 300.0,
            skew_distance: 80.0,
            spacing: 10.0,
            visible_count: 3,
            ..Default::default()
        };
        let cards = layout_slices(3, 1, [1000.0, 800.0], params);
        let center = cards.iter().find(|card| card.index == 1).unwrap();
        let right = cards.iter().find(|card| card.index == 2).unwrap();

        assert_eq!(right.center[0] - center.center[0], 180.0);
    }

    #[test]
    fn selected_overlapping_card_wins_hit_test() {
        let cards = layout_slices(5, 2, [800.0, 600.0], SliceParams::default());
        let selected = cards.iter().find(|card| card.selected).unwrap();

        assert_eq!(
            hit_test(&cards, selected.center[0], selected.center[1]),
            Some(2)
        );
    }

    #[test]
    fn texture_budget_is_bounded_by_count_and_bytes() {
        let bytes = MAX_POSTERS * POSTER_WIDTH as usize * POSTER_HEIGHT as usize * 4;

        assert_eq!(MAX_POSTERS, 32);
        assert_eq!(POSTER_WIDTH, 512);
        assert_eq!(POSTER_HEIGHT, 512);
        assert_eq!(bytes, 32 * 512 * 512 * 4);
        assert!(bytes <= 40 * 1024 * 1024);
    }

    #[test]
    fn packed_tile_upload_uses_block_rows_and_stays_within_its_tile() {
        assert_eq!(
            texture_layout(PosterEncoding::FarBc1.texture_format(), FAR_EXTENT),
            (320, 22, 7_040),
        );
        assert_eq!(
            texture_layout(PosterEncoding::NearBc7.texture_format(), POSTER_EXTENT),
            (2048, 128, 262_144),
        );
        let layer = MAX_ORDINARY_POSTERS as u32 - 1;
        let poster = video_placeholder_poster(std::path::Path::new("last.mp4"), layer);
        let mut instance = <InstanceRaw as bytemuck::Zeroable>::zeroed();
        instance.crop = crate::donor_scene::poster_crop_uv(poster.aspect_ratio, [160.0, 90.0]);
        map_far_instance(&mut instance, &poster, poster.layer, FAR_ATLAS_SIZE);
        let origin = far_tile_origin(layer);
        for (axis, (start, length)) in [(origin.x, FAR_WIDTH), (origin.y, FAR_HEIGHT)]
            .into_iter()
            .enumerate()
        {
            let first = (instance.uv[axis] + instance.crop[axis] * instance.uv[axis + 2])
                * FAR_ATLAS_SIZE as f32;
            let last =
                first + instance.crop[axis + 2] * instance.uv[axis + 2] * FAR_ATLAS_SIZE as f32;
            assert!(first >= start as f32 + 0.5);
            assert!(last <= (start + length) as f32 - 0.5);
        }
        assert_eq!(instance.misc[..2], [2, 0]);
    }

    #[test]
    fn far_and_near_uploads_consume_independent_bytes_for_the_same_resource() {
        let poster = video_placeholder_poster(std::path::Path::new("promote.mp4"), 3);
        let far = poster.take_pixels(PosterEncoding::FarRgba).unwrap();
        assert_eq!(far.len(), 160 * 88 * 4);
        let request = poster.request_restore(PosterEncoding::NearBc7).unwrap();
        assert!(request.poster().same_resource(&poster));
        drop(request);
        assert!(poster.has_encoded_pixels(PosterEncoding::NearRgba));
        poster
            .request_restore(PosterEncoding::NearBc7)
            .unwrap()
            .restore(|| true);
        assert_eq!(
            poster.take_pixels(PosterEncoding::NearBc7).unwrap().len(),
            near_bytes(PosterEncoding::NearBc7.texture_format())
        );
        poster
            .request_restore(PosterEncoding::FarRgba)
            .unwrap()
            .restore(|| true);
        assert_eq!(
            poster
                .take_pixels(PosterEncoding::FarRgba)
                .as_ref()
                .map(|bytes| &bytes[..]),
            Some(&far[..])
        );
    }

    #[test]
    fn bounded_decode_staging_handles_large_batches_cancel_and_restore() {
        let decoder = PosterDecoder::new(3);
        let make = |decoder: &PosterDecoder| {
            (0..7)
                .map(|layer| {
                    video_placeholder_with(
                        Path::new(&format!("pooled-{layer}.mp4")),
                        layer,
                        Some(decoder),
                    )
                })
                .collect::<Vec<_>>()
        };
        let cancelled = make(&decoder);
        assert_eq!(decoder.pool.counts(), (3, 1, 4));
        assert!(cancelled.iter().all(Poster::has_pixels));
        drop(cancelled);
        assert_eq!(decoder.pool.counts(), (3, 3, 4));

        decoder.begin_work();
        let posters = make(&decoder);
        assert_eq!(decoder.pool.counts(), (3, 1, 11));
        let encoding = PosterEncoding::NearRgba;
        let pinned = posters[0].clone();
        let requested = &posters[6];
        assert!(pinned.has_encoded_pixels(encoding));
        assert!(requested.needs_restore(encoding));

        decoder.begin_work();
        let cancelled_restore = requested.request_restore(encoding).unwrap();
        let mut cache = ReadBatch::default();
        decoder.restore(cancelled_restore, &mut cache, || false);
        assert!(requested.needs_restore(encoding));
        assert!(pinned.has_encoded_pixels(encoding));
        decoder.restore(
            requested.request_restore(encoding).unwrap(),
            &mut cache,
            || true,
        );
        assert!(pinned.needs_restore(encoding));

        // Taking from the shared poster does not release the lease before the
        // GPU's upload call returns. The same allocation can serve it again.
        let uploaded = requested.take_pixels(encoding).unwrap();
        let expected = video_placeholder_poster(&requested.source, requested.layer)
            .take_pixels(encoding)
            .unwrap();
        assert_eq!(&uploaded[..], &expected[..]);
        let address = uploaded.as_ptr();
        assert_eq!(decoder.pool.counts(), (3, 1, 12));
        drop(uploaded);
        assert_eq!(decoder.pool.counts(), (3, 2, 12));
        decoder.begin_work();
        decoder.restore(
            requested.request_restore(encoding).unwrap(),
            &mut cache,
            || true,
        );
        assert_eq!(requested.take_pixels(encoding).unwrap().as_ptr(), address);
    }

    #[test]
    fn parallel_decodes_advance_with_one_shared_bounded_canvas_pool() {
        let decoder = PosterDecoder::with_producers(6, 4);
        let posters = std::thread::scope(|scope| {
            let threads: Vec<_> = (0..4)
                .map(|producer| {
                    let decoder = &decoder;
                    scope.spawn(move || {
                        (producer * 8..(producer + 1) * 8)
                            .map(|layer| {
                                video_placeholder_with(
                                    Path::new("pooled.mp4"),
                                    layer,
                                    Some(decoder),
                                )
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            threads
                .into_iter()
                .flat_map(|thread| thread.join().unwrap())
                .collect::<Vec<_>>()
        });
        let (allocated, free, _) = decoder.pool.counts();
        assert!(allocated <= 6 && allocated - free <= 2);
        assert_eq!(posters.len(), 32);
        assert!(posters.iter().all(Poster::has_pixels));
        drop(posters);
        assert_eq!(decoder.pool.counts().1, allocated);
    }

    #[test]
    #[ignore = "Owned P1.4 GPU upload and near growth; explicit invocation only"]
    fn p14_near_growth_copies_original_slots_and_enforces_rgba_budget() {
        let instance = wgpu::Instance::default();
        let adapter = iced::futures::executor::block_on(
            instance.request_adapter(&wgpu::RequestAdapterOptions::default()),
        )
        .unwrap();
        let (device, queue) =
            iced::futures::executor::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                required_limits: wgpu::Limits {
                    max_texture_array_layers: 24,
                    ..wgpu::Limits::downlevel_defaults()
                },
                ..Default::default()
            }))
            .unwrap();
        let mut pipeline =
            GalleryPipeline::new(&device, &queue, wgpu::TextureFormat::Bgra8UnormSrgb);
        let viewport = Viewport::with_physical_size(iced::Size::new(640, 480), 1.0);
        let bounds = Rectangle::with_size(iced::Size::new(640.0, 480.0));
        let posters: Vec<_> = (0..17)
            .map(|i| {
                video_placeholder_poster(std::path::Path::new(&format!("poster-{i}.mp4")), 200 + i)
            })
            .collect();
        for poster in &posters {
            poster
                .request_restore(PosterEncoding::NearRgbaMips)
                .unwrap()
                .restore(|| true);
        }
        let cards: Vec<_> = posters
            .iter()
            .enumerate()
            .map(|(i, p)| SliceCard {
                index: i,
                center: [320.0, 240.0],
                size: [320.0, 180.0],
                corner_radius: 0.0,
                skew_distance: 0.0,
                selected: true,
                edge_tilt_distance: 0.0,
                rotation: 0.0,
                opacity: 1.0,
                layer: Some(p.layer),
                shape: 0,
                poster_aspect_ratio: p.aspect_ratio,
                triangle_direction: 0,
            })
            .collect();
        let frame = |count| GalleryPrimitive {
            frame_sequence: None,
            prewarm: None,
            snapshot: Arc::new(GallerySnapshot {
                defer_near_restore: false,
                cards: cards[..count].to_vec(),
                posters: Arc::new(posters[..count].to_vec()),
                ordinary: None,
                colors: GalleryColors::default(),
                scene: DonorSceneState {
                    stage: Default::default(),
                    camera: 0.0,
                    camera_target: 0.0,
                    camera_velocity: 0.0,
                    detail_progress: 0.0,
                    external_detail: false,
                    flip_options: Default::default(),
                    detail_source: None,
                    detail_card: None,
                    viewport: [640.0, 480.0],
                    time: 0.0,
                    visibility: 1.0,
                },
                depth: None,
                wall_clip: None,
            }),
            poster_feedback: None,
        };
        frame(1).prepare(&mut pipeline, &device, &queue, &bounds, &viewport);
        assert_eq!(pipeline.posters.size().depth_or_array_layers, 1);
        assert!(pipeline.near_slots[0].0.same_resource(&posters[0]));
        assert_eq!(pipeline.near_slots[0].1, 0);
        assert!(!posters[0].has_pixels());

        let frame = frame(posters.len());
        // A complete six-level RGBA poster counts against the same 8 MiB cap.
        for (expected_near, expected_far) in [(7, 1), (13, 1), (17, 17)] {
            let before_near = pipeline.near_slots.len();
            let before_far = pipeline.far_residents.len();
            frame.prepare(&mut pipeline, &device, &queue, &bounds, &viewport);
            let uploaded = (pipeline.near_slots.len() - before_near)
                * near_bytes(pipeline.posters.format())
                + (pipeline.far_residents.len() - before_far)
                    * texture_layout(pipeline.far.format(), FAR_EXTENT).2;
            assert!(uploaded <= POSTER_UPLOAD_BYTES_PER_FRAME);
            if pipeline.near_slots.len() < posters.len() {
                assert!(
                    uploaded + near_bytes(pipeline.posters.format())
                        > POSTER_UPLOAD_BYTES_PER_FRAME
                );
            }
            assert_eq!(pipeline.posters.size().depth_or_array_layers, 24);
            assert_eq!(pipeline.near_slots.len(), expected_near);
            assert_eq!(pipeline.far_residents.len(), expected_far);
            assert!(pipeline.near_slots[0].0.same_resource(&posters[0]));
            assert_eq!(pipeline.near_slots[0].1, 0);
        }
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("read back the copied near poster"),
            size: 4,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
        encoder.copy_texture_to_buffer(
            poster_layer(&pipeline.posters, 0),
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout::default(),
            },
            wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |result| {
                tx.send(result).unwrap();
            });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        rx.recv().unwrap().unwrap();
        let expected = video_placeholder_poster(&posters[0].source, 0)
            .take_pixels(PosterEncoding::NearRgba)
            .unwrap();
        assert_eq!(&*readback.slice(..).get_mapped_range(), &expected[..4]);
        assert_eq!(pipeline.far_residents.len(), posters.len());
        assert!(posters.iter().all(|p| !p.has_pixels()));
    }

    #[test]
    fn decoded_cpu_pixels_are_released_after_the_upload_claims_them() {
        let poster = Poster {
            layer: 0,
            source: PathBuf::from("poster.png"),
            aspect_ratio: 1.0,
            pixels: Arc::new(Mutex::new(PosterPixels::Ready(
                PosterEncoding::NearRgba,
                Arc::new(vec![7; 16].into()),
            ))),
            far_pixels: Arc::new(Mutex::new(PosterPixels::Unavailable)),
            recovery: PosterRecovery::Placeholder {
                kind: PosterKind::Video,
            },
        };

        assert!(poster.has_pixels());
        assert_eq!(
            poster
                .take_pixels(PosterEncoding::NearRgba)
                .as_ref()
                .map(|bytes| &bytes[..]),
            Some(&[7; 16][..])
        );
        assert!(!poster.has_encoded_pixels(PosterEncoding::NearRgba));
        assert!(poster.take_pixels(PosterEncoding::NearRgba).is_none());
    }

    #[test]
    fn released_video_pixels_are_recreated_by_the_worker_for_a_new_gpu_pipeline() {
        let poster = video_placeholder_poster(std::path::Path::new("sample.mp4"), 0);
        let initial = poster.take_pixels(PosterEncoding::NearRgba).unwrap();
        assert!(!poster.has_encoded_pixels(PosterEncoding::NearRgba));
        let (tx, rx) = std::sync::mpsc::channel();
        let worker = crate::poster_worker::PosterWorker::spawn(
            Arc::new(std::sync::atomic::AtomicU64::new(0)),
            |_| true,
        )
        .unwrap();
        worker.restore(
            std::slice::from_ref(&poster),
            std::slice::from_ref(&poster),
            vec![(poster.clone(), PosterEncoding::NearRgba)],
            false,
            move || {
                tx.send(()).unwrap();
            },
        );
        rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
        assert_eq!(
            poster
                .take_pixels(PosterEncoding::NearRgba)
                .as_ref()
                .map(|bytes| &bytes[..]),
            Some(&initial[..])
        );
        assert!(!poster.has_encoded_pixels(PosterEncoding::NearRgba));
    }

    #[test]
    fn shared_still_restores_independent_slots_with_one_canonical_read() {
        let (profile, root) = crate::poster_worker::cache::tests::fixture();
        let source = profile.join("shared.gif");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([92, 17, 201, 255]))
            .save(&source)
            .unwrap();
        let decoder = PosterDecoder::new(3);
        let owner = PosterCache::open(&profile, &root, false).unwrap();
        let pin = decode_poster_at(
            &source,
            5,
            &owner,
            1.0,
            Fingerprint::capture(&source).unwrap(),
            Some(&decoder),
            &|| true,
        )
        .unwrap();
        drop(owner);
        assert!(poster_cache_path(&source, &root).unwrap().is_file());
        let live = decoder.fork_slot(&pin, 1);
        let near = PosterEncoding::NearRgba;
        let pin_pixels = pin.take_pixels(near).unwrap();
        assert!(live.has_encoded_pixels(near));
        let live_pixels = live.take_pixels(near).unwrap();
        assert!(Arc::ptr_eq(&pin_pixels, &live_pixels));
        assert_eq!(&pin_pixels[..4], &[92, 17, 201, 255]);
        drop((pin_pixels, live_pixels));
        assert!(!pin.same_resource(&live));
        std::fs::remove_file(source).unwrap();

        // The original keyed file is still usable for both slots after source removal.
        let requests = || {
            [live.clone(), pin.clone()]
                .iter()
                .map(|poster| poster.request_restore(near).unwrap())
                .collect()
        };
        let before = decoder.pool.counts();
        let mut cache = ReadBatch::default();
        assert_eq!(
            decoder.restore_batch(requests(), &mut cache, || false, None),
            1
        );
        assert_eq!(decoder.pool.counts(), before);
        decoder.begin_work();
        assert_eq!(
            decoder.restore_batch(requests(), &mut cache, || true, None),
            1
        );
        assert_eq!(cache.open_attempts(), 1);
        let after = decoder.pool.counts();
        assert_eq!(after.0 + after.2 as usize, before.0 + before.2 as usize + 1);
        let live_pixels = live.take_pixels(near).unwrap();
        assert!(pin.has_encoded_pixels(near));
        let pin_pixels = pin.take_pixels(near).unwrap();
        assert!(Arc::ptr_eq(&pin_pixels, &live_pixels));
        assert_eq!(&pin_pixels[..4], &[92, 17, 201, 255]);
    }

    #[test]
    fn depth_neighbors_publish_exact_tiles_before_selected_mips() {
        use std::cell::Cell;
        for cancel_after_far in [false, true] {
            let decoder = PosterDecoder::new(2);
            let selected = video_placeholder_poster(Path::new("selected.mp4"), 0);
            let neighbor = video_placeholder_poster(Path::new("neighbor.mp4"), 1);
            for poster in [&selected, &neighbor] {
                let state = poster.pixels.lock().unwrap();
                let PosterPixels::Ready(PosterEncoding::NearRgba, rgba) = &*state else {
                    panic!("expected canonical fixture");
                };
                let tile = PosterEncoding::FarRgba
                    .encode_far(rgba, poster.aspect_ratio)
                    .unwrap();
                *poster.far_pixels.lock().unwrap() =
                    PosterPixels::Ready(PosterEncoding::FarRgba, Arc::new(tile.into()));
            }
            let alias = decoder.fork_slot(&neighbor, 2);
            let expected_near = {
                let state = selected.pixels.lock().unwrap();
                let PosterPixels::Ready(_, canonical) = &*state else {
                    panic!("canonical selected")
                };
                PosterEncoding::NearBc7
                    .encode(canonical.clone(), selected.aspect_ratio)
                    .unwrap()
            };
            let reference = neighbor.take_pixels(PosterEncoding::NearRgba).unwrap();
            let expected = PosterEncoding::FarBc1
                .encode_far(&reference, neighbor.aspect_ratio)
                .unwrap();
            let current = Cell::new(true);
            let wakes = Cell::new(0);
            let requests = vec![
                selected.request_restore(PosterEncoding::NearBc7).unwrap(),
                selected.request_restore(PosterEncoding::FarBc1).unwrap(),
                neighbor.request_restore(PosterEncoding::FarBc1).unwrap(),
                alias.request_restore(PosterEncoding::FarBc1).unwrap(),
            ];
            let mut cache = ReadBatch::default();
            let groups = decoder.restore_batch(
                requests,
                &mut cache,
                || current.get(),
                Some(&|| {
                    wakes.set(wakes.get() + 1);
                    assert!(matches!(
                        *selected.pixels.lock().unwrap(),
                        PosterPixels::Restoring
                    ));
                    assert!(selected.has_encoded_pixels(PosterEncoding::FarBc1));
                    for poster in [&neighbor, &alias] {
                        assert_eq!(
                            &poster.take_pixels(PosterEncoding::FarBc1).unwrap()[..],
                            &expected
                        );
                    }
                    current.set(!cancel_after_far);
                }),
            );
            assert_eq!(wakes.get(), 1);
            assert_eq!(groups, 2);
            assert_eq!(cache.open_attempts(), 0);
            if cancel_after_far {
                assert!(selected.has_encoded_pixels(PosterEncoding::NearRgba));
            } else {
                assert_eq!(
                    &selected.take_pixels(PosterEncoding::NearBc7).unwrap()[..],
                    &expected_near[..]
                );
            }
        }
    }

    #[test]
    fn depth_far_pack_rechecks_epoch_and_releases_only_its_own_request() {
        let poster = video_placeholder_poster(Path::new("depth-epoch.mp4"), 1);
        let tile = vec![37; (FAR_WIDTH * FAR_HEIGHT * 4) as usize];
        *poster.far_pixels.lock().unwrap() =
            PosterPixels::Ready(PosterEncoding::FarRgba, Arc::new(tile.into()));
        let mut stale = poster.request_restore(PosterEncoding::FarBc1).unwrap();
        let valid = std::cell::Cell::new(true);
        assert!(!stale.publish_staged_far(&|| valid.replace(false)));
        drop(stale);
        assert!(poster.has_encoded_pixels(PosterEncoding::FarRgba));
        let mut old = poster.request_restore(PosterEncoding::FarBc1).unwrap();
        assert!(old.publish_staged_far(&|| true));
        assert!(poster.take_pixels(PosterEncoding::FarBc1).is_some());
        let next = poster.request_restore(PosterEncoding::FarBc1).unwrap();
        drop(old);
        assert!(matches!(
            *poster.far_pixels.lock().unwrap(),
            PosterPixels::Restoring
        ));
        drop(next);
        assert!(matches!(
            *poster.far_pixels.lock().unwrap(),
            PosterPixels::Consumed
        ));
    }

    #[test]
    fn released_cached_pixels_restore_the_original_resource_after_source_removal() {
        let (profile, root) = crate::poster_worker::cache::tests::fixture();
        let source = profile.join("source.png");
        image::RgbaImage::new(2, 2).save(&source).unwrap();
        let cache_path = poster_cache_path(&source, &root).unwrap();
        let expected = vec![19; POSTER_BYTES];
        std::fs::write(&cache_path, &expected).unwrap();
        let cache = PosterCache::open(&profile, &root, false).unwrap();
        let poster = decode_poster_at(
            &source,
            0,
            &cache,
            1.0,
            Fingerprint::capture(&source).unwrap(),
            None,
            &|| true,
        )
        .unwrap();
        drop(cache);
        assert_eq!(
            poster
                .take_pixels(PosterEncoding::NearRgba)
                .as_ref()
                .map(|bytes| &bytes[..]),
            Some(expected.as_slice())
        );
        std::fs::remove_file(source).unwrap();
        let cancelled = poster.request_restore(PosterEncoding::NearRgba).unwrap();
        assert!(poster.request_restore(PosterEncoding::NearRgba).is_none());
        cancelled.restore(|| false);
        poster
            .request_restore(PosterEncoding::NearRgba)
            .unwrap()
            .restore(|| true);
        assert_eq!(
            poster
                .take_pixels(PosterEncoding::NearRgba)
                .as_ref()
                .map(|bytes| &bytes[..]),
            Some(expected.as_slice())
        );

        std::fs::remove_file(cache_path).unwrap();
        poster
            .request_restore(PosterEncoding::NearRgba)
            .unwrap()
            .restore(|| true);
        assert!(poster.take_pixels(PosterEncoding::NearRgba).is_none());
        assert!(
            poster.request_restore(PosterEncoding::NearRgba).is_none(),
            "failed exact resource is negative-cached"
        );
    }

    #[test]
    fn released_uncached_poster_refuses_pixels_from_a_changed_source() {
        let (profile, _) = crate::poster_worker::cache::tests::fixture();
        let source = profile.join("source.png");
        image::RgbaImage::new(2, 2).save(&source).unwrap();
        let poster = decode_poster(&source, 0).unwrap();
        assert!(poster.take_pixels(PosterEncoding::NearRgba).is_some());
        image::RgbaImage::new(7, 5).save(&source).unwrap();
        poster
            .request_restore(PosterEncoding::NearRgba)
            .unwrap()
            .restore(|| true);
        assert!(poster.take_pixels(PosterEncoding::NearRgba).is_none());
        assert!(poster.request_restore(PosterEncoding::NearRgba).is_none());
        let replacement = decode_poster(&source, 0).unwrap();
        assert!(replacement.has_pixels());
        assert!(!replacement.same_resource(&poster));
    }

    #[test]
    fn every_required_gallery_mode_places_the_selected_item_last() {
        for mode in [
            GalleryMode::Slices,
            GalleryMode::Geometric,
            GalleryMode::Wall,
        ] {
            let cards = layout_gallery(
                mode,
                40,
                17,
                [1920.0, 1080.0],
                GalleryLayoutParams::default(),
            );

            assert_eq!(cards.last().map(|card| card.index), Some(17), "{mode:?}");
        }
    }

    #[test]
    fn every_geometric_shape_uses_its_persisted_shape_and_aspect() {
        let cases = [
            (GeometricShape::Circle, 1),
            (GeometricShape::Hexagon, 2),
            (GeometricShape::Triangle, 4),
            (GeometricShape::Diamond, 3),
            (GeometricShape::Rhombus, 3),
        ];
        for (shape, code) in cases {
            let params = GeometricParams {
                shape,
                aspect: 1.4,
                ..Default::default()
            };
            let cards = layout_geometric(40, 17, [1920.0, 1080.0], params);
            let selected = cards.iter().find(|card| card.selected).unwrap();

            assert_eq!(selected.shape, code, "{shape:?}");
            assert!(selected.size[0] > 0.0 && selected.size[1] > 0.0);
        }
    }

    #[test]
    fn geometric_deformation_controls_materially_change_layout() {
        let base = layout_geometric(60, 25, [1600.0, 900.0], GeometricParams::default());
        let changed = layout_geometric(
            60,
            25,
            [1600.0, 900.0],
            GeometricParams {
                curve: GeometricCurve::Wave,
                curve_strength: 1.8,
                curve_frequency: 2.0,
                lens: 0.35,
                orbit: 0.4,
                twist: 18.0,
                scatter: 20.0,
                ..Default::default()
            },
        );

        let base_card = base.iter().find(|card| card.index == 24).unwrap();
        let changed_card = changed.iter().find(|card| card.index == 24).unwrap();
        assert_ne!(base_card.center, changed_card.center);
        assert_ne!(base_card.size, changed_card.size);
    }

    #[test]
    fn every_wall_arrangement_explicit_restore_keeps_the_selected_card_and_radius() {
        for layout in [
            WallLayout::Uniform,
            WallLayout::Brick,
            WallLayout::Masonry,
            WallLayout::Justified,
            WallLayout::Editorial,
            WallLayout::Cylinder,
        ] {
            let mut p = WallParams {
                layout,
                corner_radius: 31.0,
                ..Default::default()
            };
            let (grid, placements) = wall_placements(80, p, &[]);
            let c = placements.iter().find(|c| c.index == 23).unwrap();
            p.scroll_offset = (c.y + c.height * 0.5 - grid.total_h() * 0.5).max(0.0);
            let cards = layout_wall(80, 23, [1920.0, 1080.0], p);
            let selected = cards.iter().find(|card| card.selected).unwrap();

            assert!(
                cards.windows(2).all(|pair| pair[0].index < pair[1].index),
                "source Wall painter order: {layout:?}"
            );
            assert_eq!(selected.corner_radius, 31.0);
            assert!(
                selected.center[0] >= 0.0 && selected.center[0] <= 1920.0,
                "{layout:?}"
            );
        }
    }

    #[test]
    fn first_wall_item_is_navigable_or_explicitly_clipped_by_donor_frame() {
        for layout in [
            WallLayout::Uniform,
            WallLayout::Brick,
            WallLayout::Masonry,
            WallLayout::Justified,
            WallLayout::Editorial,
            WallLayout::Cylinder,
        ] {
            let cards = layout_wall(
                80,
                0,
                [1920.0, 1080.0],
                WallParams {
                    layout,
                    ..Default::default()
                },
            );
            let selected = cards.iter().find(|card| card.selected).unwrap();
            assert!(
                selected.center.iter().all(|value| value.is_finite()),
                "{layout:?}"
            );
            assert!(
                selected.size.iter().all(|value| value.is_finite()),
                "{layout:?}"
            );
            assert!(selected.size.iter().all(|value| *value > 0.0), "{layout:?}");
            let in_viewport = selected.center[0] + selected.size[0] * 0.5 > 0.0
                && selected.center[0] - selected.size[0] * 0.5 < 1920.0
                && selected.center[1] + selected.size[1] * 0.5 > 0.0
                && selected.center[1] - selected.size[1] * 0.5 < 1080.0;
            assert!(
                in_viewport,
                "selected first item must remain navigable: {layout:?}"
            );
            assert_eq!(
                hit_test(&cards, selected.center[0], selected.center[1]),
                Some(0),
                "{layout:?}"
            );
        }
    }

    #[test]
    fn uniform_wall_keeps_donor_composition_when_selection_moves() {
        let settled = WallParams {
            layout: WallLayout::Uniform,
            ..Default::default()
        };
        let first = layout_wall(40, 0, [1920.0, 1080.0], settled);
        let moved = layout_wall(40, 5, [1920.0, 1080.0], settled);
        let first_card = first.iter().find(|card| card.index == 0).unwrap();
        let moved_first_card = moved.iter().find(|card| card.index == 0).unwrap();
        let moved_selected = moved.iter().find(|card| card.index == 5).unwrap();

        assert_eq!(first_card.center, [190.0, 379.0]);
        assert_eq!(moved_first_card.center, first_card.center);
        assert_eq!(moved_selected.center[0], 1730.0);
        assert_ne!(moved_selected.center[0], 960.0);
    }

    #[test]
    fn wall_endpoint_uniform_first_interior_last_rows_follow_clamped_absolute_camera() {
        let p = WallParams {
            columns: 4,
            rows: 3,
            card_width: 90.0,
            card_height: 60.0,
            gap_x: 7.0,
            gap_y: 10.0,
            selected_scale: 1.0,
            ..Default::default()
        };
        // beta.24 horizontal rail: cy = vh/2 + (bar_height + gap)/2.
        // Frame H=200, content H=410, max camera=210; centers30,170,380.
        let viewport = [900.0, 700.0];
        for (selected, camera, expected_y) in
            [(0, 0.0, 296.0), (8, 70.0, 366.0), (20, 210.0, 436.0)]
        {
            let cards = layout_wall(
                24,
                selected,
                viewport,
                WallParams {
                    scroll_offset: camera,
                    ..p
                },
            );
            let card = cards.iter().find(|c| c.index == selected).unwrap();
            assert_eq!(card.center[1], expected_y, "selected {selected}");
            assert_eq!(
                hit_test(&cards, card.center[0], card.center[1]),
                Some(selected)
            );
        }
    }

    #[test]
    fn wall_endpoint_short_and_exact_fit_catalogs_do_not_move_with_selected_row() {
        let p = WallParams {
            columns: 4,
            rows: 3,
            card_width: 90.0,
            card_height: 60.0,
            gap_x: 7.0,
            gap_y: 10.0,
            selected_scale: 1.0,
            ..Default::default()
        };
        for count in [8, 12] {
            let first = layout_wall(count, 0, [900.0, 700.0], p);
            let last = layout_wall(count, count - 1, [900.0, 700.0], p);
            assert_eq!(first.len(), count);
            assert_eq!(last.len(), count, "fitting catalog must not lose first row");
            for c in &first {
                let other = last.iter().find(|x| x.index == c.index).unwrap();
                assert_eq!(c.center, other.center, "count {count}, item {}", c.index);
            }
            assert_eq!(
                first.iter().find(|c| c.index == 0).unwrap().center[1],
                296.0
            );
        }
    }

    #[test]
    fn wall_endpoint_absolute_scroll_bounds_and_same_row_selection() {
        let p = WallParams {
            columns: 4,
            rows: 3,
            card_width: 90.0,
            card_height: 60.0,
            gap_x: 7.0,
            gap_y: 10.0,
            selected_scale: 1.0,
            ..Default::default()
        };
        let positions = |selected, offset| {
            let mut cards = layout_wall(
                24,
                selected,
                [900.0, 700.0],
                WallParams {
                    scroll_offset: offset,
                    ..p
                },
            );
            cards.sort_by_key(|c| c.index);
            cards
                .iter()
                .map(|c| (c.index, c.center))
                .collect::<Vec<_>>()
        };
        assert_eq!(positions(8, 0.0), positions(11, 0.0));
        assert_eq!(positions(8, -1e6), positions(0, 0.0));
        assert_eq!(positions(8, 1e6), positions(23, 210.0));
        let partial = layout_wall(
            24,
            0,
            [900.0, 700.0],
            WallParams {
                scroll_offset: 20.0,
                ..p
            },
        );
        assert_eq!(
            partial.iter().find(|c| c.index == 0).unwrap().center[1],
            276.0
        );
    }

    #[test]
    fn wall_endpoint_frame_tracks_wrapped_hidden_rail_and_viewport_clip() {
        let p = WallParams {
            columns: 4,
            rows: 3,
            card_width: 90.0,
            card_height: 60.0,
            gap_x: 7.0,
            gap_y: 10.0,
            ..Default::default()
        };
        for (height, center_y) in [(None, 350.0), (Some(24.0), 366.0), (Some(72.0), 390.0)] {
            let frame = wall_frame(
                [900.0, 700.0],
                WallParams {
                    rail_height: height,
                    ..p
                },
            );
            assert_eq!(frame.center, [450.0, center_y]);
            assert_eq!(frame.top, center_y - 100.0);
            let rail_height = height.unwrap_or(0.0);
            if height.is_some() {
                let rail_top = (700.0 - rail_height - 8.0 - 200.0) * 0.5;
                assert_eq!(frame.top - (rail_top + rail_height), 8.0);
            }
        }
        let frame = wall_frame([160.0, 120.0], p);
        assert_eq!(frame.clip, [0.0, 0.0, 160.0, 120.0]);
    }

    #[test]
    fn wall_endpoint_motion_and_cylinder_stage_transform_remain_after_camera_placement() {
        let mut p = WallParams {
            columns: 4,
            rows: 3,
            card_width: 90.0,
            card_height: 60.0,
            gap_x: 7.0,
            gap_y: 10.0,
            selected_scale: 1.0,
            motion: 0.2,
            layout: WallLayout::Cylinder,
            ..Default::default()
        };
        p.stage.shear_y = 0.1;
        let viewport = [900.0, 700.0];
        let frame = wall_frame(viewport, p);
        let donor = DonorGridParams {
            cols: 4,
            rows: 3,
            thumb_w: 90.0,
            thumb_h: 60.0,
            gap_x: 7.0,
            gap_y: 10.0,
            layout: DonorGridLayout::Cylinder,
            cylinder_bend: p.cylinder_bend,
            cylinder_radius: p.cylinder_radius,
            stage: p.stage,
            ..Default::default()
        };
        let raw = [frame.left + 45.0, frame.top + 30.0 + 0.2 * 70.0];
        let (x, y, surface) =
            donor.cylinder_transform(raw[0], raw[1], (frame.center[0], frame.center[1]));
        let (x, y, stage) =
            donor
                .stage
                .transform(x, y, (frame.center[0], frame.center[1]), (900.0, 700.0));
        let cards = layout_wall(24, 0, viewport, p);
        let c = cards.iter().find(|c| c.index == 0).unwrap();
        assert_eq!(c.center, [x, y]);
        assert_eq!(c.size, [90.0 * surface * stage, 60.0 * surface * stage]);
    }

    #[test]
    fn wall_endpoint_shader_clip_and_real_widget_hits_share_frame_but_not_external_detail() {
        let p = WallParams {
            columns: 2,
            rows: 1,
            card_width: 100.0,
            card_height: 80.0,
            selected_scale: 1.2,
            user_engaged: true,
            ..Default::default()
        };
        let viewport = [500.0, 300.0];
        let frame = wall_frame(viewport, p);
        let cards = layout_wall(2, 0, viewport, p);
        let selected = cards.iter().find(|c| c.selected).unwrap();
        let point = [selected.center[0], frame.top - 2.0];
        assert_eq!(
            hit_test(&cards, point[0], point[1]),
            Some(0),
            "body extends beyond frame"
        );
        let snapshot = GallerySnapshot {
            defer_near_restore: false,
            ordinary: None,
            cards: cards.clone(),
            posters: Arc::new(Vec::new()),
            colors: GalleryColors::default(),
            depth: None,
            wall_clip: Some(frame.clip),
            scene: DonorSceneState {
                stage: StageParams::default(),
                camera: 0.0,
                camera_target: 0.0,
                camera_velocity: 0.0,
                detail_progress: 1.0,
                external_detail: true,
                flip_options: Default::default(),
                detail_source: None,
                detail_card: None,
                viewport,
                time: 0.0,
                visibility: 1.0,
            },
        };
        assert_eq!(snapshot.physical_clip(1.5), frame.clip.map(|v| v * 1.5));
        let raw = snapshot.ordinary_instances(1.5);
        assert!(raw[..cards.len()].iter().all(|r| r.misc[2] == 1));
        assert!(
            raw[cards.len()..].iter().all(|r| r.misc[2] == 0),
            "external detail is not a gallery body"
        );
        let program = GalleryProgram {
            prewarm: None,
            poster_feedback: None,
            snapshot: Arc::new(snapshot),
        };
        let event = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let bounds = Rectangle::new(Point::ORIGIN, Size::new(viewport[0], viewport[1]));
        for (point, expected) in [(point, None), (selected.center, Some(0))] {
            let (message, _, _) = <GalleryProgram as shader::Program<GalleryMessage>>::update(
                &program,
                &mut Default::default(),
                &event,
                bounds,
                mouse::Cursor::Available(Point::new(point[0], point[1])),
            )
            .unwrap()
            .into_inner();
            match (message, expected) {
                (Some(GalleryMessage::Pointer { index: None, .. }), None) => {}
                (
                    Some(GalleryMessage::Pointer {
                        index: Some(actual),
                        ..
                    }),
                    Some(expected),
                ) => {
                    assert_eq!(actual, expected)
                }
                other => panic!("wrong clipped hit: {other:?}"),
            }
        }
    }

    #[test]
    fn wall_masonry_and_justified_use_catalog_aspect_ratios() {
        let aspects = [4.0 / 3.0, 16.0 / 9.0, 3.0 / 4.0, 1.0];
        for layout in [WallLayout::Masonry, WallLayout::Justified] {
            let donor_layout = match layout {
                WallLayout::Masonry => DonorGridLayout::Masonry,
                WallLayout::Justified => DonorGridLayout::Justified,
                _ => unreachable!(),
            };
            let donor_params = DonorGridParams {
                layout: donor_layout,
                ..Default::default()
            };
            let expected = donor_layout::grid_placements(aspects.len(), &donor_params, &aspects);
            let cards = layout_wall_with_aspects(
                aspects.len(),
                2,
                [1920.0, 1080.0],
                WallParams {
                    layout,
                    ..Default::default()
                },
                &aspects,
            );
            for (index, expected_aspect) in aspects.iter().enumerate() {
                let card = cards.iter().find(|card| card.index == index).unwrap();
                let placement = expected
                    .iter()
                    .find(|placement| placement.index == index)
                    .unwrap();
                let focus = 1.0; // No engagement: source selected scale is not applied.
                assert_eq!(
                    card.size,
                    [placement.width * focus, placement.height * focus]
                );
                assert_eq!(card.poster_aspect_ratio, *expected_aspect);
            }
        }
    }

    #[test]
    fn wall_camera_clamps_to_catalog_content_bounds() {
        let make = |scroll_offset| {
            layout_wall(
                120,
                0,
                [1920.0, 1080.0],
                WallParams {
                    scroll_offset,
                    ..Default::default()
                },
            )
        };
        let far = make(1_000_000.0);
        let farther = make(100_000_000.0);

        assert!(!far.is_empty());
        assert_eq!(
            far.iter()
                .map(|card| (card.index, card.center))
                .collect::<Vec<_>>(),
            farther
                .iter()
                .map(|card| (card.index, card.center))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn current_wall_camera_and_ordinary_transient_motion_move_visible_cards() {
        let settled = GalleryLayoutParams::default();
        let moving = GalleryLayoutParams {
            depth: settled.depth,
            depth_camera: settled.depth_camera,
            slices: SliceParams {
                motion: 0.75,
                ..settled.slices
            },
            geometric: GeometricParams {
                motion: 0.75,
                ..settled.geometric
            },
            wall: WallParams {
                motion: 0.75,
                ..settled.wall
            },
        };
        for mode in [
            GalleryMode::Slices,
            GalleryMode::Geometric,
            GalleryMode::Wall,
        ] {
            let mut rest = settled;
            let mut current = moving;
            if mode == GalleryMode::Wall {
                // Wall's persistent content camera replaces selected-derived displacement.
                rest.wall.scroll_offset = 300.0;
                current.wall.scroll_offset = 400.0;
                current.wall.motion = 0.0;
            }
            let at_rest = layout_gallery(mode, 60, 20, [1600.0, 900.0], rest);
            let in_motion = layout_gallery(mode, 60, 20, [1600.0, 900.0], current);
            let rest_selected = at_rest.iter().find(|card| card.selected).unwrap();
            let moving_selected = in_motion.iter().find(|card| card.selected).unwrap();

            assert_ne!(rest_selected.center, moving_selected.center, "{mode:?}");
        }
    }

    #[test]
    fn detail_preserves_gallery_card_geometry_and_neighbors() {
        for mode in [
            GalleryMode::Slices,
            GalleryMode::Geometric,
            GalleryMode::Wall,
        ] {
            let browse_cards = layout_gallery(
                mode,
                60,
                20,
                [1600.0, 900.0],
                GalleryLayoutParams::default(),
            );
            let detail_cards = layout_gallery(
                mode,
                60,
                20,
                [1600.0, 900.0],
                GalleryLayoutParams {
                    ..Default::default()
                },
            );
            assert_eq!(detail_cards.len(), browse_cards.len(), "{mode:?}");
            for detail_card in &detail_cards {
                let browse_card = browse_cards
                    .iter()
                    .find(|card| card.index == detail_card.index)
                    .expect("detail card must retain its gallery identity");
                assert_eq!(detail_card.center, browse_card.center, "{mode:?}");
                assert_eq!(detail_card.size, browse_card.size, "{mode:?}");
                assert_eq!(detail_card.opacity, browse_card.opacity, "{mode:?}");
                assert_eq!(detail_card.rotation, browse_card.rotation, "{mode:?}");
                assert_eq!(
                    detail_card.skew_distance, browse_card.skew_distance,
                    "{mode:?}"
                );
                assert_eq!(
                    detail_card.edge_tilt_distance, browse_card.edge_tilt_distance,
                    "{mode:?}"
                );
            }
            assert!(detail_cards
                .iter()
                .any(|card| !card.selected && card.opacity > 0.0));
        }
    }

    #[test]
    fn slices_detail_keeps_card_geometry_at_midpoint() {
        let browse_cards = layout_gallery(
            GalleryMode::Slices,
            20,
            10,
            [1000.0, 800.0],
            GalleryLayoutParams::default(),
        );
        let detail_cards = layout_gallery(
            GalleryMode::Slices,
            20,
            10,
            [1000.0, 800.0],
            GalleryLayoutParams::default(),
        );
        let browse_selected = browse_cards.iter().find(|card| card.selected).unwrap();
        let detail_selected = detail_cards.iter().find(|card| card.selected).unwrap();
        assert_eq!(detail_selected.center, browse_selected.center);
        assert_eq!(detail_selected.size, browse_selected.size);
        assert!(detail_cards
            .iter()
            .any(|card| !card.selected && card.opacity > 0.0));
    }

    #[test]
    fn geometric_hit_testing_rejects_the_square_corner_of_a_circle() {
        let card = SliceCard {
            triangle_direction: 0,
            index: 4,
            center: [100.0, 100.0],
            size: [100.0, 100.0],
            layer: None,
            selected: true,
            opacity: 1.0,
            skew_distance: 0.0,
            edge_tilt_distance: 0.0,
            rotation: 0.0,
            corner_radius: 0.0,
            shape: 1,
            poster_aspect_ratio: 1.0,
        };

        assert_eq!(hit_test(&[card], 149.0, 149.0), None);
        assert_eq!(hit_test(&[card], 100.0, 100.0), Some(4));
    }

    #[test]
    fn hit_testing_follows_card_rotation() {
        let card = SliceCard {
            triangle_direction: 0,
            index: 8,
            center: [100.0, 100.0],
            size: [120.0, 30.0],
            layer: None,
            selected: true,
            opacity: 1.0,
            skew_distance: 0.0,
            edge_tilt_distance: 0.0,
            rotation: std::f32::consts::FRAC_PI_2,
            corner_radius: 0.0,
            shape: 0,
            poster_aspect_ratio: 4.0 / 3.0,
        };

        assert_eq!(hit_test(&[card], 100.0, 150.0), Some(8));
        assert_eq!(hit_test(&[card], 150.0, 100.0), None);
    }

    #[test]
    fn gallery_canvas_publishes_a_captured_click_for_a_hit_card() {
        let tracing_was_enabled = apply_trace::enabled();
        apply_trace::set_enabled(false);
        let card = SliceCard {
            triangle_direction: 0,
            index: 4,
            center: [100.0, 100.0],
            size: [80.0, 80.0],
            layer: None,
            selected: true,
            opacity: 1.0,
            skew_distance: 0.0,
            edge_tilt_distance: 0.0,
            rotation: 0.0,
            corner_radius: 0.0,
            shape: 0,
            poster_aspect_ratio: 1.0,
        };
        let program = GalleryProgram {
            prewarm: None,
            poster_feedback: None,
            snapshot: Arc::new(GallerySnapshot {
                defer_near_restore: false,
                ordinary: None,
                wall_clip: None,
                depth: None,
                cards: vec![card],
                posters: Arc::new(Vec::new()),
                colors: GalleryColors::default(),
                scene: DonorSceneState {
                    stage: StageParams::default(),
                    camera: 0.0,
                    camera_target: 0.0,
                    camera_velocity: 0.0,
                    detail_progress: 0.0,
                    external_detail: false,
                    flip_options: Default::default(),
                    detail_source: None,
                    detail_card: None,
                    viewport: [200.0, 200.0],
                    time: 0.0,
                    visibility: 1.0,
                },
            }),
        };
        let event = Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left));
        let action = <GalleryProgram as shader::Program<GalleryMessage>>::update(
            &program,
            &mut Default::default(),
            &event,
            Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0)),
            mouse::Cursor::Available(Point::new(100.0, 100.0)),
        )
        .expect("gallery hit must publish an action");
        let (message, _, status) = action.into_inner();
        assert!(matches!(
            message,
            Some(GalleryMessage::Pointer {
                index: Some(4),
                sequence: None,
                ..
            })
        ));
        assert_eq!(status, iced::event::Status::Captured);

        apply_trace::set_enabled(true);
        let traced_action = <GalleryProgram as shader::Program<GalleryMessage>>::update(
            &program,
            &mut Default::default(),
            &event,
            Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0)),
            mouse::Cursor::Available(Point::new(100.0, 100.0)),
        )
        .expect("traced gallery hit must publish an action");
        let (traced_message, _, traced_status) = traced_action.into_inner();
        assert!(matches!(
            traced_message,
            Some(GalleryMessage::Pointer {
                index: Some(4),
                sequence: Some(_),
                ..
            })
        ));
        assert_eq!(traced_status, iced::event::Status::Captured);
        apply_trace::set_enabled(tracing_was_enabled);
        let mut state = iced::keyboard::Modifiers::default();
        let _ = <GalleryProgram as shader::Program<GalleryMessage>>::update(
            &program,
            &mut state,
            &Event::Keyboard(iced::keyboard::Event::ModifiersChanged(
                iced::keyboard::Modifiers::CTRL,
            )),
            Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0)),
            mouse::Cursor::Available(Point::new(100.0, 100.0)),
        );
        let (message, _, status) = <GalleryProgram as shader::Program<GalleryMessage>>::update(
            &program,
            &mut state,
            &Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)),
            Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0)),
            mouse::Cursor::Available(Point::new(100.0, 100.0)),
        )
        .unwrap()
        .into_inner();
        assert!(matches!(
            message,
            Some(GalleryMessage::Pointer {
                button: crate::candidate_state::binding_codec::MouseButton::Right,
                modifiers: crate::candidate_state::binding_codec::Mods { ctrl: true, .. },
                index: Some(4),
                ..
            })
        ));
        assert_eq!(status, iced::event::Status::Captured);
    }

    #[test]
    fn gallery_canvas_keeps_zero_wheel_input_unpublished() {
        let program = GalleryProgram {
            prewarm: None,
            poster_feedback: None,
            snapshot: Arc::new(GallerySnapshot {
                defer_near_restore: false,
                ordinary: None,
                wall_clip: None,
                depth: None,
                cards: Vec::new(),
                posters: Arc::new(Vec::new()),
                colors: GalleryColors::default(),
                scene: DonorSceneState {
                    stage: StageParams::default(),
                    camera: 0.0,
                    camera_target: 0.0,
                    camera_velocity: 0.0,
                    detail_progress: 0.0,
                    external_detail: false,
                    flip_options: Default::default(),
                    detail_source: None,
                    detail_card: None,
                    viewport: [200.0, 200.0],
                    time: 0.0,
                    visibility: 1.0,
                },
            }),
        };
        let event = Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0.0, y: 0.0 },
        });

        let action = <GalleryProgram as shader::Program<GalleryMessage>>::update(
            &program,
            &mut Default::default(),
            &event,
            Rectangle::new(Point::ORIGIN, Size::new(200.0, 200.0)),
            mouse::Cursor::Available(Point::new(100.0, 100.0)),
        );

        assert!(action.is_none());
    }

    #[test]
    fn donor_instance_abi_matches_pinned_scene_payload() {
        assert_eq!(std::mem::size_of::<InstanceRaw>(), 176);
        assert_eq!(std::mem::align_of::<InstanceRaw>(), 4);
    }

    #[test]
    fn manual_hex_colors_parse_without_panicking_on_invalid_input() {
        let fallback = [0.1, 0.2, 0.3];

        assert_eq!(
            parse_hex_rgb("#ff8000", fallback),
            [1.0, 128.0 / 255.0, 0.0]
        );
        assert_eq!(
            parse_hex_rgb("  A1b2C3  ", fallback),
            [
                0xa1 as f32 / 255.0,
                0xb2 as f32 / 255.0,
                0xc3 as f32 / 255.0
            ]
        );
        assert_eq!(
            parse_hex_rgb("\t#AaBbCc\n", fallback),
            [
                0xaa as f32 / 255.0,
                0xbb as f32 / 255.0,
                0xcc as f32 / 255.0
            ]
        );
        assert_eq!(parse_hex_rgb("not-a-color", fallback), fallback);

        for value in ["", "#", "#12", "#12345", "#1234567", "zzzzzz", "#12xz45"] {
            assert_eq!(parse_hex_rgb(value, fallback), fallback, "input: {value:?}");
        }
    }

    #[test]
    fn six_byte_unicode_color_input_falls_back_without_panicking() {
        let fallback = [0.1, 0.2, 0.3];

        assert_eq!(parse_hex_rgb("あabc", fallback), fallback);
    }

    #[test]
    fn decoded_poster_keeps_source_aspect_and_letterboxes_without_cropping() {
        let path = std::env::temp_dir().join(format!(
            "wallpaper-switcher-poster-aspect-{}-{}.png",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = image::RgbaImage::from_pixel(400, 200, image::Rgba([255, 0, 0, 255]));
        source.save(&path).unwrap();

        let (pixels, aspect) = decode_poster_pixels_with_aspect(&path).unwrap();
        assert_eq!(pixels.len(), (POSTER_WIDTH * POSTER_HEIGHT * 4) as usize);
        let center = ((POSTER_HEIGHT / 2 * POSTER_WIDTH + POSTER_WIDTH / 2) * 4) as usize;
        let corner = 0;
        assert_eq!(aspect, 2.0);
        assert_eq!(&pixels[center..center + 4], &[255, 0, 0, 255]);
        assert_eq!(&pixels[corner..corner + 4], &[30, 30, 46, 255]);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn p26_pooled_poster_matches_previous_triangle_and_matte_bytes() {
        let decoder = PosterDecoder::new(2);
        let [r, g, b] = POSTER_MATTE_RGB;
        for (width, height) in [
            (2, 3),
            (1, 811),
            (793, 1),
            (1371, 255),
            (913, 647),
            (389, 1025),
            (1024, 511),
            (512, 287),
            (512, 512),
            (8193, 21),
        ] {
            let source = image::RgbaImage::from_fn(width, height, |x, y| {
                image::Rgba([
                    (x * 73 + y * 151) as u8,
                    (x * 37 + y * 79) as u8,
                    (x * 13 + y * 59) as u8,
                    ((x / 7 + y / 11) * 51) as u8,
                ])
            });
            let fitted = image::DynamicImage::ImageRgba8(source.clone())
                .resize(
                    POSTER_WIDTH,
                    POSTER_HEIGHT,
                    image::imageops::FilterType::Triangle,
                )
                .to_rgba8();
            let mut expected = image::RgbaImage::from_pixel(
                POSTER_WIDTH,
                POSTER_HEIGHT,
                image::Rgba([r, g, b, 255]),
            );
            image::imageops::overlay(
                &mut expected,
                &fitted,
                ((POSTER_WIDTH - fitted.width()) / 2).into(),
                ((POSTER_HEIGHT - fitted.height()) / 2).into(),
            );
            let mut pooled = poster_buffer(Some(&decoder));
            let aspect = render_poster_into(&source, &mut pooled, &|| true);
            assert_eq!(aspect, Some(width as f32 / height as f32));
            assert!(
                &*pooled == expected.as_raw(),
                "resized poster bytes differ for {width}x{height}"
            );
        }
        assert_eq!(decoder.pool.counts(), (1, 1, 9));
    }

    #[test]
    fn p26_cancelled_resize_returns_canvas_to_pool() {
        let decoder = PosterDecoder::new(2);
        let source = image::RgbaImage::from_pixel(640, 480, image::Rgba([255, 0, 0, 255]));
        let polls = std::cell::Cell::new(0);
        let wanted = || {
            polls.set(polls.get() + 1);
            polls.get() < 4
        };
        let mut pooled = poster_buffer(Some(&decoder));
        assert_eq!(render_poster_into(&source, &mut pooled, &wanted), None);
        assert_eq!(polls.get(), 4);
        drop(pooled);
        assert_eq!(decoder.pool.counts(), (1, 1, 0));
        let mut pooled = poster_buffer(Some(&decoder));
        assert_eq!(
            render_poster_into(&source, &mut pooled, &|| true),
            Some(4.0 / 3.0)
        );
        let center = ((POSTER_HEIGHT / 2 * POSTER_WIDTH + POSTER_WIDTH / 2) * 4) as usize;
        assert_eq!(&pooled[center..center + 4], &[255, 0, 0, 255]);
    }
}

/// Internal D1 frame; no GalleryMode/Kind branch or application event consumer.
/// Identities borrow the exact catalog revision used to construct this frame.
pub(crate) struct DepthItem<'a> {
    pub id: &'a str,
    pub master_index: usize,
    pub source_aspect: Option<f32>,
}

#[derive(Debug, Clone)]
pub(crate) struct DepthFrameCard<'a> {
    pub id: &'a str,
    pub card: SliceCard,
    pub filtered_index: usize,
    pub radii: [f32; 4],
    pub parallax: f32,
    pub crop_uv: [f32; 4],
    pub hovered: bool,
}

pub(crate) struct DepthFrame<'a> {
    pub revision: u64,
    pub viewport: [f32; 2],
    pub camera: f64,
    pub fit: f32,
    pub candidate_range: Option<(usize, usize)>,
    pub cards: Vec<DepthFrameCard<'a>>,
}

impl<'a> DepthFrame<'a> {
    pub fn visible_ids(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.cards.iter().map(|card| card.id)
    }

    pub fn hit(&self, x: f32, y: f32) -> Option<&DepthFrameCard<'a>> {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x > self.viewport[0]
            || y > self.viewport[1]
        {
            return None;
        }
        self.cards.iter().rev().find(|entry| {
            donor_layout::depth_contains(
                entry.card.center,
                entry.card.size,
                entry.card.skew_distance,
                entry.radii,
                [x, y],
            )
        })
    }
}

pub(crate) fn depth_frame<'a>(
    items: &[DepthItem<'a>],
    revision: u64,
    selected: Option<&str>,
    hovered: Option<&str>,
    camera: f32,
    viewport: [f32; 2],
    composition: [f32; 2],
    params: donor_layout::DepthParams,
    entrance: f32,
) -> DepthFrame<'a> {
    depth_frame_for(
        items.len(),
        |index| {
            items.get(index).map(|item| DepthItem {
                id: item.id,
                master_index: item.master_index,
                source_aspect: item.source_aspect,
            })
        },
        revision,
        selected,
        hovered,
        camera,
        viewport,
        composition,
        params,
        entrance,
    )
}

/// Resolve only the donor's bounded candidate window rather than cloning all
/// catalog identities/aspects on every animation frame.
pub(crate) fn depth_frame_for<'a>(
    count: usize,
    mut item_at: impl FnMut(usize) -> Option<DepthItem<'a>>,
    revision: u64,
    selected: Option<&str>,
    hovered: Option<&str>,
    camera: f32,
    viewport: [f32; 2],
    composition: [f32; 2],
    params: donor_layout::DepthParams,
    entrance: f32,
) -> DepthFrame<'a> {
    let layout = donor_layout::depth_layout(count, camera, viewport, composition, params, entrance);
    let cards = layout
        .cards
        .into_iter()
        .filter_map(|placement| {
            let item = item_at(placement.filtered_index)?;
            let aspect = valid_aspect(item.source_aspect);
            Some(DepthFrameCard {
                id: item.id,
                card: SliceCard {
                    triangle_direction: 0,
                    index: item.master_index,
                    center: placement.center,
                    size: placement.size,
                    layer: None,
                    selected: selected == Some(item.id),
                    opacity: placement.opacity,
                    skew_distance: placement.skew,
                    edge_tilt_distance: 0.0,
                    rotation: 0.0,
                    corner_radius: placement.radii[0],
                    shape: 0,
                    poster_aspect_ratio: aspect,
                },
                filtered_index: placement.filtered_index,
                radii: placement.radii,
                parallax: placement.parallax,
                crop_uv: depth_crop_uv(aspect, placement.size, placement.parallax),
                hovered: hovered == Some(item.id),
            })
        })
        .collect();
    DepthFrame {
        revision,
        viewport,
        camera: layout.camera,
        fit: layout.fit,
        candidate_range: layout.candidate_range,
        cards,
    }
}

/// Apply positional zoom/pan within decoded content, then map to the square
/// letterboxed poster. Pointer coordinates are deliberately absent.
fn depth_crop_uv(aspect: f32, size: [f32; 2], parallax: f32) -> [f32; 4] {
    let [content_x, content_y, content_w, content_h] = if aspect >= 1.0 {
        [0.0, (1.0 - 1.0 / aspect) * 0.5, 1.0, 1.0 / aspect]
    } else {
        [(1.0 - aspect) * 0.5, 0.0, aspect, 1.0]
    };
    let cover = crate::donor_scene::poster_crop_uv(aspect, size);
    let nw = cover[2] / content_w / 1.4;
    let nh = cover[3] / content_h / 1.4;
    [
        content_x + (1.0 - nw) * (0.5 + parallax.clamp(-1.0, 1.0) * 0.5) * content_w,
        content_y + (1.0 - nh) * 0.5 * content_h,
        nw * content_w,
        nh * content_h,
    ]
}

#[cfg(test)]
mod depth_frame_tests {
    use super::*;
    use donor_layout::DepthParams;
    #[test]
    fn d1_frame_ties_identity_and_reverse_hits_without_selected_last() {
        let items: Vec<_> = ["f", "b", "q", "a", "x", "z", "r", "p"]
            .iter()
            .enumerate()
            .map(|(i, id)| DepthItem {
                id,
                master_index: 100 - i,
                source_aspect: None,
            })
            .collect();
        for (camera, front) in [(3.25, "a"), (3.5, "x")] {
            let frame = depth_frame(
                &items,
                71,
                Some("z"),
                Some("a"),
                camera,
                [1200.0, 900.0],
                [600.0, 1200.0],
                DepthParams {
                    spacing: 0.0,
                    ..DepthParams::default()
                },
                1.0,
            );
            assert_eq!(frame.revision, 71);
            assert_eq!(frame.camera, camera as f64);
            assert!(frame.fit > 0.0 && frame.fit <= 1.0);
            assert_eq!(frame.candidate_range, Some((0, 7)));
            assert_eq!(frame.hit(600.0, 450.0).unwrap().id, front);
            assert_eq!(frame.cards.last().unwrap().id, front);
            assert_eq!(
                frame.visible_ids().collect::<Vec<_>>(),
                frame.cards.iter().map(|x| x.id).collect::<Vec<_>>()
            );
            for entry in &frame.cards {
                assert_eq!(entry.card.index, 100 - entry.filtered_index);
                assert_eq!(entry.card.selected, entry.id == "z");
                assert_eq!(entry.hovered, entry.id == "a");
                assert!(entry.parallax.is_finite());
                assert_eq!(entry.card.corner_radius, entry.radii[0]);
                assert_eq!(
                    (
                        entry.card.rotation,
                        entry.card.edge_tilt_distance,
                        entry.card.shape
                    ),
                    (0.0, 0.0, 0)
                );
            }
        }
    }

    #[test]
    fn d1_frame_visible_underlying_rounded_and_scissor_hits() {
        let items: Vec<_> = ["a", "b", "c"]
            .iter()
            .enumerate()
            .map(|(i, id)| DepthItem {
                id,
                master_index: i,
                source_aspect: Some(1.0),
            })
            .collect();
        let frame = depth_frame(
            &items,
            1,
            Some("a"),
            None,
            1.0,
            [1200.0, 900.0],
            [600.0, 1200.0],
            DepthParams {
                width: 300.0,
                height: 300.0,
                spacing: 200.0,
                falloff: 0.0,
                corner_radius: 50.0,
                ..DepthParams::default()
            },
            1.0,
        );
        assert_eq!(frame.hit(500.0, 450.0).unwrap().id, "b"); // overlapping body is occluded
        assert_eq!(frame.hit(330.0, 450.0).unwrap().id, "a"); // visible side of underlying body
        assert_eq!(frame.hit(451.0, 301.0).unwrap().id, "a"); // front cutout reveals underlying body
        assert!(frame.hit(251.0, 301.0).is_none()); // outermost rounded cutout
        assert!(frame.hit(550.0, 301.0).is_some());
        let outside = depth_frame(
            &items,
            1,
            None,
            None,
            1.0,
            [50.0, 50.0],
            [-20.0, 1200.0],
            DepthParams {
                spacing: 0.0,
                ..DepthParams::default()
            },
            1.0,
        );
        assert!(outside.hit(-20.0, 25.0).is_none());
        let empty = depth_frame(
            &[],
            2,
            None,
            None,
            0.0,
            [1.0, 1.0],
            [0.5, 1.0],
            DepthParams::default(),
            1.0,
        );
        assert_eq!(empty.visible_ids().count(), 0);
        assert!(empty.hit(0.5, 0.5).is_none());
    }

    #[test]
    fn d1_crop_content_relative_bounds_fallback_and_pointer_independence() {
        for aspect in [0.25, 0.5, 1.0, 2.0, 4.0] {
            let (cx, cy, cw, ch) = if aspect >= 1.0 {
                (0.0, (1.0 - 1.0 / aspect) / 2.0, 1.0, 1.0 / aspect)
            } else {
                ((1.0 - aspect) / 2.0, 0.0, aspect, 1.0)
            };
            for size in [[100.0, 1600.0], [2400.0, 100.0], [280.0, 520.0]] {
                for pan in [-2.0, -1.0, -0.25, 0.0, 0.5, 1.0, 2.0] {
                    let crop = depth_crop_uv(aspect, size, pan);
                    assert!(crop[0] >= cx - 1e-6 && crop[1] >= cy - 1e-6);
                    assert!(
                        crop[0] + crop[2] <= cx + cw + 1e-6 && crop[1] + crop[3] <= cy + ch + 1e-6
                    );
                    assert!(crop[2] > 0.0 && crop[3] > 0.0);
                    // Independent cover fractions computed from aspect ratios.
                    let ratio = (size[0] / size[1]) / aspect;
                    assert!((crop[2] / cw - ratio.min(1.0) / 1.4).abs() < 1e-5);
                    assert!((crop[3] / ch - (1.0 / ratio).min(1.0) / 1.4).abs() < 1e-5);
                }
            }
        }
        let items = [DepthItem {
            id: "wide",
            master_index: 42,
            source_aspect: None,
        }];
        let frame = depth_frame(
            &items,
            7,
            None,
            None,
            0.0,
            [1200.0, 900.0],
            [600.0, 1200.0],
            DepthParams::default(),
            1.0,
        );
        let before = frame.cards[0].crop_uv;
        for point in [[0.0, 0.0], [600.0, 450.0], [1100.0, 800.0]] {
            let _ = frame.hit(point[0], point[1]);
        }
        assert_eq!(before, frame.cards[0].crop_uv);
        assert_eq!(
            frame.cards[0].card.poster_aspect_ratio,
            DEFAULT_POSTER_ASPECT
        );
        assert_eq!(
            before,
            depth_crop_uv(DEFAULT_POSTER_ASPECT, frame.cards[0].card.size, 0.0)
        );
    }

    #[test]
    fn d1_fading_edge_crop_continues_until_body_omission() {
        let ids: Vec<_> = (0..8).map(|i| i.to_string()).collect();
        let items: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| DepthItem {
                id,
                master_index: i,
                source_aspect: Some(0.5),
            })
            .collect();
        let make = |camera| {
            depth_frame(
                &items,
                1,
                None,
                None,
                camera,
                [1200.0, 900.0],
                [600.0, 1200.0],
                DepthParams::default(),
                1.0,
            )
        };
        let fading = make(3.25);
        let edge = fading.cards.iter().find(|x| x.id == "6").unwrap();
        assert_eq!(edge.card.opacity, 0.15625);
        assert!(edge.parallax > 0.9 && edge.parallax < 1.0);
        assert!(edge.crop_uv[0] > 0.25);
        let omitted = make(3.0);
        assert!(!omitted.visible_ids().any(|id| id == "6"));
        assert!(omitted.cards.iter().all(|card| card.card.opacity > 0.0001));
    }
}

/// One owned, revision-bound frame shared by the actual widget, instance path,
/// inline detail composition, and poster demand. Catalog indices are master IDs.
#[derive(Debug, Clone)]
pub(crate) struct DepthSnapshotCard {
    pub id: String,
    pub filtered_index: usize,
    pub card: SliceCard,
    pub crop_uv: [f32; 4],
    pub hovered: bool,
    pub selection_progress: f32,
    /// Captured shader-only visual state for outgoing bodies, never actions.
    pub visual_detail_progress: Option<f32>,
    pub retained_texture: Option<Poster>,
}
#[derive(Debug, Clone)]
pub(crate) struct DepthSnapshot {
    pub revision: u64,
    pub viewport: [f32; 2],
    pub selection_frame: bool,
    pub shadows: bool,
    pub entrance: f32,
    pub launch: crate::donor_scene::LaunchAnimation,
    pub detail: crate::donor_scene::DepthDetailPresentation,
    pub cards: Vec<DepthSnapshotCard>,
    /// Visual-only outgoing stream. Never consumed by hit/detail/chrome.
    pub outgoing: Vec<DepthSnapshotCard>,
    /// Upcoming current-window demand before the zero-width incoming cut;
    /// bounded alongside geometry, so hidden incoming bodies can preload.
    pub live_poster_indices: Vec<usize>,
    pub poster_anchor: usize,
}
/// Exact successful upload ownership is required before any Depth image body
/// may sample a layer. A failed/unavailable live upload uses the existing solid
/// placeholder; an unavailable outgoing image is omitted. Physical old atlas
/// bytes may remain, but no instance can sample them as a different identity.
fn missing_depth_live_cards(frame: &DepthSnapshot, available: impl Fn(u32) -> bool) -> usize {
    frame
        .cards
        .iter()
        .filter(|body| !body.card.layer.is_some_and(&available))
        .count()
}

fn depth_resident_instances(
    frame: &DepthSnapshot,
    wanted: &[Poster],
    resident: &[Poster],
    colors: &GalleryColors,
    scale: f32,
    scene: &DonorSceneState,
) -> Vec<InstanceRaw> {
    let available = |layer| {
        wanted
            .iter()
            .find(|p| p.layer == layer)
            .is_some_and(|wanted| resident.iter().any(|p| p.same_resource(wanted)))
    };
    crate::apply_trace::event_lazy("gallery-frame-residency", || {
        let missing_cards = missing_depth_live_cards(frame, &available);
        format!(
            "depth_model=true revision={} wanted={} resident={} missing_cards={} cards={} outgoing={}",
            frame.revision,
            wanted.len(),
            resident.len(),
            missing_cards,
            frame.cards.len(),
            frame.outgoing.len(),
        )
    });
    // Only texture bindings change; never compute a second geometry frame.
    let mut frame = frame.clone();
    for body in &mut frame.cards {
        if body.card.layer.is_some_and(|layer| !available(layer)) {
            body.card.layer = None;
        }
    }
    frame.outgoing.retain(|body| {
        body.retained_texture.as_ref().is_some_and(|exact| {
            body.card.layer == Some(exact.layer)
                && wanted.iter().any(|p| p.same_resource(exact))
                && resident.iter().any(|p| p.same_resource(exact))
        })
    });
    crate::donor_scene::depth_instances(&frame, colors, scale, scene)
}

/// Donor reel cut applied to the same logical geometry and content-relative
/// crop consumed by drawing, rounded/skewed input and inline detail. Widths
/// here are full widths (InstanceRaw uses half widths). A <0.5px half-width
/// body is omitted, matching the donor outgoing/body eligibility threshold.
pub(crate) fn depth_roll_cut(body: &mut DepthSnapshotCard, fraction: f32, incoming: bool) -> bool {
    let fraction = fraction.clamp(0.0, 1.0);
    let half_width = body.card.size[0] * 0.5;
    if incoming {
        body.card.center[0] += half_width * (1.0 - fraction);
        body.card.size[0] *= fraction;
        body.crop_uv[2] *= fraction;
    } else {
        body.card.center[0] -= half_width * fraction;
        body.card.size[0] *= 1.0 - fraction;
        body.crop_uv[0] += body.crop_uv[2] * fraction;
        body.crop_uv[2] *= 1.0 - fraction;
    }
    body.card.corner_radius = body
        .card
        .corner_radius
        .min(body.card.size[0] * 0.5)
        .min(body.card.size[1] * 0.5);
    body.card.size[0].is_finite()
        && body.card.size[0] >= 1.0
        && body.card.size[1].is_finite()
        && body.card.size[1] > 0.0
        && body.card.opacity > 0.0001
}

pub(crate) fn depth_roll_cell(center: [f32; 2]) -> (i32, i32) {
    (
        (center[0] * 0.5).round() as i32,
        (center[1] * 0.5).round() as i32,
    )
}

pub(crate) fn depth_roll_start(x: f32, viewport_width: f32) -> f32 {
    -0.6 * (x / viewport_width.max(1.0)).clamp(0.0, 1.0)
}

pub(crate) fn depth_roll_step(dt: f32, duration_ms: f32) -> f32 {
    if !dt.is_finite() || dt <= 0.0 {
        return 0.0;
    }
    dt.min(0.05) * 1000.0 / duration_ms.max(50.0)
}

pub(crate) fn depth_roll_fraction(phase: f32) -> f32 {
    let t = phase.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl DepthSnapshot {
    pub fn poster_indices(&self, filtered: &[usize]) -> Vec<usize> {
        let mut result: Vec<_> = self
            .live_poster_indices
            .iter()
            .copied()
            .take(MAX_POSTERS)
            .collect();
        if filtered.is_empty() {
            return result;
        }
        let center = self.poster_anchor.min(filtered.len() - 1);
        // Visible front-to-back priority, then nearest neighbors with the
        // donor's stable lower-index tie. Work and storage stay bounded by
        // the resident-layer budget, independently of the catalog size.
        let mut distance = 0;
        while result.len() < MAX_POSTERS && distance < filtered.len() {
            if let Some(left) = center.checked_sub(distance) {
                let index = filtered[left];
                if !result.contains(&index) {
                    result.push(index);
                }
            }
            if result.len() == MAX_POSTERS {
                break;
            }
            if distance > 0 {
                if let Some(right) = center
                    .checked_add(distance)
                    .filter(|right| *right < filtered.len())
                {
                    let index = filtered[right];
                    if !result.contains(&index) {
                        result.push(index);
                    }
                }
            }
            distance += 1;
        }
        result
    }
    pub fn from_frame(frame: DepthFrame<'_>) -> Self {
        Self {
            revision: frame.revision,
            viewport: frame.viewport,
            selection_frame: false,
            shadows: false,
            entrance: 1.0,
            launch: crate::donor_scene::LaunchAnimation::None,
            detail: crate::donor_scene::DepthDetailPresentation::default(),
            outgoing: Vec::new(),
            poster_anchor: frame.cards.last().map_or(0, |entry| entry.filtered_index),
            live_poster_indices: frame.cards.iter().rev().map(|e| e.card.index).collect(),
            cards: frame
                .cards
                .into_iter()
                .map(|entry| DepthSnapshotCard {
                    id: entry.id.to_owned(),
                    filtered_index: entry.filtered_index,
                    card: entry.card,
                    crop_uv: entry.crop_uv,
                    hovered: entry.hovered,
                    selection_progress: if entry.card.selected { 1.0 } else { 0.0 },
                    visual_detail_progress: None,
                    retained_texture: None,
                })
                .collect(),
        }
    }
    pub fn hit(&self, x: f32, y: f32) -> Option<&DepthSnapshotCard> {
        if !x.is_finite()
            || !y.is_finite()
            || x < 0.0
            || y < 0.0
            || x > self.viewport[0]
            || y > self.viewport[1]
        {
            return None;
        }
        self.cards.iter().rev().find(|entry| {
            donor_layout::depth_contains(
                entry.card.center,
                entry.card.size,
                entry.card.skew_distance,
                [entry.card.corner_radius; 4],
                [x, y],
            )
        })
    }
}

#[cfg(test)]
mod d3_tests {
    use super::*;
    #[test]
    fn d3_integrated_painter_instances_reverse_hits_and_intermediate_camera_demand() {
        let ids: Vec<_> = (0..1000).map(|i| format!("stable-{i}")).collect();
        let filtered: Vec<_> = (0..1000).map(|i| 999 - i).collect();
        let items: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| DepthItem {
                id,
                master_index: filtered[i],
                source_aspect: Some(if i % 2 == 0 { 0.5 } else { 2.0 }),
            })
            .collect();
        let mut camera = crate::upstream_slices_adapter::GalleryMotionState::absolute_camera(
            0.0, 999.0, 1000.0, 280.0, 280.0,
        );
        let mut params = donor_layout::DepthParams::default();
        params.spacing = 0.0;
        for fraction in [0.0, 0.25, 0.5, 0.75] {
            let frame = DepthSnapshot::from_frame(depth_frame(
                &items,
                19,
                Some(&ids[998]),
                None,
                400.0 + fraction,
                [1200.0, 900.0],
                [600.0, 1200.0],
                params,
                1.0,
            ));
            let scene = DonorSceneState {
                stage: donor_layout::StageParams::default(),
                camera: 400.0 + fraction,
                camera_target: 999.0,
                camera_velocity: 0.0,
                detail_progress: 0.0,
                external_detail: false,
                flip_options: Default::default(),
                detail_source: None,
                detail_card: None,
                viewport: [1200.0, 900.0],
                time: 0.0,
                visibility: 1.0,
            };
            let raw =
                crate::donor_scene::depth_instances(&frame, &GalleryColors::default(), 1.0, &scene);
            assert_eq!(raw.len(), frame.cards.len());
            for (entry, instance) in frame.cards.iter().zip(&raw) {
                assert_eq!(entry.crop_uv, instance.crop);
                assert_eq!(entry.card.center, instance.rect[..2]);
            }
            let hit = frame.hit(600.0, 450.0).unwrap();
            assert_eq!(hit.id, frame.cards.last().unwrap().id);
            assert_eq!(frame.poster_indices(&filtered)[0], hit.card.index);
        }
        let mut intermediate = false;
        for revision in 0..200 {
            let moving = camera.tick(0.016);
            let frame = DepthSnapshot::from_frame(depth_frame(
                &items,
                revision,
                Some(&ids[999]),
                None,
                camera.value(),
                [1200.0, 900.0],
                [600.0, 1200.0],
                donor_layout::DepthParams::default(),
                1.0,
            ));
            let demand = frame.poster_indices(&filtered);
            assert!(demand.len() <= MAX_POSTERS);
            for card in &frame.cards {
                assert!(demand.contains(&card.card.index));
            }
            if (100.0..900.0).contains(&camera.value()) {
                intermediate = true;
                assert!(demand.iter().any(|i| (100..900).contains(i)));
                assert!(!demand.contains(&0));
            }
            if revision == 18 {
                let before = (camera.x, camera.velocity);
                camera.retarget_absolute(0.0);
                assert_eq!((camera.x, camera.velocity), before);
            }
            if !moving {
                break;
            }
        }
        assert!(intermediate);
        for count in [0, 1, 2, 3, 5] {
            let frame = DepthSnapshot::from_frame(depth_frame(
                &items[..count],
                6,
                items[..count].first().map(|i| i.id),
                None,
                0.0,
                [1200.0, 900.0],
                [600.0, 1200.0],
                donor_layout::DepthParams::default(),
                1.0,
            ));
            assert_eq!(frame.cards.is_empty(), count == 0);
            assert!(frame.poster_indices(&filtered[..count]).len() <= count);
        }
    }
    #[test]
    fn d3_large_catalog_demand_matches_visible_prefix_and_nearest_oracle() {
        let filtered: Vec<_> = (0..100_000).map(|i| 100_000 - i).collect();
        let ids: Vec<_> = filtered.iter().map(|i| format!("item-{i}")).collect();
        let items: Vec<_> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| DepthItem {
                id,
                master_index: filtered[i],
                source_aspect: None,
            })
            .collect();
        for camera in [0.0, 0.25, 50_000.5, 99_999.0] {
            let frame = DepthSnapshot::from_frame(depth_frame(
                &items,
                1,
                Some(&ids[0]),
                None,
                camera,
                [1200.0, 900.0],
                [600.0, 1200.0],
                donor_layout::DepthParams::default(),
                1.0,
            ));
            let mut expected: Vec<_> = frame
                .cards
                .iter()
                .rev()
                .map(|entry| entry.card.index)
                .take(MAX_POSTERS)
                .collect();
            let center = frame
                .cards
                .last()
                .map(|entry| {
                    filtered
                        .iter()
                        .position(|index| *index == entry.card.index)
                        .unwrap()
                })
                .unwrap_or(0);
            let mut oracle: Vec<_> = filtered.iter().enumerate().collect();
            oracle.sort_by_key(|(position, _)| position.abs_diff(center));
            for (_, index) in oracle {
                if expected.len() == MAX_POSTERS {
                    break;
                }
                if !expected.contains(index) {
                    expected.push(*index);
                }
            }
            assert_eq!(frame.poster_indices(&filtered), expected);
            assert_eq!(expected.len(), MAX_POSTERS);
        }
    }

    #[test]
    fn d3_default_instance_frame_uses_depth_owned_style_and_crop() {
        let items = [DepthItem {
            id: "selected",
            master_index: 7,
            source_aspect: Some(0.5),
        }];
        let frame = DepthSnapshot::from_frame(depth_frame(
            &items,
            1,
            Some("selected"),
            None,
            0.0,
            [1200.0, 900.0],
            [600.0, 1200.0],
            donor_layout::DepthParams::default(),
            1.0,
        ));
        let scene = DonorSceneState {
            stage: donor_layout::StageParams::default(),
            camera: 0.0,
            camera_target: 0.0,
            camera_velocity: 0.0,
            detail_progress: 0.0,
            external_detail: false,
            flip_options: Default::default(),
            detail_source: None,
            detail_card: None,
            viewport: [1200.0, 900.0],
            time: 0.0,
            visibility: 1.0,
        };
        let raw =
            crate::donor_scene::depth_instances(&frame, &GalleryColors::default(), 1.0, &scene);
        assert_eq!(
            raw[0].params[1], 0.0,
            "Depth default must ignore the old global frame switch"
        );
        assert_eq!(raw[0].border[3], 0.0);
        assert_eq!(raw[0].crop, frame.cards[0].crop_uv);
        assert_eq!(raw[0].shape[2], 1.0);
    }
}

#[cfg(test)]
mod d4b_cut_tests {
    use super::*;

    #[test]
    fn d4b_actual_resident_binding_omits_unavailable_or_overwritten_old_layer() {
        let ids = [DepthItem {
            id: "exact",
            master_index: 7,
            source_aspect: Some(2.0),
        }];
        let mut frame = DepthSnapshot::from_frame(depth_frame(
            &ids,
            1,
            Some("exact"),
            None,
            0.0,
            [1200.0, 900.0],
            [600.0, 1200.0],
            Default::default(),
            1.0,
        ));
        let exact = video_placeholder_poster(std::path::Path::new("exact-old.png"), 3);
        frame.cards[0].card.layer = Some(3);
        let mut ghost = frame.cards[0].clone();
        ghost.retained_texture = Some(exact.clone());
        frame.outgoing = vec![ghost];
        let scene = DonorSceneState {
            stage: Default::default(),
            camera: 0.0,
            camera_target: 0.0,
            camera_velocity: 0.0,
            detail_progress: 0.0,
            external_detail: false,
            flip_options: Default::default(),
            detail_source: None,
            detail_card: None,
            viewport: frame.viewport,
            time: 0.0,
            visibility: 1.0,
        };
        let colors = GalleryColors::default();
        let raw = depth_resident_instances(&frame, &[exact.clone()], &[], &colors, 1.0, &scene);
        assert_eq!(
            raw.len(),
            1,
            "unavailable old image is omitted while live geometry remains"
        );
        assert_eq!(
            raw[0].params[3], 0.0,
            "live texture failure is a truthful solid placeholder"
        );
        let raw = depth_resident_instances(
            &frame,
            &[exact.clone()],
            &[exact.clone()],
            &colors,
            1.0,
            &scene,
        );
        assert_eq!(raw.len(), 2);
        assert!(raw
            .iter()
            .all(|body| body.misc[1] == 3 && body.params[3] == 1.0));
        // A different resource using the very same ID/path/layer cannot be an
        // exact old image, even if its upload succeeded in the shared atlas.
        let changed = video_placeholder_poster(std::path::Path::new("exact-old.png"), 3);
        assert!(!changed.same_resource(&exact));
        let raw =
            depth_resident_instances(&frame, &[changed.clone()], &[changed], &colors, 1.0, &scene);
        assert_eq!(
            raw.len(),
            1,
            "bare layer/path agreement never authorizes a wrong-image ghost"
        );
        assert_eq!(raw[0].rect[2], frame.cards[0].card.size[0] * 0.5);
    }

    #[test]
    fn p0_depth_uses_an_exact_far_resident_as_a_safe_fallback() {
        let items = [DepthItem {
            id: "selected",
            master_index: 3,
            source_aspect: Some(16.0 / 9.0),
        }];
        let mut frame = DepthSnapshot::from_frame(depth_frame(
            &items,
            1,
            Some("selected"),
            None,
            0.0,
            [1200.0, 900.0],
            [600.0, 1200.0],
            Default::default(),
            1.0,
        ));
        let poster = video_placeholder_poster(Path::new("depth-far-fallback.mp4"), 3);
        frame.cards[0].card.layer = Some(poster.layer);
        let scene = DonorSceneState {
            stage: Default::default(),
            camera: 0.0,
            camera_target: 0.0,
            camera_velocity: 0.0,
            detail_progress: 0.0,
            external_detail: false,
            flip_options: Default::default(),
            detail_source: None,
            detail_card: None,
            viewport: frame.viewport,
            time: 0.0,
            visibility: 1.0,
        };

        let raw = depth_resident_instances(
            &frame,
            std::slice::from_ref(&poster),
            std::slice::from_ref(&poster),
            &GalleryColors::default(),
            1.0,
            &scene,
        );

        assert_eq!(raw.len(), 1);
        assert_eq!(raw[0].misc[..2], [2, poster.layer]);
        assert_eq!(raw[0].params[3], 1.0);
        let mut unassigned = frame.cards[0].clone();
        unassigned.card.index = 4;
        unassigned.card.selected = false;
        unassigned.card.layer = None;
        let mut unavailable = unassigned.clone();
        unavailable.card.index = 5;
        unavailable.card.layer = Some(7);
        let mut stale_ghost = frame.cards[0].clone();
        stale_ghost.retained_texture = Some(video_placeholder_poster(&poster.source, poster.layer));
        frame.cards.extend([unassigned, unavailable]);
        frame.outgoing.push(stale_ghost);
        let absent = video_placeholder_poster(Path::new("absent.png"), 7);
        let wanted = [poster.clone(), absent];
        assert_eq!(
            missing_depth_live_cards(&frame, |layer| layer == poster.layer),
            2
        );
        let raw = depth_resident_instances(
            &frame,
            &wanted,
            std::slice::from_ref(&poster),
            &GalleryColors::default(),
            1.0,
            &scene,
        );
        assert_eq!(raw.len(), 3);
        assert_eq!(
            raw.iter().map(|body| body.params[3]).collect::<Vec<_>>(),
            [1.0, 0.0, 0.0]
        );
    }

    #[test]
    fn d4b_bounded_candidate_resolution_preserves_large_index_order() {
        let count = 1_000_000;
        let mut seen = Vec::new();
        let params = donor_layout::DepthParams {
            visible_count: 21,
            ..Default::default()
        };
        let f = depth_frame_for(
            count,
            |index| {
                seen.push(index);
                Some(DepthItem {
                    id: "same",
                    master_index: index,
                    source_aspect: None,
                })
            },
            1,
            None,
            None,
            123456.5,
            [1200.0, 900.0],
            [600.0, 1200.0],
            params,
            1.0,
        );
        assert!(seen.len() <= 24);
        assert!(seen.iter().all(|i| (123445..=123468).contains(i)));
        assert_eq!(
            seen,
            f.cards.iter().map(|c| c.card.index).collect::<Vec<_>>()
        );
        let last = f
            .cards
            .iter()
            .rev()
            .take(2)
            .map(|c| c.card.index)
            .collect::<Vec<_>>();
        assert_eq!(last, [123457, 123456]);
    }

    #[test]
    fn d4b_extreme_cut_does_not_leave_hits_outside_drawn_quad() {
        let ids = [DepthItem {
            id: "cut",
            master_index: 7,
            source_aspect: Some(2.0),
        }];
        let mut f = DepthSnapshot::from_frame(depth_frame(
            &ids,
            1,
            Some("cut"),
            None,
            0.0,
            [1200.0, 900.0],
            [600.0, 1200.0],
            donor_layout::DepthParams {
                skew: 100.0,
                ..Default::default()
            },
            1.0,
        ));
        assert!(depth_roll_cut(&mut f.cards[0], 0.01, true));
        let body = &f.cards[0].card;
        let p = [
            body.center[0] - body.skew_distance * 0.25,
            body.center[1] + body.size[1] * 0.25,
        ];
        assert!(p[0] < body.center[0] - body.size[0] * 0.5 - 2.0);
        assert!(f.hit(p[0],p[1]).is_none(),"an extreme narrow/skewed cut must not retain a target beyond the raster quad, away from AA");
    }

    #[test]
    fn d4b_shader_silhouette_oracle_matches_cut_hits_away_from_aa() {
        let ids = [DepthItem {
            id: "oracle",
            master_index: 7,
            source_aspect: Some(0.5),
        }];
        for skew in [-100.0, 0.0, 100.0] {
            for radius in [0.0, 45.0] {
                let base = DepthSnapshot::from_frame(depth_frame(
                    &ids,
                    1,
                    None,
                    None,
                    0.0,
                    [1200.0, 900.0],
                    [600.0, 1200.0],
                    donor_layout::DepthParams {
                        skew,
                        corner_radius: radius,
                        ..Default::default()
                    },
                    1.0,
                ));
                for fraction in [0.01, 0.1, 0.25, 0.5, 1.0] {
                    let mut f = base.clone();
                    assert!(depth_roll_cut(&mut f.cards[0], fraction, true));
                    let b = &f.cards[0].card;
                    let hw = b.size[0] * 0.5;
                    let hh = b.size[1] * 0.5;
                    let sx = b.skew_distance * 0.5;
                    let bx = (hw - sx.abs()).max(1.0);
                    let by = hh.max(1.0);
                    let r = b.corner_radius;
                    for ix in -12..=12 {
                        for iy in -12..=12 {
                            let x = ix as f32 * (hw + sx.abs() + 6.0) / 10.0;
                            let y = iy as f32 * (hh + 6.0) / 10.0;
                            // Independent production WGSL inverse and rounded SDF;
                            // shader's padded raster quad and viewport are separate clips.
                            let qx = x + sx * y / by;
                            let dx = qx.abs() - bx + r;
                            let dy = y.abs() - by + r;
                            let sdf = dx.max(dy).min(0.0) + dx.max(0.0).hypot(dy.max(0.0)) - r;
                            let clip = (x.abs() - hw).max(y.abs() - hh);
                            if sdf.abs() < 2.01 || clip.abs() < 2.01 {
                                continue;
                            }
                            let expected = sdf < 0.0 && clip < 0.0;
                            assert_eq!(f.hit(b.center[0]+x,b.center[1]+y).is_some(),expected,
                            "skew={skew} radius={radius} fraction={fraction} x={x} y={y} sdf={sdf} clip={clip}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn d4b_independent_cut_timing_crop_and_rounded_skew_oracle() {
        for aspect in [0.25, 0.5, 2.0, 4.0] {
            let ids = [DepthItem {
                id: "oracle",
                master_index: 7,
                source_aspect: Some(aspect),
            }];
            let params = donor_layout::DepthParams {
                corner_radius: 50.0,
                skew: 30.0,
                ..Default::default()
            };
            let base = DepthSnapshot::from_frame(depth_frame(
                &ids,
                9,
                Some("oracle"),
                None,
                0.0,
                [1200.0, 900.0],
                [600.0, 1200.0],
                params,
                1.0,
            ))
            .cards
            .remove(0);
            for phase in [-0.6, 0.0, 0.25, 0.5, 0.75, 1.0, 2.0] {
                let t: f32 = phase;
                let t = t.clamp(0.0, 1.0);
                let f = t * t * (3.0 - 2.0 * t);
                assert_eq!(depth_roll_fraction(phase), f);
                for incoming in [false, true] {
                    let mut cut = base.clone();
                    let included = depth_roll_cut(&mut cut, f, incoming);
                    let remaining = if incoming { f } else { 1.0 - f };
                    assert!((cut.card.size[0] - base.card.size[0] * remaining).abs() < 1e-4);
                    let offset = if incoming { 1.0 - f } else { -f };
                    assert!(
                        (cut.card.center[0]
                            - base.card.center[0]
                            - base.card.size[0] * 0.5 * offset)
                            .abs()
                            < 1e-4
                    );
                    assert!((cut.crop_uv[2] - base.crop_uv[2] * remaining).abs() < 1e-6);
                    assert!(
                        (cut.crop_uv[0]
                            - base.crop_uv[0]
                            - if incoming { 0.0 } else { base.crop_uv[2] * f })
                        .abs()
                            < 1e-6
                    );
                    assert_eq!(included, cut.card.size[0] >= 1.0);
                    assert_eq!(cut.card.skew_distance, base.card.skew_distance);
                    assert!(cut.card.corner_radius <= cut.card.size[0] * 0.5);
                    assert!(cut.crop_uv[0] >= base.crop_uv[0] - 1e-6);
                    assert!(
                        cut.crop_uv[0] + cut.crop_uv[2] <= base.crop_uv[0] + base.crop_uv[2] + 1e-6
                    );
                }
            }
        }
        for x in [-1.0, 0.0, 300.0, 600.0, 1200.0, 2400.0] {
            assert_eq!(
                depth_roll_start(x, 1200.0),
                -0.6 * (x / 1200.0).clamp(0.0, 1.0)
            );
        }
        for duration in [0.0, 1.0, 50.0, 180.0, 250.0, 450.0] {
            assert_eq!(depth_roll_step(1.0, duration), 50.0 / duration.max(50.0));
            assert_eq!(depth_roll_step(0.025, duration), 25.0 / duration.max(50.0));
        }
        assert_eq!(depth_roll_step(f32::NAN, 450.0), 0.0);
    }
}

#[cfg(test)]
mod w4_cache_writer_tests {
    use super::*;

    #[test]
    fn canonical_cache_hit_preserves_existing_gpu_resource_identity() {
        let (profile, root) = crate::poster_worker::cache::tests::fixture();
        let source = profile.join("canonical-hit.png");
        image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255]))
            .save(&source)
            .unwrap();
        let cache = PosterCache::open(&profile, &root, true).unwrap();
        let stamp = Fingerprint::capture(&source).unwrap();
        let first =
            decode_poster_at(&source, 0, &cache, 4.0 / 3.0, stamp.clone(), None, &|| true).unwrap();
        let second =
            decode_poster_at(&source, 0, &cache, 4.0 / 3.0, stamp, None, &|| true).unwrap();

        assert!(first.same_resource(&second));
    }

    #[test]
    fn p38_changed_source_before_publication_leaves_no_cache_and_retries_fresh() {
        use std::cell::Cell;
        use std::hash::{Hash, Hasher};
        let (profile, root) = crate::poster_worker::cache::tests::fixture();
        let source = profile.join("changing.png");
        image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255]))
            .save(&source)
            .unwrap();
        let stamp = Fingerprint::capture(&source).unwrap();
        let expected = poster_cache_path_for_stamp(&source, &root, &stamp);
        // Preserve the v3 key's exact field types/order, including Option<SystemTime>.
        let mut legacy = std::collections::hash_map::DefaultHasher::new();
        3u8.hash(&mut legacy);
        source
            .to_string_lossy()
            .replace('\\', "/")
            .to_ascii_lowercase()
            .hash(&mut legacy);
        source.metadata().unwrap().len().hash(&mut legacy);
        source.metadata().unwrap().modified().ok().hash(&mut legacy);
        POSTER_WIDTH.hash(&mut legacy);
        POSTER_HEIGHT.hash(&mut legacy);
        assert_eq!(
            expected,
            root.join(format!("poster-{:016x}.rgba", legacy.finish()))
        );
        let cache = PosterCache::open(&profile, &root, true).unwrap();
        let changed = Cell::new(false);
        let result = decode_poster_at(&source, 0, &cache, 4.0 / 3.0, stamp, None, &|| {
            if !changed.get()
                && std::fs::read_dir(&root).unwrap().any(|entry| {
                    entry
                        .unwrap()
                        .path()
                        .extension()
                        .is_some_and(|ext| ext == "tmp")
                })
            {
                let mut replacement = image::RgbaImage::new(8, 5);
                for (x, y, pixel) in replacement.enumerate_pixels_mut() {
                    *pixel = image::Rgba([(x * 31) as u8, (y * 43) as u8, 180, 255]);
                }
                replacement.save(&source).unwrap();
                changed.set(true);
            }
            true
        });
        assert!(changed.get(), "mutation must occur after stage write");
        assert!(
            matches!(result, Err(PosterDecodeError::SourceChanged)),
            "changed source must be discarded"
        );
        assert!(!expected.exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
        let fresh = Fingerprint::capture(&source).unwrap();
        let current = poster_cache_path(&source, &root).unwrap();
        assert_ne!(current, expected);
        let poster =
            decode_poster_at(&source, 0, &cache, 8.0 / 5.0, fresh, None, &|| true).unwrap();
        assert_eq!(poster.aspect_ratio, 8.0 / 5.0);
        let mut pixels = vec![0; POSTER_BYTES];
        assert!(cache.read_into(&current, &mut pixels).is_some());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 1);
    }

    #[test]
    fn w4_regeneration_preserves_existing_temp_hardlink_unknown_destination_and_refused_root() {
        let (profile, root) = crate::poster_worker::cache::tests::fixture();
        let _guard = crate::poster_worker::cache::RootGuard::open(&profile, &root, true)
            .unwrap()
            .unwrap();
        let destination = root.join("poster-0123456789abcdef.rgba");
        let recovery = profile.join("recovery.sentinel");
        std::fs::write(&recovery, b"must survive").unwrap();
        let temporary = destination.with_extension("rgba.1.tmp");
        std::fs::hard_link(&recovery, &temporary).unwrap();
        publish_poster_cache(&destination, &[], b"new pixels", 1, &|| true);
        assert!(!destination.exists());
        assert_eq!(std::fs::read(&recovery).unwrap(), b"must survive");
        assert!(temporary.exists());
        std::fs::write(&destination, b"unknown legacy format").unwrap();
        publish_poster_cache(&destination, &[], b"new pixels", 2, &|| true);
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"unknown legacy format"
        );
        assert!(!destination.with_extension("rgba.2.tmp").exists());
        let fresh = root.join("poster-1111111111111111.rgba");
        let cancelled_stage = fresh.with_extension("rgba.4.tmp");
        publish_poster_cache(&fresh, &[], b"cancelled pixels", 4, &|| {
            assert_eq!(
                std::fs::read(&cancelled_stage).unwrap(),
                b"cancelled pixels"
            );
            false
        });
        assert!(!fresh.exists());
        assert!(!cancelled_stage.exists());
        publish_poster_cache(&fresh, &[], b"fresh pixels", 3, &|| true);
        assert_eq!(std::fs::read(&fresh).unwrap(), b"fresh pixels");
        // A supplied root outside the actual current Config owner cannot receive cache writes.
        let source = profile.join("owned.png");
        image::RgbaImage::new(2, 2).save(&source).unwrap();
        let uncached =
            decode_poster_cached(&source, 0, &WriteBatch::new(&root), None, &|| true).unwrap();
        assert!(matches!(
            uncached.recovery,
            PosterRecovery::Image {
                cache_path: None,
                ..
            }
        ));
        assert_eq!(
            std::fs::read(&destination).unwrap(),
            b"unknown legacy format"
        );
    }
}
#[cfg(test)]
#[path = "gpu_gallery_atlas_tests.rs"]
mod atlas_lifecycle_tests;

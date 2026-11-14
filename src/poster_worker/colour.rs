//! One small request in the existing worker; the captured still is the only input.
use crate::library::{
    color_metadata::{self, ColorMetadata},
    media_still::MediaStill,
};
use serde::{Deserialize, Serialize};
use std::{
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

pub const MAX_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 32 * 1024 * 1024;
pub const CACHE_LIMIT: u64 = 8192;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub still: MediaStill,
    pub metadata: ColorMetadata,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sorting: Option<crate::library::sort_metrics::SortMetrics>,
}
impl Entry {
    pub fn name(&self) -> String {
        let legacy = serde_json::to_vec(&(
            color_metadata::ALGORITHM_VERSION,
            &self.still.item_id,
            &self.metadata.fingerprint,
        ))
        .unwrap();
        let key = if self.sorting.is_some() {
            serde_json::to_vec(&(legacy.as_slice(), crate::library::sort_metrics::VERSION)).unwrap()
        } else {
            legacy
        };
        format!(
            "colour-{}.json",
            crate::library::color_metadata::hash_bytes(&key).to_ascii_lowercase()
        )
    }
    pub fn valid(&self) -> bool {
        self.metadata.valid()
            && self.sorting.as_ref().is_none_or(|m| m.valid())
            && !self.still.item_id.is_empty()
            && self.still.path.is_absolute()
            && super::super::library::media_still::supported(&self.still.path)
            && self.still.fingerprint.bytes <= MAX_BYTES as u64
    }
}
pub struct Job {
    pub generation: u64,
    pub epoch: u64,
    pub catalog_clock: Arc<AtomicU64>,
    pub clock: Arc<AtomicU64>,
    pub still: MediaStill,
    pub profile: PathBuf,
    pub refresh: bool,
    pub publish: Box<dyn FnOnce(Completion) + Send>,
    #[cfg(test)]
    pub held: Option<Box<dyn FnOnce() + Send>>,
}
#[derive(Debug, Clone)]
pub struct Completion {
    pub generation: u64,
    pub epoch: u64,
    pub still: MediaStill,
    pub result: Result<ColorMetadata, String>,
    pub sorting: Option<crate::library::sort_metrics::SortMetrics>,
    pub persistence: Result<(), String>,
    pub worker_failure: Option<super::WorkerFailure>,
}

pub fn owned_name(name: &str) -> bool {
    name.strip_prefix("colour-")
        .and_then(|n| n.strip_suffix(".json"))
        .is_some_and(|n| {
            n.len() == 64
                && n.bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
}
pub fn valid_bytes(name: &str, bytes: &[u8]) -> Option<Entry> {
    let e: Entry = serde_json::from_slice(bytes).ok()?;
    (e.valid() && e.name() == name).then_some(e)
}
fn cached(profile: &Path, name: &str) -> Option<Entry> {
    let root = profile.join("cache/colour-v1");
    let _guard = super::cache::RootGuard::open_colour(profile, &root, false).ok()??;
    let bytes = super::cache::read_colour_file(&root.join(name))?;
    valid_bytes(name, &bytes)
}
fn persist(profile: &Path, entry: &Entry, cancelled: &dyn Fn() -> bool) -> Result<(), String> {
    let root = profile.join("cache/colour-v1");
    let _guard = super::cache::RootGuard::open_colour(profile, &root, true)?
        .ok_or("Colour cache owner absent")?;
    let name = entry.name();
    let destination = root.join(&name);
    if let Some(old) = cached(profile, &name) {
        if old.still.item_id == entry.still.item_id && old.metadata == entry.metadata {
            return Ok(());
        }
    }
    if std::fs::symlink_metadata(&destination).is_ok() {
        return Err("Existing unvalidated colour destination retained".into());
    }
    let bytes = serde_json::to_vec(entry).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > CACHE_LIMIT {
        return Err("Colour cache entry exceeds format bound".into());
    }
    let temporary = crate::file_publication::unique_recovery_path(&destination, "colour-stage");
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)
        .map_err(|e| e.to_string())?;
    f.write_all(&bytes)
        .and_then(|_| f.sync_all())
        .map_err(|e| e.to_string())?;
    drop(f);
    if cancelled() {
        return Err("Cancelled before colour publication; staged evidence retained".into());
    }
    crate::file_publication::publish_staged_file_create_only(&temporary, &destination)
        .map_err(|e| e.to_string())
}

struct Analysis {
    colour: Result<ColorMetadata, String>,
    sorting: crate::library::sort_metrics::SortMetrics,
}

fn analyse(
    still: &MediaStill,
    profile: &Path,
    refresh: bool,
    cancelled: &dyn Fn() -> bool,
) -> Result<Analysis, String> {
    if cancelled() || !still.is_current() {
        return Err("Cancelled or stale still stamp".into());
    }
    if !crate::library::media_still::supported(&still.path) {
        return Err("Unsupported still input".into());
    }
    if still.fingerprint.bytes > MAX_BYTES as u64 {
        return Err("Still exceeds 64 MiB input bound".into());
    }
    let file = std::fs::File::open(&still.path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_BYTES || !still.is_current() || cancelled() {
        return Err("Cancelled, oversized or changed still".into());
    }
    let fingerprint = crate::library::color_metadata::hash_bytes(&bytes);
    if !refresh {
        let key = Entry {
            still: still.clone(),
            sorting: Some(crate::library::sort_metrics::SortMetrics {
                version: 1,
                hue: 99,
                saturation: 0,
                richness: 0,
            }),
            metadata: ColorMetadata {
                version: 1,
                fingerprint: fingerprint.clone(),
                dominant_rgb: [0; 3],
                hue: None,
                saturation: 0.0,
                brightness: 0.0,
                richness: 0.0,
            },
        };
        if let Some(e) = cached(profile, &key.name()) {
            if e.still.item_id == still.item_id
                && e.still.path == still.path
                && e.still.provenance == still.provenance
                && e.still.fingerprint == still.fingerprint
                && e.metadata.fingerprint == fingerprint
            {
                if let Some(sorting) = e.sorting {
                    return Ok(Analysis {
                        colour: Ok(e.metadata),
                        sorting,
                    });
                }
            }
        }
    }
    // Inspect dimensions before decode; ImageReader applies an allocation limit as well.
    let (w, h) = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?
        .into_dimensions()
        .map_err(|e| e.to_string())?;
    if w == 0 || h == 0 || w > 16384 || h > 16384 || w as u64 * h as u64 > MAX_PIXELS {
        return Err("Still exceeds 16384 edge / 32 Mpixel decode bound".into());
    }
    if cancelled() {
        return Err("Colour analysis cancelled".into());
    }
    let mut reader = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(16384);
    limits.max_image_height = Some(16384);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let decoded = reader
        .decode()
        .map_err(|e| format!("Still decode unavailable: {e}"))?;
    if cancelled() || !still.is_current() {
        return Err("Cancelled or changed still after decode".into());
    }
    let sample = decoded
        .resize(
            color_metadata::SAMPLE_EDGE,
            color_metadata::SAMPLE_EDGE,
            image::imageops::FilterType::Nearest,
        )
        .to_rgba8();
    Ok(Analysis {
        colour: color_metadata::analyse_sample(&sample, fingerprint),
        sorting: crate::library::sort_metrics::analyse(&decoded),
    })
}

pub(super) fn execute(
    job: Job,
    epoch: u64,
    current_epoch: &AtomicU64,
    shutdown: &dyn Fn() -> bool,
) {
    let clock = job.clock.clone();
    let generation = job.generation;
    #[cfg(test)]
    let mut job = job;
    let catalog_clock = job.catalog_clock.clone();
    let catalog_revision = job.still.catalog_revision;
    let cancelled = || {
        shutdown()
            || generation != clock.load(Ordering::Acquire)
            || catalog_revision != catalog_clock.load(Ordering::Acquire)
            || epoch != current_epoch.load(Ordering::Acquire)
    };
    let analysis = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        analyse(&job.still, &job.profile, job.refresh, &cancelled)
    }))
    .unwrap_or_else(|_| Err("Colour decoder failed unexpectedly".into()));
    let (mut result, mut sorting) = match analysis {
        Ok(a) => (a.colour, Some(a.sorting)),
        Err(e) => (Err(e), None),
    };
    #[cfg(test)]
    if let Some(held) = job.held.take() {
        held();
    }
    if cancelled() {
        return;
    }
    if !job.still.is_current() {
        result = Err("Still changed or disappeared; refresh to retry".into());
        sorting = None;
    }
    let persistence = result.as_ref().map_err(Clone::clone).and_then(|metadata| {
        persist(
            &job.profile,
            &Entry {
                still: job.still.clone(),
                metadata: metadata.clone(),
                sorting,
            },
            &cancelled,
        )
    });
    if !cancelled() {
        (job.publish)(Completion {
            generation,
            epoch,
            still: job.still,
            result,
            sorting,
            persistence,
            worker_failure: None,
        });
    }
}

pub(super) fn reject_queued(job: Job, failure: super::WorkerFailure) {
    if job.generation == job.clock.load(Ordering::Acquire)
        && job.still.catalog_revision == job.catalog_clock.load(Ordering::Acquire)
    {
        (job.publish)(Completion {
            generation: job.generation,
            epoch: job.epoch,
            still: job.still,
            sorting: None,
            result: Err(format!(
                "Colour worker unavailable: {failure}; Refresh colour information to retry"
            )),
            persistence: Err("Colour job did not run".into()),
            worker_failure: Some(failure),
        });
    }
}

#[cfg(test)]
#[path = "colour_tests.rs"]
mod tests;

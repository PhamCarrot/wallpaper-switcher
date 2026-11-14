use super::*;
use crate::{native_wallpaper, poster_worker, scanner};

pub(super) fn backdrop_dimensions(viewport_width: f32, viewport_height: f32) -> (u32, u32) {
    const MAX_EDGE: f32 = 1_600.0;
    let width = viewport_width.max(1.0);
    let height = viewport_height.max(1.0);
    let scale = (MAX_EDGE / width.max(height)).min(1.0);
    (
        (width * scale).round().max(1.0) as u32,
        (height * scale).round().max(1.0) as u32,
    )
}

fn decode_backdrop(path: &std::path::Path, width: u32, height: u32) -> Option<Vec<u8>> {
    let resized = image::open(path)
        .ok()?
        .resize_to_fill(width, height, image::imageops::FilterType::Triangle)
        .to_rgba8();
    Some(image::imageops::blur(&resized, 12.0).into_raw())
}

impl GpuCandidate {
    pub(super) fn request_poster_cache(&mut self, all: bool) {
        if self.cache_operation.is_some() || self.exit_requested {
            self.cache_status = "Cache operation Busy; wait for acknowledgment.".into();
            return;
        }
        if !self.ensure_preview_worker() {
            return;
        }
        let Some(tx) = WAKE_TX.get().and_then(|slot| slot.lock().ok()?.clone()) else {
            return;
        };
        let names = (!all).then(|| {
            self.wallpapers
                .iter()
                .filter_map(|item| {
                    let still = library::media_still::MediaStill::select(
                        item,
                        self.catalog_generation.load(Ordering::Acquire),
                    )?;
                    gpu_gallery::poster_cache_path(&still.path, &self.poster_cache_dir)?
                        .file_name()?
                        .to_str()
                        .map(str::to_owned)
                })
                .collect()
        });
        // Accepted exports own decoded OriginalSnapshot RGBA and hash, with no cache-file lease.
        // Same-lane execution also settles any running or queued export before enumeration.
        let request = poster_worker::cache::Request {
            profile: Config::config_dir(),
            root: self.poster_cache_dir.clone(),
            names,
            epoch: 0,
            colour: all,
            acknowledge: Box::new(move |report| {
                let _ = tx.unbounded_send(Wake::CacheAcknowledged(report));
            }),
        };
        match self.poster_worker.as_ref().unwrap().request_cache(request) {
            Ok(epoch) => {
                self.cache_operation = Some(epoch);
                self.cancel_colours();
                if all {
                    self.colours.metadata.clear();
                    self.colours.entries.clear();
                }
                self.clear_posters();
                self.cache_status = "Waiting for still analysis/decode/export barrier…".into();
            }
            Err(e) => self.cache_status = format!("Cache operation refused: {e}"),
        }
    }

    pub(super) fn backdrop_source_paths(&self) -> Vec<std::path::PathBuf> {
        if self.state.appearance.presentation == BackgroundPresentation::LiveTransparentOverlay {
            return Vec::new();
        }
        let mut sources = Vec::new();
        let mut push_existing = |path: std::path::PathBuf| {
            if path.exists() && !sources.iter().any(|candidate| candidate == &path) {
                sources.push(path);
            }
        };
        let native_cache_dir = Config::cache_dir().join("native-images");
        for active_file in self.active_wallpaper_files() {
            if let Some(index) = scanner::find_active_wallpaper_index(
                &self.wallpapers,
                &active_file,
                &native_cache_dir,
            ) {
                let Some(wallpaper) = self.wallpapers.get(index) else {
                    continue;
                };
                if let Some(preview) = wallpaper.preview_path.as_ref().filter(|path| path.exists())
                {
                    push_existing(preview.clone());
                }
                push_existing(wallpaper.file_target.clone());
            }
            push_existing(std::path::PathBuf::from(active_file));
        }
        sources
    }

    pub(super) fn refresh_backdrop(&mut self) {
        if self.state.appearance.presentation == BackgroundPresentation::LiveTransparentOverlay {
            // Invalidate queued/in-flight wallpaper work as well as installed artwork.
            self.backdrop_generation.fetch_add(1, Ordering::AcqRel);
            self.backdrop = None;
            self.backdrop_source = None;
            return;
        }
        let sources = self.backdrop_source_paths();
        let source = sources.first().cloned();
        if source == self.backdrop_source && self.backdrop.is_some() {
            return;
        }
        let generation = self.backdrop_generation.fetch_add(1, Ordering::AcqRel) + 1;
        self.backdrop = None;
        self.backdrop_source.clone_from(&source);
        if sources.is_empty() {
            return;
        }
        let [viewport_width, viewport_height] = self
            .active_monitor_id
            .as_ref()
            .and_then(|active| {
                native_wallpaper::monitors()
                    .ok()?
                    .into_iter()
                    .find(|monitor| monitor.device_id.eq_ignore_ascii_case(active))
                    .map(|monitor| [monitor.rect.width() as f32, monitor.rect.height() as f32])
            })
            .unwrap_or(self.viewport);
        let (width, height) = backdrop_dimensions(viewport_width, viewport_height);
        let generation_clock = self.backdrop_generation.clone();
        let backdrop_tx = WAKE_TX.get().and_then(|slot| slot.lock().ok()?.clone());
        let Some(backdrop_tx) = backdrop_tx else {
            return;
        };
        if let Err(error) = std::thread::Builder::new()
            .name(format!("gpu-backdrop-{generation}"))
            .spawn(move || {
                let decoded = sources.iter().find_map(|source| {
                    if generation != generation_clock.load(Ordering::Acquire) {
                        return None;
                    }
                    decode_backdrop(source, width, height)
                });
                if generation != generation_clock.load(Ordering::Acquire) {
                    return;
                }
                let message = decoded.map_or(
                    Wake::BackdropLoaded {
                        generation,
                        width: 0,
                        height: 0,
                        rgba: Vec::new(),
                    },
                    |rgba| Wake::BackdropLoaded {
                        generation,
                        width,
                        height,
                        rgba,
                    },
                );
                let _ = backdrop_tx.unbounded_send(message);
            })
        {
            log_debug(&format!("Could not start backdrop preparation: {error}"));
        }
    }

    pub(super) fn ensure_preview_worker(&mut self) -> bool {
        if self
            .poster_worker
            .as_ref()
            .is_some_and(|w| w.unavailable_reason().is_some())
        {
            self.poster_worker = None;
        }
        if self.poster_worker.is_some() {
            return true;
        }
        let Some(poster_tx) = WAKE_TX.get().and_then(|slot| slot.lock().ok()?.clone()) else {
            self.poster_request_key = None;
            self.record_poster_worker_failure("wake channel is unavailable");
            return false;
        };
        let worker_generation = self.poster_generation.clone();
        let mailbox = self.poster_mailbox.clone();
        match poster_worker::PosterWorker::spawn(worker_generation, move |completion| {
            mailbox
                .publish(completion, || {
                    poster_tx.unbounded_send(Wake::PostersReady).is_ok()
                })
                .is_ok()
        }) {
            Ok(worker) => self.poster_worker = Some(worker),
            Err(error) => {
                self.poster_request_key = None;
                self.record_poster_worker_failure(error);
                return false;
            }
        }
        true
    }
}

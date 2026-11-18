use super::{cache, Fingerprint, PosterKind, Shared};
use crate::{apply_trace, gpu_gallery};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Instant,
};

const MAX_NEW_BYTES: u64 = 128 * 1024 * 1024;

pub(crate) struct Source {
    pub path: PathBuf,
    pub kind: PosterKind,
    pub stamp: Fingerprint,
}

pub(super) struct Pass {
    sources: VecDeque<Source>,
    clock: Arc<AtomicU64>,
    revision: u64,
    epoch: u64,
    published: u64,
}

impl Pass {
    pub(super) fn new(
        sources: Vec<Source>,
        clock: Arc<AtomicU64>,
        revision: u64,
        epoch: u64,
    ) -> Self {
        Self {
            sources: sources.into(),
            clock,
            revision,
            epoch,
            published: 0,
        }
    }

    fn current(&self, shared: &Shared) -> bool {
        !shared.shutdown.load(Ordering::Acquire)
            && shared.running.load(Ordering::Acquire)
            && self.clock.load(Ordering::Acquire) == self.revision
            && shared.cache_epoch.load(Ordering::Acquire) == self.epoch
    }

    pub(super) fn run(
        mut self,
        shared: &Shared,
        cache: &cache::Maintenance,
        budget: u64,
        idle_since: Instant,
    ) {
        let quiet = || {
            shared.queue.lock().is_ok_and(|queue| {
                queue.poster_idle_since == Some(idle_since)
                    && queue.colour.is_none()
                    && !queue.cache_busy
            })
        };
        let mut remaining = cache
            .headroom(budget, &|| self.current(shared) && quiet())
            .min(MAX_NEW_BYTES.saturating_sub(self.published));
        let started_with = remaining;
        let pixels = gpu_gallery::POSTER_BYTES as u64;
        if remaining >= pixels && self.current(shared) && quiet() {
            if let Some(owner) = cache.open_posters() {
                let mut rgba = vec![0; gpu_gallery::POSTER_BYTES];
                while let Some(source) = self.sources.front() {
                    let wanted = || self.current(shared) && quiet();
                    let size = pixels + 4 * u64::from(source.kind == PosterKind::Video);
                    if remaining < pixels || !wanted() {
                        break;
                    }
                    #[cfg(test)]
                    {
                        let hook = shared.before_pregeneration.lock().unwrap().take();
                        if let Some(hook) = hook {
                            hook();
                        }
                    }
                    if size <= remaining
                        && gpu_gallery::pregenerate_poster(
                            &source.path,
                            source.kind,
                            &source.stamp,
                            &owner,
                            size,
                            &mut rgba,
                            &wanted,
                        )
                    {
                        remaining -= size;
                        self.published += size;
                        apply_trace::event_lazy("poster-pregenerated", || {
                            format!(
                                "revision={} source={:?} bytes={size}",
                                self.revision, source.path
                            )
                        });
                    } else if !wanted() {
                        // Retry this candidate with a new headroom measurement after foreground work.
                        break;
                    }
                    self.sources.pop_front();
                }
            }
        }
        // The cache guard and scratch have gone away before any other lane can
        // clear the root. Keep only the newest revision's interrupted input.
        if !self.sources.is_empty() && self.current(shared) && !quiet() {
            if let Ok(mut queue) = shared.queue.lock() {
                if self.current(shared) && queue.pregeneration.is_none() {
                    queue.pregeneration = Some(self);
                }
            }
            return;
        }
        apply_trace::event_lazy("poster-pregeneration-settled", || {
            format!(
                "revision={} input_remaining={} headroom_bytes={started_with} written_bytes={} current={}",
                self.revision, self.sources.len(), self.published, self.current(shared)
            )
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::poster_worker::{decode_source, PosterWorker};
    use std::sync::mpsc;
    use std::time::Duration;

    const WAIT: Duration = Duration::from_secs(10);

    fn source(profile: &std::path::Path, name: &str) -> Source {
        let path = profile.join(name);
        image::RgbaImage::from_pixel(12, 9, image::Rgba([5, 150, 80, 255]))
            .save(&path)
            .unwrap();
        Source {
            stamp: Fingerprint::capture(&path).unwrap(),
            path,
            kind: PosterKind::Still,
        }
    }

    fn hold_next(worker: &PosterWorker) -> (mpsc::Receiver<()>, mpsc::Sender<()>) {
        let (entered, wait) = mpsc::channel();
        let (release, held) = mpsc::channel();
        *worker.shared.before_pregeneration.lock().unwrap() = Some(Box::new(move || {
            entered.send(()).unwrap();
            held.recv_timeout(WAIT).unwrap();
        }));
        (wait, release)
    }

    fn wait_for_poster(path: &std::path::Path) {
        let deadline = Instant::now() + WAIT;
        while !path.exists() {
            assert!(Instant::now() < deadline, "missing poster {path:?}");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn t7_foreground_and_restore_run_during_pregeneration_and_clear_waits() {
        use crate::poster_worker::{PosterBatch, PosterSource};
        let (profile, root) = cache::tests::fixture();
        let generation = Arc::new(AtomicU64::new(1));
        let catalog = Arc::new(AtomicU64::new(1));
        let (done, completions) = mpsc::channel();
        let worker = PosterWorker::spawn_with_runner(
            std::thread::Builder::new(),
            generation,
            move |update| done.send(update).is_ok(),
            |_| {},
            decode_source,
            Some(cache::Maintenance::new(profile.clone(), Some(4), true)),
            1,
        )
        .unwrap();
        let background = source(&profile, "background.png");
        let cache_path = gpu_gallery::poster_cache_path(&background.path, &root).unwrap();
        let restore = gpu_gallery::decode_poster(&background.path, 0).unwrap();
        let (entered, release) = hold_next(&worker);
        worker.pregenerate(vec![background], Arc::clone(&catalog), 1);
        entered.recv_timeout(WAIT).unwrap();

        let batch = PosterBatch::new(
            1,
            vec![PosterSource {
                index: 0,
                layer: 0,
                item_id: "visible".into(),
                kind: PosterKind::Placeholder,
                still: None,
                video_stamp: None,
                path: profile.join("visible.pkg"),
            }],
            root.clone(),
        );
        worker.submit(batch).unwrap();
        let (redraw, restored) = mpsc::channel();
        worker.restore(
            &[restore.clone()],
            vec![(restore.clone(), gpu_gallery::PosterEncoding::NearRgbaMips)],
            move || redraw.send(()).unwrap(),
        );
        let first = completions.recv_timeout(WAIT).unwrap();
        assert_eq!(first.posters.len(), 1);
        assert!(completions.recv_timeout(WAIT).unwrap().finished);
        restored.recv_timeout(WAIT).unwrap();
        assert!(!cache_path.exists());
        release.send(()).unwrap();

        wait_for_poster(&cache_path);
        assert_eq!(
            cache::read_poster(&cache_path).unwrap().len(),
            gpu_gallery::POSTER_BYTES
        );
        assert!(
            completions.try_recv().is_err(),
            "disk-only work published a slot"
        );

        let current = source(&profile, "current.png");
        let queued = source(&profile, "queued.png");
        let (entered, release) = hold_next(&worker);
        worker.pregenerate(vec![current, queued], Arc::clone(&catalog), 1);
        entered.recv_timeout(WAIT).unwrap();
        let (ack, receipt) = mpsc::channel();
        let epoch = worker
            .request_cache(cache::Request {
                profile: profile.clone(),
                root: root.clone(),
                names: None,
                epoch: 0,
                colour: false,
                acknowledge: Box::new(move |report| ack.send(report).unwrap()),
            })
            .unwrap();
        assert!(receipt.recv_timeout(Duration::from_millis(100)).is_err());
        release.send(()).unwrap();
        let report = receipt.recv_timeout(WAIT).unwrap();
        assert_eq!(report.epoch, epoch);
        assert_eq!(report.deleted, 1);
        assert_eq!(report.failed, 0);
        assert!(report.error.is_none());
        assert!(!worker.cache_busy());
        let queue = worker.shared.queue.lock().unwrap();
        assert!(queue.pregeneration.is_none());
        assert!(!queue.background_active);
        drop(queue);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);

        let superseded = source(&profile, "superseded.png");
        let replacement = source(&profile, "replacement.png");
        let superseded_cache = gpu_gallery::poster_cache_path(&superseded.path, &root).unwrap();
        let replacement_cache = gpu_gallery::poster_cache_path(&replacement.path, &root).unwrap();
        let (entered, release) = hold_next(&worker);
        worker.pregenerate(vec![superseded], Arc::clone(&catalog), 1);
        entered.recv_timeout(WAIT).unwrap();
        catalog.store(2, Ordering::Release);
        worker.pregenerate(vec![replacement], catalog, 2);
        release.send(()).unwrap();
        wait_for_poster(&replacement_cache);
        assert!(!superseded_cache.exists());
    }

    #[test]
    fn t7_changed_stamp_and_cancelled_stage_leave_no_disk_record() {
        use std::cell::Cell;
        let (profile, root) = cache::tests::fixture();
        let source = source(&profile, "source.png");
        let cache = cache::PosterCache::open(&profile, &root, false).unwrap();
        let seen_stage = Cell::new(false);
        let mut pixels = vec![0; gpu_gallery::POSTER_BYTES];
        let published = gpu_gallery::pregenerate_poster(
            &source.path,
            source.kind,
            &source.stamp,
            &cache,
            pixels.len() as u64,
            &mut pixels,
            &|| {
                if std::fs::read_dir(&root).unwrap().next().is_some() {
                    seen_stage.set(true);
                    return false;
                }
                true
            },
        );
        assert!(seen_stage.get());
        assert!(!published);
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);

        let changed = Cell::new(false);
        assert!(!gpu_gallery::pregenerate_poster(
            &source.path,
            source.kind,
            &source.stamp,
            &cache,
            pixels.len() as u64,
            &mut pixels,
            &|| {
                if !changed.get() && std::fs::read_dir(&root).unwrap().next().is_some() {
                    std::fs::write(&source.path, b"replacement").unwrap();
                    changed.set(true);
                }
                true
            },
        ));
        assert!(changed.get());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0);
    }
}

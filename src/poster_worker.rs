#[path = "poster_worker/buffer_pool.rs"]
pub(crate) mod buffer_pool;
#[path = "poster_worker/cache.rs"]
pub mod cache;
#[path = "poster_worker/colour.rs"]
pub mod colour;
#[path = "poster_worker/pregeneration.rs"]
mod pregeneration;
#[path = "poster_worker/video.rs"]
pub(crate) mod video;

pub(crate) use pregeneration::Source as PregenerationSource;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::time::{Duration, Instant};
use std::{
    collections::{HashMap, HashSet, VecDeque},
    fmt,
};

use crate::gpu_gallery::{
    self, Poster, PosterDecodeError, PosterDecoder, PosterRestore, PosterTier,
};
use crate::library::media_still::Fingerprint;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum PosterKind {
    Still,
    Video,
    Placeholder,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosterSource {
    pub index: usize,
    pub layer: u32,
    pub item_id: String,
    pub(crate) kind: PosterKind,
    pub still: Option<crate::library::media_still::MediaStill>,
    pub(crate) video_stamp: Option<Fingerprint>,
    pub path: PathBuf,
}

impl PosterSource {
    pub(crate) fn fingerprint(&self) -> Option<&Fingerprint> {
        match self.kind {
            PosterKind::Still => matching_still(self).map(|still| &still.fingerprint),
            PosterKind::Video => self.video_stamp.as_ref(),
            PosterKind::Placeholder => None,
        }
    }

    pub(crate) fn video_is_current(&self) -> bool {
        self.kind != PosterKind::Video || Fingerprint::capture(&self.path) == self.video_stamp
    }
}

#[derive(Debug)]
pub struct PosterBatch {
    pub generation: u64,
    pub sources: Vec<PosterSource>,
    pub cache_dir: PathBuf,
    pub(crate) reusable: HashMap<u32, Poster>,
    pub(crate) visible: HashSet<usize>,
}

impl PosterBatch {
    pub fn new(generation: u64, sources: Vec<PosterSource>, cache_dir: PathBuf) -> Self {
        Self {
            generation,
            visible: sources.iter().map(|source| source.index).collect(),
            sources,
            cache_dir,
            reusable: Default::default(),
        }
    }
}

#[derive(Debug)]
pub struct PosterCompletion {
    pub generation: u64,
    pub cursor: usize,
    pub finished: bool,
    pub posters: Vec<DecodedPoster>,
    pub(crate) failures: Vec<DecodeFailed>,
}

#[derive(Debug)]
pub(crate) struct DecodeFailed {
    pub source: PosterSource,
    pub tiers: [PosterTier; 2],
    pub error: Arc<PosterDecodeError>,
}

impl DecodeFailed {
    fn new(source: &PosterSource, error: &Arc<PosterDecodeError>) -> Self {
        Self {
            source: source.clone(),
            // The shared still decode supplies both tiers.
            tiers: [PosterTier::Far, PosterTier::Near],
            error: Arc::clone(error),
        }
    }
}

/// One bounded delta and one wake marker cover all undrained results.
/// The cursor advances across drains until the generation changes.
#[derive(Debug, Default)]
pub struct PosterMailbox {
    state: Mutex<CompletionState>,
}

#[derive(Debug, Default)]
struct CompletionState {
    generation: u64,
    cursor: usize,
    finished: bool,
    expected: Vec<PosterSource>,
    completed: Option<PosterCompletion>,
    wake_pending: bool,
    capacity: usize,
}

impl PosterMailbox {
    pub fn expect(
        &self,
        generation: u64,
        expected: Vec<PosterSource>,
    ) -> Result<(), WorkerFailure> {
        self.expect_bounded(generation, expected, gpu_gallery::MAX_POSTERS)
    }

    pub fn expect_bounded(
        &self,
        generation: u64,
        expected: Vec<PosterSource>,
        capacity: usize,
    ) -> Result<(), WorkerFailure> {
        let capacity = capacity.clamp(gpu_gallery::MAX_POSTERS, gpu_gallery::MAX_ORDINARY_POSTERS);
        let mut state = self
            .state
            .lock()
            .map_err(|_| WorkerFailure::QueuePoisoned)?;
        state.generation = generation;
        state.cursor = 0;
        state.finished = false;
        state.expected = expected;
        state.capacity = capacity;
        state.completed = None;
        // Preserve an already queued wake: a new matching completion can reuse
        // it, and take() coordinates clearing that marker with producers.
        Ok(())
    }

    pub fn publish(
        &self,
        mut completed: PosterCompletion,
        notify: impl FnOnce() -> bool,
    ) -> Result<(), WorkerFailure> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WorkerFailure::QueuePoisoned)?;
        let incoming = completed
            .posters
            .iter()
            .map(|p| &p.source)
            .chain(completed.failures.iter().map(|f| &f.source));
        let collides = |left: &PosterSource, right: &PosterSource| {
            left.layer == right.layer || left.index == right.index
        };
        if completed.generation != state.generation
            || state.capacity == 0
            || state.finished
            || completed.cursor <= state.cursor
            || (!completed.finished && incoming.clone().next().is_none())
            || completed.posters.len() + completed.failures.len() > state.capacity
            || incoming.clone().enumerate().any(|(i, source)| {
                source.layer as usize >= state.capacity
                    || !state.expected.contains(source)
                    || incoming
                        .clone()
                        .take(i)
                        .any(|prior| collides(prior, source))
            })
            || completed.posters.iter().any(|decoded| {
                decoded.poster.source != decoded.source.path
                    || decoded.poster.layer != decoded.source.layer
            })
        {
            return Ok(());
        }
        let cursor = completed.cursor;
        let finished = completed.finished;
        if let Some(mut pending) = state.completed.take() {
            let replaced = |prior: &PosterSource| incoming.clone().any(|new| collides(prior, new));
            pending.posters.retain(|prior| !replaced(&prior.source));
            pending.failures.retain(|prior| !replaced(&prior.source));
            pending.posters.append(&mut completed.posters);
            completed.posters = pending.posters;
            pending.failures.append(&mut completed.failures);
            completed.failures = pending.failures;
        }
        state.completed = Some(completed);
        if !state.wake_pending {
            state.wake_pending = true;
            if !notify() {
                state.wake_pending = false;
                state.completed = None;
                return Err(WorkerFailure::CompletionChannelClosed);
            }
        }
        state.cursor = cursor;
        state.finished = finished;
        Ok(())
    }

    pub fn take(&self) -> Result<Option<PosterCompletion>, WorkerFailure> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WorkerFailure::QueuePoisoned)?;
        state.wake_pending = false;
        Ok(state.completed.take())
    }
}

#[cfg(test)]
mod d3_mailbox_tests {
    use super::*;
    fn source(id: &str) -> PosterSource {
        PosterSource {
            index: 7,
            layer: 0,
            item_id: id.into(),
            kind: crate::poster_worker::PosterKind::Video,
            video_stamp: None,
            still: None,
            path: PathBuf::from(id),
        }
    }
    fn completion(generation: u64, id: &str) -> PosterCompletion {
        let source = source(id);
        PosterCompletion {
            generation,
            cursor: 1,
            finished: false,
            failures: Vec::new(),
            posters: vec![DecodedPoster {
                poster: gpu_gallery::video_placeholder_poster(&source.path, source.layer),
                source,
            }],
        }
    }
    #[test]
    fn d3_stalled_gui_retains_latest_only_and_stale_identity_cannot_replace_it() {
        let mailbox = PosterMailbox::default();
        let mut wakes = 0;
        mailbox.expect(9, vec![source("current")]).unwrap();
        for _ in 0..50 {
            mailbox
                .publish(completion(9, "current"), || {
                    wakes += 1;
                    true
                })
                .unwrap();
            assert!(
                mailbox
                    .state
                    .lock()
                    .unwrap()
                    .completed
                    .as_ref()
                    .unwrap()
                    .posters
                    .len()
                    <= gpu_gallery::MAX_POSTERS
            );
        }
        mailbox
            .publish(completion(8, "current"), || {
                panic!("stale generation woke GUI")
            })
            .unwrap();
        let mut wrong = completion(9, "other");
        wrong.cursor = 2;
        mailbox
            .publish(wrong, || panic!("wrong ID/path woke GUI"))
            .unwrap();
        let mut wrong = completion(9, "current");
        wrong.cursor = 2;
        wrong.posters[0].source.kind = crate::poster_worker::PosterKind::Still;
        mailbox.publish(wrong, || panic!()).unwrap();
        let mut wrong = completion(9, "current");
        wrong.cursor = 2;
        wrong.posters[0].source.index = 0;
        mailbox.publish(wrong, || panic!()).unwrap();
        let mut wrong = completion(9, "current");
        wrong.cursor = 2;
        wrong.posters[0].source.layer = 1;
        mailbox.publish(wrong, || panic!()).unwrap();
        assert_eq!(wakes, 1);
        let result = mailbox.take().unwrap().unwrap();
        assert_eq!(result.generation, 9);
        assert_eq!(result.posters[0].source.item_id, "current");
        assert!(mailbox.take().unwrap().is_none());
    }
    #[test]
    fn d3_mailbox_take_clear_publish_interleavings_have_no_lost_wake() {
        use std::sync::atomic::AtomicUsize;
        use std::sync::Barrier;
        for _ in 0..100 {
            let mailbox = Arc::new(PosterMailbox::default());
            let wakes = Arc::new(AtomicUsize::new(0));
            mailbox.expect(1, vec![source("same")]).unwrap();
            mailbox
                .publish(completion(1, "same"), || {
                    wakes.fetch_add(1, Ordering::AcqRel);
                    true
                })
                .unwrap();
            let barrier = Arc::new(Barrier::new(2));
            let producer = mailbox.clone();
            let mark = barrier.clone();
            let count = wakes.clone();
            let thread = std::thread::spawn(move || {
                mark.wait();
                let mut next = completion(1, "same");
                next.cursor = 2;
                producer
                    .publish(next, || {
                        count.fetch_add(1, Ordering::AcqRel);
                        true
                    })
                    .unwrap();
            });
            barrier.wait();
            let taken = mailbox.take().unwrap();
            thread.join().unwrap();
            let retained = mailbox.state.lock().unwrap().completed.is_some();
            assert!(taken.is_some());
            assert_eq!(wakes.load(Ordering::Acquire), if retained { 2 } else { 1 });
            if retained {
                assert!(mailbox.take().unwrap().is_some());
            }
            assert!(!mailbox.state.lock().unwrap().wake_pending);
        }
    }
    #[test]
    fn d3_invalidation_reuses_old_token_safely_and_failed_notification_can_retry() {
        let mailbox = PosterMailbox::default();
        let mut wakes = 0;
        mailbox.expect(1, vec![source("old")]).unwrap();
        mailbox
            .publish(completion(1, "old"), || {
                wakes += 1;
                true
            })
            .unwrap();
        mailbox.expect(2, vec![]).unwrap();
        assert!(mailbox.take().unwrap().is_none());
        mailbox
            .publish(completion(1, "old"), || panic!("hidden stale completion"))
            .unwrap();
        assert!(mailbox.take().unwrap().is_none());
        mailbox.expect(3, vec![source("new")]).unwrap();
        assert_eq!(
            mailbox.publish(completion(3, "new"), || false),
            Err(WorkerFailure::CompletionChannelClosed)
        );
        assert!(mailbox.take().unwrap().is_none());
        mailbox
            .publish(completion(3, "new"), || {
                wakes += 1;
                true
            })
            .unwrap();
        mailbox.expect(4, vec![source("newest")]).unwrap();
        mailbox
            .publish(completion(4, "newest"), || {
                panic!("old wake must be reused")
            })
            .unwrap();
        assert_eq!(mailbox.take().unwrap().unwrap().generation, 4);
        assert_eq!(wakes, 2);
    }
    #[test]
    fn d3_fake_decoder_and_callback_keep_single_active_pending_and_completed_owners() {
        use std::sync::atomic::AtomicUsize;
        use std::sync::{mpsc, Barrier};
        let generation = Arc::new(AtomicU64::new(1));
        let mailbox = Arc::new(PosterMailbox::default());
        let release = Arc::new(Barrier::new(2));
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let wakes = Arc::new(AtomicUsize::new(0));
        let output = mailbox.clone();
        let count = wakes.clone();
        let blocked = release.clone();
        let worker = PosterWorker::spawn_with_builder(
            std::thread::Builder::new().name("d3-fake-poster".into()),
            generation.clone(),
            move |completion| {
                let g = completion.generation;
                let finished = completion.finished;
                let ok = output
                    .publish(completion, || {
                        count.fetch_add(1, Ordering::AcqRel);
                        true
                    })
                    .is_ok();
                if finished {
                    done_tx.send(g).unwrap();
                }
                ok
            },
            move |source, _| {
                if source.item_id == "first" {
                    started_tx.send(()).unwrap();
                    blocked.wait();
                }
                Some(gpu_gallery::video_placeholder_poster(
                    &source.path,
                    source.layer,
                ))
            },
        )
        .unwrap();
        mailbox.expect(1, vec![source("first")]).unwrap();
        worker
            .submit(PosterBatch::new(1, vec![source("first")], PathBuf::new()))
            .unwrap();
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        for (g, id) in [(2, "discarded"), (3, "latest")] {
            generation.store(g, Ordering::Release);
            mailbox.expect(g, vec![source(id)]).unwrap();
            worker
                .submit(PosterBatch::new(g, vec![source(id)], PathBuf::new()))
                .unwrap();
            assert_eq!(worker.pending_count(), 1);
        }
        release.wait();
        assert_eq!(
            done_rx
                .recv_timeout(std::time::Duration::from_secs(5))
                .unwrap(),
            3
        );
        for g in 4..10 {
            generation.store(g, Ordering::Release);
            mailbox.expect(g, vec![source("latest")]).unwrap();
            worker
                .submit(PosterBatch::new(g, vec![source("latest")], PathBuf::new()))
                .unwrap();
            assert_eq!(
                done_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap(),
                g
            );
        }
        assert_eq!(wakes.load(Ordering::Acquire), 1);
        assert_eq!(mailbox.take().unwrap().unwrap().generation, 9);
        assert_eq!(worker.pending_count(), 0);
    }
    #[test]
    fn d3_mailbox_poison_is_reported_without_publishing_or_stealing_texture() {
        let mailbox = Arc::new(PosterMailbox::default());
        let poisoned = mailbox.clone();
        assert!(std::thread::spawn(move || {
            let _guard = poisoned.state.lock().unwrap();
            panic!("injected mailbox poison");
        })
        .join()
        .is_err());
        assert_eq!(
            mailbox.expect(1, vec![source("a")]),
            Err(WorkerFailure::QueuePoisoned)
        );
        assert!(matches!(mailbox.take(), Err(WorkerFailure::QueuePoisoned)));
        assert_eq!(
            mailbox.publish(completion(1, "a"), || panic!()),
            Err(WorkerFailure::QueuePoisoned)
        );
        let recovered = PosterMailbox::default();
        recovered.expect(2, vec![source("b")]).unwrap();
        recovered.publish(completion(2, "b"), || true).unwrap();
        assert_eq!(
            recovered.take().unwrap().unwrap().posters[0].source.item_id,
            "b"
        );
    }
    #[test]
    fn d3_mailbox_rejects_duplicate_layers_before_replacing_latest() {
        let mailbox = PosterMailbox::default();
        mailbox.expect(3, vec![source("latest")]).unwrap();
        mailbox.publish(completion(3, "latest"), || true).unwrap();
        let mut invalid = completion(3, "latest");
        invalid.cursor = 2;
        invalid.posters.push(invalid.posters[0].clone());
        mailbox
            .publish(invalid, || panic!("one queued notification"))
            .unwrap();
        assert_eq!(
            mailbox.take().unwrap().unwrap().posters.len(),
            1,
            "duplicated layer cannot replace a valid completion"
        );
    }
}

#[derive(Debug, Clone)]
pub struct DecodedPoster {
    pub source: PosterSource,
    pub poster: Poster,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerFailure {
    CompletionChannelClosed,
    DecoderPanicked,
    DecoderUnavailable,
    QueuePoisoned,
    WorkerExited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkerExit {
    pub worker_id: u64,
    pub failure: WorkerFailure,
}

impl fmt::Display for WorkerFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::CompletionChannelClosed => "poster completion channel closed",
            Self::DecoderPanicked => "poster decoder failed unexpectedly",
            Self::DecoderUnavailable => "poster decode pool is unavailable",
            Self::QueuePoisoned => "poster worker queue was poisoned",
            Self::WorkerExited => "poster worker exited unexpectedly",
        };
        formatter.write_str(message)
    }
}

#[derive(Debug)]
pub struct SpawnError(std::io::Error);

impl fmt::Display for SpawnError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "could not start poster worker: {}", self.0)
    }
}

impl std::error::Error for SpawnError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmitError {
    CacheBusy,
    Closed,
    WorkerUnavailable(WorkerFailure),
    QueuePoisoned,
    InvalidColourBatch,
}

impl fmt::Display for SubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CacheBusy => formatter.write_str("poster cache operation is still settling"),
            Self::Closed => formatter.write_str("poster worker is shutting down"),
            Self::WorkerUnavailable(failure) => write!(formatter, "{failure}"),
            Self::QueuePoisoned => formatter.write_str("poster worker queue was poisoned"),
            Self::InvalidColourBatch => formatter.write_str("invalid colour batch"),
        }
    }
}

struct RestoreBatch {
    posters: Vec<PosterRestore>,
    epoch: u64,
    redraw: Box<dyn FnOnce() + Send>,
}

enum Work {
    Restore(RestoreBatch),
    Posters(PosterBatch),
    ClearCache(cache::Request),
    MaintainCache,
}
struct QueueState {
    pending: Option<Work>,
    restore: Option<RestoreBatch>,
    clear: Option<cache::Request>,
    colour: Option<Vec<colour::Job>>,
    pregeneration: Option<pregeneration::Pass>,
    background_active: bool,
    poster_idle_since: Option<Instant>,
    cache_busy: bool,
}

struct Shared {
    id: u64,
    #[cfg(test)]
    next_submit_error: Mutex<Option<SubmitError>>,
    #[cfg(test)]
    before_pregeneration: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    queue: Mutex<QueueState>,
    wake: Condvar,
    cache_epoch: AtomicU64,
    colour_revision: AtomicU64,
    shutdown: AtomicBool,
    running: AtomicBool,
    failure: Mutex<Option<WorkerFailure>>,
}

impl Shared {
    fn new() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        Self {
            id: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            #[cfg(test)]
            next_submit_error: Mutex::new(None),
            #[cfg(test)]
            before_pregeneration: Mutex::new(None),
            queue: Mutex::new(QueueState {
                pending: None,
                restore: None,
                clear: None,
                colour: None,
                pregeneration: None,
                background_active: false,
                poster_idle_since: None,
                cache_busy: false,
            }),
            wake: Condvar::new(),
            cache_epoch: AtomicU64::new(0),
            colour_revision: AtomicU64::new(0),
            shutdown: AtomicBool::new(false),
            running: AtomicBool::new(true),
            failure: Mutex::new(None),
        }
    }
}

pub struct PosterWorker {
    shared: Arc<Shared>,
}

impl fmt::Debug for PosterWorker {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PosterWorker")
            .field("running", &self.is_running())
            .field("failure", &self.unavailable_reason())
            .finish()
    }
}

impl PosterWorker {
    #[cfg(test)]
    pub fn spawn<P>(generation_clock: Arc<AtomicU64>, publish: P) -> Result<Self, SpawnError>
    where
        P: Fn(PosterCompletion) -> bool + Send + Sync + 'static,
    {
        Self::spawn_with_runner(
            std::thread::Builder::new().name("gpu-poster-decode".into()),
            generation_clock,
            publish,
            |_| {},
            decode_source,
            None,
            1,
        )
    }

    pub(crate) fn spawn_with_cache<P>(
        generation_clock: Arc<AtomicU64>,
        publish: P,
        notify_exit: impl FnOnce(WorkerExit) + Send + 'static,
        profile: PathBuf,
        cache_max_mib: Option<u32>,
        decode_threads: Option<u8>,
        preview: bool,
    ) -> Result<Self, SpawnError>
    where
        P: Fn(PosterCompletion) -> bool + Send + Sync + 'static,
    {
        Self::spawn_with_runner(
            std::thread::Builder::new().name("gpu-poster-decode".into()),
            generation_clock,
            publish,
            notify_exit,
            decode_source,
            Some(cache::Maintenance::new(profile, cache_max_mib, preview)),
            decode_worker_count(decode_threads),
        )
    }

    #[cfg(test)]
    pub(crate) fn spawn_with_builder<P, D>(
        builder: std::thread::Builder,
        generation_clock: Arc<AtomicU64>,
        publish: P,
        decode: D,
    ) -> Result<Self, SpawnError>
    where
        P: Fn(PosterCompletion) -> bool + Send + Sync + 'static,
        D: Fn(&PosterSource, &std::path::Path) -> Option<Poster> + Send + Sync + 'static,
    {
        Self::spawn_with_runner(
            builder,
            generation_clock,
            publish,
            |_| {},
            move |source, cache, _, _| {
                decode(source, cache.root()).ok_or(PosterDecodeError::InvalidPixels)
            },
            None,
            1,
        )
    }

    fn spawn_with_runner<P, D>(
        builder: std::thread::Builder,
        generation_clock: Arc<AtomicU64>,
        publish: P,
        notify_exit: impl FnOnce(WorkerExit) + Send + 'static,
        decode: D,
        maintenance: Option<cache::Maintenance>,
        workers: usize,
    ) -> Result<Self, SpawnError>
    where
        P: Fn(PosterCompletion) -> bool + Send + Sync + 'static,
        D: Fn(
                &PosterSource,
                &cache::WriteBatch,
                &PosterDecoder,
                &dyn Fn() -> bool,
            ) -> Result<Poster, PosterDecodeError>
            + Send
            + Sync
            + 'static,
    {
        let shared = Arc::new(Shared::new());
        let worker_shared = Arc::clone(&shared);
        builder
            .spawn(move || {
                // A coordinator unwind still has to report exit after all scoped
                // producers and the colour lane have released their cache guards.
                let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    run_worker(
                        &worker_shared,
                        generation_clock,
                        publish,
                        decode,
                        maintenance,
                        workers,
                    );
                }));
                // Decoder setup can fail before the scoped exit guard exists.
                if worker_shared.running.load(Ordering::Acquire) {
                    finish_worker_exit(&worker_shared, || {});
                }
                if !worker_shared.shutdown.load(Ordering::Acquire) {
                    let failure = worker_shared
                        .failure
                        .lock()
                        .ok()
                        .and_then(|failure| *failure)
                        .unwrap_or(WorkerFailure::WorkerExited);
                    notify_exit(WorkerExit {
                        worker_id: worker_shared.id,
                        failure,
                    });
                }
            })
            .map_err(SpawnError)?;
        Ok(Self { shared })
    }

    pub fn submit(&self, batch: PosterBatch) -> Result<(), SubmitError> {
        #[cfg(test)]
        if let Some(error) = self.shared.next_submit_error.lock().unwrap().take() {
            return Err(error);
        }
        self.submit_work(Work::Posters(batch))
    }

    #[cfg(test)]
    pub(crate) fn reject_next_submission(&self, error: SubmitError) {
        *self.shared.next_submit_error.lock().unwrap() = Some(error);
    }

    pub(crate) fn restore(
        &self,
        wanted: &[Poster],
        posters: Vec<(Poster, crate::gpu_gallery::PosterEncoding)>,
        redraw: impl FnOnce() + Send + 'static,
    ) {
        let Ok(mut queue) = self.shared.queue.lock() else {
            return;
        };
        if queue.cache_busy || !self.is_running() || self.shared.shutdown.load(Ordering::Acquire) {
            return;
        }
        let mut pending = queue
            .restore
            .take()
            .map_or_else(Vec::new, |batch| batch.posters);
        pending.retain(|request| {
            wanted
                .iter()
                .any(|poster| poster.same_resource(request.poster()))
        });
        pending.extend(
            posters
                .iter()
                .filter_map(|(poster, encoding)| poster.request_restore(*encoding)),
        );
        if !pending.is_empty() {
            queue.restore = Some(RestoreBatch {
                posters: pending,
                epoch: self.shared.cache_epoch.load(Ordering::Acquire),
                redraw: Box::new(redraw),
            });
            queue.poster_idle_since = None;
        }
        self.shared.wake.notify_all();
    }

    fn submit_work(&self, work: Work) -> Result<(), SubmitError> {
        if let Some(failure) = self.unavailable_reason() {
            return Err(SubmitError::WorkerUnavailable(failure));
        }
        if !self.is_running() {
            return Err(SubmitError::WorkerUnavailable(WorkerFailure::WorkerExited));
        }

        let mut queue = match self.shared.queue.lock() {
            Ok(queue) => queue,
            Err(_) => {
                record_failure(&self.shared, WorkerFailure::QueuePoisoned);
                return Err(SubmitError::QueuePoisoned);
            }
        };
        if self.shared.shutdown.load(Ordering::Acquire) {
            return Err(SubmitError::Closed);
        }
        if !self.is_running() {
            return Err(SubmitError::WorkerUnavailable(
                self.unavailable_reason()
                    .unwrap_or(WorkerFailure::WorkerExited),
            ));
        }

        if queue.cache_busy && matches!(work, Work::Posters(_)) {
            return Err(SubmitError::CacheBusy);
        }

        // This is the only pending slot. Replacing it keeps bursts bounded and
        // ensures the worker eventually sees the newest eligible demand.
        queue.pending = Some(work);
        queue.poster_idle_since = None;
        self.shared.wake.notify_all();
        Ok(())
    }

    #[cfg(test)]
    pub fn submit_colour(&self, job: colour::Job) -> Result<(), SubmitError> {
        self.submit_colours(vec![job])
    }

    pub fn submit_colours(&self, mut jobs: Vec<colour::Job>) -> Result<(), SubmitError> {
        #[cfg(test)]
        if let Some(error) = self.shared.next_submit_error.lock().unwrap().take() {
            return Err(error);
        }
        let Some(first) = jobs.first() else {
            return Err(SubmitError::InvalidColourBatch);
        };
        if jobs.len() > colour::BATCH_LIMIT
            || jobs.iter().any(|job| {
                job.profile != first.profile
                    || job.generation != first.generation
                    || job.still.catalog_revision != first.still.catalog_revision
                    || !Arc::ptr_eq(&job.clock, &first.clock)
                    || !Arc::ptr_eq(&job.catalog_clock, &first.catalog_clock)
            })
        {
            return Err(SubmitError::InvalidColourBatch);
        }
        let mut q = self
            .shared
            .queue
            .lock()
            .map_err(|_| SubmitError::QueuePoisoned)?;
        if self.shared.shutdown.load(Ordering::Acquire) || !self.is_running() {
            return Err(SubmitError::Closed);
        }
        if q.cache_busy {
            return Err(SubmitError::CacheBusy);
        }
        let epoch = self.shared.cache_epoch.load(Ordering::Acquire);
        for job in &mut jobs {
            job.epoch = epoch;
        }
        self.shared.colour_revision.fetch_add(1, Ordering::AcqRel);
        q.colour = Some(jobs);
        self.shared.wake.notify_all();
        Ok(())
    }
    pub fn cancel_colour(&self) {
        if let Ok(mut q) = self.shared.queue.lock() {
            self.shared.colour_revision.fetch_add(1, Ordering::AcqRel);
            q.colour = None;
            self.shared.wake.notify_all();
        }
    }

    pub(crate) fn pregenerate(
        &self,
        sources: Vec<PregenerationSource>,
        clock: Arc<AtomicU64>,
        revision: u64,
    ) {
        let Ok(mut queue) = self.shared.queue.lock() else {
            return;
        };
        if queue.cache_busy || !self.is_running() || self.shared.shutdown.load(Ordering::Acquire) {
            return;
        }
        queue.pregeneration = Some(pregeneration::Pass::new(
            sources,
            clock,
            revision,
            self.cache_epoch(),
        ));
        self.shared.wake.notify_all();
    }

    pub fn cache_epoch(&self) -> u64 {
        self.shared.cache_epoch.load(Ordering::Acquire)
    }
    pub fn request_cache(&self, mut request: cache::Request) -> Result<u64, SubmitError> {
        let mut queue = self
            .shared
            .queue
            .lock()
            .map_err(|_| SubmitError::QueuePoisoned)?;
        if self.shared.shutdown.load(Ordering::Acquire) || !self.is_running() {
            return Err(SubmitError::Closed);
        }
        if queue.cache_busy {
            return Err(SubmitError::CacheBusy);
        }
        request.epoch = self.shared.cache_epoch.fetch_add(1, Ordering::AcqRel) + 1;
        let epoch = request.epoch;
        queue.cache_busy = true;
        queue.colour = None;
        queue.pregeneration = None;
        if matches!(queue.pending, Some(Work::Posters(_))) {
            queue.pending = None;
        }
        queue.restore = None;
        queue.clear = Some(request);
        self.shared.wake.notify_all();
        Ok(epoch)
    }
    pub fn cache_busy(&self) -> bool {
        self.shared.queue.lock().map_or(true, |q| q.cache_busy)
    }
    pub fn cancel_pending(&self) {
        if let Ok(mut queue) = self.shared.queue.lock() {
            queue.pending = None;
            self.shared.wake.notify_all();
        }
    }

    pub fn cancel_posters_pending(&self) {
        if let Ok(mut queue) = self.shared.queue.lock() {
            if matches!(queue.pending, Some(Work::Posters(_))) {
                queue.pending = None;
                self.shared.wake.notify_all();
            }
        }
    }

    pub fn unavailable_reason(&self) -> Option<WorkerFailure> {
        if let Ok(failure) = self.shared.failure.lock() {
            if failure.is_some() {
                return *failure;
            }
        }
        (!self.is_running() && !self.shared.shutdown.load(Ordering::Acquire))
            .then_some(WorkerFailure::WorkerExited)
    }

    fn is_running(&self) -> bool {
        self.shared.running.load(Ordering::Acquire)
    }

    pub(crate) fn id(&self) -> u64 {
        self.shared.id
    }

    pub(crate) fn can_replace(&self) -> bool {
        // A stopped coordinator can still have an invalidated background batch
        // releasing its cache guard. Keep one cache owner until that settles.
        !self.is_running()
            && !self
                .shared
                .queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .background_active
    }

    #[cfg(test)]
    fn pending_count(&self) -> usize {
        self.shared
            .queue
            .lock()
            .ok()
            .and_then(|queue| queue.pending.as_ref().map(|_| 1))
            .unwrap_or(0)
    }

    #[cfg(test)]
    fn wait_until_stopped(&self) {
        let Ok(mut queue) = self.shared.queue.lock() else {
            return;
        };
        while self.is_running() {
            queue = match self.shared.wake.wait(queue) {
                Ok(queue) => queue,
                Err(_) => return,
            };
        }
    }
}

impl Drop for PosterWorker {
    fn drop(&mut self) {
        if let Ok(mut queue) = self.shared.queue.lock() {
            self.shared.shutdown.store(true, Ordering::Release);
            queue.pending = None;
        } else {
            self.shared.shutdown.store(true, Ordering::Release);
        }
        self.shared.wake.notify_all();
    }
}

struct WorkerExitGuard<'a> {
    shared: &'a Shared,
}

impl Drop for WorkerExitGuard<'_> {
    fn drop(&mut self) {
        finish_worker_exit(self.shared, || {});
    }
}
fn finish_worker_exit(shared: &Shared, after_drain: impl FnOnce()) {
    let (colour_job, clear) = {
        let mut queue = shared
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Submission and final draining see the same closed availability under this lock.
        shared.running.store(false, Ordering::Release);
        queue.pending = None;
        queue.restore = None;
        queue.pregeneration = None;
        let colour_job = queue.colour.take();
        let clear = queue.clear.take();
        queue.cache_busy = false;
        (colour_job, clear)
    };
    after_drain();
    if !shared.shutdown.load(Ordering::Acquire) {
        let has_failure = shared
            .failure
            .lock()
            .ok()
            .is_some_and(|failure| failure.is_some());
        if !has_failure {
            record_failure(&shared, WorkerFailure::WorkerExited);
        }
    }
    shared.wake.notify_all();
    if let Some(request) = clear {
        // Exit can itself run during unwind. An external acknowledgement must
        // neither re-enter the queue lock nor interrupt joined worker cleanup.
        let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (request.acknowledge)(cache::Report {
                epoch: request.epoch,
                error: Some("Worker stopped before cache barrier; no clear acknowledged".into()),
                ..cache::Report::default()
            });
        }));
    }
    if let Some(job) = colour_job.into_iter().flatten().next() {
        let failure = shared
            .failure
            .lock()
            .ok()
            .and_then(|f| *f)
            .unwrap_or(WorkerFailure::WorkerExited);
        colour::reject_queued(job, failure);
    }
}

fn wait_for_cache_barrier(shared: &Shared) -> bool {
    let settled = (|| {
        let mut queue = shared.queue.lock().ok()?;
        while queue.background_active {
            queue = shared.wake.wait(queue).ok()?;
        }
        Some(())
    })();
    if settled.is_none() {
        record_failure(shared, WorkerFailure::QueuePoisoned);
    }
    settled.is_some()
}

fn record_failure(shared: &Shared, failure: WorkerFailure) {
    if let Ok(mut current) = shared.failure.lock() {
        if current.is_none() {
            *current = Some(failure);
        }
    }
}

const MAINTENANCE_QUIET: Duration = Duration::from_millis(300);
const MAINTENANCE_INTERVAL: Duration = Duration::from_secs(60);
const COLOUR_QUIET: Duration = Duration::from_millis(300);

fn take_pending(shared: &Shared, maintenance_due: Option<Instant>) -> Option<Work> {
    let due = maintenance_due.map(|due| due.max(Instant::now() + MAINTENANCE_QUIET));
    let mut queue = match shared.queue.lock() {
        Ok(queue) => queue,
        Err(_) => {
            record_failure(shared, WorkerFailure::QueuePoisoned);
            return None;
        }
    };
    loop {
        if shared.shutdown.load(Ordering::Acquire) {
            return None;
        }
        if let Some(request) = queue.clear.take() {
            queue.poster_idle_since = None;
            return Some(Work::ClearCache(request));
        }
        if let Some(batch) = queue.pending.take() {
            queue.poster_idle_since = None;
            return Some(batch);
        }
        if let Some(batch) = queue.restore.take() {
            queue.poster_idle_since = None;
            return Some(Work::Restore(batch));
        }
        if queue.poster_idle_since.is_none() {
            queue.poster_idle_since = Some(Instant::now());
            shared.wake.notify_all();
        }
        let wait = if let Some(due) = due {
            let left = due.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Some(Work::MaintainCache);
            }
            shared
                .wake
                .wait_timeout(queue, left)
                .map(|(queue, _)| queue)
                .map_err(|_| ())
        } else {
            shared.wake.wait(queue).map_err(|_| ())
        };
        queue = match wait {
            Ok(queue) => queue,
            Err(_) => {
                record_failure(shared, WorkerFailure::QueuePoisoned);
                return None;
            }
        };
    }
}

fn maintenance_idle(shared: &Shared) -> bool {
    !shared.shutdown.load(Ordering::Acquire)
        && shared.queue.lock().is_ok_and(|queue| {
            queue.pending.is_none()
                && queue.restore.is_none()
                && queue.clear.is_none()
                && queue.colour.is_none()
                && !queue.background_active
                && !queue.cache_busy
        })
}

fn colour_idle_delay(queue: &QueueState) -> Option<Duration> {
    if queue.pending.is_some() || queue.restore.is_some() || queue.cache_busy {
        return None;
    }
    queue
        .poster_idle_since
        .map(|since| COLOUR_QUIET.saturating_sub(since.elapsed()))
}

fn wait_for_colour<'a>(
    shared: &'a Shared,
    mut queue: std::sync::MutexGuard<'a, QueueState>,
    cancelled: impl Fn() -> bool,
    has_work: impl Fn(&QueueState) -> bool,
) -> Option<std::sync::MutexGuard<'a, QueueState>> {
    loop {
        if cancelled() {
            return None;
        }
        queue = match has_work(&queue)
            .then(|| colour_idle_delay(&queue))
            .flatten()
        {
            Some(delay) if delay.is_zero() => return Some(queue),
            Some(delay) => shared.wake.wait_timeout(queue, delay).ok()?.0,
            None => shared.wake.wait(queue).ok()?,
        };
    }
}

struct BackgroundActivity<'a>(&'a Shared);

impl Drop for BackgroundActivity<'_> {
    fn drop(&mut self) {
        self.0
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .background_active = false;
        self.0.wake.notify_all();
    }
}

fn run_background_lane(shared: &Shared, maintenance: Option<(&cache::Maintenance, u64)>) {
    use windows_sys::Win32::System::Threading::{
        GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
    };
    // The pseudo-handle always refers to this worker. It must not be closed.
    let priority_set =
        unsafe { SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL) } != 0;
    crate::apply_trace::event_lazy("colour-lane", || {
        format!(
            "priority_below_normal={priority_set}; quiet_ms={}",
            COLOUR_QUIET.as_millis()
        )
    });
    let stopped =
        || shared.shutdown.load(Ordering::Acquire) || !shared.running.load(Ordering::Acquire);
    loop {
        let Some(mut queue) = shared.queue.lock().ok().and_then(|queue| {
            wait_for_colour(shared, queue, stopped, |queue| {
                queue.colour.is_some() || queue.pregeneration.is_some()
            })
        }) else {
            return;
        };
        queue.background_active = true;
        let _activity = BackgroundActivity(shared);
        let Some(jobs) = queue.colour.take() else {
            let pass = queue.pregeneration.take().unwrap();
            let idle_since = queue.poster_idle_since.unwrap();
            drop(queue);
            if let Some((cache, budget)) = maintenance {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    pass.run(shared, cache, budget, idle_since);
                }))
                .is_err()
                {
                    crate::config::log_debug("Poster pre-generation stopped after a panic");
                }
            }
            continue;
        };
        let revision = shared.colour_revision.load(Ordering::Acquire);
        let epoch = shared.cache_epoch.load(Ordering::Acquire);
        drop(queue);
        let cancelled = || {
            stopped()
                || revision != shared.colour_revision.load(Ordering::Acquire)
                || epoch != shared.cache_epoch.load(Ordering::Acquire)
        };
        colour::execute_batch(
            jobs,
            &cancelled,
            |cancelled| {
                shared.queue.lock().ok().is_some_and(|queue| {
                    wait_for_colour(shared, queue, cancelled, |_| true).is_some()
                })
            },
            || {
                if !shared.running.load(Ordering::Acquire)
                    && !shared.shutdown.load(Ordering::Acquire)
                    && revision == shared.colour_revision.load(Ordering::Acquire)
                    && epoch == shared.cache_epoch.load(Ordering::Acquire)
                {
                    shared
                        .failure
                        .lock()
                        .ok()
                        .and_then(|failure| *failure)
                        .or(Some(WorkerFailure::WorkerExited))
                } else {
                    None
                }
            },
        );
    }
}

struct DecodeTask {
    active: Arc<DecodeBatch>,
    reply: mpsc::SyncSender<DecodeReply>,
}

enum DecodeReply {
    Decoded(usize, Result<Poster, PosterDecodeError>),
    Cancelled,
    Settled(Result<(), WorkerFailure>),
}

struct DecodeBatch {
    batch: PosterBatch,
    epoch: u64,
    cache: cache::WriteBatch,
    jobs: Mutex<DecodeQueues>,
    wanted: HashSet<usize>,
    aliases: HashMap<usize, Vec<usize>>,
    reusable_by_source: HashMap<(PathBuf, PosterKind, Fingerprint), u32>,
    failed: AtomicBool,
    counters: Option<DecodeCounters>,
}

#[derive(Default)]
struct DecodeCounters {
    decoding: AtomicUsize,
    peak_decoding: AtomicUsize,
}

#[derive(Default)]
struct DecodeQueues {
    visible: VecDeque<usize>,
    backfill: VecDeque<usize>,
    visible_claimed: usize,
    backfill_claimed: usize,
}

impl DecodeBatch {
    fn new(batch: PosterBatch, epoch: u64, mut wanted: impl FnMut(&PosterSource) -> bool) -> Self {
        let mut jobs = DecodeQueues::default();
        let mut wanted_positions = HashSet::new();
        let mut leaders = HashMap::new();
        let mut aliases: HashMap<_, Vec<_>> = HashMap::new();
        // Visit visible demand first so a backfill alias never leads a visible job.
        for visible in [true, false] {
            for (position, source) in batch.sources.iter().enumerate() {
                if batch.visible.contains(&source.index) != visible || !wanted(source) {
                    continue;
                }
                wanted_positions.insert(position);
                if let Some(stamp) = source.fingerprint() {
                    let leader = *leaders
                        .entry((&source.path, source.kind, stamp))
                        .or_insert(position);
                    if leader != position {
                        aliases.entry(leader).or_default().push(position);
                        continue;
                    }
                }
                if visible {
                    jobs.visible.push_front(position);
                } else {
                    jobs.backfill.push_back(position);
                }
            }
        }
        let reusable_by_source = batch
            .reusable
            .iter()
            .filter_map(|(&layer, poster)| {
                let (kind, fingerprint) = poster.reusable_fingerprint(&batch.cache_dir)?;
                Some(((poster.source.clone(), kind, fingerprint.clone()), layer))
            })
            .collect();
        Self {
            cache: cache::WriteBatch::new(&batch.cache_dir),
            batch,
            epoch,
            jobs: Mutex::new(jobs),
            wanted: wanted_positions,
            aliases,
            reusable_by_source,
            failed: AtomicBool::new(false),
            counters: crate::apply_trace::enabled().then(DecodeCounters::default),
        }
    }

    fn is_current(&self, shared: &Shared, generation: &AtomicU64) -> bool {
        !shared.shutdown.load(Ordering::Acquire)
            && self.epoch == shared.cache_epoch.load(Ordering::Acquire)
            && self.batch.generation == generation.load(Ordering::Acquire)
    }

    fn claim(&self, shared: &Shared, generation: &AtomicU64) -> Option<usize> {
        let mut jobs = self.jobs.lock().unwrap();
        if self.failed.load(Ordering::Acquire) || !self.is_current(shared, generation) {
            return None;
        }
        if let Some(position) = jobs.visible.pop_back() {
            jobs.visible_claimed += usize::from(self.counters.is_some());
            return Some(position);
        }
        if let Some(position) = jobs.backfill.pop_front() {
            jobs.backfill_claimed += usize::from(self.counters.is_some());
            return Some(position);
        }
        None
    }

    fn decode(
        &self,
        shared: &Shared,
        generation: &AtomicU64,
        decoder: &PosterDecoder,
        decode: impl Fn(
            &PosterSource,
            &cache::WriteBatch,
            &PosterDecoder,
            &dyn Fn() -> bool,
        ) -> Result<Poster, PosterDecodeError>,
        reply: &mpsc::SyncSender<DecodeReply>,
    ) {
        while let Some(position) = self.claim(shared, generation) {
            let result = self.decode_claim(position, shared, generation, decoder, &decode);
            if reply.send(result).is_err() {
                return;
            }
        }
    }

    fn decode_claim(
        &self,
        position: usize,
        shared: &Shared,
        generation: &AtomicU64,
        decoder: &PosterDecoder,
        decode: impl Fn(
            &PosterSource,
            &cache::WriteBatch,
            &PosterDecoder,
            &dyn Fn() -> bool,
        ) -> Result<Poster, PosterDecodeError>,
    ) -> DecodeReply {
        // The wanted positions belong to this generation and cache epoch.
        let wanted = || {
            self.is_current(shared, generation)
                && !self.failed.load(Ordering::Acquire)
                && self.wanted.contains(&position)
        };
        if !wanted() {
            return DecodeReply::Cancelled;
        }
        let source = &self.batch.sources[position];
        let poster = self
            .batch
            .reusable
            .get(&source.layer)
            .filter(|poster| {
                poster.source == source.path
                    && poster.matches_source(source.kind, source.fingerprint())
                    && source.still.as_ref().is_none_or(|still| still.is_current())
                    && source.video_is_current()
            })
            .cloned()
            .or_else(|| {
                let stamp = source.fingerprint()?;
                let layer = self.reusable_by_source.get(&(
                    source.path.clone(),
                    source.kind,
                    stamp.clone(),
                ))?;
                let seed = self.batch.reusable.get(layer)?;
                (wanted() && stamp.matches(&source.path))
                    .then(|| decoder.fork_slot(seed, source.layer))
            })
            .map_or_else(
                || {
                    if !wanted() {
                        return Err(PosterDecodeError::Cancelled);
                    }
                    let _timing = crate::apply_trace::cpu_span("poster-source-decode", || {
                        format!(
                            "generation={} layer={}",
                            self.batch.generation, source.layer
                        )
                    });
                    if let Some(counters) = &self.counters {
                        let active = counters.decoding.fetch_add(1, Ordering::Relaxed) + 1;
                        counters.peak_decoding.fetch_max(active, Ordering::Relaxed);
                        let result = decode(source, &self.cache, decoder, &wanted);
                        counters.decoding.fetch_sub(1, Ordering::Relaxed);
                        result
                    } else {
                        decode(source, &self.cache, decoder, &wanted)
                    }
                },
                Ok,
            );
        if wanted() && !matches!(poster, Err(PosterDecodeError::Cancelled)) {
            DecodeReply::Decoded(position, poster)
        } else {
            DecodeReply::Cancelled
        }
    }

    fn fan_out(
        &self,
        position: usize,
        poster: Poster,
        decoder: &PosterDecoder,
    ) -> Vec<DecodedPoster> {
        // Prepare the independent upload states before the first recipient can
        // consume the shared bytes on the renderer.
        let mut results = vec![DecodedPoster {
            source: self.batch.sources[position].clone(),
            poster,
        }];
        for &alias in self.aliases.get(&position).into_iter().flatten() {
            let source = &self.batch.sources[alias];
            let poster = &results[0].poster;
            let slot = self.batch.reusable.get(&source.layer).filter(|slot| {
                slot.source == source.path
                    && slot.matches_source(source.kind, source.fingerprint())
                    && source.video_is_current()
            });
            results.push(DecodedPoster {
                source: source.clone(),
                poster: slot
                    .cloned()
                    .unwrap_or_else(|| decoder.fork_slot(poster, source.layer)),
            });
        }
        results
    }

    fn trace(&self, cancelled: usize) {
        let Some(counters) = &self.counters else {
            return;
        };
        crate::apply_trace::event_lazy("poster-decode-batch", || {
            let jobs = self.jobs.lock().unwrap();
            format!(
                "generation={}; visible_claimed={}; backfill_claimed={}; peak_active={}; cancelled={cancelled}; shared_slots={}",
                self.batch.generation,
                jobs.visible_claimed,
                jobs.backfill_claimed,
                counters.peak_decoding.load(Ordering::Relaxed),
                self.aliases.values().map(Vec::len).sum::<usize>(),
            )
        });
    }
}

fn run_worker<P, D>(
    shared: &Shared,
    generation_clock: Arc<AtomicU64>,
    publish: P,
    decode: D,
    maintenance: Option<cache::Maintenance>,
    workers: usize,
) where
    P: Fn(PosterCompletion) -> bool + Send + Sync + 'static,
    D: Fn(
            &PosterSource,
            &cache::WriteBatch,
            &PosterDecoder,
            &dyn Fn() -> bool,
        ) -> Result<Poster, PosterDecodeError>
        + Send
        + Sync,
{
    let decoder = PosterDecoder::with_producers(decode_pool_capacity(), workers);
    let (generation_clock, decoder, decode) = (&*generation_clock, &decoder, &decode);
    let maintenance = maintenance.as_ref().map(|cache| (cache, cache.budget()));
    std::thread::scope(|scope| {
        // Signal shutdown to the scoped colour thread before joining it.
        let _exit_guard = WorkerExitGuard { shared };
        if let Err(error) = std::thread::Builder::new()
            .name("gpu-colour-analysis".into())
            .spawn_scoped(scope, || run_background_lane(shared, maintenance))
        {
            crate::config::log_debug(&format!("Could not start colour worker: {error}"));
            record_failure(shared, WorkerFailure::DecoderUnavailable);
            return;
        }
        let mut pool = Vec::with_capacity(workers);
        for index in 0..workers {
            let (send, receive) = mpsc::channel::<DecodeTask>();
            let thread = std::thread::Builder::new()
                .name(format!("gpu-poster-decode-{index}"))
                .spawn_scoped(scope, move || {
                    for DecodeTask { active, reply } in receive {
                        let settled =
                            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                                active.decode(shared, generation_clock, decoder, decode, &reply);
                            }))
                            .map_err(|_| {
                                active.failed.store(true, Ordering::Release);
                                WorkerFailure::DecoderPanicked
                            });
                        drop(active);
                        let _ = reply.send(DecodeReply::Settled(settled));
                    }
                });
            if let Err(error) = thread {
                crate::config::log_debug(&format!(
                    "Could not start poster decoder {index}: {error}"
                ));
                record_failure(shared, WorkerFailure::DecoderUnavailable);
                return;
            }
            pool.push(send);
        }
        run_lane(
            shared,
            generation_clock,
            publish,
            maintenance,
            decoder,
            &pool,
        );
    });
}

fn run_lane(
    shared: &Shared,
    generation_clock: &AtomicU64,
    publish: impl Fn(PosterCompletion) -> bool,
    maintenance: Option<(&cache::Maintenance, u64)>,
    decoder: &PosterDecoder,
    pool: &[mpsc::Sender<DecodeTask>],
) {
    decoder.trace();
    let mut failed_stills = HashMap::<PathBuf, (Fingerprint, Arc<PosterDecodeError>)>::new();
    let mut failure_scope = None;
    let mut maintenance_due = maintenance.as_ref().map(|_| Instant::now());
    loop {
        let Some(work) = take_pending(shared, maintenance_due) else {
            return;
        };
        let batch = match work {
            Work::Posters(batch) => batch,
            Work::MaintainCache => {
                if let Some((cache, budget)) = &maintenance {
                    let complete = cache.run(*budget, || maintenance_idle(shared));
                    maintenance_due = Some(
                        Instant::now()
                            + if complete {
                                MAINTENANCE_INTERVAL
                            } else {
                                MAINTENANCE_QUIET
                            },
                    );
                }
                continue;
            }
            Work::Restore(batch) => {
                let mut cache = cache::ReadBatch::default();
                let candidates = batch.posters.len();
                decoder.begin_work();
                let groups = decoder.restore_batch(batch.posters, &mut cache, || {
                    !shared.shutdown.load(Ordering::Acquire)
                        && batch.epoch == shared.cache_epoch.load(Ordering::Acquire)
                });
                let opens = cache.open_attempts();
                drop(cache);
                trace_cache_batch("restore", candidates, opens);
                if groups < candidates {
                    crate::apply_trace::event_lazy("poster-restore-shared", || {
                        format!("slots={candidates}; sources={groups}")
                    });
                }
                decoder.trace();
                (batch.redraw)();
                continue;
            }
            Work::ClearCache(request) => {
                // The epoch has invalidated background work. Wait for its root guard
                // as well as the poster producers settled by this lane.
                if !wait_for_cache_barrier(shared) {
                    (request.acknowledge)(cache::Report {
                        epoch: request.epoch,
                        error: Some(
                            "Worker queue was poisoned before cache barrier; no clear acknowledged"
                                .into(),
                        ),
                        ..cache::Report::default()
                    });
                    return;
                }
                let mut report = cache::clear(
                    &request.profile,
                    &request.root,
                    request.names.as_deref(),
                    request.epoch,
                );
                if request.colour {
                    let colour = cache::clear_colour(&request.profile, request.epoch);
                    report.deleted += colour.deleted;
                    report.retained += colour.retained;
                    report.failed += colour.failed;
                    if let Some(e) = colour.error {
                        report.error = Some(format!(
                            "{}Colour cache: {e}",
                            report
                                .error
                                .map(|e| format!("Poster cache: {e}; "))
                                .unwrap_or_default()
                        ));
                    }
                }
                let poisoned = match shared.queue.lock() {
                    Ok(mut queue) => {
                        queue.cache_busy = false;
                        false
                    }
                    Err(error) => {
                        error.into_inner().cache_busy = false;
                        record_failure(shared, WorkerFailure::QueuePoisoned);
                        report.error = Some(format!(
                            "{}Worker queue was poisoned after cache operation",
                            report
                                .error
                                .map(|error| format!("{error}; "))
                                .unwrap_or_default()
                        ));
                        true
                    }
                };
                (request.acknowledge)(report);
                if poisoned {
                    return;
                }
                continue;
            }
        };
        let epoch = {
            let q = match shared.queue.lock() {
                Ok(queue) => queue,
                Err(_) => {
                    record_failure(shared, WorkerFailure::QueuePoisoned);
                    return;
                }
            };
            if q.cache_busy {
                continue;
            }
            shared.cache_epoch.load(Ordering::Acquire)
        };
        // Keep the cause for unreadable stills across adjacent demand changes.
        let scope = (epoch, &batch.cache_dir);
        if failure_scope.as_ref().map(|(e, p)| (*e, p)) != Some(scope) {
            failed_stills.clear();
            failure_scope = Some((epoch, batch.cache_dir.clone()));
        }
        decoder.begin_work();
        let mut cached_failures = Vec::new();
        let active = Arc::new(DecodeBatch::new(batch, epoch, |source| {
            let Some((still, (fingerprint, error))) = matching_still(source)
                .and_then(|still| Some((still, failed_stills.get(&still.path)?)))
            else {
                return true;
            };
            if *fingerprint != still.fingerprint || !still.is_current() {
                return true;
            }
            cached_failures.push(DecodeFailed::new(source, error));
            false
        }));
        let generation = active.batch.generation;
        let mut cursor = 0;
        let mut publish_delta = |posters: Vec<_>, failures: Vec<_>, finished| {
            cursor += posters.len() + failures.len() + usize::from(finished);
            if publish(PosterCompletion {
                generation,
                cursor,
                finished,
                posters,
                failures,
            }) {
                return true;
            }
            record_failure(shared, WorkerFailure::CompletionChannelClosed);
            false
        };
        if !active.is_current(shared, generation_clock) {
            continue;
        }
        if !cached_failures.is_empty() && !publish_delta(Vec::new(), cached_failures, false) {
            return;
        }
        let (reply, receive) = mpsc::sync_channel(pool.len());
        for thread in pool {
            if thread
                .send(DecodeTask {
                    active: Arc::clone(&active),
                    reply: reply.clone(),
                })
                .is_err()
            {
                active.failed.store(true, Ordering::Release);
                record_failure(shared, WorkerFailure::DecoderUnavailable);
                return;
            }
        }
        drop(reply);
        let mut cancelled = 0;
        let mut remaining = pool.len();
        while remaining != 0 {
            match receive.recv() {
                Ok(DecodeReply::Cancelled) => cancelled += 1,
                Ok(DecodeReply::Decoded(..)) if !active.is_current(shared, generation_clock) => {
                    cancelled += 1;
                }
                Ok(DecodeReply::Decoded(position, Ok(poster))) => {
                    for decoded in active.fan_out(position, poster, decoder) {
                        if !active.is_current(shared, generation_clock) {
                            cancelled += 1;
                            break;
                        }
                        if !publish_delta(vec![decoded], Vec::new(), false) {
                            active.failed.store(true, Ordering::Release);
                            return;
                        }
                    }
                }
                Ok(DecodeReply::Decoded(position, Err(error))) => {
                    let error = Arc::new(error);
                    let source = &active.batch.sources[position];
                    if let Some(still) = matching_still(source).filter(|still| {
                        !matches!(*error, PosterDecodeError::SourceChanged)
                            && still.is_current()
                            && active.is_current(shared, generation_clock)
                    }) {
                        if failed_stills.len() == gpu_gallery::MAX_ORDINARY_POSTERS {
                            failed_stills.clear();
                        }
                        failed_stills.insert(
                            still.path.clone(),
                            (still.fingerprint.clone(), Arc::clone(&error)),
                        );
                    }
                    let failures = std::iter::once(&position)
                        .chain(active.aliases.get(&position).into_iter().flatten())
                        .map(|&position| DecodeFailed::new(&active.batch.sources[position], &error))
                        .collect();
                    if active.is_current(shared, generation_clock)
                        && !publish_delta(Vec::new(), failures, false)
                    {
                        active.failed.store(true, Ordering::Release);
                        return;
                    }
                }
                Ok(DecodeReply::Settled(Ok(()))) => remaining -= 1,
                Ok(DecodeReply::Settled(Err(failure))) => {
                    record_failure(shared, failure);
                    return;
                }
                Err(_) => {
                    record_failure(shared, WorkerFailure::DecoderUnavailable);
                    return;
                }
            }
        }
        trace_cache_batch(
            "decode",
            active.batch.sources.len(),
            active.cache.open_attempts(),
        );
        active.trace(cancelled);
        let current = active.is_current(shared, generation_clock);
        drop(active);
        decoder.trace();
        if !current {
            continue;
        }
        if !publish_delta(Vec::new(), Vec::new(), true) {
            return;
        }
    }
}

fn matching_still(source: &PosterSource) -> Option<&crate::library::media_still::MediaStill> {
    source
        .still
        .as_ref()
        .filter(|still| still.item_id == source.item_id && still.path == source.path)
}

fn trace_cache_batch(kind: &str, candidates: usize, opens: usize) {
    crate::apply_trace::event_lazy("poster-cache-batch", || {
        format!("kind={kind}; candidates={candidates}; root_open_attempts={opens}")
    });
}

const GIB: u64 = 1024 * 1024 * 1024;

fn total_memory() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    // MEMORYSTATUSEX contains integers. The API requires its size in dwLength.
    let mut memory: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    memory.dwLength = std::mem::size_of_val(&memory) as u32;
    (unsafe { GlobalMemoryStatusEx(&mut memory) } != 0).then_some(memory.ullTotalPhys)
}

fn decode_pool_capacity() -> usize {
    if std::env::var("WALLPAPER_SWITCHER_POSTER_POOL_MIB").as_deref() == Ok("24")
        || total_memory().unwrap_or(0) < 8 * GIB
    {
        24
    } else {
        64
    }
}

fn physical_cores(affinity: usize) -> Option<usize> {
    use windows_sys::Win32::Foundation::{GetLastError, ERROR_INSUFFICIENT_BUFFER};
    use windows_sys::Win32::System::SystemInformation::{
        GetLogicalProcessorInformation, RelationProcessorCore, SYSTEM_LOGICAL_PROCESSOR_INFORMATION,
    };
    let mut bytes = 0;
    // This API describes the calling thread's processor group, as does the
    // affinity mask. The four-worker cap already saturates at six cores.
    if unsafe { GetLogicalProcessorInformation(std::ptr::null_mut(), &mut bytes) } != 0
        || unsafe { GetLastError() } != ERROR_INSUFFICIENT_BUFFER
    {
        return None;
    }
    let entry_size = std::mem::size_of::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>();
    let mut entries =
        Vec::<SYSTEM_LOGICAL_PROCESSOR_INFORMATION>::with_capacity(bytes as usize / entry_size);
    if bytes as usize % entry_size != 0
        || unsafe { GetLogicalProcessorInformation(entries.as_mut_ptr(), &mut bytes) } == 0
    {
        return None;
    }
    // On success, the API has initialized the returned byte count of records.
    unsafe { entries.set_len(bytes as usize / entry_size) };
    let cores = entries
        .iter()
        .filter(|entry| {
            entry.Relationship == RelationProcessorCore && entry.ProcessorMask & affinity != 0
        })
        .count();
    (cores != 0).then_some(cores)
}

fn automatic_decode_workers(logical: usize, cores: Option<usize>, memory: Option<u64>) -> usize {
    if memory.unwrap_or(0) < 4 * GIB {
        1
    } else if logical <= 4 {
        2
    } else {
        cores.map_or(2, |cores| cores.saturating_sub(2).clamp(1, 4))
    }
}

/// Share the Windows affinity-aware count with the catalog scanner. The standard
/// library can report the full host count even for an affinity-constrained process.
pub(crate) fn process_parallelism() -> (usize, usize) {
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessAffinityMask};
    let mut affinity = 0;
    let mut system_affinity = 0;
    let has_affinity = unsafe {
        GetProcessAffinityMask(GetCurrentProcess(), &mut affinity, &mut system_affinity) != 0
    } && affinity != 0;
    let logical = if has_affinity {
        affinity.count_ones() as usize
    } else {
        std::thread::available_parallelism().map_or(1, usize::from)
    };
    (logical, if has_affinity { affinity } else { usize::MAX })
}

fn decode_worker_count(override_threads: Option<u8>) -> usize {
    let (logical, affinity) = process_parallelism();
    let cores = physical_cores(affinity);
    let memory = total_memory();
    let workers = override_threads.map_or_else(
        || automatic_decode_workers(logical, cores, memory),
        |threads| usize::from(threads.clamp(1, 4)),
    );
    crate::config::log_debug(&format!(
        "Poster decode workers: {workers} (override={override_threads:?}, logical={logical}, physical={cores:?}, ram_bytes={memory:?})"
    ));
    workers
}

fn decode_source(
    source: &PosterSource,
    cache: &cache::WriteBatch,
    decoder: &PosterDecoder,
    wanted: &dyn Fn() -> bool,
) -> Result<Poster, PosterDecodeError> {
    if let Some(still) = &source.still {
        if still.item_id != source.item_id || still.path != source.path || !still.is_current() {
            return Err(PosterDecodeError::SourceChanged);
        }
        let decoded = gpu_gallery::decode_poster_cached(
            &still.path,
            source.layer,
            cache,
            Some(decoder),
            wanted,
        );
        if !still.is_current() {
            return Err(PosterDecodeError::SourceChanged);
        }
        return decoded;
    }
    match source.kind {
        PosterKind::Video => gpu_gallery::decode_video_poster(source, cache, decoder, wanted),
        PosterKind::Placeholder => gpu_gallery::placeholder_if_wanted(
            &source.path,
            source.layer,
            source.kind,
            Some(decoder),
            wanted,
        )
        .ok_or(PosterDecodeError::Cancelled),
        PosterKind::Still => Err(PosterDecodeError::MissingStill),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::mpsc::{self, TryRecvError};
    use std::sync::{Arc, Barrier};

    use super::{PosterBatch, PosterSource, PosterWorker, SubmitError, WorkerFailure};

    fn batch(generation: u64, name: &str) -> PosterBatch {
        PosterBatch::new(
            generation,
            vec![PosterSource {
                index: generation as usize,
                layer: 0,
                item_id: name.into(),
                kind: crate::poster_worker::PosterKind::Video,
                video_stamp: None,
                still: None,
                path: name.into(),
            }],
            std::path::PathBuf::from("unused-cache"),
        )
    }

    #[test]
    fn p36_poisoned_accepted_clear_reports_failure_without_deleting_cache() {
        use std::sync::atomic::AtomicU64;
        use std::time::Duration;
        let (profile, root) = super::cache::tests::fixture();
        let cached = root.join("poster-0123456789abcdef.rgba");
        std::fs::write(&cached, vec![9; crate::gpu_gallery::POSTER_BYTES]).unwrap();
        let shared = Arc::new(super::Shared::new());
        let (ack, receipt) = mpsc::channel();
        {
            let mut queue = shared.queue.lock().unwrap();
            queue.cache_busy = true;
            queue.background_active = true;
            queue.clear = Some(super::cache::Request {
                profile,
                root,
                names: None,
                epoch: 7,
                colour: false,
                acknowledge: Box::new(move |report| ack.send(report).unwrap()),
            });
        }
        let worker_shared = Arc::clone(&shared);
        let (done, finished) = mpsc::channel();
        let thread = std::thread::spawn(move || {
            {
                let _exit = super::WorkerExitGuard {
                    shared: &worker_shared,
                };
                let decoder = crate::gpu_gallery::PosterDecoder::with_producers(24, 1);
                super::run_lane(
                    &worker_shared,
                    &AtomicU64::new(1),
                    |_| true,
                    None,
                    &decoder,
                    &[],
                );
            }
            done.send(()).unwrap();
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while shared.queue.lock().unwrap().clear.is_some() {
            assert!(
                std::time::Instant::now() < deadline,
                "clear was not accepted"
            );
            std::thread::yield_now();
        }
        let poisoned = Arc::clone(&shared);
        assert!(std::thread::spawn(move || {
            let mut queue = poisoned.queue.lock().unwrap();
            queue.background_active = false;
            panic!("controlled cache-barrier poison");
        })
        .join()
        .is_err());
        shared.wake.notify_all();
        let report = receipt.recv_timeout(Duration::from_secs(5)).unwrap();
        finished.recv_timeout(Duration::from_secs(5)).unwrap();
        thread.join().unwrap();
        assert_eq!(report.epoch, 7);
        assert_eq!(report.deleted, 0);
        assert!(report.error.unwrap().contains("no clear acknowledged"));
        assert_eq!(
            std::fs::read(cached).unwrap(),
            vec![9; crate::gpu_gallery::POSTER_BYTES]
        );
        assert_eq!(
            *shared.failure.lock().unwrap(),
            Some(WorkerFailure::QueuePoisoned)
        );
        assert!(!shared.running.load(Ordering::Acquire));
        assert!(
            !shared
                .queue
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .cache_busy
        );
        assert!(receipt.try_recv().is_err(), "one acknowledgement only");
    }

    fn fake_decode(
        source: &PosterSource,
        _cache_dir: &std::path::Path,
    ) -> Option<crate::gpu_gallery::Poster> {
        Some(crate::gpu_gallery::video_placeholder_poster(
            &source.path,
            source.layer,
        ))
    }

    #[test]
    fn decode_worker_count_uses_ram_logical_threads_and_physical_cores() {
        use super::{automatic_decode_workers, GIB};

        assert_eq!(automatic_decode_workers(12, Some(6), Some(4 * GIB - 1)), 1);
        assert_eq!(automatic_decode_workers(12, Some(6), None), 1);
        assert_eq!(automatic_decode_workers(1, Some(1), Some(4 * GIB)), 2);
        assert_eq!(automatic_decode_workers(4, Some(4), Some(4 * GIB)), 2);
        assert_eq!(automatic_decode_workers(6, Some(3), Some(8 * GIB)), 1);
        assert_eq!(automatic_decode_workers(10, Some(5), Some(8 * GIB)), 3);
        assert_eq!(automatic_decode_workers(16, Some(8), Some(8 * GIB)), 4);
        assert_eq!(automatic_decode_workers(8, None, Some(8 * GIB)), 2);
    }

    fn named_sources(generation: u64, names: &[&str]) -> PosterBatch {
        PosterBatch::new(
            generation,
            names
                .iter()
                .enumerate()
                .map(|(index, name)| PosterSource {
                    index,
                    layer: index as u32,
                    item_id: (*name).into(),
                    path: (*name).into(),
                    video_stamp: None,
                    still: None,
                    kind: crate::poster_worker::PosterKind::Video,
                })
                .collect(),
            "unused-cache".into(),
        )
    }

    #[test]
    fn decode_queue_claims_visible_first_and_filters_unwanted_work() {
        use super::{DecodeBatch, Shared};
        use std::sync::atomic::AtomicU64;

        let mut request = named_sources(
            7,
            &[
                "backfill",
                "visible-new",
                "excluded",
                "visible-old",
                "other-backfill",
            ],
        );
        request.visible = [1, 2, 3].into_iter().collect();
        let active = DecodeBatch::new(request, 0, |source| source.index != 2);
        let clock = AtomicU64::new(7);
        let shared = Shared::new();
        assert_eq!(active.claim(&shared, &clock), Some(1));
        assert_eq!(active.claim(&shared, &clock), Some(3));
        assert_eq!(active.claim(&shared, &clock), Some(0));
        clock.store(8, Ordering::Release);
        assert_eq!(active.claim(&shared, &clock), None);
        assert_eq!(active.jobs.lock().unwrap().backfill.front(), Some(&4));
    }

    #[test]
    fn shared_gif_publishes_each_slot_early_and_reuses_only_current_pins() {
        use crate::library::media_still::{MediaStill, Provenance};
        use std::sync::atomic::AtomicU64;
        use std::time::Duration;

        const TIMEOUT: Duration = Duration::from_secs(10);
        let (profile, root) = super::cache::tests::fixture();
        let gif = profile.join("shared.gif");
        let save = |size, pixel| {
            image::RgbaImage::from_pixel(size, size, image::Rgba(pixel))
                .save(&gif)
                .unwrap();
        };
        save(2, [20, 90, 150, 255]);
        let source = |index: usize, revision| {
            let item_id = format!("item-{index}");
            PosterSource {
                index,
                layer: index as u32,
                video_stamp: None,
                still: Some(MediaStill {
                    item_id: item_id.clone(),
                    catalog_revision: revision,
                    path: gif.clone(),
                    fingerprint: super::Fingerprint::capture(&gif).unwrap(),
                    provenance: Provenance::ExistingPoster,
                }),
                path: gif.clone(),
                kind: crate::poster_worker::PosterKind::Still,
                item_id,
            }
        };
        let first = source(0, 1);
        let second = source(1, 1);
        let mut third = named_sources(1, &["held.mp4"]).sources.remove(0);
        third.index = 2;
        third.layer = 2;

        let clock = Arc::new(AtomicU64::new(1));
        let (started_tx, started_rx) = mpsc::channel();
        let (release, held) = mpsc::channel();
        let held = std::sync::Mutex::new(held);
        let (completion_tx, completion_rx) = mpsc::channel();
        let worker = PosterWorker::spawn_with_runner(
            std::thread::Builder::new(),
            clock.clone(),
            move |completion| completion_tx.send(completion).is_ok(),
            |_| {},
            move |source, cache, decoder, wanted| {
                started_tx.send(source.index).unwrap();
                if source.index == 2 {
                    held.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
                }
                super::decode_source(source, cache, decoder, wanted)
            },
            None,
            2,
        )
        .unwrap();
        let mut batch = PosterBatch::new(1, vec![first.clone(), second, third], root.clone());
        batch.visible = [1, 2].into_iter().collect();
        worker.submit(batch).unwrap();
        let take = || completion_rx.recv_timeout(TIMEOUT).unwrap();
        let mut started = [
            started_rx.recv_timeout(TIMEOUT).unwrap(),
            started_rx.recv_timeout(TIMEOUT).unwrap(),
        ];
        started.sort_unstable();
        assert_eq!(started, [1, 2]);
        let expect_slot = |completion: super::PosterCompletion, cursor, index| {
            assert_eq!(completion.cursor, cursor);
            assert!(!completion.finished);
            assert_eq!(completion.posters.len(), 1);
            let decoded = completion.posters.into_iter().next().unwrap();
            assert_eq!(decoded.source.index, index);
            assert_eq!(decoded.source.item_id, format!("item-{index}"));
            assert_eq!(decoded.poster.layer, index as u32);
            decoded.poster
        };
        let live = expect_slot(take(), 1, 1);
        let pin = expect_slot(take(), 2, 0);
        assert!(!pin.same_resource(&live));
        assert!(pin.has_pixels() && live.has_pixels());
        assert!(matches!(completion_rx.try_recv(), Err(TryRecvError::Empty)));
        release.send(()).unwrap();
        assert_eq!(take().posters[0].source.index, 2);
        assert!(take().finished);
        assert!(matches!(started_rx.try_recv(), Err(TryRecvError::Empty)));

        clock.store(2, Ordering::Release);
        let mut batch = PosterBatch::new(2, vec![source(3, 2)], root.clone());
        batch.reusable.insert(pin.layer, pin.clone());
        worker.submit(batch).unwrap();
        assert!(expect_slot(take(), 1, 3).has_pixels());
        assert!(take().finished);
        assert!(matches!(started_rx.try_recv(), Err(TryRecvError::Empty)));

        save(7, [200, 30, 20, 255]);
        let changed = source(4, 3);
        assert_ne!(
            first.still.as_ref().unwrap().fingerprint,
            changed.still.as_ref().unwrap().fingerprint
        );
        clock.store(3, Ordering::Release);
        let mut batch = PosterBatch::new(3, vec![first, changed.clone(), source(5, 3)], root);
        batch.reusable.insert(pin.layer, pin.clone());
        worker.submit(batch).unwrap();
        let mut ready = Vec::new();
        let mut failures = 0;
        for cursor in 1..=3 {
            let delta = take();
            assert_eq!((delta.cursor, delta.finished), (cursor, false));
            if delta.failures.is_empty() {
                let index = 4 + ready.len();
                ready.push(expect_slot(delta, cursor, index));
            } else {
                assert!(delta.posters.is_empty());
                assert_eq!(delta.failures.len(), 1);
                assert_eq!(delta.failures[0].source.index, 0);
                assert!(matches!(
                    *delta.failures[0].error,
                    super::PosterDecodeError::SourceChanged
                ));
                failures += 1;
            }
        }
        assert_eq!((ready.len(), failures), (2, 1));
        let updated = &ready[0];
        assert!(updated.matches_source(super::PosterKind::Still, changed.fingerprint(),));
        assert!(!pin.matches_source(super::PosterKind::Still, changed.fingerprint(),));
        let terminal = take();
        assert_eq!((terminal.cursor, terminal.finished), (4, true));
        let mut started = [
            started_rx.try_recv().unwrap(),
            started_rx.try_recv().unwrap(),
        ];
        started.sort_unstable();
        assert_eq!(started, [0, 4]);
        assert!(matches!(started_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn claimed_decode_reports_cancellation_before_and_after_the_decoder() {
        use super::{DecodeBatch, DecodeReply, PosterDecoder, Shared};
        use std::sync::atomic::AtomicU64;

        let clock = AtomicU64::new(7);
        let shared = Shared::new();
        let active = DecodeBatch::new(named_sources(7, &["obsolete", "held"]), 0, |_| true);
        let decoder = PosterDecoder::new(2);
        let obsolete = active.claim(&shared, &clock).unwrap();
        let held = active.claim(&shared, &clock).unwrap();
        let result = active.decode_claim(held, &shared, &clock, &decoder, |_, _, _, wanted| {
            assert!(wanted());
            clock.store(8, Ordering::Release);
            assert!(!wanted());
            Err(super::PosterDecodeError::InvalidPixels)
        });
        assert!(matches!(result, DecodeReply::Cancelled));
        let result = active.decode_claim(obsolete, &shared, &clock, &decoder, |_, _, _, _| {
            panic!("obsolete claim reached the decoder")
        });
        assert!(matches!(result, DecodeReply::Cancelled));
    }

    #[test]
    fn cancelled_failure_does_not_suppress_the_same_still_in_the_latest_batch() {
        use crate::library::media_still::MediaStill;
        use std::sync::atomic::AtomicU64;
        use std::time::Duration;

        const TIMEOUT: Duration = Duration::from_secs(5);
        let (profile, root) = super::cache::tests::fixture();
        image::RgbaImage::new(2, 2)
            .save(profile.join("still.png"))
            .unwrap();
        let item = crate::scanner::scan_local_images(&[profile]).remove(0);
        let still = MediaStill::select(&item, 1).unwrap();
        let source = PosterSource {
            index: 0,
            layer: 0,
            item_id: item.id,
            path: still.path.clone(),
            kind: crate::poster_worker::PosterKind::Still,
            video_stamp: None,
            still: Some(still),
        };
        let clock = Arc::new(AtomicU64::new(1));
        let (started, entered) = mpsc::channel();
        let (release, held) = mpsc::channel();
        let held = std::sync::Mutex::new(held);
        let (post, completion) = mpsc::channel();
        let attempts = AtomicUsize::new(0);
        let worker = PosterWorker::spawn_with_runner(
            std::thread::Builder::new(),
            Arc::clone(&clock),
            move |result| post.send(result).is_ok(),
            |_| {},
            move |source, cache, decoder, wanted| {
                if attempts.fetch_add(1, Ordering::Relaxed) == 0 {
                    started.send(()).unwrap();
                    held.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
                    assert!(!wanted());
                    return Err(super::PosterDecodeError::InvalidPixels);
                }
                super::decode_source(source, cache, decoder, wanted)
            },
            None,
            1,
        )
        .unwrap();
        worker
            .submit(PosterBatch::new(1, vec![source.clone()], root.clone()))
            .unwrap();
        entered.recv_timeout(TIMEOUT).unwrap();
        clock.store(2, Ordering::Release);
        let submitted = worker.submit(PosterBatch::new(2, vec![source], root));
        release.send(()).unwrap();
        submitted.unwrap();

        let poster = completion.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(
            (poster.generation, poster.cursor, poster.finished),
            (2, 1, false)
        );
        assert_eq!(poster.posters.len(), 1);
        let terminal = completion.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(
            (terminal.generation, terminal.cursor, terminal.finished),
            (2, 2, true)
        );
        assert!(matches!(completion.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn parallel_workers_settle_active_claims_and_publish_only_the_latest_batch() {
        use std::sync::{atomic::AtomicU64, Mutex};
        use std::time::Duration;

        const TIMEOUT: Duration = Duration::from_secs(5);
        let clock = Arc::new(AtomicU64::new(1));
        let (started_tx, started_rx) = mpsc::channel();
        let (release, held) = mpsc::channel();
        let held = Mutex::new(held);
        let (completion_tx, completion_rx) = mpsc::channel();
        let worker = PosterWorker::spawn_with_runner(
            std::thread::Builder::new(),
            clock.clone(),
            move |completion| completion_tx.send(completion).is_ok(),
            |_| {},
            move |source, cache, decoder, wanted| {
                started_tx.send(source.item_id.clone()).unwrap();
                if matches!(source.item_id.as_str(), "first" | "second") {
                    held.lock().unwrap().recv_timeout(TIMEOUT).unwrap();
                }
                super::decode_source(source, cache, decoder, wanted)
            },
            None,
            2,
        )
        .unwrap();

        worker
            .submit(named_sources(1, &["first", "second", "obsolete-active"]))
            .unwrap();
        let mut started = [
            started_rx.recv_timeout(TIMEOUT).unwrap(),
            started_rx.recv_timeout(TIMEOUT).unwrap(),
        ];
        started.sort();
        assert_eq!(started, ["first", "second"]);
        assert!(matches!(completion_rx.try_recv(), Err(TryRecvError::Empty)));
        clock.store(2, Ordering::Release);
        worker
            .submit(named_sources(2, &["replaced-pending"]))
            .unwrap();
        clock.store(3, Ordering::Release);
        worker
            .submit(named_sources(3, &["newest", "neighbor"]))
            .unwrap();
        release.send(()).unwrap();
        release.send(()).unwrap();

        let mut items = Vec::new();
        for cursor in 1..=3 {
            let complete = completion_rx.recv_timeout(TIMEOUT).unwrap();
            assert_eq!(complete.generation, 3);
            assert_eq!(complete.cursor, cursor);
            assert_eq!(complete.finished, cursor == 3);
            items.extend(complete.posters.into_iter().map(|p| p.source.item_id));
        }
        items.sort();
        assert_eq!(items, ["neighbor", "newest"]);
        let mut started: Vec<_> = started_rx.try_iter().collect();
        started.sort();
        assert_eq!(started, ["neighbor", "newest"]);
        assert!(matches!(completion_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn due_maintenance_waits_for_queued_posters_and_a_quiet_period() {
        use super::{maintenance_idle, take_pending, Shared, Work, MAINTENANCE_QUIET};
        use std::time::Instant;

        let shared = Shared::new();
        shared.queue.lock().unwrap().pending = Some(Work::Posters(batch(1, "visible")));
        let due = Some(Instant::now());
        assert!(!maintenance_idle(&shared));
        assert!(matches!(take_pending(&shared, due), Some(Work::Posters(_))));
        let idle_since = Instant::now();
        assert!(matches!(
            take_pending(&shared, due),
            Some(Work::MaintainCache)
        ));
        assert!(idle_since.elapsed() >= MAINTENANCE_QUIET);
    }

    #[test]
    fn worker_startup_failure_is_reported_without_panic() {
        let result = super::PosterWorker::spawn_with_builder(
            std::thread::Builder::new().stack_size(usize::MAX),
            Arc::new(std::sync::atomic::AtomicU64::new(1)),
            |_| true,
            |_, _| None,
        );

        assert!(result.is_err());
    }

    #[test]
    fn one_active_batch_and_one_pending_latest_batch_are_bounded() {
        let generation_clock = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let (started_tx, started_rx) = mpsc::channel();
        let release = Arc::new(Barrier::new(2));
        let active = Arc::new(AtomicUsize::new(0));
        let maximum_active = Arc::new(AtomicUsize::new(0));
        let decoded = Arc::new(AtomicUsize::new(0));
        let (completion_tx, completion_rx) = mpsc::channel();

        let decoder_release = Arc::clone(&release);
        let decoder_active = Arc::clone(&active);
        let decoder_maximum = Arc::clone(&maximum_active);
        let decoder_count = Arc::clone(&decoded);
        let worker = super::PosterWorker::spawn_with_builder(
            std::thread::Builder::new().name("test-poster-worker".into()),
            Arc::clone(&generation_clock),
            move |completion| completion_tx.send(completion).is_ok(),
            move |source, _| {
                let now = decoder_active.fetch_add(1, Ordering::AcqRel) + 1;
                decoder_maximum.fetch_max(now, Ordering::AcqRel);
                decoder_count.fetch_add(1, Ordering::AcqRel);
                if source.path == std::path::Path::new("first") {
                    started_tx.send(()).unwrap();
                    decoder_release.wait();
                }
                let poster = Some(crate::gpu_gallery::video_placeholder_poster(
                    &source.path,
                    source.layer,
                ));
                decoder_active.fetch_sub(1, Ordering::AcqRel);
                poster
            },
        )
        .unwrap();

        worker.submit(batch(1, "first")).unwrap();
        started_rx.recv().unwrap();

        generation_clock.store(2, Ordering::Release);
        worker.submit(batch(2, "second")).unwrap();
        generation_clock.store(3, Ordering::Release);
        worker.submit(batch(3, "third")).unwrap();
        assert_eq!(worker.pending_count(), 1);

        release.wait();
        let completion = completion_rx.recv().unwrap();
        assert_eq!(completion.generation, 3);
        assert_eq!(decoded.load(Ordering::Acquire), 2);
        assert_eq!(maximum_active.load(Ordering::Acquire), 1);
        let terminal = completion_rx.recv().unwrap();
        assert_eq!(terminal.generation, 3);
        assert!(terminal.finished && terminal.posters.is_empty());
        assert!(matches!(completion_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn stale_active_work_is_cancelled_and_reopen_can_publish_latest_generation() {
        let generation_clock = Arc::new(std::sync::atomic::AtomicU64::new(10));
        let (started_tx, started_rx) = mpsc::channel();
        let release = Arc::new(Barrier::new(2));
        let (completion_tx, completion_rx) = mpsc::channel();
        let decoder_release = Arc::clone(&release);
        let worker = super::PosterWorker::spawn_with_runner(
            std::thread::Builder::new().name("test-poster-worker-reopen".into()),
            Arc::clone(&generation_clock),
            move |completion| completion_tx.send(completion).is_ok(),
            |_| {},
            move |source, cache_dir, decoder, wanted| {
                let poster = super::decode_source(source, cache_dir, decoder, wanted);
                if source.path == std::path::Path::new("hidden") {
                    started_tx.send(()).unwrap();
                    decoder_release.wait();
                }
                poster
            },
            None,
            1,
        )
        .unwrap();

        worker.submit(batch(10, "hidden")).unwrap();
        started_rx.recv().unwrap();
        generation_clock.store(11, Ordering::Release);
        worker.cancel_pending();
        release.wait();

        assert!(matches!(
            completion_rx.try_recv(),
            Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected)
        ));
        worker.submit(batch(11, "reopened")).unwrap();
        let completion = completion_rx.recv().unwrap();
        assert_eq!(completion.generation, 11);
        assert_eq!(completion.posters.len(), 1);
    }

    #[test]
    fn completion_channel_failure_is_reported_and_replacement_can_retry() {
        let generation_clock = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let worker = super::PosterWorker::spawn_with_builder(
            std::thread::Builder::new().name("test-poster-worker-failure".into()),
            Arc::clone(&generation_clock),
            |_| false,
            fake_decode,
        )
        .unwrap();

        worker.submit(batch(1, "failure")).unwrap();
        worker.wait_until_stopped();
        assert_eq!(
            worker.unavailable_reason(),
            Some(WorkerFailure::CompletionChannelClosed)
        );
        assert!(matches!(
            worker.submit(batch(1, "retry")),
            Err(SubmitError::WorkerUnavailable(
                WorkerFailure::CompletionChannelClosed
            ))
        ));
        drop(worker);

        let (completion_tx, completion_rx) = mpsc::channel();
        let replacement = PosterWorker::spawn(Arc::clone(&generation_clock), move |completion| {
            completion_tx.send(completion).is_ok()
        })
        .unwrap();
        replacement.submit(batch(1, "retry")).unwrap();
        assert_eq!(completion_rx.recv().unwrap().generation, 1);
    }

    #[test]
    fn failed_decode_publishes_once_without_an_automatic_retry_loop() {
        let generation_clock = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let (completion_tx, completion_rx) = mpsc::channel();
        let worker = super::PosterWorker::spawn_with_builder(
            std::thread::Builder::new().name("test-poster-worker-decode-failure".into()),
            generation_clock,
            move |completion| completion_tx.send(completion).is_ok(),
            |_, _| None,
        )
        .unwrap();

        worker.submit(batch(1, "missing-or-corrupt")).unwrap();
        let failure = completion_rx.recv().unwrap();
        assert_eq!((failure.cursor, failure.finished), (1, false));
        assert!(failure.posters.is_empty());
        assert_eq!(failure.failures.len(), 1);
        assert_eq!(failure.failures[0].source.item_id, "missing-or-corrupt");
        let completion = completion_rx.recv().unwrap();
        assert_eq!((completion.cursor, completion.finished), (2, true));
        assert!(completion.posters.is_empty() && completion.failures.is_empty());
        assert!(matches!(completion_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn p32_corrupt_still_reports_each_alias_and_retries_after_epoch_or_stamp_change() {
        use super::{cache, Fingerprint, PosterDecodeError, PosterTier};
        use crate::library::media_still::{MediaStill, Provenance};
        use std::sync::atomic::AtomicU64;
        use std::time::Duration;

        const TIMEOUT: Duration = Duration::from_secs(10);
        let (profile, root) = cache::tests::fixture();
        let corrupt = profile.join("corrupt.png");
        let healthy = profile.join("healthy.png");
        std::fs::write(&corrupt, b"broken png").unwrap();
        image::RgbaImage::new(2, 2).save(&healthy).unwrap();
        let sources = |generation| {
            [&corrupt, &corrupt, &healthy]
                .into_iter()
                .enumerate()
                .map(|(index, path)| {
                    let item_id = format!("item-{index}");
                    PosterSource {
                        index,
                        layer: index as u32,
                        video_stamp: None,
                        still: Some(MediaStill {
                            item_id: item_id.clone(),
                            catalog_revision: generation,
                            path: path.clone(),
                            fingerprint: Fingerprint::capture(path).unwrap(),
                            provenance: Provenance::Image,
                        }),
                        item_id,
                        kind: crate::poster_worker::PosterKind::Still,
                        path: path.clone(),
                    }
                })
                .collect()
        };
        let clock = Arc::new(AtomicU64::new(1));
        let attempts = Arc::new(AtomicUsize::new(0));
        let decode_attempts = Arc::clone(&attempts);
        let corrupt_path = corrupt.clone();
        let (publish, completions) = mpsc::channel();
        let worker = PosterWorker::spawn_with_runner(
            std::thread::Builder::new(),
            Arc::clone(&clock),
            move |completion| publish.send(completion).is_ok(),
            |_| {},
            move |source, cache, decoder, wanted| {
                if source.path == corrupt_path {
                    decode_attempts.fetch_add(1, Ordering::Relaxed);
                }
                super::decode_source(source, cache, decoder, wanted)
            },
            None,
            1,
        )
        .unwrap();
        let submit = |generation| {
            clock.store(generation, Ordering::Release);
            worker
                .submit(PosterBatch::new(
                    generation,
                    sources(generation),
                    root.clone(),
                ))
                .unwrap();
        };
        let mut previous_cause = None;
        for (generation, attempt_count) in [(1, 1), (2, 1), (3, 2)] {
            if generation == 3 {
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
                let report = receipt.recv_timeout(TIMEOUT).unwrap();
                assert_eq!(report.epoch, epoch);
                assert!(report.error.is_none());
            }
            submit(generation);
            let failure = completions.recv_timeout(TIMEOUT).unwrap();
            assert_eq!(
                (failure.generation, failure.cursor, failure.finished),
                (generation, 2, false)
            );
            assert!(failure.posters.is_empty());
            assert_eq!(failure.failures.len(), 2);
            for (index, failed) in failure.failures.iter().enumerate() {
                assert_eq!(failed.source.index, index);
                assert_eq!(failed.source.layer, index as u32);
                assert_eq!(failed.source.item_id, format!("item-{index}"));
                assert_eq!(failed.tiers, [PosterTier::Far, PosterTier::Near]);
                assert!(matches!(*failed.error, PosterDecodeError::Dimensions(_)));
                assert!(Arc::ptr_eq(&failed.error, &failure.failures[0].error));
            }
            let cause = &failure.failures[0].error;
            if let Some(previous) = previous_cause.replace(Arc::clone(cause)) {
                assert_eq!(Arc::ptr_eq(&previous, cause), generation == 2);
            }
            let ready = completions.recv_timeout(TIMEOUT).unwrap();
            assert_eq!(ready.posters[0].source.index, 2);
            assert_eq!(
                (ready.generation, ready.cursor, ready.finished),
                (generation, 3, false)
            );
            let terminal = completions.recv_timeout(TIMEOUT).unwrap();
            assert_eq!(
                (terminal.generation, terminal.cursor, terminal.finished),
                (generation, 4, true)
            );
            assert!(terminal.failures.is_empty());
            assert_eq!(attempts.load(Ordering::Relaxed), attempt_count);
        }

        std::fs::copy(&healthy, &corrupt).unwrap();
        submit(4);
        for index in 0..3 {
            let ready = completions.recv_timeout(TIMEOUT).unwrap();
            assert!(ready.failures.is_empty());
            assert_eq!(ready.posters[0].source.index, index);
        }
        let terminal = completions.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(
            (terminal.generation, terminal.cursor, terminal.finished),
            (4, 4, true)
        );
        assert!(terminal.failures.is_empty());
        assert_eq!(attempts.load(Ordering::Relaxed), 3);
    }
    #[test]
    fn w4_cache_barrier_held_decode_stale_epoch_and_regeneration() {
        use super::cache;
        let (profile, root) = cache::tests::fixture();
        let doomed = cache::tests::poster(&root, "poster-0123456789abcdef.rgba");
        let (started_tx, started_rx) = mpsc::channel();
        let release = Arc::new(Barrier::new(2));
        let held = release.clone();
        let (post_tx, post_rx) = mpsc::channel();
        let writes = root.clone();
        let clock = Arc::new(std::sync::atomic::AtomicU64::new(1));
        let worker = PosterWorker::spawn_with_builder(
            std::thread::Builder::new().name("w4-held-decode".into()),
            clock.clone(),
            move |c| post_tx.send(c).is_ok(),
            move |source, _| {
                if source.item_id == "held" {
                    started_tx.send(()).unwrap();
                    held.wait();
                    cache::tests::poster(&writes, "poster-aaaaaaaaaaaaaaaa.rgba");
                }
                fake_decode(source, &writes)
            },
        )
        .unwrap();
        worker.submit(batch(1, "held")).unwrap();
        started_rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        let (ack_tx, ack_rx) = mpsc::channel();
        let epoch = worker
            .request_cache(cache::Request {
                profile: profile.clone(),
                root: root.clone(),
                names: None,
                epoch: 0,
                colour: false,
                acknowledge: Box::new(move |r| {
                    ack_tx.send(r).unwrap();
                }),
            })
            .unwrap();
        assert!(worker.cache_busy());
        assert!(matches!(
            worker.submit(batch(1, "blocked")),
            Err(SubmitError::CacheBusy)
        ));
        assert!(doomed.exists());
        assert!(ack_rx.try_recv().is_err());
        release.wait();
        let report = ack_rx
            .recv_timeout(std::time::Duration::from_secs(10))
            .unwrap();
        assert_eq!(report.epoch, epoch);
        eprintln!("W4 acknowledged cache receipt: {report:?}");
        assert_eq!(report.deleted, 2);
        assert_eq!(report.failed, 0);
        assert!(report.error.is_none());
        assert!(!doomed.exists());
        assert!(
            post_rx.try_recv().is_err(),
            "pre-barrier completion must be obsolete"
        );
        worker.submit(batch(1, "regenerated")).unwrap();
        let result = post_rx
            .recv_timeout(std::time::Duration::from_secs(3))
            .unwrap();
        assert_eq!(result.posters[0].source.item_id, "regenerated");
    }
    #[test]
    fn w4_existing_gif_poster_uses_shared_still_and_rejects_changed_fingerprint() {
        let (profile, root) = super::cache::tests::fixture();
        let gif = profile.join("preview.gif");
        image::RgbaImage::from_pixel(2, 2, image::Rgba([19, 200, 31, 255]))
            .save(&gif)
            .unwrap();
        // Reuse the scanner's item shape without admitting GIF to the local catalog policy.
        let image = profile.join("owned.png");
        image::RgbaImage::new(2, 2).save(&image).unwrap();
        let mut wallpaper = crate::scanner::scan_local_images(&[profile.clone()]).remove(0);
        wallpaper.source = crate::scanner::WallpaperSource::WallpaperEngine;
        wallpaper.file_target = profile.join("scene.pkg");
        wallpaper.preview_path = Some(gif.clone());
        let still = crate::library::media_still::MediaStill::select(&wallpaper, 17).unwrap();
        let source = PosterSource {
            index: 0,
            layer: 0,
            item_id: wallpaper.id.clone(),
            kind: crate::poster_worker::PosterKind::Still,
            path: gif.clone(),
            video_stamp: None,
            still: Some(still),
        };
        let mut decoder = super::PosterDecoder::new(2);
        let cache = super::cache::WriteBatch::new(&root);
        assert!(
            super::decode_source(&source, &cache, &mut decoder, &|| true)
                .unwrap()
                .has_pixels()
        );
        std::fs::write(&gif, b"changed").unwrap();
        assert!(matches!(
            super::decode_source(&source, &cache, &mut decoder, &|| true),
            Err(super::PosterDecodeError::SourceChanged)
        ));
    }
}

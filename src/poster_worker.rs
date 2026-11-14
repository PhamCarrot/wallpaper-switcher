#[path = "poster_worker/cache.rs"]
pub mod cache;
#[path = "poster_worker/colour.rs"]
pub mod colour;

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use crate::gpu_gallery::{self, Poster};

#[derive(Debug, Clone)]
pub struct PosterSource {
    pub index: usize,
    pub layer: u32,
    pub item_id: String,
    pub is_video: bool,
    pub still: Option<crate::library::media_still::MediaStill>,
    pub path: PathBuf,
}

#[derive(Debug)]
pub struct PosterBatch {
    pub generation: u64,
    pub sources: Vec<PosterSource>,
    pub cache_dir: PathBuf,
}

impl PosterBatch {
    pub fn new(generation: u64, sources: Vec<PosterSource>, cache_dir: PathBuf) -> Self {
        Self {
            generation,
            sources,
            cache_dir,
        }
    }
}

#[derive(Debug)]
pub struct PosterCompletion {
    pub generation: u64,
    pub posters: Vec<DecodedPoster>,
}

/// The callback and GUI share one retained completion and one notification
/// marker. Never put decoded RGBA batches in the unbounded event queue.
#[derive(Debug, Default)]
pub struct PosterMailbox {
    state: Mutex<CompletionState>,
}

#[derive(Debug, Default)]
struct CompletionState {
    generation: u64,
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
        state.expected = expected;
        state.capacity = capacity;
        state.completed = None;
        // Preserve an already queued wake: a new matching completion can reuse
        // it, and take() coordinates clearing that marker with producers.
        Ok(())
    }

    pub fn publish(
        &self,
        completed: PosterCompletion,
        notify: impl FnOnce() -> bool,
    ) -> Result<(), WorkerFailure> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| WorkerFailure::QueuePoisoned)?;
        if completed.generation != state.generation
            || state.expected.is_empty()
            || completed.posters.len() > state.capacity
            || completed
                .posters
                .iter()
                .any(|p| p.source.layer as usize >= state.capacity)
            || completed.posters.iter().enumerate().any(|(i, poster)| {
                completed.posters[..i].iter().any(|prior| {
                    prior.source.layer == poster.source.layer
                        || prior.source.index == poster.source.index
                })
            })
            || completed.posters.iter().any(|decoded| {
                !state.expected.iter().any(|source| {
                    source.index == decoded.source.index
                        && source.layer == decoded.source.layer
                        && source.item_id == decoded.source.item_id
                        && source.path == decoded.source.path
                        && source.is_video == decoded.source.is_video
                        && source.still == decoded.source.still
                        && decoded.poster.source == source.path
                        && decoded.poster.layer == source.layer
                })
            })
        {
            return Ok(());
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
            is_video: true,
            still: None,
            path: PathBuf::from(id),
        }
    }
    fn completion(generation: u64, id: &str) -> PosterCompletion {
        let source = source(id);
        PosterCompletion {
            generation,
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
        mailbox
            .publish(completion(9, "other"), || panic!("wrong ID/path woke GUI"))
            .unwrap();
        let mut wrong = completion(9, "current");
        wrong.posters[0].source.is_video = false;
        mailbox.publish(wrong, || panic!()).unwrap();
        let mut wrong = completion(9, "current");
        wrong.posters[0].source.index = 0;
        mailbox.publish(wrong, || panic!()).unwrap();
        let mut wrong = completion(9, "current");
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
                producer
                    .publish(completion(1, "same"), || {
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
                let ok = output
                    .publish(completion, || {
                        count.fetch_add(1, Ordering::AcqRel);
                        true
                    })
                    .is_ok();
                done_tx.send(g).unwrap();
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
    QueuePoisoned,
    WorkerExited,
}

impl fmt::Display for WorkerFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::CompletionChannelClosed => "poster completion channel closed",
            Self::DecoderPanicked => "poster decoder failed unexpectedly",
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
}

impl fmt::Display for SubmitError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CacheBusy => formatter.write_str("poster cache operation is still settling"),
            Self::Closed => formatter.write_str("poster worker is shutting down"),
            Self::WorkerUnavailable(failure) => write!(formatter, "{failure}"),
            Self::QueuePoisoned => formatter.write_str("poster worker queue was poisoned"),
        }
    }
}

enum Work {
    Posters(PosterBatch),
    ClearCache(cache::Request),
    Colour(colour::Job),
}
struct QueueState {
    pending: Option<Work>,
    clear: Option<cache::Request>,
    colour: Option<colour::Job>,
    cache_busy: bool,
}

struct Shared {
    queue: Mutex<QueueState>,
    wake: Condvar,
    cache_epoch: AtomicU64,
    shutdown: AtomicBool,
    running: AtomicBool,
    failure: Mutex<Option<WorkerFailure>>,
}

impl Shared {
    fn new() -> Self {
        Self {
            queue: Mutex::new(QueueState {
                pending: None,
                clear: None,
                colour: None,
                cache_busy: false,
            }),
            wake: Condvar::new(),
            cache_epoch: AtomicU64::new(0),
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
    pub fn spawn<P>(generation_clock: Arc<AtomicU64>, publish: P) -> Result<Self, SpawnError>
    where
        P: Fn(PosterCompletion) -> bool + Send + Sync + 'static,
    {
        Self::spawn_with_builder(
            std::thread::Builder::new().name("gpu-poster-decode".into()),
            generation_clock,
            publish,
            decode_source,
        )
    }

    fn spawn_with_builder<P, D>(
        builder: std::thread::Builder,
        generation_clock: Arc<AtomicU64>,
        publish: P,
        decode: D,
    ) -> Result<Self, SpawnError>
    where
        P: Fn(PosterCompletion) -> bool + Send + Sync + 'static,
        D: Fn(&PosterSource, &Path) -> Option<Poster> + Send + Sync + 'static,
    {
        Self::spawn_with_runner(builder, generation_clock, publish, decode)
    }

    fn spawn_with_runner<P, D>(
        builder: std::thread::Builder,
        generation_clock: Arc<AtomicU64>,
        publish: P,
        decode: D,
    ) -> Result<Self, SpawnError>
    where
        P: Fn(PosterCompletion) -> bool + Send + Sync + 'static,
        D: Fn(&PosterSource, &Path) -> Option<Poster> + Send + Sync + 'static,
    {
        let shared = Arc::new(Shared::new());
        let worker_shared = Arc::clone(&shared);
        builder
            .spawn(move || {
                let _exit_guard = WorkerExitGuard {
                    shared: Arc::clone(&worker_shared),
                };
                run_worker(worker_shared, generation_clock, publish, decode);
            })
            .map_err(SpawnError)?;
        Ok(Self { shared })
    }

    pub fn submit(&self, batch: PosterBatch) -> Result<(), SubmitError> {
        self.submit_work(Work::Posters(batch))
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
        self.shared.wake.notify_one();
        Ok(())
    }

    pub fn submit_colour(&self, mut job: colour::Job) -> Result<(), SubmitError> {
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
        job.epoch = self.shared.cache_epoch.load(Ordering::Acquire);
        q.colour = Some(job);
        self.shared.wake.notify_one();
        Ok(())
    }
    pub fn cancel_colour(&self) {
        if let Ok(mut q) = self.shared.queue.lock() {
            q.colour = None;
        }
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
        if matches!(queue.pending, Some(Work::Posters(_))) {
            queue.pending = None;
        }
        queue.clear = Some(request);
        self.shared.wake.notify_one();
        Ok(epoch)
    }
    pub fn cache_busy(&self) -> bool {
        self.shared.queue.lock().map_or(true, |q| q.cache_busy)
    }
    pub fn cancel_pending(&self) {
        if let Ok(mut queue) = self.shared.queue.lock() {
            queue.pending = None;
        }
    }

    pub fn cancel_posters_pending(&self) {
        if let Ok(mut queue) = self.shared.queue.lock() {
            if matches!(queue.pending, Some(Work::Posters(_))) {
                queue.pending = None;
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

struct WorkerExitGuard {
    shared: Arc<Shared>,
}

impl Drop for WorkerExitGuard {
    fn drop(&mut self) {
        finish_worker_exit(&self.shared, || {});
    }
}
fn finish_worker_exit(shared: &Shared, after_drain: impl FnOnce()) {
    let colour_job = {
        let mut queue = shared
            .queue
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Submission and final draining see the same closed availability under this lock.
        shared.running.store(false, Ordering::Release);
        queue.pending = None;
        let colour_job = queue.colour.take();
        if let Some(request) = queue.clear.take() {
            (request.acknowledge)(cache::Report {
                epoch: request.epoch,
                error: Some("Worker stopped before cache barrier; no clear acknowledged".into()),
                ..cache::Report::default()
            });
        }
        queue.cache_busy = false;
        colour_job
    };
    after_drain();
    if let Some(job) = colour_job {
        let failure = shared
            .failure
            .lock()
            .ok()
            .and_then(|f| *f)
            .unwrap_or(WorkerFailure::WorkerExited);
        colour::reject_queued(job, failure);
    }
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
}

fn record_failure(shared: &Shared, failure: WorkerFailure) {
    if let Ok(mut current) = shared.failure.lock() {
        if current.is_none() {
            *current = Some(failure);
        }
    }
}

fn take_pending(shared: &Shared) -> Option<Work> {
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
            return Some(Work::ClearCache(request));
        }
        if let Some(batch) = queue.pending.take() {
            return Some(batch);
        }
        if let Some(job) = queue.colour.take() {
            return Some(Work::Colour(job));
        }
        queue = match shared.wake.wait(queue) {
            Ok(queue) => queue,
            Err(_) => {
                record_failure(shared, WorkerFailure::QueuePoisoned);
                return None;
            }
        };
    }
}

fn run_worker<P, D>(shared: Arc<Shared>, generation_clock: Arc<AtomicU64>, publish: P, decode: D)
where
    P: Fn(PosterCompletion) -> bool + Send + Sync + 'static,
    D: Fn(&PosterSource, &Path) -> Option<Poster> + Send + Sync + 'static,
{
    loop {
        let Some(work) = take_pending(&shared) else {
            return;
        };
        let batch = match work {
            Work::Posters(batch) => batch,
            Work::Colour(job) => {
                let epoch = job.epoch;
                colour::execute(job, epoch, &shared.cache_epoch, &|| {
                    shared.shutdown.load(Ordering::Acquire)
                });
                continue;
            }
            Work::ClearCache(request) => {
                // Same lane: any in-flight decode/colour work has settled before enumeration.
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
                shared.queue.lock().unwrap().cache_busy = false;
                (request.acknowledge)(report);
                continue;
            }
        };
        let epoch = {
            let q = shared.queue.lock().unwrap();
            if q.cache_busy {
                continue;
            }
            shared.cache_epoch.load(Ordering::Acquire)
        };
        let mut posters = Vec::with_capacity(batch.sources.len());
        let mut cancelled = false;
        for source in &batch.sources {
            if shared.shutdown.load(Ordering::Acquire)
                || epoch != shared.cache_epoch.load(Ordering::Acquire)
                || batch.generation != generation_clock.load(Ordering::Acquire)
            {
                cancelled = true;
                break;
            }
            let decoded = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                decode(source, &batch.cache_dir)
            }));
            let Ok(decoded) = decoded else {
                record_failure(&shared, WorkerFailure::DecoderPanicked);
                return;
            };
            if let Some(poster) = decoded {
                posters.push(DecodedPoster {
                    source: source.clone(),
                    poster,
                });
            }
        }
        if cancelled
            || epoch != shared.cache_epoch.load(Ordering::Acquire)
            || shared.shutdown.load(Ordering::Acquire)
            || batch.generation != generation_clock.load(Ordering::Acquire)
        {
            continue;
        }
        if !publish(PosterCompletion {
            generation: batch.generation,
            posters,
        }) {
            record_failure(&shared, WorkerFailure::CompletionChannelClosed);
            return;
        }
    }
}

fn decode_source(source: &PosterSource, cache_dir: &Path) -> Option<Poster> {
    if let Some(still) = &source.still {
        if still.item_id != source.item_id || still.path != source.path || !still.is_current() {
            return None;
        }
        let decoded = gpu_gallery::decode_poster_cached(&still.path, source.layer, cache_dir)?;
        return still.is_current().then_some(decoded);
    }
    if source.is_video {
        Some(gpu_gallery::video_placeholder_poster(
            &source.path,
            source.layer,
        ))
    } else {
        None
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
                is_video: true,
                still: None,
                path: name.into(),
            }],
            std::path::PathBuf::from("unused-cache"),
        )
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
        assert!(matches!(completion_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[test]
    fn stale_active_work_is_cancelled_and_reopen_can_publish_latest_generation() {
        let generation_clock = Arc::new(std::sync::atomic::AtomicU64::new(10));
        let (started_tx, started_rx) = mpsc::channel();
        let release = Arc::new(Barrier::new(2));
        let (completion_tx, completion_rx) = mpsc::channel();
        let decoder_release = Arc::clone(&release);
        let worker = super::PosterWorker::spawn_with_builder(
            std::thread::Builder::new().name("test-poster-worker-reopen".into()),
            Arc::clone(&generation_clock),
            move |completion| completion_tx.send(completion).is_ok(),
            move |source, _| {
                if source.path == std::path::Path::new("hidden") {
                    started_tx.send(()).unwrap();
                    decoder_release.wait();
                }
                Some(crate::gpu_gallery::video_placeholder_poster(
                    &source.path,
                    source.layer,
                ))
            },
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
        let completion = completion_rx.recv().unwrap();
        assert!(completion.posters.is_empty());
        assert!(matches!(completion_rx.try_recv(), Err(TryRecvError::Empty)));
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
            is_video: false,
            path: gif.clone(),
            still: Some(still),
        };
        assert!(super::decode_source(&source, &root).unwrap().has_pixels());
        std::fs::write(&gif, b"changed").unwrap();
        assert!(super::decode_source(&source, &root).is_none());
    }
}

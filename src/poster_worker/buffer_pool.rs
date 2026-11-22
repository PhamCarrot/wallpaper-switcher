use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Condvar, Mutex, Weak};

use crate::gpu_gallery::POSTER_BYTES;

// Adapted from skwd-wall's bounded preview pool. Only the decode worker
// acquires leases. A lease may be returned by either the worker or the GPU.
#[derive(Debug)]
pub(crate) struct BufferPool {
    capacity: usize,
    state: Mutex<State>,
    ready: Condvar,
}

#[derive(Debug, Default)]
struct State {
    free: Vec<Vec<u8>>,
    allocated: usize,
    reuses: u64,
}

impl BufferPool {
    pub(crate) fn new(capacity: usize) -> Arc<Self> {
        assert!(
            capacity >= 2,
            "poster pool needs a scratch and staging slot"
        );
        Arc::new(Self {
            capacity,
            state: Mutex::new(State::default()),
            ready: Condvar::new(),
        })
    }

    // Producers overwrite the entire buffer before publishing it.
    pub(crate) fn take(self: &Arc<Self>) -> PosterBytes {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let bytes = loop {
            if let Some(bytes) = state.free.pop() {
                state.reuses += 1;
                break bytes;
            }
            if state.allocated < self.capacity {
                state.allocated += 1;
                drop(state);
                break vec![0; POSTER_BYTES];
            }
            state = self
                .ready
                .wait(state)
                .unwrap_or_else(std::sync::PoisonError::into_inner);
        };
        PosterBytes {
            bytes,
            pool: Some(Arc::downgrade(self)),
        }
    }

    // Reserve a canvas for each producer. A whole batch must complete before
    // the renderer can consume any of its newly staged results.
    pub(crate) fn has_staging_room(&self, producers: usize) -> bool {
        self.checked_out() < self.capacity - producers
    }

    pub(crate) fn can_keep_scratch(&self, producers: usize) -> bool {
        self.checked_out() <= self.capacity - producers
    }

    fn checked_out(&self) -> usize {
        let state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.allocated - state.free.len()
    }

    pub(crate) fn trace(&self) {
        crate::apply_trace::event_lazy("poster-decode-pool", || {
            let state = self
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            format!(
                "limit_bytes={} allocated_bytes={} checked_out_bytes={} reused={}",
                self.capacity * POSTER_BYTES,
                state.allocated * POSTER_BYTES,
                (state.allocated - state.free.len()) * POSTER_BYTES,
                state.reuses,
            )
        });
    }

    #[cfg(test)]
    pub(crate) fn counts(&self) -> (usize, usize, u64) {
        let state = self.state.lock().unwrap();
        (state.allocated, state.free.len(), state.reuses)
    }
}

pub(crate) struct PosterBytes {
    bytes: Vec<u8>,
    pool: Option<Weak<BufferPool>>,
}

impl From<Vec<u8>> for PosterBytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self { bytes, pool: None }
    }
}

impl std::fmt::Debug for PosterBytes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PosterBytes")
            .field("len", &self.bytes.len())
            .finish()
    }
}

impl Deref for PosterBytes {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.bytes
    }
}

impl DerefMut for PosterBytes {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.bytes
    }
}

impl PosterBytes {
    pub(crate) fn is_pooled(&self) -> bool {
        self.pool.is_some()
    }
}

impl Drop for PosterBytes {
    fn drop(&mut self) {
        if let Some(pool) = self.pool.as_ref().and_then(Weak::upgrade) {
            let mut state = pool
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            state.free.push(std::mem::take(&mut self.bytes));
            pool.ready.notify_one();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn exhausted_pool_waits_then_reissues_the_returned_buffer() {
        let pool = BufferPool::new(2);
        let held = pool.take();
        let mut returning = pool.take();
        returning[0] = 52;
        let address = returning.as_ptr() as usize;
        let (started, handoff) = mpsc::channel();
        let (sent, received) = mpsc::channel();
        let waiter = std::thread::spawn({
            let pool = pool.clone();
            move || {
                started.send(()).unwrap();
                sent.send(pool.take()).unwrap();
            }
        });
        handoff.recv_timeout(Duration::from_secs(5)).unwrap();
        assert!(matches!(
            received.recv_timeout(Duration::from_millis(25)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        drop(returning);
        let reused = received.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!((reused.as_ptr() as usize, reused[0]), (address, 52));
        drop((held, reused));
        waiter.join().unwrap();
        assert_eq!(pool.counts(), (2, 2, 1));
    }
}

//! One latest motion payload and one lightweight wake, independent of critical wakes.
use super::Wake;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Mutex,
};

#[derive(Debug, Default)]
pub(super) struct MotionMailbox {
    state: Mutex<State>,
    #[cfg(test)]
    active_producers: std::sync::atomic::AtomicUsize,
}

#[cfg(test)]
pub(super) struct ProducerObservation<'a>(&'a std::sync::atomic::AtomicUsize);

#[cfg(test)]
impl Drop for ProducerObservation<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Debug, Default)]
struct State {
    generation: u64,
    latest: Option<Wake>,
    wake_pending: bool,
}

impl MotionMailbox {
    #[cfg(test)]
    pub(super) fn observe_producer(&self) -> ProducerObservation<'_> {
        self.active_producers.fetch_add(1, Ordering::AcqRel);
        ProducerObservation(&self.active_producers)
    }

    #[cfg(test)]
    pub(super) fn active_producers(&self) -> usize {
        self.active_producers.load(Ordering::Acquire)
    }

    pub(super) fn publish(
        &self,
        frame: Wake,
        clock: &AtomicU64,
        notify: impl FnOnce() -> bool,
    ) -> bool {
        let Wake::MotionFrame { generation, .. } = &frame else {
            return false;
        };
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *generation != clock.load(Ordering::Acquire) || *generation < state.generation {
            return false;
        }
        state.generation = *generation;
        state.latest = Some(frame);
        // Retain an already queued marker across generation replacement.
        if !state.wake_pending {
            state.wake_pending = true;
            if !notify() {
                state.latest = None;
                state.wake_pending = false;
                return false;
            }
        }
        true
    }

    pub(super) fn take(&self) -> Option<Wake> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.wake_pending = false;
        state.latest.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::{mpsc, Arc};
    use std::time::Duration;

    fn frame(generation: u64, value: f32) -> Wake {
        Wake::MotionFrame {
            generation,
            value,
            velocity: 0.0,
            state: crate::upstream_slices_adapter::GalleryMotionState::absolute_camera(
                value, value, 250.0, 50.0, 50.0,
            ),
            selection: Default::default(),
            presentation: None,
            ordinary: None,
            hex: None,
            wall: None,
            scene: None,
        }
    }

    #[test]
    fn p310_stalled_consumer_retains_one_latest_payload_and_marker() {
        let mailbox = MotionMailbox::default();
        let clock = AtomicU64::new(1);
        let markers = AtomicUsize::new(0);
        for value in 0..10_000 {
            assert!(mailbox.publish(frame(1, value as f32), &clock, || {
                markers.fetch_add(1, Ordering::Relaxed);
                true
            }));
        }
        assert_eq!(markers.load(Ordering::Relaxed), 1);
        assert!(matches!(
            mailbox.take(),
            Some(Wake::MotionFrame { value: 9999.0, .. })
        ));
        assert!(mailbox.take().is_none());
        assert!(mailbox.publish(frame(1, 10_000.0), &clock, || {
            markers.fetch_add(1, Ordering::Relaxed);
            true
        }));
        assert_eq!(markers.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn p310_old_producer_cannot_replace_fresh_generation_or_duplicate_marker() {
        let mailbox = Arc::new(MotionMailbox::default());
        let clock = Arc::new(AtomicU64::new(1));
        let markers = Arc::new(AtomicUsize::new(0));
        assert!(mailbox.publish(frame(1, 10.0), &clock, || {
            markers.fetch_add(1, Ordering::Relaxed);
            true
        }));
        let (release, resume) = mpsc::channel();
        let old_mailbox = Arc::clone(&mailbox);
        let old_clock = Arc::clone(&clock);
        let old = std::thread::spawn(move || {
            let old = frame(1, 11.0);
            resume.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(!old_mailbox.publish(old, &old_clock, || panic!("stale producer notified")));
        });
        clock.store(2, Ordering::Release);
        assert!(mailbox.publish(frame(2, 20.0), &clock, || panic!("reuse pending marker")));
        release.send(()).unwrap();
        old.join().unwrap();
        assert_eq!(markers.load(Ordering::Relaxed), 1);
        assert!(matches!(
            mailbox.take(),
            Some(Wake::MotionFrame {
                generation: 2,
                value: 20.0,
                ..
            })
        ));
    }

    #[test]
    fn p310_producer_drain_race_preserves_terminal_frame_without_lost_wake() {
        let mailbox = Arc::new(MotionMailbox::default());
        let in_flight = Arc::new(AtomicUsize::new(0));
        let (notify, wakes) = mpsc::channel();
        let producing = Arc::clone(&mailbox);
        let pending = Arc::clone(&in_flight);
        let producer = std::thread::spawn(move || {
            let clock = AtomicU64::new(1);
            for value in 0..=10_000 {
                assert!(producing.publish(frame(1, value as f32), &clock, || {
                    assert_eq!(pending.fetch_add(1, Ordering::AcqRel), 0);
                    notify.send(()).is_ok()
                }));
                std::thread::yield_now();
            }
        });
        loop {
            wakes.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_eq!(in_flight.fetch_sub(1, Ordering::AcqRel), 1);
            let Some(Wake::MotionFrame { value, .. }) = mailbox.take() else {
                panic!("wake lost its latest payload");
            };
            if value == 10_000.0 {
                break;
            }
        }
        producer.join().unwrap();
        assert!(mailbox.take().is_none());
        assert_eq!(in_flight.load(Ordering::Acquire), 0);
        assert!(wakes.try_recv().is_err());
    }

    #[test]
    fn p310_failed_notification_discards_unobservable_payload_and_can_retry() {
        let mailbox = MotionMailbox::default();
        let clock = AtomicU64::new(1);
        assert!(!mailbox.publish(frame(1, 1.0), &clock, || false));
        assert!(mailbox.take().is_none());
        assert!(!mailbox.publish(Wake::Toggle, &clock, || panic!("non-motion publish")));
        assert!(mailbox.publish(frame(1, 2.0), &clock, || true));
        assert!(matches!(
            mailbox.take(),
            Some(Wake::MotionFrame { value: 2.0, .. })
        ));
    }
}

use super::*;
use crate::library::{
    color_metadata::{ColorBucket, ColorQuery, MetadataView},
    media_still::MediaStill,
};

#[derive(Debug, Clone)]
pub(super) enum Action {
    Bucket(ColorBucket),
}
#[derive(Debug, Clone)]
pub(super) struct Stored {
    pub still: Option<MediaStill>,
    pub reason: Option<String>,
    pub persistent: bool,
}
#[derive(Debug, Default)]
pub(super) struct State {
    pub query: ColorQuery,
    pub metadata: MetadataView,
    pub sorting: crate::library::sort_metrics::MetadataView,
    pub entries: HashMap<String, Stored>,
    pub clock: Arc<AtomicU64>,
    pub cursor: usize,
    pub pending: usize,
    pub active: bool,
    pub refresh: bool,
    pub settled: usize,
    pub unavailable: usize,
    pub nonpersistent: usize,
    pub status: String,
    pub selection_anchor: Option<String>,
    pub passive_requery: bool,
}
impl GpuCandidate {
    pub(super) fn cancel_colours(&mut self) {
        self.colours.clock.fetch_add(1, Ordering::AcqRel);
        self.colours.pending = 0;
        self.colours.active = false;
        if let Some(worker) = &self.poster_worker {
            worker.cancel_colour();
        }
    }
    pub(super) fn start_colours(&mut self, refresh: bool) {
        self.cancel_colours();
        self.colours.selection_anchor = self
            .selected
            .and_then(|i| self.wallpapers.get(i))
            .map(|i| i.id.clone());
        self.colours.metadata.clear();
        self.colours.sorting.clear();
        self.colours.entries.clear();
        self.colours.cursor = 0;
        self.colours.settled = 0;
        self.colours.unavailable = 0;
        self.colours.nonpersistent = 0;
        self.colours.refresh = refresh;
        self.colours.active = true;
        self.schedule_colour();
    }
    pub(super) fn schedule_colour(&mut self) {
        if !self.colours.active
            || self.colours.pending != 0
            || self.exit_requested
            || self.cache_operation.is_some()
        {
            return;
        }
        let mut stills = Vec::with_capacity(poster_worker::colour::BATCH_LIMIT);
        let mut unavailable = Vec::new();
        let mut cursor = self.colours.cursor;
        let catalog_revision = self.catalog_generation.load(Ordering::Acquire);
        while stills.len() < poster_worker::colour::BATCH_LIMIT {
            let Some(item) = self.wallpapers.get(cursor) else {
                break;
            };
            cursor += 1;
            if let Some(still) = MediaStill::select(item, catalog_revision) {
                stills.push(still);
            } else {
                unavailable.push(item.id.clone());
            };
        }
        let submitted = !stills.is_empty();
        if submitted {
            if !self.ensure_preview_worker() {
                self.colours.status =
                    "Colour worker unavailable; colour information is incomplete.".into();
                self.colours.active = false;
                return;
            }
            let Some(tx) = WAKE_TX.get().and_then(|slot| slot.lock().ok()?.clone()) else {
                self.colours.active = false;
                self.colours.status = "Colour wake channel unavailable".into();
                return;
            };
            let profile = Config::config_dir();
            let generation = self.colours.clock.load(Ordering::Acquire);
            let count = stills.len();
            let jobs = stills
                .into_iter()
                .map(|still| {
                    let tx = tx.clone();
                    poster_worker::colour::Job {
                        generation,
                        catalog_clock: self.catalog_generation.clone(),
                        epoch: 0,
                        clock: self.colours.clock.clone(),
                        still,
                        profile: profile.clone(),
                        refresh: self.colours.refresh,
                        publish: Box::new(move |c| {
                            let _ = tx.unbounded_send(Wake::ColourReady(c));
                        }),
                        #[cfg(test)]
                        held: None,
                    }
                })
                .collect();
            match self.poster_worker.as_ref().unwrap().submit_colours(jobs) {
                Ok(()) => self.colours.pending = count,
                Err(
                    error @ (poster_worker::SubmitError::CacheBusy
                    | poster_worker::SubmitError::Closed),
                ) => {
                    // Commit neither the cursor nor unsupported-item counts until
                    // this batch is accepted. The next motion tick retries it.
                    apply_trace::event_lazy("colour-submit-retry", || {
                        format!(
                            "worker={} cursor={} reason={error:?}",
                            self.poster_worker.as_ref().unwrap().id(),
                            self.colours.cursor
                        )
                    });
                    return;
                }
                Err(e) => {
                    self.colours.active = false;
                    self.colours.status = format!("Colour analysis refused: {e}");
                    return;
                }
            }
        }
        self.colours.cursor = cursor;
        for id in unavailable {
            self.colours.entries.insert(
                id,
                Stored {
                    still: None,
                    reason: Some("No supported stored image/poster".into()),
                    persistent: false,
                },
            );
            self.colours.unavailable += 1;
            self.colours.settled += 1;
        }
        if submitted {
            self.colours.status = format!(
                "Colour information: {}/{} settled · {} unavailable",
                self.colours.settled,
                self.wallpapers.len(),
                self.colours.unavailable
            );
            return;
        }
        self.colours.active = false;
        self.refresh_filter();
        self.schedule_posters();
        self.colours.status = format!(
            "Colour information complete: {} known · {} unavailable · {} session-only",
            self.colours.metadata.len(),
            self.colours.unavailable,
            self.colours.nonpersistent
        );
    }
    pub(super) fn install_colour(&mut self, c: poster_worker::colour::Completion) {
        if self.exit_requested
            || c.generation != self.colours.clock.load(Ordering::Acquire)
            || self
                .poster_worker
                .as_ref()
                .is_none_or(|w| w.cache_epoch() != c.epoch)
                && c.worker_failure.is_none()
        {
            return;
        }
        let worker_failed = c.worker_failure.is_some();
        self.colours.pending = if worker_failed {
            0
        } else {
            self.colours.pending.saturating_sub(1)
        };
        let current = self
            .wallpapers
            .iter()
            .find(|item| item.id == c.still.item_id)
            .and_then(|item| {
                MediaStill::select(item, self.catalog_generation.load(Ordering::Acquire))
            });
        if current.as_ref() != Some(&c.still) || !c.still.is_current() {
            self.colours.unavailable += 1;
            self.colours.entries.insert(
                c.still.item_id.clone(),
                Stored {
                    still: current,
                    reason: Some(
                        "Still changed during analysis; colour information unavailable".into(),
                    ),
                    persistent: false,
                },
            );
        } else {
            if let Some(sorting) = c.sorting.filter(|m| m.valid()) {
                self.colours
                    .sorting
                    .insert(c.still.item_id.clone(), sorting);
            }
            let persistent = c.persistence.is_ok();
            let reason = match c.result {
                Ok(m) if m.valid() => {
                    self.colours.metadata.insert(c.still.item_id.clone(), m);
                    if !persistent {
                        self.colours.nonpersistent += 1;
                    }
                    c.persistence.err().map(|e| format!("Session-only: {e}"))
                }
                result => {
                    self.colours.unavailable += 1;
                    Some(
                        result
                            .err()
                            .unwrap_or_else(|| "Invalid colour result refused".into()),
                    )
                }
            };
            self.colours.entries.insert(
                c.still.item_id.clone(),
                Stored {
                    still: Some(c.still),
                    reason,
                    persistent,
                },
            );
        }
        self.colours.settled += 1;
        if worker_failed {
            self.colours.active = false;
            self.colours.status = "Colour worker failed; completed results retained.".into();
        }
        // Passive metadata cannot change a non-colour query or its poster demand.
        // Active queries and worker failures still reconcile immediately.
        if worker_failed || self.colour_completion_affects_query() {
            self.refresh_colour_filter();
            self.schedule_posters();
        }
        self.schedule_colour();
    }
    pub(super) fn colour_action(&mut self, action: Action) {
        match action {
            Action::Bucket(bucket) => self.colours.query.toggle(bucket),
        }
        self.refresh_filter();
        self.schedule_posters();
    }
    pub(super) fn colour_completion_affects_query(&self) -> bool {
        self.colours.query.explicit()
            || matches!(
                self.sort_order,
                SortOrder::Colour
                    | SortOrder::ColourPop
                    | SortOrder::Richness
                    | SortOrder::Minimalist
            )
    }
    pub(super) fn refresh_colour_filter(&mut self) {
        self.colours.passive_requery = true;
        self.refresh_filter();
        self.colours.passive_requery = false;
    }
    pub(super) fn prune_colour_stamps(&mut self) {
        let entries = &self.colours.entries;
        self.colours.sorting.retain(|id, _| {
            entries
                .get(id)
                .and_then(|e| e.still.as_ref())
                .is_some_and(|s| s.is_current())
        });
        self.colours.metadata.retain(|id, _| {
            entries
                .get(id)
                .and_then(|e| e.still.as_ref())
                .is_some_and(|s| s.is_current())
        });
    }
}

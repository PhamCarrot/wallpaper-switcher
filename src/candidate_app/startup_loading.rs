//! Startup loading screen: covers the first interactive window until the
//! catalog is published and the first visible posters are ready.
use super::*;
use std::time::{Duration, Instant};

pub(super) const PREVIEW_WAIT: Duration = Duration::from_secs(4);
pub(super) const DISCOVERY_LIMIT: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct StartupLoading {
    pub started: Instant,
    pub catalog_at: Option<Instant>,
    pub ready: usize,
    pub total: usize,
    discovery_wake: bool,
}

impl StartupLoading {
    pub(super) fn new(started: Instant) -> Self {
        Self {
            started,
            catalog_at: None,
            ready: 0,
            total: 0,
            discovery_wake: false,
        }
    }
}

pub(super) fn startup_settled(
    started: Instant,
    catalog_at: Option<Instant>,
    now: Instant,
    ready: usize,
    failed: usize,
    total: usize,
) -> Option<&'static str> {
    let Some(catalog_at) = catalog_at else {
        return (now >= started + DISCOVERY_LIMIT).then_some("discovery-timeout");
    };
    if total == 0 {
        Some("empty")
    } else if ready + failed >= total {
        Some("previews-ready")
    } else if now >= catalog_at + PREVIEW_WAIT {
        Some("deadline")
    } else {
        None
    }
}

impl GpuCandidate {
    fn first_viewport_readiness(&self) -> (usize, usize, usize) {
        if self.filtered_indices.is_empty() {
            return (0, 0, 0);
        }
        if self.gallery_mode == GalleryMode::Depth {
            let settled = !self.poster_loading && self.poster_request_key.is_some();
            return (usize::from(settled), 0, 1);
        }
        let frame = self.ordinary_frame();
        let ready = frame.iter().filter(|e| e.card.layer.is_some()).count();
        let failed = frame
            .iter()
            .filter(|e| {
                e.card.layer.is_none() && self.poster_failed_indices.contains(&e.card.index)
            })
            .count();
        (ready, failed, frame.len())
    }

    pub(super) fn settle_startup_loading(&mut self) -> Option<Duration> {
        let mut loading = self.startup_loading?;
        let now = Instant::now();
        let mut wake = None;
        if !self.catalog_loaded && !loading.discovery_wake {
            loading.discovery_wake = true;
            wake = Some((loading.started + DISCOVERY_LIMIT).saturating_duration_since(now));
        }
        if self.catalog_loaded && loading.catalog_at.is_none() {
            loading.catalog_at = Some(now);
            wake = Some(PREVIEW_WAIT);
        }
        let (ready, failed, total) = if self.catalog_loaded {
            self.first_viewport_readiness()
        } else {
            (0, 0, 0)
        };
        loading.ready = ready;
        loading.total = total;
        match startup_settled(
            loading.started,
            loading.catalog_at,
            now,
            ready,
            failed,
            total,
        ) {
            Some(reason) => {
                self.startup_loading = None;
                apply_trace::event_lazy("startup-loading-released", || {
                    format!(
                        "reason={reason} elapsed_ms={} ready={ready} failed={failed} total={total}",
                        now.duration_since(loading.started).as_millis()
                    )
                });
            }
            None => self.startup_loading = Some(loading),
        }
        wake
    }
}

pub(super) fn startup_loading_status(loading: &StartupLoading) -> String {
    if loading.catalog_at.is_none() {
        "Finding wallpapers…".into()
    } else {
        format!(
            "Preparing previews · {} of {}",
            loading.ready.min(loading.total),
            loading.total
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn startup_loading_waits_for_catalog_then_previews_or_deadline() {
        let start = Instant::now();
        let ms = Duration::from_millis;
        let settled = |catalog, now, ready, failed, total| {
            startup_settled(start, catalog, now, ready, failed, total)
        };
        assert_eq!(
            settled(None, start + DISCOVERY_LIMIT - ms(1), 0, 0, 0),
            None
        );
        assert_eq!(
            settled(None, start + DISCOVERY_LIMIT, 0, 0, 0),
            Some("discovery-timeout")
        );
        let catalog = Some(start);
        assert_eq!(settled(catalog, start, 0, 0, 0), Some("empty"));
        assert_eq!(settled(catalog, start, 3, 0, 8), None);
        assert_eq!(settled(catalog, start, 6, 2, 8), Some("previews-ready"));
        assert_eq!(settled(catalog, start, 8, 0, 8), Some("previews-ready"));
        assert_eq!(
            settled(catalog, start + PREVIEW_WAIT - ms(1), 3, 0, 8),
            None
        );
        assert_eq!(
            settled(catalog, start + PREVIEW_WAIT, 3, 0, 8),
            Some("deadline")
        );
        let mut loading = StartupLoading::new(start);
        assert_eq!(startup_loading_status(&loading), "Finding wallpapers…");
        loading.catalog_at = catalog;
        loading.ready = 5;
        loading.total = 12;
        assert_eq!(
            startup_loading_status(&loading),
            "Preparing previews · 5 of 12"
        );
    }
}

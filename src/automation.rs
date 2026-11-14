use crate::candidate_state::{CandidateState, PlaybackOrder, Playlist, Schedule};
use crate::library::{self, CatalogEntry};

/// Independent of picker predicates/selection. Input identities come from the full library.
pub fn next_rotation_wallpaper(
    catalog: &[CatalogEntry],
    items: &[crate::scanner::WallpaperItem],
    preferences: &crate::candidate_state::local_library::RotationPreferences,
    favourites: &std::collections::BTreeSet<String>,
    previous_success: Option<&str>,
    sample: u64,
) -> Option<String> {
    use crate::scanner::WallpaperSource;
    let mut pool: Vec<_> = items
        .iter()
        .filter(|item| {
            catalog.iter().any(|entry| entry.id == item.id)
                && (!preferences.favourites_only || favourites.contains(&item.id))
                && match item.source {
                    WallpaperSource::LocalImage => preferences.images,
                    WallpaperSource::LocalVideo => preferences.videos,
                    WallpaperSource::WallpaperEngine => preferences.we,
                }
        })
        .map(|item| item.id.as_str())
        .collect();
    pool.sort_unstable();
    pool.dedup();
    if pool.len() > 1 {
        pool.retain(|id| Some(*id) != previous_success);
    }
    if pool.is_empty() {
        None
    } else {
        Some(pool[(sample % pool.len() as u64) as usize].to_owned())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleDecision {
    pub schedule_id: String,
    pub playlist_id: String,
    pub monitor_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleRejection {
    ManualHoldoff {
        until_unix: u64,
    },
    InvalidWeekday {
        weekday: u8,
    },
    NoEnabledSchedule,
    WeekdayNotScheduled,
    ScheduleNotDue,
    PlaylistUnavailable,
    TargetUnavailable {
        requested: usize,
        available: usize,
        unavailable: usize,
    },
    MonitorLocked {
        locked: usize,
        unavailable: usize,
    },
}

impl ScheduleRejection {
    pub fn reason_code(&self) -> &'static str {
        match self {
            Self::ManualHoldoff { .. } => "manual_holdoff",
            Self::InvalidWeekday { .. } => "invalid_weekday",
            Self::NoEnabledSchedule => "no_enabled_schedule",
            Self::WeekdayNotScheduled => "weekday_not_scheduled",
            Self::ScheduleNotDue => "schedule_not_due",
            Self::PlaylistUnavailable => "playlist_unavailable",
            Self::TargetUnavailable { .. } => "target_unavailable",
            Self::MonitorLocked { .. } => "monitor_locked",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleEvaluation {
    Selected(ScheduleDecision),
    Rejected(ScheduleRejection),
}

pub fn select_schedule(
    state: &CandidateState,
    now_unix: u64,
    weekday_sunday_zero: u8,
    minute_of_day: u16,
    available_monitors: &[String],
) -> Option<ScheduleDecision> {
    match evaluate_schedule(
        state,
        now_unix,
        weekday_sunday_zero,
        minute_of_day,
        available_monitors,
    ) {
        ScheduleEvaluation::Selected(decision) => Some(decision),
        ScheduleEvaluation::Rejected(_) => None,
    }
}

pub fn evaluate_schedule(
    state: &CandidateState,
    now_unix: u64,
    weekday_sunday_zero: u8,
    minute_of_day: u16,
    available_monitors: &[String],
) -> ScheduleEvaluation {
    if !state.automation_allowed(now_unix) {
        return ScheduleEvaluation::Rejected(ScheduleRejection::ManualHoldoff {
            until_unix: state.manual_holdoff_until_unix,
        });
    }
    if weekday_sunday_zero > 6 {
        return ScheduleEvaluation::Rejected(ScheduleRejection::InvalidWeekday {
            weekday: weekday_sunday_zero,
        });
    }
    let day_mask = 1_u8 << weekday_sunday_zero;
    let mut enabled_count = 0;
    let mut weekday_match_count = 0;
    let mut due_count = 0;
    let mut playlist_match_count = 0;
    let schedule = state
        .schedules
        .iter()
        .filter(|schedule| {
            if !schedule.enabled {
                return false;
            }
            enabled_count += 1;
            if schedule.days & day_mask == 0 {
                return false;
            }
            weekday_match_count += 1;
            if schedule.minute_of_day.min(1439) != minute_of_day.min(1439) {
                return false;
            }
            due_count += 1;
            let playlist_exists = state
                .playlists
                .iter()
                .any(|playlist| playlist.id == schedule.playlist_id);
            if playlist_exists {
                playlist_match_count += 1;
            }
            playlist_exists
        })
        .min_by_key(|schedule| (schedule.priority, schedule.creation_order));
    let Some(schedule) = schedule else {
        let rejection = if enabled_count == 0 {
            ScheduleRejection::NoEnabledSchedule
        } else if weekday_match_count == 0 {
            ScheduleRejection::WeekdayNotScheduled
        } else if due_count == 0 {
            ScheduleRejection::ScheduleNotDue
        } else {
            debug_assert_eq!(playlist_match_count, 0);
            ScheduleRejection::PlaylistUnavailable
        };
        return ScheduleEvaluation::Rejected(rejection);
    };
    let monitor_selection = eligible_monitors(schedule, available_monitors, state);
    if monitor_selection.eligible.is_empty() {
        let rejection = if monitor_selection.locked > 0 {
            ScheduleRejection::MonitorLocked {
                locked: monitor_selection.locked,
                unavailable: monitor_selection.unavailable,
            }
        } else {
            ScheduleRejection::TargetUnavailable {
                requested: schedule.monitor_ids.len(),
                available: monitor_selection.matching_available,
                unavailable: monitor_selection.unavailable,
            }
        };
        return ScheduleEvaluation::Rejected(rejection);
    }
    ScheduleEvaluation::Selected(ScheduleDecision {
        schedule_id: schedule.id.clone(),
        playlist_id: schedule.playlist_id.clone(),
        monitor_ids: monitor_selection.eligible,
    })
}

struct MonitorSelection {
    eligible: Vec<String>,
    matching_available: usize,
    locked: usize,
    unavailable: usize,
}

fn eligible_monitors(
    schedule: &Schedule,
    available_monitors: &[String],
    state: &CandidateState,
) -> MonitorSelection {
    let eligible: Vec<_> = available_monitors
        .iter()
        .filter(|monitor| {
            !state.locked_monitors.contains(*monitor)
                && (schedule.monitor_ids.is_empty() || schedule.monitor_ids.contains(*monitor))
        })
        .cloned()
        .collect();
    let matching_available = available_monitors
        .iter()
        .filter(|monitor| {
            schedule.monitor_ids.is_empty() || schedule.monitor_ids.contains(*monitor)
        })
        .count();
    let locked = available_monitors
        .iter()
        .filter(|monitor| {
            state.locked_monitors.contains(*monitor)
                && (schedule.monitor_ids.is_empty() || schedule.monitor_ids.contains(*monitor))
        })
        .count();
    let unavailable = schedule
        .monitor_ids
        .iter()
        .filter(|monitor| !available_monitors.contains(monitor))
        .count();
    MonitorSelection {
        eligible,
        matching_available,
        locked,
        unavailable,
    }
}

pub fn next_playlist_wallpaper(
    playlist: &Playlist,
    catalog: &[CatalogEntry],
    favorites: &std::collections::BTreeSet<String>,
    current_id: Option<&str>,
    random_sample: u64,
) -> Option<String> {
    let entries = library::evaluate_playlist(playlist, catalog, favorites);
    if entries.is_empty() {
        return None;
    }
    match playlist.order {
        PlaybackOrder::Sequential => {
            let next = current_id
                .and_then(|current| entries.iter().position(|entry| entry.id == current))
                .map_or(0, |position| (position + 1) % entries.len());
            Some(entries[next].id.clone())
        }
        PlaybackOrder::Shuffle => {
            let mut index = (random_sample as usize) % entries.len();
            if entries.len() > 1 && current_id.is_some_and(|current| entries[index].id == current) {
                index = (index + 1) % entries.len();
            }
            Some(entries[index].id.clone())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::candidate_state::{PlaylistMembership, SmartRule};
    use std::collections::{BTreeSet, HashSet};
    use std::path::PathBuf;

    fn entry(id: &str) -> CatalogEntry {
        CatalogEntry {
            id: id.into(),
            title: id.into(),
            kind: "image".into(),
            folder: PathBuf::from("wallpapers"),
            width: 1920,
            height: 1080,
            modified_unix: 0,
            source_tags: Default::default(),
            tags: BTreeSet::new(),
        }
    }

    fn playlist(id: &str, order: PlaybackOrder) -> Playlist {
        Playlist {
            id: id.into(),
            name: id.into(),
            order,
            membership: PlaylistMembership::Explicit {
                wallpaper_ids: vec!["a".into(), "b".into(), "c".into()],
            },
        }
    }

    #[test]
    fn schedule_conflicts_use_priority_then_creation_order() {
        let mut state = CandidateState::default();
        state.playlists = vec![playlist("first", PlaybackOrder::Sequential)];
        state.schedules = vec![
            Schedule {
                id: "later-created".into(),
                enabled: true,
                days: 1 << 2,
                minute_of_day: 480,
                priority: 10,
                creation_order: 8,
                playlist_id: "first".into(),
                ..Default::default()
            },
            Schedule {
                id: "winner".into(),
                enabled: true,
                days: 1 << 2,
                minute_of_day: 480,
                priority: 10,
                creation_order: 2,
                playlist_id: "first".into(),
                ..Default::default()
            },
            Schedule {
                id: "lower-priority".into(),
                enabled: true,
                days: 1 << 2,
                minute_of_day: 480,
                priority: 20,
                playlist_id: "first".into(),
                ..Default::default()
            },
        ];

        let monitors = ["one".into()];
        let evaluation = evaluate_schedule(&state, 1_000, 2, 480, &monitors);
        let ScheduleEvaluation::Selected(decision) = evaluation.clone() else {
            panic!("the existing due schedule should be selected");
        };
        assert_eq!(evaluation, ScheduleEvaluation::Selected(decision.clone()));
        assert_eq!(
            select_schedule(&state, 1_000, 2, 480, &monitors),
            Some(decision.clone())
        );
        assert_eq!(decision.schedule_id, "winner");
    }

    #[test]
    fn manual_holdoff_and_monitor_locks_block_automatic_targets() {
        let mut state = CandidateState::default();
        state.playlists = vec![playlist("daily", PlaybackOrder::Sequential)];
        state.schedules = vec![Schedule {
            id: "morning".into(),
            enabled: true,
            days: 1,
            minute_of_day: 60,
            playlist_id: "daily".into(),
            monitor_ids: vec!["one".into(), "two".into()],
            ..Default::default()
        }];
        state.locked_monitors.insert("two".into());
        state.begin_manual_holdoff(1_000);

        assert!(select_schedule(&state, 1_599, 0, 60, &["one".into(), "two".into()]).is_none());
        assert_eq!(
            select_schedule(&state, 1_600, 0, 60, &["one".into(), "two".into()])
                .unwrap()
                .monitor_ids,
            ["one"]
        );
    }

    #[test]
    fn schedule_evaluation_reports_the_first_existing_rejection_guard() {
        let mut state = CandidateState::default();
        state.playlists = vec![playlist("daily", PlaybackOrder::Sequential)];
        state.schedules = vec![Schedule {
            id: "morning".into(),
            enabled: true,
            days: 1,
            minute_of_day: 60,
            playlist_id: "daily".into(),
            monitor_ids: vec!["one".into()],
            ..Default::default()
        }];

        state.begin_manual_holdoff(1_000);
        assert_eq!(
            evaluate_schedule(&state, 1_001, 7, 61, &[]),
            ScheduleEvaluation::Rejected(ScheduleRejection::ManualHoldoff { until_unix: 1_600 })
        );
        assert!(select_schedule(&state, 1_001, 7, 61, &[]).is_none());

        state.manual_holdoff_until_unix = 0;
        state.locked_monitors.insert("one".into());
        assert_eq!(
            evaluate_schedule(&state, 1_600, 0, 60, &["one".into()]),
            ScheduleEvaluation::Rejected(ScheduleRejection::MonitorLocked {
                locked: 1,
                unavailable: 0
            })
        );
        assert_eq!(
            evaluate_schedule(&state, 1_600, 0, 60, &["other".into()]),
            ScheduleEvaluation::Rejected(ScheduleRejection::TargetUnavailable {
                requested: 1,
                available: 0,
                unavailable: 1
            })
        );
    }

    #[test]
    fn schedule_evaluation_names_schedule_and_playlist_guards() {
        let mut state = CandidateState::default();
        assert_eq!(
            evaluate_schedule(&state, 1_000, 0, 60, &["one".into()]),
            ScheduleEvaluation::Rejected(ScheduleRejection::NoEnabledSchedule)
        );

        state.schedules = vec![Schedule {
            id: "morning".into(),
            enabled: true,
            days: 1,
            minute_of_day: 60,
            playlist_id: "missing".into(),
            ..Default::default()
        }];
        assert_eq!(
            evaluate_schedule(&state, 1_000, 1, 60, &["one".into()]),
            ScheduleEvaluation::Rejected(ScheduleRejection::WeekdayNotScheduled)
        );
        assert_eq!(
            evaluate_schedule(&state, 1_000, 0, 61, &["one".into()]),
            ScheduleEvaluation::Rejected(ScheduleRejection::ScheduleNotDue)
        );
        assert_eq!(
            evaluate_schedule(&state, 1_000, 0, 60, &["one".into()]),
            ScheduleEvaluation::Rejected(ScheduleRejection::PlaylistUnavailable)
        );
        assert_eq!(
            evaluate_schedule(&state, 1_000, 7, 60, &["one".into()]),
            ScheduleEvaluation::Rejected(ScheduleRejection::InvalidWeekday { weekday: 7 })
        );
    }

    #[test]
    fn schedule_rejection_reason_codes_are_stable_for_trace_contracts() {
        assert_eq!(
            ScheduleRejection::ManualHoldoff { until_unix: 1_600 }.reason_code(),
            "manual_holdoff"
        );
        assert_eq!(
            ScheduleRejection::MonitorLocked {
                locked: 1,
                unavailable: 0
            }
            .reason_code(),
            "monitor_locked"
        );
        assert_eq!(
            ScheduleRejection::TargetUnavailable {
                requested: 1,
                available: 0,
                unavailable: 1
            }
            .reason_code(),
            "target_unavailable"
        );
    }

    #[test]
    fn sequential_wraps_and_shuffle_is_bounded_and_avoids_current() {
        let catalog = vec![entry("a"), entry("b"), entry("c")];
        let favorites = BTreeSet::new();
        let sequential = playlist("seq", PlaybackOrder::Sequential);
        assert_eq!(
            next_playlist_wallpaper(&sequential, &catalog, &favorites, Some("c"), 0),
            Some("a".into())
        );
        let shuffle = playlist("shuffle", PlaybackOrder::Shuffle);
        let selected: HashSet<_> = (0..30)
            .map(|sample| {
                next_playlist_wallpaper(&shuffle, &catalog, &favorites, Some("a"), sample).unwrap()
            })
            .collect();
        assert!(!selected.contains("a"));
        assert_eq!(selected, HashSet::from(["b".into(), "c".into()]));
    }

    #[test]
    fn smart_playlist_is_evaluated_against_the_current_catalog() {
        let playlist = Playlist {
            id: "wide".into(),
            name: "Wide".into(),
            order: PlaybackOrder::Sequential,
            membership: PlaylistMembership::Smart {
                rule: SmartRule {
                    minimum_width: 2560,
                    ..Default::default()
                },
            },
        };
        let mut wide = entry("wide");
        wide.width = 3840;

        assert_eq!(
            next_playlist_wallpaper(
                &playlist,
                &[entry("small"), wide],
                &BTreeSet::new(),
                None,
                0
            ),
            Some("wide".into())
        );
    }
}

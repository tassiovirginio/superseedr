// SPDX-FileCopyrightText: 2026 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use crate::app::AppState;
use crate::persistence::network_history::{
    enforce_retention_caps, NetworkHistoryPersistedState, NetworkHistoryPoint,
    NetworkHistoryRollupState, NetworkHistoryTiers, HOUR_1H_CAP, MINUTE_15M_CAP, MINUTE_1M_CAP,
    SECOND_1S_CAP,
};
use std::collections::{BTreeMap, VecDeque};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct NetworkHistoryTelemetry;

impl NetworkHistoryTelemetry {
    pub fn on_second_tick(app_state: &mut AppState) {
        let now_unix = current_unix_time();
        let download_bps = app_state.avg_download_history.last().copied().unwrap_or(0);
        let upload_bps = app_state.avg_upload_history.last().copied().unwrap_or(0);
        let backoff_ms_max = app_state
            .disk_backoff_history_ms
            .back()
            .copied()
            .unwrap_or(0);
        if app_state.network_history_rollups.ingest_second_sample(
            &mut app_state.network_history_state,
            now_unix,
            download_bps,
            upload_bps,
            backoff_ms_max,
        ) {
            app_state.network_history_dirty = true;
        }
    }

    pub fn apply_loaded_state(app_state: &mut AppState, state: NetworkHistoryPersistedState) {
        Self::apply_loaded_state_at(app_state, state, current_unix_time());
    }

    fn apply_loaded_state_at(
        app_state: &mut AppState,
        state: NetworkHistoryPersistedState,
        now_unix: u64,
    ) {
        let was_dirty = app_state.network_history_dirty;
        let merged = merge_state_for_late_restore(&app_state.network_history_state, state);
        let rollup_source =
            densify_state_for_restore(merged.clone(), rollup_rebuild_cutoff_unix(&merged));
        let densified = densify_state_for_restore(merged, now_unix);

        app_state.avg_download_history = densified
            .tiers
            .second_1s
            .iter()
            .map(|p| p.download_bps)
            .collect();
        app_state.avg_upload_history = densified
            .tiers
            .second_1s
            .iter()
            .map(|p| p.upload_bps)
            .collect();
        app_state.disk_backoff_history_ms = VecDeque::from(
            densified
                .tiers
                .second_1s
                .iter()
                .map(|p| p.backoff_ms_max)
                .collect::<Vec<_>>(),
        );

        app_state.minute_avg_dl_history = densified
            .tiers
            .minute_1m
            .iter()
            .map(|p| p.download_bps)
            .collect();
        app_state.minute_avg_ul_history = densified
            .tiers
            .minute_1m
            .iter()
            .map(|p| p.upload_bps)
            .collect();
        app_state.minute_disk_backoff_history_ms = VecDeque::from(
            densified
                .tiers
                .minute_1m
                .iter()
                .map(|p| p.backoff_ms_max)
                .collect::<Vec<_>>(),
        );

        app_state.network_history_state = densified;
        app_state.network_history_rollups =
            NetworkHistoryRollupState::rebuild_from_state(&rollup_source);
        // Preserve dirty state if live samples were already pending flush.
        app_state.network_history_dirty = was_dirty;
    }
}

fn current_unix_time() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn rollup_rebuild_cutoff_unix(state: &NetworkHistoryPersistedState) -> u64 {
    state
        .updated_at_unix
        .max(latest_point_timestamp(&state.tiers.second_1s))
        .max(latest_point_timestamp(&state.tiers.minute_1m))
        .max(latest_point_timestamp(&state.tiers.minute_15m))
        .max(latest_point_timestamp(&state.tiers.hour_1h))
}

fn latest_point_timestamp(points: &[NetworkHistoryPoint]) -> u64 {
    points.last().map(|point| point.ts_unix).unwrap_or(0)
}

fn merge_tier_points(
    loaded: Vec<NetworkHistoryPoint>,
    live: Vec<NetworkHistoryPoint>,
) -> Vec<NetworkHistoryPoint> {
    let mut by_ts = BTreeMap::<u64, NetworkHistoryPoint>::new();
    for point in loaded {
        by_ts.insert(point.ts_unix, point);
    }
    // Live points win for identical timestamps.
    for point in live {
        by_ts.insert(point.ts_unix, point);
    }
    by_ts.into_values().collect()
}

fn merge_state_for_late_restore(
    live_state: &NetworkHistoryPersistedState,
    loaded_state: NetworkHistoryPersistedState,
) -> NetworkHistoryPersistedState {
    let mut merged = NetworkHistoryPersistedState {
        schema_version: loaded_state.schema_version.max(live_state.schema_version),
        updated_at_unix: loaded_state.updated_at_unix.max(live_state.updated_at_unix),
        tiers: crate::persistence::network_history::NetworkHistoryTiers {
            second_1s: merge_tier_points(
                loaded_state.tiers.second_1s,
                live_state.tiers.second_1s.clone(),
            ),
            minute_1m: merge_tier_points(
                loaded_state.tiers.minute_1m,
                live_state.tiers.minute_1m.clone(),
            ),
            minute_15m: merge_tier_points(
                loaded_state.tiers.minute_15m,
                live_state.tiers.minute_15m.clone(),
            ),
            hour_1h: merge_tier_points(
                loaded_state.tiers.hour_1h,
                live_state.tiers.hour_1h.clone(),
            ),
        },
    };

    enforce_retention_caps(&mut merged);
    merged
}

fn densify_tier_points(
    points: &[NetworkHistoryPoint],
    step_secs: u64,
    max_points: usize,
    now_unix: u64,
) -> Vec<NetworkHistoryPoint> {
    if points.is_empty() || step_secs == 0 || max_points == 0 {
        return Vec::new();
    }

    let dense_end_ts = densified_end_ts(points[points.len() - 1].ts_unix, step_secs, now_unix);
    let max_window_span = step_secs.saturating_mul(max_points.saturating_sub(1) as u64);
    let dense_start_ts = points[0]
        .ts_unix
        .max(dense_end_ts.saturating_sub(max_window_span));

    let mut start_idx = 0;
    while start_idx < points.len() && points[start_idx].ts_unix < dense_start_ts {
        start_idx += 1;
    }

    let mut dense = Vec::with_capacity(max_points);
    let mut next_ts = dense_start_ts;

    for point in &points[start_idx..] {
        while next_ts < point.ts_unix && next_ts <= dense_end_ts {
            dense.push(NetworkHistoryPoint {
                ts_unix: next_ts,
                ..Default::default()
            });
            let advanced_ts = next_ts.saturating_add(step_secs);
            if advanced_ts == next_ts {
                return dense;
            }
            next_ts = advanced_ts;
        }

        if next_ts > dense_end_ts {
            break;
        }

        dense.push(point.clone());
        if point.ts_unix >= dense_end_ts {
            return dense;
        }

        let advanced_ts = point.ts_unix.saturating_add(step_secs);
        if advanced_ts == point.ts_unix {
            return dense;
        }
        next_ts = advanced_ts;
    }

    while next_ts <= dense_end_ts {
        dense.push(NetworkHistoryPoint {
            ts_unix: next_ts,
            ..Default::default()
        });
        let advanced_ts = next_ts.saturating_add(step_secs);
        if advanced_ts == next_ts {
            break;
        }
        next_ts = advanced_ts;
    }

    dense
}

fn densified_end_ts(last_point_ts: u64, step_secs: u64, now_unix: u64) -> u64 {
    if last_point_ts >= now_unix {
        return last_point_ts;
    }

    let trailing_steps = (now_unix - last_point_ts) / step_secs;
    last_point_ts.saturating_add(trailing_steps.saturating_mul(step_secs))
}

fn densify_state_for_restore(
    state: NetworkHistoryPersistedState,
    now_unix: u64,
) -> NetworkHistoryPersistedState {
    let mut dense = NetworkHistoryPersistedState {
        schema_version: state.schema_version,
        updated_at_unix: state.updated_at_unix,
        tiers: NetworkHistoryTiers {
            second_1s: densify_tier_points(&state.tiers.second_1s, 1, SECOND_1S_CAP, now_unix),
            minute_1m: densify_tier_points(&state.tiers.minute_1m, 60, MINUTE_1M_CAP, now_unix),
            minute_15m: densify_tier_points(
                &state.tiers.minute_15m,
                15 * 60,
                MINUTE_15M_CAP,
                now_unix,
            ),
            hour_1h: densify_tier_points(&state.tiers.hour_1h, 60 * 60, HOUR_1H_CAP, now_unix),
        },
    };
    enforce_retention_caps(&mut dense);
    dense
}

#[cfg(test)]
mod tests {
    use super::{
        densify_state_for_restore, densify_tier_points, merge_state_for_late_restore,
        merge_tier_points, NetworkHistoryTelemetry,
    };
    use crate::app::AppState;
    use crate::persistence::network_history::{NetworkHistoryPersistedState, NetworkHistoryPoint};
    use std::collections::VecDeque;

    #[test]
    fn merge_tier_points_prefers_live_on_timestamp_collision() {
        let loaded = vec![NetworkHistoryPoint {
            ts_unix: 10,
            download_bps: 100,
            upload_bps: 10,
            backoff_ms_max: 1,
        }];
        let live = vec![NetworkHistoryPoint {
            ts_unix: 10,
            download_bps: 200,
            upload_bps: 20,
            backoff_ms_max: 2,
        }];

        let merged = merge_tier_points(loaded, live);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].download_bps, 200);
        assert_eq!(merged[0].upload_bps, 20);
        assert_eq!(merged[0].backoff_ms_max, 2);
    }

    #[test]
    fn apply_loaded_state_merges_late_data_and_preserves_dirty() {
        let mut app_state = AppState {
            avg_download_history: vec![100],
            avg_upload_history: vec![10],
            disk_backoff_history_ms: VecDeque::from(vec![1]),
            network_history_dirty: true,
            ..Default::default()
        };
        app_state
            .network_history_state
            .tiers
            .second_1s
            .push(NetworkHistoryPoint {
                ts_unix: 2,
                download_bps: 100,
                upload_bps: 10,
                backoff_ms_max: 1,
            });

        let mut loaded = NetworkHistoryPersistedState::default();
        loaded.tiers.second_1s.push(NetworkHistoryPoint {
            ts_unix: 1,
            download_bps: 200,
            upload_bps: 20,
            backoff_ms_max: 2,
        });
        loaded.tiers.second_1s.push(NetworkHistoryPoint {
            ts_unix: 2,
            download_bps: 150,
            upload_bps: 15,
            backoff_ms_max: 3,
        });

        NetworkHistoryTelemetry::apply_loaded_state_at(&mut app_state, loaded, 2);

        // ts=2 should come from live value (100), not loaded overlap (150).
        assert_eq!(app_state.avg_download_history, vec![200, 100]);
        assert_eq!(app_state.avg_upload_history, vec![20, 10]);
        assert_eq!(
            app_state.disk_backoff_history_ms,
            VecDeque::from(vec![2, 1])
        );
        assert!(app_state.network_history_dirty);
    }

    #[test]
    fn merge_state_for_late_restore_preserves_live_point_on_overlap() {
        let mut live = NetworkHistoryPersistedState::default();
        live.tiers.second_1s.push(NetworkHistoryPoint {
            ts_unix: 5,
            download_bps: 500,
            upload_bps: 50,
            backoff_ms_max: 5,
        });
        let mut loaded = NetworkHistoryPersistedState::default();
        loaded.tiers.second_1s.push(NetworkHistoryPoint {
            ts_unix: 5,
            download_bps: 300,
            upload_bps: 30,
            backoff_ms_max: 3,
        });

        let merged = merge_state_for_late_restore(&live, loaded);
        assert_eq!(merged.tiers.second_1s.len(), 1);
        assert_eq!(merged.tiers.second_1s[0].download_bps, 500);
    }

    #[test]
    fn densify_state_for_restore_fills_sparse_second_gaps_and_tail_with_zeros() {
        let mut sparse = NetworkHistoryPersistedState::default();
        sparse.tiers.second_1s.push(NetworkHistoryPoint {
            ts_unix: 1,
            download_bps: 200,
            upload_bps: 20,
            backoff_ms_max: 2,
        });
        sparse.tiers.second_1s.push(NetworkHistoryPoint {
            ts_unix: 3,
            download_bps: 100,
            upload_bps: 10,
            backoff_ms_max: 1,
        });

        let dense = densify_state_for_restore(sparse, 4);
        assert_eq!(
            dense
                .tiers
                .second_1s
                .iter()
                .map(|p| p.download_bps)
                .collect::<Vec<_>>(),
            vec![200, 0, 100, 0]
        );
    }

    #[test]
    fn densify_state_for_restore_fills_sparse_minute_gaps_and_tail_with_zeros() {
        let mut sparse = NetworkHistoryPersistedState::default();
        sparse.tiers.minute_1m.push(NetworkHistoryPoint {
            ts_unix: 60,
            download_bps: 600,
            upload_bps: 60,
            backoff_ms_max: 3,
        });
        sparse.tiers.minute_1m.push(NetworkHistoryPoint {
            ts_unix: 180,
            download_bps: 300,
            upload_bps: 30,
            backoff_ms_max: 1,
        });

        let dense = densify_state_for_restore(sparse, 240);
        assert_eq!(
            dense
                .tiers
                .minute_1m
                .iter()
                .map(|p| p.download_bps)
                .collect::<Vec<_>>(),
            vec![600, 0, 300, 0]
        );
    }

    #[test]
    fn densify_tier_points_limits_sparse_tail_fill_to_retention_window() {
        let dense = densify_tier_points(
            &[NetworkHistoryPoint {
                ts_unix: 1,
                download_bps: 200,
                upload_bps: 20,
                backoff_ms_max: 2,
            }],
            1,
            4,
            1_000_000,
        );

        assert_eq!(
            dense.iter().map(|point| point.ts_unix).collect::<Vec<_>>(),
            vec![999_997, 999_998, 999_999, 1_000_000]
        );
        assert!(dense.iter().all(|point| point.download_bps == 0));
        assert!(dense.iter().all(|point| point.upload_bps == 0));
        assert!(dense.iter().all(|point| point.backoff_ms_max == 0));
    }

    #[test]
    fn apply_loaded_state_restores_dense_histories_from_sparse_points() {
        let mut app_state = AppState::default();
        let mut loaded = NetworkHistoryPersistedState::default();
        loaded.tiers.second_1s.push(NetworkHistoryPoint {
            ts_unix: 10,
            download_bps: 500,
            upload_bps: 50,
            backoff_ms_max: 4,
        });
        loaded.tiers.second_1s.push(NetworkHistoryPoint {
            ts_unix: 12,
            download_bps: 250,
            upload_bps: 25,
            backoff_ms_max: 2,
        });

        NetworkHistoryTelemetry::apply_loaded_state_at(&mut app_state, loaded, 13);
        assert_eq!(app_state.avg_download_history, vec![500, 0, 250, 0]);
        assert_eq!(app_state.avg_upload_history, vec![50, 0, 25, 0]);
        assert_eq!(
            app_state.disk_backoff_history_ms,
            VecDeque::from(vec![4, 0, 2, 0])
        );
    }

    #[test]
    fn apply_loaded_state_rebuilds_second_to_minute_rollup() {
        let mut app_state = AppState::default();
        let mut loaded = NetworkHistoryPersistedState {
            updated_at_unix: 59,
            ..Default::default()
        };
        loaded.tiers.second_1s = (1_u64..=59)
            .map(|ts| NetworkHistoryPoint {
                ts_unix: ts,
                download_bps: 10,
                upload_bps: 1,
                backoff_ms_max: 1,
            })
            .collect();

        NetworkHistoryTelemetry::apply_loaded_state_at(&mut app_state, loaded, 59);

        assert!(app_state.network_history_rollups.ingest_second_sample(
            &mut app_state.network_history_state,
            60,
            70,
            7,
            9,
        ));
        assert_eq!(app_state.network_history_state.tiers.minute_1m.len(), 1);
        assert_eq!(
            app_state.network_history_state.tiers.minute_1m[0].download_bps,
            11
        );
        assert_eq!(
            app_state.network_history_state.tiers.minute_1m[0].upload_bps,
            1
        );
        assert_eq!(
            app_state.network_history_state.tiers.minute_1m[0].backoff_ms_max,
            9
        );
    }

    #[test]
    fn apply_loaded_state_rebuilds_minute_to_15m_rollup() {
        let mut app_state = AppState::default();
        let mut loaded = NetworkHistoryPersistedState {
            updated_at_unix: 14 * 60,
            ..Default::default()
        };
        loaded.tiers.minute_1m = (1_u64..=14)
            .map(|idx| NetworkHistoryPoint {
                ts_unix: idx * 60,
                download_bps: 10,
                upload_bps: 2,
                backoff_ms_max: 3,
            })
            .collect();

        NetworkHistoryTelemetry::apply_loaded_state_at(&mut app_state, loaded, 14 * 60);

        for ts in (14 * 60 + 1)..=(15 * 60) {
            assert!(app_state.network_history_rollups.ingest_second_sample(
                &mut app_state.network_history_state,
                ts,
                40,
                4,
                5,
            ));
        }

        assert_eq!(app_state.network_history_state.tiers.minute_15m.len(), 1);
        assert_eq!(
            app_state.network_history_state.tiers.minute_15m[0].download_bps,
            12
        );
        assert_eq!(
            app_state.network_history_state.tiers.minute_15m[0].upload_bps,
            2
        );
        assert_eq!(
            app_state.network_history_state.tiers.minute_15m[0].backoff_ms_max,
            5
        );
    }

    #[test]
    fn apply_loaded_state_rebuilds_15m_to_hour_rollup() {
        let mut app_state = AppState::default();
        let mut loaded = NetworkHistoryPersistedState {
            updated_at_unix: 3 * 15 * 60,
            ..Default::default()
        };
        loaded.tiers.minute_15m = (1_u64..=3)
            .map(|idx| NetworkHistoryPoint {
                ts_unix: idx * 15 * 60,
                download_bps: 20,
                upload_bps: 3,
                backoff_ms_max: 4,
            })
            .collect();

        NetworkHistoryTelemetry::apply_loaded_state_at(&mut app_state, loaded, 3 * 15 * 60);

        for ts in (3 * 15 * 60 + 1)..=(4 * 15 * 60) {
            assert!(app_state.network_history_rollups.ingest_second_sample(
                &mut app_state.network_history_state,
                ts,
                80,
                8,
                9,
            ));
        }

        assert_eq!(app_state.network_history_state.tiers.hour_1h.len(), 1);
        assert_eq!(
            app_state.network_history_state.tiers.hour_1h[0].download_bps,
            35
        );
        assert_eq!(
            app_state.network_history_state.tiers.hour_1h[0].upload_bps,
            4
        );
        assert_eq!(
            app_state.network_history_state.tiers.hour_1h[0].backoff_ms_max,
            9
        );
    }
}

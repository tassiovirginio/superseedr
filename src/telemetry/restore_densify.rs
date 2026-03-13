// SPDX-FileCopyrightText: 2026 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

pub(crate) fn densify_points_for_restore<T, FTs, FZero>(
    points: &[T],
    step_secs: u64,
    max_points: usize,
    now_unix: u64,
    point_ts: FTs,
    zero_point_at: FZero,
) -> Vec<T>
where
    T: Clone,
    FTs: Fn(&T) -> u64,
    FZero: Fn(u64) -> T,
{
    if points.is_empty() || step_secs == 0 || max_points == 0 {
        return Vec::new();
    }

    let dense_end_ts = densified_end_ts(point_ts(&points[points.len() - 1]), step_secs, now_unix);
    let max_window_span = step_secs.saturating_mul(max_points.saturating_sub(1) as u64);
    let dense_start_ts = point_ts(&points[0]).max(dense_end_ts.saturating_sub(max_window_span));

    let mut start_idx = 0;
    while start_idx < points.len() && point_ts(&points[start_idx]) < dense_start_ts {
        start_idx += 1;
    }

    let mut dense = Vec::with_capacity(max_points);
    let mut next_ts = dense_start_ts;

    for point in &points[start_idx..] {
        while next_ts < point_ts(point) && next_ts <= dense_end_ts {
            dense.push(zero_point_at(next_ts));
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
        if point_ts(point) >= dense_end_ts {
            return dense;
        }

        let advanced_ts = point_ts(point).saturating_add(step_secs);
        if advanced_ts == point_ts(point) {
            return dense;
        }
        next_ts = advanced_ts;
    }

    while next_ts <= dense_end_ts {
        dense.push(zero_point_at(next_ts));
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

#[cfg(test)]
mod tests {
    use super::densify_points_for_restore;

    #[derive(Clone, Debug, PartialEq, Eq)]
    struct TestPoint {
        ts_unix: u64,
        value: u64,
    }

    #[test]
    fn densify_points_for_restore_fills_sparse_gaps_and_tail() {
        let dense = densify_points_for_restore(
            &[
                TestPoint {
                    ts_unix: 1,
                    value: 10,
                },
                TestPoint {
                    ts_unix: 3,
                    value: 30,
                },
            ],
            1,
            8,
            4,
            |point| point.ts_unix,
            |ts_unix| TestPoint { ts_unix, value: 0 },
        );

        assert_eq!(
            dense.iter().map(|point| point.value).collect::<Vec<_>>(),
            vec![10, 0, 30, 0]
        );
    }

    #[test]
    fn densify_points_for_restore_limits_fill_to_retention_window() {
        let dense = densify_points_for_restore(
            &[TestPoint {
                ts_unix: 1,
                value: 10,
            }],
            1,
            4,
            1_000_000,
            |point| point.ts_unix,
            |ts_unix| TestPoint { ts_unix, value: 0 },
        );

        assert_eq!(
            dense.iter().map(|point| point.ts_unix).collect::<Vec<_>>(),
            vec![999_997, 999_998, 999_999, 1_000_000]
        );
        assert!(dense.iter().all(|point| point.value == 0));
    }
}

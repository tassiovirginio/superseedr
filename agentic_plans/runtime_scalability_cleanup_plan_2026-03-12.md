# Runtime Scalability Cleanup

## Summary
Track a set of incremental runtime and persistence optimizations that improve Superseedr's behavior as torrent counts and library size grow, without requiring the larger multi-instance architecture work.

This plan is meant to capture the "next layer down" from the integrity scheduler work:
- activity-history restore behavior
- whole-library metric passes
- repeated sort/filter rebuilds
- startup validation scans
- other O(torrents) and O(files) work that is acceptable at small scale but should be revisited before larger library ambitions

The intent is not to over-optimize prematurely. The intent is to document which parts of the current architecture are acceptable for hundreds of torrents, which parts likely need tightening for thousands, and which parts would become blockers for more serious scale targets.

## Goals
- Preserve correctness and maintainability while reducing avoidable whole-library work.
- Keep the current single-instance architecture healthy for typical users.
- Identify low-risk performance cleanups that can land before any major refactor.
- Create a clear boundary between "worth doing now" and "only matters at much larger scale".
- Avoid prematurely complicating the code with caches or indexes unless profiling justifies them.

## Non-Goals
- No attempt to solve multi-instance coordination in this plan.
- No redesign into a database-backed catalog here.
- No commitment to optimize for extreme torrent counts immediately.
- No change that makes the code materially harder to reason about without clear measurement.

## Current Operating Assumption
A typical serious Superseedr user today likely has something like:
- under 200 torrents
- perhaps up to 500 torrents

At that scale, several O(torrents) passes are probably acceptable. The question is not whether every such pass is a bug; the question is which of them are cheap, which of them are steadily compounding, and which are likely to become future migration pain.

## Why This Plan Exists
The integrity scheduler was intentionally designed to scale to very large file counts. That solved a real file-manifest hot path, but it did not solve every other scale-sensitive path in the app.

The rest of the system still contains whole-library work in areas like:
- metric draining
- list sorting and filtering
- per-second telemetry
- persistence payload construction
- startup validation and layout checks
- restore-time history shaping

Those should be reviewed deliberately, not reactively.

## Key Areas

### 1. Activity History Restore
Relevant code:
- `src/telemetry/activity_history_telemetry.rs`
- `src/tui/screens/normal.rs`

Current state:
- restore merges sparse saved history with live state
- restore currently densifies retained activity series into in-memory chart-friendly windows
- chart rendering already knows how to build visible aligned windows from sparse data

Current assessment:
- for typical users, this is probably not urgent
- for larger libraries, eager full retained-window densification per torrent is likely unnecessary work
- this area is a good candidate for a small, self-contained cleanup later

Preferred direction:
- keep persisted and restored activity history sparse
- let chart rendering materialize only the visible window
- add a tiny cache only if profiling later proves necessary

### 2. Whole-Library Metric Draining
Relevant code:
- `src/app.rs` (`drain_latest_torrent_metrics`)

Current state:
- every metrics receiver is scanned
- any meaningful change triggers a full sort/filter rebuild of the torrent list

Current assessment:
- simple and maintainable
- probably fine for hundreds of torrents
- likely one of the first hotspots once torrent counts become much larger

Preferred direction:
- keep the current design for now
- later consider coalescing or throttling full list recomputes
- avoid incremental complexity until there is real evidence the current behavior is hurting users

### 3. Torrent List Sorting And Filtering
Relevant code:
- `src/app.rs` (`sort_and_filter_torrent_list_state`)

Current state:
- rebuilds a fresh vector of hashes
- optionally runs fuzzy match across all torrents
- sorts the entire visible set

Current assessment:
- acceptable and easy to reason about at current target sizes
- unlikely to need immediate work
- becomes more important if the app starts aiming for very large torrent counts in one process

Preferred direction:
- no immediate redesign
- if needed later, consider separating:
  - visible ordering
  - search result caching
  - resort throttling

### 4. Per-Second Telemetry Passes
Relevant code:
- `src/telemetry/ui_telemetry.rs`
- `src/telemetry/activity_history_telemetry.rs`
- `src/telemetry/network_history_telemetry.rs`

Current state:
- per-second bookkeeping walks all torrents for multiple aggregate calculations
- activity history also records per-torrent samples every second

Current assessment:
- intentional, understandable, and likely acceptable for typical users
- becomes a significant scaling concern for very large torrent counts
- more important than restore-time densification if "every torrent is active every second"

Preferred direction:
- leave unchanged for normal-scale operation
- later consider reducing work for idle torrents or moving some metrics to a less frequent cadence

### 5. Startup Validation And Layout Checks
Relevant code:
- `src/torrent_manager/manager.rs`
- `src/storage.rs`

Current state:
- startup validation and "skip hashing" flows can still perform full layout scans
- `has_complete_storage_layout` walks all files for a torrent

Current assessment:
- acceptable correctness-first behavior
- still a scaling risk for restarts on large libraries
- separate from the new bounded integrity probe scheduler

Preferred direction:
- keep current behavior while correctness is more important than startup scale
- later revisit whether validation state can avoid immediate full layout scans for every resumed torrent

### 6. Persistence Payload Construction
Relevant code:
- `src/app.rs` (`build_persist_payload`)

Current state:
- persistence rebuilds torrent settings from in-memory torrent state
- history payloads are cloned as part of snapshot writes

Current assessment:
- simple and robust for a single-process app
- not urgent at normal scale
- eventually part of the broader story if catalog size or history volume becomes large

Preferred direction:
- no immediate architectural change
- avoid piecemeal complexity unless there is a clear measured bottleneck

### 7. Chart Overlay Work
Relevant code:
- `src/tui/screens/normal.rs`

Current state:
- overlay modes compute visible windows from activity history
- multi-torrent overlay ranks torrents by recent traffic and currently recomputes some totals multiple times per draw

Current assessment:
- bounded enough for now
- a reasonable future optimization target if charting becomes a frame-time hotspot

Preferred direction:
- if needed later, compute overlay totals once per draw or once per second
- avoid persistent caches until profiling says otherwise

## Prioritization Guidance

### Worth Doing Sooner
- self-contained cleanups that remove clearly unnecessary work without changing product behavior
- examples:
  - keep activity-history restore sparse
  - reduce repeated overlay ranking calculations

### Worth Watching But Not Forcing
- O(torrents) passes that are still easy to reason about and likely fine for under 500 torrents
- examples:
  - full metric drain
  - full list sort/filter rebuilds
  - per-second telemetry scans

### Likely Bigger Future Refactor Work
- areas tied to the current one-manager-per-torrent and whole-state snapshot model
- examples:
  - startup validation strategy at very large scale
  - hot/cold state separation
  - large-catalog persistence changes

## Proposed Phasing

### Phase 1: Low-Risk Cleanup Candidates
- Revisit activity-history restore and keep it sparse in memory.
- Remove obviously repeated chart overlay calculations if profiling justifies it.
- Add comments documenting intended scale assumptions around existing O(torrents) paths.

### Phase 2: Measured Runtime Tightening
- Add lightweight instrumentation around:
  - metric drain time
  - list sort/filter time
  - telemetry tick duration
  - startup validation duration
- Use real measurements to determine whether current behavior is still acceptable.

### Phase 3: Larger Single-Instance Scale Work
- If torrent counts grow well beyond the current norm, revisit:
  - throttled resorting
  - idle-torrent telemetry reduction
  - validation deferral strategies
  - hot/cold state separation

## Decision Notes

### Activity History Densification
Current stance:
- not a must-fix-now issue for the current user profile
- still a worthwhile cleanup because it appears self-contained and the TUI already supports sparse-visible-window construction

### Caching
Current stance:
- do not add caches by default
- add only after profiling reveals repeated recomputation is meaningfully expensive

### Maintainability Bias
This plan intentionally favors:
- clear data flow
- explicit full passes where acceptable
- local, well-bounded optimizations

It intentionally avoids:
- premature indexing
- stale-cache complexity
- partial rewrites without evidence

## Success Criteria
- Superseedr remains simple and responsive for typical users.
- Small targeted cleanups reduce clearly avoidable work.
- Larger scalability decisions are postponed until they are justified by real operating data.
- The codebase stays understandable while the path to future higher scale remains open.

## Open Questions
- At what torrent count does full metric draining become meaningfully visible?
- At what library size does startup validation become the more pressing concern than the integrity scheduler?
- Is per-torrent activity history retention worth reducing for long-idle torrents?
- Which measurements should be added before making any medium-sized runtime optimization changes?

## Current Position
Superseedr does not need to optimize every O(torrents) path immediately. The main goal here is to keep a running map of where future pressure is likely to appear and to capture the few cheap wins that improve behavior without complicating the architecture.

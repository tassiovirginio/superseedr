# Startup + Churn CPU Reimplementation Plan

## Status Snapshot (2026-03-01)
This plan captures the two exploratory optimizations that materially reduced CPU in the current branch and should be reimplemented cleanly after the branch is reverted.

### Optimizations validated in the exploratory branch
1. Move rarity recomputation off peer event hot paths and onto a dedicated 1 second manager timer.
2. Ignore duplicate `MetadataTorrent` commands in the manager before hashing or state hydration.

### Important cleanup rule
The temporary profiler buckets added during investigation are not part of the final implementation.
They were useful to prove the hot paths, but the clean reimplementation should not carry them forward by default.

## Objective
Reimplement the two proven CPU reductions cleanly, with minimal code churn, retained safety guards, and targeted tests.

## Scope
### In scope
1. Rarity recompute scheduling changes.
2. Early manager-side duplicate metadata guard in `src/torrent_manager/manager.rs`.
3. Targeted regression coverage for the behavior above.

### Out of scope
1. Broad peer/tracker startup throttling.
2. Metadata parser redesign.
3. Permanent profiler framework expansion.
4. Unrelated metadata correctness fixes discovered during review.

## Summary of What the Exploratory Runs Proved
1. Peer churn CPU spikes were dominated by repeated rarity rebuilds on `PeerHavePiece` and `PeerBitfieldReceived`, not by disconnect batching.
2. Metadata parser cost was small; the expensive metadata path was duplicate manager-side hashing before the duplicate was rejected.

## Implementation Plan

## Optimization 1: Dedicated 1s Rarity Recompute Timer
### Goal
Remove full rarity rebuilds from peer event hot paths and perform them once per second in the manager loop.

### Clean design
1. Add a dedicated rarity timer in `src/torrent_manager/manager.rs`.
2. On each rarity timer tick:
- if torrent status is not `Done`, call `piece_manager.update_rarity(...)`.
3. Remove direct `update_rarity()` calls from hot paths in `src/torrent_manager/state.rs`:
- `Action::PeerDisconnected`
- `Action::PeerBitfieldReceived`
- `Action::PeerHavePiece`
- `Action::ValidationComplete`
4. Do not add any new state-side staleness guard, timestamp, or dirty-flag mechanism.
5. Keep existing assignment/choke behavior unchanged.

### Why this is acceptable
1. Rarity is a scheduling heuristic, not download correctness authority.
2. Up to 1 second of stale rarity is acceptable.
3. This avoids scaling a full rarity rebuild with every `Have` and bitfield event.

### Risks
1. Piece ordering can be up to 1 second stale.
2. Rare-piece selection can be slightly less reactive in high churn.
3. If a future correctness path truly needs immediate rarity refresh, it should be added back explicitly and narrowly.

### Validation
1. Run targeted tests already used during exploration:
- `cargo test -q peer_disconnect -- --nocapture`
- `cargo test -q peer_admission_guard -- --nocapture`
- `cargo test -q requestable_block_addresses_for_piece -- --nocapture`
2. Rerun churn scenario and confirm:
- no event-driven rarity hot bucket
- rarity work appears once per second
- `piece_manager.update_rarity` no longer dominates the window

## Optimization 2: Manager-Side Early Duplicate Metadata Guard
### Goal
Stop duplicate `MetadataTorrent` messages before any expensive hash, clone, or state work.

### Clean design
1. In `TorrentCommand::MetadataTorrent` handling in `src/torrent_manager/manager.rs`, add:
- early `if self.state.torrent.is_some() { continue; }`
2. Do not modify `src/torrent_manager/state.rs` for this optimization.
3. Keep the existing state-side duplicate guard in `Action::MetadataReceived` as-is.
4. Leave first-load metadata validation behavior unchanged:
- hybrid normalization
- info-hash validation
- metadata install
- preview event emission

### Why both guards should exist
1. Manager guard prevents duplicate CPU cost.
2. State guard remains the final safety barrier.
3. This preserves defense-in-depth without paying duplicate hash cost repeatedly.

### Risks
1. Peers may still finish sending metadata bytes they already started; that is acceptable.
2. If later logic needs duplicate metadata for diagnostics, that should be handled separately and intentionally.

### Validation
1. Keep the existing metadata initialization test:
- `cargo test -q test_metadata_received_triggers_initialization_flow -- --nocapture`
2. Add a dedicated duplicate metadata manager test if convenient during clean reimplementation:
- duplicate `MetadataTorrent` after first install does not call install path again
3. Rerun startup and confirm:
- `core.metadata.path.duplicate` is effectively zero
- duplicate metadata no longer spends time in `hash_info_dict`

## Recommended Reimplementation Order
1. Reimplement optimization 1 first.
- It addresses the original churn hot path.
2. Reimplement optimization 2 second.
- It finishes metadata duplicate suppression and is easy to validate in isolation.

## Temporary Instrumentation Guidance
Do not keep the exploratory per-section profiler scopes in the clean implementation.

If a short validation pass is needed during reimplementation, add only the smallest temporary scopes required for confirmation:
1. rarity timer branch
2. duplicate metadata path

Remove those scopes once the reruns confirm behavior.

## Acceptance Criteria
1. Churn rerun shows rarity work decoupled from `Have` and bitfield event volume.
2. Startup rerun shows duplicate metadata path near zero cost after the first metadata install.
3. Existing targeted tests remain green.
4. Final clean branch does not retain the exploratory profiler bucket sprawl.

## Follow-Up Work Not Included In This Plan
1. Tracker/peer startup fanout throttling.
2. Separate cleanup of the private metadata branch behavior in the manager.
3. Any broader metadata preview correctness fixes.

## Suggested Final Review Checklist
1. Confirm direct `update_rarity()` calls are gone from steady-state peer event handlers.
2. Confirm the duplicate metadata change is confined to `src/torrent_manager/manager.rs` and the existing state duplicate guard still exists unchanged.
3. Confirm no exploratory profiler-only code remains unless intentionally re-added for a short validation pass.

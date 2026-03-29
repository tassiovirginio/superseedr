# Per-Torrent Restart/Revalidate Refactor

## Summary
Refactor torrent lifecycle so app-level restart can stop a single manager, keep the torrent entry intact, and relaunch the same torrent with forced validation. The core cleanup is to separate "manager stopped" from "torrent deleted", because today normal shutdown and delete both flow through `DeletionComplete`, which makes single-torrent reload awkward and unsafe.

## Implementation Changes
- Add a new non-delete manager event, `ManagerEvent::Stopped { info_hash }` or equivalent, emitted when `ManagerCommand::Shutdown` finishes normal tracker-stop and runtime teardown.
- Reserve `ManagerEvent::DeletionComplete` for delete-with-files only. Do not emit it from normal shutdown anymore.
- Keep `ManagerCommand::Shutdown` as the non-destructive stop primitive. Keep `DeleteFile` unchanged for payload deletion to limit churn in this refactor.
- Update app shutdown flow to wait for the new stopped event instead of `DeletionComplete`, preserving the existing whole-app graceful shutdown behavior.
- Split app cleanup helpers into:
  - runtime-handle cleanup only: remove manager tx/rx, incoming-peer routing, metric watchers, integrity scheduler runtime tracking
  - full torrent removal: remove config entry, UI row, and runtime handles
- Add an app-owned restart orchestration path keyed by info hash. Recommended shape: a small pending-operation map that records post-stop intent such as `Restart(relaunch_spec)` and `RemoveWithoutFiles`.
- Implement restart as an app-level operation:
  - resolve the torrent from `client_configs` by info hash
  - build a relaunch spec from the current persisted source, download path, container, and file priorities
  - send `ManagerCommand::Shutdown` if a live manager exists, otherwise relaunch immediately
  - on manager stopped, clean runtime handles only, then recreate the manager with `validation_status = false`
  - relaunch into `TorrentControlState::Running` so `[R]` always means restart-and-run after validation
- Reuse the existing add/load manager creation helpers for relaunch instead of adding in-manager reset logic. The fresh manager should follow the normal startup path and validation path unchanged.
- Persist a canonical restartable `.torrent` copy when metadata arrives for magnet-backed torrents, reusing the existing torrent-copy persistence logic used for file-backed ingestion. Restart source resolution should prefer this persisted `.torrent` copy when present so forced validation can start immediately.
- Add a small UI action surface for restart:
  - `[R]` in the normal TUI screen
  - footer/help text update
  - when restart is requested, keep the torrent row visible and mark it as restarting/validating rather than removing it from the list
- Prevent duplicate restarts while one is already pending for the same info hash.

## Test Plan
- Manager lifecycle test: normal shutdown emits `Stopped` and does not emit `DeletionComplete`.
- Delete-path test: delete-with-files still emits `DeletionComplete` and still removes the torrent.
- App shutdown test: whole-app exit waits for stopped events and does not prune persisted torrent config.
- Restart flow test for file-backed torrents: selected torrent stops, runtime handles are replaced, new manager is created with forced validation, and the torrent remains in the UI/config.
- Restart flow test for magnet-backed torrents with loaded metadata: restart uses the persisted canonical `.torrent` copy and enters validating immediately.
- Remove-without-files test: config/UI removal after stop still works and no longer depends on `DeletionComplete`.
- UI reducer/keymap test: uppercase `R` dispatches restart and lowercase `r` remains RSS.
- Duplicate-request test: a second restart request for the same torrent while one is already pending is ignored or rejected cleanly.

## Assumptions
- This change adds only the TUI `[R]` trigger. CLI/watch-folder restart controls are out of scope.
- Restart always ends in `Running` after validation, even if the torrent was paused before the restart request.
- App-level relaunch is the chosen architecture; the manager will not gain an in-place `restart()` or `init_state()` reset path in this change.
- Existing delete semantics remain user-visible unchanged; this refactor only separates the lifecycle plumbing underneath them.

# Layered Shared Config Mode

## Summary
Create an opt-in shared-config mode behind `SUPERSEEDR_SHARED_CONFIG_DIR` while preserving the current single-file OS-config flow for default users. In shared mode, Superseedr loads shared `settings.toml` and `catalog.toml` plus a host-specific `hosts/<host-id>.toml`, merges them into the existing runtime `Settings`, and keeps runtime persistence local under the normal OS data dir. No data migration is required.

This plan also assumes shared mode must support live catalog sync across hosts so add, remove, and shared-setting changes converge without requiring a restart. Shared catalog removal is the authoritative signal for cross-host torrent teardown.

## Implementation Changes
- Add config mode detection in `src/config.rs`:
  - Normal mode: unchanged `ProjectDirs`-based `settings.toml`.
  - Shared mode: enabled only when `SUPERSEEDR_SHARED_CONFIG_DIR` is set.
- Add host identity resolution:
  - Use `SUPERSEEDR_HOST_ID` when present.
  - Otherwise derive a sanitized host id from hostname.
  - Host config path is `hosts/<host-id>.toml`.
- Add layered config file layout for shared mode:
  - `settings.toml` for shared non-torrent settings.
  - `catalog.toml` for the shared torrent catalog.
  - `hosts/<host-id>.toml` for machine-specific overrides.
- Keep `Settings` as the resolved runtime shape to minimize churn in `src/app.rs`, `src/main.rs`, and TUI code.
- Introduce config-layer structs:
  - `SharedSettingsConfig` for shared non-torrent settings.
  - `CatalogConfig` for torrent catalog entries only.
  - `HostConfig` for machine-specific fields.
- Route fields as follows:
  - Shared in `settings.toml`: shared `client_id` default, RSS config, shared UI/network/performance settings, shared default download location, and other non-torrent settings.
  - Shared in `catalog.toml`: torrent list and torrent-level shared state, including references to canonical shared `.torrent` artifacts for file-based torrents.
  - Host-local in `hosts/<host-id>.toml`: optional `client_id` override, `client_port`, `watch_folder`, `path_roots`.
- Add portable shared path support:
  - Allow either absolute string paths or portable refs like `{ root = "media", relative = "downloads/tv" }`.
  - Apply this to `default_download_folder` in `settings.toml` and per-torrent `download_path` in `catalog.toml`.
  - Resolve portable refs through host-local `[path_roots]`.
  - Fail clearly when a required root is missing.
- Keep environment precedence:
  - defaults
  - `settings.toml`
  - `catalog.toml`
  - `hosts/<host-id>.toml`
  - `SUPERSEEDR_*` overrides
- Replace load/save helpers with mode-aware versions:
  - Normal mode keeps current read/write behavior.
  - Shared mode reads all three files and writes only the layer that owns the edited fields.
- Shared-mode write policy:
  - TUI/app edits to shared non-torrent settings save only to `settings.toml`.
  - TUI/app edits to torrents save only to `catalog.toml`.
  - TUI/app edits to host-local fields save only to `hosts/<host-id>.toml`.
  - Shared path fields are displayed but treated as manual-file-edit-only in shared mode for v1.
- Shared `client_id` behavior:
  - Default shared-mode identity comes from `settings.toml`.
  - A host may override `client_id` in `hosts/<host-id>.toml` if explicitly desired.
  - Saves must preserve the shared `client_id` default when a host override exists.
- Add stale-write protection in shared mode:
  - Track last-loaded fingerprint per shared file.
  - Reject saves when any on-disk shared file changed since load.
  - Surface a clear reload-required message instead of silently overwriting another machine’s edits.

## Live Sync And Reconcile
- Add shared-config file watching or equivalent reload loop in shared mode:
  - Watch `settings.toml`.
  - Watch `catalog.toml`.
  - Watch `hosts/<host-id>.toml`.
  - Reload and reconcile whenever any of them changes.
- Add a shared-mode reconcile pass after reload:
  - Compute diff between old resolved `Settings` and new resolved `Settings`.
  - Bring up newly added torrents.
  - Tear down torrents removed from the shared catalog.
  - Apply shared setting changes that are safe to update live.
  - Apply host-local setting changes from the host file to the current process.
- Shared catalog removal is the authoritative cross-host delete signal:
  - If a torrent disappears from `catalog.toml`, every host must stop and remove that torrent locally.
  - Hosts must not infer logical torrent deletion purely from missing payload files.
  - File disappearance remains a data-availability signal, not a catalog-removal signal.
- Shared delete semantics:
  - In shared mode, deleting a torrent removes it from `catalog.toml` and triggers local teardown on every host via reconcile.
  - If the initiating host also deletes payload files, other hosts should converge by removing the torrent after seeing the catalog diff.
  - Other hosts should not be allowed to reintroduce the torrent through stale saves.
- Reconcile behavior for removals:
  - Removal from catalog should map to local shutdown and removal of the torrent from in-memory/runtime state.
  - Local removal due to shared catalog diff should not be treated as a host-only ad hoc action.
  - If files are already gone before a host reconciles, the host should still converge by removing the torrent once it observes the catalog change.

## Runtime Persistence And Other Files
- Keep all runtime persistence local in the normal OS data dir even in shared mode.
- Do not place these under `SUPERSEEDR_SHARED_CONFIG_DIR`:
  - `persistence/rss.toml`
  - `persistence/network_history.bin`
  - activity history persistence
  - logs
  - lock file
  - processed/watch command artifacts
- Split RSS behavior explicitly:
  - `Settings.rss` stays shared in `settings.toml`.
  - `RssPersistedState` stays local because history, feed errors, and last-sync metadata are per-instance runtime state.
- Keep existing persistence modules using local app data resolution; shared mode should not redirect them into the mounted config directory.

## CLI And UX
- Keep current Clap surface unchanged for this feature.
- Ensure CLI and app share the same config discovery rules in both modes.
- In shared mode, CLI commands continue to work using the resolved host-local watch path.
- Update config-screen UX:
  - Host-local fields remain editable.
  - Shared non-torrent fields remain editable.
  - Shared path fields show an explicit manual-edit notice pointing to `settings.toml`.
- Shared delete UX should clearly communicate that removing a torrent from the shared catalog will propagate to all hosts using that shared config root.

## Tracker Considerations
- Shared config does not make multi-host simultaneous torrent execution safe by itself.
- Shared mode should default to one logical shared `client_id` in `settings.toml`, with host override only when explicitly configured.
- Even with a shared `client_id`, multiple hosts actively announcing the same torrent can still be problematic, especially on private trackers.
- Catalog removal must propagate quickly so hosts stop tracker-facing activity promptly when a torrent is removed from the shared catalog.
- Longer term, safe multi-host execution likely needs ownership or lease semantics rather than config sync alone.

## Test Plan
- Normal-mode regression tests:
  - existing `settings.toml` loading and saving remain unchanged.
- Shared-mode loading tests:
  - merge `settings.toml`, `catalog.toml`, and `hosts/<host-id>.toml` correctly.
  - env overrides still win.
  - host id selection uses env first, then hostname.
- Path tests:
  - absolute shared paths resolve unchanged.
  - portable refs resolve through `path_roots`.
  - missing roots fail with specific errors.
  - portable refs round-trip without being rewritten to absolute paths.
- Save routing tests:
  - host-local edits touch only host files.
  - shared non-torrent edits touch only `settings.toml`.
  - torrent edits touch only `catalog.toml`.
  - shared path fields are not rewritten from the app in shared mode.
  - stale-write detection rejects conflicting saves.
  - shared `client_id` default is preserved when a host override exists.
- Shared sync tests:
  - settings watcher or reload loop detects shared settings changes.
  - catalog watcher or reload loop detects shared catalog changes.
  - host-file watcher or reload loop detects host override changes.
  - reconcile adds newly introduced torrents.
  - reconcile removes torrents missing from the shared catalog.
  - reconcile updates live-applicable shared settings without restart.
- Shared delete tests:
  - removing a torrent from `catalog.toml` causes local teardown on every host instance under test.
  - delete-with-files initiated on one host still converges other hosts once they observe the catalog removal.
  - missing payload files alone do not trigger automatic torrent removal from the catalog.
- Persistence behavior tests:
  - RSS/network/activity persistence paths remain local in shared mode.
  - `Settings.rss` is shared while `RssPersistedState` remains local.
- Acceptance scenarios:
  - no env var means no behavior change for existing users.
  - two hosts with different OS path roots can share one `settings.toml` plus one `catalog.toml`.
  - one mounted shared config root works without sharing runtime persistence.
  - deleting a torrent on one host removes it from all hosts after shared sync converges.

## Assumptions
- No migration from existing `settings.toml` into shared mode.
- Shared mode is strictly opt-in.
- Shared mode covers shared config plus live config-driven reconcile, but not broader multi-instance execution ownership or scheduling.
- `settings.toml` is the main power-user editable file for shared non-torrent settings.
- `catalog.toml` is the shared torrent catalog.
- `hosts/<host-id>.toml` is intentionally small and machine-specific.
- Shared catalog removal, not file disappearance, is the authoritative cross-host deletion signal.


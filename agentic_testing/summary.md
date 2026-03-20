# CLI And Shared Config Validation Summary

## Overall Verdict

Completed all planned phases with evidence capture.

- Passed: 5 phases (0, 1, 2, 5, 8)
- Failed: 4 phases (3, 4, 6, 7)
- Net result: validation run completed, with multiple high-confidence product defects.

## Environment Summary

- Workspace root: `<WORKSPACE_ROOT>`
- Scratch root: `<WORKSPACE_ROOT>/tmp/cli_shared_config_validation_<timestamp>`
- Binary used: `target/debug/superseedr`
- Shared config env:
  - `SUPERSEEDR_SHARED_CONFIG_DIR=<WORKSPACE_ROOT>/tmp/cli_shared_config_validation_<timestamp>/run/shared-root`
  - `SUPERSEEDR_HOST_ID=<HOST_ID>`
- Host client port: `17301`
- Local runtime config/data path resolved: `<LOCAL_APP_DATA_DIR>/com.github.jagalite.superseedr`
- Local runtime artifacts copied to scratch evidence:
  - `status_files/app_state.json`
  - `persistence/event_journal.toml`
  - local logs directory

## Phase Results

- Phase 0 Environment Preparation: **PASS**
- Phase 1 Shared Bootstrap/Sanity: **PASS**
- Phase 2 Online Status Controls: **PASS**
- Phase 3 Online Pause/Resume/Priority/Delete: **FAIL**
- Phase 4 Offline CLI Behavior: **FAIL**
- Phase 5 Live Remove Without Resurrection: **PASS**
- Phase 6 Updated-But-Missing Runtime Case: **FAIL**
- Phase 7 Stale-Write Protection: **FAIL**
- Phase 8 Watch-Folder Online Delivery: **PASS**

## Failure Notes

### Phase 3 (PRODUCT)

- **Observed:** `priority` command crashed immediately with clap debug-assert panic:
  - `Found non-required positional argument with a lower index than a required positional argument: "info_hash"`
- **Expected:** online priority operations should queue and apply like pause/resume/delete.
- **Impact:** priority control surface is unusable online.

### Phase 4 (PRODUCT)

- **Observed:** offline `priority` crashed with the same panic as Phase 3.
- **Expected:** offline priority should edit shared config directly.
- **Impact:** priority control surface is unusable offline as well.

### Phase 6 (PRODUCT)

- **Observed:** when alpha was seeded with a missing torrent path before launch, runtime omitted alpha (expected), but catalog entry did not remain present in shared config (it was pruned). After repairing the catalog entry externally, alpha loaded live without restart.
- **Expected:** target scenario expects entry to stay in shared config while absent from runtime prior to repair.
- **Impact:** configured behavior diverges from intended updated-but-missing-runtime test semantics.

### Phase 7 (PRODUCT)

- **Observed:** after an external catalog edit, triggering a persisted change from host A rewrote catalog and overwrote the external edit.
- **Expected:** stale-write protection should reject conflicting save and require reload, preserving external edits.
- **Impact:** stale-write protection appears ineffective for this path.

## High-Confidence Suspected Regressions

1. CLI `priority` subcommand schema bug causes panic in both online and offline paths.
2. Stale-write protection does not prevent overwriting externally edited shared catalog.
3. Missing-runtime precondition behavior differs from expected semantics because missing entry is pruned from shared config during runtime reconciliation.

## Notes

- A pseudo-terminal launch (`script -q ...`) was required for host runtime in this environment; direct detached launch without TTY returned `Device not configured`.
- All generated evidence and reports were left intact under the scratch root for inspection.

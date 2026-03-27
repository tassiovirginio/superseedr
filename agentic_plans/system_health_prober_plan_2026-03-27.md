# System Health Prober Plan

## Summary
Add a runtime system health prober alongside the existing torrent integrity prober.

This new prober should detect storage-environment failures that make the client unsafe or unusable even when no specific torrent has hit a read fault yet, especially in shared-config mode.

Primary example:
- the shared NAS/mount goes offline after the client has already started

Secondary examples:
- configured watch folders disappear
- the default download folder becomes unavailable
- an explicitly configured torrent download root becomes unavailable

The intended behavior in shared mode is stronger than a warning:
- if the shared root becomes unavailable, the client should enter a blocking outage state
- the outage modal stays up until the shared root becomes accessible again
- `Q` exits the client

## Goals
- Detect shared-root outages proactively instead of waiting for a torrent read fault.
- Reuse the existing healthy-vs-recovery cadence model.
- Avoid duplicating probe scheduling patterns already established by the integrity scheduler.
- Surface shared-root outages as a blocking runtime failure, not a dismissible warning.
- Allow non-root path checks to share the same probe framework while using lower severity.
- Record outage and recovery transitions in the journal.

## Non-Goals
- Do not merge torrent integrity probing and system path probing into one domain model.
- Do not add user-facing configuration for probe cadence in this phase.
- Do not attempt automatic remount or path repair.
- Do not silently continue normal shared-mode operation after the shared root becomes unavailable.

## Recommended Direction

### 1. Keep separate domain probers
Maintain two sibling runtime health systems:
- torrent integrity prober
- system health prober

They solve different problems:
- torrent integrity prober answers whether torrent data is intact and available
- system health prober answers whether the runtime storage environment is usable

### 2. Share a small generic probe framework
Extract only the reusable scheduling/health-state mechanics:
- healthy cadence
- recovery cadence
- due time
- current health state
- transition detection

Do not force torrent manifests and system paths into the same concrete probe abstraction prematurely.

### 3. Use the existing cadence pattern
Adopt the same cadence shape already used by integrity recovery:
- healthy probe interval: `60s`
- recovery probe interval: `5s`

This should apply to system health checks too:
- while healthy, probe on the normal cadence
- after a failure, switch into fast recovery reprobes
- once recovered, return to normal cadence

## Proposed Scope

### Phase 1. Shared Root Probe
Add a shared-root health probe that runs only when shared-config mode is active.

Healthy checks:
- path exists
- path is a directory
- path is readable

Runtime checks:
- if the runtime expects to write host-local artifacts on the shared root, confirm the required host path is writable too

Failure behavior:
- enter blocking outage modal
- suspend normal interaction
- continue reprobe every `5s`
- auto-dismiss when recovered
- `Q` exits the client

Journal transitions:
- `SharedRootUnavailable`
- `SharedRootRecovered`

### Phase 2. Configured Path Probe
Once the shared root is healthy, probe configured critical paths:
- watch folders
- default download folder
- per-torrent download roots when explicitly distinct

These should likely be warnings first, not necessarily a full blocking outage unless the path is critical to current runtime operation.

Journal transitions:
- `ConfiguredPathUnavailable`
- `ConfiguredPathRecovered`

### Phase 3. Shared UI/Runtime Integration
Add a blocking modal/state in the TUI for shared-root outage:
- clear explanation that the shared storage is unavailable
- client cannot continue safely while it is down
- auto-recovers when the mount returns
- `Q` exits

The modal should not be dismissible while the outage is active.

## Architecture Sketch

### New module
Suggested module:
- `src/system_health_prober.rs`

Responsibilities:
- track probe state for runtime-critical paths
- schedule healthy and recovery probes
- emit transition events
- classify severity

### App integration
App should own:
- the current system health state
- whether a blocking outage modal is active
- journal recording for outages and recoveries
- TUI behavior while blocked

The app loop should:
1. ask the system health prober what is due
2. execute the minimal path checks
3. feed results back into the prober
4. update UI/journal state on transitions

### Reuse opportunities
Reuse ideas from the integrity scheduler:
- explicit healthy/recovery cadence
- transition-only logging
- due-time scheduling
- simple state machine

Do not directly reuse torrent-specific concepts like:
- info hash ownership
- probe batches
- metadata pending
- manifest cursors

## Failure Semantics

### Shared root unavailable
Severity:
- fatal-to-runtime in shared mode

Behavior:
- block the UI
- stop pretending the runtime is healthy
- reprobe every `5s`
- recover automatically when the root is back
- `Q` exits the client

### Watch/download path unavailable
Severity:
- degraded runtime

Behavior:
- visible warning
- journal transition
- reprobe and auto-clear on recovery

This can be escalated later if certain paths prove critical enough to justify blocking.

## Testing Plan

### Unit tests
- healthy probe remains on `60s`
- failed probe switches to `5s`
- recovery returns cadence to `60s`
- transition logging fires once per outage and once per recovery
- shared-root outage enters blocking state
- shared-root recovery clears blocking state

### Integration tests
- startup in shared mode with healthy mount does not trigger outage
- mount disappears after startup and runtime enters blocking outage mode
- mount returns and runtime recovers automatically
- missing watch folder raises non-blocking path warning
- repeated failed probes during one outage do not spam duplicate journal entries

### Manual validation
- run a shared cluster on a mounted share
- disconnect or unmount the share while runtime is active
- confirm blocking modal appears
- confirm normal UI interaction is blocked
- confirm `Q` exits
- remount share and confirm runtime auto-recovers when not exited

## Open Questions
- Should a follower react differently from a leader when the shared root disappears, or is the outage equally blocking for both
- Which configured-path failures should become blocking versus warning-only
- Should the blocking outage modal suppress all background activity or only user interaction
- Should local-only mode eventually probe configured watch/download folders too, or is this shared-mode-first

## Recommendation
Implement this as:
- a new `SystemHealthProber`
- a small shared probe-state helper extracted from the integrity scheduler pattern
- a blocking shared-root outage modal with `Q` to quit

Do not over-generalize into one universal prober yet.

# Cargo Dependency Assessment

## Summary
This note evaluates every direct dependency in `Cargo.toml` with three questions in mind:
- can we remove it outright
- can we rewrite the small bit of functionality locally
- if we remove it, how much of the current Cargo graph actually disappears

The highest-value realistic cleanup candidates are:
- `figment`: only used in [`src/config.rs`](../src/config.rs), but it pulls in an older `toml` stack and 11 likely-exclusive lockfile crates
- `clap`: the CLI surface is small in [`src/main.rs`](../src/main.rs) and [`src/integrations/cli.rs`](../src/integrations/cli.rs), and removal would likely drop 12 exclusive crates
- `tracing-appender`: only initialized in [`src/main.rs`](../src/main.rs); removing it would likely drop 8 exclusive crates if simpler logging is acceptable
- `tokio-stream`: only used for `StreamExt` in [`src/torrent_manager/manager.rs`](../src/torrent_manager/manager.rs); low code impact, but almost no graph win because its transitive crates are already shared
- `data-encoding`, `urlencoding`, `hex`, `magnet-url`: all are replaceable with local helpers, though only `magnet-url` changes meaningful parsing behavior

The highest-value optional feature cut is:
- `mainline`: currently enabled through the default `dht` feature; removing it would likely drop 21 exclusive crates, but it also removes DHT peer discovery

The biggest dependency by graph weight is:
- `reqwest`: 109 reachable transitive crates and 32 likely-exclusive ones, but it is used across tracker HTTP, RSS fetching, and web seeds, so this is a strategic rewrite rather than a practical quick win

## Method
Counts below were gathered from the local lockfile and `cargo tree --offline` on March 12, 2026.

Two graph numbers are listed:
- `Reachable`: unique transitive crates reachable from that direct dependency in the current resolved graph
- `Exclusive`: crates that appear to be reachable only from that direct dependency among the current direct dependencies, so they are the best estimate of what really disappears from `Cargo.lock` if the dependency goes away

These numbers are directional, not a perfect build-size model:
- feature changes can materially change compile cost without changing the crate count much
- some crates are shared through multiple direct dependencies, so removing a dependency may simplify the manifest without shrinking the lockfile much

## Best Next Steps

### Phase 1: Low-Risk Manifest Cleanup
- Remove or rewrite `tokio-stream` by replacing the single `StreamExt` use in [`src/torrent_manager/manager.rs`](../src/torrent_manager/manager.rs).
- Replace `data-encoding` with a tiny local base32 helper for magnet info-hash decoding in [`src/app.rs`](../src/app.rs).
- Replace `urlencoding` with a local percent-decoder helper around the single magnet/query decode path in [`src/app.rs`](../src/app.rs) and [`src/torrent_manager/manager.rs`](../src/torrent_manager/manager.rs).
- Decide whether `hex` is worth localizing. It has no transitive cost, but it is used often enough that a local helper could remove a direct dependency with predictable code churn.

### Phase 2: Medium-Value Simplification
- Replace `clap` with a hand-rolled parser if the CLI remains just:
  - optional positional input
  - `add`
  - `stop-client`
- Replace `figment` with explicit `toml` loading plus environment overlay in [`src/config.rs`](../src/config.rs). This is the cleanest way to remove the duplicate `toml 0.8` stack.
- Replace `tracing-appender` if daily file rotation and non-blocking logging are not important enough to justify their support crates.

### Phase 3: Product-Level Decisions
- Consider making `dht` opt-in instead of default if private-tracker or minimal builds matter more than automatic peer discovery.
- Only target `reqwest` or `feed-rs` if we are willing to narrow product scope or accept a fairly invasive rewrite.

## Version And Feature Notes
- `ratatui 0.29.0` currently pulls `crossterm 0.28.1`, while the app directly depends on `crossterm 0.29.0`. Even if we keep both crates conceptually, version alignment is worth checking because it may remove one duplicate branch of the graph.
- `ratatui` also pulls `strum 0.26.3` and `strum_macros 0.26.4`, while the app directly depends on `strum 0.27.2` and `strum_macros 0.27.2`. Removing the direct deps alone will not fully remove the strum family from the lockfile.
- `figment` pulls `toml 0.8.23`, while the app also directly depends on `toml 0.9.11`. Replacing `figment` is the clearest duplicate-stack win in the manifest.
- `tokio` is configured with `features = ["full", "test-util"]`. Even if we keep `tokio`, narrowing that feature list is likely worth a follow-up pass.
- `reqwest` is using default features plus `json`. If dependency weight becomes important, this crate is the best place to investigate `default-features = false` and a narrower transport or TLS choice.

## Full Assessment

| Dependency | Main usage in repo | Reachable | Exclusive | Recommendation | Impact if removed or rewritten |
| --- | --- | ---: | ---: | --- | --- |
| `reqwest` | Tracker HTTP, RSS fetch, web seeds in `src/app.rs`, `src/integrations/rss_service.rs`, `src/tracker/client.rs`, `src/networking/web_seed_worker.rs` | 109 | 32 | Keep for now. Biggest graph target, but not a near-term cleanup. | High. Touches multiple subsystems and networking behavior. |
| `sha1` | v1 piece hashing, magnet or file hashes in `src/app.rs`, `src/integrations/*`, `src/torrent_manager/*` | 7 | 0 | Keep. | High. Required for BitTorrent v1 behavior. |
| `sha2` | v2 piece hashing and Merkle logic in `src/app.rs`, `src/torrent_manager/*` | 7 | 0 | Keep. | High. Required for BitTorrent v2 behavior. |
| `tokio` | Runtime backbone across app, networking, storage, TUI, RSS | 23 | 0 | Keep, but trim features later. | Very high. Core async runtime. |
| `tokio-stream` | Single `StreamExt` usage in `src/torrent_manager/manager.rs` | 27 | 0 | Good low-risk removal candidate. | Low. Likely one small refactor. |
| `thiserror` | Error derives in `src/errors.rs` and `src/resource_manager.rs` | 5 | 0 | Keep unless we want manual error impls. | Low to medium code churn for little graph gain. |
| `tracing` | Logging and instrumentation across most runtime modules | 8 | 0 | Keep. | High. Cross-cutting diagnostics. |
| `tracing-subscriber` | Logger setup in `src/main.rs` | 13 | 0 | Keep unless logging setup is simplified at the same time as `tracing-appender`. | Medium. One file, but user-visible logging behavior changes. |
| `tracing-appender` | Rolling file logging in `src/main.rs` | 28 | 8 | Good medium-value rewrite candidate. | Medium. Replace with simpler file writer or stdout-only logging. |
| `serde` | Serialization for config, protocol, persistence, torrent metadata | 6 | 0 | Keep. | Very high. Serialization foundation. |
| `serde_bencode` | Torrent parsing and wire extensions in `src/networking/*`, `src/torrent_file/*`, `src/torrent_manager/*`, `src/tracker/*` | 8 | 0 | Keep. | High. Deep protocol coupling. |
| `serde_bytes` | Compact byte-field serde in protocol, torrent, and tracker structs | 1 | 0 | Keep. | Medium. Small crate, low savings. |
| `magnet-url` | Magnet parsing in `src/app.rs` and `src/torrent_manager/manager.rs` | 0 | 0 | Rewrite candidate if we only need a narrow subset of magnet semantics. | Medium. Feasible local parser, but correctness matters. |
| `mainline` | DHT peer discovery in `src/app.rs` and `src/torrent_manager/*` | 46 | 21 | Keep as long as `dht` stays a default feature. Biggest optional feature cut. | High product impact. Removes DHT behavior. |
| `data-encoding` | Single base32 decode path in `src/app.rs` | 0 | 0 | Excellent tiny rewrite candidate. | Low. A small helper can replace it. |
| `urlencoding` | Single percent-decode path in magnet handling | 0 | 0 | Excellent tiny rewrite candidate. | Low. A small helper can replace it. |
| `crossterm` | Terminal mode and event handling in `src/main.rs`, `src/app.rs`, TUI modules | 21 | 3 | Keep, but investigate version alignment with `ratatui`. | High. Core terminal integration. |
| `ratatui` | Entire TUI rendering, layout, and widget stack | 46 | 24 | Keep. | Very high. This is the TUI. |
| `rand` | Test helpers, IDs, and small runtime randomness across app and TUI | 6 | 4 | Keep unless we want deterministic local helpers. | Low to medium. Savings are modest. |
| `directories` | App, watch, and config directory resolution in `src/config.rs`, `src/app.rs`, `src/main.rs`, `src/tui/screens/config.rs` | 5 | 2 | Possible rewrite candidate, but not urgent. | Medium. Cross-platform path logic would move in-house. |
| `toml` | Persisted settings and state read or write in `src/config.rs`, `src/persistence/*` | 6 | 4 | Keep. | Medium to high. Straightforward, but used in multiple persistence paths. |
| `hex` | Info-hash and digest encode or decode across app, integrations, telemetry, torrent manager, and TUI | 0 | 0 | Easy to rewrite locally if we want one less direct dep. | Medium only because there are many call sites. |
| `sysinfo` | Process, CPU, and memory telemetry in `src/app.rs` and `src/telemetry/ui_telemetry.rs` | 19 | 9 | Optional rewrite candidate if runtime telemetry becomes less important. | Medium. Feature is isolated, but cross-platform telemetry is annoying to own. |
| `strum` | Enum iteration traits in `src/networking/protocol.rs`, `src/theme.rs`, `src/tui/screens/normal.rs` | 0 | 0 | Remove only together with `strum_macros` if we are willing to hand-write enum lists. | Low to medium. Little graph win. |
| `strum_macros` | Enum derives in `src/app.rs`, `src/config.rs`, `src/networking/protocol.rs`, `src/theme.rs` | 5 | 0 | Same as `strum`: only worth removing as a pair. | Low to medium. Manual enum maintenance cost goes up. |
| `figment` | Config loading and env overlay in `src/config.rs` | 22 | 11 | Best medium-value rewrite candidate. | Medium. Localized to config loading and removes duplicate TOML machinery. |
| `notify` | Watch-folder monitoring in `src/app.rs` and `src/integrations/watcher.rs` | 12 | 4 | Keep unless we want polling or OS-specific watcher code. | Medium to high. File watching is user-visible and cross-platform. |
| `clap` | CLI parsing in `src/main.rs` and `src/integrations/cli.rs` | 21 | 12 | Best medium-value rewrite candidate if CLI stays small. | Medium. Localized parser rewrite. |
| `rlimit` | FD and resource limit tuning in `src/app.rs` | 1 | 0 | Keep unless we are comfortable dropping this tuning on some platforms. | Low. Savings are tiny. |
| `fuzzy-matcher` | Search and filter ranking in `src/app.rs`, `src/integrations/rss_service.rs`, `src/tui/screens/rss.rs` | 2 | 0 | Possible rewrite candidate if substring match is acceptable. | Medium. Behavior quality may regress. |
| `chrono` | Timestamp formatting and RSS or UI date handling in `src/config.rs`, `src/integrations/rss_service.rs`, `src/tui/screens/*` | 9 | 0 | Keep. | Medium. Replaceable, but not a clean win. |
| `serde_json` | Status output and theme serialization tests | 4 | 0 | Keep. | Low. Tiny shared crate with clear purpose. |
| `feed-rs` | RSS parsing in `src/integrations/rss_service.rs` | 53 | 6 | Keep unless we intentionally narrow RSS support. | Medium to high. Only one call site, but the parser is doing real protocol work. |
| `regex` | RSS or config validation and filtering in `src/integrations/rss_service.rs`, `src/config.rs`, `src/tui/screens/rss.rs` | 4 | 0 | Keep. | Medium. Shared and low-cost. |

## Prioritized Recommendation
If the goal is to reduce dependency count without destabilizing the product, the best order is:
1. `tokio-stream`
2. `data-encoding`
3. `urlencoding`
4. `figment`
5. `clap`
6. `tracing-appender`

If the goal is to shrink the overall dependency graph the most, the biggest levers are:
1. `reqwest` by far, but only with a major networking rewrite
2. `mainline`, but only by changing the default DHT product behavior
3. `ratatui`, which is not a practical removal target unless the app stops being a TUI
4. `figment` and `clap`, which are the most realistic graph wins

## Current Position
The manifest does not look bloated in a random way. Most direct dependencies map to real product surface area. The strongest cleanup story is not "delete lots of crates"; it is:
- remove the tiny one-off helpers first
- rewrite `figment` and possibly `clap`
- decide deliberately whether DHT and rolling file logging are product priorities
- investigate version and feature alignment before attempting any large networking rewrite

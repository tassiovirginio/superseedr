# Shared Config Mode

## Overview
Shared config mode is a layered, single-leader mode.

- One client is the active leader.
- The leader is the only process allowed to consume the shared inbox and write shared state.
- Followers are read-only for shared state.
- Followers may still watch their own local ingress folder and relay files into the shared inbox.

Shared mode is enabled only when `SUPERSEEDR_SHARED_CONFIG_DIR` is set.

## Environment Variables

### `SUPERSEEDR_SHARED_CONFIG_DIR`
Absolute path to the shared root.

When set, Superseedr uses layered shared config from that root:

- `settings.toml`
- `catalog.toml`
- `torrent_metadata.toml`
- `hosts/<host-id>.toml`

Example:

```bash
SUPERSEEDR_SHARED_CONFIG_DIR=/mnt/superseedr-root
```

### `SUPERSEEDR_SHARED_HOST_ID`
Optional explicit host id for selecting `hosts/<host-id>.toml`.

If unset, Superseedr falls back to a sanitized hostname.

Example:

```bash
SUPERSEEDR_SHARED_HOST_ID=seedbox-a
```

## Shared Root Layout

```text
/mnt/superseedr-root/
  settings.toml
  catalog.toml
  torrent_metadata.toml
  hosts/
    seedbox-a.toml
    desktop-a.toml
  torrents/
  inbox/
  processed/
  status/
    app_state.json
  data/
  superseedr.lock
```

## File Ownership

### `settings.toml`
Authoritative shared global settings.

Leader-written:
- shared `client_id` default
- RSS config
- shared UI and performance settings
- shared default download folder

Follower-readable:
- yes

### `catalog.toml`
Authoritative shared torrent desired state.

Leader-written:
- torrent list
- per-torrent control state
- per-torrent download path
- per-torrent file priorities

Follower-readable:
- yes

### `torrent_metadata.toml`
Leader-written derived metadata snapshot.

Follower-readable:
- yes

### `hosts/<host-id>.toml`
Host-local runtime settings.

Kept fields:
- optional `client_id`
- `client_port`
- `watch_folder`

Removed from shared mode:
- `path_roots`

### `torrents/`
Canonical shared `.torrent` artifact store.

Leader-written:
- yes

Follower-readable:
- yes

### `inbox/`
Authoritative shared command/watch inbox.

Leader:
- watches it
- consumes it

Followers and CLI:
- may drop `.torrent`, `.magnet`, `.path`, and `.control` files into it

### `processed/`
Archive of shared inbox items after the leader handles them.

### `status/app_state.json`
Shared runtime status snapshot.

Leader-written:
- yes

Follower-readable:
- yes

### `superseedr.lock`
Shared-root lock file used to define leadership.

## Leadership

Shared mode uses the existing file-lock mechanism, but the lock file lives in the shared root.

- Leader: successfully acquires `/shared-root/superseedr.lock`
- Follower: fails to acquire that lock

Crash/takeover behavior:

- no lease protocol
- no reconciliation
- if the leader exits or crashes and the lock is released, another client may become leader

Shared mode is only supported on storage where cross-client file locking works correctly.

## Watch Folder Model

Each host may still define its own local ingress folder in `hosts/<host-id>.toml`:

```toml
client_port = 6681
watch_folder = "/srv/local-watch"
```

Behavior:

- Leader watches its own `watch_folder` directly.
- Leader also watches `/shared-root/inbox/`.
- Follower watches only its own `watch_folder`.
- When a follower sees a supported file in its local `watch_folder`, it relays that file into `/shared-root/inbox/`.
- Followers never ingest local watch-folder files into shared state directly.

Supported dropped file types:

- `.torrent`
- `.magnet`
- `.path`
- `.control`

Shared mode does not use `SUPERSEEDR_WATCH_PATH_1`, `SUPERSEEDR_WATCH_PATH_2`, and similar extra watch-path variables.

## Data Path Rules

Shared mode requires all torrent payload data to live under the shared root.

Rules:

- shared-mode download paths must resolve inside the shared root
- shared-mode download paths are stored root-relative in layered config
- host-specific path translation is not supported in shared mode

Example:

```toml
# settings.toml
default_download_folder = "data/downloads"
```

```toml
# catalog.toml
[[torrents]]
name = "Shared Collection"
download_path = "data/downloads/shared-collection"
```

At runtime, those resolve under `SUPERSEEDR_SHARED_CONFIG_DIR`.

If a shared-mode download path points outside the shared root, startup fails.

## CLI Behavior

In shared mode:

- `superseedr add ...` drops a file into `/shared-root/inbox/`
- `pause`, `resume`, `delete`, and `priority` drop `.control` files into `/shared-root/inbox/`
- followers never mutate shared config directly
- offline shared-mode CLI does not edit `settings.toml` or `catalog.toml`

If no leader is running:

- mutating requests are still queued in `/shared-root/inbox/`
- they are applied later when a leader starts

`delete` is a first-class dropped `.control` request, just like pause and resume.

## Status Behavior

Shared-mode status uses the shared status file:

- shared snapshot path: `/shared-root/status/app_state.json`
- `superseedr status` requests a fresh snapshot when a leader is running
- if no leader is running, it reads the latest shared snapshot and reports that it may be stale
- `superseedr status --follow` polls the shared status file; it does not toggle a shared follow-state flag

## Shared Mode vs Normal Mode

Shared mode changes only the shared control/config/data plane behavior.

Normal mode still keeps using:

- local app-data lock
- local config path
- local offline mutation behavior

## Example

Shared root:

```text
/srv/superseedr-root/
  settings.toml
  catalog.toml
  torrent_metadata.toml
  hosts/
    seedbox-a.toml
    desktop-a.toml
  torrents/
  inbox/
  processed/
  status/
  data/
  superseedr.lock
```

Shared settings:

```toml
# settings.toml
client_id = "shared-node"
default_download_folder = "data/downloads"
global_upload_limit_bps = 8000000
```

Shared catalog:

```toml
# catalog.toml
[[torrents]]
name = "Shared Collection"
torrent_or_magnet = "shared:torrents/0123456789abcdef0123456789abcdef01234567.torrent"
download_path = "data/downloads/shared-collection"
```

Leader-capable host:

```toml
# hosts/seedbox-a.toml
client_port = 6681
watch_folder = "/srv/local-watch"
```

Follower host:

```toml
# hosts/desktop-a.toml
client_port = 6681
watch_folder = "D:\\superseedr-watch"
```

Launch:

```bash
SUPERSEEDR_SHARED_CONFIG_DIR=/srv/superseedr-root \
SUPERSEEDR_SHARED_HOST_ID=seedbox-a \
superseedr
```

## Notes

- Shared mode is opt-in.
- Shared mode keeps the layered file layout.
- Shared mode does not use shared-config live reload.
- Shared mode does not use reconciliation or merge logic between clients.
- Followers are readers and relays, not shared-state writers.

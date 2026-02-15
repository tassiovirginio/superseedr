# TUI Architecture (Current)

## Module Layout
- `src/tui/view.rs`: top-level draw dispatcher and shared post-processing (theme effects).
- `src/tui/events.rs`: top-level input dispatcher and cross-cutting key handling.
- `src/tui/screen_context.rs`: read-only draw context (`ScreenContext`, `AppViewModel`).
- `src/tui/screens/*.rs`: per-screen draw + event handling.
- `src/tui/layout.rs`: layout planning helpers.
- `src/tui/tree.rs`: tree navigation/filtering helpers.
- `src/tui/formatters.rs`: rendering format helpers.

## Runtime Flow
1. `App::run` receives events and manager updates.
2. Input is routed through `tui::events::handle_event(event, &mut app)`.
3. Draw loop ticks UI effects clock in `App`, then calls `tui::view::draw(f, &app_state, &settings)`.
4. In power-saving mode, drawing is gated by `app_state.ui.needs_redraw`.

## State Ownership Matrix
- `AppState` (domain/application core):
  - torrent/session/runtime metrics and histories
  - manager-facing state and persisted values
  - sorting configuration (`torrent_sort`, `peer_sort`)
  - error/warning and lifecycle flags (`should_quit`, `shutdown_progress`, etc.)
- `AppState.ui` (UI-owned transient state):
  - redraw/effects timing: `needs_redraw`, effect clocks
  - shared UI interaction state: selection + search
  - per-screen substates:
    - `config`
    - `delete_confirm`
    - `file_browser`
- `AppMode`:
  - now acts as high-level route/screen id (`Normal`, `Config`, `FileBrowser`, etc.)
  - payload data has been migrated into `AppState.ui` substates.

## Current Transition Summary
- `Welcome`: `Esc` -> `Normal`.
- `Normal`:
  - `/` enters search.
  - `z` -> `PowerSaving`.
  - `c` -> `Config`.
  - `a` -> `FileBrowser` (add torrent flow).
  - `d`/`D` -> `DeleteConfirm`.
  - `Q` sets quit flag.
  - `Esc` clears `system_error` (stays in `Normal`).
- `PowerSaving`: `z` -> `Normal`.
- `Config`:
  - `Esc`/`Q` applies edited settings and returns to `Normal`.
  - `Enter` edits field or opens `FileBrowser` for path selection.
- `FileBrowser`:
  - `Y` confirms current action.
  - `Esc` returns to `Normal` or `Config` depending on browser mode.
  - `/` enters browser search.
- `DeleteConfirm`: `Enter` confirms and returns to `Normal`; `Esc` cancels.

## Help Overlay
- `show_help` remains a global overlay flag (not a dedicated `AppMode` yet).
- Windows: `m` press toggles help.
- Non-Windows: `m` press opens, `m` release closes, `Esc` closes.

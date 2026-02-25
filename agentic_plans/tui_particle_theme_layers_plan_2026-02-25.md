# TUI Particle Theme Layers Plan (`Flowers`)

## Summary
Add a new full theme with animated particle effects rendered as an explicit layer in the TUI.  
The implementation will be theme-driven, deterministic, and integrated into the shared draw pipeline.

Locked decisions:
- New particle theme variant: `Flowers`.
- Existing themes do not gain particle effects.
- `Welcome` screen remains untouched.
- Particle rendering applies only outside `AppMode::Welcome`.
- Foreground particles may overwrite UI glyphs.
- FPS behavior remains unchanged (respect current data rate).

## Scope
In scope:
- Theme model extensions to represent particle layer/effect configuration.
- New rendering module for particle passes.
- Draw pipeline integration for non-welcome screens.
- Theme serialization/parsing/display/test updates.

Out of scope:
- Any welcome screen code changes.
- FPS policy changes.
- Retrofitting particle effects to existing themes.

## Interface Changes
1. Extend `ThemeName` enum in `src/theme.rs`:
- `Flowers`

2. Extend name mappings in `src/theme.rs`:
- Serialize key: `flowers`
- Display label: `Flowers`
- Parse normalization and resolution entry for this name

3. Extend effect model in `src/theme.rs`:
- Add `ParticleLayerMode` enum:
  - `None`
  - `Background`
  - `Foreground`
  - `Both`
- Add `ThemeParticleEffect` struct (new), including:
  - `enabled: bool`
  - `layer_mode: ParticleLayerMode`
  - profile/discriminator for effect type (`flowers`)
  - density/speed/intensity knobs with safe defaults
- Add `particle: ThemeParticleEffect` to `ThemeEffects`.
- Update `ThemeEffects::enabled()` to include particle-enabled state.

## Rendering Architecture
1. Add `src/tui/particles.rs` with shared rendering helpers:
- `render_particle_background(f, ctx, spec)`
- `render_particle_foreground(f, ctx, spec)`
- Stateless procedural generation from:
  - `(x, y)`
  - `ctx.frame_time` (from `effects_phase_time`)
  - profile parameters

2. Integrate into `src/tui/view.rs` for all non-welcome modes:
- Before mode draw: render background particle layer if enabled.
- Render mode screen as currently implemented.
- Run `apply_theme_effects_to_frame` color pass as currently implemented.
- After color pass: render foreground particle layer if enabled.

3. Keep `AppMode::Welcome` path exactly as-is:
- No particle layer calls added there.
- Existing `welcome::draw` and existing global effects pass behavior unchanged.

## Theme Profiles
Implement profile defaults inside `Theme::builtin`:
1. `Flowers`
- Layer: `Background`
- Low drift speed, low-medium density
- Petal-like glyph subset (`.`, `*`, `o`, `+`) with warm/pastel accents

## Performance/Safety Constraints
- Complexity target remains O(width * height) per enabled layer.
- No per-particle persistent state in `AppState`.
- Clamp density by terminal area to avoid overload in large terminals.
- Clamp visual intensity and temporal frequencies to avoid aggressive strobe behavior.
- Preserve power-saving behavior (no redraw policy changes).

## Files Planned
- `src/theme.rs`
- `src/tui/view.rs`
- `src/tui/effects.rs` (only if needed for enabled-state plumbing)
- `src/tui/particles.rs` (new)
- `src/tui/README.md`

No planned changes:
- `src/tui/screens/welcome.rs`

## Test Plan
1. `src/theme.rs` tests
- Add new themes to `all_theme_names()`.
- Add snake_case parse tests.
- Add display-format tests.
- Verify serde roundtrip includes new themes.

2. Draw pipeline tests (`src/tui/view.rs` or nearby)
- Background particle pass is called before non-welcome mode draw.
- Foreground particle pass is called after theme effects pass.
- Non-particle themes preserve prior behavior.
- Welcome mode remains unchanged by new particle-layer integration.

3. Safety/regression tests
- `[FX]` footer indicator still reflects effects-enabled state for new particle themes.
- Tiny terminal sizes do not panic (`1x1`, narrow/short frames).
- Existing theme cycling and mode rendering remain stable.

## Acceptance Criteria
- Selecting `Flowers` shows particle animation in non-welcome screens.
- Existing themes look and behave exactly as before.
- Welcome screen behavior and visuals are unchanged.
- Build/test suite remains green for theme and TUI modules.

## Assumptions
- New theme names are acceptable in settings and UI labels.
- Foreground overwrite is intentional per product decision.
- Lower FPS settings may look less smooth and this is acceptable.

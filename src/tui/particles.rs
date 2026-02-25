// SPDX-FileCopyrightText: 2025 The superseedr Contributors
// SPDX-License-Identifier: GPL-3.0-or-later

use ratatui::prelude::{Color, Frame};

use crate::theme::{
    blend_colors, color_to_rgb, ParticleProfile, ThemeContext, ThemeParticleEffect,
};

pub(crate) fn apply_theme_particles_background_to_frame(f: &mut Frame, ctx: &ThemeContext) {
    let particle = ctx.theme.effects.particle;
    if !particle.enabled || !particle.layer_mode.has_background() {
        return;
    }
    render_particles(f, ctx, particle, false);
}

pub(crate) fn apply_theme_particles_foreground_to_frame(f: &mut Frame, ctx: &ThemeContext) {
    let particle = ctx.theme.effects.particle;
    if !particle.enabled || !particle.layer_mode.has_foreground() {
        return;
    }
    render_particles(f, ctx, particle, true);
}

fn render_particles(
    f: &mut Frame,
    ctx: &ThemeContext,
    particle: ThemeParticleEffect,
    is_foreground: bool,
) {
    if matches!(particle.profile, ParticleProfile::None) {
        return;
    }

    let area = f.area();
    let width = area.width as f64;
    let height = area.height as f64;
    if width <= 0.0 || height <= 0.0 {
        return;
    }

    let area_scale = ((width * height) / 12_000.0).sqrt().max(1.0);
    let base_density = particle.density.max(0.001) as f64;
    let density = (base_density / area_scale).clamp(0.001, 0.20);
    let phase = ctx.frame_time * particle.speed.max(0.1) as f64;
    let glow = (particle.intensity as f64).clamp(0.1, 1.0);

    let buf = f.buffer_mut();
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            let local_x = x - area.left();
            let local_y = y - area.top();
            if let Some(cell) = buf.cell_mut((x, y)) {
                let underlying_fg = cell.fg;
                if let Some((glyph, color)) = sample_particle(
                    ctx,
                    particle.profile,
                    local_x as f64,
                    local_y as f64,
                    width,
                    height,
                    phase,
                    density,
                    glow,
                    underlying_fg,
                    is_foreground,
                ) {
                    cell.set_symbol(glyph);
                    cell.fg = color;
                }
            }
        }
    }
}

fn sample_particle(
    ctx: &ThemeContext,
    profile: ParticleProfile,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    phase: f64,
    density: f64,
    glow: f64,
    underlying_fg: Color,
    reactive_tint: bool,
) -> Option<(&'static str, Color)> {
    match profile {
        ParticleProfile::Sakura => sample_sakura(
            ctx,
            x,
            y,
            width,
            height,
            phase,
            density,
            glow,
            underlying_fg,
            reactive_tint,
        ),
        ParticleProfile::Matrix => sample_matrix(
            ctx,
            x,
            y,
            width,
            height,
            phase,
            density,
            glow,
            underlying_fg,
        ),
        ParticleProfile::BioluminescentReef => sample_bioluminescent_reef(
            ctx,
            x,
            y,
            width,
            height,
            phase,
            density,
            glow,
            underlying_fg,
        ),
        ParticleProfile::None => None,
    }
}

fn sample_sakura(
    ctx: &ThemeContext,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    phase: f64,
    density: f64,
    glow: f64,
    underlying_fg: Color,
    reactive_tint: bool,
) -> Option<(&'static str, Color)> {
    let _ = (width, height);
    // Sakura intentionally reuses the original flowers-style motion profile.
    let drift = ((x * 0.12) - (phase * 1.9)).sin() + ((y * 0.07) - (phase * 1.2)).cos();
    let grain = ((x * 0.31) + (y * 0.15) - phase).sin();
    let score = (drift * 0.7) + (grain * 0.3);
    let threshold = 1.75 - density * 9.0;
    if score < threshold {
        return None;
    }

    let pick = hash01(x, y, phase, 24.0);
    let glyph = if pick > 0.80 {
        "o"
    } else if pick > 0.55 {
        "*"
    } else if pick > 0.30 {
        "+"
    } else {
        "."
    };
    let palette = [
        ctx.theme.scale.categorical.pink,
        ctx.theme.scale.categorical.pink,
        ctx.theme.scale.categorical.pink,
        ctx.theme.scale.categorical.flamingo,
        ctx.theme.scale.categorical.rosewater,
        ctx.theme.scale.categorical.flamingo,
    ];
    let mut base = palette[(pick * palette.len() as f64) as usize % palette.len()];
    if reactive_tint && !matches!(underlying_fg, Color::Reset) {
        base = glow_color(base, underlying_fg, 0.08);
    }
    Some((
        glyph,
        glow_color(base, ctx.theme.semantic.white, glow * 0.10),
    ))
}

fn sample_matrix(
    ctx: &ThemeContext,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    phase: f64,
    density: f64,
    glow: f64,
    underlying_fg: Color,
) -> Option<(&'static str, Color)> {
    let _ = (width, underlying_fg);
    let h = height.max(2.0);
    let col = x.floor();

    // Columns randomly phase in/out over time windows.
    let window_t = (phase * 1.35).floor();
    let col_seed = hash01(col, window_t, 0.0, 101.0);
    let active = col_seed > (0.32 + (1.0 - density).clamp(0.0, 1.0) * 0.28);
    if !active {
        return None;
    }

    // Per-column falling head and string length.
    let speed = 3.0 + hash01(col, 0.0, phase, 17.0) * 3.2;
    let head = (phase * speed + hash01(col, 0.0, 0.0, 23.0) * h).rem_euclid(h);
    let len = (4.0 + hash01(col, 0.0, phase, 29.0) * (h * 0.28)).clamp(4.0, h * 0.45);
    let dy = (head - y).rem_euclid(h);
    if dy > len {
        return None;
    }

    // Random dropout inside active strings creates hacking in/out behavior.
    let dropout = hash01(col, y.floor(), (phase * 8.0).floor(), 59.0);
    if dropout < 0.18 {
        return None;
    }

    let pick = hash01(col, (y * 0.61).floor(), (phase * 12.0).floor(), 53.0);
    let glyph = if pick > 0.88 {
        "1"
    } else if pick > 0.76 {
        "0"
    } else if pick > 0.64 {
        "7"
    } else if pick > 0.52 {
        "3"
    } else if pick > 0.40 {
        "9"
    } else if pick > 0.30 {
        "|"
    } else {
        ":"
    };

    let tail_ratio = if len <= 0.001 {
        0.0
    } else {
        (dy / len).clamp(0.0, 1.0)
    };
    let base = if tail_ratio < 0.08 {
        ctx.theme.semantic.white
    } else if tail_ratio < 0.24 {
        ctx.theme.scale.categorical.sky
    } else if tail_ratio < 0.55 {
        ctx.theme.scale.categorical.teal
    } else {
        ctx.theme.scale.categorical.green
    };

    Some((
        glyph,
        glow_color(
            base,
            ctx.theme.semantic.white,
            (glow * 0.18).clamp(0.0, 0.24),
        ),
    ))
}

fn sample_bioluminescent_reef(
    ctx: &ThemeContext,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    phase: f64,
    density: f64,
    glow: f64,
    underlying_fg: Color,
) -> Option<(&'static str, Color)> {
    let _ = underlying_fg;
    let w = width.max(2.0);
    let h = height.max(2.0);
    let nx = x / (w - 1.0);
    let ny = y / (h - 1.0);

    // Slow multi-layer drift suggests plankton depth and current.
    let drift_x = x - (phase * 0.9) + ((ny * 9.0) + phase * 0.25).sin() * 1.4;
    let drift_y = y + (phase * 0.45) + ((nx * 7.0) - phase * 0.22).cos() * 0.9;
    let field_a = ((drift_x * 0.16) + (drift_y * 0.11)).sin().abs();
    let field_b = ((drift_x * 0.07) - (drift_y * 0.19) + phase * 0.31)
        .cos()
        .abs();
    let field = (field_a * 0.62) + (field_b * 0.38);

    let sparse = hash01(drift_x, drift_y, phase, 149.0);
    let threshold = 0.92 + ((1.0 - density).clamp(0.0, 1.0) * 0.06);
    if field < threshold || sparse < 0.58 {
        return None;
    }

    let pick = hash01(x, y, phase, 163.0);
    let glyph = if pick > 0.88 {
        "·"
    } else if pick > 0.64 {
        "."
    } else if pick > 0.40 {
        "•"
    } else {
        "·"
    };

    let depth = hash01(x * 0.41, y * 0.73, phase, 173.0);
    let base = if depth > 0.88 {
        ctx.theme.scale.categorical.sky
    } else if depth > 0.62 {
        ctx.theme.scale.categorical.teal
    } else {
        ctx.theme.scale.categorical.green
    };

    Some((
        glyph,
        glow_color(
            base,
            ctx.theme.semantic.white,
            (glow * (0.10 + depth * 0.18)).clamp(0.0, 0.30),
        ),
    ))
}

fn hash01(x: f64, y: f64, phase: f64, salt: f64) -> f64 {
    let n = ((x * 12.9898) + (y * 78.233) + (phase * 37.719) + salt).sin() * 43758.5453;
    n.fract().abs()
}

fn glow_color(base: Color, highlight: Color, amount: f64) -> Color {
    blend_colors(
        color_to_rgb(base),
        color_to_rgb(highlight),
        amount.clamp(0.0, 0.65),
    )
}

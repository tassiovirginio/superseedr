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
        ctx.theme.scale.categorical.flamingo,
        ctx.theme.scale.categorical.rosewater,
        ctx.theme.semantic.white,
    ];
    let mut base = palette[(pick * palette.len() as f64) as usize % palette.len()];
    if reactive_tint && !matches!(underlying_fg, Color::Reset) {
        base = glow_color(base, underlying_fg, 0.28);
    }
    Some((
        glyph,
        glow_color(base, ctx.theme.semantic.white, glow * 0.35),
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
